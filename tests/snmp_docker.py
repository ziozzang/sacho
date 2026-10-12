#!/usr/bin/env python3
"""Native SNMPv2c/v3 GET against an owned Net-SNMP daemon container."""
import json,pathlib,shutil,subprocess,tempfile,time,urllib.request,uuid
from smoke import BINARY,free_port,wait_for

def docker(*args):return subprocess.check_output(['docker',*map(str,args)],text=True).strip()
def main():
 name='sacho-snmp-'+uuid.uuid4().hex[:10];image=name+':test';container=False
 with tempfile.TemporaryDirectory() as tmp:
  root=pathlib.Path(tmp);(root/'Dockerfile').write_text('FROM debian:trixie-slim\nRUN apt-get update && apt-get install -y --no-install-recommends snmpd && rm -rf /var/lib/apt/lists/* && chmod 777 /var/lib/snmp\nCOPY snmpd.conf /etc/sacho-snmpd.conf\nENTRYPOINT ["snmpd","-f","-Lo","-C","-c","/etc/sacho-snmpd.conf","-p","/tmp/snmpd.pid"]\n')
  (root/'snmpd.conf').write_text('agentAddress udp:0.0.0.0:1161\nrocommunity fixture-public\ncreateUser fixture-user SHA-256 "fixture-auth-password" AES "fixture-privacy-password"\nrouser fixture-user priv\ncreateUser fixture-noauth\nrouser fixture-noauth noauth\ncreateUser fixture-auth SHA "fixture-auth-only-password"\nrouser fixture-auth auth\nsysLocation Test fixture\nsysContact operator@example.test\n')
  binary=root/'sacho';shutil.copy2(BINARY,binary);vault=root/'vault';key=root/'key';subprocess.run([str(binary),'secrets','init','--vault',str(vault),'--key-file',str(key)],capture_output=True,check=True)
  port=free_port();base=f'http://127.0.0.1:{port}';cfg=root/'cfg';cfg.write_text(f'bind="127.0.0.1:{port}"\ndatabase="{root}/db"\npublic_url="{base}"\n[secrets]\nvault="{vault}"\nkey_file="{key}"\n');log=open(root/'server.log','w');server=None
  def request(body):
   r=urllib.request.Request(base+'/api/admin/test',data=json.dumps(body).encode(),headers={'Content-Type':'application/json','X-Sacho-Request':'1'})
   with urllib.request.urlopen(r,timeout=15) as response:return json.load(response)
  try:
   build=subprocess.run(['docker','build','-t',image,str(root)],capture_output=True,text=True);assert build.returncode==0,build.stderr[-1800:]
   docker('run','-d','--name',name,'--cap-drop','ALL','--security-opt','no-new-privileges','-p','127.0.0.1::1161/udp',image);container=True
   mapped=json.loads(docker('inspect',name))[0]['NetworkSettings']['Ports']['1161/udp'][0]['HostPort'];time.sleep(1)
   server=subprocess.Popen([str(binary),'serve','--config',str(cfg)],stdout=log,stderr=log)
   wait_for(lambda:urllib.request.urlopen(base+'/healthz').status==200)
   spec={'name':'Owned SNMP probe','kind':'snmp','target':'127.0.0.1:'+mapped,'interval_seconds':10,'timeout_seconds':5,'publish_value':True,'display_pointer':'/value','snmp':{'version':'2c','oid':'1.3.6.1.2.1.1.3.0','community_env':'COMMUNITY'},'input_updates':[{'name':'COMMUNITY','value':'fixture-public','encrypted':True}]}
   result=request(spec);assert result['status']=='operational',(result,docker('logs',name)[-1800:]);parsed=json.loads(result['output']);assert parsed['type']=='timeticks' and isinstance(parsed['value'],int)
   strings={**spec,'snmp':{**spec['snmp'],'oid':'1.3.6.1.2.1.1.6.0'}};result=request(strings);assert result['status']=='operational' and json.loads(result['output'])['value']=='Test fixture',result
   v3={**spec,'snmp':{'version':'3','oid':'1.3.6.1.2.1.1.3.0','username':'fixture-user','security':'authPriv','auth_protocol':'sha256','auth_env':'AUTH','privacy_env':'PRIVACY'},'input_updates':[{'name':'AUTH','value':'fixture-auth-password','encrypted':True},{'name':'PRIVACY','value':'fixture-privacy-password','encrypted':True}]}
   result=request(v3);assert result['status']=='operational',(result,docker('logs',name)[-1800:]);assert json.loads(result['output'])['type']=='timeticks'
   noauth={**v3,'snmp':{**v3['snmp'],'username':'fixture-noauth','security':'noAuthNoPriv'},'input_updates':[]};assert request(noauth)['status']=='operational'
   auth={**v3,'snmp':{**v3['snmp'],'username':'fixture-auth','security':'authNoPriv','auth_protocol':'sha1'},'input_updates':[{'name':'AUTH','value':'fixture-auth-only-password','encrypted':True}]};assert request(auth)['status']=='operational'
   empty={**v3,'input_updates':[{'name':'AUTH','value':'','encrypted':True},{'name':'PRIVACY','value':'fixture-privacy-password','encrypted':True}]};assert request(empty)['status']=='outage'
   missing={**spec,'snmp':{**spec['snmp'],'oid':'1.3.6.1.2.1.99999.0'}};assert request(missing)['status']=='outage'
   wrong={**spec,'timeout_seconds':1,'input_updates':[{'name':'COMMUNITY','value':'wrong-fixture-community','encrypted':True}]};result=request(wrong);assert result['status']=='outage' and 'wrong-fixture-community' not in result['detail']
   assert not subprocess.check_output([str(binary),'secrets','list','--vault',str(vault),'--key-file',str(key)],text=True).strip(),'draft secret inputs persisted'
   print('PASS: native SNMPv2c timeticks/string, v3 SHA256/AES128 authPriv, injected credentials, absent OID, timeout and draft-secret non-persistence with an actual Net-SNMP container')
  finally:
   if server and server.poll() is None:server.terminate();server.wait(timeout=15)
   log.close()
   if container:subprocess.run(['docker','rm','-f',name],capture_output=True)
   subprocess.run(['docker','image','rm',image],capture_output=True)
if __name__=='__main__':main()
