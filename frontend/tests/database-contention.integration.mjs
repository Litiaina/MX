// Production HTTP handlers, real SQLite/WAL and eight accounts. No live data.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {readFile,writeFile} from 'node:fs/promises';
import {DatabaseSync} from 'node:sqlite';
import {startIsolatedMx} from './isolated-mx.mjs';
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/tests/'));
assert.ok(!process.env.MX_TEST_N1_URL,'Never exercise production storage');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const timings=[],failures=[];let owner;
const refreshes=new WeakMap();
const call=async(path,user=owner,method='GET',data,expected=[200])=>{
 if(user?.refresh_token){
  const expires=JSON.parse(Buffer.from(user.access_token.split('.')[1],'base64url')).exp;
  if(expires*1000<Date.now()+120000){
   if(!refreshes.has(user))refreshes.set(user,mx.call('/mx/v1/auth/refresh',null,'POST',{refresh_token:user.refresh_token}).then(result=>Object.assign(user,result.body)).finally(()=>refreshes.delete(user)));
   await refreshes.get(user);
  }
 }
 const result=await mx.call(path,user?.access_token,method,data,expected);timings.push(result.milliseconds);return result.body;
};
let writer;
try{
 console.log(`Database contention artifacts: ${mx.directory}`);
 const password=`Test-only!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{name:'Load test',email:'large@test.invalid',password},[201]);
 owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'large@test.invalid',password});
 const module=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Large dataset',singular_name:'Record'},[201])).module;
 for(const key of ['name','detail'])await call(`/mx/v1/admin/modules/${module.uid}/fields`,owner,'POST',{key,label:key,field_type:'text'},[201]);
 const users=[];
 for(let index=0;index<8;index++){
  const email=`large-${index}@test.invalid`;await call('/mx/v1/user/create',owner,'POST',{email,name:`User ${index}`,password,access_level:2},[201]);
  const user=await call('/mx/v1/auth/authenticate',null,'POST',{email,password});
  const access=await call(`/mx/v1/admin/accounts/${user.uid}/modules`);
  await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner,'PUT',{revision:access.revision,grants:[{module_uid:module.uid,can_read:true,can_create:true,can_update:true,can_delete:false,can_configure:false,can_report:true,can_attachments:true}]});users.push(user);
 }
 writer=new DatabaseSync(join(mx.directory,'test.db'));writer.exec('PRAGMA busy_timeout=1000;BEGIN IMMEDIATE');
 const insert=writer.prepare("INSERT INTO mx_records(uid,control_year,control_no,date,office,requestor,subject,routed_to_div,remarks,module_uid) VALUES(?,0,?,'','','','','','',?)");
 const versionInsert=writer.prepare("INSERT INTO mx_record_versions(uid,record_uid,module_uid,version_no,event,actor_uid,created_at) VALUES(?,?,?,1,'created',?,?)");
 for(let index=1;index<=100000;index++){insert.run(`seed-${index}`,index,module.uid);versionInsert.run(`seed-version-${index}`,`seed-${index}`,module.uid,owner.uid,Date.now()-86400000-index);}
 // Synthetic rows bypass normal creation, so advance its hidden sequence just
 // as a real 100,000-record deployment would. Otherwise each create correctly
 // rejects duplicate control numbers rather than exercising import writes.
 writer.exec('INSERT INTO mx_control_sequence(year,last_value) VALUES(0,100000) ON CONFLICT(year) DO UPDATE SET last_value=100000;COMMIT');console.log('100,000 existing records seeded into disposable SQLite.');
 const endpoints=['/mx/v1/modules','/mx/v1/notifications','/mx/v1/collaboration/calls','/mx/v1/drive',`/mx/v1/modules/${module.uid}/schema`,`/mx/v1/modules/${module.uid}/records?limit=50`,`/mx/v1/admin/modules/${module.uid}/storage-layout`];
 // Reproduce the exact flaw: an independent real writer may not prevent pure
 // reads merely because their handlers also try to migrate/update the schema.
 writer.exec('BEGIN IMMEDIATE');
 const heldStarted=performance.now();
 try{await Promise.all(endpoints.map(path=>call(path)));}finally{writer.exec('ROLLBACK');}
 const heldWriteReadMilliseconds=Math.round(performance.now()-heldStarted);
 assert.ok(heldWriteReadMilliseconds<5000,`Read handlers waited behind a writer: ${heldWriteReadMilliseconds}ms`);
 console.log(`All seven affected read handlers work behind a held WAL writer (${heldWriteReadMilliseconds}ms).`);
 const rows=Number(process.env.MX_TEST_IMPORT_ROWS||25000);
 const bytes=Buffer.from('Name,Detail\n'+Array.from({length:rows},(_,index)=>`Imported ${index},Synthetic large-import row ${index}`).join('\n'));
 const imported=await mx.api.post(`/mx/v1/modules/${module.uid}/imports`,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{job_uid:randomUUID(),file:{name:'large.csv',mimeType:'text/csv',buffer:bytes}},timeout:120000});
 assert.equal(imported.status(),200,await imported.text());let job=await imported.json();
 const base=`/mx/v1/modules/${module.uid}/imports/${job.uid}`;
 job=await call(base+'/configure',owner,'PUT',{sheets:[{index:0,header_row:1,first_data_row:2,columns:[{column:1,key:'name',fill_down:false,date_format:''},{column:2,key:'detail',fill_down:false,date_format:''}]}]});
 assert.equal(job.pending,rows);
 let running=true,polls=0;const started=performance.now();
 const readers=users.map(async(user,index)=>{while(running){try{await call(endpoints[((polls++)+index)%6],user);}catch(error){failures.push(error.message);running=false;throw error;}await new Promise(resolve=>setTimeout(resolve,1000));}});
 // Observe rejections immediately while keeping complete failure evidence.
 const observedReaders=Promise.allSettled(readers);
 try{
  let previous=0;
  while(job.pending){assert.equal(failures.length,0,failures.join('\n'));job=await call(base+'/process',owner,'POST');assert.equal(job.failed,0,JSON.stringify(job));if(job.imported>=previous+5000){previous=job.imported;console.log(`${job.imported}/${rows} imported alongside eight active clients`);}}
 }finally{running=false;await observedReaders;}
 assert.deepEqual(failures,[]);assert.equal(job.imported,rows);assert.equal(job.failed,0);assert.equal(job.state,'completed');
 assert.equal((await call(`/mx/v1/modules/${module.uid}/records?limit=1`)).total,100000+rows);
 const versions=writer.prepare('SELECT count(*) n FROM mx_record_versions').get().n;assert.equal(versions,100000+rows,'Each imported row retains its normal version');
 assert.equal(writer.prepare("SELECT count(*) n FROM mx_audit_log WHERE event_uid LIKE 'record-version:%'").get().n,rows,'Import audits are committed with their records');
 assert.equal(writer.prepare("SELECT count(*) n FROM mx_import_rows WHERE state='imported'").get().n,rows);
 const importMilliseconds=Math.round(performance.now()-started);
 // Failure used to continue after import and restart: cover both explicitly.
 await Promise.all(users.map(user=>Promise.all(endpoints.slice(0,6).map(path=>call(path,user)))));
 writer.close();writer=undefined;await mx.restart();
 await Promise.all(users.map(user=>Promise.all(endpoints.slice(0,6).map(path=>call(path,user)))));
 writer=new DatabaseSync(join(mx.directory,'test.db'));
 assert.equal(writer.prepare("SELECT count(*) n FROM mx_audit_log WHERE event_uid LIKE 'record-version:%'").get().n,100000+rows,'Startup reconstructs older missing audit history without repeated request-time scans');
 writer.close();writer=undefined;await mx.restart();
 await Promise.all(users.map(user=>call('/mx/v1/modules',user)));
 const log=await readFile(join(mx.directory,'server.log'),'utf8');assert.ok(!/database is locked|database table is locked|pool.*timed out/i.test(log),'Server log must not conceal database contention');
 const sorted=timings.sort((a,b)=>a-b),report={passed:true,version:'1.0.0-rc.4',existingRecords:100000,importedRecords:rows,concurrentAccounts:8,readEndpoints:endpoints.length,heldWriteReadMilliseconds,importMilliseconds,requests:timings.length,backgroundPolls:polls,p95Milliseconds:sorted[Math.floor(sorted.length*.95)],maxMilliseconds:sorted.at(-1),postImportAndRestartReads:true,normalVersionHistory:true,databaseLockErrors:0,productionWrites:0};
 await writeFile(join(mx.directory,'database-contention-results.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report));
}finally{try{writer?.exec('ROLLBACK');}catch{}writer?.close();await mx.close();}
