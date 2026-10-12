#!/usr/bin/env python3
"""Source-aware editor, scoped/ordinary inputs, native ping and shape-preserving graphs."""
import json,os,pathlib,threading,shutil,subprocess,tempfile,time,urllib.request,urllib.error,sqlite3
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from smoke import BINARY,free_port,wait_for
from playwright.sync_api import sync_playwright,expect

def main():
 with tempfile.TemporaryDirectory() as tmp:
  root=pathlib.Path(tmp);binary=root/'sacho';shutil.copy2(BINARY,binary);vault=root/'vault';key=root/'key';subprocess.run([str(binary),'secrets','init','--vault',str(vault),'--key-file',str(key)],check=True,capture_output=True)
  port=free_port();base=f'http://127.0.0.1:{port}';cfg=root/'cfg.toml';cfg.write_text(f'bind="127.0.0.1:{port}"\ndatabase="{root}/db"\npublic_url="{base}"\n[secrets]\nvault="{vault}"\nkey_file="{key}"\n')
  log=open(root/'log','w');server=None
  def start():return subprocess.Popen([str(binary),'serve','--config',str(cfg)],stdout=log,stderr=log)
  def request(path,body=None,method=None):
   req=urllib.request.Request(base+path,data=json.dumps(body).encode() if body is not None else None,method=method,headers={'Content-Type':'application/json','X-Sacho-Request':'1'})
   with urllib.request.urlopen(req,timeout=20) as r:return json.load(r)
  try:
   server=start();wait_for(lambda:request('/healthz'))
   templates={e['id']:e for e in request('/api/admin/check-templates')};assert request('/api/admin/examples')==list(templates.values())
   for address in ['127.0.0.1','::1']:
    result=request('/api/admin/test',{**templates['ping']['spec'],'target':address,'ping':{'count':2,'min_replies':2,'wait_ms':1000,'interval_ms':100}});assert result['status']=='operational',result;output=json.loads(result['output']);assert output['received']==2 and output['loss_percent']==0 and output['rtt_ms']>=0,output
   # Ordinary globals have explicit plaintext readback; encrypted globals do not.
   request('/api/admin/inputs/PLAIN_FIXTURE',{'value':'ordinary-config-value','enabled':True,'encrypted':False},'PUT')
   listing=request('/api/admin/inputs')['inputs'];ordinary=next(i for i in listing if i['name']=='PLAIN_FIXTURE');assert ordinary['value']=='ordinary-config-value' and not ordinary['encrypted']
   request('/api/admin/inputs/PLAIN_FIXTURE',{'value':'encrypted-global-fixture','enabled':True,'encrypted':True},'PUT');assert 'encrypted-global-fixture' not in json.dumps(request('/api/admin/inputs'))
   connection=sqlite3.connect(root/'db');assert 'encrypted-global-fixture' not in connection.execute("SELECT COALESCE(value,'') FROM settings WHERE key='probe.literal_values'").fetchone()[0]
   request('/api/admin/inputs/PLAIN_FIXTURE',method='DELETE')
   # Transient draft inputs execute without saving any value.
   scoped=dict(name='Scoped fixture',kind='lua',target='local token=sacho.env("LOCAL_TOKEN"); assert(#token>0); return sacho.json_encode({value=sacho.env("LOCAL_VALUE"),token_length=#token})',interval_seconds=60,timeout_seconds=3,publish_value=True,display_pointer='/value',input_updates=[{'name':'LOCAL_VALUE','value':'plain check input','encrypted':False},{'name':'LOCAL_TOKEN','value':'private-scoped-sentinel','encrypted':True}])
   assert request('/api/admin/test',scoped)['status']=='operational';assert not request('/api/admin/items')
   ident=request('/api/admin/items',scoped)['id'];item=request('/api/admin/items')[0];assert item['spec']['input_bindings']['LOCAL_VALUE']['value']=='plain check input';assert 'private-scoped-sentinel' not in json.dumps(item) and 'input_updates' not in item['spec'];assert 'private-scoped-sentinel' not in connection.execute('SELECT spec FROM items').fetchone()[0]
   assert all(not i['name'].startswith('SACHO_ITEM_') for i in request('/api/admin/inputs')['inputs'])
   unrelated={**scoped,'input_bindings':{},'input_updates':[]};assert request('/api/admin/test',unrelated)['status']=='outage','check inputs escaped their scope'
   assert request('/api/admin/test',item['spec'])['status']=='operational'
   assert 'plain check input' not in json.dumps(request('/api/admin/export'))
   # Restart and hot secret replacement maintain isolation and names-only readback.
   server.terminate();server.wait(timeout=15);server=start();wait_for(lambda:request('/healthz'));assert request('/api/admin/test',item['spec'])['status']=='operational'
   update={**item['spec'],'input_updates':[{'name':'LOCAL_TOKEN','value':'rotated-scoped-sentinel','encrypted':True}]};request('/api/admin/items/'+ident,update,'PUT');current=request('/api/admin/items')[0]['spec'];assert 'rotated-scoped-sentinel' not in json.dumps(current);assert request('/api/admin/test',current)['status']=='operational'
   # Rebinding an encrypted reference from another monitor is rejected.
   try:request('/api/admin/items',{**current,'name':'Foreign binding'});raise AssertionError('foreign secret reference accepted')
   except urllib.error.HTTPError as e:assert e.code==400
   # Plain script templates are executable and use selected inputs.
   disk={**templates['shell-disk']['spec'],'value_rules':None};assert request('/api/admin/test',disk)['status']=='operational'
   process={**templates['shell-process']['spec'],'input_updates':[{'name':'PROCESS_NAME','value':subprocess.check_output(['ps','-p',str(os.getpid()),'-o','comm='],text=True).strip(),'encrypted':False}]};result=request('/api/admin/test',process);assert result['status']=='operational',result
   class AuthFixture(BaseHTTPRequestHandler):
    received=None
    def do_GET(self):
     AuthFixture.received=self.headers.get('Authorization');self.send_response(200);self.end_headers();self.wfile.write(b'{"value":75}')
    def log_message(self,*args):pass
   http=ThreadingHTTPServer(('127.0.0.1',0),AuthFixture);threading.Thread(target=http.serve_forever,daemon=True).start()
   try:
    authorization='Bearer fixture-quote"-backslash\\-value'
    custom={**templates['shell-http']['spec'],'input_updates':[{'name':'CHECK_URL','value':f'http://127.0.0.1:{http.server_port}/value','encrypted':False},{'name':'AUTHORIZATION','value':authorization,'encrypted':True}]}
    result=request('/api/admin/test',custom);assert result['status']=='operational' and result['display_value']=='75',result;assert AuthFixture.received==authorization
   finally:http.shutdown();http.server_close()
   with sync_playwright() as p:
    browser=p.chromium.launch(headless=True);page=browser.new_page(viewport={'width':1440,'height':1100});page.add_init_script("localStorage.setItem('sacho-language','en')");errors=[];page.on('pageerror',lambda e:errors.append(str(e)));page.goto(base+'/admin');page.get_by_role('button',name='Edit',exact=True).click()
    expect(page.locator('[name=prometheus_query]')).not_to_be_visible();expect(page.locator('[name=prometheus_query]')).to_be_disabled();expect(page.locator('[name=expected_status]')).not_to_be_visible();expect(page.locator('[name=expected_exit]')).to_be_visible();expect(page.locator('#value-options')).not_to_have_attribute('open','')
    page.locator('[data-edit-section=edit-values]').click();expect(page.locator('#value-options')).to_have_attribute('open','');expect(page.locator('[name=trend_pointer]')).to_be_disabled();page.locator('[name=publish_trend]').check();expect(page.locator('[name=trend_pointer]')).to_be_enabled();page.locator('#value-options summary').click();expect(page.locator('[name=rule_warning]')).not_to_be_visible()
    page.locator('[name=kind]').select_option('prometheus');expect(page.locator('[name=prometheus_query]')).to_be_visible();expect(page.locator('[name=expected_status]')).to_be_visible();expect(page.locator('[name=expected_exit]')).not_to_be_visible();page.locator('[name=kind]').select_option('dns');expect(page.locator('[name=dns_record_type]')).to_be_visible();expect(page.locator('[name=prometheus_query]')).not_to_be_visible();page.locator('[name=kind]').select_option('push');expect(page.locator('[name=cron]')).not_to_be_visible();expect(page.locator('[name=target]')).not_to_be_visible();expect(page.locator('#test-draft')).to_be_disabled()
    page.locator('[name=kind]').select_option('lua');page.locator('[name=threshold]').fill('1');expect(page.locator('[name=comparison]')).to_be_enabled();page.locator('[name=threshold]').fill('');expect(page.locator('[name=comparison]')).to_be_disabled()
    rows=page.locator('.item-input-row');expect(rows).to_have_count(2);assert 'rotated-scoped-sentinel' not in page.content();rows.filter(has=page.locator('.binding-name[value="LOCAL_TOKEN"]')).locator('[data-action=remove-item-input]').click();page.get_by_role('button',name='Save service',exact=True).click();page.locator('#edit-dialog').wait_for(state='hidden');assert 'LOCAL_TOKEN' not in request('/api/admin/items')[0]['spec']['input_bindings']
    page.get_by_role('button',name='Edit',exact=True).click();page.get_by_role('button',name='Add input',exact=True).click();row=page.locator('.item-input-row').last;row.locator('.binding-name').fill('UI_NORMAL');row.locator('[data-encryption]').uncheck();row.locator('.binding-value').fill('browser ordinary value');page.get_by_role('button',name='Save service',exact=True).click();page.locator('#edit-dialog').wait_for(state='hidden');assert request('/api/admin/items')[0]['spec']['input_bindings']['UI_NORMAL']=={'value':'browser ordinary value','encrypted':False}
    page.get_by_role('button',name='Check templates',exact=True).click();expect(page.locator('[data-id=ping]')).to_be_visible();page.locator('[data-id=ping]').click();expect(page.locator('[name=kind]')).to_have_value('ping');expect(page.locator('[name=ping_count]')).to_be_visible();expect(page.locator('.sacho-lua-editor')).to_have_count(0);page.locator('[data-close=edit-dialog]').click();assert not errors,errors;browser.close()
   request('/api/admin/items/'+ident,method='DELETE');assert not subprocess.check_output([str(binary),'secrets','list','--vault',str(vault),'--key-file',str(key)],text=True).strip()
   # Actual data points with a missing bucket must produce two SVG subpaths.
   graph=dict(name='Graph fixture',kind='lua',target='return sacho.json_encode({value=10})',interval_seconds=5,timeout_seconds=3,publish_value=True,display_pointer='/value',publish_trend=True,trend_pointer='/value')
   gid=request('/api/admin/items',graph)['id'];wait_for(lambda:request('/api/status')['items'][0]['status']=='operational');at=int(time.time())//5*5
   for offset,value in [(-40,10),(-35,20),(-30,15),(-20,30),(-15,25),(-10,35)]:connection.execute('INSERT INTO checks(item_id,at,status,latency_ms,output,detail,metric_value,display_value) VALUES(?,?,?,?,?,?,?,?)',(gid,at+offset,'operational',1,json.dumps({'value':value}),'',value,str(value)))
   connection.commit()
   with sync_playwright() as p:
    browser=p.chromium.launch(headless=True);page=browser.new_page();page.add_init_script("localStorage.setItem('sacho-language','en')");page.goto(base);page.locator('#range').select_option('3600');page.locator('#precision').select_option('5');expect(page.locator('.trend-svg path')).to_have_count(1);path=page.locator('.trend-svg path').get_attribute('d');assert 'C' in path and path.count('M')>=2,path
    page.locator('[data-graph-curve]').select_option('straight');path=page.locator('.trend-svg path').get_attribute('d');assert 'C' not in path and 'L' in path and path.count('M')>=2,path
    page.locator('.trend-panel summary').click();expect(page.locator('.trend-svg')).not_to_be_visible();page.locator('#range').select_option('21600');expect(page.locator('.trend-panel')).not_to_have_attribute('open','');page.locator('.trend-panel summary').click();expect(page.locator('.trend-svg')).to_be_visible();browser.close()
   connection.close();print('PASS: native IPv4/IPv6 ping, plaintext/encrypted global and scoped inputs, isolation/restart/rotation/deletion, source-aware controls, folds, curve selection and missing-data gaps')
  finally:
   if server and server.poll() is None:server.terminate();server.wait(timeout=20)
   log.close()
if __name__=='__main__':main()
