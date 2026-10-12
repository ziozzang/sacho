#!/usr/bin/env python3
"""Run an isolated real Hangang binary; production routes remain untouched."""
import json,os,pathlib,subprocess,tempfile,threading,urllib.request,urllib.error,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from smoke import BINARY,free_port,wait_for
HANGANG=pathlib.Path(os.environ.get('HANGANG_BINARY',pathlib.Path.home()/'hangang/target/release/hangang'))
class Auth(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  cookie=self.headers.get('Cookie','')
  if cookie=='sacho-test=admin':email='admin@example.com'
  elif cookie=='sacho-test=reader':email='reader@example.com'
  else:self.send_response(403);self.end_headers();return
  self.send_response(200);self.send_header('x-forwarded-email',email);self.end_headers()
def main():
 auth=ThreadingHTTPServer(('127.0.0.1',0),Auth);threading.Thread(target=auth.serve_forever,daemon=True).start()
 with tempfile.TemporaryDirectory() as tmp:
  tmp=pathlib.Path(tmp);port=free_port();gateway=free_port();admin=free_port()
  cfg=tmp/'sacho.toml';cfg.write_text(f'bind="127.0.0.1:{port}"\ndatabase="{tmp}/sacho.db"\npublic_url="http://127.0.0.1:{gateway}"\n[auth]\nmode="auth-proxy"\nemail_header="X-Forwarded-Email"\nadmins=["admin@example.com"]\n')
  authentication={'url':f'http://127.0.0.1:{auth.server_port}/verify','request_headers':['cookie'],'response_headers':['x-forwarded-email'],'timeout_ms':1000}
  backend=[f'http://127.0.0.1:{port}']
  routes=[{'id':'sacho-hopae','path_prefix':'/_hopae','priority':30,'access_mode':'protected','backends':backend,'auth':authentication},{'id':'sacho-admin-api','path_prefix':'/api/admin/','priority':20,'access_mode':'protected','backends':backend,'auth':authentication},{'id':'sacho-console','path_prefix':'/admin','priority':20,'access_mode':'protected','backends':backend,'auth':authentication},{'id':'sacho-public','path_prefix':'/','access_mode':'public','backends':backend}]
  proxy=tmp/'hangang.json';proxy.write_text(json.dumps({'http':routes}))
  log=open(tmp/'integration.log','w');sacho=subprocess.Popen([str(BINARY),'serve','--config',str(cfg)],stdout=log,stderr=log)
  env=os.environ.copy();env['HANGANG_ADMIN_TOKEN']=uuid.uuid4().hex
  hangang=subprocess.Popen([str(HANGANG),'--config',str(proxy),'--listen',f'127.0.0.1:{gateway}','--admin',f'127.0.0.1:{admin}'],env=env,stdout=log,stderr=log)
  def request(path,cookie=None,forged=True):
   headers={}
   if cookie:headers['Cookie']='sacho-test='+cookie
   if forged:headers['X-Forwarded-Email']='admin@example.com'
   req=urllib.request.Request(f'http://127.0.0.1:{gateway}'+path,headers=headers)
   with urllib.request.urlopen(req,timeout=5) as r:return json.load(r)
  try:
   wait_for(lambda:request('/healthz'))
   assert request('/api/status')['status']=='unknown'
   try:request('/_hopae/login');raise AssertionError('Hopae namespace bypassed authentication')
   except urllib.error.HTTPError as e:assert e.code==403,e.code
   for cookie,expected in [(None,403),('reader',401)]:
    try:request('/api/admin/items',cookie);raise AssertionError('forged administrator accepted')
    except urllib.error.HTTPError as e:assert e.code==expected,e.code
   assert request('/api/admin/items','admin')==[]
   print('PASS: real Hangang public routing, auth-proxy identity injection, forged-header stripping, non-admin denial and administrator access')
  except Exception:
   print((tmp/'integration.log').read_text());raise
  finally:
   for child in [hangang,sacho]:
    child.terminate()
    try:child.wait(timeout=20)
    except subprocess.TimeoutExpired:child.kill();child.wait()
   log.close()
 auth.shutdown()
if __name__=='__main__':main()
