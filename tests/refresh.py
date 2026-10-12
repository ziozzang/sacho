#!/usr/bin/env python3
"""Browser refresh survives clock skew, tab/dialog pauses and fetch failures."""
import datetime,json,pathlib,subprocess,tempfile,urllib.request
from playwright.sync_api import sync_playwright,expect
from smoke import BINARY,free_port,wait_for

def main():
 with tempfile.TemporaryDirectory() as temporary:
  directory=pathlib.Path(temporary);port=free_port();base=f'http://127.0.0.1:{port}'
  cfg=directory/'sacho.toml';cfg.write_text(f'bind="127.0.0.1:{port}"\ndatabase="{directory}/data.db"\npublic_url="{base}"\n')
  log=(directory/'server.log').open('w');server=subprocess.Popen([str(BINARY),'serve','--config',str(cfg)],stdout=log,stderr=log)
  def api(path,body=None):
   request=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json','X-Sacho-Request':'1'})
   with urllib.request.urlopen(request) as response:return json.load(response)
  try:
   wait_for(lambda:api('/healthz'));api('/api/admin/items',{'name':'Refresh fixture','kind':'lua','target':'return "1"','interval_seconds':5,'timeout_seconds':3,'publish_value':True});wait_for(lambda:api('/api/status')['status']=='operational')
   with sync_playwright() as p:
    browser=p.chromium.launch();page=browser.new_page();page.add_init_script("localStorage.setItem('sacho-language','en')")
    # Client wall clock is decades behind server; timers must use performance.now.
    page.clock.install(time=datetime.datetime(2001,1,1,tzinfo=datetime.timezone.utc))
    state={'value':'41','unknown':False,'failed':False,'requests':0}
    def status(route):
     state['requests']+=1
     if state['failed']:route.fulfill(status=503,json={'error':'fixture unavailable'});return
     data=route.fetch().json();data['items'][0]['display_value']=state['value'];data['status']='unknown' if state['unknown'] else 'operational';route.fulfill(json=data)
    page.route('**/api/status',status);page.goto(base);expect(page.locator('.metric')).to_have_text('41');page.locator('.bar').first.wait_for()
    page.locator('#range').select_option('3600');page.locator('.bar').first.wait_for();page.locator('#precision').select_option('5');page.locator('.bar').first.wait_for()
    state['value']='42';page.clock.fast_forward(35000);expect(page.locator('.metric')).to_have_text('42');expect(page.locator('#range')).to_have_value('3600');expect(page.locator('#precision')).to_have_value('5')
    # Do not refetch while a dialog is open.
    page.locator('#subscribe-dialog').evaluate('(dialog)=>dialog.showModal()');count=state['requests'];page.clock.fast_forward(35000);assert state['requests']==count;page.locator('#subscribe-dialog').evaluate('(dialog)=>dialog.close()')
    # Hidden tab pauses; visibility restoration refreshes without timer advancement.
    page.evaluate("Object.defineProperty(document,'hidden',{configurable:true,value:true});document.dispatchEvent(new Event('visibilitychange'))")
    count=state['requests'];page.clock.fast_forward(35000);assert state['requests']==count
    state['value']='43';page.evaluate("Object.defineProperty(document,'hidden',{configurable:true,value:false});document.dispatchEvent(new Event('visibilitychange'))");expect(page.locator('.metric')).to_have_text('43')
    # Failed refresh keeps the last data and one warning; recovery retries.
    state['failed']=True;page.clock.fast_forward(35000);expect(page.locator('.refresh-error')).to_be_visible();expect(page.locator('.metric')).to_have_text('43')
    page.clock.fast_forward(5000);expect(page.locator('.refresh-error')).to_have_count(1)
    state['failed']=False;state['value']='44';page.clock.fast_forward(5000);expect(page.locator('.metric')).to_have_text('44');expect(page.locator('.refresh-error')).to_have_count(0)
    state['unknown']=True;page.evaluate("window.dispatchEvent(new Event('online'))");expect(page.get_by_role('heading',name='Waiting for service observations')).to_be_visible()
    state['value']='45';page.clock.fast_forward(5000);expect(page.locator('.metric')).to_have_text('45')
    state['unknown']=False;state['value']='46';page.evaluate("window.dispatchEvent(new Event('online'))");expect(page.locator('.metric')).to_have_text('46')
    browser.close();print('PASS: 30s/5s reload, wall-clock skew, period/gap preservation, dialog/hidden pause, immediate tab/network restoration, stale-data notice and retry')
  finally:
   server.terminate();server.wait(timeout=20);log.close()
if __name__=='__main__':main()
