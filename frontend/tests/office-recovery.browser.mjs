// Disposable MX/N1 fixture only. No physical audio, live account or deployment writes.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createServer} from 'node:http';
import {request as httpsRequest} from 'node:https';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {officeFixture} from './office-fixtures.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
if(process.env.MX_TEST_N1_URL)throw new Error('Recovery tests require isolated storage.');
if(!process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/tests'))throw new Error('Set external-drive artifact directory.');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const proxy=createServer((incoming,outgoing)=>{
 const upstream=httpsRequest({hostname:'127.0.0.1',port:mx.port,path:incoming.url,method:incoming.method,headers:incoming.headers,rejectUnauthorized:false},response=>{outgoing.writeHead(response.statusCode,response.headers);response.pipe(outgoing);});
 upstream.on('error',()=>{outgoing.writeHead(502);outgoing.end();});incoming.pipe(upstream);
});
await new Promise(resolve=>proxy.listen(0,'127.0.0.1',resolve));const origin=`http://127.0.0.1:${proxy.address().port}`;
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const results=[],opened=[];let inspected;
async function until(check,label,timeout=30000){const end=Date.now()+timeout;while(Date.now()<end){if(await check())return;await new Promise(resolve=>setTimeout(resolve,100));}throw new Error(`Timeout: ${label}`);}
try{
 console.log(`Office recovery artifacts: ${mx.directory}`);
 const password=`Test-only!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{name:'Office Recovery',email:'recovery@test.invalid',password},[201]);
 const user=await call('/mx/v1/auth/authenticate',null,'POST',{email:'recovery@test.invalid',password});
 const operation=randomUUID(),bytes=officeFixture('.xlsx');
 await call('/mx/v1/drive/uploads',user,'POST',{operation_uid:operation,file_name:'Recovery.xlsx',mime_type:'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',size:bytes.length,last_modified:1});
 let part;for(let attempt=0;attempt<3;attempt++){part=await mx.api.put(`/mx/v1/drive/uploads/${operation}/parts/0`,{headers:{Authorization:`Bearer ${user.access_token}`,'Content-Type':'application/octet-stream'},data:bytes});if(part.status()!==502)break;}assert.equal(part.status(),200);
 const item=await call(`/mx/v1/drive/uploads/${operation}/finish`,user,'POST');
 for(const engine of (process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
  const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
  const browser=await ({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}: {})});opened.push(browser);
  const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}}),unexpected=[],errors=[],conversionRequests=[];
  await context.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},user);
  await context.route('**/*',route=>{const url=new URL(route.request().url());if(['http:','https:'].includes(url.protocol)&&url.origin!==origin){unexpected.push(url.href);return route.abort();}if(url.searchParams.get('office')==='true')conversionRequests.push(url.pathname);return route.continue();});
  const workspace=await context.newPage();inspected=workspace;workspace.on('pageerror',error=>errors.push(error.message));
  await workspace.goto(`${origin}/#drive`);await workspace.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();
  const popupPromise=context.waitForEvent('page');await workspace.locator('.file-row').filter({hasText:item.name}).dblclick();const popup=await popupPromise;
  await popup.waitForURL(/\/office\/index\.html#file=/);assert.ok(popup.url().endsWith(item.uid));await popup.close();
  await workspace.goto(`${origin}/#drive?item=${item.uid}`);
  const linked=workspace.getByRole('dialog');await linked.getByRole('button',{name:'Open in MX Office',exact:true}).waitFor();
  await workspace.screenshot({path:join(mx.directory,`${engine}-office-drive-launch.png`)});
  assert.deepEqual(conversionRequests,[],'Opening Office files must not start server PDF conversion');
  await linked.getByRole('button',{name:'Close preview',exact:true}).click();
  const editor=await context.newPage();inspected=editor;editor.on('pageerror',error=>errors.push(error.message));
  let release,seen,finished;const gate=new Promise(resolve=>release=resolve),started=new Promise(resolve=>seen=resolve),handled=new Promise(resolve=>finished=resolve);
  const hold=async route=>{try{const response=await route.fetch();seen();await gate;await route.fulfill({response});}finally{finished();}};
  await editor.route('**/mx/v1/auth/session',hold);await editor.goto(`${origin}/office/index.html#file=${item.uid}`);await started;
  await editor.getByRole('button',{name:'Cancel opening',exact:true}).click();release();await handled;await editor.unroute('**/mx/v1/auth/session',hold);
  await editor.getByRole('button',{name:'Retry opening',exact:true}).waitFor();await editor.waitForTimeout(300);
  assert.equal(await editor.locator('.office-loading.failed').count(),1);assert.equal(await editor.locator('.office-loading [role="progressbar"],.office-loading progress').count(),0);
  assert.equal(await editor.locator('script[src$="/office/vendor/soffice.js"]').count(),0,'Late session response cannot restart a cancelled load');
  if(engine!=='webkit'){
   await editor.route('**/office/office-thread.js',async route=>{const response=await route.fetch(),source=await response.text();await route.fulfill({response,body:source.replace('const send = (message) => zeta.mainPort.postMessage(message);',"const send = (message) => message.cmd === 'opened' ? setTimeout(() => zeta.mainPort.postMessage(message), 70000) : zeta.mainPort.postMessage(message);")});});
   await editor.getByRole('button',{name:'Retry opening',exact:true}).click();
   await until(()=>editor.locator('.current-status').innerText().then(text=>text.includes('Open and render document')),'real engine awaiting slow workbook');
   assert.ok(await editor.evaluate(()=>window.PThread.runningWorkers.length>0));
   await editor.getByRole('button',{name:'Cancel opening',exact:true}).click();await editor.getByRole('button',{name:'Download loaded copy',exact:true}).waitFor();
   assert.equal(await editor.evaluate(()=>window.PThread.runningWorkers.length),0,'Cancellation terminates computation, not only the spinner');
   const downloaded=editor.waitForEvent('download');await editor.getByRole('button',{name:'Download loaded copy',exact:true}).click();const stream=await(await downloaded).createReadStream(),chunks=[];for await(const chunk of stream)chunks.push(chunk);assert.deepEqual(Buffer.concat(chunks),bytes);
   await editor.unroute('**/office/office-thread.js');await editor.getByRole('button',{name:'Retry opening',exact:true}).click();
   await until(()=>editor.locator('#qtcanvas').getAttribute('data-native-focused').then(value=>value==='true'),'retry opens actual engine');
  }
  const oversized=await context.newPage();inspected=oversized;let buffered=0;
  await oversized.route(`**/mx/v1/drive/items/${item.uid}/office`,async route=>{const response=await route.fetch(),value=await response.json();value.item.size=20*1024**3;await route.fulfill({response,json:value});});
  oversized.on('request',request=>{if(new URL(request.url()).pathname===`/mx/v1/drive/items/${item.uid}/download`)buffered++;});
  await oversized.goto(`${origin}/office/index.html#file=${item.uid}`);await oversized.getByRole('button',{name:'Download original',exact:true}).waitFor();
  assert.equal(buffered,0,'A multi-gigabyte Office file must not be buffered into the editor');
  const original=oversized.waitForEvent('download');await oversized.getByRole('button',{name:'Download original',exact:true}).click();const originalStream=await(await original).createReadStream(),originalChunks=[];for await(const chunk of originalStream)originalChunks.push(chunk);assert.deepEqual(Buffer.concat(originalChunks),bytes);
  await oversized.close();inspected=editor;
  assert.equal((await call(`/mx/v1/drive/items/${item.uid}`,user)).revision,1);assert.deepEqual(unexpected,[]);assert.deepEqual(errors,[]);assert.deepEqual(conversionRequests,[]);
  results.push({engine,drive_without_pdf:true,cancel_before_session:true,worker_cancellation:engine!=='webkit',retry:engine!=='webkit',large_file_streamed_fallback:true,unchanged_original:true});
  console.log(`${engine}: direct Office launch, linked-file prompt, cancellation/recovery and unchanged source pass`);
  await context.close();await browser.close();opened.splice(opened.indexOf(browser),1);
 }
 await writeFile(join(mx.directory,'office-recovery-results.json'),JSON.stringify({passed:true,results},null,2));
}catch(error){if(inspected&&!inspected.isClosed())await inspected.screenshot({path:join(mx.directory,'office-recovery-failed.png')}).catch(()=>{});throw error;}
finally{for(const browser of opened)await browser.close();proxy.closeAllConnections();await new Promise(resolve=>proxy.close(resolve));await mx.close();}
