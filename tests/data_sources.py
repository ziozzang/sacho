#!/usr/bin/env python3
"""Source examples through actual Lua runner; optional owned PostgreSQL container."""
import argparse,json,pathlib,subprocess,tempfile,threading,urllib.parse,urllib.request,os,uuid,time
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from smoke import BINARY,ROOT,free_port,wait_for
class Prometheus(BaseHTTPRequestHandler):
 mode='vector';query=None;authorized=False
 def log_message(self,*args):pass
 def do_GET(self):
  Prometheus.query=urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)['query'][0]
  Prometheus.authorized=self.headers.get('Authorization')=='Bearer encrypted-fixture-token'
  sample=[time.time(),'42.5'];series={'metric':{'job':'fixture'},'value':sample}
  if self.mode=='scalar':data={'resultType':'scalar','result':sample}
  elif self.mode=='empty':data={'resultType':'vector','result':[]}
  elif self.mode=='multiple':data={'resultType':'vector','result':[series,series]}
  elif self.mode=='nan':data={'resultType':'scalar','result':[time.time(),'NaN']}
  elif self.mode=='histogram':data={'resultType':'vector','result':[{'metric':{},'histogram':{}}]}
  else:data={'resultType':'vector','result':[series]}
  body=json.dumps({'status':'error' if self.mode=='error' else 'success','data':data}).encode()
  self.send_response(422 if self.mode=='error' else 200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--postgres',action='store_true');args=parser.parse_args()
 fixture=ThreadingHTTPServer(('127.0.0.1',0),Prometheus);threading.Thread(target=fixture.serve_forever,daemon=True).start();container=None
 try:
  with tempfile.TemporaryDirectory() as temporary:
   directory=pathlib.Path(temporary);vault=directory/'vault.enc';key=directory/'key';query='sum(rate(requests_total{path="/a?x=1&y=2"}[5m]))'
   subprocess.run([str(BINARY),'secrets','init','--vault',str(vault),'--key-file',str(key)],check=True)
   values={'PROMETHEUS_URL':f'http://127.0.0.1:{fixture.server_port}','PROMETHEUS_QUERY':query,'PROMETHEUS_BEARER':'Bearer encrypted-fixture-token','UNALLOWED_SECRET':'must-not-be-visible'}
   allowed=['PROMETHEUS_URL','PROMETHEUS_QUERY','PROMETHEUS_BEARER']
   if args.postgres:
    container='sacho-source-test-'+uuid.uuid4().hex[:12];port=free_port()
    subprocess.run(['docker','run','-d','--rm','--name',container,'--tmpfs','/var/lib/postgresql/data:rw','-p',f'127.0.0.1:{port}:5432','-e','POSTGRES_PASSWORD=fixture-only-password','postgres:17'],check=True,stdout=subprocess.DEVNULL)
    wait_for(lambda:subprocess.run(['psql','-X','-w','-h','127.0.0.1','-p',str(port),'-U','postgres','-c','SELECT 1'],env={**os.environ,'PGPASSWORD':'fixture-only-password','PGCONNECT_TIMEOUT':'2'},capture_output=True).returncode==0,timeout=45)
    subprocess.run(['docker','exec',container,'psql','-U','postgres','-c','CREATE TABLE readonly_fixture(value integer)'],check=True,stdout=subprocess.DEVNULL)
    pg={'PGHOST':'127.0.0.1','PGPORT':str(port),'PGDATABASE':'postgres','PGUSER':'postgres','PGPASSWORD':'fixture-only-password','PGCONNECT_TIMEOUT':'5','PGSQL_QUERY':"SELECT json_build_object('value',1,'version',version())"}
    values.update(pg);allowed+=list(pg)
   for name,value in values.items():
    subprocess.run([str(BINARY),'secrets','set',name,'--vault',str(vault),'--key-file',str(key)],input=value,text=True,check=True)
   assert b'Bearer encrypted-fixture-token' not in vault.read_bytes()
   port=free_port();base=f'http://127.0.0.1:{port}';cfg=directory/'sacho.toml';cfg.write_text(f'bind="127.0.0.1:{port}"\ndatabase="{directory}/data.db"\npublic_url="{base}"\nprobe_env={json.dumps(allowed)}\n[secrets]\nvault="{vault}"\nkey_file="{key}"\n')
   logfile=(directory/'server.log').open('w');server=subprocess.Popen([str(BINARY),'serve','--config',str(cfg)],stdout=logfile,stderr=logfile,env={k:v for k,v in os.environ.items() if k not in values})
   def request(path,body=None):
    req=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json','X-Sacho-Request':'1'})
    with urllib.request.urlopen(req,timeout=20) as response:return json.load(response)
   def test(source,pointer='/value'):
    return request('/api/admin/test',{'name':'Data source','kind':'lua','target':source,'timeout_seconds':10,'interval_seconds':60,'publish_value':True,'display_pointer':pointer})
   try:
    wait_for(lambda:request('/healthz'))
    source=(ROOT/'examples/lua/prometheus.lua').read_text()
    for mode in ('vector','scalar','empty','multiple','nan','histogram','error'):
     Prometheus.mode=mode;result=test(source)
     if mode in ('vector','scalar'):assert result['status']=='operational' and result['display_value']=='42.5',result
     else:assert result['status']=='outage',result
     assert Prometheus.query==query and Prometheus.authorized
    blocked=test('return sacho.env("UNALLOWED_SECRET")');assert blocked['status']=='outage';assert 'must-not-be-visible' not in json.dumps(blocked)
    # Native HTTP checks resolve allowed header names from the same vault.
    Prometheus.mode='vector';result=request('/api/admin/test',{'name':'Header input','kind':'http','target':values['PROMETHEUS_URL']+'/api/v1/query?query=up','headers_env':{'Authorization':'PROMETHEUS_BEARER'}});assert result['status']=='operational' and Prometheus.authorized
    if args.postgres:
     source=(ROOT/'examples/lua/postgresql.lua').read_text();result=test(source);assert result['status']=='operational' and result['display_value']=='1',result
     assert 'PostgreSQL' in test(source,'/version')['display_value']
     for sql in ['INSERT INTO readonly_fixture VALUES (1)',"SELECT json_build_object('value',pg_sleep(10))",'SELECT 1; SELECT 2', "SELECT '[1,2]'::json", "SELECT json_build_object('value',null)"]:
      invalid=source.replace('local query = sacho.env("PGSQL_QUERY")','local query = '+json.dumps(sql));assert test(invalid)['status']=='outage'
     print('PASS: actual PostgreSQL JSON/version query, injected encrypted password, read-only rejection, SQL timeout and invalid shape rejection')
    assert request('/api/incidents')==[]
    print('PASS: Prometheus scalar/vector, URL encoding, encrypted auth injection, empty/multiple/NaN/histogram/error rejection, native HTTP injection and allowlist enforcement')
   finally:
    server.terminate();server.wait(timeout=20);logfile.close()
 finally:
  fixture.shutdown();fixture.server_close()
  if container:subprocess.run(['docker','rm','-f',container],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
if __name__=='__main__':main()
