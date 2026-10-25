// Real service-worker installation/cache/offline shells; no private documents.
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {createServer} from 'node:http';
import {request as httpsRequest} from 'node:https';
import {chromium,firefox} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';

assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/PROJECT-DISK-1TB/'));
assert.ok(process.env.TMPDIR?.startsWith('/mnt/PROJECT-DISK-1TB/'));
assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const proxy=createServer((incoming,outgoing)=>{
 const upstream=httpsRequest({hostname:'127.0.0.1',port:mx.port,path:incoming.url,method:incoming.method,headers:incoming.headers,rejectUnauthorized:false},response=>{outgoing.writeHead(response.statusCode,response.headers);response.pipe(outgoing);});
 upstream.on('error',()=>{outgoing.writeHead(502);outgoing.end();});incoming.pipe(upstream);
});
await new Promise(resolve=>proxy.listen(0,'127.0.0.1',resolve));
const origin=`http://127.0.0.1:${proxy.address().port}`,results={passed:false,browsers:{}},browsers=[];
try{
 console.log(`Office asset cache artifacts: ${mx.directory}`);
 for(const engine of (process.env.MX_TEST_BROWSERS||'chromium,firefox').split(',')){
  const browser=await({chromium,firefox})[engine].launch({headless:true,executablePath:engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox',env:disconnectedBrowserAudio(mx.directory),...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}: {})});browsers.push(browser);
  const context=await browser.newContext();const page=await context.newPage();
  await page.goto(origin,{waitUntil:'networkidle'});
  const installed=await page.evaluate(async()=>{
   const manifest=await(await fetch('/office/precache.json')).json();
   const registration=await navigator.serviceWorker.register('/office/offline-worker.js',{scope:'/office/'});
   const worker=registration.installing||registration.waiting||registration.active;
   await new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>reject(Error('Office public asset cache installation timed out')),90000);
    const check=()=>{if(worker.state==='activated'){clearTimeout(timer);resolve();}else if(worker.state==='redundant'){clearTimeout(timer);reject(Error('Office public asset cache installation failed'));}};
    worker.addEventListener('statechange',check);check();
   });
   const cache=await caches.open(manifest.cache);
   const keys=(await cache.keys()).map(request=>new URL(request.url).pathname).sort();
   return {cache:manifest.cache,keys,expected:[...manifest.assets,'/office/precache.json'].sort()};
  });
  assert.deepEqual(installed.keys,installed.expected,'Only the complete public manifest is cached');
  assert.ok(!installed.keys.some(path=>path.startsWith('/mx/')||path.includes('token')));
  await context.setOffline(true);
  for(const path of ['/office/index.html','/office/attachment.html']){
   const response=await page.goto(origin+path,{waitUntil:'domcontentloaded'});
   assert.equal(response.status(),200);assert.ok(response.fromServiceWorker());
   assert.ok(await page.evaluate(()=>!!navigator.serviceWorker.controller));
  }
  results.browsers[engine]={passed:true,assets:installed.keys.length,complete_public_cache:true,offline_editor_shell:true,offline_attachment_shell:true,private_documents_cached:false};
  console.log(`${engine}: real service worker cached ${installed.keys.length} public assets and reopened both shells offline`);
  await context.close();await browser.close();
 }
 results.passed=true;
}catch(error){results.error=error.stack;throw error;}
finally{await writeFile(join(mx.directory,'office-assets-results.json'),JSON.stringify(results,null,2));await Promise.all(browsers.map(browser=>browser.close().catch(()=>{})));await mx.close();await new Promise(resolve=>proxy.close(resolve));}
