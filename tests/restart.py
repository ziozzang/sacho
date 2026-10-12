#!/usr/bin/env python3
"""Exercise signed HTTPS automatic updates against a private release origin."""
import base64, hashlib, json, os, pathlib, shutil, ssl, subprocess, tempfile, threading, time, urllib.request
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding,PublicFormat
from smoke import BINARY,free_port,wait_for
class Origin(BaseHTTPRequestHandler):
 manifest=b'';artifact=b''
 def log_message(self,*args):pass
 def do_GET(self):
  body=self.manifest if self.path=='/manifest' else self.artifact
  self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
def candidate(fail):
 return ('''#!/usr/bin/env python3
import sys,json,pathlib,tomllib
if '--version' in sys.argv:print('sacho 9.0.0');sys.exit(0)
'''+('sys.exit(1)\n' if fail else '''from http.server import BaseHTTPRequestHandler,HTTPServer
config=tomllib.loads(pathlib.Path(sys.argv[sys.argv.index('--config')+1]).read_text())
host,port=config['bind'].split(':')
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  body=json.dumps({'status':'ok','version':'9.0.0'}).encode();self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
server=HTTPServer((host,int(port)),Handler)
pathlib.Path(sys.argv[sys.argv.index('--ready-file')+1]).write_text('9.0.0')
server.serve_forever()
''')).encode()
def run_case(fail):
 with tempfile.TemporaryDirectory() as tmp:
  tmp=pathlib.Path(tmp);binary=tmp/'sacho';shutil.copy2(BINARY,binary);initial=hashlib.sha256(binary.read_bytes()).digest()
  from cryptography import x509
  from cryptography.x509.oid import NameOID,ExtendedKeyUsageOID
  from cryptography.hazmat.primitives.asymmetric import rsa
  from cryptography.hazmat.primitives import hashes,serialization
  import datetime,ipaddress
  ca_key=rsa.generate_private_key(public_exponent=65537,key_size=2048)
  leaf_key=rsa.generate_private_key(public_exponent=65537,key_size=2048)
  name=x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,'Sacho test CA')]);now=datetime.datetime.now(datetime.timezone.utc)
  ca=x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(ca_key.public_key()).serial_number(x509.random_serial_number()).not_valid_before(now-datetime.timedelta(minutes=1)).not_valid_after(now+datetime.timedelta(days=1)).add_extension(x509.BasicConstraints(ca=True,path_length=None),True).sign(ca_key,hashes.SHA256())
  leaf=x509.CertificateBuilder().subject_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,'127.0.0.1')])).issuer_name(name).public_key(leaf_key.public_key()).serial_number(x509.random_serial_number()).not_valid_before(now-datetime.timedelta(minutes=1)).not_valid_after(now+datetime.timedelta(days=1)).add_extension(x509.BasicConstraints(ca=False,path_length=None),True).add_extension(x509.SubjectAlternativeName([x509.IPAddress(ipaddress.ip_address('127.0.0.1'))]),False).add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]),False).sign(ca_key,hashes.SHA256())
  (tmp/'ca.pem').write_bytes(ca.public_bytes(serialization.Encoding.PEM))
  (tmp/'leaf.pem').write_bytes(leaf.public_bytes(serialization.Encoding.PEM))
  (tmp/'tls.key').write_bytes(leaf_key.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.PKCS8,serialization.NoEncryption()))
  origin=ThreadingHTTPServer(('127.0.0.1',0),Origin);context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.load_cert_chain(tmp/'leaf.pem',tmp/'tls.key');origin.socket=context.wrap_socket(origin.socket,server_side=True)
  url=f'https://127.0.0.1:{origin.server_port}'
  Origin.artifact=candidate(fail);key=Ed25519PrivateKey.generate();public=base64.b64encode(key.public_key().public_bytes(Encoding.Raw,PublicFormat.Raw)).decode()
  target=os.environ.get('SACHO_TARGET',subprocess.check_output(['rustc','-vV'],text=True).split('host: ')[1].splitlines()[0])
  payload=json.dumps({'version':'9.0.0','target':target,'artifact_url':url+'/artifact','sha256':hashlib.sha256(Origin.artifact).hexdigest(),'size':len(Origin.artifact)}).encode()
  Origin.manifest=json.dumps({'payload':base64.b64encode(payload).decode(),'signature':base64.b64encode(key.sign(payload)).decode()}).encode()
  threading.Thread(target=origin.serve_forever,daemon=True).start()
  port=free_port();base=f'http://127.0.0.1:{port}';config=tmp/'sacho.toml'
  config.write_text(f'bind="127.0.0.1:{port}"\ndatabase="{tmp}/data.db"\npublic_url="{base}"\n[updates]\nenabled=true\ninterval_seconds=60\npublic_key="{public}"\nmanifest_url="{url}/manifest"\nca_file="{tmp}/ca.pem"\n')
  log=open(tmp/'server.log','w');proc=subprocess.Popen([str(binary),'serve','--config',str(config)],stdout=log,stderr=log)
  def version():
   with urllib.request.urlopen(base+'/healthz',timeout=2) as res:return json.load(res)['version']
  try:
   if fail:
    wait_for(lambda:'failed readiness; rolling back' in (tmp/'server.log').read_text(),timeout=25)
    assert wait_for(version)==subprocess.check_output([str(BINARY),'--version'],text=True).strip().split()[-1];assert hashlib.sha256(binary.read_bytes()).digest()==initial
   else:
    wait_for(lambda:version()=='9.0.0',timeout=25)
    wait_for(lambda:'automatic update healthy' in (tmp/'server.log').read_text(),timeout=40)
    assert not list(tmp.glob('.sacho-rollback-*'))
   proc.terminate();proc.wait(timeout=20)
   assert proc.returncode==0
  except Exception:
   print((tmp/'server.log').read_text(),flush=True);raise
  finally:
   if proc.poll() is None:proc.terminate();proc.wait(timeout=20)
   log.close();origin.shutdown()
 print('PASS: signed HTTPS update '+('failed readiness restores previous binary' if fail else 'restarts worker and removes rollback after healthy probation'))
if __name__=='__main__':run_case(False);run_case(True)
