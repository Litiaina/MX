// Real isolated MX/SQLite backups; N1 HTTP contract fixture only. No live data
// or desktop audio. Artifacts belong on the external test drive.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/tests'));assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url))),browsers=[],results=[];
const path='/mx/v1/admin/backups',origin=`https://127.0.0.1:${mx.port}`;
const call=async(route,user,method='GET',data,expected=[200])=>(await mx.call(route,user?.access_token,method,data,expected)).body;
let inspected,releaseUpload;
async function until(check,label,timeout=20000){const end=Date.now()+timeout;while(Date.now()<end){if(await check())return;await new Promise(resolve=>setTimeout(resolve,75));}throw Error(`Timeout: ${label}`);}
try {
 console.log(`Backup feedback artifacts: ${mx.directory}`);
 const password=`Test-only!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'backups@test.invalid',name:'Backup Test',password},[201]);
 const user=await call('/mx/v1/auth/authenticate',null,'POST',{email:'backups@test.invalid',password});
 for(const name of (process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
  const type=name==='brave'?chromium:({chromium,firefox,webkit})[name];
  const executablePath=name==='brave'?'/usr/bin/brave-browser':name==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':name==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
  const browser=await type.launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(['chromium','brave'].includes(name)?{args:['--no-sandbox','--mute-audio']}: {})});browsers.push(browser);
  const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}});inspected=page;const errors=[];let posts=0,gets=0,failStatus=false,failCreate=false,abortCreate=false,loseAcknowledgement=false,holdStatus=false,releaseStatus;
  page.on('pageerror',error=>errors.push(error.message));
  await page.route(`**${path}`,async route=>{
   if(route.request().method()==='GET'){gets++;if(holdStatus)await new Promise(resolve=>releaseStatus=resolve);if(failStatus)return route.fulfill({status:503,json:{response:'Test status connection interrupted'}});}
   if(route.request().method()==='POST'){posts++;if(abortCreate)return route.abort('failed');if(failCreate)return route.fulfill({status:502,json:{response:'Test N1 could not store the backup'}});if(loseAcknowledgement){const response=await route.fetch();assert.equal(response.status(),201);return route.abort('failed');}}
   await route.continue();
  });
  await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},user);
  await page.goto(`${origin}/#admin`);await page.getByRole('button',{name:'Backups',exact:true}).click();const panel=page.getByRole('region',{name:'Database backups',exact:true});await panel.waitFor();
  const create=panel.getByRole('button',{name:'Create backup',exact:true});await until(()=>create.isEnabled(),'backup history loaded');
  let entered=false;const gate=new Promise(resolve=>releaseUpload=resolve);
  mx.n1.control.beforeOneShot=async request=>{if(decodeURIComponent(request.url).includes('__mx/backups/')){entered=true;await gate;}};
  await create.click();await panel.getByRole('button',{name:'Creating backup…',exact:true}).waitFor();const progress=panel.getByRole('progressbar',{name:'Database backup activity'});await progress.waitFor();assert.equal(await progress.getAttribute('value'),null,'no invented percentage');
  await until(()=>entered,'real snapshot reached held N1 storage');assert.equal((await call(path,user)).backup_running,true);
  await Promise.all(Array.from({length:8},()=>call(path,user,'POST',undefined,[409])));assert.equal(posts,1,'UI sends one create, server rejects eight concurrent attempts');
  await until(()=>panel.locator('.backup-elapsed').innerText().then(text=>!text.endsWith(' 0s')),'visible elapsed time');
  failStatus=true;await panel.getByRole('button',{name:'Refresh status',exact:true}).click();await panel.getByRole('alert').filter({hasText:'Backup status unavailable'}).waitFor();assert.ok(await progress.isVisible(),'poll failure does not claim completion');failStatus=false;
  await until(()=>panel.getByRole('alert').filter({hasText:'Backup status unavailable'}).count().then(count=>count===0),'automatic status recovery');
  for(const [theme,width] of [['light',1280],['dark',1024],['dark',390],['light',320]]){
   await page.setViewportSize({width,height:900});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);await page.waitForTimeout(100);
   assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'no page-wide overflow');assert.ok(await progress.isVisible());await page.screenshot({path:join(mx.directory,`${name}-backup-${theme}-${width}.png`)});
  }
  await page.setViewportSize({width:1280,height:900});
  await page.getByRole('button',{name:'Audit',exact:true}).click();await page.getByRole('button',{name:'Backups',exact:true}).click();assert.ok(await progress.isVisible(),'feedback survives operations tabs');
  await page.goto(`${origin}/#drive`);await page.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();
  await page.goto(`${origin}/#admin`);await panel.waitFor();await progress.waitFor();await panel.getByText(/Observed for/).waitFor();
  const before=(await call(path,user)).backups.length;releaseUpload();mx.n1.control.beforeOneShot=undefined;
  await until(()=>panel.locator('.backup-feedback.success').count().then(count=>count===1),'server-confirmed completion after navigation');await until(()=>create.isEnabled(),'create enabled after completion');assert.equal(await progress.count(),0);
  const completed=(await call(path,user)).backups;assert.equal(completed.length,before+1);const object=mx.n1.objects.get(completed[0].object_key);assert.ok(object);assert.equal(object.bytes.subarray(0,15).toString(),'SQLite format 3');
  let releaseVerify;const verificationGate=new Promise(resolve=>releaseVerify=resolve);
  const verificationRoute=async route=>{const response=await route.fetch();assert.equal(response.status(),200);await verificationGate;await route.fulfill({response});};
  await page.route(`**${path}/${completed[0].uid}/verify`,verificationRoute);await panel.getByRole('button',{name:'Verify',exact:true}).first().click();
  const verifying=panel.getByRole('button',{name:'Verifying…',exact:true});await verifying.waitFor();assert.ok(await verifying.isDisabled());releaseVerify();
  await until(()=>verifying.count().then(count=>count===0),'verification activity completes');await page.unroute(`**${path}/${completed[0].uid}/verify`,verificationRoute);
  const idleGets=gets;await page.waitForTimeout(2200);assert.equal(gets,idleGets,'idle view stops polling');
  failCreate=true;await create.click();await panel.getByRole('alert').filter({hasText:'Test N1 could not store the backup'}).waitFor();await until(()=>create.isEnabled(),'create failure releases controls');assert.equal(await progress.count(),0);failCreate=false;
  // Commit succeeds but its HTTP acknowledgement is lost. Hold status checks
  // long enough to inspect the uncertain state; never replay the create POST.
  await page.waitForTimeout(1100);holdStatus=true;loseAcknowledgement=true;const beforeLost=posts;
  await create.click();await panel.getByRole('button',{name:'Checking backup…',exact:true}).waitFor();assert.ok(await progress.isVisible());assert.equal(posts,beforeLost+1);assert.ok(await panel.getByRole('button',{name:'Checking backup…',exact:true}).isDisabled());
  await until(()=>typeof releaseStatus==='function','uncertain operation reconciliation');holdStatus=false;releaseStatus();
  await until(()=>panel.locator('.backup-feedback.success').count().then(count=>count===1),'lost acknowledgement reconciles from verified history');await until(()=>create.isEnabled(),'reconciled completion releases controls');assert.equal(posts,beforeLost+1,'no automatic replay of uncertain backup creation');loseAcknowledgement=false;
  abortCreate=true;const beforeAborted=posts;await create.click();await panel.getByRole('button',{name:'Checking backup…',exact:true}).waitFor();
  await panel.getByRole('alert').filter({hasText:'no new completed backup is recorded'}).waitFor();await until(()=>create.isEnabled(),'unacknowledged/uncommitted request can be retried only after status reconciliation');
  assert.equal(await progress.count(),0);assert.equal(posts,beforeAborted+1);abortCreate=false;
  assert.deepEqual(errors,[]);assert.deepEqual(mx.n1.failures,[]);results.push({browser:name,immediate_activity:true,elapsed:true,indeterminate:true,server_confirmed:true,concurrent_attempts_rejected:8,status_recovery:true,navigation:true,lost_acknowledgement:true,uncommitted_request_reconciled:true,verification_activity:true,idle_polling_stopped:true,responsive:true});
  console.log(`${name}: backup feedback, real snapshot, concurrent guard, navigation, failures and lost acknowledgement passed`);await browser.close();
 }
 await writeFile(join(mx.directory,'backup-results.json'),JSON.stringify({passed:true,results},null,2));
}catch(error){if(inspected&&!inspected.isClosed())await inspected.screenshot({path:join(mx.directory,'backup-failed.png')}).catch(()=>{});throw error;}
finally{releaseUpload?.();for(const browser of browsers)await browser.close();await mx.close();}
