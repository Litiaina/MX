// Real MX HTTP/SQLite and production UI; storage bytes use only the N1 fixture.
// Audio is disconnected per process; never touches workstation audio settings.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {DatabaseSync} from 'node:sqlite';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';

if(process.env.MX_TEST_N1_URL) throw new Error('Drive sharing test requires the isolated N1 fixture, not a live deployment.');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const base='/mx/v1/drive',browsers=[],results=[],failures=[];
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const until=async(check,label)=>{const deadline=Date.now()+30000;while(Date.now()<deadline){if(await check())return;await new Promise(r=>setTimeout(r,100));}throw new Error(label);};
const messages=(channel,user)=>call(`/mx/v1/collaboration/channels/${channel}/messages`,user);
const shares=(channel,user,q='',offset=0)=>call(`${base}/conversations/${channel}/items?${new URLSearchParams({q,offset:String(offset)})}`,user);
const notifications=user=>call('/mx/v1/notifications?limit=100',user);
const browserEnvironment={...process.env,PULSE_SERVER:`unix:${mx.directory}/no-audio.sock`,PIPEWIRE_REMOTE:'mx_test_disconnected',ALSA_CONFIG_PATH:fileURLToPath(new URL('./alsa-null.conf',import.meta.url)),SDL_AUDIODRIVER:'dummy',ALSOFT_DRIVERS:'null'};
async function silentVideo(){
  const browser=await chromium.launch({headless:true,executablePath:'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome',env:browserEnvironment,args:['--no-sandbox','--mute-audio']});browsers.push(browser);
  try{
    const page=await browser.newPage();
    return Buffer.from(await page.evaluate(async()=>{
      const canvas=document.createElement('canvas');canvas.width=64;canvas.height=64;
      const context=canvas.getContext('2d'),stream=canvas.captureStream(10),chunks=[];
      const recorder=new MediaRecorder(stream,{mimeType:'video/webm;codecs=vp8'});
      const stopped=new Promise(resolve=>recorder.onstop=resolve);recorder.ondataavailable=event=>chunks.push(event.data);recorder.start();
      for(let i=0;i<10;i++){context.fillStyle=i%2?'#184d90':'#258864';context.fillRect(0,0,64,64);await new Promise(resolve=>setTimeout(resolve,100));}
      recorder.stop();await stopped;stream.getTracks().forEach(track=>track.stop());
      return Array.from(new Uint8Array(await new Blob(chunks).arrayBuffer()));
    }));
  }finally{await browser.close();}
}
const grant=(item,user,target,role='viewer')=>call(`${base}/items/${item.uid}/sharing`,user,'POST',{user_uid:target.uid,role});
const groupGrant=(item,user,group,role='viewer')=>call(`${base}/items/${item.uid}/sharing`,user,'POST',{space_uid:group.uid,role});
async function upload(user,name,bytes=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/l9sAAAAASUVORK5CYII=','base64'),mime='image/png'){
  const operation=randomUUID();
  await call(`${base}/uploads`,user,'POST',{operation_uid:operation,file_name:name,mime_type:mime,size:bytes.length,last_modified:1});
  for(let attempt=0;attempt<3;attempt++){
    const response=await mx.api.put(`${base}/uploads/${operation}/parts/0`,{headers:{Authorization:`Bearer ${user.access_token}`,'Content-Type':'application/octet-stream'},data:bytes});
    if(response.status()===502&&attempt<2)continue;
    assert.equal(response.status(),200,await response.text());break;
  }
  return call(`${base}/uploads/${operation}/finish`,user,'POST');
}
async function pageFor(browser,user){
  const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1440,height:1000}});
  await context.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},user);
  const page=await context.newPage();await page.goto(`https://127.0.0.1:${mx.port}/#drive`);
  await page.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();
  await until(()=>page.locator('.live-status').count().then(async n=>n?true:await page.getByText('Live',{exact:true}).count()>0),'Live connection');
  return page;
}
async function shareUi(page,item,query,target,group=false){
  await page.getByRole('textbox',{name:'Search MX Drive',exact:true}).fill(item.name);
  await page.getByRole('button',{name:'Search Drive',exact:true}).click();
  await page.getByRole('button',{name:`More actions for ${item.name}`,exact:true}).click();
  await page.getByRole('menuitem',{name:'Share',exact:true}).click();
  if(group)await page.getByRole('button',{name:'Spaces & groups',exact:true}).click();
  await page.getByRole('textbox',{name:group?'Find a collaboration space':'Find an MX account',exact:true}).fill(query);
  await page.getByRole('button',{name:'Find',exact:true}).click();
  await page.getByRole('button',{name:`Share with ${group?'space ':''}${target}`,exact:true}).click();
  await page.getByRole('status').filter({hasText:'Shared in the conversation'}).waitFor();
}
async function holdItemResponse(page,item){
  let resume,started,finished,failure;
  const gate=new Promise(resolve=>resume=resolve),seen=new Promise(resolve=>started=resolve),done=new Promise(resolve=>finished=resolve);
  const pattern=`**${base}/items/${item.uid}`;
  const handler=async route=>{
    try{const response=await route.fetch();started();await gate;await route.fulfill({response});}
    catch(reason){failure=reason;started();}finally{finished();}
  };
  await page.route(pattern,handler);
  return{started:seen,release:async()=>{resume();await done;await page.unroute(pattern,handler);if(failure)throw failure;}};
}
async function openModule(page,module){
  await page.getByRole('complementary',{name:'Workspace navigation'}).getByRole('button',{name:'Modules',exact:true}).click();
  await page.getByRole('dialog',{name:'Browse modules',exact:true}).getByRole('button',{name:`Open ${module.name}`,exact:true}).click();
  await page.waitForURL(new RegExp(`#module/${module.uid}`));
}
async function videoNavigationChecks(page,item,other,channel,module){
  const navigation=page.getByRole('navigation',{name:'Main navigation'}),close=page.getByRole('button',{name:'Close preview',exact:true});
  const chatHash=`collaboration?channel=${channel}`;
  const chatCard=page.locator('.chat-message').getByRole('link',{name:`Open ${item.name} in MX Drive`,exact:true});
  const openFromChat=async()=>{await page.evaluate(hash=>location.hash=hash,chatHash);await chatCard.waitFor();await chatCard.click();await close.waitFor();await page.waitForFunction(()=>document.querySelector('.preview-dialog video')?.readyState>=2);};
  const plainDrive=async()=>{const loaded=page.waitForResponse(response=>response.request().method()==='GET'&&new URL(response.url()).pathname===base);await navigation.getByRole('button',{name:'MX Drive',exact:true}).click();await loaded;await page.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();};
  const noPreview=async label=>{await page.waitForTimeout(200);assert.equal(await close.count(),0,label);assert.equal(await page.locator('.preview-dialog video').count(),0,label);};

  assert.ok(!page.url().includes('item='),'Closing consumes the shared-file URL intent');
  for(const destination of['Dashboard','Collaboration']){
    await navigation.getByRole('button',{name:destination,exact:true}).click();await page.waitForURL(new RegExp(`#${destination.toLowerCase()}$`));
    await plainDrive();await noPreview(`${destination} → Drive must remain closed`);
  }
  await page.reload();await page.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();await noPreview('Reload must not resurrect a dismissed video');
  await openFromChat();await page.locator('.preview-dialog video').evaluate(async video=>{video.muted=true;await video.play();});
  await page.goBack();await chatCard.waitFor();await noPreview('Back navigation removes the video player');
  await page.goForward();await close.waitFor();await page.waitForFunction(()=>document.querySelector('.preview-dialog video')?.readyState>=2);
  await page.keyboard.press('Escape');await close.waitFor({state:'detached'});
  assert.ok(!page.url().includes('item='),'Escape also clears the shared-file URL intent');
  await page.goBack();await chatCard.waitFor();await page.goForward();await page.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();await noPreview('Forward must not resurrect a dismissed video');
  await openFromChat();await page.locator('.preview-overlay').click({position:{x:5,y:5}});await close.waitFor({state:'detached'});
  assert.ok(!page.url().includes('item='),'Backdrop dismissal clears the shared-file URL intent');

  let staleTickets=0;
  const countTicket=request=>{if(request.method()==='POST'&&new URL(request.url()).pathname===`${base}/items/${item.uid}/ticket`)staleTickets++;};
  page.on('request',countTicket);
  for(const action of['module','drive-view','new-link']){
    const held=await holdItemResponse(page,item);
    try{
      await page.evaluate(uid=>location.hash=`drive?item=${uid}`,item.uid);await held.started;
      if(action==='module'){
        await openModule(page,module);await plainDrive();
      }else if(action==='drive-view'){
        await page.getByRole('navigation',{name:'Drive views'}).getByRole('button',{name:'My Drive',exact:true}).click();
      }else{
        await page.evaluate(uid=>location.hash=`drive?item=${uid}`,other.uid);
        await page.locator('.preview-dialog').getByRole('heading',{name:other.name,exact:true}).waitFor();
      }
    }finally{await held.release();}
    if(action==='new-link'){
      await page.waitForTimeout(200);await page.locator('.preview-dialog').getByRole('heading',{name:other.name,exact:true}).waitFor();await close.click();
    }else await noPreview(`Delayed metadata after ${action} must not reopen the video`);
    assert.ok(!page.url().includes('item='),'Dismissed or superseded URL intent is cleared');
  }
  page.off('request',countTicket);assert.equal(staleTickets,0,'Cancelled shared-video requests must never issue a preview ticket');
  await openFromChat();await close.click();await noPreview('Explicitly selecting the same shared video still works after cancellation');
}
function seedOldShares(owner,recipient,group){
  const c=new DatabaseSync(join(mx.directory,'test.db'));c.exec('PRAGMA busy_timeout=10000;PRAGMA foreign_keys=ON;BEGIN IMMEDIATE');
  try{
    const ids=[];
    for(let i=0;i<55;i++){
      const uid=randomUUID();ids.push(uid);
      c.prepare("INSERT INTO mx_drive_items(uid,owner_uid,name,kind,created_at,updated_at) VALUES(?,?,?,'folder',1,1)").run(uid,owner.uid,`Old share ${String(i).padStart(2,'0')}`);
      c.prepare("INSERT INTO mx_drive_space_grants(item_uid,space_uid,role) VALUES(?,?,'viewer')").run(uid,group.uid);
      if(i===0)c.prepare("INSERT INTO mx_drive_grants(item_uid,user_uid,role) VALUES(?,?,'viewer')").run(uid,recipient.uid);
    }
    c.exec('COMMIT');return ids;
  }catch(error){c.exec('ROLLBACK');throw error;}finally{c.close();}
}
try{
  console.log(`Drive sharing artifacts: ${mx.directory}`);
  const password=`Test-only!${randomUUID()}`;
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@test.invalid',name:'Drive Owner',password},[201]);
  const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@test.invalid',password}),users=[];
  for(let i=1;i<8;i++){
    await call('/mx/v1/user/create',owner,'POST',{email:`recipient${i}@test.invalid`,name:`Recipient ${i}`,password,access_level:2},[201]);
    users.push(await call('/mx/v1/auth/authenticate',null,'POST',{email:`recipient${i}@test.invalid`,password}));
  }
  // Fresh installations intentionally have no starter module. Provision this
  // navigation fixture explicitly rather than depending on a removed default.
  const navigationModule=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Sharing navigation',singular_name:'Entry'},[201])).module;
  const moduleAccess=await call(`/mx/v1/admin/accounts/${users[0].uid}/modules`,owner);
  await call(`/mx/v1/admin/accounts/${users[0].uid}/modules`,owner,'PUT',{revision:moduleAccess.revision,grants:[{module_uid:navigationModule.uid,can_read:true,can_create:false,can_update:false,can_delete:false,can_configure:false,can_report:true,can_attachments:true}]});
  const file=await upload(owner,'Concurrent reference.png');
  const group=await call('/mx/v1/collaboration/channels',owner,'POST',{kind:'group',name:'Eight users',member_uids:users.map(u=>u.uid)},[201]);
  const requestsBefore=mx.n1.requests.length;
  const sent=await Promise.all(users.map(user=>grant(file,owner,user)));
  assert.equal(new Set(sent.map(r=>r.channel_uid)).size,7);
  const replay=await Promise.all(Array.from({length:8},()=>grant(file,owner,users[0])));
  assert.equal(replay.filter(r=>r.changed).length,0);
  const groupReplays=await Promise.all(Array.from({length:8},()=>groupGrant(file,owner,group)));
  assert.equal(groupReplays.filter(r=>r.changed).length,1);
  const groupMessages=await messages(group.uid,users[0]);
  assert.equal(groupMessages.messages.length,1);assert.equal(groupMessages.messages[0].drive_links[0].item.uid,file.uid);
  for(let i=0;i<users.length;i++){
    const list=await notifications(users[i]);assert.equal(list.notifications.filter(n=>n.kind==='file.shared').length,2);
    assert.equal((await messages(sent[i].channel_uid,users[i])).messages.length,1);
  }
  assert.equal((await notifications(owner)).notifications.filter(n=>n.kind==='file.shared').length,0);
  assert.equal(mx.n1.requests.length,requestsBefore,'Sharing does not upload, download or copy N1 bytes');
  const old=seedOldShares(owner,users[0],group);
  const pages=await Promise.all([0,50].map(offset=>shares(group.uid,users[0],'',offset)));
  assert.deepEqual(pages.map(p=>p.items.length),[50,6]);assert.equal(pages[0].total,56);
  assert.equal(new Set(pages.flatMap(p=>p.items.map(i=>i.uid))).size,56);
  assert.equal((await shares(group.uid,users[0],'Old share 54')).items.length,1);
  assert.ok((await shares(sent[0].channel_uid,users[0])).items.some(i=>i.uid===old[0]),'Old personal share remains discoverable');
  const outsider=await call('/mx/v1/collaboration/channels',owner,'POST',{kind:'group',name:'Private unrelated',member_uids:[]},[201]);
  await call(`${base}/conversations/${outsider.uid}/items`,users[0],'GET',undefined,[403]);
  await call(`${base}/items/${file.uid}/sharing`,users[0],'POST',{space_uid:group.uid,role:'editor'},[403]);
  await mx.restart();assert.equal((await messages(group.uid,users[0])).messages.length,1);
  assert.equal((await notifications(users[0])).notifications.filter(n=>n.kind==='file.shared').length,2);
  results.push({concurrentAccounts:8,replays:'one message per grant',storage:'metadata only',oldShares:'56 items over two pages',durability:'MX restart',ACL:'nonmember and nonowner denied'});
  console.log(JSON.stringify(results.at(-1)));
  // One-second synthetic, silent VP8 video: no personal files or audio devices.
  const video=await silentVideo();
  for(const engine of(process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
    const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:browserEnvironment,...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const item=await upload(owner,`UI ${engine}.webm`,video,'video/webm'),teamName=`UI ${engine} Team`,team=await call('/mx/v1/collaboration/channels',owner,'POST',{kind:'group',name:teamName,member_uids:[users[0].uid]},[201]);
    const recipient=await pageFor(browser,users[0]),sender=await pageFor(browser,owner),errors=[];
    recipient.on('pageerror',e=>errors.push(e.message));sender.on('pageerror',e=>errors.push(e.message));
    await shareUi(sender,item,'Recipient 1','Recipient 1');
    await recipient.locator('.notification-toast').filter({hasText:item.name}).waitFor();
    await recipient.locator('.notification-toast').filter({hasText:item.name}).getByRole('button').first().click();
    const card=recipient.locator('.chat-message').getByRole('link',{name:`Open ${item.name} in MX Drive`,exact:true});
    await card.waitFor();await card.click();await recipient.getByRole('button',{name:'Close preview',exact:true}).waitFor();
    await recipient.waitForFunction(()=>document.querySelector('.preview-dialog video')?.readyState>=2);
    assert.ok(recipient.url().includes(`item=${item.uid}`));await recipient.getByRole('button',{name:'Close preview',exact:true}).click();
    let staleOpens=0;
    const countStale=request=>{if(request.method()==='GET'&&new URL(request.url()).pathname===`${base}/items/${item.uid}`)staleOpens++;};
    recipient.on('request',countStale);
    await openModule(recipient,navigationModule);
    const driveLoaded=recipient.waitForResponse(response=>response.request().method()==='GET'&&new URL(response.url()).pathname===base);
    await recipient.getByRole('navigation',{name:'Main navigation'}).getByRole('button',{name:'MX Drive',exact:true}).click();
    await driveLoaded;await recipient.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();
    await recipient.waitForTimeout(500);
    recipient.off('request',countStale);
    assert.equal(staleOpens,0,'Module → Drive must not reload the dismissed shared video');
    assert.equal(await recipient.getByRole('button',{name:'Close preview',exact:true}).count(),0,'Module → Drive must not reopen the dismissed preview');
    await videoNavigationChecks(recipient,item,file,(await grant(item,owner,users[0])).channel_uid,navigationModule);
    await sender.getByRole('button',{name:'Close dialog',exact:true}).click();
    await shareUi(sender,item,teamName,teamName,true);
    await recipient.locator('.notification-toast').filter({hasText:item.name}).waitFor();
    await recipient.locator('.notification-toast').filter({hasText:item.name}).getByRole('button').first().click();
    await recipient.locator('.chat-message').getByRole('link',{name:`Open ${item.name} in MX Drive`,exact:true}).waitFor();
    await recipient.getByRole('button',{name:'Toggle conversation details',exact:true}).click();
    await recipient.getByRole('region',{name:'Shared from MX Drive',exact:true}).getByRole('link',{name:`Open ${item.name} in MX Drive`,exact:true}).waitFor();
    await recipient.getByText('Chat attachments',{exact:true}).waitFor();
    const panel=recipient.getByRole('region',{name:'Shared from MX Drive',exact:true});
    await panel.getByRole('searchbox',{name:'Search shared files',exact:true}).fill('no matching item');
    await panel.getByRole('button',{name:'Search',exact:true}).click();
    await panel.getByText('No Drive shares match your search.',{exact:true}).waitFor();
    await panel.getByRole('searchbox',{name:'Search shared files',exact:true}).fill('');
    await panel.getByRole('button',{name:'Search',exact:true}).click();
    await panel.getByRole('link',{name:`Open ${item.name} in MX Drive`,exact:true}).waitFor();
    for(const theme of ['light','dark']){
      await recipient.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);
      await recipient.screenshot({path:join(mx.directory,`${engine}-shares-${theme}.png`)});
    }
    await recipient.setViewportSize({width:390,height:850});
    await recipient.waitForFunction(()=>document.querySelector('.sidebar').getBoundingClientRect().right<=0);
    const mobileCard=recipient.getByRole('region',{name:'Shared from MX Drive'}).getByRole('link',{name:`Open ${item.name} in MX Drive`,exact:true});
    const geometry=await mobileCard.evaluate(node=>{const box=node.getBoundingClientRect();return{left:box.left,right:box.right,width:innerWidth,overflow:node.scrollWidth>node.clientWidth+1};});
    assert.ok(geometry.left>=0&&geometry.right<=geometry.width+1&&!geometry.overflow,JSON.stringify(geometry));
    assert.ok(await recipient.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'No mobile viewport overflow');
    await recipient.screenshot({path:join(mx.directory,`${engine}-shares-mobile.png`)});
    await mobileCard.click();await recipient.getByRole('button',{name:'Close preview',exact:true}).waitFor();
    await recipient.getByRole('button',{name:'Close preview',exact:true}).click();
    await recipient.setViewportSize({width:1440,height:1000});
    await recipient.goto(`https://127.0.0.1:${mx.port}/#collaboration?channel=${team.uid}`);
    await recipient.locator('.chat-message').getByRole('link',{name:`Open ${item.name} in MX Drive`,exact:true}).waitFor();
    await recipient.getByRole('button',{name:'Toggle conversation details',exact:true}).click();
    await recipient.getByRole('region',{name:'Shared from MX Drive'}).getByRole('link',{name:`Open ${item.name} in MX Drive`,exact:true}).waitFor();
    // Both grants must be removed: a second valid grant correctly keeps access.
    await grant(item,owner,users[0],null);await groupGrant(item,owner,team,null);
    await until(()=>recipient.locator('.chat-message .drive-share-card.unavailable').count().then(n=>n===1),'Revocation redacts the current card');
    await until(()=>recipient.getByRole('region',{name:'Shared from MX Drive'}).getByRole('link').count().then(n=>n===0),'Revocation removes the group shared-file entry');
    await call(`${base}/items/${item.uid}/ticket`,users[0],'POST',{},[404]);
    assert.deepEqual(errors,[]);results.push({engine,personal:'toast → chat → real silent-video preview',navigation:'module/dashboard/chat round trips, reload, back/forward, button/Escape/backdrop dismissal, repeated explicit opening',delayedMetadata:'module exit, Drive-view change and newer link cancel obsolete preview/ticket',group:'toast + card + shared-file view',revocation:'card unavailable + ticket denied',themes:'light, dark, mobile'});
    console.log(JSON.stringify(results.at(-1)));await browser.close();
  }
}catch(error){failures.push(error.stack);process.exitCode=1;for(const browser of browsers)for(const context of browser.contexts())for(const page of context.pages())await page.screenshot({path:join(mx.directory,`sharing-failure-${browsers.indexOf(browser)}-${browser.contexts().indexOf(context)}-${context.pages().indexOf(page)}.png`),fullPage:true}).catch(()=>{});}
finally{for(const browser of browsers)await browser.close().catch(()=>{});await writeFile(join(mx.directory,'sharing-results.json'),JSON.stringify({passed:!failures.length,results,failures},null,2));console.log(JSON.stringify({passed:!failures.length,results,failures,artifacts:mx.directory}));await mx.close();}
