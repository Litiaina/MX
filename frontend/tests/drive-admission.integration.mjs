// Deterministic admission regression, not a capacity qualification or live-N1 proof.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {startIsolatedMx} from './isolated-mx.mjs';
import {onTestDisk,until} from './concurrency-fixture.mjs';

assert.ok(onTestDisk(process.env.MX_TEST_ARTIFACTS));
assert.ok(onTestDisk(process.env.TMPDIR));
assert.ok(onTestDisk(process.env.MX_TEST_BINARY));
assert.ok(!process.env.MX_TEST_N1_URL,'Only the owned storage boundary fixture may be stalled');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const report={passed:false,scope:'Eight deliberately stalled parts: unchanged cap; foreground DB progress and early invalid-metadata rejection',checks:[]};
const release=Promise.withResolvers();let pending=[];
const safeError=error=>String(error.message).split('\n')[0].replace(/Bearer\s+\S+/g,'Bearer [redacted]');
const call=async(path,token,method='GET',data,expected=[200])=>(await mx.call(path,token,method,data,expected)).body;
console.log(`Drive admission evidence: ${mx.directory}`);
try{
  const password=`Fixture!${randomUUID()}`,email='admission@test.invalid';
  await call('/mx/v1/auth/create',mx.signupKey,'POST',{email,name:'Admission control',password},[201]);
  const auth=await call('/mx/v1/auth/authenticate',null,'POST',{email,password});
  const module=(await call('/mx/v1/admin/modules',auth.access_token,'POST',{name:'Independent records',singular_name:'Record'},[201])).module;
  await call(`/mx/v1/admin/modules/${module.uid}/fields`,auth.access_token,'POST',{key:'name',label:'Name',field_type:'text'},[201]);
  const base='/mx/v1/drive/uploads',bytes=Buffer.alloc(1024,73),operations=[];
  for(let i=0;i<8;i++){
    const operation=randomUUID();operations.push(operation);
    await call(base,auth.access_token,'POST',{operation_uid:operation,file_name:`Held ${i}.bin`,mime_type:'application/octet-stream',size:bytes.length,last_modified:1});
  }
  let entered=0;
  // This test injects waiting, not an acknowledgement failure/retry. The separate
  // recovery suite covers the fixture's default transient first-part failure.
  mx.n1.control.beforePart=async session=>{session.failedOnce=true;entered++;await release.promise;};
  pending=operations.map(operation=>mx.api.put(`${base}/${operation}/parts/0`,{headers:{Authorization:`Bearer ${auth.access_token}`,'Content-Type':'application/octet-stream'},data:bytes,timeout:30000}));
  await until(()=>entered===8,'all eight transfer slots are occupied',10000);
  const invalidStarted=performance.now();
  try{
    const response=await mx.api.post(base,{headers:{Authorization:`Bearer ${auth.access_token}`},data:{operation_uid:randomUUID(),file_name:'Invalid.bin',mime_type:'application/octet-stream',size:-1},timeout:2000});
    assert.equal(response.status(),400,await response.text());
    report.checks.push({name:'invalid-metadata-does-not-wait-for-transfer-slot',passed:true,milliseconds:performance.now()-invalidStarted});
  }catch(error){report.checks.push({name:'invalid-metadata-does-not-wait-for-transfer-slot',passed:false,error:safeError(error)});}
  const saveStarted=performance.now();
  const saved=await call(`/mx/v1/modules/${module.uid}/records`,auth.access_token,'POST',{operation_uid:randomUUID(),values:{name:'Saved while N1 is stalled'}},[201]);
  assert.ok(saved.uid);assert.ok(performance.now()-saveStarted<2000,'No N1 await may retain the SQLite writer turn');
  await call('/mx/v1/drive',auth.access_token);
  report.checks.push({name:'foreground-record-and-drive-read-progress',passed:true,milliseconds:performance.now()-saveStarted,stalledParts:entered});
  release.resolve();
  const parts=await Promise.all(pending);for(const part of parts)assert.equal(part.status(),200,await part.text());
  for(const operation of operations){
    const item=await call(`${base}/${operation}/finish`,auth.access_token,'POST');
    const response=await mx.api.get(`/mx/v1/drive/items/${item.uid}/download`,{headers:{Authorization:`Bearer ${auth.access_token}`}});
    assert.equal(response.status(),200);assert.deepEqual(await response.body(),bytes);
  }
  assert.deepEqual(mx.n1.failures,[]);
  report.checks.push({name:'all-stalled-parts-finalize-with-original-bytes',passed:true,files:8});
  assert.ok(report.checks.every(c=>c.passed));report.passed=true;
}catch(error){report.failure=safeError(error);process.exitCode=1;console.error(report.failure);}
finally{
  release.resolve();await Promise.allSettled(pending);await mx.close();
  await writeFile(join(mx.directory,'drive-admission-results.json'),JSON.stringify(report,null,2),{mode:0o600});
  console.log(JSON.stringify({passed:report.passed,directory:mx.directory,checks:report.checks}));
}
