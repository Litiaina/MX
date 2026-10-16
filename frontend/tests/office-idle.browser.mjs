// Real bundled Office + disposable MX/N1 contract fixture. No production data,
// public CDN, host Office service or physical audio devices.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createServer} from 'node:http';
import {request as httpsRequest} from 'node:https';
import {chromium,firefox} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {officeFixture,unzip} from './office-fixtures.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/tests'));assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url))),browsers=[],results=[];
const proxy=createServer((incoming,outgoing)=>{const upstream=httpsRequest({hostname:'127.0.0.1',port:mx.port,path:incoming.url,method:incoming.method,headers:incoming.headers,rejectUnauthorized:false},response=>{outgoing.writeHead(response.statusCode,response.headers);response.pipe(outgoing);});upstream.on('error',()=>outgoing.destroy());incoming.pipe(upstream);});
await new Promise(resolve=>proxy.listen(0,'127.0.0.1',resolve));const origin=`http://127.0.0.1:${proxy.address().port}`;
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
let inspected;
async function until(check,label,timeout=90000){const deadline=Date.now()+timeout;while(Date.now()<deadline){if(await check())return;await new Promise(resolve=>setTimeout(resolve,100));}throw Error(`Timeout: ${label}`);}
try{
 console.log(`Office idle-save artifacts: ${mx.directory}`);
 const password=`Test-only!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'idle@test.invalid',name:'Office Idle Test',password},[201]);
 const user=await call('/mx/v1/auth/authenticate',null,'POST',{email:'idle@test.invalid',password});
 for(const name of (process.env.MX_TEST_BROWSERS||'chromium,brave,firefox').split(',')){
  const type=name==='firefox'?firefox:chromium;
  const executablePath=name==='brave'?'/usr/bin/brave-browser':name==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome';
  const browser=await type.launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(name!=='firefox'?{args:['--no-sandbox','--mute-audio']}: {})});browsers.push(browser);
  const context=await browser.newContext({viewport:{width:1280,height:900},serviceWorkers:'block'});
  await context.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},user);
  await context.route('**/office/office-thread.js',async route=>{const response=await route.fetch();let source=await response.text();source=source.replace('model.storeToURL(`file:///tmp/mx-office/export','console.info("MX_TEST_EXPORT",data.acceptInput!==false); model.storeToURL(`file:///tmp/mx-office/export');await route.fulfill({response,body:source});});
  const operation=randomUUID(),bytes=officeFixture('.xlsx');
  await call('/mx/v1/drive/uploads',user,'POST',{operation_uid:operation,file_name:`Idle-${name}.xlsx`,mime_type:'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',size:bytes.length,last_modified:1});
  for(let attempt=0;attempt<3;attempt++){const response=await mx.api.put(`/mx/v1/drive/uploads/${operation}/parts/0`,{headers:{Authorization:`Bearer ${user.access_token}`,'Content-Type':'application/octet-stream'},data:bytes});if(response.status()===502&&attempt<2)continue;assert.equal(response.status(),200);break;}
  const item=await call(`/mx/v1/drive/uploads/${operation}/finish`,user,'POST');
  const page=await context.newPage();inspected=page;const errors=[],exports=[];
  page.on('pageerror',error=>errors.push(error.message));page.on('console',event=>{if(event.text().startsWith('MX_TEST_EXPORT'))exports.push({manual:event.text().endsWith('true'),at:Date.now()});});
  await page.goto(`${origin}/office/index.html#file=${item.uid}`);
  const canvas=page.locator('#qtcanvas');await until(()=>canvas.getAttribute('data-native-focused').then(value=>value==='true'),'real spreadsheet ready');
  await canvas.focus();await page.keyboard.press('Control+Home',{delay:80});
  for(let index=0;index<3;index++)await page.keyboard.press('ArrowDown',{delay:80});
  await until(()=>page.getByLabel('Selected cell',{exact:true}).innerText().then(value=>value==='A4'),'native target-cell selection acknowledged');
  const marker=`MX_IDLE_${name}_${randomUUID().slice(0,8)}`;
  await page.keyboard.type(marker,{delay:35});
  await page.waitForTimeout(17_000);
  assert.equal(exports.length,0,'Idle preparation does not accept an unfinished cell');
  assert.equal((await call(`/mx/v1/drive/items/${item.uid}`,user)).revision,1);
  await page.keyboard.type('_FINISHED',{delay:35});await page.keyboard.press('Enter',{delay:80});
  // Wheel activity and a held scrollbar/selection drag must count as editing,
  // even though the document model itself is not changing.
  const box=await canvas.boundingBox();await page.mouse.move(box.x+400,box.y+350);
  for(let index=0;index<5;index++){await page.mouse.wheel(0,index%2?100:-100);await page.waitForTimeout(3500);}
  assert.equal(exports.length,0,'No automatic export while scrolling');
  await page.mouse.down();await page.waitForTimeout(17_000);assert.equal(exports.length,0,'No export during a held pointer drag');await page.mouse.up();
  // Composition can include long periods without a keydown or a model change.
  await canvas.dispatchEvent('compositionstart',{data:'test'});await page.waitForTimeout(17_000);assert.equal(exports.length,0,'No export while text composition remains active');await canvas.dispatchEvent('compositionend',{data:'test'});
  // End the synthetic composition through an explicit save. It must still
  // accept the current cell, store its exact recovery payload and confirm MX.
  await page.keyboard.press('Control+s');
  await until(async()=> (await call(`/mx/v1/drive/items/${item.uid}`,user)).revision===2,'explicit save after interaction');
  await until(()=>page.locator('.save-feedback').getAttribute('title').then(value=>value?.includes('Saved to MX · revision 2 ·')),'confirmed save feedback');
  let download=await mx.api.get(`/mx/v1/drive/items/${item.uid}/download`,{headers:{Authorization:`Bearer ${user.access_token}`}});
  const firstEntries=unzip(await download.body());
  await writeFile(join(mx.directory,`${name}-first-saved-strings.xml`),firstEntries['xl/sharedStrings.xml']||firstEntries['xl/worksheets/sheet1.xml']);
  assert.ok(JSON.stringify(firstEntries).includes(marker+'_FINISHED'),'Unfinished cell survived the idle pause and continued typing, including shared-string exports');
  await canvas.focus();await page.keyboard.press('Control+Home',{delay:80});
  await until(()=>page.getByLabel('Selected cell',{exact:true}).innerText().then(value=>value==='A1'),'native edit target acknowledged before automatic-save typing');
  await page.keyboard.type(marker+'_AUTO',{delay:35});await page.keyboard.press('Enter',{delay:80});
  await until(()=>page.getByLabel('Selected cell',{exact:true}).innerText().then(value=>value==='A2'),'Enter has committed and advanced the native selection');
  const selected=await page.getByLabel('Selected cell',{exact:true}).innerText();
  await until(async()=> (await call(`/mx/v1/drive/items/${item.uid}`,user)).revision===3,'automatic save after genuine idle/cooldown',60000);
  await until(()=>page.locator('.save-feedback').getAttribute('title').then(value=>value?.includes('Saved to MX · revision 3 ·')),'automatic confirmation');
  assert.ok(exports.some(value=>!value.manual),'Automatic preparation uses the non-accepting native path');
  assert.equal(await page.getByLabel('Selected cell',{exact:true}).innerText(),selected,'Background preparation does not reselect the cell');
  assert.equal(await page.evaluate(()=>document.activeElement?.id),'qtcanvas','Background save does not steal browser focus');
  download=await mx.api.get(`/mx/v1/drive/items/${item.uid}/download`,{headers:{Authorization:`Bearer ${user.access_token}`}});
  const entries=unzip(await download.body());await writeFile(join(mx.directory,`${name}-auto-saved-strings.xml`),entries['xl/sharedStrings.xml']||entries['xl/worksheets/sheet1.xml']);assert.ok(JSON.stringify(entries).includes(marker+'_AUTO'),`Saved model contains the entire automatic edit: ${entries['xl/sharedStrings.xml']}`);assert.match(entries['xl/worksheets/sheet1.xml'],/A2\*B2/);
  // Holding only the network phase must not prevent editing. An explicit
  // Ctrl+S during that background upload must preserve the newest cell input.
  let releasePart;const partGate=new Promise(resolve=>releasePart=resolve);
  let held=false;const holdPart=async route=>{if(!held){held=true;await partGate;}await route.continue().catch(()=>{});};
  await context.route('**/uploads/*/parts/*',holdPart);
  await canvas.focus();await page.keyboard.press('Control+Home',{delay:80});
  await until(()=>page.getByLabel('Selected cell',{exact:true}).innerText().then(value=>value==='A1'),'native target before next background edit');
  await page.keyboard.type(marker+'_BACKGROUND',{delay:35});await page.keyboard.press('Enter',{delay:80});
  await until(()=>held,'automatic upload reaches the held network phase',60000);
  await canvas.focus();await page.keyboard.press('Control+Home',{delay:80});
  await until(()=>page.getByLabel('Selected cell',{exact:true}).innerText().then(value=>value==='A1'),'native target while background upload is held');
  await page.keyboard.type(marker+'_LATEST',{delay:35});await page.keyboard.press('Control+s');
  releasePart();await until(async()=> (await call(`/mx/v1/drive/items/${item.uid}`,user)).revision===5,'manual save queued behind the automatic upload',60000);
  await until(()=>page.locator('.save-feedback').getAttribute('title').then(value=>value?.includes('Saved to MX · revision 5 ·')),'queued manual confirmation');
  await context.unroute('**/uploads/*/parts/*',holdPart);
  download=await mx.api.get(`/mx/v1/drive/items/${item.uid}/download`,{headers:{Authorization:`Bearer ${user.access_token}`}});
  assert.ok(JSON.stringify(unzip(await download.body())).includes(marker+'_LATEST'),'Queued explicit Save includes the latest unfinished cell, not the older automatic snapshot');
  assert.deepEqual(errors,[]);await page.screenshot({path:join(mx.directory,`${name}-idle-office.png`)});
  results.push({browser:name,unfinished_input_preserved:true,wheel_and_drag_deferred:true,composition_deferred:true,manual_save:true,automatic_save:true,manual_save_during_background_upload:true,selection_and_focus_preserved:true,exports});
  console.log(`${name}: real idle, cell input, scrolling, drag, composition and confirmed saves passed`);
  await browser.close();browsers.splice(browsers.indexOf(browser),1);
 }
 await writeFile(join(mx.directory,'office-idle-results.json'),JSON.stringify({passed:true,results,limits:'Whole-document native export is not preemptible; this does not certify physical power loss or real Safari.'},null,2));
}catch(error){if(inspected&&!inspected.isClosed())await inspected.screenshot({path:join(mx.directory,'office-idle-failed.png')}).catch(()=>{});throw error;}
finally{for(const browser of browsers)await browser.close();proxy.closeAllConnections();await new Promise(resolve=>proxy.close(resolve));await mx.close();}
