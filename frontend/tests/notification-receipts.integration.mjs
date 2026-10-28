// Real old-build -> candidate upgrade with account-scoped read-all receipts.
// Both servers, SQLite snapshots and evidence are disposable and HDD-only.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {randomUUID,createHash} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {startIsolatedMx} from './isolated-mx.mjs';
import {onTestDisk,until} from './concurrency-fixture.mjs';

assert.ok(onTestDisk(process.env.MX_TEST_ARTIFACTS));
assert.ok(onTestDisk(process.env.MX_TEST_BINARY));
assert.ok(onTestDisk(process.env.MX_TEST_LEGACY_BINARY));
assert.ok(onTestDisk(process.env.TMPDIR));
assert.ok(!process.env.MX_TEST_N1_URL);
process.umask(0o077);
const root=fileURLToPath(new URL('../',import.meta.url));
const report={passed:false,scope:'Disposable legacy migration and real API receipt/restart checks; not capacity or physical power-loss qualification'};
const password=`Disposable!${randomUUID()}`;
let old,current,oldClosed=false;
function hashTables(path){
 const db=new DatabaseSync(path,{readOnly:true});
 try{return Object.fromEntries(['users','mx_modules','mx_fields','mx_records','mx_notifications','mx_record_values'].map(table=>[table,createHash('sha256').update(JSON.stringify(db.prepare(`SELECT * FROM ${table} ORDER BY ${table==='mx_record_values'?'record_uid,field_uid':'uid'}`).all())).digest('hex')]));}
 finally{db.close();}
}
try{
 old=await startIsolatedMx(root,{binary:process.env.MX_TEST_LEGACY_BINARY});
 await old.call('/mx/v1/auth/create',old.signupKey,'POST',{email:'receipts-owner@test.invalid',name:'Receipt owner',password},[201]);
 const owner=(await old.call('/mx/v1/auth/authenticate',null,'POST',{email:'receipts-owner@test.invalid',password})).body;
 await old.call('/mx/v1/user/create',owner.access_token,'POST',{email:'receipts-peer@test.invalid',name:'Receipt peer',password,access_level:0},[201]);
 const peer=(await old.call('/mx/v1/auth/authenticate',null,'POST',{email:'receipts-peer@test.invalid',password})).body;
 const legacyModule=(await old.call('/mx/v1/admin/modules',owner.access_token,'POST',{name:'Existing upgrade records',singular_name:'Existing record'},[201])).body.module;
 await old.call(`/mx/v1/admin/modules/${legacyModule.uid}/fields`,owner.access_token,'POST',{key:'name',label:'Name',field_type:'text',config:{}},[201]);
 for(let i=0;i<20;i++)await old.call(`/mx/v1/modules/${legacyModule.uid}/records`,peer.access_token,'POST',{operation_uid:randomUUID(),values:{name:`Existing record ${i}`}},[201]);
 await old.close();
 oldClosed=true;
 const path=join(old.directory,'test.db');
 const db=new DatabaseSync(path);
 assert.equal(db.prepare("SELECT count(*) n FROM sqlite_master WHERE name='mx_notification_read_state'").get().n,0,'Must start from an actual pre-watermark schema');
 const now=Date.now();
 db.exec('BEGIN IMMEDIATE');
 // Establish exactly sized inboxes in this owned fixture; retain its real
 // API-created module, field, records, values, revisions and accounts.
 db.exec('DELETE FROM mx_notification_outbox; DELETE FROM mx_notifications;');
 const insert=db.prepare('INSERT INTO mx_notifications(uid,recipient_uid,kind,title,created_at,read_at) VALUES(?,?,?,?,?,?)');
 for(const user of [owner,peer])for(let i=0;i<2500;i++)insert.run(`${user.uid}-${i}`,user.uid,'security.test','Legacy notification',now-5000+i,i<10?now-6000:null);
 db.exec('COMMIT');db.close();
 const before=hashTables(path);
 current=await startIsolatedMx(root,{seedDatabase:path});
 assert.deepEqual(hashTables(join(current.directory,'test.db')),before,'Upgrade preserves original account, schema, record and notification payloads');
 const a=(await current.call('/mx/v1/auth/authenticate',null,'POST',{email:'receipts-owner@test.invalid',password})).body;
 const b=(await current.call('/mx/v1/auth/authenticate',null,'POST',{email:'receipts-peer@test.invalid',password})).body;
 const inbox=async user=>(await current.call('/mx/v1/notifications?limit=100',user.access_token)).body;
 assert.equal((await inbox(a)).unread,2490);
 assert.equal((await inbox(b)).unread,2490);
 const mark=await current.call('/mx/v1/notifications/read-all',a.access_token,'POST');
 assert.equal(mark.body.updated,2490);
 assert.equal((await inbox(a)).unread,0);
 assert.equal((await inbox(b)).unread,2490,'Other account remains unread');
 assert.deepEqual(hashTables(join(current.directory,'test.db')),before,'Read-all does not rewrite legacy notification payloads');
 const all=await inbox(a);
 assert.equal(all.notifications.length,100);
 assert.ok(all.notifications.every(n=>n.read_at!==null));
 assert.equal((await current.call('/mx/v1/notifications?unread_only=true',a.access_token)).body.notifications.length,0);
 await current.call(`/mx/v1/notifications/${all.notifications[0].uid}/read`,b.access_token,'POST',undefined,[404]);
 assert.equal((await current.call('/mx/v1/notifications/read-all',a.access_token,'POST')).body.updated,0);
 const module=(await current.call('/mx/v1/admin/modules',a.access_token,'POST',{name:'Receipt delivery',singular_name:'Receipt'},[201])).body.module;
 await current.call(`/mx/v1/admin/modules/${module.uid}/fields`,a.access_token,'POST',{key:'name',label:'Name',field_type:'text',config:{}},[201]);
 await current.call(`/mx/v1/modules/${module.uid}/records`,b.access_token,'POST',{operation_uid:randomUUID(),values:{name:'New delivery after read-all'}},[201]);
 await until(async()=>(await inbox(a)).unread===1,'new actual delivery remains unread',15000);
 const newNotice=(await inbox(a)).notifications.find(n=>n.kind==='record.created');
 assert.ok(newNotice);assert.equal(newNotice.read_at,null);
 await current.restart({signal:'SIGKILL'});
 const restored=await inbox(a);
 assert.equal(restored.unread,1);
 assert.ok(restored.notifications.filter(n=>n.kind==='security.test').every(n=>n.read_at!==null));
 assert.equal((await inbox(b)).unread,2490);
 assert.equal((await current.call('/mx/v1/notifications/read-all',a.access_token,'POST')).body.updated,1);
 assert.equal((await inbox(a)).unread,0);
 report.passed=true;
 Object.assign(report,{legacyDirectory:old.directory,candidateDirectory:current.directory,legacyRows:5000,legacyRecords:20,legacyDataHashes:before,rawPayloadsPreserved:true,accountIsolation:true,oldReadTimesRetained:true,newDeliveryUnread:true,receiptAndUnreadDeliverySurvivedRestart:true});
}catch(error){report.error=String(error.stack||error).replace(/Bearer\s+[A-Za-z0-9._-]+/g,'Bearer [redacted]');process.exitCode=1;console.error(report.error);}
finally{
 if(current)await current.close();
 if(old&&!oldClosed)await old.close();
 await writeFile(join(process.env.MX_TEST_ARTIFACTS,'notification-receipts-results.json'),JSON.stringify(report,null,2));
 console.log(JSON.stringify(report));
}
