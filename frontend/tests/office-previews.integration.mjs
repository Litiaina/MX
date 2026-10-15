// Optional PDF-preview API regression on disposable MX/N1 storage only.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {readFile,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {DatabaseSync} from 'node:sqlite';
import {startIsolatedMx} from './isolated-mx.mjs';
if(process.env.MX_TEST_N1_URL)throw new Error('Use isolated storage, not live N1.');
if(!process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/tests'))throw new Error('Set external-drive artifact directory.');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
async function upload(user,name,bytes){
 const operation=randomUUID();await call('/mx/v1/drive/uploads',user,'POST',{operation_uid:operation,file_name:name,mime_type:'application/octet-stream',size:bytes.length,last_modified:1});
 let response;for(let attempt=0;attempt<3;attempt++){response=await mx.api.put(`/mx/v1/drive/uploads/${operation}/parts/0`,{headers:{Authorization:`Bearer ${user.access_token}`,'Content-Type':'application/octet-stream'},data:bytes});if(response.status()!==502)break;}assert.equal(response.status(),200);
 return call(`/mx/v1/drive/uploads/${operation}/finish`,user,'POST');
}
try{
 console.log(`Office preview API artifacts: ${mx.directory}`);
 const password=`Test-only!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{name:'Preview tester',email:'preview@test.invalid',password},[201]);
 const user=await call('/mx/v1/auth/authenticate',null,'POST',{email:'preview@test.invalid',password});
 const small=await upload(user,'Bounded preview.rtf',Buffer.from(`{\\rtf1\\ansi MX preview ${randomUUID()}}`));
 const ticket=await call(`/mx/v1/drive/items/${small.uid}/ticket`,user,'POST',{});
 const replies=await Promise.all(Array.from({length:8},()=>mx.api.get(`${ticket.url}?office=true`,{timeout:60000})));
 const pdfs=[];for(const reply of replies){assert.equal(reply.status(),200,await reply.text());const bytes=await reply.body();assert.equal(bytes.subarray(0,5).toString(),'%PDF-');pdfs.push(bytes);}
 for(const pdf of pdfs)assert.deepEqual(pdf,pdfs[0]);
 assert.equal((await call(`/mx/v1/drive/items/${small.uid}`,user)).revision,1);
 const db=new DatabaseSync(join(mx.directory,'test.db'));
 try{db.exec('PRAGMA busy_timeout=10000');db.prepare('UPDATE mx_drive_versions SET size=? WHERE item_uid=?').run(20*1024**3,small.uid);}finally{db.close();}
 const before=mx.n1.requests.length;const rejected=await mx.api.get(`${ticket.url}?office=true`);assert.equal(rejected.status(),422);assert.match((await rejected.json()).response,/too complex/);assert.equal(mx.n1.requests.length,before,'Reject metadata size before downloading a multi-gigabyte original');
 const restored=new DatabaseSync(join(mx.directory,'test.db'));try{restored.exec('PRAGMA busy_timeout=10000');restored.prepare('UPDATE mx_drive_versions SET size=? WHERE item_uid=?').run(small.size,small.uid);}finally{restored.close();}
 const checks={eight_identical_pdf_requests:true,original_revision_unchanged:true,oversized_metadata_rejected_before_n1:true};
 if(process.env.MX_TEST_WORKBOOK){
  const book=await upload(user,'Complex workbook.xlsx',await readFile(process.env.MX_TEST_WORKBOOK));const resource=await call(`/mx/v1/drive/items/${book.uid}/ticket`,user,'POST',{});
  const response=await mx.api.get(`${resource.url}?office=true`);assert.equal(response.status(),422);assert.match((await response.json()).response,/too complex/);checks.complex_workbook_rejected_without_conversion=true;
 }
 assert.deepEqual(mx.n1.failures,[]);
 await writeFile(join(mx.directory,'office-preview-results.json'),JSON.stringify({passed:true,checks},null,2));console.log(JSON.stringify(checks));
}finally{await mx.close();}
