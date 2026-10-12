#!/usr/bin/env python3
"""Executable starters, hot vault inputs, native sources, push and durable actions."""
import json,os,pathlib,shutil,socket,struct,subprocess,tempfile,threading,time,urllib.request,urllib.error
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from smoke import BINARY,free_port,wait_for
from playwright.sync_api import sync_playwright,expect
class API(BaseHTTPRequestHandler):
 sample=42;posts=[]
 def do_GET(self):
  payload={'status':'success','data':{'resultType':'vector','result':[{'metric':{},'value':[int(time.time()),str(self.sample)]}]}}
  body=json.dumps(payload).encode();self.send_response(200);self.end_headers();self.wfile.write(body)
 def do_POST(self):
  self.posts.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))));self.send_response(204);self.end_headers()
 def log_message(self,*args):pass

def main():
 api=ThreadingHTTPServer(('127.0.0.1',0),API);threading.Thread(target=api.serve_forever,daemon=True).start()
 dns=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);dns.bind(('127.0.0.1',0));dns.settimeout(.2);stop=threading.Event()
 def dns_server():
  while not stop.is_set():
   try:
    data,peer=dns.recvfrom(4096);pos=12
    while data[pos]:pos+=1+data[pos]
    end=pos+5;question=data[12:end];type_=struct.unpack('!H',data[pos+1:pos+3])[0]
    answer=b'\xc0\x0c'+struct.pack('!HHIH',type_,1,30,4)+socket.inet_aton('192.0.2.7')
    dns.sendto(data[:2]+struct.pack('!HHHHH',0x8180,1,1,0,0)+question+answer,peer)
   except socket.timeout:pass
   except OSError:break
 threading.Thread(target=dns_server,daemon=True).start()
 with tempfile.TemporaryDirectory() as tmp:
  root=pathlib.Path(tmp);binary=root/'sacho';shutil.copy2(BINARY,binary);vault=root/'vault';key=root/'key'
  subprocess.run([str(binary),'secrets','init','--vault',str(vault),'--key-file',str(key)],check=True,capture_output=True)
  port=free_port();base=f'http://127.0.0.1:{port}';cfg=root/'sacho.toml'
  cfg.write_text(f'bind="127.0.0.1:{port}"\ndatabase="{root}/db"\npublic_url="{base}"\n[secrets]\nvault="{vault}"\nkey_file="{key}"\n')
  log=open(root/'log','w');server=None
  def start():return subprocess.Popen([str(binary),'serve','--config',str(cfg)],stdout=log,stderr=log)
  def request(path,body=None,method=None,headers=None):
   req=urllib.request.Request(base+path,data=json.dumps(body).encode() if body is not None else None,method=method,headers={'Content-Type':'application/json','X-Sacho-Request':'1',**(headers or {})})
   with urllib.request.urlopen(req,timeout=20) as r:return json.load(r)
  def rejected(path,body=None,method=None,headers=None,code=400):
   try:request(path,body,method,headers);raise AssertionError('request unexpectedly accepted')
   except urllib.error.HTTPError as e:assert e.code==code,(path,e.code,e.read())
  try:
   server=start();wait_for(lambda:request('/healthz'))
   examples=request('/api/admin/examples');cards={e['id']:e for e in examples}
   for kind in ['shell','python','lua']:
    result=request('/api/admin/test',cards[kind]['spec']);assert result['status']=='operational' and result['display_value'],(kind,result)
   sentinel='private-input-fixture-012345';request('/api/admin/inputs/FIXTURE_SECRET',{'value':sentinel,'enabled':True},'PUT')
   assert sentinel not in json.dumps(request('/api/admin/inputs')) and sentinel.encode() not in vault.read_bytes()
   draft=dict(name='Input probe',kind='lua',target='return sacho.env("FIXTURE_SECRET")',contains=sentinel,interval_seconds=5,timeout_seconds=3)
   assert request('/api/admin/test',draft)['status']=='operational'
   request('/api/admin/inputs/FIXTURE_SECRET',{'value':'rotated-fixture','enabled':True},'PUT');draft['contains']='rotated-fixture';assert request('/api/admin/test',draft)['status']=='operational'
   request('/api/admin/inputs/FIXTURE_SECRET',{'enabled':False},'PUT');assert request('/api/admin/test',draft)['status']=='outage'
   request('/api/admin/inputs/FIXTURE_SECRET',{'enabled':True},'PUT')
   rejected('/api/admin/inputs/PATH',{'value':'bad','enabled':True},'PUT');rejected('/api/admin/inputs/HOME',{'value':'bad','enabled':True},'PUT')
   # Dynamic grants and hot values survive service restart without a config rewrite.
   server.terminate();server.wait(timeout=15);server=start();wait_for(lambda:request('/healthz'));assert request('/api/admin/test',draft)['status']=='operational'
   prom={**cards['prometheus-native']['spec'],'target':f'http://127.0.0.1:{api.server_port}'};result=request('/api/admin/test',prom);assert result['status']=='operational' and float(result['display_value'])==42,result
   API.sample='NaN';assert request('/api/admin/test',prom)['status']=='outage';API.sample=42
   dns_spec={**cards['dns']['spec'],'dns_server':f'127.0.0.1:{dns.getsockname()[1]}','dns_expected':['192.0.2.7']};result=request('/api/admin/test',dns_spec);assert result['status']=='operational' and result['display_value']=='192.0.2.7',result
   dns_spec['dns_expected']=['192.0.2.9'];assert request('/api/admin/test',dns_spec)['status']=='outage'
   tcp={**cards['tcp']['spec'],'target':f'127.0.0.1:{api.server_port}'};assert request('/api/admin/test',tcp)['status']=='operational'
   push={**cards['push']['spec'],'interval_seconds':5,'timeout_seconds':3};pid=request('/api/admin/items',push)['id'];issued=request('/api/admin/push/'+pid+'/token',{},'POST');token=issued['token'];h={'Authorization':'Bearer '+token}
   rejected('/api/push/'+pid,{'status':'operational','data':{'value':7}},headers={'Authorization':'Bearer wrong'},code=401)
   request('/api/push/'+pid,{'status':'operational','data':{'value':7}},headers=h);assert request('/api/status')['items'][0]['display_value']=='7'
   wait_for(lambda:request('/api/status')['items'][0]['status']=='outage',timeout=12);assert request('/api/status')['items'][0]['display_value']=='7' and request('/api/status')['items'][0]['display_value_stale']
   request('/api/push/'+pid,{'status':'operational','data':{'value':8}},headers=h);assert request('/api/status')['items'][0]['status']=='operational'
   new=request('/api/admin/push/'+pid+'/token',{},'POST');rejected('/api/push/'+pid,{'status':'operational','data':{}},headers=h,code=401)
   request('/api/admin/push/'+pid+'/token',method='DELETE');rejected('/api/push/'+pid,{'status':'operational','data':{}},headers={'Authorization':'Bearer '+new['token']},code=401)
   request('/api/admin/items/'+pid,method='DELETE')
   # No action on drafts; delayed failure retries once, honors streak and post-action hold.
   count=root/'attempts';marker=root/'success'
   script=f'import os,json,pathlib\np=pathlib.Path({str(count)!r});n=int(p.read_text())+1 if p.exists() else 1;p.write_text(str(n));event=json.loads(os.environ["SACHO_EVENT_JSON"]);assert event["event"]=="check.failed"\nraise SystemExit(0 if n>=2 else 1)'
   action=dict(trigger='failure',kind='python',target=script,after_count=2,delay_seconds=2,cooldown_seconds=30,max_attempts=2,retry_seconds=1,wait_after_seconds=10,timeout_seconds=3)
   spec=dict(name='Owned action probe',kind='shell',target='exit 1',interval_seconds=5,timeout_seconds=3,actions=[action])
   request('/api/admin/test',spec);assert not count.exists();aid=request('/api/admin/items',spec)['id']
   time.sleep(2);assert not count.exists();wait_for(lambda:count.exists() and count.read_text()=='2',timeout=15)
   jobs=request('/api/admin/notifications');job=next(j for j in jobs if j['kind']=='action');wait_for(lambda:any(j['kind']=='action' and j['delivered_at'] for j in request('/api/admin/notifications')))
   assert next(j for j in request('/api/admin/notifications') if j['kind']=='action')['attempts']==2
   assert 'target' not in request('/api/admin/export')['items'][0]['spec'] and 'actions' not in request('/api/admin/export')['items'][0]['spec']
   last=request('/api/admin/items')[0]['last_check'];time.sleep(6);assert count.read_text()=='2','transition action repeated';assert request('/api/admin/items')[0]['last_check']==last,'post-action hold was ignored'
   # Recovery action can POST per-monitor JSON, separately from shared notifications.
   success=dict(trigger='success',kind='https',target=f'http://127.0.0.1:{api.server_port}',body='{"event":"{{ event }}","status":"{{ status }}"}',timeout_seconds=3)
   spec.update(target='exit 0',actions=[success]);request('/api/admin/items/'+aid,spec,'PUT');request('/api/admin/items/'+aid+'/run',{},'POST');wait_for(lambda:API.posts);assert API.posts[-1]=={'event':'check.succeeded','status':'operational'}
   request('/api/admin/items/'+aid,method='DELETE')
   grace=dict(name='Startup grace',kind='lua',target='return true',interval_seconds=5,timeout_seconds=3,initial_delay_seconds=3)
   gid=request('/api/admin/items',grace)['id'];time.sleep(1);assert request('/api/admin/items')[0]['last_check'] is None;wait_for(lambda:request('/api/admin/items')[0]['last_check']);request('/api/admin/items/'+gid,method='DELETE')
   # Initial delay is persisted and run-now overrides cron.
   scheduled=dict(name='Scheduled probe',kind='lua',target='return true',interval_seconds=5,timeout_seconds=3,cron='0 0 1 1 *',schedule_timezone='Asia/Seoul',initial_delay_seconds=10)
   sid=request('/api/admin/items',scheduled)['id'];time.sleep(1.5);assert request('/api/admin/items')[0]['last_check'] is None
   request('/api/admin/items/'+sid+'/run',{},'POST');wait_for(lambda:request('/api/admin/items')[0]['last_check']);request('/api/admin/items/'+sid,method='DELETE')
   rejected('/api/admin/items',{**scheduled,'schedule_timezone':'not-a-zone'})
   with sync_playwright() as p:
    browser=p.chromium.launch(headless=True);page=browser.new_page(viewport={'width':1440,'height':1000});page.add_init_script("localStorage.setItem('sacho-language','en')");page.goto(base+'/admin');page.get_by_role('button',name='Check examples',exact=True).click()
    for kind in ['shell','python','lua']:
     page.locator('[data-action=use-example][data-id='+kind+']').click();expect(page.locator('[name=kind]')).to_have_value(kind);page.get_by_role('button',name='Test draft',exact=True).click();expect(page.locator('#draft-result')).to_contain_text('Operational');page.locator('[data-close=edit-dialog]').click()
    page.get_by_role('button',name='Monitors',exact=True).click();page.get_by_role('button',name='+ Add service',exact=True).click();page.locator('[name=name]').fill('Browser action config');page.locator('[name=kind]').select_option('lua');page.locator('.sacho-lua-editor .cm-content').fill('return true');page.locator('details.action-config').first.locator('summary').click();page.locator('[name=action_failure_kind]').select_option('lua');page.locator('[name=action_failure_target]').fill('return true');page.locator('[name=action_failure_after_count]').fill('2');page.locator('[name=action_failure_delay_seconds]').fill('4');page.get_by_role('button',name='Save service',exact=True).click();page.locator('#edit-dialog').wait_for(state='hidden');owned=request('/api/admin/items')[0];assert owned['spec']['actions'][0]['after_count']==2 and owned['spec']['actions'][0]['delay_seconds']==4;page.get_by_role('button',name='Edit',exact=True).click();expect(page.locator('[name=action_failure_after_count]')).to_have_value('2');page.locator('[data-close=edit-dialog]').click();request('/api/admin/items/'+owned['id'],method='DELETE')
    page.get_by_role('button',name='Environment & secrets',exact=True).click();expect(page.locator('#admin-content')).to_contain_text('FIXTURE_SECRET');assert 'rotated-fixture' not in page.content()
    page.locator('#input-form [name=name]').fill('BROWSER_INPUT');page.locator('#input-form [name=value]').fill('browser-private-value');page.locator('#input-form .primary').click();expect(page.locator('#admin-content')).to_contain_text('BROWSER_INPUT');assert 'browser-private-value' not in page.content()
    page.locator('#timezone').select_option('UTC');assert page.evaluate("localStorage.getItem('sacho-timezone')")=='UTC';page.locator('#language').select_option('ko');expect(page.locator('#admin-content')).to_contain_text('환경변수');browser.close()
   request('/api/admin/inputs/FIXTURE_SECRET',method='DELETE');assert all(i['name']!='FIXTURE_SECRET' for i in request('/api/admin/inputs')['inputs'])
   print('PASS: language starters, hot encrypted inputs + restart/privacy, native PromQL/DNS/TCP, Push expiry/rotation/revocation, delayed/retried/state-gated actions, cron/run-now, and admin dropdowns')
  finally:
   if server and server.poll() is None:server.terminate();server.wait(timeout=20)
   log.close()
 stop.set();dns.close();api.shutdown();api.server_close()
if __name__=='__main__':main()
