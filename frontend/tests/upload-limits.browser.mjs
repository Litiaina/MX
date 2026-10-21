// Real 66 MiB module uploads; isolated MX/N1 contract only, all artifacts on HDD.
import assert from 'node:assert/strict';
import { randomUUID, createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { readFile, writeFile } from 'node:fs/promises';
import { request as httpsRequest } from 'node:https';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';

assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/'), 'Use only the HDD');
assert.ok(process.env.TMPDIR?.startsWith('/mnt/ST1000LM049/'), 'Browser temporary files stay on HDD');
assert.ok(!process.env.MX_TEST_N1_URL, 'Never use live N1');
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
const browsers = [], results = { passed:false, checks:[], browsers:{} };
let owner, inspected;
const call = async (path, method='GET', data, status=[200], actor=owner) => (await mx.call(path, actor?.access_token, method, data, status)).body;
const until = async (check,label) => { const end=Date.now()+60000; while(Date.now()<end){if(await check())return;await new Promise(resolve=>setTimeout(resolve,100));}throw Error(label); };
const policyPath='/mx/v1/uploads/limits', savePath='/mx/v1/admin/upload-limits';
async function setLimits(record,chat=null) {
  const current=await call(policyPath);
  return call(savePath,'PUT',{revision:current.revision,settings:{record_max_size_mb:record,collaboration_max_size_mb:chat}});
}
try {
  console.log(`Upload-limit artifacts: ${mx.directory}`);
  const password=`Disposable!${randomUUID()}`;
  await mx.call('/mx/v1/auth/create',mx.signupKey,'POST',{email:'owner@limits.test',name:'Owner',password},[201]);
  owner=await call('/mx/v1/auth/authenticate','POST',{email:'owner@limits.test',password});
  await call('/mx/v1/user/create','POST',{email:'member@limits.test',name:'Member',password,access_level:2},[201]);
  const member=await call('/mx/v1/auth/authenticate','POST',{email:'member@limits.test',password});
  const channel=await call('/mx/v1/collaboration/direct','POST',{user_uid:member.uid},[200,201]);
  const initial=await call(policyPath);assert.equal(initial.record_max_size_bytes,100*1024**2);assert.equal(initial.settings.record_max_size_mb,null);
  await call(savePath,'PUT',{revision:initial.revision,settings:{record_max_size_mb:200}},[403],member);
  for(const bad of [-1,1.5,'100'])await call(savePath,'PUT',{revision:initial.revision,settings:{record_max_size_mb:bad}},[400]);
  await call(savePath,'PUT',{revision:initial.revision,settings:{record_max_size_mb_typo:100}},[400]);
  const deploymentBefore=await call('/mx/v1/deployment/config');
  await setLimits(80,120);
  await call(savePath,'PUT',{revision:initial.revision,settings:{record_max_size_mb:20}},[409]);
  assert.equal((await call(policyPath)).record_max_size_bytes,80*1024**2);
  // A stale identity editor must not remove the independently saved policy.
  delete deploymentBefore.config.uploads;
  await call('/mx/v1/admin/deployment-config','PUT',{config:deploymentBefore.config});
  assert.equal((await call(policyPath)).record_max_size_bytes,80*1024**2);
  assert.equal((await call('/mx/v1/collaboration/channels')).file_max_size_bytes,120*1024**2);
  const raced=await call(policyPath);
  const writers=await Promise.all(Array.from({length:8},(_,index)=>mx.call(savePath,owner.access_token,'PUT',{revision:raced.revision,settings:{record_max_size_mb:80+index}},[200,409])));
  assert.equal(writers.filter(item=>item.status===200).length,1);
  assert.equal(writers.filter(item=>item.status===409).length,7);
  await setLimits(null,null);
  results.checks.push('Authenticated policy; administrator-only writes; invalid limits rejected; stale identity preserved; revision conflict; eight writes one winner; independent chat limit');

  const module=(await call('/mx/v1/admin/modules','POST',{name:'Upload limits test',singular_name:'Upload entry'},[201])).module;
  await call(`/mx/v1/admin/modules/${module.uid}/fields`,'POST',{key:'name',label:'Name',field_type:'text'},[201]);
  const field=await call(`/mx/v1/admin/modules/${module.uid}/fields`,'POST',{key:'files',label:'Documents',field_type:'attachments',config:{multiple:true}},[201]);
  const fieldUid=field.uid||field.field.uid;
  const create=async name=>call(`/mx/v1/modules/${module.uid}/records`,'POST',{values:{name}},[201]);
  const get=uid=>call(`/mx/v1/modules/${module.uid}/records/${uid}`);
  const upload=async(record,name,bytes,status=201)=>{
    const response=await mx.api.post(`/mx/v1/records/${record.uid}/attachments/fields/${fieldUid}`,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{base_revision:String(record.revision),files:{name,mimeType:'application/octet-stream',buffer:bytes}}});
    assert.equal(response.status(),status,await response.text());return response.json();
  };
  const rejectedRecord=await create('Server boundaries');
  await setLimits(1);
  const startRequests=mx.n1.requests.length;
  const rejected=await upload(rejectedRecord,'Too-large.bin',Buffer.alloc(1024**2+1,9),413);
  assert.ok(rejected.response.includes('1 MiB'));assert.ok(rejected.response.includes('Administration'));
  // Expect/continue makes early Content-Length rejection observable without
  // the HTTP client's writer racing the server's deliberate body shutdown.
  const bodyRejected=await new Promise((resolve,reject)=>{
    const request=httpsRequest({hostname:'127.0.0.1',port:mx.port,path:`/mx/v1/records/${rejectedRecord.uid}/attachments/fields/${fieldUid}`,method:'POST',rejectUnauthorized:false,headers:{Authorization:`Bearer ${owner.access_token}`,'Content-Type':'multipart/form-data; boundary=oversized','Content-Length':3*1024**2,Expect:'100-continue'}},response=>{const chunks=[];response.on('data',chunk=>chunks.push(chunk));response.on('end',()=>resolve({status:response.statusCode,body:JSON.parse(Buffer.concat(chunks))}));});
    request.on('continue',()=>reject(Error('Oversized Content-Length must be rejected before reading the body')));request.on('error',reject);request.flushHeaders();
  });assert.equal(bodyRejected.status,413);assert.ok(bodyRejected.body.response.includes('1 MiB'));
  assert.deepEqual(await get(rejectedRecord.uid),rejectedRecord);
  assert.equal(mx.n1.requests.length,startRequests,'Oversized files never reach N1');
  // No Content-Length: streamed multipart still cannot bypass the per-file guard.
  const boundary='limits-'+randomUUID();
  const chunked=await new Promise((resolve,reject)=>{
    const request=httpsRequest({hostname:'127.0.0.1',port:mx.port,path:`/mx/v1/records/${rejectedRecord.uid}/attachments/fields/${fieldUid}`,method:'POST',rejectUnauthorized:false,headers:{Authorization:`Bearer ${owner.access_token}`,'Content-Type':`multipart/form-data; boundary=${boundary}`}},response=>{const chunks=[];response.on('data',chunk=>chunks.push(chunk));response.on('end',()=>resolve({status:response.statusCode,body:JSON.parse(Buffer.concat(chunks))}));});
    request.on('error',reject);request.write(`--${boundary}\r\nContent-Disposition: form-data; name="base_revision"\r\n\r\n1\r\n--${boundary}\r\nContent-Disposition: form-data; name="files"; filename="Chunked.bin"\r\nContent-Type: application/octet-stream\r\n\r\n`);
    request.write(Buffer.alloc(1024**2+1,9));request.end(`\r\n--${boundary}--\r\n`);
  });assert.equal(chunked.status,413);assert.ok(chunked.body.response.includes('1 MiB'));
  assert.deepEqual(await get(rejectedRecord.uid),rejectedRecord);assert.equal(mx.n1.requests.length,startRequests);
  await upload(rejectedRecord,'Exactly-one-MiB.bin',Buffer.alloc(1024**2,5));
  results.checks.push('Per-file and body limits: JSON 413, exact boundary accepted, chunked bypass rejected, no rejected metadata/N1 writes');

  const diskFailureRow=await create('Temporary disk unavailable'), beforeDiskFailure=mx.n1.requests.length;
  await mx.restart({temporaryDirectory:join(mx.directory,'missing-spool-directory')});
  const diskFailure=await upload(diskFailureRow,'Not-saved.bin',Buffer.from('not saved'),507);
  assert.ok(diskFailure.response.includes('TMPDIR'));assert.deepEqual(await get(diskFailureRow.uid),diskFailureRow);
  assert.equal(mx.n1.requests.length,beforeDiskFailure);
  await mx.restart({temporaryDirectory:process.env.TMPDIR});
  results.checks.push('Unavailable temporary disk returns actionable JSON 507 with no N1 object or record mutation');

  const bytes=Buffer.alloc(66*1024**2,17), expected=createHash('sha256').update(bytes).digest('hex');
  const originalPath=join(mx.directory,'66-MiB-original.bin');await writeFile(originalPath,bytes);
  const apiRecord=await create('API 66 MiB');await setLimits(80);
  const uploaded=await upload(apiRecord,'API-66-MiB.bin',bytes);
  assert.equal(uploaded.attachments[0].size,bytes.length);
  const downloaded=await mx.api.get(`/mx/v1/records/${apiRecord.uid}/attachments/${uploaded.attachments[0].uid}/download`,{headers:{Authorization:`Bearer ${owner.access_token}`}});
  assert.equal(downloaded.status(),200);assert.equal(createHash('sha256').update(await downloaded.body()).digest('hex'),expected);
  const objectCount=mx.n1.objects.size;
  await mx.restart();assert.equal((await call(policyPath)).record_max_size_bytes,80*1024**2);assert.equal(mx.n1.objects.size,objectCount);
  await setLimits(null);assert.equal((await call(policyPath)).record_max_size_bytes,100*1024**2);
  results.checks.push('Real 66 MiB file accepted above old 50 MiB UI ceiling; multipart N1 storage/download hash; override survives restart; Server default restores mx.config value');

  await setLimits(0,0);await mx.restart();
  assert.equal((await call(policyPath)).record_max_size_bytes,0);
  assert.equal((await call('/mx/v1/collaboration/channels')).file_max_size_bytes,0);
  // Above both shipped defaults, including the old 100 MiB chat fallback.
  const uncappedBytes=Buffer.alloc(101*1024**2,23);
  const uncappedRow=await create('No file-size limit above both defaults');
  const uncappedUpload=await upload(uncappedRow,'Uncapped-101-MiB.bin',uncappedBytes);
  assert.equal(uncappedUpload.attachments[0].size,uncappedBytes.length);
  const message=await call(`/mx/v1/collaboration/channels/${channel.uid}/messages`,'POST',{body:'Unlimited API',operation_uid:randomUUID()},[200,201]);
  const shared=await mx.api.post(`/mx/v1/collaboration/messages/${message.uid}/files`,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{file:{name:'Uncapped-101-MiB.bin',mimeType:'application/octet-stream',buffer:uncappedBytes}}});
  assert.equal(shared.status(),201,await shared.text());
  const sharedBody=await shared.json(), sharedFile=sharedBody.file || sharedBody;
  const chatDownload=await mx.api.get(`/mx/v1/collaboration/files/${sharedFile.uid}/download`,{headers:{Authorization:`Bearer ${member.access_token}`}});
  assert.equal(chatDownload.status(),200);assert.equal(createHash('sha256').update(await chatDownload.body()).digest('hex'),createHash('sha256').update(uncappedBytes).digest('hex'));
  await setLimits(1,1);
  const blockedMessage=await call(`/mx/v1/collaboration/channels/${channel.uid}/messages`,'POST',{body:'Restore cap',operation_uid:randomUUID()},[200,201]);
  const blockedChat=await mx.api.post(`/mx/v1/collaboration/messages/${blockedMessage.uid}/files`,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{file:{name:'Rejected.bin',mimeType:'application/octet-stream',buffer:Buffer.alloc(1024**2+1)}}});
  assert.equal(blockedChat.status(),413);
  results.checks.push('Zero means uncapped: 101 MiB record/chat files exceed both startup defaults; persisted across restart; recipient download hash; finite chat cap can be restored');

  for(const engine of(process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')) {
    await setLimits(null);
    const executablePath=engine==='brave'?'/usr/bin/brave-browser':engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await(engine==='brave'?chromium:{chromium,firefox,webkit}[engine]).launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(['chromium','brave'].includes(engine)?{args:['--no-sandbox','--mute-audio']}: {})});browsers.push(browser);
    const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1440,height:1000}});
    await context.addInitScript(({tokens,origin})=>{if(location.origin!==origin)return;sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);localStorage.setItem('mx_admin_tab','deployment');},{tokens:owner,origin:`https://127.0.0.1:${mx.port}`});
    const page=await context.newPage();inspected=page;const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.goto(`https://127.0.0.1:${mx.port}/#admin`);
    const panel=page.getByRole('region',{name:'Upload limits',exact:true});
    await panel.getByRole('combobox',{name:'Record attachment limit',exact:true}).waitFor();
    await panel.getByRole('combobox',{name:'Record attachment limit',exact:true}).selectOption({label:'Custom limit'});
    const policyControl=await panel.getByRole('combobox',{name:'Record attachment limit',exact:true}).elementHandle();
    await page.getByRole('button',{name:'Refresh current workspace',exact:true}).click();
    assert.equal(await policyControl.evaluate(node=>node.isConnected),true,'Global refresh/reconnect must not destroy an unsaved upload policy');
    assert.equal(await panel.getByRole('combobox',{name:'Record attachment limit',exact:true}).inputValue(),'custom','The unsaved custom choice survives refresh');
    const number=panel.getByRole('spinbutton',{name:'Record maximum (MiB)',exact:true});await number.fill('0');
    await panel.getByRole('button',{name:'Save upload limits',exact:true}).click();await panel.getByRole('alert').getByText('Enter a positive whole number',{exact:false}).waitFor();
    await number.fill('80');await panel.getByRole('button',{name:'Save upload limits',exact:true}).click();
    await until(async()=>(await call(policyPath)).record_max_size_bytes===80*1024**2,'80 MiB applies without restart');
    await panel.getByText('Upload limits saved.',{exact:false}).waitFor();
    await page.screenshot({path:join(mx.directory,`${engine}-upload-settings.png`)});
    for(const width of [390,320]){await page.setViewportSize({width,height:900});assert.ok(await panel.evaluate(node=>node.scrollWidth<=node.clientWidth+1),'Upload controls fit on mobile');}
    await page.setViewportSize({width:1440,height:1000});
    for(const name of ['Record attachment limit','Collaboration file limit'])await panel.getByRole('combobox',{name,exact:true}).selectOption({label:'No file-size limit'});
    await panel.getByRole('button',{name:'Save upload limits',exact:true}).click();
    await until(async()=>{const policy=await call(policyPath);return policy.record_max_size_bytes===0&&policy.collaboration_max_size_bytes===0;},'Both no-limit modes saved');
    await panel.getByText('Upload limits saved.',{exact:false}).waitFor();
    await page.screenshot({path:join(mx.directory,`${engine}-unlimited-settings.png`)});
    const row=await create(`Browser ${engine}`);
    await page.goto(`https://127.0.0.1:${mx.port}/#module/${module.uid}?record=${row.uid}`);
    let form=page.getByRole('dialog',{name:'View Upload entry',exact:true});await form.waitFor();await form.getByRole('button',{name:'Edit upload entry',exact:true}).click();
    form=page.getByRole('dialog',{name:'Edit Upload entry',exact:true});await form.getByText('No file-size limit',{exact:false}).waitFor();
    // Lower a live server setting while the record remains open. Picking files
    // refreshes policy rather than using a stale 80/50 MiB browser constant.
    await setLimits(1);
    await form.locator('input[type="file"]').setInputFiles(originalPath);
    await form.locator('.attachment-section').getByRole('alert').getByText('1 MiB record attachment limit',{exact:false}).waitFor();
    assert.equal(await form.getByRole('button',{name:'Remove 66-MiB-original.bin',exact:true}).count(),0);
    await setLimits(0,0);
    await form.locator('input[type="file"]').setInputFiles(originalPath);
    await form.getByRole('button',{name:'Remove 66-MiB-original.bin',exact:true}).waitFor();
    await page.screenshot({path:join(mx.directory,`${engine}-66-MiB-queued.png`)});
    await form.getByRole('button',{name:'Update Upload entry',exact:true}).click();await form.waitFor({state:'hidden',timeout:60000});
    const saved=await get(row.uid);assert.equal(saved.attached_files[0].size,bytes.length);
    assert.equal(mx.n1.objects.get(saved.attached_files[0].object_key)?.bytes.length,bytes.length);
    await page.goto(`https://127.0.0.1:${mx.port}/#collaboration?channel=${channel.uid}`);
    const composer=page.getByRole('textbox',{name:'Message',exact:true});await composer.waitFor();
    await until(async()=>await page.locator('.message-composer label.composer-action').getAttribute('title')==='Attach files up to No file-size limit','Chat respects zero rather than falling back to 100 MiB');
    await page.locator('.message-composer input[type="file"]').setInputFiles(originalPath);
    await page.getByRole('button',{name:'Remove 66-MiB-original.bin',exact:true}).waitFor();
    await composer.fill(`No cap browser ${engine}`);await page.getByRole('button',{name:'Send',exact:true}).click();
    await until(async()=>{const messages=(await call(`/mx/v1/collaboration/channels/${channel.uid}/messages`)).messages;return messages.some(item=>item.body===`No cap browser ${engine}`&&item.files.some(file=>file.size===bytes.length));},'Uncapped chat actual file upload');
    assert.deepEqual(errors,[]);results.browsers[engine]={passed:true,real_upload_bytes:bytes.length,checks:['UI administrator custom/default/unlimited save/no restart','Local validation and mobile fit','Open form refreshes stale policy','66 MiB actual uncapped module and Collaboration uploads to N1']};
    console.log(`${engine}: actual 66 MiB record upload and live limits passed`);await context.close();await browser.close();
  }
  await setLimits(null);
  const configuration=await readFile(join(mx.directory,'mx.config'),'utf8');
  await writeFile(join(mx.directory,'mx.config'),configuration.replace('attachment_max_size_mb=100','attachment_max_size_mb=70'));
  assert.equal((await call(policyPath)).record_max_size_bytes,100*1024**2,'Editing the file is not falsely reported as live reload');
  await mx.restart();assert.equal((await call(policyPath)).record_max_size_bytes,70*1024**2,'Restart loads the edited startup fallback');
  assert.equal((await get(apiRecord.uid)).attached_files[0].size,bytes.length,'Changing policy never removes existing attachments');
  results.checks.push('mx.config edits require restart; restart loads the actual edited value; existing attachments survive lower limits');
  const changedConfig=await readFile(join(mx.directory,'mx.config'),'utf8');await writeFile(join(mx.directory,'mx.config'),changedConfig.replace('attachment_max_size_mb=70','attachment_max_size_mb=0').replace('collaboration_file_max_size_mb=100','collaboration_file_max_size_mb=0'));
  await setLimits(null,null);await mx.restart();assert.equal((await call(policyPath)).record_max_size_bytes,0);assert.equal((await call('/mx/v1/collaboration/channels')).file_max_size_bytes,0);
  results.checks.push('mx.config accepts 0 as an uncapped startup fallback for both upload contexts');
  assert.deepEqual(mx.n1.failures,[]);results.passed=true;
}catch(error){results.error=error.stack;if(inspected&&!inspected.isClosed())await inspected.screenshot({path:join(mx.directory,'upload-limits-failure.png')}).catch(()=>{});throw error;}
finally{await writeFile(join(mx.directory,'upload-limits-results.json'),JSON.stringify(results,null,2));await Promise.all(browsers.map(browser=>browser.close().catch(()=>{})));await mx.close();}
