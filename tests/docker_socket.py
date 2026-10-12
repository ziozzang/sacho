#!/usr/bin/env python3
"""Embedded Docker example against an isolated Unix HTTP fixture, no host mutation."""
import json, pathlib, socketserver, subprocess, tempfile, threading, urllib.request
from http.server import BaseHTTPRequestHandler
from smoke import BINARY, ROOT, free_port, wait_for
from playwright.sync_api import sync_playwright, expect
class Inspect(BaseHTTPRequestHandler):
 state='running';health='healthy';missing=False
 def log_message(self,*args): pass
 def do_GET(self):
  assert self.path == '/containers/test-container/json'
  body=json.dumps({'State':{'Status':self.state,'Running':self.state=='running','Health':{'Status':self.health}},'RestartCount':2,'Config':{'Env':['PRIVATE=secret-value']}}).encode()
  self.send_response(404 if self.missing else 200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
def main():
 with tempfile.TemporaryDirectory() as temp:
  temp=pathlib.Path(temp);socket=temp/'docker.sock';server=socketserver.UnixStreamServer(str(socket),Inspect);threading.Thread(target=server.serve_forever,daemon=True).start()
  port=free_port();base=f'http://127.0.0.1:{port}';cfg=temp/'sacho.toml';cfg.write_text(f'bind="127.0.0.1:{port}"\ndatabase="{temp}/db"\nprobe_env=["DOCKER_SOCKET","DOCKER_CONTAINER"]\n')
  import os
  process=subprocess.Popen([str(BINARY),'serve','--config',str(cfg)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env={**os.environ,'DOCKER_SOCKET':str(socket),'DOCKER_CONTAINER':'test-container'})
  def request(path,body=None,method=None):
   r=urllib.request.Request(base+path,data=json.dumps(body).encode() if body is not None else None,method=method,headers={'Content-Type':'application/json','X-Sacho-Request':'1'})
   with urllib.request.urlopen(r,timeout=10) as response:return json.load(response)
  try:
   wait_for(lambda:request('/healthz'))
   script=subprocess.check_output([str(BINARY),'example','docker'],text=True);assert script==(ROOT/'examples/lua/docker.lua').read_text()
   spec=dict(name='Container status',kind='lua',target=script,interval_seconds=5,timeout_seconds=3,threshold=1,comparison='gte',json_pointer='/healthy',publish_value=True,display_pointer='/display',public_message_pointer='/message')
   for state,health,missing,status,display in [('running','healthy',False,'operational','running / healthy'),('running','unhealthy',False,'outage','running / unhealthy'),('running','starting',False,'outage','running / starting'),('exited','healthy',False,'outage','exited / healthy'),('running','healthy',True,'outage','not found')]:
    Inspect.state=state;Inspect.health=health;Inspect.missing=missing
    result=request('/api/admin/test',spec);assert result['status']==status,result;assert result['display_value']==display,result
   warning={**spec,'failure_status':'degraded'}
   assert request('/api/admin/test',warning)['status']=='degraded'
   for method in ['POST','DELETE']:
    bad={**spec,'target':f'return sacho.http({{url="http://localhost/version",unix_socket={json.dumps(str(socket))},method="{method}"}})'}
    assert request('/api/admin/test',bad)['status']=='outage'
   Inspect.state='running';Inspect.health='healthy';Inspect.missing=False
   ident=request('/api/admin/items',spec)['id'];wait_for(lambda:request('/api/status')['items'][0]['display_value']=='running / healthy')
   # A failed query with no scalar retains the previous observation and a safe public explanation.
   failed={**spec,'target':'return sacho.json_encode({healthy=0,message="Container query failed",private="secret-value"})'}
   request('/api/admin/items/'+ident,failed,'PUT');request('/api/admin/items/'+ident+'/run',{},'POST')
   data=wait_for(lambda: (lambda d: d if d['items'][0]['public_message']=='Container query failed' else None)(request('/api/status')))
   item=data['items'][0];assert item['display_value']=='running / healthy' and item['display_value_stale'];assert 'secret-value' not in json.dumps(data)
   with sync_playwright() as p:
    browser=p.chromium.launch();page=browser.new_page();page.add_init_script("localStorage.setItem('sacho-language','en')");page.goto(base)
    card=page.locator('.service').filter(has_text='Container status');expect(card).to_contain_text('running / healthy');expect(card).to_contain_text('Last known value');expect(card).to_contain_text('Container query failed')
    page.set_viewport_size({'width':390,'height':844});assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
    missing_spec={**failed,'name':'No previous value'};request('/api/admin/items',missing_spec);page.reload();expect(page.locator('.service').filter(has_text='No previous value')).to_contain_text('Value unavailable');browser.close()
   print('PASS: Unix socket Docker states, read-only requests, private inspect fields, failed-query value retention and public UI')
  finally:
   process.terminate();process.wait(timeout=20);server.shutdown();server.server_close()
if __name__=='__main__':main()
