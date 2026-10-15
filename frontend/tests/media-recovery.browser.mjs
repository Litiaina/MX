// Real MX/N1 contract + real native playback. Never opens desktop audio devices.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
if(process.env.MX_TEST_N1_URL)throw Error('Use disposable storage, not live N1.');
if(!process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/tests'))throw Error('Set external-drive artifact directory.');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url))),opened=[],results=[];
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const base='/mx/v1/drive';let inspected;
async function until(check,label,timeout=30000){const end=Date.now()+timeout;while(Date.now()<end){if(await check())return;await new Promise(resolve=>setTimeout(resolve,100));}throw Error(`Timeout: ${label}`);}
async function launch(name){
 const executablePath=({brave:'/opt/brave.com/brave/brave',chromium:'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome',firefox:'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox',webkit:'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh'})[name];
 const browser=await(name==='brave'?chromium:({chromium,firefox,webkit})[name]).launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(['brave','chromium'].includes(name)?{args:['--no-sandbox','--mute-audio']}: {})});opened.push(browser);return browser;
}
async function upload(user,name,mime,bytes){
 const operation=randomUUID();await call(`${base}/uploads`,user,'POST',{operation_uid:operation,file_name:name,mime_type:mime,size:bytes.length,last_modified:1});
 let response;for(let attempt=0;attempt<3;attempt++){response=await mx.api.put(`${base}/uploads/${operation}/parts/0`,{headers:{Authorization:`Bearer ${user.access_token}`,'Content-Type':'application/octet-stream'},data:bytes});if(response.status()!==502)break;}assert.equal(response.status(),200);
 return call(`${base}/uploads/${operation}/finish`,user,'POST');
}
function silentWave(){const size=12*16000*2,bytes=Buffer.alloc(44+size);bytes.write('RIFF');bytes.writeUInt32LE(36+size,4);bytes.write('WAVEfmt ',8);bytes.writeUInt32LE(16,16);bytes.writeUInt16LE(1,20);bytes.writeUInt16LE(1,22);bytes.writeUInt32LE(16000,24);bytes.writeUInt32LE(32000,28);bytes.writeUInt16LE(2,32);bytes.writeUInt16LE(16,34);bytes.write('data',36);bytes.writeUInt32LE(size,40);return bytes;}
try{
 console.log(`Media recovery artifacts: ${mx.directory}`);
 const password=`Test-only!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'media-owner@test.invalid',name:'Media Owner',password},[201]);
 const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'media-owner@test.invalid',password});
 const audio=await upload(owner,'Recovery.wav','audio/wav',silentWave());
 // Generate a silent animated WebM from an artificial canvas, not a physical
 // camera/microphone/screen. Reuse the exact same bytes across all engines.
 const recorderBrowser=await launch('chromium'),recorder=await recorderBrowser.newPage();
 const videoBytes=Buffer.from(await recorder.evaluate(async()=>{
  const canvas=document.createElement('canvas');canvas.width=320;canvas.height=180;
  const ctx=canvas.getContext('2d'),stream=canvas.captureStream(15),recorder=new MediaRecorder(stream,{mimeType:'video/webm;codecs=vp8'}),chunks=[];
  recorder.ondataavailable=event=>chunks.push(event.data);
  const complete=new Promise(resolve=>recorder.onstop=async()=>resolve(Array.from(new Uint8Array(await new Blob(chunks).arrayBuffer()))));
  let frame=0;const timer=setInterval(()=>{ctx.fillStyle=frame++%2?'#185f9d':'#47bb80';ctx.fillRect(0,0,320,180);ctx.fillStyle='#fff';ctx.fillText(String(frame),20,30);},66);
  recorder.start();await new Promise(resolve=>setTimeout(resolve,6000));recorder.stop();clearInterval(timer);stream.getTracks().forEach(track=>track.stop());return await complete;
 }));await recorderBrowser.close();opened.splice(opened.indexOf(recorderBrowser),1);
 const video=await upload(owner,'Recovery.webm','video/webm',videoBytes),tickets=[];
 for(let index=0;index<8;index++){
  const email=`media-${index}@test.invalid`;await call('/mx/v1/user/create',owner,'POST',{email,name:`Media ${index}`,password,access_level:2},[201]);
  const user=await call('/mx/v1/auth/authenticate',null,'POST',{email,password});
  await call(`${base}/items/${audio.uid}/sharing`,owner,'POST',{user_uid:user.uid,role:'viewer'});
  tickets.push(await call(`${base}/items/${audio.uid}/ticket`,user,'POST',{}));
 }
 for(const status of [429,503]){
  mx.n1.control.beforeStream=(_request,response)=>{response.writeHead(status,{'Retry-After':'17'});response.end('Private upstream detail');};
  const before=mx.n1.requests.length,replies=await Promise.all(tickets.map(ticket=>mx.api.get(ticket.url,{headers:{Range:'bytes=120-180'}})));
  for(const reply of replies){assert.equal(reply.status(),status);assert.equal(reply.headers()['retry-after'],'17');assert.equal(reply.headers()['cache-control'],'no-store');assert.ok(!(await reply.text()).includes('Private upstream detail'));}
  assert.equal(mx.n1.requests.length-before,8,'Exactly one upstream request per client: no MX limiter or retry loop');
 }
 mx.n1.control.beforeStream=undefined;
 for(const ticket of tickets){const reply=await mx.api.get(ticket.url,{headers:{Range:'bytes=120-180'}});assert.equal(reply.status(),206);assert.deepEqual(await reply.body(),silentWave().subarray(120,181));}
 console.log('Eight-user HTTP 429/503 propagation, no hidden retries, and exact-byte range recovery pass');
 for(const name of(process.env.MX_TEST_BROWSERS||'brave,chromium,firefox,webkit').split(',')){
  const browser=await launch(name),context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}}),page=await context.newPage();inspected=page;
  await context.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  let fault=429,probes=0,faultSeconds=2;
  await context.route('**/mx/v1/drive/media/*',route=>{
   if(!fault)return route.continue();
   const status=fault;if(route.request().headers().range==='bytes=0-0'){probes++;fault=0;}
   return route.fulfill({status,headers:{'Retry-After':String(faultSeconds),'Cache-Control':'no-store','Content-Type':'application/json'},body:JSON.stringify({response:'Injected storage outage'})});
  });
  await page.goto(`https://127.0.0.1:${mx.port}/#drive?item=${video.uid}`);
  await page.getByRole('status').filter({hasText:'Storage is temporarily limiting requests'}).waitFor();
  await page.screenshot({path:join(mx.directory,`${name}-video-recovery-notice.png`)});
  await until(()=>page.locator('video').evaluate(media=>media.readyState>=2),'real video loads after 429');
  assert.equal(probes,1,'One bounded diagnostic probe, not an endless request loop');
  await page.locator('video').evaluate(media=>{media.muted=true;return media.play();});
  await until(()=>page.locator('video').evaluate(media=>media.currentTime>.3&&!media.paused),'real video playback');
  await page.screenshot({path:join(mx.directory,`${name}-video-recovered.png`)});
  await page.getByRole('button',{name:'Close preview',exact:true}).click();
  await page.goto(`https://127.0.0.1:${mx.port}/#drive?item=${audio.uid}`);
  const media=page.locator('audio');await until(()=>media.evaluate(node=>node.readyState>=2),'native audio loads');
  await media.evaluate(async node=>{node.muted=true;node.currentTime=3;await node.play();});await until(()=>media.evaluate(node=>node.currentTime>3.1),'native audio plays');
  // Deterministic mid-play network error: native error information itself is
  // readonly, so inject only that event/property; HTTP diagnosis/reload/seek and
  // subsequent playback still use the actual media element and server bytes.
  fault=503;
  await media.evaluate(node=>{window.mxRecoveryPosition=node.currentTime;Object.defineProperty(node,'error',{configurable:true,value:{code:2}});node.dispatchEvent(new Event('error'));});
  await page.getByRole('status').filter({hasText:'HTTP 503'}).waitFor();
  await page.screenshot({path:join(mx.directory,`${name}-audio-recovery-notice.png`)});
  await media.evaluate(node=>delete node.error);
  await until(async()=>await page.locator('.preview-media-notice').count()===0&&await media.evaluate(node=>node.readyState>=2&&!node.paused&&node.currentTime>=window.mxRecoveryPosition-.2),'playback resumes from its position');
  assert.equal(await page.locator('.preview-media-notice').count(),0);
  await page.screenshot({path:join(mx.directory,`${name}-audio-resumed.png`)});
  fault=503;faultSeconds=5;await media.evaluate(node=>{Object.defineProperty(node,'error',{configurable:true,value:{code:2}});node.dispatchEvent(new Event('error'));});
  await page.getByRole('status').filter({hasText:'HTTP 503'}).waitFor();
  await page.setViewportSize({width:390,height:850});await page.evaluate(()=>document.documentElement.dataset.theme='dark');
  const notice=await page.locator('.preview-media-notice').evaluate(node=>({fits:node.scrollWidth<=node.clientWidth+1,width:node.getBoundingClientRect().width}));
  assert.ok(notice.fits&&notice.width<=390,'Mobile recovery feedback wraps without overflow');
  await page.screenshot({path:join(mx.directory,`${name}-media-recovery-dark-mobile.png`)});
  const requests=[];page.on('request',request=>{if(request.url().includes('/drive/media/'))requests.push(request.url());});
  await page.getByRole('button',{name:'Close preview',exact:true}).click();await page.goto(`https://127.0.0.1:${mx.port}/#modules`);
  await page.waitForTimeout(5500);assert.deepEqual(requests,[],'Closing/navigation cancels the pending retry');assert.equal(await page.locator('video,audio').count(),0);
  assert.deepEqual(errors,[]);results.push({browser:name,real_video_after_429:true,audio_recovery:'simulated terminal network event; real HTTP/reload/seek/play',pending_retry_cancelled:true});console.log(`${name}: native video, recovery feedback, audio seek/play restoration and navigation cleanup pass`);
  await context.close();await browser.close();opened.splice(opened.indexOf(browser),1);
 }
 assert.deepEqual(mx.n1.failures,[]);
 await writeFile(join(mx.directory,'media-recovery-results.json'),JSON.stringify({passed:true,eight_user_transport:true,results},null,2));
}catch(error){if(inspected&&!inspected.isClosed())await inspected.screenshot({path:join(mx.directory,'media-recovery-failure.png')}).catch(()=>{});throw error;}
finally{for(const browser of opened)await browser.close();await mx.close();}
