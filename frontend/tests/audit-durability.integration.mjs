// Acknowledged bounded audit batches must persist before HTTP replies complete.
// This is an isolated fault/barrier regression, not a 100-user capacity test.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {randomUUID} from 'node:crypto';
import {readFile,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {startIsolatedMx} from './isolated-mx.mjs';
import {onTestDisk,sleep} from './concurrency-fixture.mjs';
assert.ok(onTestDisk(process.env.MX_TEST_ARTIFACTS));assert.ok(onTestDisk(process.env.TMPDIR));assert.ok(onTestDisk(process.env.MX_TEST_BINARY));
assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const report={passed:false,scope:'Real invalid-response audit barrier, bounded batching, restart and exact receipt count; not power-loss/capacity qualification'};
let blocker,pending=[];
try{
 const password=`Disposable!${randomUUID()}`;
 await mx.call('/mx/v1/auth/create',mx.signupKey,'POST',{email:'audit@test.invalid',name:'Audit fixture',password},[201]);
 const auth=(await mx.call('/mx/v1/auth/authenticate',null,'POST',{email:'audit@test.invalid',password})).body;
 blocker=new DatabaseSync(join(mx.directory,'test.db'));blocker.exec('BEGIN IMMEDIATE');
 let completed=0;
 pending=Array.from({length:96},()=>mx.call('/mx/v1/drive/uploads',auth.access_token,'POST',{operation_uid:randomUUID(),file_name:'Invalid.bin',size:-1,mime_type:'application/octet-stream'},[400]).then(result=>{completed++;return result;}));
 await sleep(1500);
 assert.equal(completed,0,'Even a validation response waits for its audit persistence acknowledgement');
 blocker.exec('COMMIT');blocker.close();blocker=undefined;
 await Promise.all(pending);
 const inspect=()=>{
  const db=new DatabaseSync(join(mx.directory,'test.db'),{readOnly:true});
  try{return db.prepare("SELECT count(*) n,count(DISTINCT event_uid) distinctEvents,sum(success) successes,min(status_code) minimumStatus,max(status_code) maximumStatus FROM mx_audit_log WHERE path='/mx/v1/drive/uploads'").get();}finally{db.close();}
 };
 const expected={n:96,distinctEvents:96,successes:0,minimumStatus:400,maximumStatus:400};
 assert.deepEqual({...inspect()},expected);
 const log=await readFile(join(mx.directory,'server.log'),'utf8');
 assert.ok(log.split('\n').some(line=>line.includes('Slow SQLite serialized mutation')&&line.includes('audit_sender')),'The blocked operation is the audit writer, not invalid-upload admission');
 await mx.restart({signal:'SIGKILL'});
 assert.deepEqual({...inspect()},expected);
 report.passed=true;Object.assign(report,{directory:mx.directory,concurrentReceipts:96,noEarlyReplies:true,auditsPresentBeforeRepliesComplete:true,uniqueEvents:96,eventsSurvivedRestart:96});
}catch(error){report.error=String(error.stack||error).replace(/Bearer\s+[A-Za-z0-9._-]+/g,'Bearer [redacted]');process.exitCode=1;console.error(report.error);}
finally{
 if(blocker){try{blocker.exec('ROLLBACK');}finally{blocker.close();}}
 await Promise.allSettled(pending);await mx.close();
 await writeFile(join(mx.directory,'audit-durability-results.json'),JSON.stringify(report,null,2),{mode:0o600});
 console.log(JSON.stringify(report));
}
