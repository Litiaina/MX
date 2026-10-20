// Disposable MX + SQLite + bundled production UI. No live deployment or N1.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join, isAbsolute } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';

assert.ok(isAbsolute(process.env.MX_TEST_ARTIFACTS || ''),'Set MX_TEST_ARTIFACTS under the test directory');
assert.ok(!process.env.MX_TEST_N1_URL,'Do not use live N1');
const mx = await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const browsers = [], results = { api:[], browsers:{} };
let activePage;
const password = `Disposable!${randomUUID()}`;
const call = async (path,user,method='GET',data,expected=[200]) => (await mx.call(path,user?.access_token,method,data,expected)).body;
const until = async (check,label) => { const end=Date.now()+15000; while(Date.now()<end){if(await check())return;await new Promise(resolve=>setTimeout(resolve,80));}throw Error(label); };
try {
  console.log(`Record workspace artifacts: ${mx.directory}`);
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@workspace.test',name:'Owner',password},[201]);
  const owner = await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@workspace.test',password});
  const users=[];
  for (const name of ['Recipient','Restricted person']) {
    const email=`${users.length}@workspace.test`;
    await call('/mx/v1/user/create',owner,'POST',{email,name,password,access_level:2},[201]);
    users.push(await call('/mx/v1/auth/authenticate',null,'POST',{email,password}));
  }
  const module=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Requests',singular_name:'Request',description:'Requests and their review context'},[201])).module;
  const other=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Employees',singular_name:'Employee'},[201])).module;
  const field = async (key,label,type) => call(`/mx/v1/admin/modules/${module.uid}/fields`,owner,'POST',{key,label,field_type:type,table_visible:true},[201]);
  const name=await field('name','Name','text'),details=await field('details','Details','long_text');
  let record=await call(`/mx/v1/modules/${module.uid}/records`,owner,'POST',{values:{name:'A request to review',details:'Full text remains readable.\nFinal visible line.'}},[201]);
  const grant=await call(`/mx/v1/admin/accounts/${users[0].uid}/modules`,owner);
  await call(`/mx/v1/admin/accounts/${users[0].uid}/modules`,owner,'PUT',{revision:grant.revision,grants:[{module_uid:module.uid,can_read:true,can_create:false,can_update:false,can_delete:false,can_configure:false,can_report:false,can_attachments:false}]});
  const restrictedBefore=await call(`/mx/v1/admin/accounts/${users[1].uid}/modules`,owner);
  const paths = `/mx/v1/modules/${module.uid}/records/${record.uid}`;
  const preferences='/mx/v1/account/preferences';
  const initial=await call(preferences,owner);
  await call(preferences,owner,'PUT',{revision:initial.revision,preferences:{theme:'dark',notifications:{sound_enabled:false},module_reading:{[module.uid]:{bold_fields:[name.uid]}}}});
  await call(preferences,owner,'PUT',{revision:initial.revision,preferences:{theme:'light'}},[409]);
  assert.equal((await call(preferences,owner)).preferences.theme,'dark');
  assert.deepEqual((await call(preferences,users[0])).preferences.module_reading,{});
  await call(preferences,owner,'PUT',{preferences:{font_size_px:33}},[400]);
  const found=await call('/mx/v1/collaboration/people?q=Recipient&limit=1',owner);
  assert.equal(found.people.length,1);assert.equal(found.people[0].uid,users[0].uid);
  assert.equal((await call('/mx/v1/collaboration/people?q=%25&limit=1',owner)).people.length,0,'LIKE wildcards are escaped');
  const parallel=await Promise.all(Array.from({length:8},()=>call('/mx/v1/collaboration/direct',owner,'POST',{user_uid:users[0].uid},[200,201])));
  assert.equal(new Set(parallel.map(result=>result.uid)).size,1,'Concurrent sends reuse one direct channel');
  results.api.push('Personal preferences isolated; stale saves reject without overwrite; validated size; bounded/escaped people search; eight concurrent direct-channel requests reuse one channel');

  for (const engine of (process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')) {
    const current=await call(preferences,owner);
    await call(preferences,owner,'PUT',{revision:current.revision,preferences:{...current.preferences,font_size_px:16,module_reading:{},notifications:{sound_enabled:false}}});
    const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1440,height:950}});activePage=page;
    const errors=[];page.on('pageerror',error=>{errors.push(error.message);console.error('Browser error',JSON.stringify({engine,url:page.url(),message:error.message,stack:error.stack}));});
    await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
    const origin=`https://127.0.0.1:${mx.port}`;
    await page.goto(`${origin}/#module/${module.uid}`);
    const row=page.locator(`[data-record-uid="${record.uid}"]`);
    const cell=row.locator(`[data-field-uid="${name.uid}"]`);
    await cell.waitFor();
    assert.equal(await page.locator('.cell-edit-button,[data-active-cell-editor]').count(),0);
    await page.screenshot({path:join(mx.directory,`${engine}-read-only-records.png`)});
    const baseFont=await cell.evaluate(node=>parseFloat(getComputedStyle(node).fontSize));
    await cell.evaluate(node=>{const range=document.createRange();range.selectNodeContents(node);const selection=getSelection();selection.removeAllRanges();selection.addRange(range);node.click();});
    assert.equal(await page.getByRole('dialog').count(),0,'Selecting table text does not open an editor');
    await page.evaluate(()=>getSelection().removeAllRanges());
    await cell.click();
    const view=page.getByRole('dialog',{name:'View Request',exact:true});await view.waitFor();
    assert.ok(await view.getByLabel('Name',{exact:true}).isDisabled());
    await view.getByRole('button',{name:'Close',exact:true}).click();
    await row.getByRole('button',{name:'Edit Request 1',exact:true}).click();
    const editor=page.getByRole('dialog',{name:'Edit Request',exact:true});await editor.waitFor();
    await editor.getByLabel('Name',{exact:true}).fill(`Saved via full form ${engine}`);
    await editor.getByRole('button',{name:'Send record',exact:true}).click();
    const send=page.getByRole('dialog',{name:'Send record',exact:true});await send.waitFor();
    assert.ok((await send.innerText()).includes('not your unfinished edits'));
    await page.keyboard.press('Escape');await send.waitFor({state:'hidden'});
    assert.equal(await editor.getByLabel('Name',{exact:true}).inputValue(),`Saved via full form ${engine}`);
    await editor.getByRole('button',{name:'Update Request',exact:true}).click();
    await until(async()=>(await call(paths,owner)).values.name===`Saved via full form ${engine}`,'Full record edit persisted');
    await editor.waitFor({state:'hidden'});

    await page.getByRole('button',{name:'Reading settings',exact:true}).click();
    const reading=page.getByRole('dialog',{name:'Make Requests easier to read',exact:true});
    await reading.getByRole('checkbox',{name:'Bold Name',exact:true}).check();
    await reading.getByRole('button',{name:'Save my reading settings',exact:true}).click();await reading.waitFor({state:'hidden'});
    await until(async()=>Number(await cell.evaluate(node=>getComputedStyle(node).fontWeight))>=700,'Personal emphasis applied');
    assert.ok(Number(await row.locator(`[data-field-uid="${details.uid}"]`).evaluate(node=>getComputedStyle(node).fontWeight))<700,'Unchosen field remains normal');
    await page.reload();await cell.waitFor();await until(async()=>Number(await cell.evaluate(node=>getComputedStyle(node).fontWeight))>=700,'Emphasis survives reload');
    await row.getByRole('button',{name:'Edit Request 1',exact:true}).click();
    await until(async()=>Number(await editor.getByLabel('Name',{exact:true}).evaluate(node=>getComputedStyle(node).fontWeight))>=700,'Form preference applies to input');
    await editor.getByRole('button',{name:'Close',exact:true}).click();

    // No manual clipboard or module navigation. Drop both automatic send
    // acknowledgments, then explicitly retry the same operation.
    await row.getByRole('button',{name:'More actions for Request 1',exact:true}).click();
    await page.getByRole('menuitem',{name:'Send record',exact:true}).click();await send.waitFor();
    await send.getByRole('searchbox',{name:'Send to',exact:true}).fill('Recipient');
    await send.getByRole('button',{name:'R Recipient',exact:true}).or(send.getByRole('button',{name:'Recipient',exact:true})).first().click();
    const context=`Please review this request (${engine})`;
    await send.getByRole('textbox',{name:'Message (optional)',exact:true}).fill(context);
    await page.screenshot({path:join(mx.directory,`${engine}-send-record.png`)});
    let drops=0;
    await page.route('**/collaboration/channels/*/messages',async route=>{
      if(route.request().method()!=='POST'||drops>=2)return route.continue();
      drops++;await route.fetch();await route.abort('failed');
    });
    await send.getByRole('button',{name:'Send record',exact:true}).click();
    await send.getByRole('button',{name:'Retry same send',exact:true}).waitFor();
    await send.getByRole('button',{name:'Retry same send',exact:true}).click();
    await send.getByText('Sent to Recipient',{exact:true}).waitFor();
    assert.ok(await send.getByRole('button',{name:'Done',exact:true}).evaluate(node=>node===document.activeElement),'Confirmed send retains keyboard focus in the dialog');
    const messages=await call(`/mx/v1/collaboration/channels/${parallel[0].uid}/messages`,users[0]);
    const matching=messages.messages.filter(message=>message.body===context);
    assert.equal(matching.length,1,'Retries create exactly one message');
    assert.equal(matching[0].record_links[0].uid,record.uid);
    const notifications=await call('/mx/v1/notifications',users[0]);
    assert.ok(notifications.notifications.some(item=>JSON.stringify(item).includes(parallel[0].uid)),'Recipient receives collaboration notification');
    assert.ok(page.url().includes(`#module/${module.uid}`),'Send stays in the original module');
    await send.getByRole('button',{name:'Done',exact:true}).click();
    await page.unroute('**/collaboration/channels/*/messages');

    await page.getByRole('button',{name:'Personal settings',exact:true}).click();
    const settings=page.getByRole('dialog',{name:'Display and refresh',exact:true});
    await settings.getByRole('button',{name:'Large',exact:true}).click();
    await settings.getByRole('button',{name:'Save preferences',exact:true}).click();await settings.waitFor({state:'hidden'});
    await until(async()=>Math.abs((await cell.evaluate(node=>parseFloat(getComputedStyle(node).fontSize)))/baseFont-1.25)<.02,'Actual table font enlarges');
    await page.getByRole('button',{name:'Personal settings',exact:true}).click();
    await settings.getByRole('button',{name:'Extra large',exact:true}).click();
    await settings.getByRole('button',{name:'Cancel',exact:true}).click();
    await until(async()=>Math.abs((await cell.evaluate(node=>parseFloat(getComputedStyle(node).fontSize)))/baseFont-1.25)<.02,'Cancel restores saved text size');
    assert.deepEqual((await call(preferences,owner)).preferences.module_reading[module.uid].bold_fields,[name.uid],'Display save preserves emphasis');

    await page.setViewportSize({width:390,height:950});
    await page.evaluate(()=>{document.documentElement.style.fontSize='200%';document.documentElement.dataset.theme='dark';});
    await page.getByRole('button',{name:'Reading settings',exact:true}).click();await reading.waitFor();
    const readingBounds=await reading.boundingBox();assert.ok(readingBounds.x>=0&&readingBounds.x+readingBounds.width<=391);
    assert.ok(await reading.evaluate(node=>node.scrollWidth<=node.clientWidth+1),'Reading dialog fits enlarged text');
    await reading.getByRole('checkbox',{name:'Bold Name',exact:true}).scrollIntoViewIfNeeded();
    assert.ok(await reading.getByRole('checkbox',{name:'Bold Name',exact:true}).isVisible(),'Field options remain reachable at 200%');
    assert.ok(await reading.locator('.reading-field>span:first-of-type').first().evaluate(node=>node.clientWidth>=parseFloat(getComputedStyle(node).fontSize)*4),'Field names retain readable width, not one-letter columns');
    await page.screenshot({path:join(mx.directory,`${engine}-reading-mobile-200.png`)});
    await reading.getByRole('button',{name:'Save my reading settings',exact:true}).scrollIntoViewIfNeeded();
    const saveBounds=await reading.getByRole('button',{name:'Save my reading settings',exact:true}).boundingBox();assert.ok(saveBounds.y>=0&&saveBounds.y+saveBounds.height<=951,'Save is fully reachable without clipping');
    await reading.getByRole('button',{name:'Cancel',exact:true}).click();
    await row.getByRole('button',{name:'More actions for Request 1',exact:true}).click();await page.getByRole('menuitem',{name:'Send record',exact:true}).click();await send.waitFor();
    const sendBounds=await send.boundingBox();assert.ok(sendBounds.x>=0&&sendBounds.x+sendBounds.width<=391);
    assert.ok(await send.evaluate(node=>node.scrollWidth<=node.clientWidth+1),'Send dialog fits enlarged text');
    await page.screenshot({path:join(mx.directory,`${engine}-send-mobile-200.png`)});
    await page.keyboard.press('Escape');await send.waitFor({state:'hidden'});
    await page.setViewportSize({width:1440,height:950});await page.evaluate(()=>document.documentElement.style.fontSize='125%');

    await page.goto(`${origin}/#admin`);await page.getByRole('button',{name:'Modules & fields',exact:true}).click();
    await page.getByRole('button',{name:'Configure Requests',exact:true}).waitFor();
    assert.equal(await page.locator('.module-builder').count(),0,'No implicit first-module editing');
    await page.getByRole('button',{name:'New module',exact:true}).click();
    const create=page.getByRole('dialog',{name:'Create a separate module',exact:true});await create.getByLabel('Module name',{exact:true}).fill(`Separate ${engine}`);
    await create.getByRole('button',{name:'Create new module',exact:true}).click();await create.waitFor({state:'hidden'});
    await page.locator('.module-workbench-scope').getByRole('heading',{name:`Separate ${engine}`,exact:true}).waitFor();
    assert.equal((await call('/mx/v1/modules',owner)).modules.find(item=>item.uid===module.uid).name,'Requests','Creating did not edit the original');
    await page.getByRole('button',{name:'All modules',exact:true}).click();await page.getByRole('button',{name:'Configure Requests',exact:true}).click();
    const add=page.getByRole('form',{name:'Add field',exact:true});await add.getByLabel('Label',{exact:true}).fill('Unfinished field');
    await page.getByRole('button',{name:'All modules',exact:true}).click();
    const confirmation=page.getByRole('alertdialog');await confirmation.waitFor();await confirmation.getByRole('button',{name:'Cancel',exact:true}).click();
    assert.equal(await add.getByLabel('Label',{exact:true}).inputValue(),'Unfinished field','Draft retained after cancelling module switch');
    await add.getByLabel('Label',{exact:true}).fill('');
    await page.getByRole('navigation',{name:'Module configuration sections',exact:true}).getByRole('button',{name:'Module details',exact:true}).click();
    assert.equal(await page.locator('.module-permissions').count(),0,'Details and permissions are separate screens');
    await page.getByLabel('Name',{exact:true}).fill('Unsaved module name');
    await page.getByRole('button',{name:'All modules',exact:true}).click();await confirmation.waitFor();await confirmation.getByRole('button',{name:'Cancel',exact:true}).click();
    assert.equal(await page.getByLabel('Name',{exact:true}).inputValue(),'Unsaved module name');
    await page.getByRole('button',{name:'Discard',exact:true}).click();
    for(const [theme,width,font]of[['light',1440,20],['dark',390,20],['light',1440,32],['dark',390,32]]){
      await page.setViewportSize({width,height:950});
      await page.evaluate(({theme,font})=>{document.documentElement.dataset.theme=theme;document.documentElement.style.fontSize=`${font/16*100}%`;},{theme,font});
      await page.evaluate(()=>scrollTo(0,0));
      await page.waitForTimeout(300); // Finish responsive drawer/control transitions before measuring.
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),`${engine}: admin fits ${width} at ${font}px`);
      await page.screenshot({path:join(mx.directory,`${engine}-module-details-${theme}-${width}-${font}.png`)});
    }
    const moreTools=page.getByRole('button',{name:'More workspace actions',exact:true});await moreTools.click();
    const toolsMenu=page.getByRole('menu',{name:'Workspace actions',exact:true});await toolsMenu.waitFor();
    await page.keyboard.press('End');assert.ok(await toolsMenu.getByRole('menuitem',{name:'Sign out',exact:true}).evaluate(node=>node===document.activeElement));
    await page.keyboard.press('Escape');await toolsMenu.waitFor({state:'hidden'});assert.ok(await moreTools.evaluate(node=>node===document.activeElement));
    assert.deepEqual(errors,[]);
    results.browsers[engine]='Read-only cells/text selection; full-form save; personal field emphasis/form/reload; direct message/context/notification; two lost acknowledgments + deduplicated retry; real enlarged text; separate module creation and dirty guards; light/dark, desktop/mobile, 200% text';
    console.log(`${engine}: record workspace passed`);await browser.close();
  }
  // Sending never grants access, even when a recipient cannot open the record.
  const restrictedChannel=await call('/mx/v1/collaboration/direct',owner,'POST',{user_uid:users[1].uid},[200,201]);
  await call(`/mx/v1/collaboration/channels/${restrictedChannel.uid}/messages`,owner,'POST',{body:'Restricted reference',record_uids:[record.uid],operation_uid:randomUUID()},[201]);
  const restricted=await call(`/mx/v1/collaboration/channels/${restrictedChannel.uid}/messages`,users[1]);
  assert.deepEqual(restricted.messages[0].record_links,[],'Unauthorized recipient cannot see record metadata');
  await call(paths,users[1],'GET',undefined,[403]);
  const restrictedAfter=await call(`/mx/v1/admin/accounts/${users[1].uid}/modules`,owner);
  assert.equal(restrictedAfter.revision,restrictedBefore.revision,'Sending did not change access revision');
  assert.deepEqual(restrictedAfter.modules.find(item=>item.module_uid===module.uid),restrictedBefore.modules.find(item=>item.module_uid===module.uid),'Target module grant is unchanged');
  assert.ok(restrictedAfter.modules.every(item=>!item.grant.can_read),'Even newly created test modules remain inaccessible');
  results.api.push('Sharing to a restricted recipient leaves access unchanged and redacts the card');
  await writeFile(join(mx.directory,'record-workspace-results.json'),JSON.stringify(results,null,2));
  console.log(JSON.stringify(results,null,2));
}catch(error){if(activePage&&!activePage.isClosed()){
  await activePage.screenshot({path:join(mx.directory,'workspace-failure.png')}).catch(()=>{});
  console.log(await activePage.evaluate(()=>({width:innerWidth,scroll:document.documentElement.scrollWidth,overflow:[...document.querySelectorAll('body *')].filter(node=>{const b=node.getBoundingClientRect();return b.right>innerWidth+2&&b.width>0&&b.left>=0&&getComputedStyle(node).position!=='fixed';}).slice(0,30).map(node=>({tag:node.tagName,class:node.className,width:node.getBoundingClientRect().width,right:node.getBoundingClientRect().right,text:node.textContent?.slice(0,80)}))})))
}throw error;}
finally{await Promise.all(browsers.map(browser=>browser.close().catch(()=>{})));await mx.close();}
