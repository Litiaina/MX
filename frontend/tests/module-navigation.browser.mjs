// 100-module navigation using the real MX API and production frontend. No live data.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';

assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/PROJECT-DISK-1TB/'), 'Use only the HDD for test artifacts');
assert.ok(process.env.TMPDIR?.startsWith('/mnt/PROJECT-DISK-1TB/'), 'Use only the HDD for temporary files');
assert.ok(!process.env.MX_TEST_N1_URL, 'Never use live N1');
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
const browsers = [], results = {passed:false,browsers:{}};
const call = async (path, user, method='GET', data, expected=[200]) => (await mx.call(path,user?.access_token,method,data,expected)).body;
const until = async (check,label) => { const deadline=Date.now()+30000; while(Date.now()<deadline){if(await check())return;await new Promise(resolve=>setTimeout(resolve,100));}throw Error(label); };
let inspected;
try {
  console.log(`Module navigation artifacts: ${mx.directory}`);
  const password=`Disposable!${randomUUID()}`;
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@modules.test',name:'Owner',password},[201]);
  const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@modules.test',password});
  await call('/mx/v1/user/create',owner,'POST',{email:'operator@modules.test',name:'Operator',password,access_level:3},[201]);
  const operator=await call('/mx/v1/auth/authenticate',null,'POST',{email:'operator@modules.test',password});
  const modules=[];
  for(let index=0;index<100;index++)modules.push((await call('/mx/v1/admin/modules',owner,'POST',{
    name:`Department ${String(index).padStart(3,'0')}`,singular_name:'Entry',description:`Workspace ${index}`,icon:'database',position:index*10,config:{navigation_group:index<20?'Finance':'Operations'}
  },[201])).module);
  const grants=await call(`/mx/v1/admin/accounts/${operator.uid}/modules`,owner);
  await call(`/mx/v1/admin/accounts/${operator.uid}/modules`,owner,'PUT',{revision:grants.revision,grants:modules.slice(0,2).map(module=>({module_uid:module.uid,can_read:true,can_create:false,can_update:false,can_delete:false,can_configure:false,can_report:false,can_attachments:false}))});
  assert.equal((await call('/mx/v1/modules',operator)).modules.length,2);
  for(const engine of (process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')) {
    const executablePath=engine==='brave'?'/usr/bin/brave-browser':engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await(engine==='brave'?chromium:{chromium,firefox,webkit}[engine]).launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(['chromium','brave'].includes(engine)?{args:['--no-sandbox','--mute-audio']}: {})});browsers.push(browser);
    const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1440,height:950}});
    await context.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
    await context.addInitScript(()=>{
      let leaving=false;window.addEventListener('beforeunload',()=>{leaving=true;});window.addEventListener('pagehide',()=>{leaving=true;});window.addEventListener('pageshow',()=>{leaving=false;});
      const nativeFetch=window.fetch.bind(window);window.fetch=(input,init)=>{
        if(leaving&&/\/mx\/v1\/(?:modules\/[^/]+\/records(?:\?|$)|account\/preferences|notifications)/.test(String(input)))sessionStorage.setItem('mx_test_record_fetch_after_exit',String(Number(sessionStorage.getItem('mx_test_record_fetch_after_exit')||0)+1));
        return nativeFetch(input,init);
      };
    });
    const page=await context.newPage();inspected=page;const errors=[];page.on('pageerror',error=>{errors.push(error.message);console.error(JSON.stringify({event:'pageerror',url:page.url(),stack:error.stack}));});
    await page.goto(`https://127.0.0.1:${mx.port}/#drive`);
    const sidebar=page.getByRole('complementary',{name:'Workspace navigation'}), trigger=sidebar.getByRole('button',{name:'Modules',exact:true});
    await trigger.waitFor();assert.ok(await sidebar.locator('.main-nav button').count()<=5,'100 modules do not become 100 sidebar icons');
    const dialog=page.getByRole('dialog',{name:'Browse modules',exact:true});
    await trigger.click();await dialog.waitFor();
    assert.ok(await dialog.getByRole('searchbox',{name:'Search modules',exact:true}).evaluate(node=>document.activeElement===node),'Opening the library focuses search');
    assert.equal(await dialog.locator('.module-browser-results > button').count(),40,'Only one bounded page is mounted');
    await dialog.getByRole('button',{name:'Next',exact:true}).click();assert.ok((await dialog.locator('footer').innerText()).includes('41–80 of 100'));
    await dialog.getByRole('button',{name:'Next',exact:true}).click();assert.equal(await dialog.locator('.module-browser-results > button').count(),20);
    assert.ok(await dialog.getByRole('button',{name:'Next',exact:true}).isDisabled());
    await dialog.getByRole('searchbox',{name:'Search modules',exact:true}).fill('Department 099');
    assert.equal(await dialog.locator('.module-browser-results > button').count(),1);
    await dialog.getByRole('searchbox',{name:'Search modules',exact:true}).fill('no-such-module');
    await dialog.getByRole('heading',{name:'No matching modules',exact:true}).waitFor();
    await dialog.getByRole('button',{name:'Clear filters',exact:true}).click();
    await dialog.getByRole('combobox',{name:'Filter modules by group',exact:true}).selectOption({label:'Finance'});
    assert.equal(await dialog.locator('.module-browser-results > button').count(),20);
    await page.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});assert.ok(await trigger.evaluate(node=>document.activeElement===node),'Escape returns focus to the opener');
    await trigger.click();await dialog.waitFor();
    await page.locator('.module-browser-backdrop').click({position:{x:3,y:3}});
    await dialog.waitFor({state:'hidden'});assert.ok(await trigger.evaluate(node=>document.activeElement===node),'Outside click closes the module browser and restores focus');

    // Existing administrator position/group settings control both menu and library.
    await page.goto(`https://127.0.0.1:${mx.port}/#admin`);await page.getByRole('button',{name:'Modules & fields',exact:true}).click();
    const create=page.getByRole('dialog',{name:'Create a separate module',exact:true});
    await page.getByRole('button',{name:'New module',exact:true}).click();await create.waitFor();
    await page.keyboard.press('Escape');await create.waitFor({state:'hidden'});
    await page.getByRole('button',{name:'New module',exact:true}).click();await create.waitFor();
    await create.getByRole('textbox',{name:'Module name',exact:true}).fill('Unsaved draft');
    await create.locator('..').click({position:{x:3,y:3}});
    const discard=page.getByRole('alertdialog',{name:'Discard this new module?',exact:true});await discard.waitFor();
    await discard.getByRole('button',{name:'Cancel',exact:true}).click();
    assert.equal(await create.getByRole('textbox',{name:'Module name',exact:true}).inputValue(),'Unsaved draft');
    await page.keyboard.press('Escape');await discard.waitFor();
    await discard.getByRole('button',{name:'Discard draft',exact:true}).click();await create.waitFor({state:'hidden'});
    assert.equal((await call('/mx/v1/modules',owner)).modules.length,100,'Dismissing a draft never creates or changes a module');
    await page.getByRole('searchbox',{name:'Find a module',exact:true}).fill('Department 099');
    await page.getByRole('button',{name:'Configure Department 099',exact:true}).click();
    await page.getByRole('button',{name:'Module details',exact:true}).click();
    const order = -10-modules.length-Object.keys(results.browsers).length;
    await page.getByRole('spinbutton',{name:'Navigation order',exact:false}).fill(String(order));
    await page.getByRole('button',{name:'Save changes',exact:true}).click();
    await until(async()=>{const first=(await call('/mx/v1/modules',owner)).modules[0];return first.uid===modules[99].uid&&first.position===order;},'Saved position reorders persisted modules');
    await page.goto(`https://127.0.0.1:${mx.port}/#drive`);await page.reload();await trigger.click();await dialog.waitFor();
    assert.equal(await dialog.locator('.module-browser-results > button').first().getAttribute('aria-label'),'Open Department 099');
    await dialog.getByRole('button',{name:'Open Department 099',exact:true}).click();await dialog.waitFor({state:'hidden'});
    await until(async()=>page.url().includes(`#module/${modules[99].uid}`),'A library card opens the intended module');

    await page.getByRole('button',{name:'Expand navigation to search',exact:true}).click();
    await trigger.click();const shortcuts=sidebar.getByRole('region',{name:'Module shortcuts',exact:true});await shortcuts.waitFor();
    assert.equal(await shortcuts.locator('.module-shortcut-list button').count(),12,'Expanded shortcuts stay bounded');
    await shortcuts.getByRole('searchbox',{name:'Filter module shortcuts',exact:true}).fill('Department 042');
    assert.equal(await shortcuts.locator('.module-shortcut-list button').count(),1);
    await shortcuts.getByRole('button',{name:'Department 042',exact:true}).click();
    await until(async()=>page.url().includes(`#module/${modules[42].uid}`),'Shortcut opens the searched workspace');
    if(await sidebar.getByRole('button',{name:'Collapse navigation',exact:true}).isVisible())await sidebar.getByRole('button',{name:'Collapse navigation',exact:true}).click();

    const savedPreferences=await call('/mx/v1/account/preferences',owner);
    await call('/mx/v1/account/preferences',owner,'PUT',{revision:savedPreferences.revision,preferences:{...savedPreferences.preferences,auto_scale:false,ui_scale_percent:100,font_size_px:24,font_scale_percent:null}});
    await page.reload();await trigger.waitFor();
    await until(async()=>await page.evaluate(()=>parseFloat(getComputedStyle(document.documentElement).fontSize))>=24,'Saved enlarged-text preference is actually applied');
    for(const theme of ['light','dark'])for(const width of [1440,390,320]) {
      await page.setViewportSize({width,height:950});
      await page.evaluate(theme=>{document.documentElement.dataset.theme=theme;},theme);
      if(width<=820){await page.getByRole('button',{name:'Open navigation',exact:true}).click();if(await trigger.getAttribute('aria-expanded')!=='true')await trigger.click();await sidebar.getByRole('button',{name:'Browse all modules (100)',exact:true}).click();}else await trigger.click();
      await dialog.waitFor();
      assert.ok(await dialog.locator('h2').evaluate(node=>parseFloat(getComputedStyle(node).fontSize))>=28,'Module text actually enlarges, not merely the container');
      assert.ok(await dialog.evaluate(node=>node.scrollWidth<=node.clientWidth+1),'Library fits at enlarged text and small widths');
      const box=await dialog.boundingBox();assert.ok(box.x>=0&&box.x+box.width<=width+1,'Modal fits the viewport');
      assert.ok(await dialog.getByRole('button',{name:'Close module browser',exact:true}).isVisible());
      await page.screenshot({path:join(mx.directory,`${engine}-modules-${theme}-${width}.png`)});
      await page.keyboard.press('Escape');
      if(width<=820)await sidebar.getByRole('button',{name:'Collapse navigation',exact:true}).click();
    }
    const currentPreferences=await call('/mx/v1/account/preferences',owner);
    await call('/mx/v1/account/preferences',owner,'PUT',{revision:currentPreferences.revision,preferences:savedPreferences.preferences});
    const limited=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1440,height:950}});await limited.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},operator);
    const reader=await limited.newPage();inspected=reader;await reader.goto(`https://127.0.0.1:${mx.port}/#drive`);await reader.getByRole('button',{name:'Modules',exact:true}).click();
    const restricted=reader.getByRole('dialog',{name:'Browse modules',exact:true});await restricted.waitFor();assert.equal(await restricted.locator('.module-browser-results > button').count(),2);
    await restricted.getByRole('searchbox',{name:'Search modules',exact:true}).fill('Department 099');await restricted.getByRole('heading',{name:'No matching modules',exact:true}).waitFor();
    await limited.close();assert.equal(await page.evaluate(()=>Number(sessionStorage.getItem('mx_test_record_fetch_after_exit')||0)),0,'Record, preference and notification refreshes do not start requests while leaving/reloading the document');assert.deepEqual(errors,[]);
    results.browsers[engine]={passed:true,checks:['100 modules, bounded rail/menu/library','Search, group filter, paging, empty recovery','Administrator order persisted and reflected in navigation','Keyboard focus/Escape','Light/dark, enlarged text, 1440/390/320 widths','Only account-authorized modules visible']};
    console.log(`${engine}: 100-module navigation passed`);await context.close();await browser.close();
  }
  results.passed=true;
}catch(error){results.error=error.stack;if(inspected&&!inspected.isClosed())await inspected.screenshot({path:join(mx.directory,'module-navigation-failure.png')}).catch(()=>{});throw error;}
finally{await writeFile(join(mx.directory,'module-navigation-results.json'),JSON.stringify(results,null,2));await Promise.all(browsers.map(browser=>browser.close().catch(()=>{})));await mx.close();}
