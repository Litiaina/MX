// 100,000 records and a held WAL writer; never a production database or N1.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {DatabaseSync} from 'node:sqlite';
import {startIsolatedMx} from './isolated-mx.mjs';
import {workflowTemplate} from '../src/lib/util/workflowTemplates.ts';
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/PROJECT-DISK-1TB/'));
assert.ok(process.env.TMPDIR?.startsWith('/mnt/PROJECT-DISK-1TB/'));
assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const results={passed:false};let writer;
try{
  console.log(`Module process scale artifacts: ${mx.directory}`);
  const password=`Disposable!${randomUUID()}`;
  await mx.call('/mx/v1/auth/create',mx.signupKey,'POST',{email:'owner@process-scale.test',name:'Owner',password},[201]);
  const owner=(await mx.call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@process-scale.test',password})).body;
  const call=async(path,method='GET',data,expected=[200])=>(await mx.call(path,owner.access_token,method,data,expected)).body;
  const module=(await call('/mx/v1/admin/modules','POST',{name:'Scale process',singular_name:'Item'},[201])).module;
  const base=`/mx/v1/modules/${module.uid}`;
  await call(`${base}/workflow`,'PUT',{revision:0,definition:{...workflowTemplate('crm'),enabled:true},confirm_existing_records:true});
  writer=new DatabaseSync(join(mx.directory,'test.db'));writer.exec('PRAGMA busy_timeout=1000;BEGIN IMMEDIATE');
  const insert=writer.prepare("INSERT INTO mx_records(uid,control_year,control_no,date,office,requestor,subject,routed_to_div,remarks,module_uid) VALUES(?,0,?,'','','','','','',?)");
  for(let i=0;i<100000;i++)insert.run(`process-scale-${String(i).padStart(6,'0')}`,i+1,module.uid);
  writer.exec('COMMIT');
  assert.equal(writer.prepare('SELECT COUNT(*) AS n FROM mx_workflow_instances').get().n,0,'Enabling workflow never creates one instance per existing row');
  // One independent connection owns SQLite's WAL write lock. Every workflow
  // GET, including config impact preview, must remain purely read-only.
  writer.exec('BEGIN IMMEDIATE');const start=performance.now();let pages;
  try{pages=await Promise.all(Array.from({length:8},()=>call(`${base}/work-queue`)));await Promise.all([call(`${base}/workflow`),call(`${base}/records/process-scale-000000/workflow`),call(`${base}/records/process-scale-000000/workflow/history`)]);}finally{writer.exec('ROLLBACK');}
  results.held_writer_read_milliseconds=Math.round(performance.now()-start);assert.ok(results.held_writer_read_milliseconds<5000,'Workflow reads must not wait for the writer');
  assert.ok(pages.every(p=>p.items.length===40&&p.has_next));assert.equal(new Set(pages[0].items.map(i=>i.uid)).size,40);
  const next=await call(`${base}/work-queue?after=${pages[0].next_cursor}`);assert.ok(next.items.every(i=>!pages[0].items.some(first=>first.uid===i.uid)));
  writer.exec('BEGIN IMMEDIATE');const totalsStart=performance.now();let totals;
  try{totals=await Promise.all(Array.from({length:4},()=>call(`${base}/work-queue?counts_only=true`)));}finally{writer.exec('ROLLBACK');}
  results.grouped_totals_milliseconds=Math.round(performance.now()-totalsStart);assert.ok(results.grouped_totals_milliseconds<5000,'Grouped workspace totals must remain read-only behind a writer');
  assert.ok(totals.every(t=>t.total===100000&&t.items.length===0&&t.stage_totals.find(s=>s.state==='lead').total===100000));
  const record='process-scale-000000',status=await call(`${base}/records/${record}/workflow`);
  const action={action:'qualify',configuration_revision:status.configuration_revision,record_revision:status.record_revision,workflow_revision:status.workflow_revision,note:''};
  const concurrent=await Promise.all(Array.from({length:8},()=>call(`${base}/records/${record}/workflow/actions`,'POST',{...action,operation_uid:randomUUID()},[200,409])));
  assert.equal(concurrent.filter(r=>r.event_uid).length,1,'Distinct concurrent action attempts cannot both commit');
  assert.equal((await call(`${base}/records/${record}/workflow/history`)).items.length,1);
  assert.equal(writer.prepare('SELECT COUNT(*) AS n FROM mx_workflow_instances').get().n,1);
  const changedCounts=await call(`${base}/work-queue?counts_only=true`);assert.equal(changedCounts.stage_totals.find(s=>s.state==='qualified').total,1);assert.equal(changedCounts.stage_totals.find(s=>s.state==='lead').total,99999);
  const before=writer.prepare('SELECT COUNT(*) AS n, SUM(control_no) AS total FROM mx_records').get();
  await mx.restart();const after=writer.prepare('SELECT COUNT(*) AS n, SUM(control_no) AS total FROM mx_records').get();assert.deepEqual(after,before,'Migration/restart preserves all record identifiers and numbers');
  assert.equal((await call(`${base}/records/${record}/workflow`)).state,'qualified');
  results.records=100000;results.concurrent_readers=8;results.window_size=40;results.distinct_concurrent_attempts=8;results.instances_materialized=1;results.passed=true;console.log(JSON.stringify(results));
}finally{writer?.close();await writeFile(join(mx.directory,'module-processes-scale-results.json'),JSON.stringify(results,null,2));await mx.close();}
