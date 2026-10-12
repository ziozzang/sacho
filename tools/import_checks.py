#!/usr/bin/env python3
"""Register/update real example checks without embedding any credentials."""
import argparse,json,pathlib,urllib.request
ROOT=pathlib.Path(__file__).resolve().parents[1]
def main():
 p=argparse.ArgumentParser();p.add_argument('--url',default='http://127.0.0.1:8080');p.add_argument('--admin-email');p.add_argument('--usage',action='store_true');p.add_argument('--tps',action='store_true');args=p.parse_args()
 def api(path,method='GET',body=None):
  headers={'Content-Type':'application/json','X-Sacho-Request':'1'}
  if args.admin_email:headers['X-Forwarded-Email']=args.admin_email
  request=urllib.request.Request(args.url.rstrip('/')+path,headers=headers,method=method,data=None if body is None else json.dumps(body).encode())
  with urllib.request.urlopen(request) as res:return json.load(res)
 examples=[('Hangang → Dorang','hangang.lua','Infrastructure','/value',60),('Dorang health','dorang.lua','Infrastructure','/value',60)]
 if args.usage:examples += [('Codex allowance','codex-usage.lua','Subscription usage','/display',300),('Claude allowance','claude-usage.lua','Subscription usage','/display',300)]
 if args.tps:examples += [('Dorang TPS','dorang-tps.lua','Latest information','/tps',60)]
 existing={item['spec']['name']:item for item in api('/api/admin/items')}
 for name,file,group,pointer,interval in examples:
  item=existing.get(name);spec=dict(item['spec']) if item else {}
  spec.update(name=name,kind='lua',target=(ROOT/'examples/lua'/file).read_text(),group=group,interval_seconds=interval,timeout_seconds=30,publish_value=True,display_pointer=pointer,metric_unit=' TPS' if file=='dorang-tps.lua' else '',threshold=None)
  path='/api/admin/items/'+item['id'] if item else '/api/admin/items';api(path,'PUT' if item else 'POST',spec)
  print('Registered:',name)
if __name__=='__main__':main()
