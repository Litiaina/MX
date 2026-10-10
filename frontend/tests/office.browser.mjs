// Real bundled WebAssembly engine, real MX auth/SQLite and N1 contract storage.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { writeFile } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';
import { DatabaseSync } from 'node:sqlite';
import { startIsolatedMx } from './isolated-mx.mjs';
import { officeFixture, unzip } from './office-fixtures.mjs';
import { createServer } from 'node:http';
import { request as httpsRequest } from 'node:https';
const root=fileURLToPath(new URL('../',import.meta.url)),mx=await startIsolatedMx(root);
// Localhost is a browser secure context. A test-only loopback proxy avoids
// installing a test CA on the user's PC; deployed LAN Office still needs HTTPS.
let serverReachable=true;
const proxy=createServer((incoming,outgoing)=>{if(!serverReachable){incoming.socket.destroy();return;}const upstream=httpsRequest({hostname:'127.0.0.1',port:mx.port,path:incoming.url,method:incoming.method,headers:incoming.headers,rejectUnauthorized:false},response=>{outgoing.writeHead(response.statusCode,response.headers);response.pipe(outgoing);});upstream.on('error',()=>{outgoing.writeHead(502);outgoing.end();});incoming.pipe(upstream);});
await new Promise(resolve=>proxy.listen(0,'127.0.0.1',resolve));const origin=`http://127.0.0.1:${proxy.address().port}`;
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const base='/mx/v1/drive';
const browserNames=(process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',');
async function upload(user,name,bytes,extra={}){
  const operation_uid=randomUUID(),state=await call(`${base}/uploads`,user,'POST',{operation_uid,file_name:name,mime_type:'application/octet-stream',size:bytes.length,last_modified:Date.now(),...extra});
  for(let index=0;index<Math.max(1,Math.ceil(bytes.length/state.part_size));index++){
    let response;for(let attempt=0;attempt<3;attempt++){response=await mx.api.put(`${base}/uploads/${operation_uid}/parts/${index}`,{headers:{Authorization:`Bearer ${user.access_token}`,'Content-Type':'application/octet-stream'},data:bytes.subarray(index*state.part_size,(index+1)*state.part_size)});if(response.status()!==502)break;}assert.equal(response.status(),200,await response.text());
  }return call(`${base}/uploads/${operation_uid}/finish`,user,'POST');
}
async function wait(check,label,timeout=120000){const end=Date.now()+timeout;while(Date.now()<end){if(await check())return;await new Promise(resolve=>setTimeout(resolve,200));}throw new Error(`Timeout: ${label}`);}
const downloadButton=page=>page.getByRole('button',{name:'Download copy',exact:true,includeHidden:true});
async function openOptions(page){if(await page.getByRole('button',{name:'Document options',exact:true}).getAttribute('aria-expanded')!=='true')await page.getByRole('button',{name:'Document options',exact:true}).click();}
async function closeOptions(page){if(await page.getByRole('button',{name:'Document options',exact:true}).getAttribute('aria-expanded')==='true')await page.getByRole('button',{name:'Close document options',exact:true}).click();}
async function downloadCopy(page){await openOptions(page);await downloadButton(page).click();}
function rows(sql,...args){const db=new DatabaseSync(join(mx.directory,'test.db'));try{return db.prepare(sql).all(...args);}finally{db.close();}}
async function localDraft(page,account,item){return page.evaluate(async({account,item})=>{const database=await new Promise((resolve,reject)=>{const request=indexedDB.open('mx-office-local-drafts',1);request.onsuccess=()=>resolve(request.result);request.onerror=()=>reject(request.error);});try{const draft=await new Promise((resolve,reject)=>{const request=database.transaction('drafts').objectStore('drafts').get(`${account}:${item}`);request.onsuccess=()=>resolve(request.result);request.onerror=()=>reject(request.error);});return draft?{...draft,blob:Array.from(new Uint8Array(await draft.blob.arrayBuffer())),saveFile:!!draft.saveFile}:null;}finally{database.close();}},{account,item});}
async function editedContents(uid,user){const response=await mx.api.get(`${base}/items/${uid}/download`,{headers:{Authorization:`Bearer ${user.access_token}`}});assert.equal(response.status(),200);return unzip(await response.body());}
async function typeDocument(page,text){await page.locator('#qtcanvas').focus();await page.keyboard.press('Control+End',{delay:80});await page.keyboard.press('Enter',{delay:80});await page.keyboard.type(text,{delay:30});await page.waitForTimeout(300);}
const launched=[],browserResults=[];
let inspectedPage;
try{
  console.log(`Office test artifacts: ${mx.directory}`);
  const password=`Test-only!${randomUUID()}`;
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'office-admin@test.invalid',name:'Office Admin',password},[201]);
  const admin=await call('/mx/v1/auth/authenticate',null,'POST',{email:'office-admin@test.invalid',password});
  await call('/mx/v1/user/create',admin,'POST',{email:'office-viewer@test.invalid',name:'Office Viewer',password,access_level:3},[201]);
  const viewer=await call('/mx/v1/auth/authenticate',null,'POST',{email:'office-viewer@test.invalid',password});
  const editors=[];
  for(let i=0;i<8;i++){const email=`office-editor-${i}@test.invalid`;await call('/mx/v1/user/create',admin,'POST',{email,name:`Office Editor ${i}`,password,access_level:2},[201]);editors.push(await call('/mx/v1/auth/authenticate',null,'POST',{email,password}));}
  const files={};for(const extension of ['.docx','.xlsx','.pptx'])files[extension]=await upload(admin,`Test${extension}`,officeFixture(extension));
  await call(`${base}/items/${files['.docx'].uid}/office`,viewer,'GET',undefined,[404]);
  await call(`${base}/items/${files['.docx'].uid}/sharing`,admin,'POST',{user_uid:viewer.uid,role:'viewer'});
  const race=await upload(admin,'Eight-user-save.docx',officeFixture('.docx'));
  const prepared=[];
  for(const editor of editors){await call(`${base}/items/${race.uid}/sharing`,admin,'POST',{user_uid:editor.uid,role:'editor'});const operation_uid=randomUUID(),bytes=officeFixture('.docx');await call(`${base}/uploads`,editor,'POST',{operation_uid,item_uid:race.uid,base_revision:1,file_name:'Eight-user-save.docx',mime_type:'application/octet-stream',size:bytes.length,last_modified:Date.now()});let response;for(let attempt=0;attempt<3;attempt++){response=await mx.api.put(`${base}/uploads/${operation_uid}/parts/0`,{headers:{Authorization:`Bearer ${editor.access_token}`,'Content-Type':'application/octet-stream'},data:bytes});if(response.status()!==502)break;}assert.equal(response.status(),200);prepared.push({editor,operation_uid});}
  const commits=await Promise.all(prepared.map(({editor,operation_uid})=>mx.call(`${base}/uploads/${operation_uid}/finish`,editor.access_token,'POST',undefined,[200,409])));
  assert.equal(commits.filter(result=>result.status===200).length,1);assert.equal(commits.filter(result=>result.status===409).length,7);assert.equal(rows('SELECT COUNT(*) n FROM mx_drive_versions WHERE item_uid=?',race.uid)[0].n,2);
  console.log('Eight simultaneous Office-version commits: one winner, seven conflicts, original version retained');
  assert.equal((await mx.api.get('/office/index.html')).headers()['cross-origin-opener-policy'],'same-origin');
  assert.equal((await mx.api.get('/')).headers()['cross-origin-opener-policy'],undefined);
  const manifest=await(await mx.api.get('/office/precache.json')).json();
  assert.ok((await(await mx.api.get('/office/offline-worker.js')).text()).includes(`// MX build generation: ${manifest.cache}`),'Every asset generation must change the served worker script so cached clients receive updates');
  const gzip=await mx.api.head('/office/vendor/soffice.wasm',{headers:{'Accept-Encoding':'gzip'}});
  assert.equal(gzip.status(),200);assert.equal(gzip.headers()['content-encoding'],'gzip');assert.equal(gzip.headers()['content-type'],'application/wasm');
  for(const name of browserNames){
    const document=await upload(admin,`Test-${name}.docx`,officeFixture('.docx'));
    const type={chromium,firefox,webkit}[name],options={headless:true};
    if(name==='chromium'){options.executablePath=process.env.MX_TEST_CHROMIUM||'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome';options.args=['--mute-audio'];}
    if(name==='firefox')options.executablePath=process.env.MX_TEST_FIREFOX||'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox';
    if(name==='webkit'&&process.env.MX_TEST_WEBKIT)options.executablePath=process.env.MX_TEST_WEBKIT;
    const browser=await type.launch(options);launched.push(browser);
    const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}});
    await context.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},admin);
    const unexpected=[],errors=[];
    await context.route('**/*',route=>{const url=new URL(route.request().url());if(['https:','http:'].includes(url.protocol)&&url.origin!==origin){unexpected.push(url.href);return route.abort();}return route.continue();});
    if(process.env.MX_TEST_OFFICE_GRAPHICS_FALLBACK){
      const fallback=await context.newPage();inspectedPage=fallback;fallback.on('pageerror',error=>errors.push(error.message));
      const engineRequests=[];fallback.on('request',request=>{if(request.url().includes('/vendor/soffice'))engineRequests.push(request.url());});
      await fallback.goto(`${origin}/office/index.html#file=${document.uid}`);
      await fallback.getByText('This browser or graphics driver cannot provide WebGL in an Office worker.',{exact:false}).first().waitFor();
      assert.equal(await fallback.getByRole('button',{name:'Save to MX',exact:true}).isDisabled(),true);
      const downloadEvent=fallback.waitForEvent('download');await downloadCopy(fallback);
      const stream=await(await downloadEvent).createReadStream();const chunks=[];for await(const chunk of stream)chunks.push(chunk);
      assert.deepEqual(Buffer.concat(chunks),officeFixture('.docx'));assert.equal((await call(`${base}/items/${document.uid}`,admin)).revision,1);
      assert.deepEqual(errors,[]);assert.deepEqual(engineRequests,[]);assert.equal(unexpected.length,0);
      await fallback.screenshot({path:join(mx.directory,`${name}-office-graphics-fallback.png`)});
      browserResults.push({browser:name,result:'graphics fallback only; Office editing not supported in this runtime'});
      console.log(`${name}: unsupported worker graphics detected before engine load, unchanged original downloads successfully`);
      await context.close();await browser.close();launched.splice(launched.indexOf(browser),1);continue;
    }
    if(process.env.MX_TEST_OFFICE_SHELL_ONLY){
      const editor=await context.newPage();inspectedPage=editor;
      editor.on('pageerror',error=>errors.push(error.message));editor.on('dialog',dialog=>dialog.accept());
      await editor.goto(`${origin}/office/index.html#file=${document.uid}`);
      await wait(async()=>{if(errors.length)throw new Error(errors.join('; '));if(await editor.locator('.alert').count())throw new Error(await editor.locator('.alert').innerText());return downloadButton(editor).isEnabled();},'compact shell opens real DOCX');
      await wait(async()=>(await editor.locator('#qtcanvas').getAttribute('data-native-focused'))==='true','compact shell native focus ready');
      for(const [theme,width] of [['light',1280],['dark',1280],['dark',390]]){
        await editor.evaluate(value=>document.documentElement.dataset.theme=value,theme);await editor.setViewportSize({width,height:900});await editor.waitForTimeout(500);
        const stage=await editor.locator('.editor-stage').boundingBox(),toolbar=await editor.locator('.toolbar').boundingBox();
        assert.equal(toolbar.height,38,'MX must use one compact, non-wrapping row');
        assert.equal(stage.y,38);assert.equal(stage.height,862,'The rest of the viewport belongs to the native editor');
        assert.ok(await editor.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
        assert.equal(await editor.locator('footer,.offline-toolbar,.narrow-hint').count(),0);
        assert.equal(await editor.locator('.document-options').isVisible(),false);
        assert.equal(await editor.getByRole('checkbox',{name:'Keep offline copy on this device'}).count(),0);
        assert.equal(await editor.getByRole('button',{name:'Download copy',exact:true}).count(),0);
        await openOptions(editor);
        assert.equal(await editor.getByRole('dialog',{name:'Document options'}).isVisible(),true);
        await wait(()=>editor.getByRole('button',{name:'Open from Drive',exact:true}).evaluate(button=>button===document.activeElement),'Options are keyboard reachable',5000);
        assert.deepEqual(await editor.locator('.editor-stage').boundingBox(),stage,'Opening settings must not resize the native canvas');
        assert.ok((await editor.locator('.document-options').boundingBox()).x>=0,'Options must fit narrow screens');
        await editor.screenshot({path:join(mx.directory,`${name}-office-compact-menu-${theme}-${width}.png`)});
        await editor.keyboard.press('Escape');
        assert.equal(await editor.locator('.document-options').isVisible(),false);
        assert.equal(await editor.getByRole('button',{name:'Document options',exact:true}).evaluate(button=>button===document.activeElement),true);
        if(width<700){
          await openOptions(editor);await editor.getByRole('button',{name:'Scroll editor right',exact:true}).click();
          assert.equal(await editor.locator('.editor-stage').evaluate(stage=>stage.scrollLeft),240);await closeOptions(editor);
        }else await editor.locator('.editor-stage').evaluate(stage=>stage.scrollLeft=0);
        await editor.screenshot({path:join(mx.directory,`${name}-office-compact-${theme}-${width}.png`)});
      }
      await editor.setViewportSize({width:1280,height:900});await editor.locator('.editor-stage').evaluate(stage=>stage.scrollLeft=0);
      await editor.waitForTimeout(500);await openOptions(editor);
      await editor.locator('#qtcanvas').click({position:{x:600,y:350}});
      assert.equal(await editor.locator('.document-options').isVisible(),false,'Clicking the editor closes optional controls');
      await wait(async()=>(await editor.locator('#qtcanvas').getAttribute('data-native-focused'))==='true','native focus after menu dismissal');
      const marker=`MX ${name} compact editor saved`;
      await typeDocument(editor,marker);
      await wait(()=>editor.getByRole('button',{name:'Save to MX',exact:true}).isEnabled(),'compact shell edit is unsaved');
      assert.equal(await editor.locator('.save-feedback').getAttribute('title'),'Changes not yet saved to MX');
      assert.equal((await editor.locator('.editor-stage').boundingBox()).height,862,'Dirty feedback must not resize the editor');
      let releaseSave,finishWaiting=false;const saveGate=new Promise(resolve=>releaseSave=resolve);
      const delayedFinish=async route=>{finishWaiting=true;await saveGate;await route.continue();};
      await context.route('**/uploads/*/finish',delayedFinish);
      try{
        await editor.keyboard.press('Control+s');await wait(()=>finishWaiting,'compact Ctrl+S reaches real version commit');
        assert.equal(await editor.getByRole('button',{name:'Saving…',exact:true}).isDisabled(),true);
        assert.equal(await editor.locator('.save-feedback.saved').count(),0);
        assert.equal((await call(`${base}/items/${document.uid}`,admin)).revision,1);
        assert.equal((await editor.locator('.editor-stage').boundingBox()).height,862,'Saving feedback must not resize the editor');
        releaseSave();await wait(()=>editor.locator('.save-feedback.saved').isVisible(),'compact server acknowledgement');
      }finally{releaseSave();await context.unroute('**/uploads/*/finish',delayedFinish);}
      assert.match(await editor.locator('.save-feedback').getAttribute('title'),/Saved to MX · revision 2 · /);
      assert.match((await editedContents(document.uid,admin))['word/document.xml'],new RegExp(marker),'Verify actual exported input, not only a successful commit');
      assert.equal(rows('SELECT COUNT(DISTINCT object_key) n FROM mx_drive_versions WHERE item_uid=?',document.uid)[0].n,2);
      const downloadEvent=editor.waitForEvent('download');await downloadCopy(editor);
      const stream=await(await downloadEvent).createReadStream(),chunks=[];for await(const chunk of stream)chunks.push(chunk);
      assert.match(unzip(Buffer.concat(chunks))['word/document.xml'],new RegExp(marker));
      assert.equal(await editor.locator('.document-options').isVisible(),false);
      await openOptions(editor);await editor.getByRole('checkbox',{name:'Keep offline copy on this device'}).check();
      await wait(async()=>!!await localDraft(editor,admin.uid,document.uid),'compact menu keeps offline retention functional');
      assert.equal((await editor.locator('.editor-stage').boundingBox()).height,862,'Offline options must not resize the editor');
      await closeOptions(editor);await editor.screenshot({path:join(mx.directory,`${name}-office-compact-saved.png`)});
      if(name==='chromium'){
        const tlsContext=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}}),tls=await tlsContext.newPage();inspectedPage=tls;
        await tls.goto(`https://127.0.0.1:${mx.port}/office/index.html`);
        await wait(async()=>await tls.locator('.offline-details').count()===1,'genuine TLS error available inside options',15000);
        assert.equal(await tls.locator('.offline-details').isVisible(),false);
        assert.equal((await tls.locator('.toolbar').boundingBox()).height,38);
        await openOptions(tls);await tls.getByText('Offline reopening unavailable · details and retry',{exact:true}).click();
        await tls.getByText('Offline reopening is unavailable because the browser does not trust MX’s HTTPS certificate.',{exact:false}).waitFor();
        assert.equal(await tls.getByRole('button',{name:'Retry offline preparation',exact:true}).isVisible(),true);
        await tls.screenshot({path:join(mx.directory,`${name}-office-compact-tls-options.png`)});await tlsContext.close();inspectedPage=editor;
      }
      assert.deepEqual(errors,[]);assert.deepEqual(unexpected,[]);
      browserResults.push({browser:name,result:'compact shell only: real DOCX edit/save/download, menu/offline retention, fixed geometry, light/dark/narrow',presentationEditing:false});
      console.log(`${name}: compact shell, actual Ctrl+S/export/version/download, offline menu and unchanged canvas geometry pass`);
      await context.close();await browser.close();launched.splice(launched.indexOf(browser),1);continue;
    }
    if(!process.env.MX_TEST_OFFICE_FORMATS_ONLY){
    const workspace=await context.newPage();await workspace.goto(`${origin}/#drive`);await workspace.getByRole('heading',{name:'MX Drive'}).waitFor();
    const popupPromise=context.waitForEvent('page');await workspace.getByRole('button',{name:`Edit ${document.name} in MX Office`,exact:true}).click();const office=await popupPromise;
    inspectedPage=office;
    office.on('pageerror',error=>{errors.push(error.message);console.log(`${name} JS error: ${error.message}`);});office.on('console',message=>{console.log(`${name} console: ${message.text().slice(0,400)}`);});
    office.on('dialog',dialog=>dialog.accept());
    const diagnostic=setTimeout(()=>void office.evaluate(()=>({calledRun:window.Module?.calledRun,main:typeof window.Module?.uno_main,scripts:window.Module?.uno_scripts,dependencies:window.runDependencies,workers:window.PThread?.runningWorkers?.length,body:document.body.innerText})).then(result=>console.log('Boot diagnostic:',JSON.stringify(result))).catch(()=>{}),10000);
    try{await wait(async()=>{if(errors.length)throw new Error(errors.join('; '));const alert=office.locator('.alert:not(.warning)');if(await alert.count())throw new Error(await alert.innerText());return downloadButton(office).isEnabled();},'real engine opened DOCX');}catch(error){await office.screenshot({path:join(mx.directory,`${name}-boot-failed.png`)});console.log(await office.locator('body').innerText());throw error;}
    assert.equal(await office.evaluate(()=>crossOriginIsolated),true);
    clearTimeout(diagnostic);
    await office.waitForTimeout(1500);
    await office.screenshot({path:join(mx.directory,`${name}-office-docx.png`)});
    for(const [theme,width] of [['dark',1280],['dark',390],['light',1280]]){
      await office.evaluate(value=>document.documentElement.dataset.theme=value,theme);await office.setViewportSize({width,height:900});
      await office.waitForTimeout(500);assert.ok(await office.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
      if(width<700){
        assert.ok((await office.locator('#qtcanvas').boundingBox()).width>=960,'Native toolbar must scroll, not be horizontally distorted');
        assert.ok(await office.locator('.editor-stage').evaluate(stage=>stage.scrollWidth>stage.clientWidth));
        await openOptions(office);await office.getByRole('button',{name:'Scroll editor right',exact:true}).click();await closeOptions(office);
        assert.equal(await office.locator('.editor-stage').evaluate(stage=>stage.scrollLeft),240);
      }else await office.locator('.editor-stage').evaluate(stage=>stage.scrollLeft=0);
      await office.screenshot({path:join(mx.directory,`${name}-office-shell-${theme}-${width}.png`)});
    }
    assert.equal(unexpected.length,0,'Office must work with every external request blocked');
    assert.deepEqual(errors,[]);
    console.log(`${name}: real bundled Office engine opens DOCX without internet`);
    await office.locator('#qtcanvas').focus();await office.keyboard.press('Control+End');await office.keyboard.press('Enter');await office.keyboard.type(`MX ${name} edited`,{delay:30});
    await wait(()=>office.getByRole('button',{name:'Save to MX',exact:true}).isEnabled(),'UI edit makes document dirty',10000);
    await office.screenshot({path:join(mx.directory,`${name}-office-edited.png`)});
    await office.getByRole('button',{name:'Save to MX',exact:true}).click();
    await wait(async()=>{const record=await call(`${base}/items/${document.uid}`,admin);return record.revision===2;},'Office save creates revision 2');
    const download=await mx.api.get(`${base}/items/${document.uid}/download`,{headers:{Authorization:`Bearer ${admin.access_token}`}});assert.equal(download.status(),200);
    const saved=await download.body();assert.match(unzip(saved)['word/document.xml'],new RegExp(`MX ${name} edited`));
    assert.equal(rows('SELECT COUNT(*) n FROM mx_drive_versions WHERE item_uid=?',document.uid)[0].n,2);
    assert.equal(rows('SELECT COUNT(DISTINCT object_key) n FROM mx_drive_versions WHERE item_uid=?',document.uid)[0].n,2);
    console.log(`${name}: real keyboard edit saved to a new N1 object with readable DOCX contents`);
    await wait(()=>office.getByRole('button',{name:'Save to MX',exact:true}).isDisabled(),'saved document clean',10000);
    await openOptions(office);await office.getByRole('checkbox',{name:'Keep offline copy on this device'}).check();
    await wait(async()=>!!await localDraft(office,admin.uid,document.uid),'initial offline copy');
    await wait(()=>office.getByText('Editor available offline',{exact:false}).isVisible(),'offline editor fully cached');
    await closeOptions(office);
    const cached=await office.evaluate(async()=>{const keys=await caches.keys();const urls=[];for(const key of keys)for(const request of await(await caches.open(key)).keys())urls.push(new URL(request.url).pathname);return urls;});
    assert.equal(cached.some(path=>path.startsWith('/mx/')||path==='/'),false,'Never cache private APIs or the authenticated workspace');
    const beforeOffline=mx.n1.requests.length;
    await context.setOffline(true);await typeDocument(office,`MX ${name} offline`);
    await wait(async()=>{const draft=await localDraft(office,admin.uid,document.uid);return draft&&unzip(Buffer.from(draft.blob))['word/document.xml'].includes(`MX ${name} offline`);},'offline edit checkpoint');
    assert.equal(mx.n1.requests.length,beforeOffline,'Offline editing performs no N1 requests');
    await office.screenshot({path:join(mx.directory,`${name}-office-offline.png`)});
    // Firefox's forced browser offline flag can reject navigation before the
    // worker gets a fetch event. Cold-load against an actual TCP outage instead.
    serverReachable=false;await context.setOffline(false);
    await office.close({runBeforeUnload:false});
    const reopened=await context.newPage();reopened.on('dialog',dialog=>dialog.accept());reopened.on('pageerror',error=>errors.push(error.message));
    inspectedPage=reopened;
    await reopened.goto(`${origin}/office/index.html`);await reopened.getByRole('button',{name:new RegExp(document.name.replace('.','\\.'))}).click();
    await wait(()=>downloadButton(reopened).isEnabled(),'cold offline reopen uses cached engine and IndexedDB');
    await reopened.waitForTimeout(1000);await typeDocument(reopened,`MX ${name} reopened`);
    assert.equal(mx.n1.requests.length,beforeOffline,'Cold offline reopening never contacts N1');
    serverReachable=true;await context.setOffline(false);await reopened.getByRole('button',{name:'Save to MX',exact:true}).click();
    await wait(async()=>(await call(`${base}/items/${document.uid}`,admin)).revision===3,'reconnected save creates revision 3');
    const recovered=(await editedContents(document.uid,admin))['word/document.xml'];assert.match(recovered,new RegExp(`MX ${name} offline`));assert.match(recovered,new RegExp(`MX ${name} reopened`));
    console.log(`${name}: edits survive connection loss, tab closure, fully offline reopening and reconnection save`);
    // Lost acknowledgements: commit on the server, drop all three responses,
    // then retry exactly the persisted upload operation. Never create duplicates.
    await wait(()=>reopened.getByRole('button',{name:'Save to MX',exact:true}).isDisabled(),'recovered document clean',10000);
    await typeDocument(reopened,`MX ${name} uncertain save`);
    const dropFinish=async route=>{await route.fetch();await route.abort('failed');};
    await context.route('**/uploads/*/finish',dropFinish);
    await reopened.getByRole('button',{name:'Save to MX',exact:true}).click();
    await wait(()=>reopened.locator('.save-feedback').filter({hasText:'Not saved to MX · your document is retained'}).isVisible(),'lost save acknowledgement retained');
    assert.equal((await call(`${base}/items/${document.uid}`,admin)).revision,4);
    await context.unroute('**/uploads/*/finish',dropFinish);
    // Reload an uncertain save before retrying: the exact File/operation and
    // base revision must survive IndexedDB serialization, not only JS memory.
    await wait(async()=>!!(await localDraft(reopened,admin.uid,document.uid))?.saveFile,'uncertain upload payload retained locally');
    await reopened.reload();await reopened.getByRole('button',{name:new RegExp(document.name.replace('.','\\.'))}).click();
    await wait(()=>downloadButton(reopened).isEnabled(),'uncertain save document reopened');
    await reopened.getByRole('button',{name:'Save to MX',exact:true}).click();
    await wait(()=>reopened.getByRole('button',{name:'Save to MX',exact:true}).isDisabled(),'idempotent save reconciliation');
    assert.equal((await call(`${base}/items/${document.uid}`,admin)).revision,4);
    assert.equal(rows('SELECT COUNT(*) n FROM mx_drive_versions WHERE item_uid=?',document.uid)[0].n,4);
    assert.deepEqual(errors,[]);
    console.log(`${name}: a lost save acknowledgement reconciles to exactly one version`);
    await reopened.screenshot({path:join(mx.directory,`${name}-office-recovered.png`)});
    const competitor=await upload(admin,document.name,officeFixture('.docx'),{item_uid:document.uid,base_revision:4});assert.equal(competitor.revision,5);
    await typeDocument(reopened,`MX ${name} conflict proposal`);await reopened.getByRole('button',{name:'Save to MX',exact:true}).click();
    await wait(()=>reopened.getByText('This file changed in MX. Your edits have not overwritten it.',{exact:true}).isVisible(),'stale save shows deliberate conflict choice');
    assert.equal((await call(`${base}/items/${document.uid}`,admin)).revision,5);
    await reopened.getByRole('button',{name:'Save my edits as the next version',exact:true}).click();
    await wait(async()=>(await call(`${base}/items/${document.uid}`,admin)).revision===6,'explicit conflict resolution preserves both versions');
    assert.match((await editedContents(document.uid,admin))['word/document.xml'],new RegExp(`MX ${name} conflict proposal`));
    assert.equal(rows('SELECT COUNT(*) n FROM mx_drive_versions WHERE item_uid=?',document.uid)[0].n,6);
    console.log(`${name}: stale Office save never overwrites silently; explicit resolution adds a normal version`);
    await wait(()=>reopened.getByRole('button',{name:'Save to MX',exact:true}).isDisabled(),'resolved document clean');
    await typeDocument(reopened,`MX ${name} separate copy`);
    await reopened.keyboard.press('Control+Shift+s');
    const copyName=`Separate-${name}-${randomUUID()}.docx`;
    await reopened.getByLabel('New file name').fill(copyName);
    await context.route('**/uploads/*/finish',dropFinish);
    await reopened.getByRole('button',{name:'Save copy to My Drive',exact:true}).click();
    await wait(()=>reopened.locator('.save-feedback').filter({hasText:'Copy not confirmed · retry keeps the same file and operation'}).isVisible(),'copy response loss retained');
    const listCopies=async()=>(await call(`${base}?view=mine&q=${encodeURIComponent(copyName)}`,admin)).items.filter(item=>item.name===copyName);
    assert.equal((await listCopies()).length,1);
    await context.unroute('**/uploads/*/finish',dropFinish);
    await reopened.reload();await reopened.getByRole('button',{name:new RegExp(document.name.replace('.','\\.'))}).click();
    await wait(()=>reopened.getByRole('button',{name:'Save copy to My Drive',exact:true}).isVisible(),'pending copy restored across reload');
    await reopened.getByRole('button',{name:'Save copy to My Drive',exact:true}).click();
    await wait(()=>reopened.getByText('Saved as a separate file in your My Drive.',{exact:false}).isVisible(),'same copy operation reconciled');
    const copies=await listCopies();assert.equal(copies.length,1);assert.equal(copies[0].revision,1);
    assert.match((await editedContents(copies[0].uid,admin))['word/document.xml'],new RegExp(`MX ${name} separate copy`));
    assert.equal((await call(`${base}/items/${document.uid}`,admin)).revision,6);
    console.log(`${name}: Save As survives lost acknowledgements and reload with one separate file, original unchanged`);
    await reopened.close({runBeforeUnload:false});
    }
    // Independent real-sheet and presentation round trips, including native Save.
    for(const extension of (process.env.MX_TEST_OFFICE_FORMATS||'.xlsx,.pptx').split(',')){
      const file=await upload(admin,`Test-${name}${extension}`,officeFixture(extension));
      const editor=await context.newPage();inspectedPage=editor;editor.on('dialog',dialog=>dialog.accept());editor.on('pageerror',error=>errors.push(error.message));
      editor.on('console',message=>console.log(`${name} ${extension}: ${message.text().slice(0,400)}`));
      await editor.goto(`${origin}/office/index.html#file=${file.uid}`);
      await wait(async()=>{const alerts=editor.locator('.alert:not(.warning)');if(await alerts.count())throw new Error(await alerts.innerText());return downloadButton(editor).isEnabled();},`real engine opens ${extension}`);
      await editor.waitForTimeout(1000);await editor.locator('#qtcanvas').focus();
      assert.equal(await editor.getByRole('button',{name:/^Sign in(?: to save)?$/}).count(),0,'Authenticated Office must not display a sign-in action');
      if(extension==='.xlsx'){
          await editor.locator('#qtcanvas').click({position:{x:20,y:12}});
          await editor.waitForTimeout(1500);
          await editor.screenshot({path:join(mx.directory,`${name}-guarded-file-menu.png`)});
          await editor.locator('#qtcanvas').click({position:{x:75,y:65}});
        await wait(()=>editor.getByText('Open another document from MX Drive in a separate editor tab.',{exact:false}).isVisible(),'File > Open safely routes to MX guidance',5000);
        await editor.waitForTimeout(1000);
        await editor.locator('#qtcanvas').click({position:{x:62,y:43}});
        await editor.waitForTimeout(1000);
        await editor.locator('#qtcanvas').focus();await editor.keyboard.press('Control+o');
        await wait(()=>editor.getByText('Open another document from MX Drive in a separate editor tab.',{exact:false}).isVisible(),'Open shortcut routes to MX guidance',5000);
        await editor.screenshot({path:join(mx.directory,`${name}-guarded-native-open.png`)});
        assert.deepEqual(errors,[],'Native Open must not crash the Office engine');
        assert.equal(await editor.locator('.alert:not(.warning)').count(),0,'Native Open must not stop the editor');
        assert.equal((await call(`${base}/items/${file.uid}`,admin)).revision,1,'Open commands must not replace the MX file');
      }
      const marker=`MX ${name} ${extension.slice(1)} edit`;
      if(['.ppt','.pptx','.odp'].includes(extension)){
        await openOptions(editor);
        await editor.getByText('Presentations are read-only in this build.',{exact:false}).waitFor();
        await editor.keyboard.type(marker,{delay:30});
        assert.equal(await editor.getByRole('button',{name:'Save to MX',exact:true}).isDisabled(),true);
        assert.equal((await call(`${base}/items/${file.uid}/office`,admin)).editing_supported,false);
        const downloadEvent=editor.waitForEvent('download');await downloadCopy(editor);
        const stream=await(await downloadEvent).createReadStream();const chunks=[];for await(const chunk of stream)chunks.push(chunk);
        assert.deepEqual(Buffer.concat(chunks),officeFixture(extension));assert.equal((await call(`${base}/items/${file.uid}`,admin)).revision,1);
        await editor.screenshot({path:join(mx.directory,`${name}-office-presentation-readonly.png`)});
        console.log(`${name}: presentation renders read-only; editing is explicitly NOT qualified`);
        await editor.close({runBeforeUnload:false});continue;
      }
      if(extension==='.xlsx'){await editor.locator('#qtcanvas').focus();await wait(async()=>(await editor.locator('#qtcanvas').getAttribute('data-native-focused'))==='true','native document focus restored after Open',10000);await editor.keyboard.press('Control+Home',{delay:80});await editor.keyboard.type(marker,{delay:30});await editor.keyboard.press('Enter',{delay:80});}
      else throw new Error(`No real keyboard edit fixture is implemented for ${extension}; do not count filter availability as validation.`);
      await wait(()=>editor.getByRole('button',{name:'Save to MX',exact:true}).isEnabled(),`${extension} keyboard edit dirty`,10000);
      await editor.waitForTimeout(300);await editor.screenshot({path:join(mx.directory,`${name}-office-${extension.slice(1)}-edited.png`)});
      // Delay a real save request, not the save response/UI. A pending operation
      // must never be presented as a confirmed MX save.
      let releaseSave;
      const saveGate=new Promise(resolve=>releaseSave=resolve);
      let finishWaiting=false;
      const delayedFinish=async route=>{finishWaiting=true;await saveGate;await route.continue();};
      await context.route('**/uploads/*/finish',delayedFinish);
      try {
        if(extension==='.xlsx')await editor.locator('#qtcanvas').click({position:{x:106,y:43}});else await editor.getByRole('button',{name:'Save to MX',exact:true}).click();
        await wait(()=>finishWaiting,`${extension} real save waits for commit`);
        assert.equal(await editor.getByRole('button',{name:'Saving…',exact:true}).isDisabled(),true);
        assert.match(await editor.locator('.save-feedback').innerText(),/Saving to MX|Confirming saved version/);
        assert.equal(await editor.locator('.save-feedback.saved').count(),0);
        assert.equal((await call(`${base}/items/${file.uid}`,admin)).revision,1,'Pending upload is not yet an acknowledged MX version');
        await editor.locator('#qtcanvas').focus();
        await wait(async()=>(await editor.locator('#qtcanvas').getAttribute('data-native-focused'))==='true','native focus while a save is pending',10000);
        await editor.keyboard.press('Control+Home',{delay:80});
        for(let i=0;i<3;i++)await editor.keyboard.press('ArrowRight',{delay:80});
        await wait(async()=>(await editor.getByLabel('Selected cell',{exact:true}).innerText())==='D1','actual cell selected during save',10000);
        // Leave these NEWER edits uncommitted while the older snapshot saves.
        await editor.keyboard.type(`MX ${name} typed during save`,{delay:30});
        releaseSave();
        await wait(async()=>(await call(`${base}/items/${file.uid}`,admin)).revision===2,`${extension} save creates new object`);
      } finally { releaseSave();await context.unroute('**/uploads/*/finish',delayedFinish); }
      const contents=await editedContents(file.uid,admin);assert.match(Object.values(contents).join(''),new RegExp(marker));
      if(extension==='.xlsx'){assert.match(contents['xl/worksheets/sheet1.xml'],/A2\*B2/);assert.match(contents['xl/worksheets/sheet1.xml'],/170000/);}
      assert.equal(rows('SELECT COUNT(DISTINCT object_key) n FROM mx_drive_versions WHERE item_uid=?',file.uid)[0].n,2);
      await wait(()=>editor.locator('.save-feedback.saved').filter({hasText:'Saved to MX · revision 2'}).isVisible(),'native toolbar Save acknowledges MX revision visibly');
      await wait(()=>editor.locator('.save-feedback').filter({hasText:'newer edits still need saving'}).isVisible(),'acknowledging an older snapshot must not claim the newer input was saved');
      assert.doesNotMatch(Object.values(contents).join(''),new RegExp(`MX ${name} typed during save`));
      await wait(()=>editor.getByRole('button',{name:'Save to MX',exact:true}).isEnabled(),'newer cell input remains saveable');
      await editor.getByRole('button',{name:'Save to MX',exact:true}).click();
      await wait(async()=>(await call(`${base}/items/${file.uid}`,admin)).revision===3,'save the edits made during the preceding upload');
      assert.match(Object.values(await editedContents(file.uid,admin)).join(''),new RegExp(`MX ${name} typed during save`));
      await wait(()=>editor.locator('.save-feedback.saved').filter({hasText:'Saved to MX · revision 3'}).isVisible(),'newer input is now acknowledged too');
      console.log(`${name}: real ${extension} edit/export/save round trip verified${extension==='.xlsx'?' including formula and native toolbar Save':''}`);
      if(extension==='.xlsx'){
        for(const [action,column,revision] of [['shortcut',3,4],['menu',4,5]]){
          const text=`MX ${name} ${action} saved without Enter`;
          await editor.locator('#qtcanvas').focus();await editor.waitForTimeout(500);
          await editor.keyboard.press('Control+Home',{delay:80});
          for(let i=0;i<column;i++)await editor.keyboard.press('ArrowRight',{delay:80});
          // Wait for the actual native cursor, not just synthetic key delivery.
          // The canvas's new accessible cell indicator is driven by UNO selection.
          await wait(async()=>(await editor.getByLabel('Selected cell',{exact:true}).innerText())===`${String.fromCharCode(65+column)}1`,'native cell selection acknowledged',10000);
          await editor.keyboard.type(text,{delay:30});
          // Deliberately leave the cell's native input editor active.
          if(action==='shortcut')await editor.keyboard.press('Control+s',{delay:80});
          else{
            await editor.locator('#qtcanvas').click({position:{x:20,y:12}});
            await editor.waitForTimeout(500);
            await editor.screenshot({path:join(mx.directory,`${name}-native-file-save-menu.png`)});
            await editor.locator('#qtcanvas').click({position:{x:80,y:287}});
          }
          await wait(async()=>(await call(`${base}/items/${file.uid}`,admin)).revision===revision,`${action} saves pending cell input`);
          const stored=await editedContents(file.uid,admin);
          assert.match(Object.values(stored).join(''),new RegExp(text));
          assert.match(stored['xl/worksheets/sheet1.xml'],/A2\*B2/);
          await wait(()=>editor.locator('.save-feedback.saved').filter({hasText:`Saved to MX · revision ${revision}`}).isVisible(),`${action} shows acknowledged MX save`);
          assert.equal(rows('SELECT COUNT(*) n FROM mx_drive_versions WHERE item_uid=?',file.uid)[0].n,revision);
          assert.equal(rows('SELECT COUNT(DISTINCT object_key) n FROM mx_drive_versions WHERE item_uid=?',file.uid)[0].n,revision);
        }
        await editor.locator('#qtcanvas').focus();await editor.waitForTimeout(500);await editor.keyboard.press('Control+s',{delay:80});
        await wait(()=>editor.getByText('No unsaved changes to save',{exact:true}).first().isVisible(),'saving a clean document is explained');
        assert.equal((await call(`${base}/items/${file.uid}`,admin)).revision,5,'Saving unchanged content must not manufacture a new version');
        await editor.screenshot({path:join(mx.directory,`${name}-office-save-feedback.png`)});
        console.log(`${name}: Ctrl+S and File > Save include uncommitted cell input; acknowledged revision/time shown; clean save adds no version`);
      }
      await editor.close({runBeforeUnload:false});
    }
    if(!process.env.MX_TEST_OFFICE_GRAPHICS_FALLBACK){
      if(name==='chromium'){
      // Use a genuinely untrusted test TLS certificate. Do not replace the
      // service-worker API with a mock or add a CA to the host/browser trust.
      // Firefox's WebDriver ignoreHTTPSErrors overrides worker TLS too; that
      // setup cannot exercise certificate rejection and is not counted here.
      const tlsContext=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}});
      await tlsContext.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},admin);
      const tls=await tlsContext.newPage();inspectedPage=tls;tls.on('pageerror',error=>errors.push(error.message));
      await tls.goto(`https://127.0.0.1:${mx.port}/office/index.html`);
      await wait(async()=>await tls.locator('.offline-details').count()===1,'untrusted TLS exposes offline details in document options',15000);
      assert.equal(await tls.locator('.offline-details').isVisible(),false,'Optional certificate details must not occupy the editor');
      await openOptions(tls);
      assert.equal(await tls.locator('.offline-details').getAttribute('open'),null,'Technical warning must not flood the editor by default');
      await tls.getByText('Offline reopening unavailable · details and retry',{exact:true}).click();
      await tls.getByText('Offline reopening is unavailable because the browser does not trust MX’s HTTPS certificate.',{exact:false}).waitFor();
      await tls.screenshot({path:join(mx.directory,`${name}-office-untrusted-tls.png`)});
      await tlsContext.close();
      }
      const file=await upload(admin,`Auth-recovery-${name}.xlsx`,officeFixture('.xlsx'));
      const editor=await context.newPage();inspectedPage=editor;editor.on('pageerror',error=>errors.push(error.message));editor.on('dialog',dialog=>dialog.accept());
      await editor.addInitScript(()=>{
        window.mxOfficeInputTimeline=[];
        const record=event=>window.mxOfficeInputTimeline.push({event,at:performance.now(),focused:document.getElementById('qtcanvas')?.dataset.nativeFocused,active:document.activeElement?.id,documentFocused:document.hasFocus(),height:document.getElementById('qtcanvas')?.clientHeight});
        window.addEventListener('resize',()=>record('resize'));
        window.addEventListener('keydown',event=>{if(event.target.id==='qtcanvas')record(`key:${event.key}`);},true);
      });
      await editor.goto(`${origin}/office/index.html#file=${file.uid}`);
      await wait(()=>downloadButton(editor).isEnabled(),'auth test opens actual spreadsheet');
      assert.equal(await editor.getByRole('button',{name:/^Sign in(?: to save)?$/}).count(),0);
      const marker=`MX ${name} retained after session revocation`;
      await editor.locator('#qtcanvas').focus();await wait(async()=>(await editor.locator('#qtcanvas').getAttribute('data-native-focused'))==='true','auth test native focus ready',10000);await editor.keyboard.press('Control+Home',{delay:80});await editor.keyboard.type(marker,{delay:30});await editor.keyboard.press('Enter',{delay:80});
      await writeFile(join(mx.directory,`${name}-initial-input-timeline.json`),JSON.stringify(await editor.evaluate(()=>window.mxOfficeInputTimeline),null,2));
      await wait(()=>editor.getByRole('button',{name:'Save to MX',exact:true}).isEnabled(),'auth test has unsaved edits',10000);
      if(process.env.MX_TEST_OFFICE_INPUT_DIAGNOSTIC){
      const beforeRevocation=editor.waitForEvent('download');
      await downloadCopy(editor);
      const initialStream=await(await beforeRevocation).createReadStream(),initialChunks=[];
      for await(const chunk of initialStream)initialChunks.push(chunk);
      const initialContents=unzip(Buffer.concat(initialChunks));
      await writeFile(join(mx.directory,`${name}-before-auth-recovery.json`),JSON.stringify(initialContents));
      assert.ok(Object.values(initialContents).join('').includes(marker),'The complete typed text must exist before revoking authentication');
      }
      // Revoke actual server sessions in the disposable DB; do not fabricate a
      // frontend expiry event or replace HTTP auth responses with mocks.
      const database=new DatabaseSync(join(mx.directory,'test.db'));
      try{database.prepare('UPDATE users SET auth_version=auth_version+1 WHERE uid=?').run(admin.uid);}finally{database.close();}
      Object.assign(admin,await call('/mx/v1/auth/authenticate',null,'POST',{email:'office-admin@test.invalid',password}));
      await editor.getByRole('button',{name:'Save to MX',exact:true}).click();
      await wait(()=>editor.getByLabel('Email',{exact:true}).isVisible(),'real session revocation asks for authentication');
      assert.equal((await call(`${base}/items/${file.uid}`,admin)).revision,1);
      await editor.getByRole('button',{name:'Cancel',exact:true}).click();
      await editor.getByRole('button',{name:'Sign in to save',exact:true}).click();
      await editor.getByLabel('Email',{exact:true}).fill('office-viewer@test.invalid');await editor.getByLabel('Password',{exact:true}).fill(password);
      await editor.getByRole('button',{name:'Sign in to save',exact:true}).click();
      await editor.getByText('Sign in with the account that opened this document.',{exact:true}).waitFor();
      assert.equal((await call(`${base}/items/${file.uid}`,admin)).revision,1,'A different account cannot publish the retained edits');
      await editor.getByLabel('Email',{exact:true}).fill('office-admin@test.invalid');await editor.getByLabel('Password',{exact:true}).fill(password);
      await editor.getByRole('button',{name:'Sign in to save',exact:true}).click();
      await wait(()=>editor.getByText('Signed in. Your edits are still open; save when ready.',{exact:true}).isVisible(),'original account authenticated without reloading document');
      assert.equal(await editor.getByRole('button',{name:/^Sign in(?: to save)?$/}).count(),0);
      await editor.getByRole('button',{name:'Save to MX',exact:true}).click();
      await wait(async()=>(await call(`${base}/items/${file.uid}`,admin)).revision===2,'retained edits save after genuine sign-in');
      assert.ok(Object.values(await editedContents(file.uid,admin)).join('').includes(marker),'The complete actual typed text must survive authentication recovery and save');
      assert.equal(rows('SELECT COUNT(*) n FROM mx_drive_versions WHERE item_uid=?',file.uid)[0].n,2);
      await editor.screenshot({path:join(mx.directory,`${name}-office-auth-recovered.png`)});
      console.log(`${name}: sign-in hidden while authenticated; real revocation/cancel/wrong-account/re-login preserves edits and publishes one version`);
      await editor.close({runBeforeUnload:false});
    }
    if(!process.env.MX_TEST_OFFICE_FORMATS_ONLY){
      const viewContext=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}});
      await viewContext.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},viewer);
      await viewContext.route('**/*',route=>new URL(route.request().url()).origin===origin?route.continue():route.abort());
      const view=await viewContext.newPage();inspectedPage=view;
      await view.goto(`${origin}/office/index.html#file=${files['.docx'].uid}`);
      await wait(()=>downloadButton(view).isEnabled(),'viewer opens real read-only Office');
      assert.equal(await view.getByRole('button',{name:'Save to MX',exact:true}).isDisabled(),true);
      await typeDocument(view,'Must not modify shared viewer file');assert.equal(await view.getByRole('button',{name:'Save to MX',exact:true}).isDisabled(),true);
      assert.equal((await call(`${base}/items/${files['.docx'].uid}`,admin)).revision,1);
      console.log(`${name}: shared viewer opens read-only and cannot publish an edit`);await viewContext.close();
    }
    assert.deepEqual(errors,[]);assert.equal(unexpected.length,0);
    browserResults.push({browser:name,result:process.env.MX_TEST_OFFICE_FORMATS_ONLY?'focused format checks only':'Word/Excel editing and offline/retry/conflict checks',presentationEditing:false});
    await context.close();await browser.close();launched.splice(launched.indexOf(browser),1);
  }
  await writeFile(join(mx.directory,'office-results.json'),JSON.stringify({browsers:browserNames,results:browserResults,shellOnly:!!process.env.MX_TEST_OFFICE_SHELL_ONLY,formatsOnly:!!process.env.MX_TEST_OFFICE_FORMATS_ONLY,formats:(process.env.MX_TEST_OFFICE_FORMATS||'.xlsx,.pptx').split(','),presentationEditing:false,productionQualified:false,passed:true},null,2));
}catch(error){if(inspectedPage&&!inspectedPage.isClosed()){await inspectedPage.screenshot({path:join(mx.directory,'office-failure.png')});console.log(await inspectedPage.locator('body').innerText());}throw error;}finally{for(const browser of launched)await browser.close();await new Promise(resolve=>proxy.close(resolve));await mx.close();}
