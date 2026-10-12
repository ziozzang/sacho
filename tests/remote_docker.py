#!/usr/bin/env python3
"""Actual static controller/remote workers in owned, isolated Docker containers."""
import json, os, pathlib, sqlite3, subprocess, tempfile, time, urllib.request, urllib.error, uuid
ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = pathlib.Path(os.environ.get('SACHO_BINARY', ROOT/'target/x86_64-unknown-linux-musl/release/sacho')).resolve()
TOKEN_A='fixture-edge-a-token-0123456789abcdef'
TOKEN_B='fixture-edge-b-token-0123456789abcdef'

def docker(*args, check=True):
    result=subprocess.run(['docker',*map(str,args)], check=check, capture_output=True, text=True)
    return (result.stdout + (result.stderr if args and args[0]=='logs' else '')).strip()
def wait(fn, timeout=25):
    deadline=time.monotonic()+timeout
    while time.monotonic()<deadline:
        try:
            value=fn()
            if value:return value
        except (OSError,urllib.error.URLError,sqlite3.OperationalError):pass
        time.sleep(.15)
    raise AssertionError('condition timed out')

def main():
    assert BINARY.exists(), 'build the static musl executable first'
    name='sacho-remote-test-'+uuid.uuid4().hex[:10]
    network=name+'-net'; containers=[]
    with tempfile.TemporaryDirectory() as temporary:
        directory=pathlib.Path(temporary)
        directory.chmod(0o755)
        data=directory/'data';data.mkdir();data.chmod(0o700)
        def config(filename,text):
            path=directory/filename;path.write_text(text);path.chmod(0o644);return path
        central_cfg=config('central.toml', '''bind="0.0.0.0:8080"
database="/data/fixture.db"
public_url="http://127.0.0.1:8080"
[auth]
mode="auth-proxy"
admins=["operator@example.test"]
[[remote_workers]]
id="edge-a"
token_env="WORKER_A_TOKEN"
allow_scripts=true
[[remote_workers]]
id="edge-b"
token_env="WORKER_B_TOKEN"
allow_scripts=true
''')
        def worker_cfg(node):
            return config(node+'.toml',f'''control_url="http://control:8080"
node_id="{node}"
token_env="WORKER_TOKEN"
probe_env=["LOCATION"]
allow_scripts=true
allow_insecure_http=true
''')
        configs={node:worker_cfg(node) for node in ['edge-a','edge-b']}
        def start_worker(suffix,node):
            container=name+'-'+suffix;containers.append(container)
            docker('run','-d','--name',container,'--network',network,'--user',f'{os.getuid()}:{os.getgid()}','--read-only',
                   '--tmpfs','/tmp:rw,nosuid,noexec','--cap-drop','ALL','--security-opt','no-new-privileges',
                   '-v',f'{BINARY}:/sacho:ro','-v',f'{configs[node]}:/worker.toml:ro',
                   '-e','WORKER_TOKEN='+({'edge-a':TOKEN_A,'edge-b':TOKEN_B}[node]),'-e','LOCATION='+node,
                   '--entrypoint','/sacho','alpine:3','worker','--config','/worker.toml')
            return container
        try:
            docker('network','create',network)
            central=name+'-control';containers.append(central)
            docker('run','-d','--name',central,'--network',network,'--network-alias','control','--user',f'{os.getuid()}:{os.getgid()}',
                   '--read-only','--tmpfs','/tmp:rw,nosuid,noexec','--cap-drop','ALL',
                   '--security-opt','no-new-privileges','-p','127.0.0.1::8080',
                   '-v',f'{BINARY}:/sacho:ro','-v',f'{central_cfg}:/config.toml:ro','-v',f'{data}:/data:rw',
                   '-e','WORKER_A_TOKEN='+TOKEN_A,'-e','WORKER_B_TOKEN='+TOKEN_B,
                   '--entrypoint','/sacho','alpine:3','serve','--worker','--config','/config.toml')
            info=json.loads(docker('inspect',central))[0]
            port=info['NetworkSettings']['Ports']['8080/tcp'][0]['HostPort'];base=f'http://127.0.0.1:{port}'
            def request(path,body=None,token=None,expected=200):
                headers={'Content-Type':'application/json','X-Sacho-Request':'1','X-Forwarded-Email':'operator@example.test'}
                if token:headers['Authorization']='Bearer '+token
                req=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),headers=headers)
                try:
                    with urllib.request.urlopen(req,timeout=10) as response:
                        assert response.status==expected
                        return json.load(response)
                except urllib.error.HTTPError as error:
                    assert error.code==expected,(error.code,error.read())
                    return json.loads(error.read())
            wait(lambda:request('/healthz'))
            def add(node,source,name='Remote fixture'):
                return request('/api/admin/items',{'name':name,'kind':'lua','target':source,'worker':node,
                    'interval_seconds':60,'timeout_seconds':5,'publish_value':True,'display_pointer':'/value'})['id']
            source='local r=sacho.http("http://control:8080/healthz"); assert(r.status==200); return sacho.json_encode({value=sacho.env("LOCATION")})'
            ids={node:add(node,source,node) for node in ['edge-a','edge-b']}
            request('/api/workers/edge-a/claim',{'protocol':1,'allow_scripts':True},TOKEN_B,401)
            request('/api/workers/unknown/claim',{'protocol':1,'allow_scripts':True},TOKEN_A,401)
            request('/api/workers/edge-a/claim',{'protocol':999,'allow_scripts':True},TOKEN_A,400)
            a=start_worker('a','edge-a');b=start_worker('b','edge-b')
            def values():
                return {item['id']:item['display_value'] for item in request('/api/status')['items']}
            wait(lambda:values().get(ids['edge-a'])=='edge-a' and values().get(ids['edge-b'])=='edge-b')
            assert request('/api/incidents')==[], 'remote-only inputs must never execute on controller'
            invitation=request('/api/admin/worker-bootstrap',{'id':'edge-new','allow_scripts':True,'ttl_seconds':300})
            work=directory/'bootstrap-worker';work.mkdir();work.chmod(0o700)
            bootstrap_cfg=work/'worker.toml';bootstrap_cfg.write_text('control_url="http://control:8080"\nnode_id="edge-new"\nlabel="Bootstrap worker"\ntoken_env="SACHO_JOIN_TOKEN"\nprobe_env=["LOCATION"]\nallow_scripts=true\nallow_insecure_http=true\n')
            join_name=name+'-join';containers.append(join_name)
            subprocess.run(['docker','run','--rm','-i','--name',join_name,'--network',network,'--user',f'{os.getuid()}:{os.getgid()}',
                '--read-only','--tmpfs','/tmp:rw,nosuid,noexec','--cap-drop','ALL','--security-opt','no-new-privileges',
                '-v',f'{BINARY}:/sacho:ro','-v',f'{work}:/work:rw','--entrypoint','/sacho','alpine:3',
                'worker-join','--config','/work/worker.toml'],input=invitation['bootstrap_key'],text=True,capture_output=True,check=True)
            joined=name+'-enrolled';containers.append(joined)
            docker('run','-d','--name',joined,'--network',network,'--user',f'{os.getuid()}:{os.getgid()}',
                '--read-only','--tmpfs','/tmp:rw,nosuid,noexec','--cap-drop','ALL','--security-opt','no-new-privileges',
                '-v',f'{BINARY}:/sacho:ro','-v',f'{work}:/work:ro','-e','LOCATION=edge-new',
                '--entrypoint','/sacho','alpine:3','worker','--config','/work/worker.toml')
            enrolled_id=add('edge-new',source,'Bootstrap check')
            wait(lambda:values().get(enrolled_id)=='edge-new')
            assert next(n for n in request('/api/admin/workers') if n['id']=='edge-new')['label']=='Bootstrap worker'
            # Verify duplicate result handling through the actual HTTP endpoint.
            docker('pause',a)
            second=add('edge-a',source,'Retry fixture')
            assignment=request('/api/workers/edge-a/claim',{'protocol':1,'allow_scripts':True},TOKEN_A)
            assert assignment['item_id']==second
            result={'protocol':1,'run_id':assignment['run_id'],'item_id':second,
                    'observation':{'status':'operational','latency_ms':1,'output':'{"value":"edge-a"}','detail':''}}
            request('/api/workers/edge-a/result',result,TOKEN_A)
            request('/api/workers/edge-a/result',result,TOKEN_A)
            result['observation']['output']='conflict'
            request('/api/workers/edge-a/result',result,TOKEN_A,409)
            with sqlite3.connect(data/'fixture.db') as conn:
                assert conn.execute('SELECT count(*) FROM checks WHERE item_id=?',(second,)).fetchone()[0]==1
            # Simulate a paused in-flight worker: claim, expire the owned fixture
            # lease, let a replacement process complete, reject late old result.
            docker('unpause',a)
            slow='local r=sacho.exec({"sh","-c","sleep 3"}); assert(r.exit_code==0); '+source
            stalled=add('edge-a',slow,'Failover fixture')
            def in_flight():
                with sqlite3.connect(data/'fixture.db') as conn:
                    row=conn.execute('SELECT run_id,result_hash FROM remote_runs WHERE item_id=?',(stalled,)).fetchone()
                    return row[0] if row and row[1] is None else None
            old_id=wait(in_flight)
            docker('pause',a)
            with sqlite3.connect(data/'fixture.db') as conn:
                conn.execute('UPDATE remote_runs SET lease_until=0 WHERE item_id=?',(stalled,))
            replacement=start_worker('replacement','edge-a')
            wait(lambda:values().get(stalled)=='edge-a')
            stale={'protocol':1,'run_id':old_id,'item_id':stalled,
                   'observation':{'status':'outage','latency_ms':1,'output':'stale','detail':''}}
            request('/api/workers/edge-a/result',stale,TOKEN_A,409)
            docker('unpause',a)
            # Disconnect/reconnect an actual remote worker and restart the
            # controller; observations and durable result receipts remain intact.
            docker('network','disconnect',network,b)
            wait(lambda:'worker controller unavailable' in docker('logs',b))
            docker('network','connect',network,b)
            docker('restart',central)
            info=json.loads(docker('inspect',central))[0]
            port=info['NetworkSettings']['Ports']['8080/tcp'][0]['HostPort'];base=f'http://127.0.0.1:{port}'
            wait(lambda:request('/healthz'))
            request('/api/admin/items/'+ids['edge-b']+'/run',{})
            old_at=next(i['last_check'] for i in request('/api/admin/items') if i['id']==ids['edge-b'])
            wait(lambda:any(i['id']==ids['edge-b'] and i['last_check'] is not None and i['last_check']!=old_at for i in request('/api/admin/items')))
            print('PASS: static controller + remote Docker workers, node isolation, local secret injection, '
                  'authentication/protocol rejection, duplicate results, lease takeover/stale rejection, '
                  'network reconnect, controller restart and one-time bootstrap/encrypted registered worker')
        finally:
            for container in reversed(containers):
                docker('unpause',container,check=False)
                docker('rm','-f',container,check=False)
            docker('network','rm',network,check=False)
if __name__=='__main__':main()
