// Real independent systemd worker + MX/SQLite, locally signed RC fixtures.
// No existing deployment, N1 data, production signing key or desktop audio.
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,readFile,writeFile,copyFile,cp,symlink,readlink,stat,unlink,realpath} from 'node:fs/promises';
import {createReadStream,constants} from 'node:fs';
import {join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';
import {execFile,spawn} from 'node:child_process';
import {promisify} from 'node:util';
import {createServer} from 'node:http';
import {createServer as createTcpServer} from 'node:net';
import {DatabaseSync} from 'node:sqlite';
import {request,chromium,firefox,webkit} from 'playwright';
import {startN1Fixture} from './n1-contract-fixture.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';

assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/tests'));
assert.ok(process.env.MX_TEST_BINARY?.startsWith('/mnt/ST1000LM049/'));
assert.ok(process.env.MX_TEST_STATIC_SITE_PATH?.startsWith('/mnt/ST1000LM049/'));
assert.ok(!process.env.MX_TEST_N1_URL,'Never use live N1 for an updater fault test');
const exec=promisify(execFile),repository=fileURLToPath(new URL('../../',import.meta.url));
const bootstrapVersion=process.env.MX_TEST_BOOTSTRAP_VERSION||'1.0.0-rc.1';
const candidateVersion=process.env.MX_TEST_CANDIDATE_VERSION||'1.0.0-rc.2';
for(const version of [bootstrapVersion,candidateVersion])assert.match(version,/^1\.0\.0-rc\.[1-9]\d*$/,'Explicit numbered RC fixture versions are required');
assert.ok(Number(candidateVersion.split('.').at(-1))>Number(bootstrapVersion.split('.').at(-1)),'The candidate must be newer than the preserved bootstrap');
const brokenVersion=candidateVersion.replace(/\d+$/,value=>String(Number(value)+1));
const directory=await mkdtemp(join(process.env.MX_TEST_ARTIFACTS,'mx-updates-'));
const deployment=join(directory,'deployment'),root=join(directory,'updates'),configPath=join(directory,'mx-updates.json');
const id=randomUUID().slice(0,8),unit=`mx-update-test-${id}`,workerUnit=`${unit}-worker`,pathUnit=`${unit}-worker.path`;
const runtime=join(process.env.XDG_RUNTIME_DIR||`/run/user/${process.getuid()}`,'systemd/user');
const registered=[],browsers=[],results=[],resources=new Map();let releases=[],interruptDownload=false,failFeed=false,holdFeed=0,api,feed,n1;
let user,viewer,origin,binary,signup,config,driveFile;
const createdRecords=[],driveBytes=Buffer.from('MX update durability fixture: N1 bytes stay independent of application release slots.\n');
async function until(check,label,timeout=90000){const end=Date.now()+timeout;let error;while(Date.now()<end){try{if(await check())return;}catch(reason){if(reason instanceof assert.AssertionError)throw reason;error=reason;}await new Promise(resolve=>setTimeout(resolve,100));}throw Error(`Timeout: ${label}${error?`: ${error.message}`:''}`);}
function passed(test,details={}){results.push({test,passed:true,...details});console.log(JSON.stringify(results.at(-1)));}
async function ctl(...args){return (await exec('/usr/bin/systemctl',['--user','--no-ask-password',...args],{timeout:120000})).stdout.trim();}
async function freePort(){const server=createTcpServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const port=server.address().port;await new Promise(resolve=>server.close(resolve));return port;}
async function json(path){return JSON.parse(await readFile(path,'utf8'));}
async function state(){return json(join(root,'state.json'));}
async function call(path,method='GET',data,token=user?.access_token,expected=[200]){
  const response=await api.fetch(path,{method,headers:token?{Authorization:`Bearer ${token}`}:{},...(data===undefined?{}:{data})});const text=await response.text();let body;try{body=text?JSON.parse(text):null;}catch{throw Error(`${path}: non-JSON ${response.status()} ${text.slice(0,100)}`);}assert.ok(expected.includes(response.status()),`${method} ${path}: ${response.status()} ${text}`);return {status:response.status(),body};
}
async function queued(action,version){const uid=randomUUID();await call(`/mx/v1/admin/updates/${action}`,'POST',{operation_uid:uid,version:version||null},undefined,[202]);return uid;}
async function done(uid,phase='completed'){let operation;await until(async()=>{const value=await state();operation=value.operation?.uid===uid?value.operation:value.history.find(op=>op.uid===uid);if(operation&&['completed','failed','rolled_back','deferred'].includes(operation.phase)){assert.equal(operation.phase,phase,JSON.stringify(operation));return true;}return false;},`${uid} ${phase}`);await until(async()=>{try{return (await json(join(root,'request.json'))).uid!==uid;}catch(error){if(error.code==='ENOENT')return true;throw error;}},'durable job cleanup');return operation;}
async function idle(){await until(async()=>!(await call('/mx/v1/admin/updates')).body.busy,'worker idle');}
async function safeCopy(source,target){await copyFile(source,target,constants.COPYFILE_FICLONE);}
async function bundle(source,version){const output=join(directory,`mx-${version}.mxupdate`);await exec(binary,['--update-package',source,join(directory,'test-signing.pk8'),output,'rc'],{timeout:120000});return {path:output,metadata:await json(output.replace(/\.mxupdate$/,'.json'))};}
function offer(version,bundle){resources.set(`/${version}.json`,{bytes:Buffer.from(JSON.stringify(bundle.metadata)),type:'application/json'});resources.set(`/${version}.mxupdate`,{path:bundle.path,type:'application/octet-stream'});return {tag_name:`v${version}`,draft:false,prerelease:true,body:`Fixture ${version}: signed update verification.`,assets:[{name:'mx-x86_64-unknown-linux-gnu.json',browser_download_url:`${feed.origin}/${version}.json`},{name:'mx-x86_64-unknown-linux-gnu.mxupdate',browser_download_url:`${feed.origin}/${version}.mxupdate`}]};}
async function savedRecords(){const db=new DatabaseSync(join(deployment,'test.db'),{readOnly:true});try{return Number(db.prepare('SELECT COUNT(*) AS count FROM mx_records').get().count);}finally{db.close();}}
async function addRecord(moduleUid,fieldUid,text){const value=(await call(`/mx/v1/modules/${moduleUid}/records`,'POST',{values:{[fieldUid]:text},operation_uid:randomUUID()},undefined,[200,201])).body;createdRecords.push({uid:value.uid,moduleUid,fieldUid,text});}
async function verifyBusiness(){for(const record of createdRecords){const current=(await call(`/mx/v1/modules/${record.moduleUid}/records/${record.uid}`)).body;assert.equal(current.values[record.fieldUid],record.text);const history=(await call(`/mx/v1/modules/${record.moduleUid}/records/${record.uid}/versions`)).body;assert.ok(history.versions.length>=1);}if(driveFile){const response=await api.get(`/mx/v1/drive/items/${driveFile.uid}/download`,{headers:{Authorization:`Bearer ${user.access_token}`}});assert.equal(response.status(),200,await response.text());assert.deepEqual(await response.body(),driveBytes);}}

try{
 console.log(`Updater artifacts: ${directory}`);
 assert.equal((await exec(process.env.MX_TEST_BINARY,['--version'])).stdout.trim(),`litiaina-mx ${bootstrapVersion}`,'Supply the preserved bootstrap matching MX_TEST_BOOTSTRAP_VERSION, and the newer candidate separately in MX_TEST_CANDIDATE_BINARY.');
 await Promise.all([mkdir(deployment),mkdir(join(root,'slots/bootstrap/frontend/dist'),{recursive:true}),mkdir(join(directory,'worker'))]);
 binary=join(directory,'worker/litiaina-mx');await safeCopy(process.env.MX_TEST_BINARY,binary);await exec('/usr/bin/strip',['--strip-debug',binary]);await safeCopy(binary,join(root,'slots/bootstrap/litiaina-mx'));
 await cp(process.env.MX_TEST_STATIC_SITE_PATH,join(root,'slots/bootstrap/frontend/dist'),{recursive:true,force:true});await copyFile(join(repository,'LICENSE'),join(root,'slots/bootstrap/LICENSE'));
 await symlink('slots/bootstrap',join(root,'current'));
 const key=await exec(binary,['--update-keygen',join(directory,'test-signing.pk8')]);const keys=JSON.parse(key.stdout.match(/trusted_keys: (\{[^\n]+\})/)[1]);
 n1=await startN1Fixture();
 const server=createServer(async(req,res)=>{try{
   if(req.url==='/releases'){if(holdFeed)await new Promise(resolve=>setTimeout(resolve,holdFeed));if(failFeed){res.writeHead(503);res.end('test release connection failure');return;}res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify(releases));return;}
   const resource=resources.get(req.url);if(!resource){res.writeHead(404);res.end();return;}
   if(resource.bytes){res.writeHead(200,{'Content-Type':resource.type});res.end(resource.bytes);return;}
   const size=(await stat(resource.path)).size;res.writeHead(200,{'Content-Type':resource.type,'Content-Length':size});
   if(interruptDownload){const stream=createReadStream(resource.path,{end:1024*1024});stream.pipe(res,{end:false});stream.on('end',()=>res.destroy());}else{createReadStream(resource.path).pipe(res);}
 }catch(error){res.destroy(error);}});await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));feed={server,origin:`http://127.0.0.1:${server.address().port}`};
 const port=await freePort(),redirect=await freePort();origin=`https://127.0.0.1:${port}`;signup=randomUUID();const password=`Test-only!${randomUUID()}`;
 await writeFile(join(deployment,'mx.env'),`JWT_SECRET=${randomUUID()}${randomUUID()}\nAUTH_KEYS=${signup}\nN1_MX_SECRET=unused-isolated-test\n`,{mode:0o600});
 await writeFile(join(deployment,'mx.config'),`[server]\nhost=127.0.0.1\nhttp_port=${redirect}\nhttps_port=${port}\ncert_path=${join(deployment,'cert.pem')}\nkey_path=${join(deployment,'key.pem')}\n[database]\ndb_path=${join(deployment,'test.db')}\npool_max_size=8\npool_min_idle=2\n[jwt_token_config]\nlogin_token_expiration=900\nrefresh_token_expiration=2592000\n[static_site_hosting]\nstatic_site_path=${join(root,'current/frontend/dist')}\n[n1]\nbase_url=${n1.url}\nfragment=isolated-no-external-storage\ninsecure_tls=true\n`);
 await exec('openssl',['req','-x509','-newkey','rsa:2048','-nodes','-keyout',join(deployment,'key.pem'),'-out',join(deployment,'cert.pem'),'-days','1','-subj','/CN=localhost']);
 config={format:1,root,deployment_dir:deployment,service:`${unit}.service`,scope:'user',repository:'Litiaina/aris',channel:'rc',trusted_keys:keys,feed_url:`${feed.origin}/releases`,allow_loopback_http:true,notice_seconds:0,health_timeout_seconds:8,drain_timeout_seconds:30,check_interval_seconds:21600};await writeFile(configPath,JSON.stringify(config,null,2));
 const definitions={
  [`${unit}.service`]:`[Unit]\nDescription=Disposable MX update test ${id}\n[Service]\nType=simple\nWorkingDirectory=${deployment}\nEnvironment=MX_UPDATE_CONFIG=${configPath}\nEnvironment=TMPDIR=${process.env.TMPDIR}\nEnvironment=RUST_LOG=warn\nExecStart=${join(root,'current/litiaina-mx')}\nTimeoutStopSec=30\n`,
  // Compress many intentional fault jobs into seconds without the host's
  // default five-start burst limit masking the application assertions. This
  // fixture-only setting does not change the deployment unit templates.
  [`${workerUnit}.service`]:`[Unit]\nDescription=Disposable independent MX updater ${id}\nStartLimitIntervalSec=0\n[Service]\nType=oneshot\nWorkingDirectory=${deployment}\nEnvironment=TMPDIR=${process.env.TMPDIR}\nExecStart=${binary} --update-worker --config ${configPath}\nTimeoutStartSec=300\n`,
  [pathUnit]:`[Unit]\nDescription=Disposable MX updater path ${id}\n[Path]\nPathExists=${join(root,'request.json')}\nUnit=${workerUnit}.service\n`,
 };
 for(const [name,contents] of Object.entries(definitions)){const path=join(directory,name);await writeFile(path,contents);await ctl('link','--runtime',path);registered.push(name);}
 await ctl('daemon-reload');await ctl('start',`${unit}.service`);await ctl('start',pathUnit);
 api=await request.newContext({baseURL:origin,ignoreHTTPSErrors:true,timeout:15000});await until(async()=> (await api.get('/ping',{timeout:1000})).ok(),'initial server');
 await call('/mx/v1/auth/create','POST',{email:'updater@test.invalid',name:'Update Administrator',password},signup,[201]);user=(await call('/mx/v1/auth/authenticate','POST',{email:'updater@test.invalid',password},null)).body;
 await call('/mx/v1/user/create','POST',{email:'viewer@test.invalid',name:'Update Viewer',password,access_level:3},undefined,[201]);viewer=(await call('/mx/v1/auth/authenticate','POST',{email:'viewer@test.invalid',password},null)).body;
 const module=(await call('/mx/v1/admin/modules','POST',{name:'Update recovery checks',singular_name:'Entry'},undefined,[201])).body;const moduleUid=module.uid||module.module.uid;
 const field=(await call(`/mx/v1/admin/modules/${moduleUid}/fields`,'POST',{key:'description',label:'Description',field_type:'text',required:false,options:[]},undefined,[201])).body;const fieldUid='description';assert.ok(field.uid||field.field?.uid,JSON.stringify(field));
 await Promise.all(Array.from({length:8},(_,i)=>addRecord(moduleUid,'description',`Preserved entry ${i}`)));assert.equal(await savedRecords(),8);
 const driveOperation=randomUUID();await call('/mx/v1/drive/uploads','POST',{operation_uid:driveOperation,file_name:'update-survival.txt',mime_type:'text/plain',size:driveBytes.length,last_modified:1},undefined,[200,201]);
 for(let attempt=0;attempt<3;attempt++){const drivePart=await api.put(`/mx/v1/drive/uploads/${driveOperation}/parts/0`,{headers:{Authorization:`Bearer ${user.access_token}`,'Content-Type':'application/octet-stream'},data:driveBytes});if(drivePart.status()===502&&attempt<2)continue;assert.equal(drivePart.status(),200,await drivePart.text());break;}driveFile=(await call(`/mx/v1/drive/uploads/${driveOperation}/finish`,'POST',undefined,undefined,[200,201])).body;await verifyBusiness();
 await call('/mx/v1/admin/updates','GET',undefined,viewer.access_token,[403]);await call('/mx/v1/admin/updates/check','POST',{operation_uid:randomUUID(),version:null},viewer.access_token,[403]);await call('/mx/v1/admin/updates/policy','PUT',{mode:'automatic',start_hour:2,end_hour:3,utc_offset_minutes:480},viewer.access_token,[403]);
 assert.equal((await call('/mx/v1/admin/updates')).body.managed,true);passed('administrator-only update authority; bootstrap uses managed slots and eight real records');

 // Qualify the actual release binary/frontend when supplied. Otherwise compile
 // a real second version from disposable source, without changing the repository.
 let candidateBinary=process.env.MX_TEST_CANDIDATE_BINARY;
 if(candidateBinary){
   assert.ok(candidateBinary.startsWith('/mnt/ST1000LM049/'));
   assert.ok(process.env.MX_TEST_CANDIDATE_STATIC_SITE_PATH?.startsWith('/mnt/ST1000LM049/'));
   assert.equal((await exec(candidateBinary,['--version'])).stdout.trim(),`litiaina-mx ${candidateVersion}`);
 }else{
 const candidate=join(directory,'candidate-source');await mkdir(candidate);await cp(join(repository,'src'),join(candidate,'src'),{recursive:true});
 const toml=(await readFile(join(repository,'Cargo.toml'),'utf8')).replace(/(^version\s*=\s*")[^"]+/m,(_all,prefix)=>`${prefix}${candidateVersion}`);
 const lock=(await readFile(join(repository,'Cargo.lock'),'utf8')).replace(/(name = "litiaina-mx"\nversion = ")[^"]+/,(_all,prefix)=>`${prefix}${candidateVersion}`);await writeFile(join(candidate,'Cargo.toml'),toml);await writeFile(join(candidate,'Cargo.lock'),lock);
 const buildLog=await import('node:fs/promises').then(fs=>fs.open(join(directory,'candidate-build.log'),'w'));
 const candidateTarget=process.env.CARGO_TARGET_DIR||'/mnt/ST1000LM049/build/target';
 await new Promise((resolve,reject)=>{const child=spawn('/home/altear/.cargo/bin/cargo',['build','--offline','--locked','--manifest-path',join(candidate,'Cargo.toml')],{env:{...process.env,CARGO_TARGET_DIR:candidateTarget,CARGO_BUILD_JOBS:'3'},stdio:['ignore',buildLog.fd,buildLog.fd]});child.on('error',reject);child.on('exit',code=>code===0?resolve():reject(Error(`Candidate compilation failed: ${code}; see ${directory}/candidate-build.log`)));});await buildLog.close();
 candidateBinary=join(candidateTarget,'debug/litiaina-mx');
 }
 const release=join(directory,`release-${candidateVersion}`);await mkdir(release);await safeCopy(candidateBinary,join(release,'litiaina-mx'));await exec('/usr/bin/strip',['--strip-debug',join(release,'litiaina-mx')]);
 if(process.env.MX_TEST_CANDIDATE_STATIC_SITE_PATH){await mkdir(join(release,'frontend'));await cp(process.env.MX_TEST_CANDIDATE_STATIC_SITE_PATH,join(release,'frontend/dist'),{recursive:true});}
 else{await cp(join(root,'slots/bootstrap/frontend'),join(release,'frontend'),{recursive:true});}
 await copyFile(join(repository,'LICENSE'),join(release,'LICENSE'));
 const good=await bundle(release,candidateVersion);releases=[offer(candidateVersion,good)];
 failFeed=true;await done(await queued('check'), 'failed');failFeed=false;assert.equal((await call('/mx/v1/system/update-health', 'GET',undefined,null)).body.version,bootstrapVersion);
 const metadataResource=resources.get(`/${candidateVersion}.json`);const originalMetadata=metadataResource.bytes;metadataResource.bytes=Buffer.from(JSON.stringify({...good.metadata,signature:Buffer.alloc(64).toString('base64')}));await done(await queued('check'),'failed');metadataResource.bytes=originalMetadata;
 await done(await queued('check'));assert.equal((await state()).available.version,candidateVersion);passed('release outage and invalid signatures leave the running binary/database unchanged');
 interruptDownload=true;await done(await queued('stage',candidateVersion),'failed');interruptDownload=false;assert.equal(await readlink(join(root,'current')),'slots/bootstrap');
 await done(await queued('stage',candidateVersion));await idle();assert.equal((await state()).staged.version,candidateVersion);assert.equal(await savedRecords(),8);passed('interrupted package download is recoverable; restaging verifies every signed file without restarting MX');

 const localHour=(new Date().getUTCHours()+8)%24;const outside={mode:'automatic',start_hour:(localHour+3)%24,end_hour:(localHour+4)%24,utc_offset_minutes:480};
 await call('/mx/v1/admin/updates/policy','PUT',outside);await ctl('start',`${workerUnit}.service`);await idle();assert.equal((await api.get('/mx/v1/system/update-health')).status(),200);assert.equal(await readlink(join(root,'current')),'slots/bootstrap');passed('automatic installation remains deferred outside a configurable maintenance window');
 const inside={...outside,start_hour:(localHour+23)%24,end_hour:(localHour+2)%24};await call('/mx/v1/admin/updates/policy','PUT',inside);const beforePid=await ctl('show','--value','--property=MainPID',`${unit}.service`);
 await ctl('start',`${workerUnit}.service`);
 let sawMaintenance=false;await until(async()=>{try{const response=await api.get('/mx/v1/modules',{headers:{Authorization:`Bearer ${user.access_token}`},timeout:1000});if(response.status()===503)sawMaintenance=true;const health=await call('/mx/v1/system/update-health','GET',undefined,null);return health.body.version===candidateVersion&&!health.body.maintenance;}catch{return false;}},'healthy automatic activation');await idle();
 assert.equal((await state()).operation.phase,'completed');assert.notEqual(await ctl('show','--value','--property=MainPID',`${unit}.service`),beforePid);assert.equal(await savedRecords(),8);assert.ok(sawMaintenance,'business routes reject requests during activation');
 const goodJournal=await json(join(root,'activation.json'));assert.equal(goodJournal.phase,'committed');const snapshot=join(root,'backups',goodJournal.snapshot);const backupDb=new DatabaseSync(snapshot,{readOnly:true});assert.equal(backupDb.prepare('PRAGMA integrity_check').get().integrity_check,'ok');backupDb.close();await verifyBusiness();passed('automatic real-version activation restarts a separate systemd service, gates writes, verifies the final backup and preserves record contents/history and N1-backed Drive bytes');
 await addRecord(moduleUid,fieldUid,'Acknowledged after the successful update');assert.equal(await savedRecords(),9);await ctl('start',`${workerUnit}.service`);assert.equal(await savedRecords(),9);passed('a completed activation journal never rewinds later acknowledged user writes');
 await call('/mx/v1/admin/updates/policy','PUT',{...inside,mode:'manual'});

 // The signed bad fixture changes SQLite at startup, then deliberately runs
 // the wrong actual MX version. It must fail readiness and restore the backup.
 const badSource=join(directory,`release-${brokenVersion}`);await mkdir(badSource);await cp(join(release,'frontend'),join(badSource,'frontend'),{recursive:true});await copyFile(join(repository,'LICENSE'),join(badSource,'LICENSE'));
 const migration=`import{DatabaseSync}from'node:sqlite';const db=new DatabaseSync('test.db');db.exec('CREATE TABLE IF NOT EXISTS candidate_migration_probe(value TEXT); INSERT INTO candidate_migration_probe VALUES (1)');db.close();`;
 const wrapper=`#!/bin/sh\nif [ "$1" = "--version" ] || [ "$1" = "-V" ]; then printf 'litiaina-mx ${brokenVersion}\\n'; exit 0; fi\n'${process.execPath}' --no-warnings --input-type=module -e '${migration.replaceAll("'","'\\''")}'\nexec '${join(release,'litiaina-mx')}'\n`;
 await writeFile(join(badSource,'litiaina-mx'),wrapper,{mode:0o755});const bad=await bundle(badSource,brokenVersion);
 const uploadUid=randomUUID();const upload=await api.post(`/mx/v1/admin/updates/package?operation_uid=${uploadUid}`,{headers:{Authorization:`Bearer ${user.access_token}`,'Content-Type':'application/octet-stream'},data:await readFile(bad.path),timeout:120000});assert.equal(upload.status(),202,await upload.text());await done(uploadUid);await idle();assert.equal((await state()).staged.version,brokenVersion);passed('signed offline upload stages independently of an external release feed');
 const concurrent=await Promise.all(Array.from({length:8},async()=>{const uid=randomUUID();const result=await call('/mx/v1/admin/updates/install','POST',{operation_uid:uid,version:brokenVersion},undefined,[202,409,503]);return {uid,status:result.status};}));assert.equal(concurrent.filter(r=>r.status===202).length,1);const winner=concurrent.find(r=>r.status===202);await done(winner.uid,'rolled_back');await idle();
 assert.equal((await call('/mx/v1/system/update-health','GET',undefined,null)).body.version,candidateVersion);assert.equal(await savedRecords(),9);let db=new DatabaseSync(join(deployment,'test.db'),{readOnly:true});assert.equal(db.prepare("SELECT COUNT(*) AS count FROM sqlite_master WHERE name='candidate_migration_probe'").get().count,0);db.close();await verifyBusiness();passed('eight simultaneous install requests have one winner; failed post-migration readiness restores both prior binary/frontend and the latest pre-update SQLite snapshot');
 await addRecord(moduleUid,fieldUid,'Preserved through interrupted rollback');
 await ctl('stop',pathUnit);const crashUid=await queued('install',brokenVersion);await ctl('start','--no-block',`${workerUnit}.service`);
 await until(async()=>{const journal=await json(join(root,'activation.json'));if(journal.uid!==crashUid||journal.phase!=='validating')return false;const probe=new DatabaseSync(join(deployment,'test.db'),{readOnly:true});try{return probe.prepare("SELECT COUNT(*) AS count FROM sqlite_master WHERE name='candidate_migration_probe'").get().count===1;}finally{probe.close();}},'candidate migration before worker crash');
 await ctl('kill','--signal=SIGKILL',`${workerUnit}.service`);await until(async()=>['failed','inactive'].includes(await ctl('show','--value','--property=ActiveState',`${workerUnit}.service`)),'worker exit after SIGKILL');
 await until(async()=>{const response=await api.get('/mx/v1/system/update-health',{timeout:1000});return response.ok()&&(await response.json()).maintenance;},'gated candidate listener after worker interruption');
 await call(`/mx/v1/modules/${moduleUid}/records`,'POST',{values:{[fieldUid]:'MUST NOT COMMIT'},operation_uid:randomUUID()},undefined,[503]);
 await ctl('reset-failed',`${workerUnit}.service`);await ctl('start',`${workerUnit}.service`);await done(crashUid,'rolled_back');await idle();assert.equal(await savedRecords(),10);db=new DatabaseSync(join(deployment,'test.db'),{readOnly:true});assert.equal(db.prepare("SELECT COUNT(*) AS count FROM sqlite_master WHERE name='candidate_migration_probe'").get().count,0);db.close();await verifyBusiness();await ctl('start',pathUnit);passed('SIGKILL during activation retains maintenance; an independent worker recovers exact record contents/history and N1-backed Drive references before reopening writes');

 releases=[offer(brokenVersion,bad)];await call('/mx/v1/admin/updates/policy','PUT',inside);await done(await queued('check'));await idle();assert.equal((await state()).operation.action,'check');assert.equal((await state()).operation.phase,'completed');assert.equal((await call('/mx/v1/system/update-health','GET',undefined,null)).body.version,candidateVersion);await call('/mx/v1/admin/updates/policy','PUT',{...inside,mode:'manual'});passed('a failed candidate is not retried automatically; manual policy remains selectable');

 for(const name of (process.env.MX_TEST_BROWSERS||'chromium,brave,firefox,webkit').split(',')){
   const type=name==='brave'?chromium:({chromium,firefox,webkit})[name];const executablePath=name==='brave'?'/usr/bin/brave-browser':name==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':name==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
   const browser=await type.launch({headless:true,executablePath,env:disconnectedBrowserAudio(directory),...(['chromium','brave'].includes(name)?{args:['--no-sandbox','--mute-audio']}: {})});browsers.push(browser);const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}});const errors=[];page.on('pageerror',error=>errors.push(error.message));
   await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);localStorage.setItem('mx_admin_tab','updates');},user);await page.goto(`${origin}/#admin`);const panel=page.getByRole('region',{name:'MX updates',exact:true});await panel.waitFor();await panel.getByRole('heading',{name:'Installation policy'}).waitFor();
   await panel.getByLabel('Install updates',{exact:true}).selectOption('automatic');
   const draft=await panel.getByLabel('Install updates',{exact:true}).elementHandle();await page.getByRole('button',{name:'Refresh current workspace',exact:true}).click();await panel.getByLabel('Window time zone').waitFor();assert.ok(await draft.evaluate(element=>element.isConnected),'global refresh/reconnect retains the mounted update form');assert.equal(await panel.getByLabel('Install updates',{exact:true}).inputValue(),'automatic','refresh preserves the unsaved operator choice');
   await panel.getByLabel('Window time zone').selectOption('480');await panel.getByLabel('Window starts').selectOption('2');await panel.getByLabel('Window ends').selectOption('2');await panel.getByRole('button',{name:'Save policy',exact:true}).click();await panel.getByRole('alert').filter({hasText:'different maintenance start/end hours'}).waitFor();
   await panel.getByLabel('Install updates',{exact:true}).selectOption('manual');await panel.getByRole('button',{name:'Save policy',exact:true}).click();await panel.getByText('Installation policy saved.',{exact:true}).waitFor();
   let posts=0,ackUid;holdFeed=1000;const checkRoute=async route=>{posts++;ackUid=route.request().postDataJSON().operation_uid;const response=await route.fetch();assert.equal(response.status(),202);await route.abort('failed');};await page.route('**/mx/v1/admin/updates/check',checkRoute);await panel.getByRole('button',{name:'Check for updates',exact:true}).click();await until(()=>!!ackUid,'queued UI request');await done(ackUid);await until(()=>panel.getByRole('button',{name:'Check for updates',exact:true}).isEnabled(),'lost acknowledgement reconciled in UI');assert.equal(posts,1,'the UI never replays an uncertain update request');holdFeed=0;await page.unroute('**/mx/v1/admin/updates/check',checkRoute);
   for(const [theme,width] of [['light',1280],['dark',1024],['dark',390],['light',320]]){await page.setViewportSize({width,height:900});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);await page.waitForTimeout(100);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'no page overflow');await page.screenshot({path:join(directory,`${name}-updates-${theme}-${width}.png`),fullPage:true});}
   assert.deepEqual(errors,[]);passed(`${name}: operator policy, local validation, release/rollback history and responsive light/dark UI`);await browser.close();
 }
 assert.deepEqual(n1.failures,[]);await writeFile(join(directory,'update-results.json'),JSON.stringify({passed:true,directory,bootstrap_version:bootstrapVersion,candidate_version:candidateVersion,results},null,2));
}catch(error){let shot=0;for(const browser of browsers)for(const context of browser.contexts())for(const page of context.pages())if(!page.isClosed())await page.screenshot({path:join(directory,`updates-failed-${shot++}.png`),fullPage:true}).catch(()=>{});await writeFile(join(directory,'update-results.json'),JSON.stringify({passed:false,directory,results,error:error.stack},null,2));throw error;}
finally{
 for(const browser of browsers)await browser.close().catch(()=>{});await api?.dispose();
 for(const name of [pathUnit,`${workerUnit}.service`,`${unit}.service`])await ctl('stop',name).catch(()=>{});
 for(const name of registered){const link=join(runtime,name);try{if(await realpath(link)===join(directory,name))await unlink(link);}catch{}}
 await ctl('daemon-reload').catch(()=>{});for(const name of registered)await ctl('reset-failed',name).catch(()=>{});
 await n1?.close();if(feed)await new Promise(resolve=>feed.server.close(resolve));
}
