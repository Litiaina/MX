// Administration remains usable with an independent writer and active outbox.
// Owned fixture only; no changes to a deployment or real accounts.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {startIsolatedMx} from './isolated-mx.mjs';
import {onTestDisk,sleep,until} from './concurrency-fixture.mjs';
for(const key of ['MX_TEST_ARTIFACTS','MX_TEST_BINARY','TMPDIR'])assert.ok(onTestDisk(process.env[key]),key);
assert.ok(!process.env.MX_TEST_N1_URL);
process.umask(0o077);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const report={passed:false,scope:'Administration read-only grants, concurrent mutations, revision checks and restart regression; not capacity qualification'};
let db,locked=false,pending=[];
try{
 const password=`Disposable!${randomUUID()}`;
 await mx.call('/mx/v1/auth/create',mx.signupKey,'POST',{email:'admin-writes@test.invalid',name:'Administration fixture',password},[201]);
 const owner=(await mx.call('/mx/v1/auth/authenticate',null,'POST',{email:'admin-writes@test.invalid',password})).body;
 const people=[];
 for(let i=0;i<3;i++){
  const email=`admin-peer-${i}@test.invalid`;
  await mx.call('/mx/v1/user/create',owner.access_token,'POST',{email,name:`Administration peer ${i}`,password,access_level:2},[201]);
  people.push((await mx.call('/mx/v1/auth/authenticate',null,'POST',{email,password})).body);
 }
 const module=(await mx.call('/mx/v1/admin/modules',owner.access_token,'POST',{name:'Concurrent administration',singular_name:'Record'},[201])).body.module;
 const modulePath=`/mx/v1/admin/modules/${module.uid}`,recordPath=`/mx/v1/modules/${module.uid}/records`;
 await mx.call(modulePath+'/fields',owner.access_token,'POST',{key:'name',label:'Name',field_type:'text',config:{}},[201]);
 db=new DatabaseSync(join(mx.directory,'test.db'));db.exec('PRAGMA busy_timeout=10000');
 const peer=people[0],grantPath=`/mx/v1/admin/accounts/${peer.uid}/modules`;
 assert.equal(db.prepare('SELECT count(*) n FROM mx_module_access_revisions WHERE user_uid=?').get(peer.uid).n,0);
 db.exec('BEGIN IMMEDIATE');locked=true;
 let finished=false;
 const read=mx.call(grantPath,owner.access_token).then(result=>{finished=true;return result;});pending.push(read);
 await sleep(1200);
 const readWasUnblocked=finished;
 db.exec('ROLLBACK');locked=false;
 const initial=(await read).body;
 assert.ok(readWasUnblocked,'Viewing grants must not take SQLite writer admission');
 assert.equal(initial.revision,0);
 assert.equal(db.prepare('SELECT count(*) n FROM mx_module_access_revisions WHERE user_uid=?').get(peer.uid).n,0,'GET must not create a revision row');
 const grant={module_uid:module.uid,can_read:true,can_create:true,can_update:true,can_delete:false,can_configure:false,can_report:true,can_attachments:true};
 for(const person of people)await mx.call(`/mx/v1/admin/accounts/${person.uid}/modules`,owner.access_token,'PUT',{revision:0,grants:[grant]});
 await mx.call(grantPath,owner.access_token,'PUT',{revision:0,grants:[]},[409]);
 assert.equal((await mx.call(grantPath,owner.access_token)).body.revision,1);
 assert.equal(db.prepare('SELECT count(*) n FROM mx_module_account_permissions WHERE user_uid=? AND module_uid=?').get(peer.uid,module.uid).n,1);
 // A permissions failure after metadata UPDATE must roll back the module too.
 db.exec(`CREATE TRIGGER mx_test_permission_failure BEFORE UPDATE ON mx_module_permissions WHEN NEW.module_uid='${module.uid}' BEGIN SELECT RAISE(ABORT,'test permission failure'); END`);
 await mx.call(modulePath,owner.access_token,'PUT',{name:'Must roll back',permissions:module.permissions},[500]);
 assert.equal(db.prepare('SELECT name FROM mx_modules WHERE uid=?').get(module.uid).name,module.name);
 db.exec('DROP TRIGGER mx_test_permission_failure');
 // Active creation fanout overlaps actual admin/field/preference mutations.
 const creates=Promise.all(Array.from({length:32},(_,i)=>mx.call(recordPath,owner.access_token,'POST',{operation_uid:randomUUID(),values:{name:`Administrative load ${i}`}},[201])));
 const updates=Promise.all(Array.from({length:16},(_,i)=>mx.call(modulePath,owner.access_token,'PUT',{description:`Committed description ${i}`})));
 const fields=Promise.all(Array.from({length:8},(_,i)=>mx.call(modulePath+'/fields',owner.access_token,'POST',{key:`extra_${i}`,label:`Extra ${i}`,field_type:'text',config:{}},[201])));
 const preferences=Promise.all(people.map(person=>mx.call('/mx/v1/account/preferences',person.access_token,'PUT',{revision:0,preferences:{theme:'dark',font_size_px:18}})));
 pending.push(creates,updates,fields,preferences);
 const [records,,createdFields]=await Promise.all([creates,updates,fields,preferences]);
 assert.equal(new Set(records.map(r=>r.body.uid)).size,32);
 assert.equal(db.prepare('SELECT count(*) n FROM mx_fields WHERE module_uid=?').get(module.uid).n,9);
 await mx.call('/mx/v1/account/preferences',peer.access_token,'PUT',{revision:0,preferences:{theme:'light'}},[409]);
 assert.equal(db.prepare('SELECT revision FROM mx_user_preferences WHERE user_uid=?').get(peer.uid).revision,1);
 assert.ok(createdFields.every(result=>result.body.uid));
 await until(()=>db.prepare('SELECT count(*) n FROM mx_notification_outbox').get().n===0,'administrative load notification drain',30000);
 db.close();db=undefined;
 await mx.restart({signal:'SIGKILL'});
 db=new DatabaseSync(join(mx.directory,'test.db'),{readOnly:true});
 assert.equal(db.prepare('SELECT count(*) n FROM mx_records WHERE module_uid=?').get(module.uid).n,32);
 assert.equal(db.prepare('SELECT count(*) n FROM mx_fields WHERE module_uid=?').get(module.uid).n,9);
 assert.equal((await mx.call(grantPath,owner.access_token)).body.revision,1);
 assert.equal((await mx.call('/mx/v1/account/preferences',peer.access_token)).body.revision,1);
 assert.deepEqual(db.prepare('PRAGMA foreign_key_check').all(),[]);
 assert.equal(db.prepare('PRAGMA integrity_check').get().integrity_check,'ok');
 report.passed=true;Object.assign(report,{directory:mx.directory,grantReadWhileWriterHeld:true,noRevisionRowOnGet:true,staleGrantAndPreferenceWritesRejected:true,moduleRollback:true,concurrentRecords:32,concurrentModuleUpdates:16,concurrentNewFields:8,commitsSurvivedRestart:true});
}catch(error){report.error=String(error.stack||error).replace(/Bearer\s+[A-Za-z0-9._-]+/g,'Bearer [redacted]');process.exitCode=1;console.error(report.error);}
finally{
 if(locked)db.exec('ROLLBACK');await Promise.allSettled(pending);db?.close();await mx.close();
 await writeFile(join(mx.directory,'administration-writes-results.json'),JSON.stringify(report,null,2),{mode:0o600});console.log(JSON.stringify(report));
}
