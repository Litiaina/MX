// Real MX HTTP/UI and SQLite; explicit N1 contract fixture.
// Eight-account concurrency and native module relationship validation.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {isolatedBrowserAudio} from './silent-audio.mjs';

const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const browsers=[];const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const schemaField=async(module,key,type,config={})=>await call(`/mx/v1/admin/modules/${module}/fields`,owner,'POST',{key,label:key,field_type:type,required:false,config},[201]);
let owner;const password=`Test-only!${randomUUID()}`;
const until=async(check,label)=>{const deadline=Date.now()+20000;while(Date.now()<deadline){if(await check())return;await new Promise(resolve=>setTimeout(resolve,100));}throw Error(label);};
try{
  console.log(`Relationship artifacts: ${mx.directory}`);
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@office.test',name:'Owner',password},[201]);
  owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@office.test',password});
  const users=[];for(let i=0;i<8;i++){const email=`editor${i}@office.test`;await call('/mx/v1/user/create',owner,'POST',{email,name:`Editor ${i}`,password,access_level:2},[201]);users.push(await call('/mx/v1/auth/authenticate',null,'POST',{email,password}));}
  const patients=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Patients',singular_name:'Patient'},[201])).module;
  const bills=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Billing',singular_name:'Bill'},[201])).module;
  await schemaField(patients.uid,'name','text');await schemaField(patients.uid,'amount','decimal');
  await schemaField(bills.uid,'name','text');const relation=await schemaField(bills.uid,'patients','relationship',{target_module_uid:patients.uid,label_field:'name',multiple:true});
  await schemaField(bills.uid,'names','lookup',{relationship_field:'patients',target_field:'name'});
  for(const fn of ['sum','count','average','min','max'])await schemaField(bills.uid,fn,'rollup',{relationship_field:'patients',target_field:'amount',function:fn});
  for(let i=0;i<8;i++)await schemaField(bills.uid,`note${i}`,'text');
  for(const user of users){const access=await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner);await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner,'PUT',{revision:access.revision,grants:[patients,bills].map(m=>({module_uid:m.uid,can_read:true,can_create:true,can_update:true,can_delete:false,can_configure:false,can_report:true,can_attachments:true}))});}
  const create=(module,values)=>call(`/mx/v1/modules/${module}/records`,owner,'POST',{operation_uid:randomUUID(),values},[201]);
  const a=await create(patients.uid,{name:'Alice',amount:100});const b=await create(patients.uid,{name:'Bob',amount:200});
  const charlie=await create(patients.uid,{name:'Charlie',amount:50});
  await Promise.all(Array.from({length:55},(_,i)=>create(patients.uid,{name:`Paging ${String(i).padStart(2,'0')}`,amount:i})));
  const first=await call(`/mx/v1/relationships/${relation.uid}/options?q=Paging&page=1`,owner);
  const second=await call(`/mx/v1/relationships/${relation.uid}/options?q=Paging&page=2`,owner);
  assert.equal(first.items.length,50);assert.equal(first.has_next,true);assert.equal(second.items.length,5);assert.equal(second.has_next,false);
  assert.equal(new Set([...first.items,...second.items].map(item=>item.uid)).size,55);
  let record=await create(bills.uid,{name:'Invoice',patients:[a.uid,b.uid]});
  assert.deepEqual(record.values.names.toSorted(),['Alice','Bob']);assert.equal(record.values.sum,300);assert.equal(record.values.count,2);assert.equal(record.values.average,150);assert.equal(record.values.min,100);assert.equal(record.values.max,200);
  const linked=await call(`/mx/v1/relationships/${relation.uid}/options?q=Alice`,users[0]);assert.equal(linked.items[0].uid,a.uid);
  const base=record.revision;
  await Promise.all(users.map((user,i)=>call(`/mx/v1/modules/${bills.uid}/records/${record.uid}`,user,'PATCH',{base_revision:base,changes:{[`note${i}`]:`Edit ${i}`}})));
  record=await call(`/mx/v1/modules/${bills.uid}/records/${record.uid}`,owner);assert.equal(record.revision,base+8);for(let i=0;i<8;i++)assert.equal(record.values[`note${i}`],`Edit ${i}`);
  const collisions=await Promise.all(users.map((user,i)=>mx.call(`/mx/v1/modules/${bills.uid}/records/${record.uid}`,user.access_token,'PATCH',{base_revision:record.revision,changes:{patients:i%2?[a.uid]:[b.uid]}},[200,409])));
  assert.equal(collisions.filter(r=>r.status===200).length,1);assert.equal(collisions.filter(r=>r.status===409).length,7);
  const current=await call(`/mx/v1/modules/${bills.uid}/records/${record.uid}`,owner);
  await call(`/mx/v1/modules/${bills.uid}/records/${record.uid}`,owner,'PATCH',{base_revision:current.revision,changes:{patients:[a.uid,b.uid]}});
  await call(`/mx/v1/modules/${patients.uid}/records/${a.uid}`,owner,'DELETE',undefined,[409]);
  const renamed=await call(`/mx/v1/modules/${patients.uid}/records/${a.uid}`,owner,'PATCH',{base_revision:a.revision,changes:{name:'Alice renamed',amount:125}});
  record=await call(`/mx/v1/modules/${bills.uid}/records/${record.uid}`,owner);assert.equal(record.values.sum,325);assert.deepEqual(record.values.names.toSorted(),['Alice renamed','Bob']);
  assert.ok((await call(`/mx/v1/relationships/${patients.uid}/${a.uid}/related`,owner)).items.some(r=>r.uid===record.uid));
  const patientFields=await call(`/mx/v1/modules/${patients.uid}/schema`,owner);
  const nameField=patientFields.fields.find(field=>field.key==='name');
  await call(`/mx/v1/admin/schema/fields/${nameField.uid}`,owner,'PUT',{active:false},[400]);
  await call(`/mx/v1/admin/schema/fields/${relation.uid}`,owner,'PUT',{active:false},[400]);
  const cleared=await create(bills.uid,{name:'Clear links',patients:[charlie.uid]});
  const empty=await call(`/mx/v1/modules/${bills.uid}/records/${cleared.uid}`,owner,'PATCH',{base_revision:cleared.revision,changes:{patients:[]}});
  assert.equal(empty.values.patients??null,null);assert.equal(empty.values.count,0);assert.equal(empty.values.sum,0);
  const alien=await create(bills.uid,{name:'Not a patient'});await call(`/mx/v1/modules/${bills.uid}/records/${record.uid}`,owner,'PATCH',{base_revision:record.revision,changes:{patients:[alien.uid]}},[400]);
  const account=await call(`/mx/v1/admin/accounts/${users[7].uid}/modules`,owner);await call(`/mx/v1/admin/accounts/${users[7].uid}/modules`,owner,'PUT',{revision:account.revision,grants:[{module_uid:bills.uid,can_read:true,can_create:true,can_update:true,can_delete:false,can_configure:false,can_report:true,can_attachments:true}]});
  const redacted=await call(`/mx/v1/modules/${bills.uid}/records/${record.uid}`,users[7]);assert.equal(redacted.values.patients,null);assert.equal(redacted.values.sum,null);assert.equal(redacted.values.names,null);assert.equal(redacted.relationships.patients.restricted,true);
  await call(`/mx/v1/relationships/${relation.uid}/options`,users[7],'GET',undefined,[403]);
  await call(`/mx/v1/modules/${bills.uid}/records/${record.uid}`,users[7],'PATCH',{base_revision:record.revision,changes:{patients:[a.uid]}},[400]);
  const csv=await mx.api.get(`/mx/v1/reports/export.csv?kind=records&module_uid=${bills.uid}`,{headers:{Authorization:`Bearer ${users[7].access_token}`}});assert.equal(csv.status(),200);const exported=await csv.text();assert.ok(!exported.includes(a.uid)&&!exported.includes(b.uid),'CSV never leaks inaccessible linked IDs');
  const versions=await call(`/mx/v1/modules/${bills.uid}/records/${record.uid}/versions`,users[7]);
  const snapshot=await call(`/mx/v1/modules/${bills.uid}/records/${record.uid}/versions/${versions.versions[0].uid}`,users[7]);assert.ok(!JSON.stringify(snapshot).includes(a.uid)&&!JSON.stringify(snapshot).includes(b.uid),'History redacts linked identifiers too');

  for(const engine of (process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
    const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await ({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:isolatedBrowserAudio(),...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
    await page.goto(`https://127.0.0.1:${mx.port}/#module/${bills.uid}`);
    await page.getByRole('searchbox',{name:'Search billing',exact:true}).fill('Invoice');await page.getByRole('button',{name:'Search',exact:true}).click();
    await page.getByRole('button',{name:'Edit patients for row 1',exact:true}).click();await page.getByRole('searchbox',{name:'Search patients',exact:true}).fill('Charlie');
    await until(async()=>await page.getByRole('combobox',{name:'Choose patients'}).locator('option').count()>1,'Relationship options available');
    await page.getByRole('combobox',{name:'Choose patients'}).selectOption(charlie.uid);
    await page.getByRole('button',{name:'Save link',exact:true}).click();
    await until(async()=>{const stored=await call(`/mx/v1/modules/${bills.uid}/records/${record.uid}`,owner);return stored.values.patients.includes(charlie.uid)&&stored.values.sum===375;},'Inline selection saved');
    await page.screenshot({path:join(mx.directory,`${engine}-relationship-inline.png`)});
    await page.getByRole('button',{name:'Edit Bill 1',exact:true}).click();
    await page.getByRole('group',{name:'patients',exact:true}).waitFor();
    for(const [theme,width] of [['light',1440],['dark',1440],['dark',390]]){await page.evaluate(value=>document.documentElement.dataset.theme=value,theme);await page.setViewportSize({width,height:850});assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));await page.screenshot({path:join(mx.directory,`${engine}-relationship-${theme}-${width}.png`)});}
    await page.goto(`https://127.0.0.1:${mx.port}/#admin`);await page.getByRole('button',{name:'Modules & fields',exact:true}).click();await page.getByRole('combobox',{name:'Change module',exact:true}).selectOption(bills.uid);
    const fieldForm=page.locator('.module-section-panel form').filter({has:page.getByRole('button',{name:'Add field to Billing',exact:true})});
    await fieldForm.getByLabel('Label',{exact:true}).fill(`UI link ${engine}`);await fieldForm.getByLabel('Key (optional override)',{exact:true}).fill(`ui_link_${engine}`);
    await fieldForm.getByRole('combobox',{name:'Type',exact:true}).selectOption('relationship');await fieldForm.getByRole('combobox',{name:'Linked module',exact:true}).selectOption(patients.uid);
    await until(async()=>await fieldForm.getByRole('combobox',{name:'Display field',exact:true}).locator('option').count()>1,'Admin target fields loaded');await fieldForm.getByRole('combobox',{name:'Display field',exact:true}).selectOption('name');
    await fieldForm.getByRole('button',{name:'Add field to Billing',exact:true}).click();await page.getByText('Field created.',{exact:true}).waitFor();
    assert.ok((await call(`/mx/v1/modules/${bills.uid}/schema`,owner)).fields.some(field=>field.key===`ui_link_${engine}`&&field.field_type==='relationship'));
    await page.screenshot({path:join(mx.directory,`${engine}-relationship-admin.png`)});
    const stored=await call(`/mx/v1/modules/${bills.uid}/records/${record.uid}`,owner);await call(`/mx/v1/modules/${bills.uid}/records/${record.uid}`,owner,'PATCH',{base_revision:stored.revision,changes:{patients:[a.uid,b.uid]}});
    assert.deepEqual(errors,[],`${engine} has no runtime UI errors`);await browser.close();console.log(`${engine}: relationship selectors, mobile/themes and administrator configuration passed`);
  }
  console.log('PASS: 8-user disjoint merge and same-field conflict, bounded selectors, lookups/rollups, history/CSV ACL redaction and deletion safety');
}catch(error){for(const browser of browsers)for(const context of browser.contexts())for(const page of context.pages())await page.screenshot({path:join(mx.directory,'failure.png')}).catch(()=>{});throw error;}
finally{for(const browser of browsers)await browser.close();await mx.close();}
