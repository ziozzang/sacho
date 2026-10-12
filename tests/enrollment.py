#!/usr/bin/env python3
"""Enrollment through real CLI/API, including a committed-but-lost response."""
import hashlib,json,pathlib,socket,sqlite3,subprocess,tempfile,threading,tomllib,urllib.request,urllib.error
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from smoke import BINARY,free_port,wait_for

def main():
 with tempfile.TemporaryDirectory() as temporary:
  directory=pathlib.Path(temporary);port=free_port();base=f'http://127.0.0.1:{port}'
  central=directory/'controller.toml';central.write_text(f'bind="127.0.0.1:{port}"\npublic_url="{base}"\ndatabase="{directory}/database.db"\n')
  logfile=(directory/'controller.log').open('w');server=subprocess.Popen([str(BINARY),'serve','--config',str(central)],stdout=logfile,stderr=logfile)
  def api(path,body=None,token=None,expected=200,method=None):
   headers={'Content-Type':'application/json','X-Sacho-Request':'1'}
   if token:headers['Authorization']='Bearer '+token
   req=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),headers=headers,method=method)
   try:
    with urllib.request.urlopen(req,timeout=10) as response:assert response.status==expected;return json.load(response)
   except urllib.error.HTTPError as error:assert error.code==expected,(error.code,error.read());return json.loads(error.read())
  class Proxy(BaseHTTPRequestHandler):
   lost=False;tokens=[]
   def log_message(self,*args):pass
   def do_POST(self):
    body=self.rfile.read(int(self.headers.get('Content-Length','0')))
    authorization=self.headers.get('Authorization','')
    if self.path.endswith('/registration'):Proxy.tokens.append(authorization.removeprefix('Bearer '))
    req=urllib.request.Request(base+self.path,data=body,headers={'Authorization':authorization,'Content-Type':'application/json'})
    try:
     with urllib.request.urlopen(req,timeout=10) as response:status=response.status;data=response.read()
    except urllib.error.HTTPError as response:status=response.code;data=response.read()
    if self.path=='/api/workers/enroll' and status==200 and not Proxy.lost:
     Proxy.lost=True;self.connection.shutdown(socket.SHUT_RDWR);self.connection.close();return
    self.send_response(status);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
  proxy=ThreadingHTTPServer(('127.0.0.1',0),Proxy);threading.Thread(target=proxy.serve_forever,daemon=True).start()
  worker=directory/'worker.toml';worker.write_text(f'control_url="http://127.0.0.1:{proxy.server_port}"\nnode_id="edge-new"\nlabel="Remote west"\ntoken_env="SACHO_FIXTURE_JOIN_TOKEN"\nallow_scripts=true\nprobe_env=["LOCATION"]\n')
  try:
   wait_for(lambda:api('/healthz'))
   invitation=api('/api/admin/worker-bootstrap',{'id':'edge-new','allow_scripts':False,'ttl_seconds':60})
   key=invitation['bootstrap_key'];original=worker.read_bytes()
   first=subprocess.run([str(BINARY),'worker-join','--config',str(worker)],input=key,text=True,capture_output=True)
   assert first.returncode!=0 and Proxy.lost,first.stderr
   assert worker.read_bytes()==original
   second=subprocess.run([str(BINARY),'worker-join','--config',str(worker)],input='',text=True,capture_output=True,check=True)
   assert json.loads(second.stdout)['registered']
   configured=tomllib.loads(worker.read_text());assert configured['controller_id'];assert configured['probe_env']==['LOCATION']
   assert configured['control_url']==f'http://127.0.0.1:{proxy.server_port}'
   token=Proxy.tokens[-1];assert len(token)==64
   nodes=api('/api/admin/workers');assert nodes[0]['label']=='Remote west' and not nodes[0]['allow_scripts']
   assert 'token_hash' not in json.dumps(nodes)
   with sqlite3.connect(directory/'database.db') as conn:
    record=json.loads(conn.execute("SELECT value FROM settings WHERE key='worker.node.edge-new'").fetchone()[0]);assert record['token_hash']==hashlib.sha256(token.encode()).hexdigest()
   for file in directory.iterdir():
    if file.is_file():assert key.encode() not in file.read_bytes() and token.encode() not in file.read_bytes(),file.name
   body={'protocol':1,'node_id':'edge-new','label':'Replay','version':'0.1.4','platform':'fixture','token_hash':hashlib.sha256(token.encode()).hexdigest(),'allow_scripts':True}
   api('/api/workers/enroll',body,key,401)
   api('/api/admin/items',{'name':'HTTP only','worker':'edge-new','kind':'http','target':base+'/healthz'})
   api('/api/admin/items',{'name':'Forbidden script','worker':'edge-new','kind':'lua','target':'return true'},expected=400)
   api('/api/admin/workers/edge-new',method='DELETE')
   api('/api/workers/edge-new/claim',{'protocol':1,'allow_scripts':True},token,401)
   invitation=api('/api/admin/worker-bootstrap',{'id':'edge-new','allow_scripts':True,'ttl_seconds':60})
   third=subprocess.run([str(BINARY),'worker-join','--config',str(worker)],input=invitation['bootstrap_key'],text=True,capture_output=True,check=True)
   assert json.loads(third.stdout)['allow_scripts'];new_token=Proxy.tokens[-1]
   # Revoked token was used only to discover revocation. Recovery/readback uses
   # the new credential after rejoin and must differ from the old bearer.
   subprocess.run([str(BINARY),'worker-join','--config',str(worker)],input='',text=True,capture_output=True,check=True)
   new_token=Proxy.tokens[-1];assert new_token!=token
   api('/api/workers/edge-new/claim',{'protocol':1,'allow_scripts':True},token,401)
   api('/api/admin/items',{'name':'Allowed Lua','worker':'edge-new','kind':'lua','target':'return true'})
   print('PASS: one-use enrollment, lost-response recovery, private encrypted local credential, hash-only central state, permission ceiling, revocation and fresh re-enrollment credential')
  finally:
   proxy.shutdown();proxy.server_close();server.terminate();server.wait(timeout=20);logfile.close()
if __name__=='__main__':main()
