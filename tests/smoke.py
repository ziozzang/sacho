#!/usr/bin/env python3
"""Hermetic end-to-end checks of the actual binary, Lua, SQLite and delivery."""
import contextlib, email, json, os, pathlib, signal, socket, socketserver, subprocess, tempfile, threading, time, urllib.request, urllib.error
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
ROOT=pathlib.Path(__file__).resolve().parents[1]
BINARY=pathlib.Path(os.environ.get('SACHO_BINARY',ROOT/'target/debug/sacho')).resolve()
def free_port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def wait_for(fn,timeout=15):
 deadline=time.monotonic()+timeout
 while time.monotonic()<deadline:
  try:
   result=fn()
   if result:return result
  except (OSError,urllib.error.HTTPError):pass
  time.sleep(.15)
 raise AssertionError('condition timed out')
class Fixture(BaseHTTPRequestHandler):
 failed=False;events=[]
 def log_message(self,*args):pass
 def do_GET(self):
  body=json.dumps({'status':'ok','version':'v-test','tps':42.5}).encode()
  self.send_response(503 if self.failed else 200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers.get('Content-Length','0'))));self.events.append(body)
  self.send_response(204);self.end_headers()
class SMTP(socketserver.StreamRequestHandler):
 messages=[]
 def handle(self):
  self.wfile.write(b'220 localhost test SMTP\r\n')
  while True:
   line=self.rfile.readline()
   if not line:return
   command=line.decode().split()[0].upper()
   if command in ('EHLO','HELO'):self.wfile.write(b'250-localhost\r\n250 SIZE 1000000\r\n')
   elif command=='DATA':
    self.wfile.write(b'354 End with dot\r\n');body=[]
    while True:
     line=self.rfile.readline()
     if line in (b'.\r\n',b''):break
     body.append(line)
    self.messages.append(b''.join(body));self.wfile.write(b'250 accepted\r\n')
   elif command=='QUIT':self.wfile.write(b'221 bye\r\n');return
   else:self.wfile.write(b'250 ok\r\n')
def main():
 http=ThreadingHTTPServer(('127.0.0.1',0),Fixture);threading.Thread(target=http.serve_forever,daemon=True).start()
 smtp=socketserver.ThreadingTCPServer(('127.0.0.1',0),SMTP);smtp.daemon_threads=True;threading.Thread(target=smtp.serve_forever,daemon=True).start()
 with tempfile.TemporaryDirectory() as tmp:
  tmp=pathlib.Path(tmp);port=free_port();base=f'http://127.0.0.1:{port}';upstream=f'http://127.0.0.1:{http.server_port}'
  cfg=tmp/'sacho.toml';cfg.write_text(f'bind="127.0.0.1:{port}"\ndatabase="{tmp}/data.db"\npublic_url="{base}"\n[auth]\nmode="auth-proxy"\nadmins=["admin@example.com"]\n[smtp]\nhost="127.0.0.1"\nport={smtp.server_address[1]}\nfrom="status@example.com"\nsecurity="plain"\n')
  log=open(tmp/'server.log','w');proc=subprocess.Popen([str(BINARY),'serve','--config',str(cfg)],stdout=log,stderr=log)
  def request(path,method='GET',body=None,admin=True,extra=None):
   headers={'Content-Type':'application/json','X-Sacho-Request':'1'}
   if admin:headers['X-Forwarded-Email']='admin@example.com'
   headers.update(extra or {})
   req=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),headers=headers,method=method)
   with urllib.request.urlopen(req,timeout=15) as res:return json.load(res)
  try:
   wait_for(lambda:request('/healthz'))
   try:request('/api/admin/items',admin=False);raise AssertionError('missing auth accepted')
   except urllib.error.HTTPError as e:assert e.code==401
   lua=f'local r=sacho.http("{upstream}/health"); assert(r.status==200,"upstream unhealthy"); local d=sacho.json_decode(r.body); return sacho.json_encode({{version=d.version,tps=d.tps}})'
   spec={'name':'Lua scenario','kind':'lua','target':lua,'interval_seconds':5,'timeout_seconds':3,'publish_value':True,'display_pointer':'/version','display_label':'Version'}
   tested=request('/api/admin/test','POST',spec);assert tested['status']=='operational',tested;assert tested['display_value']=='v-test'
   assert request('/api/incidents')==[]
   # External programs own application-specific logic; Lua only executes argv.
   external=dict(spec,name='External scenario')
   program='import json,sys,os;assert sys.stdin.read()=="";assert "HOME" not in os.environ;assert "SACHO_PROBE_ALLOWED_ENV" not in os.environ;print(json.dumps({"version":sys.argv[1]}))'
   external['target']=f'local r=sacho.exec({{"python3","-c",{json.dumps(program)},"literal $(echo unsafe)"}});assert(r.exit_code==0);assert(sacho.usage==nil);return r.stdout'
   result=request('/api/admin/test','POST',external);assert result['status']=='operational',result;assert result['display_value']=='literal $(echo unsafe)',result
   program='import sys;sys.stdout.write("a"*200000);sys.stderr.write("b"*200000)'
   external['target']=f'local r=sacho.exec({{"python3","-c",{json.dumps(program)}}});assert(r.exit_code==0);assert(#r.stdout==65536);assert(#r.stderr==65536);return "bounded"'
   result=request('/api/admin/test','POST',external);assert result['status']=='operational',result
   external['target']='local r=sacho.exec({"python3","-c","import sys;sys.exit(7)"});assert(r.exit_code==7);return "expected failure"'
   result=request('/api/admin/test','POST',external);assert result['status']=='operational',result
   external.update(timeout_seconds=1,target='sacho.exec({"python3","-c","import time;time.sleep(10)"})')
   started=time.monotonic();result=request('/api/admin/test','POST',external);assert result['status']=='outage',result;assert time.monotonic()-started<4
   request('/api/admin/webhooks','POST',{'name':'test','url':upstream+'/hook'})
   request('/api/subscribe','POST',{'email':'reader@example.com'})
   wait_for(lambda:len(SMTP.messages)>=1)
   # Get token from private test database; never expose it in public APIs.
   import sqlite3
   conn=sqlite3.connect(tmp/'data.db');token=conn.execute('SELECT token FROM subscribers').fetchone()[0]
   # GET must not confirm (email scanners follow links automatically).
   with urllib.request.urlopen(base+'/subscription/confirm?token='+token) as res:assert b'<form' in res.read()
   assert conn.execute('SELECT verified FROM subscribers').fetchone()[0]==0
   req=urllib.request.Request(base+'/subscription/confirm?token='+token,data=b'',method='POST')
   with urllib.request.urlopen(req) as res:assert res.status==200
   item=request('/api/admin/items','POST',spec)['id']
   wait_for(lambda:request('/api/status')['items'][0]['status']=='operational')
   assert request('/api/status')['items'][0]['display_value']=='v-test'
   request('/api/admin/layout','PUT',{'sections':[{'title':'Lua functional checks','item_ids':[item]}]})
   assert request('/api/status')['site']['layout'][0]['title']=='Lua functional checks'
   # Display version cannot affect health without a numeric condition.
   spec.update(display_pointer='/tps',metric_unit=' TPS',threshold=10,comparison='gt',json_pointer='/tps')
   request('/api/admin/items/'+item,'PUT',spec);request('/api/admin/items/'+item+'/run','POST')
   wait_for(lambda:request('/api/status')['items'][0]['display_value']=='42.5')
   Fixture.failed=True;request('/api/admin/items/'+item+'/run','POST')
   wait_for(lambda:request('/api/status')['status']=='outage')
   wait_for(lambda:len(Fixture.events)==1 and len(SMTP.messages)==2)
   request('/api/admin/items/'+item+'/run','POST');time.sleep(2.5)
   assert len(Fixture.events)==1 and len(SMTP.messages)==2,'duplicate incident notification'
   Fixture.failed=False;request('/api/admin/items/'+item+'/run','POST')
   wait_for(lambda:request('/api/status')['status']=='operational')
   wait_for(lambda:len(Fixture.events)==2 and len(SMTP.messages)==3)
   assert [e['event'] for e in Fixture.events]==['incident.started','incident.resolved']
   assert all('delivery_id' in e for e in Fixture.events)
   logs=request('/api/admin/items/'+item+'/logs');assert any(l['status']=='outage' for l in logs)
   for precision in (5,10,30,60,300,1800,86400):
    to=int(time.time())+1;history=request(f'/api/history/{item}?from={to-3600}&to={to}&precision={precision}');assert history['buckets']
   with urllib.request.urlopen(urllib.request.Request(base+'/subscription/unsubscribe?token='+token,data=b'',method='POST')) as res:assert res.status==200
   assert not request('/api/admin/subscribers')
   proc.terminate();proc.wait(timeout=20)
   proc=subprocess.Popen([str(BINARY),'serve','--config',str(cfg)],stdout=log,stderr=log)
   wait_for(lambda:request('/healthz'));assert len(request('/api/incidents'))==1
   assert request('/api/status')['site']['layout'][0]['title']=='Lua functional checks'
   conn.close()
   print('PASS: Lua HTTP + JSON, draft testing, display strings/TPS, auth, subscriptions/SMTP, incident deduplication, recovery/webhooks, precision, persistence and layout')
  finally:
   proc.terminate()
   try:proc.wait(timeout=20)
   except subprocess.TimeoutExpired:proc.kill();proc.wait()
   log.close()
 http.shutdown();smtp.shutdown()
if __name__=='__main__':main()
