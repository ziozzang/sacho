#!/usr/bin/env python3
"""Admin thresholds, graph/freshness, setup/export, delivery controls and live worker heartbeats."""
import json,os,pathlib,shutil,subprocess,tempfile,threading,time,urllib.request,urllib.error,sqlite3,socketserver
from http.server import ThreadingHTTPServer
from smoke import ROOT,BINARY,free_port,wait_for,Fixture,SMTP
from playwright.sync_api import sync_playwright,expect
TOKEN='fixture-worker-token-0123456789abcdef'
def main():
 http=ThreadingHTTPServer(('127.0.0.1',0),Fixture);threading.Thread(target=http.serve_forever,daemon=True).start()
 smtp=socketserver.ThreadingTCPServer(('127.0.0.1',0),SMTP);smtp.daemon_threads=True;threading.Thread(target=smtp.serve_forever,daemon=True).start()
 with tempfile.TemporaryDirectory() as directory:
  directory=pathlib.Path(directory);binary=directory/'sacho';shutil.copy2(BINARY,binary);port=free_port();base=f'http://127.0.0.1:{port}';cfg=directory/'sacho.toml'
  cfg.write_text(f'bind="127.0.0.1:{port}"\ndatabase="{directory}/db"\npublic_url="{base}"\n[[remote_workers]]\nid="edge"\ntoken_env="TEST_NODE_TOKEN"\nallow_scripts=true\n[smtp]\nhost="127.0.0.1"\nport={smtp.server_address[1]}\nfrom="status@example.com"\nsecurity="plain"\n')
  log=open(directory/'server.log','w');server=subprocess.Popen([str(binary),'serve','--config',str(cfg)],stdout=log,stderr=log,env={**os.environ,'TEST_NODE_TOKEN':TOKEN});worker=None
  def request(path,body=None,method=None,headers=None):
   r=urllib.request.Request(base+path,data=json.dumps(body).encode() if body is not None else None,method=method,headers={'Content-Type':'application/json','X-Sacho-Request':'1',**(headers or {})})
   with urllib.request.urlopen(r,timeout=20) as response:return json.load(response)
  try:
   wait_for(lambda:request('/healthz'))
   rules=dict(pointer='/remaining',direction='low',warning=20,critical=5)
   spec=dict(name='Allowance fixture',kind='lua',target='return sacho.json_encode({remaining=80,private="private-inline-sentinel"})',interval_seconds=5,timeout_seconds=3,publish_value=True,display_pointer='/remaining',metric_unit='%',publish_trend=True,trend_pointer='/remaining',value_rules=rules)
   for n,status in [(80,'operational'),(20,'degraded'),(5,'outage')]:
    draft={**spec,'target':f'return sacho.json_encode({{remaining={n}}})'};result=request('/api/admin/test',draft);assert result['status']==status and result['display_value']==str(n),result
   result=request('/api/admin/test',{**spec,'json_pointer':'/missing','threshold':1,'comparison':'gte'});assert result['status']=='outage' and result['display_value']=='80',result
   invalid={**spec,'target':'local = broken'};assert not request('/api/admin/validate',invalid)['valid']
   try:request('/api/admin/items',invalid);raise AssertionError('invalid Lua saved')
   except urllib.error.HTTPError as e:assert e.code==400
   missing={**spec,'required_env':['NOT_INJECTED'],'target':'return 1'};assert 'NOT_INJECTED' in json.dumps(request('/api/admin/validate',missing)['warnings'])
   examples=request('/api/admin/examples');assert {e['id'] for e in examples}=={'http','json','docker','postgresql','prometheus','shell','python','lua','dns','tcp','push','prometheus-native','ping','snmp','shell-disk','shell-process','shell-http'}
   for e in examples:assert request('/api/admin/validate',e['spec'])['valid']
   ident=request('/api/admin/items',spec)['id'];wait_for(lambda:request('/api/status')['items'][0]['last_success'])
   now=int(time.time());connection=sqlite3.connect(directory/'db');connection.execute('INSERT INTO checks(item_id,at,status,latency_ms,output,detail,metric_value,display_value) VALUES(?,?,?,?,?,?,?,?)',(ident,now-60,'operational',12,'{"remaining":60}', '',60,'60'));connection.commit()
   spec.update(target='return sacho.json_encode({remaining=20,private="private-inline-sentinel"})');request('/api/admin/items/'+ident,spec,'PUT');request('/api/admin/items/'+ident+'/run',{},'POST')
   data=wait_for(lambda:(lambda d:d if d['items'][0]['status']=='degraded' else None)(request('/api/status')));item=data['items'][0];assert item['value_severity']=='degraded';assert item['next_check'] and item['last_success'];assert 'private-inline-sentinel' not in json.dumps(data)
   history=request(f'/api/history/{ident}?from={now-120}&to={int(time.time())+1}&precision=5');assert any(b['metric_min']==20 and b['metric_max']>=20 for b in history['buckets'])
   with sync_playwright() as p:
    browser=p.chromium.launch();page=browser.new_page(viewport={'width':1440,'height':1100});errors=[];page.on('pageerror',lambda e:errors.append(str(e)));page.add_init_script("localStorage.setItem('sacho-language','en')")
    page.goto(base);card=page.locator('.service').filter(has_text='Allowance fixture');expect(card.locator('.metric.degraded')).to_contain_text('20%');expect(card.locator('.trend-svg')).to_be_visible();expect(card.locator('.freshness')).to_contain_text('Last healthy check');card.locator('.trend-svg circle').first.focus();expect(page.locator('#chart-tooltip')).to_contain_text('Bucket average');card.locator('[data-series]').select_option('latency');expect(card.locator('.trend-svg')).to_be_visible()
    page.set_viewport_size({'width':390,'height':844});assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
    page.set_viewport_size({'width':1440,'height':1100});page.goto(base+'/admin');page.screenshot(path=str(ROOT/'runtime/screenshots/admin-overview-v017.png'),full_page=True);page.set_viewport_size({'width':390,'height':844});assert page.evaluate('document.documentElement.scrollWidth<=innerWidth');page.set_viewport_size({'width':1440,'height':1100});expect(page.locator('.summary-card')).to_have_count(4);page.locator('#monitor-search').fill('does-not-exist');expect(page.locator('#monitor-list')).to_contain_text('No matching monitors');page.get_by_role('button',name='Clear filters',exact=True).click();page.locator('#monitor-location').select_option('');expect(page.locator('.admin-item')).to_have_count(1);page.locator('#monitor-filter').select_option('degraded');expect(page.locator('.admin-item')).to_have_count(1);page.locator('#monitor-search').fill('allowance');expect(page.locator('.admin-item')).to_have_count(1);page.get_by_role('button',name='Edit',exact=True).first.click();page.screenshot(path=str(ROOT/'runtime/screenshots/admin-editor-v017.png'),full_page=True);page.set_viewport_size({'width':390,'height':844});assert page.locator('#edit-dialog').evaluate('(d)=>d.scrollWidth<=d.clientWidth'),page.locator('#edit-dialog').evaluate('(d)=>[...d.querySelectorAll("*")].filter(e=>e.getBoundingClientRect().right>d.getBoundingClientRect().right).map(e=>[e.tagName,e.className,e.getBoundingClientRect().width]).slice(0,15)');page.set_viewport_size({'width':1440,'height':1100});expect(page.locator('#edit-form fieldset')).to_have_count(6);expect(page.locator('#value-options')).to_have_attribute('open','');expect(page.locator('[name=rule_warning]')).to_have_value('20');expect(page.locator('[name=rule_critical]')).to_have_value('5');expect(page.locator('[name=publish_trend]')).to_be_checked();page.get_by_role('button',name='Validate configuration',exact=True).click();expect(page.locator('#validation-result')).to_contain_text('Configuration valid');page.locator('[name=rule_warning]').fill('25');page.get_by_role('button',name='Save service',exact=True).click();page.locator('#edit-dialog').wait_for(state='hidden');assert request('/api/admin/items')[0]['spec']['value_rules']['warning']==25
    page.get_by_role('button',name='Check templates',exact=True).click();page.locator('.panel').filter(has_text='Docker container').get_by_role('button',name='Use template').click();expect(page.locator('[name=required_env]')).to_have_value('DOCKER_SOCKET, DOCKER_CONTAINER');expect(page.locator('#example-inputs')).to_contain_text('DOCKER_SOCKET');page.locator('[data-close=edit-dialog]').click()
    page.get_by_role('button',name='Settings',exact=True).click()
    with page.expect_download() as download:page.get_by_role('button',name='Export safe configuration',exact=True).click()
    text=pathlib.Path(download.value.path()).read_text();assert 'private-inline-sentinel' not in text and 'target_omitted' in text and TOKEN not in text
    page.get_by_role('button',name='Notifications',exact=True).click();page.locator('[name=settle_seconds]').fill('3');page.locator('[name=cooldown_seconds]').fill('8');page.locator('#policy-form button.primary').click();wait_for(lambda:request('/api/admin/notification-policy')['settle_seconds']==3)
    assert not errors,errors;browser.close();print('PASS: admin thresholds, trend/freshness UI, catalogue, validation and export',flush=True)
   # A change in numeric definition clears only scalar history, preserving health checks.
   count=connection.execute('SELECT COUNT(*) FROM checks WHERE item_id=?',(ident,)).fetchone()[0];changed={**spec,'trend_pointer':'/other'};request('/api/admin/items/'+ident,changed,'PUT');assert connection.execute('SELECT COUNT(*) FROM checks WHERE item_id=?',(ident,)).fetchone()[0]>=count;assert connection.execute('SELECT COUNT(*) FROM checks WHERE item_id=? AND metric_value IS NOT NULL',(ident,)).fetchone()[0]==0
   request('/api/admin/items/'+ident,method='DELETE')
   request('/api/admin/webhooks',dict(name='Owned callback',url=f'http://127.0.0.1:{http.server_port}/hook'))
   flap=dict(name='Short problem',kind='lua',target='return "bad"',contains='good',interval_seconds=5,timeout_seconds=3)
   fid=request('/api/admin/items',flap)['id'];wait_for(lambda:request('/api/status')['items'][0]['status']=='outage');flap['target']='return "good"';request('/api/admin/items/'+fid,flap,'PUT');request('/api/admin/items/'+fid+'/run',{},'POST');wait_for(lambda:request('/api/status')['items'][0]['status']=='operational');time.sleep(4);assert not Fixture.events,Fixture.events
   assert connection.execute("SELECT COUNT(*) FROM outbox WHERE error LIKE 'suppressed:%'").fetchone()[0]>=1
   request('/api/admin/notification-policy',dict(settle_seconds=0,cooldown_seconds=0,maintenance_until=int(time.time())+60),'PUT');flap['target']='return "bad"';request('/api/admin/items/'+fid,flap,'PUT');request('/api/admin/items/'+fid+'/run',{},'POST');wait_for(lambda:request('/api/status')['items'][0]['status']=='outage');assert not Fixture.events
   request('/api/subscribe',{'email':'reader@example.com'});wait_for(lambda:SMTP.messages);request('/api/admin/notification-policy',dict(settle_seconds=0,cooldown_seconds=0,maintenance_until=0),'PUT');flap['target']='return "good"';request('/api/admin/items/'+fid,flap,'PUT');request('/api/admin/items/'+fid+'/run',{},'POST');wait_for(lambda:request('/api/status')['items'][0]['status']=='operational');time.sleep(2.5);assert not Fixture.events,'orphan recovery after maintenance'
   request('/api/admin/items/'+fid,method='DELETE')
   # Authorization failure must not create a heartbeat.
   try:request('/api/workers/edge/heartbeat',dict(protocol=1,allow_scripts=True));raise AssertionError('unauthorized heartbeat')
   except urllib.error.HTTPError as e:assert e.code==401
   wc=directory/'worker.toml';wc.write_text(f'control_url="{base}"\nnode_id="edge"\ntoken_env="TEST_NODE_TOKEN"\nallow_scripts=true\n')
   worker=subprocess.Popen([str(binary),'worker','--config',str(wc)],stdout=log,stderr=log,env={**os.environ,'TEST_NODE_TOKEN':TOKEN})
   wait_for(lambda:request('/api/admin/workers')[0]['connection']=='online');assert request('/api/admin/workers')[0]['reported_version']==request('/healthz')['version']
   remote=dict(name='Long remote check',worker='edge',kind='lua',target='sacho.exec({"python3","-c","import time;time.sleep(35)"}); return true',interval_seconds=60,timeout_seconds=40)
   request('/api/admin/items',remote);wait_for(lambda:request('/api/admin/workers')[0]['active_checks']);start=int(time.time());time.sleep(31)
   agent=request('/api/admin/workers')[0];assert agent['connection']=='online' and agent['last_seen']>=start+20 and agent['active_checks'],agent
   worker.terminate();worker.wait(timeout=10);worker=None;connection.execute("UPDATE settings SET value=? WHERE key='worker.heartbeat.edge'",(json.dumps({'at':int(time.time())-60,'version':'test'}),));connection.commit();assert request('/api/admin/workers')[0]['connection']=='offline';connection.close()
   print('PASS: admin thresholds, graph/freshness/mobile, catalogue/validation/export privacy, debounce/maintenance/SMTP, and authenticated heartbeats during long remote checks')
  except Exception:
   try:
    print('Fixture diagnostic:',[(i['spec']['name'],i['status'],i['revision']) for i in request('/api/admin/items')],flush=True)
    print('Fixture deliveries:',request('/api/admin/notifications'),flush=True)
    print((directory/'server.log').read_text()[-3000:],flush=True)
   except Exception:pass
   raise
  finally:
   if worker:worker.terminate();worker.wait(timeout=10)
   server.terminate();server.wait(timeout=20);log.close()
 http.shutdown();http.server_close();smtp.shutdown();smtp.server_close()
if __name__=='__main__':main()
