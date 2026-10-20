// Real records/schema/auth/SQLite; isolated N1 fixture, no deployment data/audio.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { chromium,firefox,webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
if(process.env.MX_TEST_N1_URL)throw Error('Column summary checks require the isolated storage fixture.');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url))),browsers=[],results=[];
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
async function until(check,label){const deadline=Date.now()+20000;while(Date.now()<deadline){if(await check())return;await new Promise(r=>setTimeout(r,100));}throw Error(label);}
const env={...process.env,PULSE_SERVER:`unix:${mx.directory}/no-audio.sock`,PIPEWIRE_REMOTE:'mx_summaries_disconnected',ALSA_CONFIG_PATH:fileURLToPath(new URL('./alsa-null.conf',import.meta.url)),SDL_AUDIODRIVER:'dummy',ALSOFT_DRIVERS:'null'};
let inspected;
try{
 console.log(`Column summary artifacts: ${mx.directory}`);
 const password=`Test-only!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@test.invalid',name:'Owner',password},[201]);
 const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@test.invalid',password}),users=[];
 for(let i=0;i<8;i++){await call('/mx/v1/user/create',owner,'POST',{email:`editor${i}@test.invalid`,name:`Editor ${i}`,password,access_level:2},[201]);users.push(await call('/mx/v1/auth/authenticate',null,'POST',{email:`editor${i}@test.invalid`,password}));}
 const module=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Cash flow',singular_name:'Entry'},[201])).module;
 const root=`/mx/v1/modules/${module.uid}/records`;
 const field=async(key,label,type,config={})=>call(`/mx/v1/admin/modules/${module.uid}/fields`,owner,'POST',{key,label,field_type:type,searchable:true,table_visible:true,config},[201]);
 const tenant=await field('tenant','Tenant','text');
 const amount=await field('amount','Amount','decimal',{table_summary:'sum',summary_decimals:2});
 await field('units','Units','integer',{table_summary:'average',summary_decimals:2});
 const income=await field('income','Income','formula',{expression:'amount * units',decimals:2,table_summary:'sum',summary_decimals:2});
 await field('serial','Entry number','auto_number',{table_summary:'count'});
 for(const user of users.slice(0,7)){const access=await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner);await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner,'PUT',{revision:access.revision,grants:[{module_uid:module.uid,can_read:true,can_create:true,can_update:true,can_delete:false,can_configure:false,can_report:false,can_attachments:false}]});}
 for(let start=0;start<120;start+=8)await Promise.all(Array.from({length:Math.min(8,120-start)},(_,n)=>{const i=start+n;return call(root,owner,'POST',{operation_uid:randomUUID(),values:{tenant:`${i%2?'Odd':'Even'} tenant ${i}`,amount:i===119?null:i*1.25-50,units:i%3+1}},[201]);}));
 const full=await call(`${root}?limit=256`,owner),sum=(data,key)=>data.reduce((v,r)=>v+Number(r.values[key]||0),0);
 assert.equal(full.total,120);assert.equal(full.summaries.amount.value,sum(full.data,'amount'));assert.equal(full.summaries.amount.count,119);assert.equal(full.summaries.serial.value,120);assert.equal(full.summaries.income.value,sum(full.data,'income'));
 const paged=await Promise.all([1,2,3].map(page=>call(`${root}?limit=50&page=${page}`,owner)));
 assert.deepEqual(paged.map(p=>p.data.length),[50,50,20]);for(const p of paged)assert.deepEqual(p.summaries,full.summaries);
 const filtered=await call(`${root}?limit=25&filters=${encodeURIComponent(JSON.stringify({tenant:'Even'}))}`,owner),allFiltered=await call(`${root}?limit=256&q=Even`,owner);
 assert.equal(filtered.total,60);assert.equal(filtered.summaries.amount.value,sum(allFiltered.data,'amount'));
 const empty=await call(`${root}?q=does-not-exist`,owner);assert.equal(empty.total,0);assert.equal(empty.summaries.amount.value,0);assert.equal(empty.summaries.serial.value,0);assert.equal(empty.summaries.units.value,null);
 await call(`${root}?attachments=with`,users[0],'GET',undefined,[400]);
 await call(root,users[7],'GET',undefined,[403]);
 await call(`/mx/v1/admin/schema/fields/${tenant.uid}`,owner,'PUT',{config:{table_summary:'sum'}},[400]);
 await call(`/mx/v1/admin/schema/fields/${amount.uid}`,owner,'PUT',{config:{table_summary:'sum',summary_decimals:-1}},[400]);
 await call(`/mx/v1/admin/schema/fields/${amount.uid}`,users[0],'PUT',{config:{table_summary:'sum'}},[403]);
 const deleted=full.data[0];await call(`${root}/${deleted.uid}`,owner,'DELETE',undefined,[204]);
 let current=await call(`${root}?limit=256`,owner);assert.equal(current.total,119);assert.equal(current.summaries.amount.value,sum(current.data,'amount'));
 await call(`/mx/v1/admin/trash/${deleted.uid}/restore`,owner,'POST');
 await call(`/mx/v1/admin/schema/fields/${income.uid}`,owner,'PUT',{config:{expression:'amount * units * 2',decimals:2,table_summary:'sum',summary_decimals:2}});
 current=await call(`${root}?limit=256`,owner);assert.equal(current.summaries.income.value,sum(current.data,'income'));
 const writers=users.slice(0,7).map((user,i)=>call(`${root}/${current.data[i].uid}`,user,'PATCH',{base_revision:current.data[i].revision,changes:{amount:i+100}}));
 const reads=Array.from({length:8},async()=>{const page=await call(`${root}?limit=256`,owner);assert.equal(page.summaries.amount.value,sum(page.data,'amount'));assert.equal(page.summaries.income.value,sum(page.data,'income'));assert.equal(page.total,page.data.length);});
 await Promise.all([...writers,...reads]);
 results.push({api:'120 records, all-pages totals, filters, empties, nullable/negative/zero values, delete/restore, current formulas, ACL',concurrency:'eight simultaneous readers during seven writer commits; each response is a consistent row/count/summary snapshot'});
 for(const engine of(process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
  const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
  const browser=await({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env,...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
  const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1440,height:1000},locale:'en-US'});
  await context.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
  const page=await context.newPage();inspected=page;const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(`https://127.0.0.1:${mx.port}/#admin`);await page.getByRole('button',{name:'Modules & fields',exact:true}).click();await page.getByRole('button',{name:`Configure ${module.name}`,exact:true}).click();
  await page.getByRole('navigation',{name:'Module configuration sections',exact:true}).getByRole('button',{name:'Column totals',exact:true}).click();
  await page.getByRole('combobox',{name:'Summary for Amount',exact:true}).selectOption('sum');
  await page.getByRole('spinbutton',{name:'Summary decimals for Amount',exact:true}).fill('2');
  await page.getByRole('button',{name:'Save summary for Amount',exact:true}).click();await page.getByRole('status').filter({hasText:'Amount summary saved.'}).waitFor();
  await page.getByRole('navigation',{name:'Main navigation'}).getByRole('button',{name:'Cash flow',exact:true}).click();
  await page.getByRole('combobox',{name:'Record navigation',exact:true}).selectOption({label:'Pages'});
  const footer=page.locator('tfoot'),value=footer.getByLabel('Amount summary',{exact:true});
  const data=await call(`${root}?limit=256`,owner),formatted=data.summaries.amount.value.toLocaleString('en-US',{minimumFractionDigits:2,maximumFractionDigits:2});
  await until(()=>value.innerText().then(text=>text===formatted),'all matching sum footer');
  await page.getByRole('button',{name:'Next',exact:true}).click();await until(()=>page.getByRole('spinbutton',{name:'Page',exact:true}).inputValue().then(v=>v==='2'),'second page');assert.equal(await value.innerText(),formatted);
  await page.getByRole('searchbox',{name:'Search cash flow',exact:true}).fill('Even');await page.getByRole('button',{name:'Search',exact:true}).click();
  const even=await call(`${root}?q=Even&limit=256`,owner),evenText=even.summaries.amount.value.toLocaleString('en-US',{minimumFractionDigits:2,maximumFractionDigits:2});
  await until(()=>value.innerText().then(text=>text===evenText),'filtered total');
  const nextAmount=engine==='chromium'?250:engine==='firefox'?260:270;
  await page.getByRole('button',{name:'Edit Entry 1',exact:true}).click();
  const recordForm=page.getByRole('dialog',{name:'Edit Entry',exact:true});
  await recordForm.getByRole('spinbutton',{name:'Amount',exact:true}).fill(String(nextAmount));
  await recordForm.getByRole('button',{name:'Update Entry',exact:true}).click();await recordForm.waitFor({state:'hidden'});
  const afterEdit=await call(`${root}?q=Even&limit=256`,owner);
  const display=n=>n.toLocaleString('en-US',{minimumFractionDigits:2,maximumFractionDigits:2});
  await until(async()=>{const actual=await call(`${root}?q=Even&limit=256`,owner);return actual.data[0].values.amount===nextAmount&&await value.innerText()===display(actual.summaries.amount.value);},'full record edit refreshes filtered totals');
  const removed=afterEdit.data[0];await call(`${root}/${removed.uid}`,owner,'DELETE',undefined,[204]);
  const afterDelete=await call(`${root}?q=Even&limit=256`,owner);
  await until(()=>value.innerText().then(text=>text===display(afterDelete.summaries.amount.value)),'live deletion refreshes totals');
  await call(`/mx/v1/admin/trash/${removed.uid}/restore`,owner,'POST');
  const afterRestore=await call(`${root}?q=Even&limit=256`,owner);
  await until(()=>value.innerText().then(text=>text===display(afterRestore.summaries.amount.value)),'live restoration refreshes totals');
  for(const [theme,width]of[['light',1440],['dark',1440],['dark',390]]){
    await page.evaluate(t=>document.documentElement.dataset.theme=t,theme);await page.setViewportSize({width,height:1000});if(width===390)await page.waitForFunction(()=>document.querySelector('.sidebar').getBoundingClientRect().right<=0);await footer.scrollIntoViewIfNeeded();
    assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));const box=await footer.boundingBox();assert.ok(box.x>=0&&box.x+box.width<=width+1);
    const visible=await footer.locator('.column-summary strong').evaluateAll(nodes=>nodes.map(node=>{const a=node.getBoundingClientRect(),b=node.closest('td').getBoundingClientRect(),c=node.closest('tfoot').getBoundingClientRect();return{text:node.textContent,valueBottom:a.bottom,cellBottom:b.bottom,footerBottom:c.bottom,valueTop:a.top,cellTop:b.top,right:a.right,cellRight:b.right};}));
    for(const v of visible)assert.ok(v.valueBottom<=v.cellBottom+1&&v.valueBottom<=v.footerBottom+1&&v.valueTop>=v.cellTop&&v.right<=v.cellRight+1,JSON.stringify(v));
    await page.screenshot({path:join(mx.directory,`${engine}-totals-${theme}-${width}.png`)});
  }
  await page.setViewportSize({width:1440,height:1000});await page.getByRole('searchbox',{name:'Search cash flow',exact:true}).fill('does-not-exist');await page.getByRole('button',{name:'Search',exact:true}).click();await until(()=>value.innerText().then(text=>text==='0.00'),'empty filtered total');
  assert.deepEqual(errors,[]);results.push({engine,ui:'admin configuration, aligned footer, pagination/filter updates, full-record edit/live delete/restore, empty totals, light/dark/mobile'});console.log(JSON.stringify(results.at(-1)));await browser.close();
 }
}catch(error){process.exitCode=1;results.push({failure:error.stack});if(inspected&&!inspected.isClosed())await inspected.screenshot({path:join(mx.directory,'totals-failure.png')}).catch(()=>{});}
finally{for(const b of browsers)await b.close().catch(()=>{});await writeFile(join(mx.directory,'record-summaries-results.json'),JSON.stringify({passed:!process.exitCode,results},null,2));console.log(JSON.stringify({passed:!process.exitCode,results,artifacts:mx.directory}));await mx.close();}
