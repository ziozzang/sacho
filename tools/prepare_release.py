#!/usr/bin/env python3
"""Prepare raw binary assets and manifests signed by the native Sacho executable."""
import argparse,hashlib,json,os,pathlib,re,shutil,subprocess,tempfile

def main():
 p=argparse.ArgumentParser();p.add_argument('--binary',type=pathlib.Path,required=True);p.add_argument('--target',required=True);p.add_argument('--version',required=True);p.add_argument('--signer',type=pathlib.Path);p.add_argument('--output',type=pathlib.Path,default=pathlib.Path('dist'));args=p.parse_args()
 if not re.fullmatch(r'\d+\.\d+\.\d+',args.version):p.error('version must be stable semver without v prefix')
 if not re.fullmatch(r'[A-Za-z0-9_-]+',args.target):p.error('invalid target')
 args.output.mkdir(parents=True,exist_ok=True);name='sacho-'+args.target;dest=args.output/name
 shutil.copy2(args.binary,dest);dest.chmod(0o755);data=dest.read_bytes()
 payload={'version':args.version,'target':args.target,'artifact_url':f'https://github.com/ziozzang/sacho/releases/download/v{args.version}/{name}','sha256':hashlib.sha256(data).hexdigest(),'size':len(data)}
 if args.signer:
  if not os.environ.get('SACHO_RELEASE_SIGNING_KEY'):p.error('release signing key missing')
  with tempfile.TemporaryDirectory() as tmp:
   file=pathlib.Path(tmp)/'payload.json';file.write_text(json.dumps(payload,separators=(',',':')))
   subprocess.run([str(args.signer.resolve()),'sign-release','--payload',str(file),'--output',str(args.output/(name+'.manifest.json'))],check=True)
 else:
  (args.output/(name+'.unsigned.json')).write_text(json.dumps(payload,indent=2)+'\n')
 checksum=args.output/(name+'.sha256');checksum.write_text(payload['sha256']+'  '+name+'\n')
 print('Prepared',name,'signed' if args.signer else 'unsigned qualification only')
if __name__=='__main__':main()
