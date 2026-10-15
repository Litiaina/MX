// In-place widening with 800 real imported records, never deployment data.
import assert from 'node:assert/strict';
import { randomUUID,createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join,isAbsolute } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { DatabaseSync } from 'node:sqlite';
import { chromium,firefox,webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';
assert.ok(isAbsolute(process.env.MX_TEST_ARTIFACTS||''));assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url))),browsers=[],results={api:[],browsers:[]};let owner,activePage;
const call=async(path,method='GET',data,status=[200])=>(await mx.call(path,owner?.access_token,method,data,status)).body;
function snapshot(){const db=new DatabaseSync(join(mx.directory,'test.db'),{readOnly:true}),result={};for(const table of ['mx_records','mx_record_values','mx_record_versions']){const hash=createHash('sha256');let count=0;for(const row of db.prepare(`SELECT * FROM ${table} ORDER BY rowid`).iterate()){hash.update(JSON.stringify(row));count++;}result[table]={count,hash:hash.digest('hex')};}db.close();return result;}
try{
  console.log(`Field datatype artifacts: ${mx.directory}`);
  const account={email:'datatype@test.invalid',name:'Datatype test',password:`Test-only!${randomUUID()}`};
  await mx.call('/mx/v1/auth/create',mx.signupKey,'POST',account,[201]);owner=(await mx.call('/mx/v1/auth/authenticate',null,'POST',account)).body;
  const module=(await call('/mx/v1/admin/modules','POST',{name:'Imported data',singular_name:'Entry'},[201])).module;
  const field=async(key,label,type='text',extra={})=>call(`/mx/v1/admin/modules/${module.uid}/fields`,'POST',{key,label,field_type:type,...extra},[201]);
  const title=await field('title','Title','text',{required:true,unique_value:true});const note=await field('note','Note');
  const csv='Title,Note\n'+Array.from({length:800},(_,index)=>`Imported-${String(index).padStart(4,'0')},"Original note ${index} with unicode café and two lines\nKept exactly"\n`).join('');
  const base=`/mx/v1/modules/${module.uid}/imports`,upload=await mx.api.post(base,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{job_uid:randomUUID(),delimiter:'comma',file:{name:'800-records.csv',mimeType:'text/csv',buffer:Buffer.from(csv)}}});assert.equal(upload.status(),200);
  let job=await upload.json();job=await call(`${base}/${job.uid}/configure`,'PUT',{sheets:[{index:0,header_row:1,first_data_row:2,columns:[{column:1,key:'title',date_format:'',fill_down:false},{column:2,key:'note',date_format:'',fill_down:false}]}]});
  while(job.pending)job=await call(`${base}/${job.uid}/process`,'POST');assert.equal(job.imported,800);assert.equal(job.failed,0);
  const before=snapshot();assert.equal(before.mx_records.count,800);
  const schemaBefore=await call(`/mx/v1/modules/${module.uid}/schema`);
  const converted=await call(`/mx/v1/admin/schema/fields/${note.uid}`,'PUT',{field_type:'long_text',base_field_type:'text'});
  assert.equal(converted.uid,note.uid);assert.equal(converted.key,'note');assert.equal(converted.field_type,'long_text');assert.equal(converted.sortable,false);
  assert.deepEqual(snapshot(),before,'All 800 records, values, revisions and history unchanged by widening');
  assert.equal((await call(`/mx/v1/modules/${module.uid}/schema`)).revision,schemaBefore.revision+1);
  await call(`/mx/v1/admin/schema/fields/${note.uid}`,'PUT',{field_type:'integer'},[400]);
  await call(`/mx/v1/admin/schema/fields/${note.uid}`,'PUT',{field_type:'text'},[400]);
  await call(`/mx/v1/admin/schema/fields/${note.uid}`,'PUT',{field_type:'not_a_type'},[400]);
  await call(`/mx/v1/admin/schema/fields/${note.uid}`,'PUT',{field_type:'long_text',base_field_type:'text',label:'Stale label'},[409]);
  assert.deepEqual(snapshot(),before);
  // Eight simultaneous conversions share the same base datatype; one commits.
  const race=await field('race','Race');
  const writes=await Promise.all(Array.from({length:8},()=>mx.call(`/mx/v1/admin/schema/fields/${race.uid}`,owner.access_token,'PUT',{field_type:'long_text',base_field_type:'text'},[200,409])));
  assert.equal(writes.filter(result=>result.status===200).length,1);assert.equal(writes.filter(result=>result.status===409).length,7);
  assert.deepEqual(snapshot(),before);
  await call(`/mx/v1/admin/schema/fields/${title.uid}`,'PUT',{field_type:'long_text',base_field_type:'text'});
  await call(`/mx/v1/modules/${module.uid}/records`,'POST',{values:{title:'Imported-0000'}},[409,400]);assert.deepEqual(snapshot(),before,'Unique values remain enforced');
  results.api.push('800 imported records preserved byte-for-byte in SQLite values/history/revisions; stable field ID/key; schema revision advances once; sorting disabled; narrowing/unsafe types refused; stale base rejected; eight conversions one winner; uniqueness retained');
  for(const engine of(process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
    const current=await field(`ui_${engine}`,`UI note ${engine}`);
    const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1440,height:950}});await context.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
    const page=await context.newPage();activePage=page;const errors=[];page.on('pageerror',error=>errors.push(error.message));
    const records=await page.context().newPage();await records.goto(`https://127.0.0.1:${mx.port}/#module/${module.uid}`);await records.locator('summary').filter({hasText:'Advanced filters and sorting'}).click();await records.getByRole('combobox',{name:'Sort field',exact:true}).selectOption(current.key);await records.getByRole('button',{name:'Apply filters',exact:true}).click();
    await page.goto(`https://127.0.0.1:${mx.port}/#admin`);await page.getByRole('button',{name:'Modules & fields',exact:true}).click();await page.getByRole('combobox',{name:'Change module',exact:true}).selectOption(module.uid);
    const card=page.locator(`[data-schema-field="${current.uid}"]`);await card.getByRole('combobox',{name:`Datatype for ${current.label}`,exact:true}).selectOption('long_text');await card.getByRole('button',{name:'Save field',exact:true}).click();
    await page.getByRole('button',{name:'Change datatype',exact:true}).click();await page.waitForFunction(uid=>document.querySelector(`[data-schema-field="${uid}"] select`)?.disabled===true,current.uid);
    const stored=(await call(`/mx/v1/modules/${module.uid}/schema`)).fields.find(field=>field.uid===current.uid);assert.equal(stored.field_type,'long_text');assert.equal(stored.key,current.key);assert.deepEqual(snapshot(),before);
    await records.waitForFunction(()=>document.querySelector('.records-result-summary')?.textContent?.includes('800'));await records.getByRole('button',{name:'Refresh',exact:true}).click();await records.waitForFunction(()=>document.querySelector('select')!==null);
    await records.waitForFunction(()=>[...document.querySelectorAll('label')].find(label=>label.textContent.trim().startsWith('Sort field'))?.querySelector('select')?.value==='');
    await records.getByRole('button',{name:'Edit Entry 1',exact:true}).click();const editor=records.locator('.record-dialog');await editor.waitFor();const textarea=editor.getByRole('textbox',{name:'Note',exact:true});assert.equal(await textarea.evaluate(node=>node.tagName),'TEXTAREA');assert.ok((await textarea.inputValue()).includes('unicode café and two lines\nKept exactly'));await editor.getByRole('button',{name:'Cancel',exact:true}).click();await editor.waitFor({state:'hidden'});
    await page.screenshot({path:join(mx.directory,`${engine}-datatype-converted.png`)});assert.deepEqual(errors,[]);results.browsers.push(engine);console.log(`${engine}: in-place datatype UI and stale sort recovery passed`);await browser.close();
  }
  await writeFile(join(mx.directory,'field-type-change-results.json'),JSON.stringify({passed:true,...results},null,2));
}catch(error){if(activePage&&!activePage.isClosed())await activePage.screenshot({path:join(mx.directory,'field-type-change-failure.png')}).catch(()=>{});throw error;}
finally{await Promise.all(browsers.map(browser=>browser.close().catch(()=>{})));await mx.close();}
