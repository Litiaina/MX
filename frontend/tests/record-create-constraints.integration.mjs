// Disposable HTTP/SQLite regression: module fields must not inherit hidden
// legacy uniqueness, and restored sequence state must not block future saves.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {DatabaseSync} from 'node:sqlite';
import {startIsolatedMx} from './isolated-mx.mjs';
import {onTestDisk} from './concurrency-fixture.mjs';

assert.ok(onTestDisk(process.env.MX_TEST_ARTIFACTS));
assert.ok(onTestDisk(process.env.TMPDIR));
assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const evidence={passed:false,checks:[]};
const baseline=process.env.MX_TEST_EXPECT_LEGACY_CONTROL_CONFLICT==='1';
try {
  const password=`Disposable!${randomUUID()}`;
  await mx.call('/mx/v1/auth/create',mx.signupKey,'POST',{email:'constraints@test.invalid',name:'Constraint tester',password},[201]);
  const user=(await mx.call('/mx/v1/auth/authenticate',null,'POST',{email:'constraints@test.invalid',password})).body;
  const call=async(path,method='GET',data,expected=[200])=>(await mx.call(path,user.access_token,method,data,expected)).body;
  const module=async name=>(await call('/mx/v1/admin/modules','POST',{name,singular_name:'Entry'},[201])).module;
  const a=await module('Independent control numbers A'),b=await module('Independent control numbers B');
  for(const m of [a,b]) {
    await call(`/mx/v1/admin/modules/${m.uid}/fields`,'POST',{key:'date',label:'Date',field_type:'date',unique_value:false},[201]);
    await call(`/mx/v1/admin/modules/${m.uid}/fields`,'POST',{key:'control_no',label:'Control number',field_type:'integer',unique_value:false},[201]);
  }
  const create=(m,values,operation_uid=randomUUID(),expected=[201])=>call(`/mx/v1/modules/${m.uid}/records`,'POST',{values,operation_uid},expected);
  const values={date:'2026-10-27',control_no:17};
  const first=await create(a,values);
  if(baseline) {
    const conflict=await create(b,values,randomUUID(),[409]);
    assert.match(conflict.response,/unique field or record relationship/);
    evidence.checks.push('Reproduced reported generic conflict with no unique fields: same control number in an independent module');
  } else {
    const same=await create(a,values),other=await create(b,values);
    assert.deepEqual(first.values,values);assert.deepEqual(same.values,values);assert.deepEqual(other.values,values);
    assert.equal(new Set([first.uid,same.uid,other.uid]).size,3);
    evidence.checks.push('Repeated non-unique control numbers save within one module and across independent modules');

    const parallel=await Promise.all(Array.from({length:20},(_,i)=>create(i%2?a:b,values)));
    assert.equal(new Set(parallel.map(r=>r.uid)).size,20);
    const operation=randomUUID(),saved=await create(a,values,operation);
    assert.equal((await create(a,values,operation,[200])).uid,saved.uid);
    evidence.checks.push('Twenty concurrent identical non-unique values commit; same operation replays exactly once');

    const c=await module('Restored hidden sequence');
    await call(`/mx/v1/admin/modules/${c.uid}/fields`,'POST',{key:'name',label:'Name',field_type:'text'},[201]);
    const original=await create(c,{name:'Existing record before sequence rewind'});
    const db=new DatabaseSync(join(mx.directory,'test.db'));
    try {
      // This independent fixture connection deliberately corrupts metadata to
      // simulate a restored sequence. Cooperate with the live durable outbox
      // using MX's existing busy budget, not SQLite's default zero-wait failure.
      db.exec('PRAGMA busy_timeout=10000');
      const old=db.prepare('SELECT last_value FROM mx_control_sequence WHERE year=0').get().last_value;
      db.prepare('UPDATE mx_control_sequence SET last_value=0 WHERE year=0').run();
      await create(c,{name:'Saved after stale sequence'});
      assert.ok(db.prepare('SELECT last_value FROM mx_control_sequence WHERE year=0').get().last_value>old);
      db.prepare('DELETE FROM mx_control_sequence WHERE year=0').run();
      await create(c,{name:'Saved after missing sequence'});
      assert.equal(db.prepare('PRAGMA integrity_check').get().integrity_check,'ok');
      assert.equal(db.prepare('PRAGMA foreign_key_check').all().length,0);
    } finally {db.close();}
    assert.equal((await call(`/mx/v1/modules/${c.uid}/records/${original.uid}`)).values.name,'Existing record before sequence rewind');
    evidence.checks.push('Stale and missing hidden sequence state self-reconciles without changing any existing record');

    await call(`/mx/v1/admin/modules/${c.uid}/fields`,'POST',{key:'reference',label:'External reference',field_type:'text',unique_value:true},[201]);
    const unique=await create(c,{name:'First unique value',reference:'KEEP-UNIQUE'});
    const conflict=await create(c,{name:'Must not save',reference:'keep-unique'},randomUUID(),[409]);
    assert.match(conflict.response,/External reference/);assert.match(conflict.response,/must be unique/);
    const second=await create(c,{name:'Other record',reference:'SECOND'});
    const patch=await call(`/mx/v1/modules/${c.uid}/records/${second.uid}`,'PATCH',{base_revision:second.revision,changes:{reference:'KEEP-UNIQUE'}},[409]);
    assert.match(patch.response,/External reference/);
    const replace=await call(`/mx/v1/modules/${c.uid}/records/${second.uid}`,'PUT',{base_revision:second.revision,values:{name:'Other record',reference:'KEEP-UNIQUE'}},[409]);
    assert.match(replace.response,/External reference/);
    assert.equal((await call(`/mx/v1/modules/${c.uid}/records/${second.uid}`)).values.reference,'SECOND');
    assert.equal((await call(`/mx/v1/modules/${c.uid}/records/${unique.uid}`)).values.reference,'KEEP-UNIQUE');
    evidence.checks.push('Explicit uniqueness remains enforced on create/PATCH/PUT, names the field, and atomically preserves both records');
    await mx.restart();
    assert.deepEqual((await call(`/mx/v1/modules/${a.uid}/records/${first.uid}`)).values,values);
    await create(b,values);
    evidence.checks.push('Restart preserves prior values and permits new duplicate values in non-unique fields');
  }
  evidence.passed=true;
  console.log(JSON.stringify({directory:mx.directory,...evidence}));
} finally {
  await writeFile(join(mx.directory,'record-create-constraints-results.json'),JSON.stringify(evidence,null,2));
  await mx.close();
}
