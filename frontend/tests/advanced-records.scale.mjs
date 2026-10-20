// Disposable fixture seeding, then real HTTP reads under a WAL writer.
// This is a structural/load regression, not a production hardware benchmark.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {DatabaseSync} from 'node:sqlite';
import {writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {startIsolatedMx} from './isolated-mx.mjs';
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/home/altear/Temp Environment/')||process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/'));
assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const results={passed:false,records:100000,checks:[],timings:[]};let database;
const call=async(path,user,method='GET',data,expected=[200])=>{const response=await mx.call(path,user?.access_token,method,data,expected);results.timings.push({method,path,milliseconds:response.milliseconds});return response.body;};
try {
  console.log(`Advanced scale artifacts: ${mx.directory}`);
  const password=`Disposable!${randomUUID()}`;
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@scale.test',name:'Owner',password},[201]);
  const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@scale.test',password});
  const users=[];for(let i=0;i<8;i++){const email=`editor${i}@scale.test`;await call('/mx/v1/user/create',owner,'POST',{email,name:`Editor ${i}`,password,access_level:2},[201]);users.push(await call('/mx/v1/auth/authenticate',null,'POST',{email,password}));}
  const employees=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Employees',singular_name:'Employee'},[201])).module;
  const requests=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Requests',singular_name:'Request'},[201])).module;
  for(const user of users){const p=await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner);await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner,'PUT',{revision:p.revision,grants:[employees,requests].map(m=>({module_uid:m.uid,can_read:true,can_create:true,can_update:true,can_report:true,can_delete:false,can_configure:false,can_attachments:true}))});}
  const field=(module,key,type,config={})=>call(`/mx/v1/admin/modules/${module.uid}/fields`,owner,'POST',{key,label:key,field_type:type,config},[201]);
  await field(employees,'name','text');await field(employees,'allowance','decimal');
  const employee=await call(`/mx/v1/modules/${employees.uid}/records`,owner,'POST',{values:{name:'Employee',allowance:500000}},[201]);
  const relationship=await field(requests,'employee','relationship',{target_module_uid:employees.uid,label_field:'name',multiple:false});
  const amount=await field(requests,'amount','decimal',{table_summary:'sum'}),status=await field(requests,'status','select',{options:['Approved','Pending']});
  await field(employees,'used','rollup',{direction:'incoming',source_module_uid:requests.uid,relationship_field:'employee',target_field:'amount',function:'sum',filter:{match:'all',conditions:[{field:'status',operator:'eq',value:'Approved'}]}});
  await field(employees,'remaining','formula',{expression:'allowance-used',decimals:2});
  database=new DatabaseSync(join(mx.directory,'test.db'));database.exec('PRAGMA foreign_keys=ON;PRAGMA busy_timeout=10000;BEGIN IMMEDIATE;');
  const insert=database.prepare("INSERT INTO mx_records(uid,module_uid,control_year,control_no,date,office,requestor,subject,routed_to_div,remarks) VALUES(?,?,0,?,'','','','','','')");
  const value=database.prepare('INSERT INTO mx_record_values(record_uid,field_uid,value_text,value_real) VALUES(?,?,?,?)');
  const own=database.prepare("INSERT INTO mx_record_owners(record_uid,creator_uid,provenance) VALUES(?,?,'scale-fixture')");
  const started=performance.now();
  for(let i=0;i<results.records;i++){const id=randomUUID();insert.run(id,requests.uid,i+2);own.run(id,users[i%8].uid);value.run(id,relationship.uid,employee.uid,null);value.run(id,amount.uid,null,5);value.run(id,status.uid,i%2?'Pending':'Approved',null);}
  database.exec('COMMIT;');results.seedMilliseconds=Math.round(performance.now()-started);
  const recordPath=`/mx/v1/modules/${employees.uid}/records/${employee.uid}`;
  const record=await call(recordPath,owner);assert.equal(record.values.used,250000);assert.equal(record.values.remaining,250000);
  const listPath=`/mx/v1/modules/${requests.uid}/records`;
  const first=await call(`${listPath}?window=start&limit=50`,users[0]);assert.equal(first.total,100000);assert.equal(first.data.length,50);assert.equal(first.summaries.amount.value,500000);
  const next=await call(`${listPath}?window=after&limit=50&metadata=false&cursor=${encodeURIComponent(first.window.cursors.at(-1))}`,users[0]);assert.equal(next.data.length,50);assert.equal(new Set([...first.data,...next.data].map(r=>r.uid)).size,100);
  const plan=database.prepare('EXPLAIN QUERY PLAN SELECT record_uid FROM mx_record_links WHERE target_uid=? AND field_uid=?').all(employee.uid,relationship.uid);assert.ok(plan.some(row=>row.detail.includes('mx_record_links_target')));results.linkQueryPlan=plan;
  results.checks.push('100,000 linked requests: correct filtered balance, bounded 50-row cursor windows, whole-result totals, indexed incoming links');
  await call(`/mx/v1/admin/modules/${requests.uid}`,owner,'PUT',{config:{record_visibility:'creator'}});
  for(const [i,user]of users.entries()){const mine=await call(`${listPath}?limit=1`,user);assert.equal(mine.total,12500);assert.equal(mine.summaries.amount.value,62500);const balance=await call(recordPath,user);assert.equal(balance.values.used,i%2?0:62500);}
  results.checks.push('Eight users: privacy before count/page/summary and incoming aggregation on 100,000 rows');
  // Hold a real uncommitted writer: reads must not attempt migrations/DDL.
  database.exec('BEGIN IMMEDIATE;');insert.run(randomUUID(),requests.uid,100002);
  const reads=await Promise.all(users.flatMap(user=>[call(`${listPath}?window=start&limit=10`,user),call(recordPath,user),call('/mx/v1/notifications?limit=5',user)]));
  database.exec('ROLLBACK;');assert.equal(reads.length,24);
  results.checks.push('24 concurrent HTTP reads complete while another connection holds the WAL writer; no database-locked responses');
  assert.equal(database.prepare('PRAGMA integrity_check').get().integrity_check,'ok');
  results.passed=true;console.log(JSON.stringify({passed:true,checks:results.checks,seedMilliseconds:results.seedMilliseconds,maxRequestMilliseconds:Math.max(...results.timings.map(t=>t.milliseconds))}));
}finally{if(database){try{database.exec('ROLLBACK');}catch{}database.close();}await writeFile(join(mx.directory,'advanced-scale-results.json'),JSON.stringify(results,null,2));await mx.close();}
