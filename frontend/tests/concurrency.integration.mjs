// Capacity qualification: real HTTPS/auth/SQLite/WebSockets and an owned N1.
// Run only on the HDD, against explicitly selected optimized binaries.
// No retry of a failed write is hidden in the measured workload.
import assert from 'node:assert/strict';
import {randomUUID,randomBytes} from 'node:crypto';
import {mkdtemp,writeFile,readFile,copyFile,chmod} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
import {cpus,totalmem,platform,release} from 'node:os';
import {DatabaseSync} from 'node:sqlite';
import {startIsolatedMx} from './isolated-mx.mjs';
import {startN1Lab,client,until,sleep,distribution,resourceSample,safeDisk,digest,onTestDisk,testDiskRoot} from './concurrency-fixture.mjs';
import {workflowTemplate} from '../src/lib/util/workflowTemplates.ts';
import {officeFixture,unzip,zip} from './office-fixtures.mjs';
import {officeChaosWitness} from './office-chaos-witness.mjs';

const require=createRequire(import.meta.url);
// Use Playwright's already-installed WebSocket transport, not a download.
const {ws:WebSocket}=require('../node_modules/playwright-core/lib/utilsBundle.js');
assert.ok(onTestDisk(process.env.MX_TEST_ARTIFACTS));
assert.ok(onTestDisk(process.env.TMPDIR));
assert.ok(onTestDisk(process.env.MX_TEST_BINARY));
assert.ok(!process.env.MX_TEST_N1_URL,'Only the owned N1 is allowed');
const count=Number(process.env.MX_LOAD_USERS||100),rounds=Number(process.env.MX_LOAD_ROUNDS||3);
const seedRows=Number(process.env.MX_LOAD_SEED_ROWS||0),soakSeconds=Number(process.env.MX_LOAD_SOAK_SECONDS||0);
const importRows=Number(process.env.MX_LOAD_IMPORT_ROWS||0);
const mixedSeconds=Number(process.env.MX_LOAD_MIXED_SECONDS||0);
assert.ok(Number.isInteger(count)&&count>=1&&count<=1000);
const directory=await mkdtemp(join(process.env.MX_TEST_ARTIFACTS,'mx-concurrency-'));
console.log(`Concurrency evidence: ${directory}`);
const report={passed:false,scope:'API virtual users with real WebSockets; optional explicitly reported graphical Office witness, not 100/1000 graphical editors; no physical power-cut qualification',testDiskRoot,users:count,rounds,seedRows,soakSeconds,mixedSeconds,importRows,started:new Date().toISOString(),phases:[],resources:[],failures:[]};
report.host={cpu:cpus()[0]?.model,logicalCpus:cpus().length,ramBytes:totalmem(),os:platform(),kernel:release(),node:process.version,loadGenerator:'Same host; loopback HTTPS/WebSockets'};
report.calibration=process.env.MX_LOAD_CALIBRATION==='1';
report.fileProbe=process.env.MX_LOAD_FILE_PROBE==='1';
report.filePayload=process.env.MX_LOAD_RANDOM_FILES==='1'?'crypto-random':'repeated-byte diagnostic payload';
const sockets=[],records=[],importedRecords=[],mixedRecords=[],mixedAttachments=[],mixedVersions=[],mixedFinalEdits=new Map(),readWatermarks=new Map();let n1,mx,http,monitor,database,phase='setup',resourceFailure;
const password=`Disposable!${randomUUID()}`;
const users=[];
async function save(){await writeFile(join(directory,'results.json'),JSON.stringify(report,null,2));}
async function parallel(items,limit,fn){let i=0;return Promise.all(Array.from({length:Math.min(limit,items.length)},async()=>{while(i<items.length){const index=i++;await fn(items[index],index);}}));}
async function run(name,fn){
  if(report.fileProbe&&!['simultaneous-login-and-refresh','authenticated-live-connections','mixed-record-round-1','N1-record-file-round-trip','same-name-upload-collision-preserves-winner','process-crash-recovery','interrupted-writes-and-pending-notification-recovery','server-log-and-history-integrity'].includes(name)){report.phases.push({name,skipped:true});return;}
  phase=name;console.log(`START ${name}`);const begin=performance.now(),offset=http.samples.length;try{const details=await fn();assert.ok(!resourceFailure,resourceFailure);const samples=http.samples.slice(offset);const result={name,passed:true,seconds:(performance.now()-begin)/1000,statuses:samples.reduce((a,s)=>(a[s.status]=(a[s.status]||0)+1,a),{}),latencyMs:distribution(samples.map(s=>s.ms)),...details};report.phases.push(result);console.log(JSON.stringify(result));await save();}catch(e){report.phases.push({name,passed:false,seconds:(performance.now()-begin)/1000,error:e.stack});throw e;}}
const call=async(path,user,method='GET',data,expected=[200],headers)=>http.call(path,user?.access_token,method,data,expected,headers,method==='POST'&&path.includes('/attachments/fields/')?180000:method==='GET'?15000:30000);
async function connectLive(){
  await parallel(users,20,async user=>{
    const ticket=(await call('/mx/v1/live/ticket',user,'POST',undefined,[201])).body.ticket;
    const socket=new WebSocket(`wss://localhost:${mx.port}/mx/v1/live?ticket=${ticket}`,{ca:await readFile(join(mx.directory,'cert.pem')),handshakeTimeout:45000});
    const state={user,socket,events:0,created:new Set(),notifications:new Set(),duplicates:0,sync:0,closed:false,lastPublication:0,outOfOrder:0};sockets.push(state);
    socket.on('message',text=>{const e=JSON.parse(text.toString());state.events++;if(e.type.startsWith('record.')||e.type==='notification.created'){if(e.sequence<=state.lastPublication)state.outOfOrder++;state.lastPublication=e.sequence;}if(e.type==='record.created')state.created.add(e.payload.uid||e.payload.record_uid);if(e.type==='notification.created'){if(state.notifications.has(e.payload.uid))state.duplicates++;state.notifications.add(e.payload.uid);}if(e.type==='sync.required')state.sync++;});
    socket.on('close',()=>state.closed=true);socket.on('error',e=>state.error=e.message);
    await new Promise((resolve,reject)=>{socket.once('open',resolve);socket.once('error',reject);});
  });
}
try{
  n1=await startN1Lab(directory);
  const binary=join(directory,'litiaina-mx');await copyFile(process.env.MX_TEST_BINARY,binary);await chmod(binary,0o500);
  mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)),{storage:n1,loginTokenExpiration:7200,binary});
  report.mxDirectory=mx.directory;report.n1Directory=n1.dir;
  report.n1CapacityProfile=n1.capacityProfile;
  report.deadlines={normalReadMs:15000,normalWriteMs:30000,attachmentUploadMs:180000,productionBrowserUploadMs:1800000};
  report.binaries={mx:{path:binary,sha256:digest(await readFile(binary))},n1:{path:n1.binary,sha256:digest(await readFile(n1.binary))}};
  // This fixture's certificate has no IP SAN; pin it as the test CA and verify
  // hostname via localhost. Do not disable TLS validation on the load client.
  http=client(`https://localhost:${mx.port}`,{ca:await readFile(join(mx.directory,'cert.pem')),timeout:45000});
  monitor=setInterval(async()=>{try{
    await safeDisk(directory);const sample={at:Date.now(),phase,mx:await resourceSample(mx.pid),n1:await resourceSample(n1.pid)};report.resources.push(sample);
    try{if(database)sample.pendingNotificationJobs=database.prepare('SELECT count(*) n FROM mx_notification_outbox').get().n;}catch{/* sampling may coincide with the deliberate crash/recovery */}
    if(sample.mx?.rssKiB>3*1024**2||sample.n1?.rssKiB>3*1024**2)resourceFailure='Owned server exceeded 3GiB RSS safety ceiling';
    const memory=await readFile('/proc/meminfo','utf8');const available=Number(memory.match(/^MemAvailable:\s+(\d+)/m)?.[1]||0);
    if(available<1024**2)resourceFailure='Host memory safety floor (1GiB available)';
  }catch(e){resourceFailure=e.message;}
  if(resourceFailure){
    clearInterval(monitor);http.close();
    // Only the two children created by this fixture, never a service/daemon.
    for(const pid of [mx.pid,n1.pid])try{process.kill(pid,'SIGKILL');}catch{}
  }},1000);
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@concurrency.test',name:'Capacity owner',password},[201]);
  const owner=(await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@concurrency.test',password})).body;
  const module=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Capacity records',singular_name:'Capacity record'},[201])).body.module;
  const base=`/mx/v1/modules/${module.uid}/records`;
  for(const [key,type,config,extra] of [['name','text',{},{}],['amount','decimal',{},{}],['double','formula',{expression:'amount * 2',decimals:2},{}],['secret','text',{access_scope:'creator'},{}],['files','attachments',{multiple:true},{}]])await call(`/mx/v1/admin/modules/${module.uid}/fields`,owner,'POST',{key,label:key,field_type:type,config,...extra},[201]);
  const schema=(await call(`/mx/v1/modules/${module.uid}/schema`,owner)).body;
  const attachmentField=schema.fields.find(f=>f.key==='files');assert.ok(attachmentField);
  console.log(`Provisioning ${count} separate accounts via the real API (not measured user workload)`);
  await parallel(Array.from({length:count},(_,i)=>i),4,async i=>{
    const email=`user${i}@concurrency.test`;
    await call('/mx/v1/user/create',owner,'POST',{email,name:`Capacity user ${i}`,password,access_level:2},[201]);
    const user=(await call('/mx/v1/auth/authenticate',null,'POST',{email,password})).body;
    assert.ok(user.uid&&user.access_token);users[i]=user;
    const access=(await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner)).body;
    await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner,'PUT',{revision:access.revision,grants:[{module_uid:module.uid,can_read:true,can_create:true,can_update:true,can_delete:false,can_configure:false,can_report:true,can_attachments:true}]});
  });
  database=new DatabaseSync(join(mx.directory,'test.db'),{readOnly:true});
  await run('simultaneous-login-and-refresh',async()=>{
    await Promise.all(users.map(async user=>{
      const login=(await call('/mx/v1/auth/authenticate',null,'POST',{email:user.email,password})).body;
      assert.equal(login.uid,user.uid);
      const renewed=(await call('/mx/v1/auth/refresh',null,'POST',{refresh_token:login.refresh_token})).body;
      assert.equal(renewed.uid,user.uid);Object.assign(user,renewed);
    }));
    return {distinctAccounts:count,concurrentPasswordLogins:count,refreshes:count};
  });
  if(seedRows){
    assert.ok(Number.isInteger(seedRows)&&seedRows>0&&seedRows<=1000000);
    console.log(`Preparing ${seedRows} existing records directly as fixture data, NOT counted as API writes`);
    const seed=new DatabaseSync(join(mx.directory,'test.db'));
    try{seed.exec('PRAGMA foreign_keys=ON;PRAGMA busy_timeout=10000;BEGIN IMMEDIATE;');
      const insert=seed.prepare("INSERT INTO mx_records(uid,module_uid,control_year,control_no,date,office,requestor,subject,routed_to_div,remarks) VALUES(?,?,0,?,'','','','','','')");
      const own=seed.prepare("INSERT INTO mx_record_owners VALUES(?,?,'load-fixture')");
      const value=seed.prepare('INSERT INTO mx_record_values(record_uid,field_uid,value_text,value_real) VALUES(?,?,?,?)');
      const name=schema.fields.find(f=>f.key==='name').uid,amount=schema.fields.find(f=>f.key==='amount').uid,double=schema.fields.find(f=>f.key==='double').uid;
      for(let i=0;i<seedRows;i++){const uid=`fixture-${i}`;insert.run(uid,module.uid,i+1);own.run(uid,users[i%users.length].uid);value.run(uid,name,`Existing ${i}`,null);value.run(uid,amount,null,i);value.run(uid,double,null,i*2);}
      seed.prepare('INSERT INTO mx_control_sequence(year,last_value) VALUES(0,?) ON CONFLICT(year) DO UPDATE SET last_value=excluded.last_value').run(seedRows);
      seed.exec('COMMIT');
    }finally{seed.close();}
  }
  await run('authenticated-live-connections',async()=>{
    await connectLive();
    assert.equal(sockets.length,count);return {connections:count};
  });
  for(let round=0;round<rounds;round++)await run(`mixed-record-round-${round+1}`,async()=>{
    // Every user is active, not 100 queued requests sharing one credential.
    const outcomes=await Promise.allSettled(users.map(async(user,i)=>{
      const operation_uid=randomUUID();const values={name:`load-${round}-${i}`,amount:i+.25,secret:`PRIVATE-${round}-${i}`};
      const created=(await call(base,user,'POST',{operation_uid,values},[201])).body;
      records.push({record:created,user,operation_uid,values});
      assert.equal(created.values.double,(i+.25)*2);
      const row=(await call(`${base}/${created.uid}`,user)).body;assert.equal(row.values.secret,values.secret);
      const changed=(await call(`${base}/${created.uid}`,user,'PATCH',{base_revision:row.revision,changes:{amount:i+1.5}})).body;
      assert.equal(changed.values.double,(i+1.5)*2);
      const list=(await call(`${base}?limit=10`,user)).body;assert.ok(list.data.length>0);
      await call('/mx/v1/notifications?limit=10',user);
      const other=(await call(`${base}/${created.uid}`,users[(i+1)%count])).body;
      if(count>1)assert.ok(!('secret' in other.values),'Another account must not receive a restricted field');
    }));
    const failures=outcomes.filter(v=>v.status==='rejected').map(v=>v.reason.stack);
    report.failures.push(...failures);assert.equal(failures.length,0,`${failures.length} unrelated record create/read/patch failures; full details retained in results.json`);
    const expected=records.length*count; // owner + all users, minus each actor
    await until(()=>database.prepare("SELECT count(*) n FROM mx_notifications WHERE module_uid=? AND kind='record.created'").get(module.uid).n===expected,'all durable creation notifications',120000);
    assert.equal(database.prepare('SELECT count(*) n FROM mx_records WHERE module_uid=?').get(module.uid).n,records.length+seedRows);
    assert.equal(database.prepare('SELECT count(DISTINCT control_no) n FROM mx_records WHERE module_uid=?').get(module.uid).n,records.length+seedRows,'No duplicate sequence allocation');
    assert.ok(sockets.every(s=>!s.closed&&!s.error),'Live connections survive the workload');
    assert.ok(sockets.every(s=>s.duplicates===0),'No duplicate live notification IDs');
    assert.ok(sockets.every(s=>s.outOfOrder===0),'Private and public publications keep their sequence order');
    return {writes:count*2,records:records.length,durableCreationNotifications:expected,liveEvents:sockets.reduce((n,s)=>n+s.events,0),resyncs:sockets.reduce((n,s)=>n+s.sync,0)};
  });
  await run('idempotency-and-real-conflicts',async()=>{
    const existing=records.slice(0,count);
    await Promise.all(existing.map(async item=>{const replay=(await call(base,item.user,'POST',{operation_uid:item.operation_uid,values:item.values},[200])).body;assert.equal(replay.uid,item.record.uid);}));
    const target=(await call(`${base}/${records[0].record.uid}`,users[0])).body;
    const conflicts=await Promise.all(users.map((user,i)=>call(`${base}/${target.uid}`,user,'PATCH',{base_revision:target.revision,changes:{name:`conflict-${i}`}},[200,409])));
    assert.equal(conflicts.filter(r=>r.status===200).length,1);
    assert.equal(conflicts.filter(r=>r.status===409).length,count-1);
    assert.ok(conflicts.filter(r=>r.status===409).every(r=>r.body.conflicts?.length===1));
    assert.equal(database.prepare('SELECT count(*) n FROM mx_records WHERE module_uid=?').get(module.uid).n,records.length+seedRows);
    return {replays:existing.length,winners:1,expectedFieldConflicts:count-1};
  });
  await run('collaboration-messages-and-replays',async()=>{
    const channel=(await call('/mx/v1/collaboration/channels',users[0],'POST',{name:'Capacity channel',member_uids:users.slice(1).map(u=>u.uid)},[201])).body.uid;
    const path=`/mx/v1/collaboration/channels/${channel}/messages`;
    const sent=await Promise.all(users.map(async(user,i)=>{const operation_uid=randomUUID();const body=`Capacity message ${i}`;const first=(await call(path,user,'POST',{operation_uid,body},[201])).body.message;const replay=(await call(path,user,'POST',{operation_uid,body},[200])).body.message;assert.equal(first.uid,replay.uid);await call(path,user);return first;}));
    assert.equal(new Set(sent.map(m=>m.uid)).size,count);
    assert.equal(database.prepare('SELECT count(*) n FROM mx_messages WHERE channel_uid=?').get(channel).n,count);
    await until(()=>database.prepare("SELECT count(*) n FROM mx_notifications WHERE target_type='channel' AND target_uid=? AND kind='message.created'").get(channel).n===count*(count-1),'all durable channel notifications',120000);
    return {messages:count,duplicateReplayRows:0,durableMessageNotifications:count*(count-1)};
  });
  await run('notification-receipts-with-active-writers',async()=>{
    const results=await Promise.all(users.map(async(user,i)=>{
      const inbox=(await call('/mx/v1/notifications?limit=10',user)).body.notifications;
      assert.ok(inbox.length>0);
      await call(`/mx/v1/notifications/${inbox[0].uid}/read`,user,'POST',undefined,[204]);
      if(count>1)await call(`/mx/v1/notifications/${inbox[0].uid}/read`,users[(i+1)%count],'POST',undefined,[404]);
      const own=records.find(r=>r.user.uid===user.uid).record;
      const row=(await call(`${base}/${own.uid}`,user)).body;
      await call(`${base}/${own.uid}`,user,'PATCH',{base_revision:row.revision,changes:{amount:900+i}});
      await call('/mx/v1/notifications/read-all',user,'POST');
      const after=(await call('/mx/v1/notifications?limit=10',user)).body;
      const watermark=database.prepare('SELECT through_created_at,read_at FROM mx_notification_read_state WHERE recipient_uid=?').get(user.uid);
      assert.ok(watermark);
      readWatermarks.set(user.uid,watermark);
      for(const notice of after.notifications){
        if(notice.created_at<=watermark.through_created_at)assert.notEqual(notice.read_at,null,'Read-all is reflected by the API, including old payload rows');
      }
      // New deliveries may displace the receipt from the first API page. Check
      // the exact acknowledged row rather than accepting an empty filtered list.
      const receipt=database.prepare('SELECT recipient_uid,read_at FROM mx_notifications WHERE uid=?').get(inbox[0].uid);
      assert.equal(receipt.recipient_uid,user.uid);assert.notEqual(receipt.read_at,null);
      return own.uid;
    }));
    assert.equal(new Set(results).size,count);
    return {accounts:count,concurrentEdits:count,readReceipts:count,readAllReceipts:count,foreignReceiptAttemptsDenied:count>1?count:0};
  });
  if(importRows)await run('import-with-active-readers-and-editors',async()=>{
    assert.ok(Number.isInteger(importRows)&&importRows>=1&&importRows<=100000);
    const before=database.prepare('SELECT count(*) n FROM mx_records WHERE module_uid=?').get(module.uid).n;
    const bytes=Buffer.from('Name,Amount\n'+Array.from({length:importRows},(_,i)=>`Imported ${i},${i}.25`).join('\n'));
    const uploaded=await mx.api.post(`/mx/v1/modules/${module.uid}/imports`,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{job_uid:randomUUID(),file:{name:'capacity.csv',mimeType:'text/csv',buffer:bytes}},timeout:120000});
    assert.equal(uploaded.status(),200,await uploaded.text());let job=await uploaded.json();
    const path=`/mx/v1/modules/${module.uid}/imports/${job.uid}`;
    job=(await call(path+'/configure',owner,'PUT',{sheets:[{index:0,header_row:1,first_data_row:2,columns:[{column:1,key:'name',fill_down:false,date_format:''},{column:2,key:'amount',fill_down:false,date_format:''}]}]})).body;
    let running=true,requests=0;const failures=[];
    const load=Promise.allSettled(users.map(async(user,i)=>{let turn=0;while(running){try{
      if(turn%5===0){const item=records.find(r=>r.user.uid===user.uid);const row=(await call(`${base}/${item.record.uid}`,user)).body;await call(`${base}/${row.uid}`,user,'PATCH',{base_revision:row.revision,changes:{amount:2000+turn+i}});}
      else await call(`${base}?window=start&limit=10&metadata=false`,user);
      turn++;requests++;await sleep(500);
    }catch(e){failures.push(e.stack);break;}}}));
    try{while(job.pending){job=(await call(path+'/process',owner,'POST')).body;assert.equal(job.failed,0,JSON.stringify(job));assert.deepEqual(failures,[]);}}
    finally{running=false;await load;}
    assert.deepEqual(failures,[]);assert.equal(job.imported,importRows);assert.equal(job.state,'completed');
    assert.equal(database.prepare('SELECT count(*) n FROM mx_records WHERE module_uid=?').get(module.uid).n,before+importRows);
    assert.equal(database.prepare("SELECT count(*) n FROM mx_import_rows WHERE job_uid=? AND state='imported'").get(job.uid).n,importRows);
    for(const row of database.prepare("SELECT source_row,record_uid FROM mx_import_rows WHERE job_uid=? AND state='imported' ORDER BY source_row").all(job.uid)){
      const index=row.source_row-2;
      importedRecords.push({uid:row.record_uid,expectedName:`Imported ${index}`,expectedAmount:index+.25});
    }
    return {imported:importRows,failedRows:0,concurrentAccounts:count,backgroundIterations:requests};
  });
  await run('workflow-and-operational-books',async()=>{
    const modBase=`/mx/v1/modules/${module.uid}`;
    await call(`${modBase}/workflow`,owner,'PUT',{revision:0,definition:{...workflowTemplate('crm'),enabled:true},confirm_existing_records:true});
    await Promise.all(users.map(async(user,i)=>{const path=`${base}/${records[i].record.uid}/workflow`;const state=(await call(path,user)).body;await call(`${path}/actions`,user,'POST',{action:'qualify',configuration_revision:state.configuration_revision,record_revision:state.record_revision,workflow_revision:state.workflow_revision,note:'Capacity qualification',operation_uid:randomUUID()});assert.equal((await call(path,user)).body.state,'qualified');}));
    const books=`${modBase}/books`,cfg=(await call(books,owner)).body;
    await call(books,owner,'PUT',{revision:cfg.revision,settings:{...cfg.settings,accounting:true,inventory:true,currency:'PHP',operator_levels:[0,2]}});
    const command=(user,data,expected=[200])=>call(`${books}/commands`,user,'POST',{configuration_revision:1,operation_uid:randomUUID(),...data},expected);
    const cash=(await command(owner,{kind:'account',code:'CASH',name:'Cash',category:'asset'})).body.uid;
    const capital=(await command(owner,{kind:'account',code:'CAPITAL',name:'Capital',category:'equity'})).body.uid;
    const item=(await command(owner,{kind:'item',code:'CAPACITY',name:'Capacity units',unit:'unit'})).body.uid;
    await command(owner,{kind:'stock',item_uid:item,movement:'receive',quantity:String(Math.floor(count/2)),note:'Capacity opening stock',source:null});
    await Promise.all(users.map(user=>command(user,{kind:'journal',date:'2026-10-27',description:'Concurrent exact-money posting',source:null,lines:[{account_uid:cash,debit:'0.30',credit:''},{account_uid:capital,debit:'',credit:'0.30'}]})));
    const reserves=await Promise.all(users.map(user=>command(user,{kind:'stock',item_uid:item,movement:'reserve',quantity:'1',note:'Concurrent hold',source:null},[200,409])));
    assert.equal(reserves.filter(r=>r.status===200).length,Math.floor(count/2));
    assert.equal((await call(`${books}/list/items`,owner)).body.items[0].available,'0.000');
    const balances=(await call(`${books}/list/accounts`,owner)).body.items;
    assert.equal(balances.find(a=>a.code==='CASH').balance,(count*.3).toFixed(2));
    assert.equal(balances.find(a=>a.code==='CAPITAL').balance,(-count*.3).toFixed(2));
    return {workflowActions:count,balancedJournals:count,stockReservations:Math.floor(count/2),expectedInsufficientStockConflicts:count-Math.floor(count/2),overdraws:0};
  });
  await run('creator-isolation-and-current-permissions',async()=>{
    await call(`/mx/v1/admin/modules/${module.uid}`,owner,'PUT',{config:{record_visibility:'creator'}});
    await Promise.all(users.map(async(user,i)=>{
      const listed=(await call(`${base}?limit=10`,user)).body;
      assert.ok(listed.data.every(r=>records.some(own=>own.record.uid===r.uid&&own.user.uid===user.uid)||r.uid.startsWith('fixture-')));
      if(count>1)await call(`${base}/${records.find(r=>r.user.uid!==user.uid).record.uid}`,user,'GET',undefined,[404]);
    }));
    const hidden=(await call(base,owner,'POST',{operation_uid:randomUUID(),values:{name:'Administrator private entry',amount:7}},[201])).body;
    await sleep(1000);
    assert.ok(sockets.every(s=>!s.created.has(hidden.uid)),'Creator-private record must not be broadcast to other accounts');
    await call(`/mx/v1/admin/modules/${module.uid}`,owner,'PUT',{config:{record_visibility:'module'}});
    const user=users[0],access=(await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner)).body;
    await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner,'PUT',{revision:access.revision,grants:[]});
    await call(base,user,'GET',undefined,[403]);
    await call(base,user,'POST',{operation_uid:randomUUID(),values:{name:'Denied entry'}},[403]);
    const next=(await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner)).body;
    await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner,'PUT',{revision:next.revision,grants:[{module_uid:module.uid,can_read:true,can_create:true,can_update:true,can_delete:false,can_configure:false,can_report:true,can_attachments:true}]});
    const blocker=new DatabaseSync(join(mx.directory,'test.db'));blocker.exec('PRAGMA busy_timeout=10000;BEGIN IMMEDIATE');
    const waiting=call(base,user,'POST',{operation_uid:randomUUID(),values:{name:'Revoked while queued'}},[403]);
    // The independent WAL writer leaves auth/permission reads available. Revoke
    // the grant while the actual save is waiting to begin its transaction.
    await sleep(200);blocker.prepare('UPDATE mx_module_account_permissions SET can_create=0 WHERE user_uid=? AND module_uid=?').run(user.uid,module.uid);blocker.exec('COMMIT');blocker.close();
    await waiting;
    const restored=(await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner)).body;
    await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner,'PUT',{revision:restored.revision,grants:[{module_uid:module.uid,can_read:true,can_create:true,can_update:true,can_delete:false,can_configure:false,can_report:true,can_attachments:true}]});
    return {isolatedAccounts:count,rawLivePrivateRecordLeaks:0,revokedReadsAndWritesDenied:true,queuedSaveRechecksGrant:true};
  });
  if(soakSeconds)await run('sustained-read-write-soak',async()=>{
    const end=Date.now()+soakSeconds*1000;let iterations=0,writes=0;
    await Promise.all(users.map(async(user,i)=>{let turn=0;while(Date.now()<end){
      if(turn%10===0){const item=records.find(r=>r.user.uid===user.uid);const row=(await call(`${base}/${item.record.uid}`,user)).body;await call(`${base}/${row.uid}`,user,'PATCH',{base_revision:row.revision,changes:{amount:1000+turn+i}});writes++;}
      else await call(turn%3===0?'/mx/v1/notifications?limit=10':turn%3===1?`${base}?window=start&limit=20&metadata=false`:`/mx/v1/modules/${module.uid}/work-queue?counts_only=true`,user);
      turn++;iterations++;await sleep(500+(i%11)*13);
    }}));
    assert.ok(sockets.every(s=>!s.closed&&!s.error),'All live clients survive the soak');
    return {iterations,writes,seconds:soakSeconds};
  });
  if(mixedSeconds)await run('mixed-chaos-records-uploads-and-office',async()=>{
    assert.ok(Number.isInteger(mixedSeconds)&&mixedSeconds>=30&&mixedSeconds<=3600);
    const iterations=Array(count).fill(0);
    const totals={recordCreates:0,recordEdits:0,attachmentUploads:0,driveVersionCommits:0,reads:0,uploadedBytes:0,downloadedBytes:0};
    let failure;const mixedAbort=new AbortController();
    const attach=async(user,row,data,name)=>{
      const boundary=`mixed-${randomUUID()}`;
      const payload=Buffer.concat([Buffer.from(`--${boundary}\r\nContent-Disposition: form-data; name="base_revision"\r\n\r\n${row.revision}\r\n--${boundary}\r\nContent-Disposition: form-data; name="files"; filename="${name}"\r\nContent-Type: application/octet-stream\r\n\r\n`),data,Buffer.from(`\r\n--${boundary}--\r\n`)]);
      const uploaded=(await call(`/mx/v1/records/${row.uid}/attachments/fields/${attachmentField.uid}`,user,'POST',payload,[201],{'Content-Type':`multipart/form-data; boundary=${boundary}`})).body;
      const attachment=uploaded.attachments[0];assert.ok(attachment?.uid);
      const ticket=(await call(`/mx/v1/records/${row.uid}/attachments/${attachment.uid}/preview-ticket`,user,'POST',undefined,[201])).body;
      mixedAttachments.push({recordUid:row.uid,uid:attachment.uid,user,dataHash:digest(data),length:data.length});
      assert.deepEqual((await call(ticket.url,user)).bytes,data);totals.attachmentUploads++;totals.uploadedBytes+=data.length;totals.downloadedBytes+=data.length;
    };
    // Begin the measured load with an already usable editor, as requested for
    // "someone is editing while 100 users work". Cold-load timing has its own
    // separate gate. A setup error still fails this run; it is never retried.
    const ready=Promise.withResolvers();
    const witness=process.env.MX_LOAD_OFFICE_BROWSER ? officeChaosWitness({mx,owner,call,directory,seconds:mixedSeconds,signal:mixedAbort.signal,onReady:ready.resolve}) : Promise.resolve({enabled:false});
    witness.catch(error=>{failure=error;mixedAbort.abort(error);ready.reject(error);});
    if(process.env.MX_LOAD_OFFICE_BROWSER)await ready.promise;
    const started=Date.now(),end=started+mixedSeconds*1000;
    // Stop all workers after a failed operation; never retry a failed measured write.
    const load=users.map(async(user,i)=>{
      let turn=0,driveItem;
      while(Date.now()<end&&!failure){
        try{
          const own=records.find(r=>r.user.uid===user.uid).record;
          if(i%5===0){const values={name:`Mixed ${i}-${turn}`,amount:turn+i};const created=(await call(base,user,'POST',{operation_uid:randomUUID(),values},[201])).body;mixedRecords.push({uid:created.uid,user,values});totals.recordCreates++;}
          else if(i%5===1){const row=(await call(`${base}/${own.uid}`,user)).body;const amount=turn+i+5000;await call(`${base}/${own.uid}`,user,'PATCH',{base_revision:row.revision,changes:{amount}});mixedFinalEdits.set(own.uid,{user,amount});totals.recordEdits++;}
          else if(i%5===2){const row=(await call(`${base}/${own.uid}`,user)).body;await attach(user,row,randomBytes(turn%4===0?1024*1024:256*1024),`mixed-${turn}.bin`);}
          else if(i%5===3){
            const entries=unzip(officeFixture('.docx'));entries['word/document.xml']=entries['word/document.xml'].replace('MX original document',`Mixed Office version ${i}-${turn}`);const bytes=zip(entries);
            const operation_uid=randomUUID(),path='/mx/v1/drive/uploads';
            await call(path,user,'POST',{operation_uid,...(driveItem?{item_uid:driveItem.uid,base_revision:driveItem.revision}:{}),file_name:`Office-${i}.docx`,mime_type:'application/vnd.openxmlformats-officedocument.wordprocessingml.document',size:bytes.length,last_modified:turn});
            await call(`${path}/${operation_uid}/parts/0`,user,'PUT',bytes);
            driveItem=(await call(`${path}/${operation_uid}/finish`,user,'POST')).body;
            const version=database.prepare('SELECT current_version_uid uid FROM mx_drive_items WHERE uid=?').get(driveItem.uid);assert.ok(version?.uid);
            mixedVersions.push({itemUid:driveItem.uid,versionUid:version.uid,user,dataHash:digest(bytes),length:bytes.length});
            assert.deepEqual((await call(`/mx/v1/drive/items/${driveItem.uid}/download`,user)).bytes,bytes);totals.driveVersionCommits++;totals.uploadedBytes+=bytes.length;totals.downloadedBytes+=bytes.length;
          }else{await call(turn%2?'/mx/v1/notifications?limit=20':`${base}?window=start&limit=20&metadata=false`,user);totals.reads++;}
          iterations[i]++;turn++;await sleep(250+(i%7)*31);
        }catch(error){failure=error;mixedAbort.abort(error);throw error;}
      }
    });
    const [workers,editor]=await Promise.all([Promise.allSettled(load).then(results=>({results,finishedAtMs:Date.now()})),Promise.allSettled([witness])]);
    const outcomes=[...workers.results,...editor];
    report.mixedSnapshot={iterations,totals,office:outcomes.at(-1).status==='fulfilled'?outcomes.at(-1).value:{...outcomes.at(-1).reason.officeEvidence,error:outcomes.at(-1).reason.message},startedAtMs:started,workersFinishedAtMs:workers.finishedAtMs,elapsedSeconds:(Date.now()-started)/1000};
    const rejected=outcomes.filter(r=>r.status==='rejected');assert.deepEqual(rejected.map(r=>r.reason.stack),[]);
    assert.ok(iterations.every(n=>n>0),'Every account must perform real work');
    assert.ok(sockets.every(s=>!s.closed&&!s.error),'All live connections survive the mixed workload');
    const office=outcomes.at(-1).value;
    const overlappingOfficeSaves=office.acknowledgedVersions?.filter(v=>v.verifiedAtMs>=started&&v.verifiedAtMs<=workers.finishedAtMs).length||0;
    if(process.env.MX_LOAD_OFFICE_BROWSER)assert.ok(overlappingOfficeSaves>=3,'At least three real, byte-verified Office saves must overlap the active mixed workers');
    return {activeAccounts:count,seconds:(Date.now()-started)/1000,iterations,totals,office,overlappingOfficeSaves,failedMeasuredOperations:0};
  });
  await run('N1-record-file-round-trip',async()=>{
    await Promise.all(users.map(async(user,i)=>{
      const item=records.find(r=>r.user.uid===user.uid);const row=(await call(`${base}/${item.record.uid}`,user)).body;
      const data=report.filePayload==='crypto-random'?randomBytes(256*1024):Buffer.alloc(256*1024,i%251),boundary=`mx-${randomUUID()}`;
      data.writeUInt32LE(i,0); // Distinguish every payload in the 1,000-user run too.
      const bytes=Buffer.concat([Buffer.from(`--${boundary}\r\nContent-Disposition: form-data; name="base_revision"\r\n\r\n${row.revision}\r\n--${boundary}\r\nContent-Disposition: form-data; name="files"; filename="${i}.bin"\r\nContent-Type: application/octet-stream\r\n\r\n`),data,Buffer.from(`\r\n--${boundary}--\r\n`)]);
      const uploaded=(await call(`/mx/v1/records/${row.uid}/attachments/fields/${attachmentField.uid}`,user,'POST',bytes,[201],{'Content-Type':`multipart/form-data; boundary=${boundary}`})).body;
      const attachment=uploaded.attachments[0];assert.ok(attachment?.uid,JSON.stringify(uploaded));
      const ticket=(await call(`/mx/v1/records/${row.uid}/attachments/${attachment.uid}/preview-ticket`,user,'POST',undefined,[201])).body;
      const downloaded=await call(ticket.url,user);assert.deepEqual(downloaded.bytes,data);
      const partial=await call(ticket.url,user,'GET',undefined,[206],{Range:'bytes=1024-2047'});assert.deepEqual(partial.bytes,data.subarray(1024,2048));
      item.file={uid:attachment.uid,dataHash:digest(data),length:data.length};
    }));
    return {files:count,bytes:count*256*1024,byteExact:true,rangeExact:true,storage:'real N1, strict durability, isolated 1+0 pool on authorized test volume'};
  });
  if(count>1)await run('same-name-upload-collision-preserves-winner',async()=>{
    const target=records[0].record.uid,row=(await call(`${base}/${target}`,users[0])).body;
    const outcomes=await Promise.all([0,1].map(async i=>{
      const data=Buffer.alloc(256*1024,i+101),boundary=`collision-${randomUUID()}`;
      const bytes=Buffer.concat([Buffer.from(`--${boundary}\r\nContent-Disposition: form-data; name="base_revision"\r\n\r\n${row.revision}\r\n--${boundary}\r\nContent-Disposition: form-data; name="files"; filename="collision.bin"\r\nContent-Type: application/octet-stream\r\n\r\n`),data,Buffer.from(`\r\n--${boundary}--\r\n`)]);
      return {i,data,result:await call(`/mx/v1/records/${target}/attachments/fields/${attachmentField.uid}`,users[i],'POST',bytes,[201,400,409],{'Content-Type':`multipart/form-data; boundary=${boundary}`})};
    }));
    const wins=outcomes.filter(o=>o.result.status===201);assert.equal(wins.length,1,'Only one attachment with this name may commit');
    const attachment=(await call(`${base}/${target}`,users[0])).body.attached_files.find(f=>f.file_name==='collision.bin');assert.ok(attachment);
    const ticket=(await call(`/mx/v1/records/${target}/attachments/${attachment.uid}/preview-ticket`,users[0],'POST',undefined,[201])).body;
    assert.deepEqual((await call(ticket.url,users[0])).bytes,wins[0].data,'The losing upload must not overwrite or trash the winning file bytes');
    return {concurrentAttempts:2,committed:1,winningBytesPreserved:true};
  });
  await run('process-crash-recovery',async()=>{
    // Snapshot every acknowledged initial record, not only the first record per
    // account selected for a file upload. Preserve exact field values/revision.
    await parallel(records,20,async item=>{
      const row=(await call(`${base}/${item.record.uid}`,item.user)).body;
      item.beforeCrash={values:row.values,revision:row.revision};
    });
    await parallel(importedRecords,20,async item=>{
      const row=(await call(`${base}/${item.uid}`,owner)).body;
      assert.equal(row.values.name,item.expectedName);assert.equal(row.values.amount,item.expectedAmount);
      assert.equal(row.values.double,item.expectedAmount*2);
      item.beforeCrash={values:row.values,revision:row.revision};
    });
    for(const s of sockets)s.socket.terminate();
    await mx.restart({signal:'SIGKILL'});await n1.restart('SIGKILL');
    sockets.length=0;await connectLive();
    for(const [uid,expected] of readWatermarks){
      assert.deepEqual(database.prepare('SELECT through_created_at,read_at FROM mx_notification_read_state WHERE recipient_uid=?').get(uid),expected,'Acknowledged account read state survives restart');
    }
    await parallel(records,20,async item=>{
      const row=(await call(`${base}/${item.record.uid}`,item.user)).body;
      assert.deepEqual({values:row.values,revision:row.revision},item.beforeCrash,'Every acknowledged initial record retains its exact fields and revision');
    });
    await parallel(importedRecords,20,async item=>{
      const row=(await call(`${base}/${item.uid}`,owner)).body;
      assert.deepEqual({values:row.values,revision:row.revision},item.beforeCrash,'Every acknowledged imported record retains its exact fields and revision');
    });
    await Promise.all(users.map(async user=>{
      const item=records.find(r=>r.user.uid===user.uid);
      const row=(await call(`${base}/${item.record.uid}`,user)).body;assert.ok(row.revision>=2);
      const attachment=row.attached_files.find(f=>f.uid===item.file.uid);assert.ok(attachment);
      const ticket=(await call(`/mx/v1/records/${row.uid}/attachments/${attachment.uid}/preview-ticket`,user,'POST',undefined,[201])).body;
      assert.equal(digest((await call(ticket.url,user)).bytes),item.file.dataHash);
    }));
    // Inspect every acknowledged mixed create/upload, including historical Drive
    // versions beyond the history UI's 100-item display limit. These are reads,
    // never retries/replays of a measured failed write.
    await parallel(mixedRecords,20,async item=>{
      const row=(await call(`${base}/${item.uid}`,item.user)).body;
      assert.equal(row.values.name,item.values.name);assert.equal(row.values.amount,item.values.amount);
      assert.equal(row.values.double,item.values.amount*2);
    });
    await parallel([...mixedFinalEdits],20,async([uid,item])=>{
      const row=(await call(`${base}/${uid}`,item.user)).body;
      assert.equal(row.values.amount,item.amount);assert.equal(row.values.double,item.amount*2);
    });
    await parallel(mixedAttachments,20,async item=>{
      const ticket=(await call(`/mx/v1/records/${item.recordUid}/attachments/${item.uid}/preview-ticket`,item.user,'POST',undefined,[201])).body;
      const bytes=(await call(ticket.url,item.user)).bytes;
      assert.equal(bytes.length,item.length);assert.equal(digest(bytes),item.dataHash);
    });
    await parallel(mixedVersions,20,async item=>{
      const bytes=(await call(`/mx/v1/drive/items/${item.itemUid}/download?version_uid=${item.versionUid}`,item.user)).bytes;
      assert.equal(bytes.length,item.length);assert.equal(digest(bytes),item.dataHash);
    });
    const office=report.mixedSnapshot?.office;
    if(office?.enabled)await parallel(office.acknowledgedVersions,10,async item=>{
      const bytes=(await call(`/mx/v1/drive/items/${office.itemUid}/download?version_uid=${item.versionUid}`,owner)).bytes;
      assert.equal(bytes.length,item.length);assert.equal(digest(bytes),item.dataHash);
    });
    assert.equal(database.prepare('PRAGMA integrity_check').get().integrity_check,'ok');
    assert.deepEqual(database.prepare('PRAGMA foreign_key_check').all(),[]);
    return {acknowledgedRecordsSurvived:true,initialRecordSnapshotsVerified:records.length,importedRecordSnapshotsVerified:importedRecords.length,acknowledgedFileBytesSurvived:true,accountReadWatermarksVerified:readWatermarks.size,mixedRecordCreatesVerified:mixedRecords.length,mixedFinalEditsVerified:mixedFinalEdits.size,mixedAttachmentHashesVerified:mixedAttachments.length,mixedHistoricalVersionHashesVerified:mixedVersions.length,actualOfficeSavedVersionHashesVerified:office?.acknowledgedVersions?.length||0,sqliteIntegrity:'ok',foreignKeyViolations:0,liveClientsReconnected:count};
  });
  await run('interrupted-writes-and-pending-notification-recovery',async()=>{
    // Hold delivery with an explicit disposable fault, not a long writer lock.
    // The worker must retain its jobs; normal record commits must still succeed.
    const fault=new DatabaseSync(join(mx.directory,'test.db'));
    fault.exec("PRAGMA busy_timeout=10000;CREATE TRIGGER injected_pending_delivery BEFORE INSERT ON mx_notifications BEGIN SELECT RAISE(ABORT,'injected-pending-delivery'); END;");
    const before=database.prepare('SELECT count(*) n FROM mx_records WHERE module_uid=?').get(module.uid).n;
    const attempts=users.map((user,i)=>({user,operation_uid:randomUUID(),values:{name:`crash-${i}`,amount:17}}));
    const acknowledged=[];
    const pending=Promise.allSettled(attempts.map(async a=>{const result=await call(base,a.user,'POST',{operation_uid:a.operation_uid,values:a.values},[201]);acknowledged.push(result.body.uid);}));
    await sleep(75);
    await mx.restart({signal:'SIGKILL'});
    const outcomes=await pending;
    for(const s of sockets)s.socket.terminate();sockets.length=0;await connectLive();
    fault.exec('DROP TRIGGER injected_pending_delivery');fault.close();
    const reconciled=await Promise.all(attempts.map(async a=>(await call(base,a.user,'POST',{operation_uid:a.operation_uid,values:a.values},[200,201])).body));
    assert.equal(new Set(reconciled.map(r=>r.uid)).size,count,'Every original operation reconciles to exactly one row');
    assert.ok(acknowledged.every(uid=>reconciled.some(r=>r.uid===uid)),'Every acknowledged pre-crash write survives');
    assert.equal(database.prepare('SELECT count(*) n FROM mx_records WHERE module_uid=?').get(module.uid).n,before+count);
    const ids=reconciled.map(r=>r.uid),placeholders=ids.map(()=>'?').join(',');
    await until(()=>database.prepare(`SELECT count(*) n FROM mx_notifications WHERE kind='record.created' AND target_uid IN (${placeholders})`).get(...ids).n===count*count,'pending notifications recover after crash',120000);
    await Promise.all(users.map(async user=>{const inbox=(await call('/mx/v1/notifications?limit=100',user)).body.notifications;assert.ok(inbox.some(n=>ids.includes(n.target_uid)),'Recovered notifications are inspectable through the account API');}));
    return {attempts:count,acknowledgedBeforeCrash:acknowledged.length,interruptedTransports:outcomes.filter(r=>r.status==='rejected').length,reconciledExactlyOnce:count,durableNotificationsRecovered:count*count,expectedInjectedDeliveryFault:true};
  });
  await run('server-log-and-history-integrity',async()=>{
    const log=await readFile(join(mx.directory,'server.log'),'utf8');
    const storageLog=await readFile(join(n1.dir,'engine.log'),'utf8');
    assert.ok(!/\bERROR\b|panicked|IO error/i.test(storageLog),'No hidden storage-engine failures');
    const unexpected=log.split('\n').filter(line=>!line.includes('injected-pending-delivery')).join('\n');
    assert.ok(!/database is locked|database table is locked|timed out|ERROR|panicked/i.test(unexpected),'No hidden audit, notification, pool or SQLite failures in server log (explicit delivery-fault lines excluded)');
    assert.ok(sockets.every(s=>s.outOfOrder===0),'Reconnected live streams preserve publication order');
    for(const item of records){const history=database.prepare('SELECT count(*) n FROM mx_record_versions WHERE record_uid=?').get(item.record.uid).n;assert.ok(history>=(item.file?3:2),'Creation, edit and attachment history preserved');}
    assert.equal(database.prepare('PRAGMA integrity_check').get().integrity_check,'ok');
    const auditCreates=database.prepare("SELECT count(*) n FROM mx_audit_log WHERE path=? AND method='POST' AND action='record.create' AND status_code=201 AND success=1").get(base).n;
    assert.ok(auditCreates>=records.length,'Every acknowledged normal create retains successful audit evidence');
    assert.equal(database.prepare('SELECT count(*) n FROM mx_audit_log WHERE status_code>=500').get().n,0,'No hidden failed mutation audit entries');
    return {hiddenServerErrors:0,versionedApiRecords:records.length,successfulCreationAuditEntries:auditCreates};
  });
  report.passed=true;
}catch(e){
  report.failures.push(e.stack);console.error(e);process.exitCode=1;
  // Overload/fault runs must still distinguish a rejected operation from lost
  // acknowledged data. This is evidence, never a substitute for a passing run.
  if(database)try{
    const exists=database.prepare('SELECT EXISTS(SELECT 1 FROM mx_records WHERE uid=?) ok');
    report.failureSnapshot={acknowledgedRecordCreates:records.length+importedRecords.length+mixedRecords.length,missingAcknowledgedRecords:records.filter(r=>!exists.get(r.record.uid).ok).length+importedRecords.filter(r=>!exists.get(r.uid).ok).length+mixedRecords.filter(r=>!exists.get(r.uid).ok).length,acknowledgedMixedAttachments:mixedAttachments.length,acknowledgedMixedDriveVersions:mixedVersions.length,fileHashesRecheckedAfterRestart:report.phases.some(p=>p.name==='process-crash-recovery'&&p.passed),sqliteIntegrity:database.prepare('PRAGMA integrity_check').get().integrity_check,foreignKeyViolations:database.prepare('PRAGMA foreign_key_check').all().length,pendingNotificationJobs:database.prepare('SELECT count(*) n FROM mx_notification_outbox').get().n};
  }catch(error){report.failureSnapshot={inspectionError:error.message};}
}
finally{
  clearInterval(monitor);for(const s of sockets)s.socket.terminate();
  if(http){report.http={peakInFlight:http.peak,requests:http.samples.length,latencyMs:distribution(http.samples.map(s=>s.ms))};await writeFile(join(directory,'http-samples.json'),JSON.stringify(http.samples));http.close();}
  database?.close();if(mx)await mx.close();else if(n1)await n1.close();
  // Keep an independent, credential-free acknowledgement ledger even for failed
  // runs. A later read-only/restart audit must not depend on vanished JS memory.
  const withoutCredentials=({user,...item})=>({...item,actorUid:user.uid});
  await writeFile(join(directory,'acknowledged-data.json'),JSON.stringify({
    records:records.map(withoutCredentials),importedRecords,mixedRecords:mixedRecords.map(withoutCredentials),
    mixedFinalEdits:[...mixedFinalEdits].map(([uid,item])=>({uid,...withoutCredentials(item)})),
    mixedAttachments:mixedAttachments.map(withoutCredentials),mixedVersions:mixedVersions.map(withoutCredentials),
    graphicalOffice:report.mixedSnapshot?.office||null,
  },null,2),{mode:0o600});
  report.finished=new Date().toISOString();await save();console.log(`Results: ${directory}/results.json; passed=${report.passed}`);
}
