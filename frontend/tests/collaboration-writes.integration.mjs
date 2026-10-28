// Real API contention, membership rollback, permissions and crash recovery.
// Uses only an owned database/process. This is not capacity qualification.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {startIsolatedMx} from './isolated-mx.mjs';
import {onTestDisk,until} from './concurrency-fixture.mjs';

for(const key of ['MX_TEST_ARTIFACTS','MX_TEST_BINARY','TMPDIR'])assert.ok(onTestDisk(process.env[key]),key);
assert.ok(!process.env.MX_TEST_N1_URL);
process.umask(0o077);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const report={passed:false,scope:'Real Collaboration mutation contention and rollback regression; not 100-user capacity or physical power-loss qualification'};
const password=`Disposable!${randomUUID()}`,pending=[];
let db;
try{
 await mx.call('/mx/v1/auth/create',mx.signupKey,'POST',{email:'channels-owner@test.invalid',name:'Channel owner',password},[201]);
 const owner=(await mx.call('/mx/v1/auth/authenticate',null,'POST',{email:'channels-owner@test.invalid',password})).body;
 const people=[];
 for(let i=0;i<3;i++){
  const email=`channels-peer-${i}@test.invalid`;
  await mx.call('/mx/v1/user/create',owner.access_token,'POST',{email,name:`Channel peer ${i}`,password,access_level:2},[201]);
  people.push((await mx.call('/mx/v1/auth/authenticate',null,'POST',{email,password})).body);
 }
 const [peer,target,outsider]=people;
 const module=(await mx.call('/mx/v1/admin/modules',owner.access_token,'POST',{name:'Concurrent channel records',singular_name:'Record'},[201])).body.module;
 await mx.call(`/mx/v1/admin/modules/${module.uid}/fields`,owner.access_token,'POST',{key:'name',label:'Name',field_type:'text',config:{}},[201]);
 for(const person of people){
  const access=(await mx.call(`/mx/v1/admin/accounts/${person.uid}/modules`,owner.access_token)).body;
  await mx.call(`/mx/v1/admin/accounts/${person.uid}/modules`,owner.access_token,'PUT',{revision:access.revision,grants:[{module_uid:module.uid,can_read:true,can_create:true,can_update:true,can_delete:false,can_configure:false,can_report:true,can_attachments:true}]});
 }
 const records=`/mx/v1/modules/${module.uid}/records`;
 const channels='/mx/v1/collaboration/channels';
 const channel=(await mx.call(channels,owner.access_token,'POST',{name:'Atomic membership',member_uids:[peer.uid]},[201])).body.uid;
 const path=`${channels}/${channel}`;
 const message=(await mx.call(path+'/messages',owner.access_token,'POST',{operation_uid:randomUUID(),body:'Original content'},[201])).body.message;
 db=new DatabaseSync(join(mx.directory,'test.db'));db.exec('PRAGMA busy_timeout=10000');
 // Reproduce partial membership changes deterministically: the membership SQL
 // succeeds, but the associated history insert fails. Both must roll back.
 db.exec(`CREATE TRIGGER mx_test_member_add_failure BEFORE INSERT ON mx_messages WHEN NEW.channel_uid='${channel}' AND NEW.event_kind='member_added' BEGIN SELECT RAISE(ABORT,'test membership history failure'); END`);
 await mx.call(path+'/members',owner.access_token,'POST',{user_uid:target.uid,role:'member'},[400]);
 assert.equal(db.prepare('SELECT count(*) n FROM mx_channel_members WHERE channel_uid=? AND user_uid=?').get(channel,target.uid).n,0,'Failed history must not grant access');
 db.exec('DROP TRIGGER mx_test_member_add_failure');
 await mx.call(path+'/members',owner.access_token,'POST',{user_uid:target.uid,role:'member'});
 db.exec(`CREATE TRIGGER mx_test_member_remove_failure BEFORE INSERT ON mx_messages WHEN NEW.channel_uid='${channel}' AND NEW.event_kind='member_removed' BEGIN SELECT RAISE(ABORT,'test membership history failure'); END`);
 await mx.call(path+'/members/'+target.uid,owner.access_token,'DELETE',undefined,[500]);
 assert.equal(db.prepare('SELECT count(*) n FROM mx_channel_members WHERE channel_uid=? AND user_uid=?').get(channel,target.uid).n,1,'Failed history must not silently remove access');
 db.exec('DROP TRIGGER mx_test_member_remove_failure');
 await mx.call(path+'/members/'+target.uid,owner.access_token,'DELETE',undefined,[204]);
 await mx.call(path+'/messages',outsider.access_token,'GET',undefined,[403]);
 await mx.call(`/mx/v1/collaboration/messages/${message.uid}`,peer.access_token,'PUT',{body:'Unauthorized edit'},[403]);
 await mx.call(`/mx/v1/collaboration/messages/${message.uid}/pin`,peer.access_token,'PUT',undefined,[403]);
 // Independent writers, channel creation and the direct find-or-create race
 // overlap with real notification delivery; no retries hide failed requests.
 const backgroundRecords=Promise.all(Array.from({length:32},(_,i)=>mx.call(records,owner.access_token,'POST',{operation_uid:randomUUID(),values:{name:`Concurrent record ${i}`}},[201])));
 pending.push(backgroundRecords);
 const createdChannels=Promise.all(Array.from({length:16},(_,i)=>mx.call(channels,owner.access_token,'POST',{name:`Concurrent channel ${i}`,member_uids:[peer.uid]},[201])));
 pending.push(createdChannels);
 const direct=Promise.all(Array.from({length:32},()=>mx.call('/mx/v1/collaboration/direct',owner.access_token,'POST',{user_uid:peer.uid},[200,201])));
 pending.push(direct);
 const [recordResults,channelResults,directResults]=await Promise.all([backgroundRecords,createdChannels,direct]);
 assert.equal(new Set(recordResults.map(r=>r.body.uid)).size,32);
 assert.equal(new Set(channelResults.map(r=>r.body.uid)).size,16);
 assert.equal(new Set(directResults.map(r=>r.body.uid)).size,1);
 assert.equal(directResults.filter(r=>r.body.created).length,1,'Exactly one direct conversation is created');
 const directUid=directResults[0].body.uid;
 assert.equal(db.prepare('SELECT count(*) n FROM mx_channel_members WHERE channel_uid=?').get(directUid).n,2);
 // Concurrent toggles must be atomic, not independent read-then-insert races.
 await Promise.all(Array.from({length:32},()=>mx.call(`/mx/v1/collaboration/messages/${message.uid}/reactions`,peer.access_token,'POST',{emoji:'👍'})));
 assert.equal(db.prepare('SELECT count(*) n FROM mx_message_reactions WHERE message_uid=?').get(message.uid).n,0);
 await mx.call(path,owner.access_token,'PUT',{name:'Updated channel',description:'Committed metadata',invite_policy:'owner'});
 await mx.call(path+'/members',peer.access_token,'POST',{user_uid:target.uid,role:'member'},[403]);
 await mx.call(`/mx/v1/collaboration/messages/${message.uid}`,owner.access_token,'PUT',{body:'Committed content'});
 await mx.call(`/mx/v1/collaboration/messages/${message.uid}/pin`,owner.access_token,'PUT');
 db.exec(`CREATE TRIGGER mx_test_message_delete_failure BEFORE UPDATE OF deleted_at ON mx_messages WHEN NEW.uid='${message.uid}' BEGIN SELECT RAISE(ABORT,'test tombstone failure'); END`);
 await mx.call(`/mx/v1/collaboration/messages/${message.uid}`,owner.access_token,'DELETE',undefined,[404]);
 assert.equal(db.prepare('SELECT count(*) n FROM mx_message_pins WHERE message_uid=?').get(message.uid).n,1,'Failed tombstone must not remove its pin');
 assert.equal(db.prepare('SELECT deleted_at FROM mx_messages WHERE uid=?').get(message.uid).deleted_at,null);
 db.exec('DROP TRIGGER mx_test_message_delete_failure');
 await Promise.all(Array.from({length:16},()=>mx.call(path+'/read',peer.access_token,'POST')));
 await until(()=>db.prepare('SELECT count(*) n FROM mx_notification_outbox').get().n===0,'notification queue drain',30000);
 db.close();db=undefined;
 await mx.restart({signal:'SIGKILL'});
 db=new DatabaseSync(join(mx.directory,'test.db'),{readOnly:true});
 assert.equal(db.prepare('SELECT count(*) n FROM mx_records WHERE module_uid=?').get(module.uid).n,32);
 assert.equal(db.prepare('SELECT body FROM mx_messages WHERE uid=?').get(message.uid).body,'Committed content');
 assert.equal(db.prepare('SELECT name,invite_policy FROM mx_channels WHERE uid=?').get(channel).name,'Updated channel');
 assert.equal(db.prepare('SELECT count(*) n FROM mx_message_pins WHERE message_uid=?').get(message.uid).n,1);
 assert.equal(db.prepare("SELECT count(*) n FROM mx_messages WHERE channel_uid=? AND event_kind IN ('member_added','member_removed')").get(channel).n,2);
 assert.ok(db.prepare('SELECT last_read_message_id FROM mx_channel_members WHERE channel_uid=? AND user_uid=?').get(channel,peer.uid).last_read_message_id>0);
 assert.deepEqual(db.prepare('PRAGMA foreign_key_check').all(),[]);
 assert.equal(db.prepare('PRAGMA integrity_check').get().integrity_check,'ok');
 await mx.call(path+'/messages',target.access_token,'GET',undefined,[403]);
 await mx.call(`/mx/v1/collaboration/messages/${message.uid}`,owner.access_token,'DELETE',undefined,[204]);
 assert.equal(db.prepare('SELECT count(*) n FROM mx_message_pins WHERE message_uid=?').get(message.uid).n,0);
 report.passed=true;Object.assign(report,{directory:mx.directory,concurrentRecords:32,concurrentChannels:16,directRequests:32,directConversations:1,concurrentReactionToggles:32,atomicAddRemoveAndDelete:true,permissionsPreserved:true,commitsSurvivedRestart:true});
}catch(error){report.error=String(error.stack||error).replace(/Bearer\s+[A-Za-z0-9._-]+/g,'Bearer [redacted]');process.exitCode=1;console.error(report.error);}
finally{
 await Promise.allSettled(pending);db?.close();await mx.close();
 await writeFile(join(mx.directory,'collaboration-writes-results.json'),JSON.stringify(report,null,2),{mode:0o600});
 console.log(JSON.stringify(report));
}
