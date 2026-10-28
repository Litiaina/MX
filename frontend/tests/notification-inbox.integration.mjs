// Reproduce the 100-reader reconnect burst on a COPY of a retained failed
// capacity fixture. This is not a login, write-load or N1 qualification.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {randomUUID,createHmac,createHash} from 'node:crypto';
import {readFile,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {startIsolatedMx} from './isolated-mx.mjs';
import {onTestDisk,client,distribution} from './concurrency-fixture.mjs';

for(const key of ['MX_TEST_ARTIFACTS','MX_TEST_BINARY','MX_TEST_INBOX_DATABASE','TMPDIR'])assert.ok(onTestDisk(process.env[key]),key);
assert.ok(!process.env.MX_TEST_N1_URL);
process.umask(0o077);
const report={passed:false,scope:'100 account-specific notification readers and auth-version checks on a copied capacity fixture; signed fixture JWTs, not password-login, Office, upload or general capacity qualification',rounds:[]};
const jwtSecret=randomUUID()+randomUUID();
let mx,http,db;
try{
 report.mxSha256=createHash('sha256').update(await readFile(process.env.MX_TEST_BINARY)).digest('hex');
 report.sourceDatabase=process.env.MX_TEST_INBOX_DATABASE;
 mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)),{seedDatabase:process.env.MX_TEST_INBOX_DATABASE,jwtSecret});
 report.directory=mx.directory;
 db=new DatabaseSync(join(mx.directory,'test.db'),{readOnly:true});
 report.notificationRows=db.prepare('SELECT count(*) n FROM mx_notifications').get().n;
 assert.ok(report.notificationRows>=500000,'Must retain a genuinely large inbox history');
 const accounts=db.prepare("SELECT u.uid,u.email,u.access_level,u.auth_version,count(*) history FROM users u JOIN mx_notifications n ON n.recipient_uid=u.uid WHERE u.email LIKE 'user%@concurrency.test' GROUP BY u.uid ORDER BY u.email LIMIT 100").all();
 assert.equal(accounts.length,100);
 const header=Buffer.from(JSON.stringify({alg:'HS256',typ:'JWT'})).toString('base64url');
 const users=accounts.map(user=>{
  const payload=Buffer.from(JSON.stringify({...user,history:undefined,token_kind:'access',exp:Math.floor(Date.now()/1000)+3600})).toString('base64url');
  return {...user,token:`${header}.${payload}.${createHmac('sha256',jwtSecret).update(`${header}.${payload}`).digest('base64url')}`};
 });
 http=client(`https://localhost:${mx.port}`,{ca:await readFile(join(mx.directory,'cert.pem')),timeout:15000});
 const inbox=async(user,query='')=>{
  const result=(await http.call('/mx/v1/notifications?limit=100'+query,user.token)).body;
  assert.equal(result.notifications.length,100);
  assert.equal(new Set(result.notifications.map(n=>n.uid)).size,100);
  assert.ok(result.notifications.every(n=>db.prepare('SELECT recipient_uid FROM mx_notifications WHERE uid=?').get(n.uid)?.recipient_uid===user.uid));
  return result;
 };
 for(let round=0;round<3;round++){
  const offset=http.samples.length,start=performance.now();
  const outcomes=await Promise.allSettled(users.map(async user=>{
   const result=await inbox(user);
   const session=(await http.call('/mx/v1/auth/session',user.token)).body;
   assert.equal(session.uid,user.uid);
   return result;
  }));
  const errors=outcomes.filter(r=>r.status==='rejected').map(r=>String(r.reason));
  report.rounds.push({name:`latest-${round+1}`,seconds:(performance.now()-start)/1000,requests:http.samples.length-offset,latencyMs:distribution(http.samples.slice(offset).map(s=>s.ms)),errors});
  console.log(JSON.stringify(report.rounds.at(-1)));
  assert.deepEqual(errors,[],'All 100 readers and auth-version checks must succeed without retries');
  const results=outcomes.map(r=>r.value);
  if(round===0){
   // Cursor ties must advance by UID, without duplicates or lost records.
   const user=users[0],last=results[0].notifications[0];
   const cursor=(await http.call(`/mx/v1/notifications?limit=100&after=${last.created_at}&after_uid=${last.uid}`,user.token)).body;
   assert.ok(cursor.notifications.every(n=>n.created_at>last.created_at||(n.created_at===last.created_at&&n.uid>last.uid)));
  }
 }
 await Promise.all(users.map(user=>http.call('/mx/v1/notifications/read-all',user.token,'POST')));
 const offset=http.samples.length,start=performance.now();
 await Promise.all(users.map(async user=>{
  const result=(await http.call('/mx/v1/notifications?limit=100&unread_only=true',user.token)).body;
  assert.equal(result.unread,0);assert.equal(result.notifications.length,0);
 }));
 report.rounds.push({name:'acknowledged-unread-range',seconds:(performance.now()-start)/1000,requests:http.samples.length-offset,latencyMs:distribution(http.samples.slice(offset).map(s=>s.ms))});
 const log=await readFile(join(mx.directory,'server.log'),'utf8');
 assert.ok(!/\bERROR\b|database is locked|panicked/i.test(log),'No hidden pool/SQLite failures');
 assert.equal(db.prepare('PRAGMA integrity_check').get().integrity_check,'ok');
 report.passed=true;
}catch(error){report.error=error.stack;process.exitCode=1;console.error(error);}
finally{
 db?.close();http?.close();if(mx)await mx.close();
 await writeFile(join(process.env.MX_TEST_ARTIFACTS,'notification-inbox-results.json'),JSON.stringify(report,null,2));
 console.log(JSON.stringify({passed:report.passed,notificationRows:report.notificationRows,rounds:report.rounds,error:report.error}));
}
