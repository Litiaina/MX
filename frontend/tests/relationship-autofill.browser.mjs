// Disposable real MX HTTP/SQLite + production UI. No deployment data/live N1.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';

if(process.env.MX_TEST_N1_URL)throw Error('Use the isolated fixture, not live N1.');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const browsers=[];const results={api:[],browsers:{}};
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const until=async(check,label)=>{const end=Date.now()+25000;while(Date.now()<end){if(await check())return;await new Promise(resolve=>setTimeout(resolve,80));}throw Error(label);};
const password=`Test-only!${randomUUID()}`;
let owner;
try{
  console.log(`Auto-fill artifacts: ${mx.directory}`);
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@autofill.test',name:'Owner',password},[201]);
  owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@autofill.test',password});
  const users=[];
  for(let i=0;i<8;i++){const email=`editor${i}@autofill.test`;await call('/mx/v1/user/create',owner,'POST',{email,name:`Editor ${i}`,password,access_level:2},[201]);users.push(await call('/mx/v1/auth/authenticate',null,'POST',{email,password}));}
  const employees=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Employees',singular_name:'Employee'},[201])).module;
  const inventory=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Inventory',singular_name:'Inventory'},[201])).module;
  const field=async(module,key,label,type,config={},required=false)=>call(`/mx/v1/admin/modules/${module}/fields`,owner,'POST',{key,label,field_type:type,config,required},[201]);
  const sourceName=await field(employees.uid,'name','Name','text');
  const sourceClass=await field(employees.uid,'class','Classification','text');
  await field(employees.uid,'department','Department','text');
  await field(employees.uid,'rate','Rate','decimal');
  await field(employees.uid,'flag','Enabled','boolean');
  await field(employees.uid,'joined','Joined','date');
  await field(employees.uid,'number','Number','integer');
  const sourceFormula=await field(employees.uid,'gross','Gross','formula',{expression:'rate * 2',decimals:2});
  const destination=await field(inventory.uid,'classification','Classification','select',{options:['JO','REG']},true);
  await field(inventory.uid,'department','Department','text');
  await field(inventory.uid,'stock','Stock used','text');
  for(let i=0;i<8;i++)await field(inventory.uid,`note${i}`,`Note ${i}`,'text');
  const mappings=[{source_field:'class',target_field:'classification'},{source_field:'department',target_field:'department'}];
  const relation=await field(inventory.uid,'issuer','Issuer','relationship',{target_module_uid:employees.uid,label_field:'name',auto_fill:mappings});
  await field(inventory.uid,'live_class','Current classification','lookup',{relationship_field:'issuer',target_field:'class'});
  const config={...relation.config,auto_fill:mappings};
  const setConfig=auto_fill=>call(`/mx/v1/admin/schema/fields/${relation.uid}`,owner,'PUT',{config:{...config,auto_fill}});
  for(const user of users){const access=await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner);await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner,'PUT',{revision:access.revision,grants:[employees,inventory].map(m=>({module_uid:m.uid,can_read:true,can_create:true,can_update:true,can_delete:false,can_configure:false,can_report:true,can_attachments:true}))});}
  const create=(module,values)=>call(`/mx/v1/modules/${module}/records`,owner,'POST',{operation_uid:randomUUID(),values},[201]);
  let a=await create(employees.uid,{name:'Alice',class:'JO',department:'HR',rate:0,flag:false,joined:'2026-10-13',number:0});
  const b=await create(employees.uid,{name:'Bob',class:'REG',department:'IT',rate:123.5});
  const empty=await create(employees.uid,{name:'Missing classification'});
  const invalid=await create(employees.uid,{name:'Invalid classification',class:'OTHER',department:'Legal'});
  const preview=(value,user=owner)=>call(`/mx/v1/relationships/${relation.uid}/preview`,user,'POST',{value});
  const typed=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Typed copies',singular_name:'Typed copy'},[201])).module;
  for(const [key,type] of [['rate','decimal'],['flag','boolean'],['joined','date'],['number','integer'],['gross','decimal']])await field(typed.uid,key,key,type);
  const typedRelation=await field(typed.uid,'employee','Employee','relationship',{target_module_uid:employees.uid,label_field:'name',auto_fill:['rate','flag','joined','number','gross'].map(key=>({source_field:key,target_field:key}))});
  const typedRecord=await create(typed.uid,{employee:a.uid});
  assert.equal(typedRecord.values.rate,0);assert.equal(typedRecord.values.flag,false);assert.equal(typedRecord.values.joined,'2026-10-13');assert.equal(typedRecord.values.number,0);assert.equal(typedRecord.values.gross,0);
  await call(`/mx/v1/admin/schema/fields/${sourceFormula.uid}`,owner,'PUT',{config:{expression:'rate + 10',decimals:2}});
  const typedPreview=await call(`/mx/v1/relationships/${typedRelation.uid}/preview`,owner,'POST',{value:a.uid});assert.equal(typedPreview.values.gross,10);
  assert.equal((await call(`/mx/v1/modules/${typed.uid}/records/${typedRecord.uid}`,owner)).values.gross,0,'Copy does not follow changed formula');
  assert.deepEqual(await preview(a.uid),{values:{classification:'JO',department:'HR'},computed:{live_class:['JO']}});
  assert.deepEqual(await preview(null),{values:{classification:null,department:null},computed:{live_class:[]}});
  assert.equal((await preview(empty.uid)).values.classification,null);
  await call(`/mx/v1/relationships/${relation.uid}/preview`,owner,'POST',{value:invalid.uid},[400]);
  const corrected=await create(inventory.uid,{issuer:invalid.uid,classification:'REG'});
  assert.equal(corrected.values.classification,'REG');assert.equal(corrected.values.department,'Legal','Explicit override skips invalid copied source while other mappings still fill');
  await call(`/mx/v1/relationships/${relation.uid}/preview`,owner,'POST',{value:[a.uid]},[400]);
  await call(`/mx/v1/relationships/${relation.uid}/preview`,owner,'POST',{value:randomUUID()},[400]);
  let record=await create(inventory.uid,{issuer:a.uid,stock:'Medicine'});
  assert.equal(record.values.classification,'JO');assert.equal(record.values.department,'HR');
  const alien=await create(inventory.uid,{classification:'REG'});
  await call(`/mx/v1/relationships/${relation.uid}/preview`,owner,'POST',{value:alien.uid},[400]);
  for(const bad of [
    [{source_field:'rate',target_field:'classification'}],
    [{source_field:'class',target_field:'missing'}],
    [{source_field:'missing',target_field:'classification'}],
    [{source_field:'class',target_field:'classification'},{source_field:'department',target_field:'classification'}],
    [{source_field:'class',target_field:'live_class'}],
  ])await call(`/mx/v1/admin/schema/fields/${relation.uid}`,owner,'PUT',{config:{...config,auto_fill:bad}},[400]);
  await call(`/mx/v1/admin/schema/fields/${relation.uid}`,owner,'PUT',{config:{...config,multiple:true}},[400]);
  await call(`/mx/v1/admin/modules/${inventory.uid}/fields`,owner,'POST',{key:'another',label:'Another relationship',field_type:'relationship',config},[400]);
  for(const blocked of [sourceName,sourceClass,destination])await call(`/mx/v1/admin/schema/fields/${blocked.uid}`,owner,'PUT',{active:false},[400]);
  await call(`/mx/v1/modules/${inventory.uid}/records`,owner,'POST',{values:{issuer:empty.uid}},[400]);
  results.api.push('Typed mappings, required destination, missing values, invalid/foreign selections and schema dependencies');
  a=await call(`/mx/v1/modules/${employees.uid}/records/${a.uid}`,owner,'PATCH',{base_revision:a.revision,changes:{class:'REG'}});
  assert.equal((await call(`/mx/v1/modules/${inventory.uid}/records/${record.uid}`,owner)).values.classification,'JO','Saved copies stay historical');
  assert.equal((await preview(a.uid)).values.classification,'REG');
  a=await call(`/mx/v1/modules/${employees.uid}/records/${a.uid}`,owner,'PATCH',{base_revision:a.revision,changes:{class:'JO'}});
  const base=record.revision;
  await Promise.all(users.map((user,i)=>call(`/mx/v1/modules/${inventory.uid}/records/${record.uid}`,user,'PATCH',{base_revision:base,changes:{[`note${i}`]:`Editor ${i}`}})));
  record=await call(`/mx/v1/modules/${inventory.uid}/records/${record.uid}`,owner);
  assert.equal(record.revision,base+8);
  for(let i=0;i<8;i++)assert.equal(record.values[`note${i}`],`Editor ${i}`);
  const collisions=await Promise.all(users.map(user=>mx.call(`/mx/v1/modules/${inventory.uid}/records/${record.uid}`,user.access_token,'PATCH',{base_revision:record.revision,changes:{issuer:b.uid}},[200,409])));
  assert.equal(collisions.filter(r=>r.status===200).length,1);
  assert.equal(collisions.filter(r=>r.status===409).length,7);
  for(const result of collisions.filter(r=>r.status===409))assert.ok(result.body.conflicts.some(f=>f.field_key==='classification'),'Copied destination also receives conflict checks');
  record=await call(`/mx/v1/modules/${inventory.uid}/records/${record.uid}`,owner);
  assert.equal(record.values.classification,'REG');assert.equal(record.values.department,'IT');
  const versions=await call(`/mx/v1/modules/${inventory.uid}/records/${record.uid}/versions`,owner);
  const version=await call(`/mx/v1/modules/${inventory.uid}/records/${record.uid}/versions/${versions.versions[0].uid}`,owner);
  assert.equal(version.values.classification,'REG');
  record=await call(`/mx/v1/modules/${inventory.uid}/records/${record.uid}`,owner,'PATCH',{base_revision:record.revision,changes:{issuer:a.uid,classification:'REG'}});
  assert.equal(record.values.classification,'REG','Explicit destination override wins');assert.equal(record.values.department,'HR');
  const grant=await call(`/mx/v1/admin/accounts/${users[7].uid}/modules`,owner);
  await call(`/mx/v1/admin/accounts/${users[7].uid}/modules`,owner,'PUT',{revision:grant.revision,grants:[{module_uid:inventory.uid,can_read:true,can_create:true,can_update:true,can_delete:false,can_configure:false,can_report:true,can_attachments:true}]});
  await call(`/mx/v1/relationships/${relation.uid}/preview`,users[7],'POST',{value:a.uid},[403]);
  await call(`/mx/v1/modules/${inventory.uid}/records/${record.uid}`,users[7],'PATCH',{base_revision:record.revision,changes:{issuer:b.uid}},[400]);
  await call(`/mx/v1/relationships/${relation.uid}/preview`,null,'POST',{value:a.uid},[401]);
  results.api.push('Eight-account independent-field merging; one winner/seven conflicts on relationship and copied values; normal version history; explicit overrides; access denial');

  for(const engine of (process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
    await setConfig(mappings.slice(0,1));
    const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await ({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:{...process.env,PULSE_SERVER:`unix:${mx.directory}/no-audio.sock`,PIPEWIRE_REMOTE:'mx_autofill_disconnected',ALSA_CONFIG_PATH:fileURLToPath(new URL('./alsa-null.conf',import.meta.url)),SDL_AUDIODRIVER:'dummy',ALSOFT_DRIVERS:'null'},...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1440,height:950}});const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
    const origin=`https://127.0.0.1:${mx.port}`;
    await page.goto(`${origin}/#admin`);await page.getByRole('button',{name:'Modules & fields',exact:true}).click();await page.getByRole('button',{name:`Configure ${inventory.name}`,exact:true}).click();
    const card=page.locator('.schema-card').filter({has:page.locator('.schema-field-title strong').filter({hasText:/^Issuer$/})});
    await card.getByRole('button',{name:'Add field mapping',exact:true}).click();
    await card.getByRole('combobox',{name:'Linked source field 2',exact:true}).selectOption('department');
    await card.getByRole('combobox',{name:'Fill this field 2',exact:true}).selectOption('department');
    await card.getByRole('button',{name:'Save field',exact:true}).click();
    await until(async()=>{const schema=await call(`/mx/v1/modules/${inventory.uid}/schema`,owner);return schema.fields.find(f=>f.key==='issuer').config.auto_fill.length===2;},'Admin mapping persisted');
    await page.goto(`${origin}/#module/${inventory.uid}`);
    await page.getByRole('button',{name:'+ New inventory',exact:true}).click();
    const dialog=page.getByRole('dialog').filter({has:page.getByRole('group',{name:'Issuer',exact:true})});
    const choose=dialog.getByRole('combobox',{name:'Choose Issuer',exact:true});
    const classification=dialog.getByRole('combobox',{name:/^Classification/});
    const department=dialog.getByRole('textbox',{name:'Department',exact:true});
    const lookup=dialog.getByRole('textbox',{name:/^Current classification/});
    const select=async uid=>{await until(async()=>await choose.locator(`option[value="${uid}"]`).count()===1,'Name selectable');await choose.selectOption(uid);};
    await select(a.uid);await until(async()=>await classification.inputValue()==='JO'&&await department.inputValue()==='HR','Name fills both destinations immediately');
    assert.equal(await lookup.inputValue(),'JO');
    await select(b.uid);await until(async()=>await classification.inputValue()==='REG'&&await department.inputValue()==='IT','Changing name replaces both destinations');
    await dialog.getByRole('button',{name:'Remove Bob',exact:true}).click();
    await until(async()=>await classification.inputValue()===''&&await department.inputValue()==='','Clearing name clears copies');
    // Delay the real Alice response, then choose Bob. Obsolete Alice must not win.
    let release;const gate=new Promise(resolve=>release=resolve);let held=false,heldDone=false;
    const route=async request=>{if(request.request().postDataJSON().value===a.uid&&!held){held=true;const response=await request.fetch();await gate;try{await request.fulfill({response});}catch{}finally{heldDone=true;}}else await request.continue();};
    await page.route('**/relationships/*/preview',route);
    await select(a.uid);await until(()=>held,'Alice preview intercepted');
    await select(b.uid);await until(async()=>await department.inputValue()==='IT','Newest response applied');release();
    await until(()=>heldDone,'Obsolete response returned');
    await page.unroute('**/relationships/*/preview',route);
    await until(async()=>!(await dialog.getByRole('button',{name:'Save Inventory',exact:true}).isDisabled()),'Latest preview settled');
    assert.equal(await classification.inputValue(),'REG');assert.equal(await department.inputValue(),'IT');
    // Typing an override while the source request is pending must survive it.
    let releaseTyped;const typedGate=new Promise(resolve=>releaseTyped=resolve);let typedHeld=false;
    const typedRoute=async request=>{typedHeld=true;const response=await request.fetch();await typedGate;await request.fulfill({response});};
    await page.route('**/relationships/*/preview',typedRoute);await select(a.uid);await until(()=>typedHeld,'Override preview intercepted');
    await department.fill('User override');releaseTyped();
    await until(async()=>!(await dialog.getByRole('button',{name:'Save Inventory',exact:true}).isDisabled()),'Override preview settled');
    assert.equal(await department.inputValue(),'User override');assert.equal(await classification.inputValue(),'JO');
    await page.unroute('**/relationships/*/preview',typedRoute);
    await dialog.getByRole('button',{name:'Cancel',exact:true}).click();await dialog.waitFor({state:'hidden'});
    await page.getByRole('button',{name:'+ New inventory',exact:true}).click();
    await until(async()=>await lookup.inputValue()==='JO'&&!(await dialog.getByRole('button',{name:'Save Inventory',exact:true}).isDisabled()),'Restored draft preview settled');
    assert.equal(await department.inputValue(),'User override','Reopening a draft preserves copied-value overrides');
    // Network failure is explicit and cannot commit partial automatic values.
    const failRoute=route=>route.fulfill({status:400,contentType:'application/json',body:JSON.stringify({response:'Test interrupted linked lookup'})});
    await page.route('**/relationships/*/preview',failRoute);await select(b.uid);
    await dialog.getByRole('button',{name:'Retry linked fields',exact:true}).waitFor();
    assert.equal(await dialog.getByRole('button',{name:'Save Inventory',exact:true}).isDisabled(),true);
    await page.unroute('**/relationships/*/preview',failRoute);
    await dialog.getByRole('button',{name:'Cancel',exact:true}).click();await dialog.waitFor({state:'hidden'});
    await page.getByRole('button',{name:'+ New inventory',exact:true}).click();
    await dialog.getByRole('button',{name:'Retry linked fields',exact:true}).waitFor();
    assert.equal(await dialog.getByRole('button',{name:'Save Inventory',exact:true}).isDisabled(),true,'Reopening an interrupted draft cannot save stale automatic values');
    await dialog.getByRole('button',{name:'Retry linked fields',exact:true}).click();await until(async()=>await department.inputValue()==='IT','Retry fills latest selection');
    // Reset during a held response must not refill a cleared form afterwards.
    let releaseReset;const resetGate=new Promise(resolve=>releaseReset=resolve);let resetHeld=false,resetDone=false;
    const resetRoute=async request=>{resetHeld=true;const response=await request.fetch();await resetGate;try{await request.fulfill({response});}catch{}finally{resetDone=true;}};
    await page.route('**/relationships/*/preview',resetRoute);await select(a.uid);await until(()=>resetHeld,'Reset preview intercepted');
    await dialog.getByRole('button',{name:'Clear form',exact:true}).click();releaseReset();await page.unroute('**/relationships/*/preview',resetRoute);
    await until(()=>resetDone,'Reset obsolete response returned');
    assert.equal(await classification.inputValue(),'');assert.equal(await department.inputValue(),'');
    await select(a.uid);await until(async()=>await department.inputValue()==='HR','Selected after reset');
    await dialog.getByRole('textbox',{name:'Stock used',exact:true}).fill(`Browser ${engine}`);
    for(const [theme,width] of [['light',1440],['dark',1440],['dark',390]]){
      await page.evaluate(value=>document.documentElement.dataset.theme=value,theme);await page.setViewportSize({width,height:900});
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
      await page.screenshot({path:join(mx.directory,`${engine}-autofill-${theme}-${width}.png`)});
    }
    await page.setViewportSize({width:1440,height:950});await dialog.getByRole('button',{name:'Save Inventory',exact:true}).click();await dialog.waitFor({state:'hidden'});
    const found=await call(`/mx/v1/modules/${inventory.uid}/records?q=${encodeURIComponent(`Browser ${engine}`)}`,owner);const saved=found.data.find(r=>r.values.stock===`Browser ${engine}`);assert.ok(saved);
    assert.equal(saved.values.issuer,a.uid);assert.equal(saved.values.classification,'JO');assert.equal(saved.values.department,'HR');
    await page.getByRole('searchbox',{name:'Search inventory',exact:true}).fill(`Browser ${engine}`);await page.getByRole('button',{name:'Search',exact:true}).click();
    await page.getByRole('button',{name:'Edit Inventory 1',exact:true}).click();
    const fullChoice=dialog.getByRole('combobox',{name:'Choose Issuer',exact:true});await until(async()=>await fullChoice.locator(`option[value="${b.uid}"]`).count()===1,'Full form selectable');await fullChoice.selectOption(b.uid);
    await until(async()=>await classification.inputValue()==='REG','Full form fills classification before save');
    await dialog.getByRole('button',{name:'Update Inventory',exact:true}).click();await dialog.waitFor({state:'hidden'});await until(async()=>{const r=await call(`/mx/v1/modules/${inventory.uid}/records/${saved.uid}`,owner);return r.values.issuer===b.uid&&r.values.classification==='REG'&&r.values.department==='IT';},'Full form saves all mapped fields');
    await page.getByRole('button',{name:'Edit Inventory 1',exact:true}).click();
    await until(async()=>await fullChoice.locator(`option[value="${a.uid}"]`).count()===1,'Alice available');await fullChoice.selectOption(a.uid);
    await until(async()=>await dialog.getByRole('textbox',{name:'Department',exact:true}).inputValue()==='HR','Second full-form preview');
    const current=await call(`/mx/v1/modules/${inventory.uid}/records/${saved.uid}`,owner);
    await call(`/mx/v1/modules/${inventory.uid}/records/${saved.uid}`,users[0],'PATCH',{base_revision:current.revision,changes:{classification:'JO',department:'Elsewhere'}});
    await dialog.getByRole('button',{name:'Update Inventory',exact:true}).click();
    await until(async()=>await dialog.locator('.conflict-row').count()===2,'Each copied field has a visible conflict');
    await dialog.locator('.conflict-row').filter({hasText:'Classification'}).getByRole('button',{name:'Use latest',exact:true}).click();
    assert.equal(await dialog.locator('.conflict-row').count(),1,'Do not save until every copied conflict is resolved');
    await dialog.locator('.conflict-row').filter({hasText:'Department'}).getByRole('button',{name:'Keep mine',exact:true}).click();
    await dialog.getByRole('button',{name:'Update Inventory',exact:true}).click();await dialog.waitFor({state:'hidden'});
    await until(async()=>{const r=await call(`/mx/v1/modules/${inventory.uid}/records/${saved.uid}`,owner);return r.values.issuer===a.uid&&r.values.classification==='JO'&&r.values.department==='HR';},'Per-field conflict decisions saved atomically');
    assert.deepEqual(errors,[]);
    results.browsers[engine]='Admin mappings, immediate full-form fill, live lookup, change/clear, stale response cancellation, typing/draft override, failure/retry, reset, normal save, per-field conflicts, light/dark/mobile';
    console.log(`${engine}: auto-fill passed`);await page.close();await browser.close();
  }
  await writeFile(join(mx.directory,'relationship-autofill-results.json'),JSON.stringify(results,null,2));console.log(JSON.stringify(results,null,2));
}catch(error){for(const [i,browser] of browsers.entries())for(const context of browser.contexts())for(const page of context.pages())await page.screenshot({path:join(mx.directory,`failure-${i}.png`)}).catch(()=>{});throw error;}
finally{await Promise.all(browsers.map(browser=>browser.close().catch(()=>{})));await mx.close();}
