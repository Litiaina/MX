// Real Office engine + disposable MX/SQLite/N1 contract fixture. Tab termination
// tests recovery boundaries; this is NOT a physical power-loss certification.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {writeFile,readFile,realpath} from 'node:fs/promises';
import {join,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createServer} from 'node:http';
import {request as httpsRequest} from 'node:https';
import {chromium,firefox} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {officeFixture,unzip} from './office-fixtures.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/tests'));
if(process.env.MX_TEST_N1_URL){
 // Live storage is forbidden except the RC.1 runner's kernel-verified owned
 // disposable engine. Never accept a mere "allow live" flag or production URL.
 const engine=process.env.MX_TEST_OWNED_N1_DIR,pid=process.env.MX_TEST_OWNED_N1_PID;
 assert.match(engine||'',/^\/mnt\/ST1000LM049\/tests\/n1-rc1\/mx-n1-rc1-[^/]+\/engine$/);
 assert.match(pid||'',/^\d+$/);
 assert.equal(await realpath(`/proc/${pid}/cwd`),engine);
 assert.ok((await realpath(`/proc/${pid}/exe`)).startsWith('/mnt/ST1000LM049/tests/n1-rc1/'));
 const config=await readFile(join(engine,'n1_config.ini'),'utf8');
 assert.match(config,/^host=127\.0\.0\.1$/m);
 assert.equal(config.match(/^roots=(.+)$/m)?.[1],join(engine,'storage'));
 assert.equal(config.match(/^fs_storage_path=(.+)$/m)?.[1],join(engine,'control'));
 assert.equal(process.env.MX_TEST_N1_URL,`https://127.0.0.1:${config.match(/^https_port=(\d+)$/m)?.[1]}`);
 assert.equal(await realpath(process.env.MX_TEST_N1_CREDENTIALS_FILE),join(dirname(engine),'test-fragment.env'));
}
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url))),opened=[],results=[],network=[];
const proxy=createServer((incoming,outgoing)=>{const upstream=httpsRequest({hostname:'127.0.0.1',port:mx.port,path:incoming.url,method:incoming.method,headers:incoming.headers,rejectUnauthorized:false},response=>{outgoing.writeHead(response.statusCode,response.headers);response.pipe(outgoing);});upstream.on('error',()=>{outgoing.writeHead(502);outgoing.end();});incoming.pipe(upstream);});
await new Promise(resolve=>proxy.listen(0,'127.0.0.1',resolve));const origin=`http://127.0.0.1:${proxy.address().port}`;
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
let inspected;
async function until(check,label,timeout=120000){const end=Date.now()+timeout;while(Date.now()<end){if(await check())return;await new Promise(resolve=>setTimeout(resolve,150));}throw new Error(`Timeout: ${label}`);}
async function recovery(page,uid){return page.evaluate(async uid=>{const db=await new Promise((resolve,reject)=>{const r=indexedDB.open('mx-office-local-drafts',2);r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});try{const rows=await new Promise((resolve,reject)=>{const r=db.transaction('drafts').objectStore('drafts').getAll();r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});return rows.filter(row=>row.item.uid===uid).map(row=>({key:row.key,generation:row.generation,acknowledged:row.acknowledgedGeneration,operation:row.saveOperation,pending:!!row.saveFile,retained:row.retainOffline,size:row.blob.size}));}finally{db.close();}},uid);}
async function type(page,text){await page.locator('#qtcanvas').focus();await until(()=>page.locator('#qtcanvas').getAttribute('data-native-focused').then(value=>value==='true'),'native focus before typing');await page.keyboard.press('Control+End',{delay:80});await page.keyboard.press('Enter',{delay:80});await page.keyboard.type(text,{delay:25});await page.waitForTimeout(250);}
try{
 console.log(`Office durable-save artifacts: ${mx.directory}`);
 const password=`Test-only!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'durable@test.invalid',name:'Durable Office',password},[201]);
 const user=await call('/mx/v1/auth/authenticate',null,'POST',{email:'durable@test.invalid',password});
 for(const name of (process.env.MX_TEST_BROWSERS||'chromium,firefox').split(',')){
  const brave=name==='brave';const typeEngine=name==='firefox'?firefox:chromium;
  const executablePath=brave?'/usr/bin/brave-browser':name==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome';
  const browser=await typeEngine.launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(name!=='firefox'?{args:['--no-sandbox','--mute-audio']}: {})});opened.push(browser);
  const context=await browser.newContext({viewport:{width:1280,height:900},serviceWorkers:'block'});const errors=[];
  context.on('request',request=>{const url=new URL(request.url());if(url.pathname.startsWith('/mx/'))network.push({browser:name,method:request.method(),path:url.pathname,phase:'request',at:Date.now()});});
  context.on('response',response=>{const url=new URL(response.url());if(url.pathname.startsWith('/mx/'))network.push({browser:name,path:url.pathname,status:response.status(),phase:'response',at:Date.now()});});
  context.on('requestfailed',request=>{const url=new URL(request.url());if(url.pathname.startsWith('/mx/'))network.push({browser:name,path:url.pathname,phase:'failed',reason:request.failure()?.errorText,at:Date.now()});});
  await context.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},user);
  let exports=0,uploadRequests=0;
  await context.route('**/office/office-thread.js',async route=>{const response=await route.fetch();const source=await response.text();await route.fulfill({response,body:source.replace('model.storeToURL(`file:///tmp/mx-office/export','console.info("MX_TEST_EXPORT"); model.storeToURL(`file:///tmp/mx-office/export')});});
  context.on('request',request=>{if(new URL(request.url()).pathname==='/mx/v1/drive/uploads'&&request.method()==='POST')uploadRequests++;});
  const operation=randomUUID(),bytes=officeFixture('.docx');
  await call('/mx/v1/drive/uploads',user,'POST',{operation_uid:operation,file_name:`Durable-${name}.docx`,mime_type:'application/vnd.openxmlformats-officedocument.wordprocessingml.document',size:bytes.length,last_modified:1});
  let part;for(let attempt=0;attempt<3;attempt++){part=await mx.api.put(`/mx/v1/drive/uploads/${operation}/parts/0`,{headers:{Authorization:`Bearer ${user.access_token}`,'Content-Type':'application/octet-stream'},data:bytes});if(part.status()!==502)break;}assert.equal(part.status(),200);
  const item=await call(`/mx/v1/drive/uploads/${operation}/finish`,user,'POST');
  async function open(restore=false){const page=await context.newPage();inspected=page;page.on('pageerror',error=>errors.push(error.message));page.on('console',event=>{if(event.text()==='MX_TEST_EXPORT')exports++;});await page.goto(`${origin}/office/index.html#file=${item.uid}`);if(restore)await page.getByRole('button',{name:'Open local copy',exact:true}).click();await until(()=>page.locator('#qtcanvas').getAttribute('data-native-focused').then(value=>value==='true'),'real document ready');return page;}
  async function saved(page,revision){await until(async()=>{const title=await page.locator('.save-feedback').getAttribute('title');return title?.includes(`Saved to MX · revision ${revision} ·`);},`confirmed revision ${revision}`);assert.equal((await call(`/mx/v1/drive/items/${item.uid}`,user)).revision,revision);console.log(`${name}: revision ${revision} confirmed`);}
  async function contents(){const response=await mx.api.get(`/mx/v1/drive/items/${item.uid}/download`,{headers:{Authorization:`Bearer ${user.access_token}`}});assert.equal(response.status(),200);return unzip(await response.body())['word/document.xml'];}
  let page=await open();
  let releasePart,partSeen;const partGate=new Promise(resolve=>releasePart=resolve),partStarted=new Promise(resolve=>partSeen=resolve);
  const holdPart=async route=>{partSeen();await partGate;await route.abort('failed').catch(()=>{});};
  await context.route('**/uploads/*/parts/*',holdPart);
  const marker=`MX ${name} durable checkpoint ${randomUUID()}`;await type(page,marker);const exportBefore=exports;await page.keyboard.press('Control+s');await partStarted;
  const pending=await recovery(page,item.uid);assert.equal(pending.length,1);assert.equal(pending[0].pending,true);assert.equal(pending[0].retained,false,'recovery does not require offline retention');assert.ok(pending[0].operation);assert.equal(exports-exportBefore,1,'one export, not one for Save plus another checkpoint');
  assert.equal((await call(`/mx/v1/drive/items/${item.uid}`,user)).revision,1);assert.ok(!await page.locator('.save-feedback.saved').count());
  await page.close();releasePart();await context.unroute('**/uploads/*/parts/*',holdPart);
  page=await open(true);await saved(page,2);assert.ok((await contents()).includes(marker));assert.equal((await recovery(page,item.uid)).length,0,'confirmed recovery copy removed');
  const unchangedExports=exports;await page.keyboard.press('Control+s');await page.waitForTimeout(800);assert.equal(exports,unchangedExports,'unchanged save reuses the accepted document snapshot');assert.equal((await call(`/mx/v1/drive/items/${item.uid}`,user)).revision,2);
  // Commit on the server, withhold its acknowledgement, then terminate the tab.
  let releaseFinish,finishSeen;const finishGate=new Promise(resolve=>releaseFinish=resolve),finishStarted=new Promise(resolve=>finishSeen=resolve);
  const holdFinish=async route=>{const response=await route.fetch();assert.equal(response.status(),200);finishSeen();await finishGate;await route.abort('failed').catch(()=>{});};
  await context.route('**/uploads/*/finish',holdFinish);
  const lostMarker=`MX ${name} committed response lost`;await type(page,lostMarker);await page.keyboard.press('Control+s');await finishStarted;
  assert.equal((await call(`/mx/v1/drive/items/${item.uid}`,user)).revision,3);assert.equal((await recovery(page,item.uid))[0].pending,true);
  await page.close();releaseFinish();await context.unroute('**/uploads/*/finish',holdFinish);
  page=await open(true);await saved(page,3);assert.ok((await contents()).includes(lostMarker));
  const history=await call(`/mx/v1/drive/items/${item.uid}/history`,user);assert.equal(history.versions.length,3,'lost acknowledgements do not create duplicate versions');
  // Offline checkpoint is automatic and independent of a Service Worker.
  await context.setOffline(true);await type(page,`MX ${name} offline recovered`);
  await until(async()=>{const drafts=await recovery(page,item.uid);return drafts.some(row=>row.generation!==row.acknowledged);},'automatic offline checkpoint',45000);
  await page.close();await context.setOffline(false);page=await open(true);await saved(page,4);assert.ok((await contents()).includes(`MX ${name} offline recovered`));
  // Storage failure must not start an unprotected automatic upload silently.
  await page.evaluate(()=>{const original=IDBDatabase.prototype.transaction;window.failRecovery=true;IDBDatabase.prototype.transaction=function(...args){if(window.failRecovery&&this.name==='mx-office-local-drafts'&&args[1]==='readwrite')throw new DOMException('Test storage full','QuotaExceededError');return original.apply(this,args);};});
  const beforeFailure=uploadRequests;await type(page,`MX ${name} storage failure retry`);await page.keyboard.press('Control+s');await page.getByRole('button',{name:'Save directly to MX',exact:true}).waitFor();assert.equal(uploadRequests,beforeFailure);assert.equal((await call(`/mx/v1/drive/items/${item.uid}`,user)).revision,4);
  await page.evaluate(()=>window.failRecovery=false);await page.keyboard.press('Control+s');await saved(page,5);assert.ok((await contents()).includes(`MX ${name} storage failure retry`));
  assert.deepEqual(errors,[]);await page.screenshot({path:join(mx.directory,`${name}-durable-save.png`)});
  results.push({browser:name,one_export_per_save:true,unchanged_export_skipped:true,tab_close_during_upload:true,lost_ack_reconciled:true,offline_recovery:true,storage_failure_visible:true,revision:5});
  console.log(`${name}: real exports, interrupted upload, lost acknowledgement, automatic offline checkpoint and quota failure passed`);
  await browser.close();opened.splice(opened.indexOf(browser),1);
 }
 await writeFile(join(mx.directory,'office-durable-results.json'),JSON.stringify({passed:true,results,limits:'Tab termination, not physical power-loss certification. Linux WebKit worker graphics are unsupported; no Safari claim.'},null,2));
}catch(error){await writeFile(join(mx.directory,'office-durable-trace.json'),JSON.stringify({network,n1:mx.n1.requests,ui:inspected&&!inspected.isClosed()?await inspected.locator('.save-feedback').getAttribute('title').catch(()=>null):null},null,2));if(inspected&&!inspected.isClosed())await inspected.screenshot({path:join(mx.directory,'office-durable-failed.png')}).catch(()=>{});throw error;}
finally{for(const browser of opened)await browser.close();proxy.closeAllConnections();await new Promise(resolve=>proxy.close(resolve));await mx.close();}
