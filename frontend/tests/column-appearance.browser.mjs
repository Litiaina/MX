// Real persistence + production frontend in disposable HDD-only databases.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/PROJECT-DISK-1TB/'));
assert.ok(process.env.TMPDIR?.startsWith('/mnt/PROJECT-DISK-1TB/'));
assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const result={passed:false,browsers:{}};
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const until=async(fn,label)=>{const end=Date.now()+20000;while(Date.now()<end){if(await fn())return;await new Promise(resolve=>setTimeout(resolve,70));}throw Error(label);};
let browser,page;
try {
 console.log(`Column appearance artifacts: ${mx.directory}`);
 const password=`Disposable!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@appearance.test',name:'Owner',password},[201]);
 const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@appearance.test',password});
 await call('/mx/v1/user/create',owner,'POST',{email:'other@appearance.test',name:'Other',password,access_level:2},[201]);
 const other=await call('/mx/v1/auth/authenticate',null,'POST',{email:'other@appearance.test',password});
 const module=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Requests',singular_name:'Request'},[201])).module;
 const base=`/mx/v1/modules/${module.uid}`;
 const fields={};
 for(const [key,label,field_type,config] of [['name','Name','text',{}],['status','Status','select',{options:['In review','Approved']}],['amount','Amount','decimal',{}],['notes','Notes','long_text',{}]]) {
  fields[key]=await call(`/mx/v1/admin/modules/${module.uid}/fields`,owner,'POST',{key,label,field_type,config,table_visible:true},[201]);
 }
 const longText='Full readable text. '.repeat(35)+'Final visible line.';
 await call(`${base}/records`,owner,'POST',{operation_uid:randomUUID(),values:{name:'A readable request',status:'In review',amount:1250.25,notes:longText}},[201]);
 const before=await call(`${base}/records`,owner);
 const pref='/mx/v1/account/preferences';
 for(const engine of (process.env.MX_TEST_BROWSERS||'chromium,brave,firefox,webkit').split(',')) {
  const current=await call(pref,owner);
  await call(pref,owner,'PUT',{revision:current.revision,preferences:{...current.preferences,theme:'light',accent_color:null,module_reading:{},reduced_motion:false,auto_scale:false,font_size_px:16}});
  const executablePath=engine==='brave'?'/usr/bin/brave-browser':engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
  browser=await(engine==='brave'?chromium:{chromium,firefox,webkit}[engine]).launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(['chromium','brave'].includes(engine)?{args:['--no-sandbox','--mute-audio']}: {})});
  const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1440,height:950}});
  await context.addInitScript(t=>{sessionStorage.setItem('mx_access_token',t.access_token);sessionStorage.setItem('mx_refresh_token',t.refresh_token);},owner);
  page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(`https://127.0.0.1:${mx.port}/#module/${module.uid}`);
  const row=page.locator('.records-table tr[data-record-uid]').first();await row.waitFor();
  const cell=key=>row.locator(`[data-field-uid="${fields[key].uid}"]`);
  assert.equal(await cell('status').locator('.column-badge').innerText(),'In review');
  assert.equal(await cell('amount').evaluate(n=>getComputedStyle(n).textAlign),'right');
  assert.equal(await cell('name').evaluate(n=>getComputedStyle(n).borderRightWidth),'0px');
  assert.ok(await cell('notes').innerText().then(text=>text.includes('Final visible line.')));
  const openReading=async()=>{await page.getByRole('button',{name:'Reading settings',exact:true}).click();await page.locator('.reading-dialog').getByRole('combobox',{name:'Display Name',exact:true}).waitFor();};
  await openReading();const reading=page.locator('.reading-dialog');
  await reading.getByRole('combobox',{name:'Display Name',exact:true}).selectOption('badge');
  await reading.getByRole('combobox',{name:'Color Name',exact:true}).selectOption('rose');
  await reading.getByRole('checkbox',{name:'Bold Name',exact:true}).check();
  await page.keyboard.press('Escape');
  const confirmation=page.getByRole('alertdialog',{name:'Discard appearance changes?',exact:true});await confirmation.waitFor();
  await confirmation.getByRole('button',{name:'Cancel',exact:true}).click();
  assert.equal(await reading.getByRole('combobox',{name:'Color Name',exact:true}).inputValue(),'rose','Cancel dismissal keeps the draft');
  await reading.locator('..').click({position:{x:3,y:3}});await confirmation.waitFor();
  await confirmation.getByRole('button',{name:'Cancel',exact:true}).click();
  await until(()=>reading.evaluate(n=>n.contains(document.activeElement)),'Canceling outside dismissal restores dialog focus');
  await reading.getByRole('button',{name:'Save my reading settings',exact:true}).click();await reading.waitFor({state:'hidden'});
  await until(()=>cell('name').locator('.column-badge[data-tone="rose"]').count(),'Saved appearance renders');
  const saved=await call(pref,owner);
  assert.deepEqual(saved.preferences.module_reading[module.uid].columns[fields.name.uid],{style:'badge',tone:'rose'});
  assert.deepEqual(saved.preferences.module_reading[module.uid].bold_fields,[fields.name.uid]);
  assert.deepEqual((await call(pref,other)).preferences.module_reading,{},'Personal appearance never changes another account');
  await page.reload();await cell('name').locator('.column-badge[data-tone="rose"]').waitFor();
  for(const theme of ['light','dark']) {
   await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);
   const palette=await page.evaluate(()=>{const css=getComputedStyle(document.documentElement);return{primary:css.getPropertyValue('--primary').trim(),bg:css.getPropertyValue('--bg').trim()};});
   assert.equal(palette.primary,theme==='dark'?'#2563eb':'#c94128');
   if(theme==='dark')assert.equal(palette.bg,'#090a0b');
   await page.screenshot({path:join(mx.directory,`${engine}-${theme}-records.png`),animations:'disabled'});
  }
  await openReading();await reading.getByRole('combobox',{name:'Display Status',exact:true}).selectOption('text');
  const changed=await call(pref,owner);
  await call(pref,owner,'PUT',{revision:changed.revision,preferences:{...changed.preferences,module_reading:{[module.uid]:{bold_fields:[]}}}});
  await reading.getByRole('button',{name:'Save my reading settings',exact:true}).click();
  await reading.getByRole('alert').filter({hasText:'changed in another window'}).waitFor();
  await until(()=>reading.evaluate(n=>n.contains(document.activeElement)),'Save failure keeps focus in the modal');
  assert.deepEqual((await call(pref,owner)).preferences.module_reading[module.uid],{bold_fields:[]},'Stale modal never overwrites another window');
  await page.keyboard.press('Escape');await confirmation.getByRole('button',{name:'Discard changes',exact:true}).click();await reading.waitFor({state:'hidden'});
  for(const width of [390,320]) {
   await page.setViewportSize({width,height:950});await page.evaluate(()=>document.documentElement.style.fontSize='150%');
   await openReading();
   assert.ok(await reading.evaluate(n=>n.scrollWidth<=n.clientWidth+1),'Appearance dialog reflows at enlarged text');
   await reading.getByRole('combobox',{name:'Color Name',exact:true}).scrollIntoViewIfNeeded();
   await page.screenshot({path:join(mx.directory,`${engine}-appearance-${width}.png`),animations:'disabled'});
   await page.keyboard.press('Escape');await reading.waitFor({state:'hidden'});
  }
  await page.setViewportSize({width:1440,height:950});
  await page.emulateMedia({reducedMotion:'reduce'});
  assert.ok(await page.locator('.records-page').evaluate(n=>parseFloat(getComputedStyle(n).animationDuration)<.001),'OS reduced-motion disables route animation');
  await page.emulateMedia({reducedMotion:'no-preference'});
  await page.evaluate(()=>document.documentElement.dataset.reducedMotion='true');
  assert.ok(await page.locator('.records-page').evaluate(n=>parseFloat(getComputedStyle(n).animationDuration)<.001),'MX reduced-motion disables route animation');
  const custom=await call(pref,owner);
  await call(pref,owner,'PUT',{revision:custom.revision,preferences:{...custom.preferences,theme:'dark',accent_color:'#e0ac44'}});
  await page.reload();await row.waitFor();
  await until(()=>page.evaluate(()=>getComputedStyle(document.documentElement).getPropertyValue('--primary').trim()==='#e0ac44'),'Account preferences finish loading after refresh');
  assert.equal(await page.evaluate(()=>getComputedStyle(document.documentElement).getPropertyValue('--primary').trim()),'#e0ac44','Explicit custom accents survive dark mode');
  const deployment=await call('/mx/v1/deployment/config',null);
  const refreshed=page.waitForResponse(response=>new URL(response.url()).pathname==='/mx/v1/deployment/config'&&response.request().method()==='GET');
  await call('/mx/v1/admin/deployment-config',owner,'PUT',{config:deployment.config});
  await refreshed;await new Promise(resolve=>setTimeout(resolve,300));
  assert.equal(await page.evaluate(()=>getComputedStyle(document.documentElement).getPropertyValue('--primary').trim()),'#e0ac44','Live deployment refresh must not overwrite personal appearance');
  assert.equal(await page.evaluate(()=>document.documentElement.dataset.theme),'dark');
  assert.deepEqual(errors,[]);
  result.browsers[engine]={passed:true,checks:['Default category badges','Plain numeric alignment and horizontal row rules','Complete long text','Saved personal color/badge/bold and reload','Account isolation','Draft dismissal protection','Stale preference conflict','Mobile enlarged text','OS and MX reduced motion','Light/dark default and custom accent preservation']};
  await context.close();await browser.close();browser=null;console.log(`${engine}: column appearance passed`);
 }
 assert.deepEqual((await call(`${base}/records`,owner)).records,before.records,'Appearance never mutates business records');
 result.passed=true;
}catch(error){result.error=error.stack;if(page&&!page.isClosed())await page.screenshot({path:join(mx.directory,'appearance-failure.png')}).catch(()=>{});throw error;}
finally{if(browser)await browser.close();await writeFile(join(mx.directory,'column-appearance-results.json'),JSON.stringify(result,null,2));await mx.close();}
