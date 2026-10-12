#!/usr/bin/env python3
"""Request-by-request visibility enforcement and persistent graph preferences."""
import json,pathlib,sqlite3,subprocess,tempfile,shutil,time,urllib.request,urllib.error
from smoke import BINARY,free_port,wait_for
from playwright.sync_api import sync_playwright,expect

def main():
 with tempfile.TemporaryDirectory() as tmp:
  root=pathlib.Path(tmp);binary=root/'sacho';shutil.copy2(BINARY,binary);port=free_port();base=f'http://127.0.0.1:{port}';cfg=root/'cfg';cfg.write_text(f'bind="127.0.0.1:{port}"\ndatabase="{root}/db"\npublic_url="{base}"\n[auth]\nmode="auth-proxy"\nemail_header="X-Fixture-Identity"\nadmins=["admin@example.com"]\n')
  log=open(root/'log','w');server=subprocess.Popen([str(binary),'serve','--config',str(cfg)],stdout=log,stderr=log)
  def request(path,body=None,method=None,email=None):
   headers={'Content-Type':'application/json','X-Sacho-Request':'1'}
   if email:headers['X-Fixture-Identity']=email
   req=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),method=method,headers=headers)
   with urllib.request.urlopen(req,timeout=15) as r:return json.load(r)
  def forbidden(path,email=None):
   try:request(path,email=email);raise AssertionError('restricted endpoint leaked')
   except urllib.error.HTTPError as e:assert e.code==404
  def ids(email=None):return {i['id'] for i in request('/api/status',email=email)['items']}
  try:
   wait_for(lambda:request('/healthz'))
   groups=[{'name':'operations','emails':[' Ops@Example.com ']}];request('/api/admin/access-groups',groups,'PUT','admin@example.com')
   base_spec=dict(kind='lua',target='return sacho.json_encode({value=42})',interval_seconds=60,initial_delay_seconds=86400,timeout_seconds=3,publish_value=True,display_pointer='/value',publish_trend=True,trend_pointer='/value')
   monitors={}
   for mode,name in [('public','Public fixture'),('authenticated','Signed-in fixture'),('groups','Restricted fixture')]:
    spec={**base_spec,'name':name,'visibility':mode,'access_groups':['operations'] if mode=='groups' else []};monitors[mode]=request('/api/admin/items',spec,email='admin@example.com')['id']
   cadence=request('/api/admin/items',{**base_spec,'name':'Cadence fixture','interval_seconds':30,'publish_trend':False},email='admin@example.com')['id']
   connection=sqlite3.connect(root/'db');now=int(time.time());at=now//30*30
   connection.execute('UPDATE items SET status=?,last_check=? WHERE id=?',('operational',now,cadence))
   for mode,ident in monitors.items():
    connection.execute('UPDATE items SET status=?,last_check=? WHERE id=?',('outage' if mode=='groups' else 'operational',now,ident))
    # One absent 30s bucket between expected 60s observations is not a missed check.
    for offset,value in [(300,10),(240,20),(180,15),(60,30)]:connection.execute('INSERT INTO checks(item_id,at,status,latency_ms,output,detail,metric_value,display_value) VALUES(?,?,?,?,?,?,?,?)',(ident,at-offset,'operational',value,'{"value":42}','',value,str(value)))
    connection.execute('INSERT INTO incidents(id,item_id,name,status,opened_at,detail) VALUES(?,?,?,?,?,?)',(mode,ident,name if mode!='groups' else 'PRIVATE_INCIDENT_SENTINEL','outage',now,'private execution detail'))
   connection.execute("INSERT INTO settings(key,value) VALUES('layout',?)",[json.dumps([{'title':'Public section','item_ids':[monitors['public']]},{'title':'PRIVATE_SECTION_SENTINEL','item_ids':[monitors['groups']]}])]);connection.commit()
   assert ids()=={monitors['public'],cadence};assert ids('reader@example.com')=={monitors['public'],monitors['authenticated'],cadence};assert ids('ops@example.com')==set(monitors.values())|{cadence};assert ids('admin@example.com')==set(monitors.values())|{cadence}
   anonymous=json.dumps(request('/api/status'));assert 'PRIVATE_SECTION_SENTINEL' not in anonymous and monitors['groups'] not in anonymous and request('/api/status')['status']=='operational'
   assert request('/api/status',email='ops@example.com')['status']=='outage'
   for email in [None,'reader@example.com']:forbidden('/api/history/'+monitors['groups'],email);assert 'PRIVATE_INCIDENT_SENTINEL' not in json.dumps(request('/api/incidents',email=email))
   assert len(request('/api/incidents',email='ops@example.com'))==3
   assert request('/api/history/'+monitors['groups'],email='ops@example.com')['buckets']
   # Membership revocation affects existing callers without logout or server restart.
   request('/api/admin/access-groups',[{'name':'operations','emails':[]}],'PUT','admin@example.com');forbidden('/api/history/'+monitors['groups'],'ops@example.com');assert monitors['groups'] not in ids('ops@example.com')
   request('/api/admin/access-groups',groups,'PUT','admin@example.com')
   try:request('/api/admin/access-groups',email='ops@example.com');raise AssertionError('viewer became admin')
   except urllib.error.HTTPError as e:assert e.code==401
   # A pending formerly public email is rechecked against the current grade.
   connection.execute('INSERT INTO subscribers(id,email,token,verified,created_at) VALUES(?,?,?,?,?)',('sub','subscriber@example.com','fixture-token',1,now));connection.commit()
   connection.execute('UPDATE items SET status=? WHERE id=?',('operational',monitors['groups']));connection.commit()
   hidden_spec={**base_spec,'name':'Restricted fixture','visibility':'groups','access_groups':['operations'],'target':'return false','initial_delay_seconds':0};request('/api/admin/items/'+monitors['groups'],hidden_spec,'PUT','admin@example.com');request('/api/admin/items/'+monitors['groups']+'/run',{},'POST','admin@example.com')
   wait_for(lambda:request('/api/admin/items/'+monitors['groups']+'/logs',email='admin@example.com')[0]['status']=='outage')
   assert connection.execute("SELECT COUNT(*) FROM outbox WHERE kind='email' AND json_extract(payload,'$.item_id')=?",[monitors['groups']]).fetchone()[0]==0
   with sync_playwright() as p:
    browser=p.chromium.launch();page=browser.new_page();page.add_init_script("localStorage.setItem('sacho-language','en')");page.goto(base);expect(page.locator('#range')).to_have_value('3600');expect(page.locator('.trend-svg path')).to_have_count(1);path=page.locator('.trend-svg path').get_attribute('d');assert 'C' in path and path.count('M')==2,path
    page.locator('#range').select_option('21600');page.locator('#precision').select_option('30');page.locator('[data-graph-curve]').select_option('straight');page.locator('.trend-panel summary').click();page.reload();expect(page.locator('#range')).to_have_value('21600');expect(page.locator('#precision')).to_have_value('30');expect(page.locator('[data-graph-curve]')).to_have_value('straight');expect(page.locator('.trend-panel')).not_to_have_attribute('open','');page.locator('#remember-view').uncheck();page.reload();expect(page.locator('#range')).to_have_value('3600');expect(page.locator('#remember-view')).not_to_be_checked()
    page.locator('#remember-view').check();site=request('/api/admin/settings',email='admin@example.com')['site'];request('/api/admin/settings',{'title':site['title'],'description':site['description'],'view_preferences':'signed_in'},'PUT','admin@example.com');page.reload();expect(page.locator('#remember-view')).to_be_disabled()
    page.set_extra_http_headers({'X-Fixture-Identity':'ops@example.com'});page.reload();expect(page.locator('#remember-view')).to_be_enabled();expect(page.locator('.service')).to_have_count(4);page.locator('#range').select_option('86400');page.reload();expect(page.locator('#range')).to_have_value('86400')
    page.set_extra_http_headers({'X-Fixture-Identity':'reader@example.com'});page.reload();expect(page.locator('#range')).to_have_value('3600');expect(page.locator('.service')).to_have_count(3)
    page.set_extra_http_headers({'X-Fixture-Identity':'admin@example.com'});page.goto(base+'/admin');page.get_by_role('button',name='Settings',exact=True).click();expect(page.locator('#access-group-form')).to_be_visible();expect(page.locator('.access-group-name')).to_have_value('operations');expect(page.locator('[name=view_preferences]')).to_have_value('signed_in');page.get_by_role('button',name='Add group',exact=True).click();page.locator('.access-group-name').last.fill('customers');page.locator('.access-group-emails').last.fill('customer@example.com');page.get_by_role('button',name='Save access groups',exact=True).click();expect(page.locator('.access-group-name')).to_have_count(2)
    page.get_by_role('button',name='Monitors',exact=True).click();page.locator(f'[data-action=edit][data-id="{monitors["groups"]}"]').click();expect(page.locator('[name=visibility]')).to_have_value('groups');expect(page.locator('[name=access_groups]')).to_have_values(['operations']);page.locator('[data-close=edit-dialog]').click();browser.close()
   connection.close();print('PASS: visibility matrix, request-level history/incidents/layout/aggregate enforcement, immediate membership revocation, notification isolation, interval-aware curves and per-identity view preferences/policy/UI')
  finally:server.terminate();server.wait(timeout=20);log.close()
if __name__=='__main__':main()
