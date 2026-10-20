// Test application routing/dedup and recovery with instrumented browser delivery.
// Headless tests do not claim that a Windows OS banner was visibly rendered.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const browsers=[],results={browsers:{},limitation:'OS desktop banner delivery still requires a real target OS/browser/certificate test. No physical audio output.'};
let currentPages=[],currentEngine='';
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const until=async(check,label)=>{const deadline=Date.now()+20000;while(Date.now()<deadline){if(await check())return;await new Promise(r=>setTimeout(r,80));}throw Error(label);};
try{
  console.log(`Notification artifacts: ${mx.directory}`);
  const password=`Disposable!${randomUUID()}`;
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@notify.test',name:'Owner',password},[201]);
  const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@notify.test',password});
  await call('/mx/v1/user/create',owner,'POST',{email:'recipient@notify.test',name:'Recipient',password,access_level:2},[201]);
  const recipient=await call('/mx/v1/auth/authenticate',null,'POST',{email:'recipient@notify.test',password});
  await call('/mx/v1/account/preferences',recipient,'PUT',{preferences:{notifications:{browser_enabled:true,sound_enabled:false,messages:true,digest:'immediate'}}});
  const channel=await call('/mx/v1/collaboration/direct',owner,'POST',{user_uid:recipient.uid},[200,201]);
  let sequence=0;
  const send=()=>call(`/mx/v1/collaboration/channels/${channel.uid}/messages`,owner,'POST',{body:`Notification ${++sequence}`,operation_uid:randomUUID()},[200,201]);
  for(const [engine,launcher,executablePath] of [
    ['chromium',chromium,'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome'],
    ['firefox',firefox,'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox'],
    ['webkit',webkit,'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh'],
    ...(process.env.MX_TEST_BRAVE_EXECUTABLE?[['brave',chromium,process.env.MX_TEST_BRAVE_EXECUTABLE]]:[])]){
    if(process.env.MX_TEST_BROWSERS&&!process.env.MX_TEST_BROWSERS.split(',').includes(engine))continue;
    const browser=await launcher.launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(['chromium','brave'].includes(engine)?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1440,height:950}});
    currentEngine=engine;currentPages=[];
    await context.addInitScript(tokens=>{
      // A mis-set workstation clock must not skip server notifications or
      // incorrectly classify freshly recovered notifications as a day old.
      const nativeNow=Date.now;Date.now=()=>nativeNow()+86400000;
      window.__desktopStatuses=[];window.addEventListener('mx-desktop-status',event=>window.__desktopStatuses.push(event.detail));
      window.__networkTrace=[];const nativeFetch=window.fetch;window.fetch=async(input,options)=>{const url=String(input);const trace={url,started:Date.now(),aborted:options?.signal?.aborted};window.__networkTrace.push(trace);try{const response=await nativeFetch(input,options);trace.status=response.status;return response;}catch(error){trace.error=String(error);throw error;}};
      sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);
      window.__mxFocus=true;window.__mxVisible='visible';window.__desktopCalls=[];
      Object.defineProperty(document,'visibilityState',{get:()=>window.__mxVisible});document.hasFocus=()=>window.__mxFocus;
      class TestNotification {static permission='granted';constructor(title,options){if(window.__rejectDesktop)throw new DOMException('Disposable native rejection','NotAllowedError');window.__desktopCalls.push({title,options});}close(){}}
      window.Notification=TestNotification;
      window.__liveSockets=[];const NativeWebSocket=window.WebSocket;window.WebSocket=class extends NativeWebSocket{constructor(...args){super(...args);window.__liveSockets.push(this);}};
      window.__setMxFocus=(focused,visible=focused?'visible':'hidden')=>{window.__mxFocus=focused;window.__mxVisible=visible;window.dispatchEvent(new Event(focused?'focus':'blur'));document.dispatchEvent(new Event('visibilitychange'));};
    },recipient);
    const page=await context.newPage();currentPages.push(page);const errors=[];page.on('pageerror',e=>errors.push(e.message));
    let initialInboxFailures=0;
    const initialInboxPattern='**/mx/v1/notifications?unread_only=false';
    await context.route(initialInboxPattern,async route=>{initialInboxFailures++;await route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({response:'Disposable initial inbox outage'})});});
    await page.goto(`https://127.0.0.1:${mx.port}/#account`);
    await page.getByRole('heading',{name:'Notifications',exact:true}).waitFor();
    await until(async()=>initialInboxFailures>=6,'Initial inbox requests exhausted their bounded retries');
    // An HTTP inbox response alone is not initialization: parsing/preferences
    // can still be pending. The recovery cursor request proves readiness.
    const recoveredInitialization=page.waitForResponse(response=>response.url().includes('/mx/v1/notifications?limit=100&after=')&&response.status()===200);
    await context.unroute(initialInboxPattern);await page.evaluate(()=>window.dispatchEvent(new Event('online')));await recoveredInitialization;
    await until(async()=>await page.locator('.live-status').count()===0||await page.locator('.live-status').innerText().then(t=>t.includes('Live')).catch(()=>false),'Live connection');
    await send();await page.locator('.notification-toast').first().waitFor();
    assert.equal(await page.evaluate(()=>window.__desktopCalls.length),0,'Focused MX uses only web notifications');
    await page.evaluate(()=>window.__setMxFocus(false));await send();
    await until(async()=>(await page.evaluate(()=>window.__desktopCalls.length))===1,'Minimized/background desktop delivery');
    assert.equal(await page.evaluate(()=>window.__desktopCalls[0].options.silent),true);
    await page.evaluate(()=>window.__setMxFocus(true));await send();
    await until(async()=>(await page.locator('.notification-toast').count())>=3,'Foreground web alerts continue');
    assert.equal(await page.evaluate(()=>window.__desktopCalls.length),1,'Returning to MX suppresses OS popup');
    // Another MX tab focused must suppress the hidden tab as well.
    const other=await context.newPage();currentPages.push(other);await other.goto(`https://127.0.0.1:${mx.port}/#account`);await other.getByRole('heading',{name:'Notifications',exact:true}).waitFor();
    await page.evaluate(()=>window.__setMxFocus(false));await other.evaluate(()=>window.__setMxFocus(true));await send();
    await until(async()=>(await other.locator('.notification-toast').count())>=1,'Second tab received event');
    assert.equal(await page.evaluate(()=>window.__desktopCalls.length),1,'Background tab does not override focused MX tab');
    assert.equal(await other.evaluate(()=>window.__desktopCalls.length),0);
    await other.evaluate(()=>window.__setMxFocus(false));await send();
    await until(async()=>(await page.evaluate(()=>window.__desktopCalls.length))+(await other.evaluate(()=>window.__desktopCalls.length))===2,'Exactly one background OS submission across tabs');
    await context.setOffline(true);
    const canceled=await page.evaluate(()=>{const cancel=event=>event.preventDefault();window.addEventListener('beforeunload',cancel,{capture:true});const event=new Event('beforeunload',{cancelable:true});window.dispatchEvent(event);window.removeEventListener('beforeunload',cancel,{capture:true});return event.defaultPrevented;});assert.equal(canceled,true);
    for(const tab of [page,other])await tab.evaluate(()=>window.__liveSockets.forEach(socket=>socket.close()));
    await send();await send();await send();
    const recovered=page.waitForResponse(response=>response.url().includes('/mx/v1/notifications?limit=100&after=')&&response.status()===200);
    await context.setOffline(false);
    for(const tab of [page,other])await tab.evaluate(()=>window.dispatchEvent(new Event('online')));
    await recovered;
    await until(async()=>(await page.evaluate(()=>window.__desktopCalls.length))+(await other.evaluate(()=>window.__desktopCalls.length))===5,'Durable catch-up recovers three missed notifications once across tabs');
    // Diagnostics must retain actual failures when the user returns to MX,
    // and a draft checkbox must not run a test using old server preferences.
    await page.evaluate(()=>window.__setMxFocus(true));
    const diagnostics=page.getByRole('status',{name:'Desktop notification diagnostics',exact:true});
    const testButton=page.getByRole('button',{name:'Test when I switch away',exact:true});
    await page.getByRole('combobox',{name:'Delivery',exact:true}).selectOption('daily');
    assert.equal(await testButton.isDisabled(),true);assert.ok((await diagnostics.innerText()).includes('Unsaved notification settings'));
    await page.getByRole('combobox',{name:'Delivery',exact:true}).selectOption('immediate');
    await testButton.click();await page.evaluate(()=>{window.__rejectDesktop=true;window.__setMxFocus(false);});
    await until(async()=>(await diagnostics.innerText()).includes('browser rejected desktop delivery'),'Failed test has an actual rejection result');
    await page.evaluate(()=>window.__setMxFocus(true));
    assert.ok((await diagnostics.innerText()).includes('browser rejected desktop delivery'),'Refocusing cannot replace a failure with Ready');
    assert.equal(await diagnostics.getByText('A test is queued.',{exact:false}).count(),0);
    assert.equal(await testButton.isEnabled(),true,'A completed rejected test can be retried');
    await page.evaluate(()=>{window.__rejectDesktop=false;location.hash='drive';});await page.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();
    await page.evaluate(()=>{location.hash='account';});await page.getByRole('heading',{name:'Notifications',exact:true}).waitFor();
    assert.ok((await diagnostics.innerText()).includes('browser rejected desktop delivery'),'Last account result survives switching modules');
    await page.screenshot({path:join(mx.directory,`${engine}-notification-settings.png`)});
    assert.deepEqual(errors,[]);results.browsers[engine]={passed:true,checks:['initial inbox outage recovery','focused suppression','minimized/background submission','focus return','multi-tab focused suppression','cross-tab deduplication','offline/reconnect catch-up','cancelled-exit recovery','one-day client clock skew','unsaved settings guard','real rejection diagnostics preserved across focus and module changes']};
    console.log(`${engine}: focus-aware notification checks passed`);await context.close();
  }
}catch(error){for(const[tab,page]of currentPages.entries())if(!page.isClosed()){const trace=await page.evaluate(()=>({url:location.href,visible:document.visibilityState,focus:document.hasFocus(),network:window.__networkTrace.filter(t=>t.url.includes('notifications')),desktop:window.__desktopCalls,statuses:window.__desktopStatuses}));await writeFile(join(mx.directory,`${currentEngine}-tab-${tab}-failure.json`),JSON.stringify(trace,null,2));console.error(JSON.stringify(trace));}throw error;}
finally{await writeFile(join(mx.directory,'notification-focus-results.json'),JSON.stringify(results,null,2));for(const browser of browsers)await browser.close();await mx.close();}
