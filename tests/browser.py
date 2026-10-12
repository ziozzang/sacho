#!/usr/bin/env python3
"""Browser exercise for embedded Lua editing, draft runs, latest values and layout."""
import json, pathlib, subprocess, tempfile, urllib.request, os, time, sys
sys.path.insert(0,str(pathlib.Path(__file__).parent))
from smoke import free_port,wait_for,BINARY,ROOT
from playwright.sync_api import sync_playwright, expect

def main():
 with tempfile.TemporaryDirectory() as tmp:
  tmp=pathlib.Path(tmp);port=free_port();base=f'http://127.0.0.1:{port}'
  config=tmp/'sacho.toml';config.write_text(f'bind="127.0.0.1:{port}"\ndatabase="{tmp}/sacho.db"\npublic_url="{base}"\nretention_days=90\n')
  log=open(tmp/'server.log','w');server=subprocess.Popen([str(BINARY),'serve','--config',str(config)],stdout=log,stderr=log)
  def request(path,method='GET',body=None):
   req=urllib.request.Request(base+path,method=method,headers={'Content-Type':'application/json','X-Sacho-Request':'1'},data=None if body is None else json.dumps(body).encode())
   with urllib.request.urlopen(req) as r:return json.load(r)
  try:
   wait_for(lambda:request('/healthz'))
   with sync_playwright() as p:
    browser=p.chromium.launch(headless=True);page=browser.new_page(viewport={'width':1440,'height':1100},locale='en-US')
    errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
    page.add_init_script("localStorage.setItem('sacho-language','en')")
    page.goto(base+'/admin');page.get_by_role('button',name='+ Add service',exact=True).click()
    page.locator('[name=name]').fill('Gateway scenario');page.locator('[name=group]').fill('Infrastructure');page.locator('[name=kind]').select_option('lua');page.locator('[name=interval_seconds]').fill('5');page.locator('[name=timeout_seconds]').fill('3')
    editor=page.locator('.sacho-lua-editor .cm-content');editor.wait_for()
    editor.fill('return sacho.json_encode({version="v0.1.0",tps=42.5})')
    page.locator('[name=publish_value]').check();page.locator('[name=display_pointer]').fill('/version');page.locator('[name=display_label]').fill('Version');page.locator('[name=metric_unit]').fill('')
    page.get_by_role('button',name='Test draft',exact=True).click();expect(page.locator('#draft-result')).to_contain_text('Operational')
    assert not request('/api/incidents')
    # Language switching must preserve the Lua draft and its editor value.
    draft=page.locator('[name=target]').input_value();page.locator('.dialog-language').click();assert page.locator('[name=target]').input_value()==draft,(draft,page.locator('[name=target]').input_value())
    page.locator('.dialog-language').click()
    editor.click();page.keyboard.press('End');page.keyboard.type('\n-- editor draft survives language changes')
    # Save through the native textarea synced by CodeMirror.
    page.get_by_role('button',name='Save service',exact=True).click();page.locator('#edit-dialog').wait_for(state='hidden')
    second=request('/api/admin/items','POST',{'name':'Dorang TPS','group':'Applications','kind':'lua','target':'return sacho.json_encode({tps=42.5})','publish_value':True,'display_pointer':'/tps','metric_unit':' TPS'})['id']
    page.reload();page.get_by_role('button',name='Layout',exact=True).click();page.locator('.layout-card').first.wait_for()
    page.get_by_role('button',name='Add section',exact=True).click();sections=page.locator('.layout-section');sections.last.locator('[data-section-name]').fill('Live readings')
    # Real HTML drag-and-drop between sections, then keyboard-accessible reorder.
    card=page.locator(f'[data-drag-id="{second}"]');target=sections.last.locator('.layout-drop');card.drag_to(target)
    assert sections.last.locator('.layout-card').count()==1
    page.get_by_role('button',name='Save layout',exact=True).click();wait_for(lambda:request('/api/status')['site']['layout'][-1]['item_ids']==[second])
    page.get_by_role('button',name='Preview',exact=True).click();page.locator('#layout-preview .service').first.wait_for()
    page.screenshot(path=str(ROOT/'runtime/screenshots/admin-layout.png'),full_page=True)
    wait_for(lambda:all(i['last_check'] is not None for i in request('/api/status')['items']))
    page.goto(base+'/');page.get_by_text('v0.1.0',exact=False).first.wait_for();page.get_by_text('42.5 TPS',exact=True).wait_for();page.locator('.bar').first.wait_for()
    page.locator('#range').select_option('3600');expect(page.locator('#precision')).to_have_value('30');assert not page.locator('#precision option[value="86400"]').count();page.locator('#precision').select_option('5');page.locator('.bar').last.wait_for();page.locator('.bar').last.hover();expect(page.locator('#chart-tooltip')).to_be_visible();expect(page.locator('#chart-tooltip')).to_contain_text(' → ');page.locator('.bar').last.focus();expect(page.locator('#chart-tooltip')).to_be_visible();page.keyboard.press('Escape');expect(page.locator('#chart-tooltip')).to_be_hidden();page.locator('.bar').last.click();assert page.locator('.detail:not([hidden])').count()==1
    page.screenshot(path=str(ROOT/'runtime/screenshots/status-desktop.png'),full_page=True)
    page.locator('#language').click();page.get_by_text('모든 시스템이 정상 운영 중입니다').wait_for()
    page.set_viewport_size({'width':390,'height':844});page.screenshot(path=str(ROOT/'runtime/screenshots/status-mobile-ko.png'),full_page=True)
    page.wait_for_timeout(600);assert page.evaluate('document.documentElement.scrollWidth <= window.innerWidth');assert page.locator('.bars').evaluate_all('(charts)=>charts.every(chart=>chart.scrollWidth<=chart.clientWidth)')
    # Identity fixture exercises the public account strip without production data.
    page.route('**/api/account',lambda route:route.fulfill(json={'email':'reader@example.com','subscription':'subscribed','logout_url':'/identity/logout'}))
    page.reload();expect(page.locator('.subscription-state.subscribed')).to_contain_text('reader@example.com');assert page.locator('#account a').get_attribute('href')=='/identity/logout'
    page.route('**/api/account',lambda route:route.fulfill(json={'email':'reader@example.com','subscription':'unsubscribed','logout_url':None}))
    page.reload();expect(page.locator('#account')).to_be_visible();assert page.locator('.subscription-state.subscribed').count()==0;assert page.locator('#account a').count()==0
    page.locator('#subscribe-open').evaluate('(button)=>button.disabled=false');page.locator('#subscribe-open').click();assert page.locator('#subscribe-form [name=email]').input_value()=='reader@example.com'
    assert not errors,errors
    browser.close()
    print('PASS: embedded Lua editor, language draft preservation, test/save, version/TPS display, drag/drop persistence, preview, automatic 1h resolution, 5s chart and Korean mobile layout')
  finally:
   server.terminate()
   try:server.wait(timeout=20)
   except subprocess.TimeoutExpired:server.kill();server.wait()
   log.close()
if __name__=='__main__':main()
