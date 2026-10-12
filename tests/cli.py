#!/usr/bin/env python3
"""Embedded docs and encrypted inputs work without a checkout or running service."""
import pathlib,subprocess,tempfile,os,json
from smoke import BINARY,ROOT

def main():
 with tempfile.TemporaryDirectory() as temporary:
  directory=pathlib.Path(temporary)
  def run(*args,input=None,ok=True):
   result=subprocess.run([str(BINARY),*args],input=input,text=True,capture_output=True,cwd=directory)
   assert (result.returncode==0)==ok,(args,result.stderr)
   return result
  for language,suffix in [('en',''),('ko','.ko')]:
   expected=(ROOT/f'docs/SETUP-HOWTO{suffix}.md').read_text()
   assert run('help','setup','--language',language).stdout==expected
   assert run('setup-howto','--language',language).stdout==expected
  for language,suffix in [('en',''),('ko','.ko')]:
   assert run('help','monitoring','--language',language).stdout==(ROOT/f'docs/MONITORING{suffix}.md').read_text()
  assert 'Run the status page' in run('help','serve').stdout
  run('help','nonexistent',ok=False);run('help','setup','--language','invalid',ok=False)
  for name in ('prometheus','postgresql','external-command','docker'):
   assert run('example',name).stdout==(ROOT/f'examples/lua/{name}.lua').read_text()
  for name,path in [('shell','shell/basic.sh'),('python','python/basic.py'),('lua','lua/basic.lua')]:
   assert run('example',name).stdout==(ROOT/'examples'/path).read_text()
  for name,path in [('ping','native/ping.json'),('snmp','native/snmp.json'),('shell-disk','shell/disk-usage.sh'),('shell-process','shell/process-count.sh'),('shell-http','shell/http-auth.sh')]:
   assert run('template',name).stdout==(ROOT/'examples'/path).read_text()
  assert not (directory/'sacho.db').exists()
  vault=directory/'vault.enc';key=directory/'unlock.key';paths=['--vault',str(vault),'--key-file',str(key)]
  run('secrets','init',*paths);run('secrets','init',*paths,ok=False)
  sentinel='cli-secret-sentinel-never-on-disk';run('secrets','set','SOURCE_BEARER',*paths,input=sentinel+'\n')
  listed=run('secrets','list',*paths);assert listed.stdout=='SOURCE_BEARER\n' and sentinel not in listed.stderr
  assert sentinel.encode() not in vault.read_bytes() and sentinel.encode() not in key.read_bytes()
  assert vault.stat().st_mode&0o077==0 and key.stat().st_mode&0o077==0
  run('secrets','remove','SOURCE_BEARER',*paths);assert run('secrets','list',*paths).stdout==''
  run('secrets','set','PATH',*paths,input='forbidden',ok=False)
  print('PASS: standalone English/Korean setup help, preserved CLI help, exported Lua examples, encrypted stdin-only secrets, private files and names-only listing')
if __name__=='__main__':main()
