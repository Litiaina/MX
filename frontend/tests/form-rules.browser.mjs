// Isolated real MX/SQLite + fixture N1. No deployment data or hardware audio.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
if(process.env.MX_TEST_N1_URL)throw Error('Form tests require isolated fixture N1.');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const browsers=[],results={api:[],browsers:{}};
const until=async(check,label)=>{const end=Date.now()+20000;while(Date.now()<end){if(await check())return;await new Promise(r=>setTimeout(r,80));}throw Error(label);};
const password=`Test-only!${randomUUID()}`;let owner;
const call=async(path,method='GET',data,expected=[200],user=owner)=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const when=(field,value)=>({match:'all',conditions:[{field,operator:'eq',value}]});
try{
  console.log(`Form rule artifacts: ${mx.directory}`);
  await call('/mx/v1/auth/create','POST',{email:'owner@rules.test',name:'Owner',password},[201],{access_token:mx.signupKey});
  owner=await call('/mx/v1/auth/authenticate','POST',{email:'owner@rules.test',password},[200],null);
  const users=[];for(let i=0;i<8;i++){const email=`editor${i}@rules.test`;await call('/mx/v1/user/create','POST',{email,name:`Editor ${i}`,password,access_level:2},[201]);users.push(await call('/mx/v1/auth/authenticate','POST',{email,password},[200],null));}
  const cases=(await call('/mx/v1/admin/modules','POST',{name:'Cases',singular_name:'Case'},[201])).module;
  const employees=(await call('/mx/v1/admin/modules','POST',{name:'Employees',singular_name:'Employee'},[201])).module;
  const field=(module,key,label,type,config={},required=false)=>call(`/mx/v1/admin/modules/${module}/fields`,'POST',{key,label,field_type:type,config,required},[201]);
  const reason=await field(cases.uid,'reason','Reason','text');
  const amount=await field(cases.uid,'amount','Amount','decimal');
  const code=await field(cases.uid,'code','Code','text');
  const paid=await field(cases.uid,'paid','Paid','boolean');
  const classification=await field(cases.uid,'classification','Classification','select',{options:['JO','REG']});
  const name=await field(employees.uid,'name','Name','text');await field(employees.uid,'class','Class','text');
  const employee=await field(cases.uid,'employee','Employee','relationship',{target_module_uid:employees.uid,label_field:'name',auto_fill:[{source_field:'class',target_field:'classification'}]});
  const typeConfig={options:['off','leave'],presentation:'checkbox',option_details:[{value:'off',label:'Off duty',fill:{code:'OFF',paid:false}},{value:'leave',label:'Leave',fill:{code:'LV',paid:true}}]};
  const type=await field(cases.uid,'request_type','Request type','select',typeConfig,true);
  const subConfig={options:['annual','unpaid','other'],option_details:[{value:'annual',label:'Annual leave',available_when:when('classification','REG')},{value:'unpaid',label:'Unpaid leave',available_when:when('classification','JO')}],form_rules:{visible_when:when('request_type','leave')}};
  const subtype=await field(cases.uid,'subtype','Leave category','select',subConfig);
  const extra=await field(cases.uid,'extra','Extra details','text',{form_rules:{visible_when:{match:'all',conditions:[{field:'request_type',operator:'eq',value:'leave'}],groups:[{match:'any',conditions:[{field:'classification',operator:'eq',value:'REG'},{field:'amount',operator:'gt',value:100}]}]}}});
  const fileConfig={multiple:true,storage_name:'files',form_rules:{visible_when:when('request_type','leave')}};
  const files=await field(cases.uid,'files','Files','attachments',fileConfig);
  await field(cases.uid,'threshold','Threshold','integer');await field(cases.uid,'threshold_detail','Threshold details','text',{form_rules:{visible_when:{match:'all',conditions:[{field:'threshold',operator:'gt',value:0}]}}});
  for(let i=0;i<8;i++)await field(cases.uid,`note${i}`,`Note ${i}`,'text');
  await call(`/mx/v1/admin/schema/fields/${reason.uid}`,'PUT',{config:{form_rules:{visible_when:when('request_type','leave'),required_when:when('request_type','leave')}}});
  await call(`/mx/v1/admin/schema/fields/${amount.uid}`,'PUT',{config:{form_rules:{visible_when:when('request_type','leave'),required_when:when('request_type','leave')}}});
  await call(`/mx/v1/admin/schema/fields/${code.uid}`,'PUT',{config:{form_rules:{enabled_when:when('request_type','leave')}}});
  const create=(module,values,user=owner)=>call(`/mx/v1/modules/${module}/records`,'POST',{operation_uid:randomUUID(),values},[201],user);
  const alice=await create(employees.uid,{name:'Alice',class:'REG'}),bob=await create(employees.uid,{name:'Bob',class:'JO'});
  for(const user of users){const grant=await call(`/mx/v1/admin/accounts/${user.uid}/modules`);await call(`/mx/v1/admin/accounts/${user.uid}/modules`,'PUT',{revision:grant.revision,grants:[cases,employees].map(m=>({module_uid:m.uid,can_read:true,can_create:true,can_update:true,can_delete:false,can_configure:false,can_report:true,can_attachments:true}))});}
  const path=uid=>`/mx/v1/modules/${cases.uid}/records/${uid}`;
  let r=await create(cases.uid,{employee:alice.uid,request_type:'leave',reason:'Annual vacation',amount:0,subtype:'annual'});
  assert.equal(r.values.classification,'REG');assert.equal(r.values.code,'LV');assert.equal(r.values.paid,true);assert.equal(r.values.amount,0);
  const uploaded=await mx.api.post(`/mx/v1/records/${r.uid}/attachments/fields/${files.uid}`,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{base_revision:String(r.revision),files:{name:'rules-fixture.txt',mimeType:'text/plain',buffer:Buffer.from('Retained branch attachment')}}});assert.equal(uploaded.status(),201,await uploaded.text());r=await call(path(r.uid));assert.equal(r.attached_files.length,1);
  const leaveRevision=r.revision,history=await call(`${path(r.uid)}/versions`),leaveVersion=history.versions[0].uid;
  r=await call(path(r.uid),'PATCH',{base_revision:r.revision,changes:{request_type:'off'}});assert.equal(r.values.reason??null,null);assert.equal(r.values.amount??null,null);assert.equal(r.values.subtype??null,null);assert.equal(r.values.code,'OFF');assert.equal(r.values.paid,false);assert.equal(r.attached_files.length,1);
  assert.equal((await call(`${path(r.uid)}/versions/${leaveVersion}`)).values.reason,'Annual vacation');
  for(const changes of [{reason:'bypass'},{code:'bypass'},{request_type:'leave'},{subtype:'annual'}])await call(path(r.uid),'PATCH',{base_revision:r.revision,changes},[400]);
  await call(`/mx/v1/modules/${cases.uid}/records`,'POST',{values:{request_type:'off',reason:'hidden'}},[400]);
  const denied=await mx.api.post(`/mx/v1/records/${r.uid}/attachments/fields/${files.uid}`,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{base_revision:String(r.revision),files:{name:'denied.txt',mimeType:'text/plain',buffer:Buffer.from('Do not upload')}}});assert.equal(denied.status(),400,await denied.text());assert.ok((await denied.json()).response.includes('not applicable'));
  await call(`${path(r.uid)}/versions/${leaveVersion}`,'POST');r=await call(path(r.uid));assert.equal(r.values.reason,'Annual vacation');assert.equal(r.attached_files.length,1);
  r=await call(path(r.uid),'PATCH',{base_revision:r.revision,changes:{employee:bob.uid}});assert.equal(r.values.classification,'JO');assert.equal(r.values.subtype??null,null,'A parent auto-fill invalidates a dependent choice');
  await call(path(r.uid),'PATCH',{base_revision:r.revision,changes:{subtype:'annual'}},[400]);
  const stale=r.revision;r=await call(path(r.uid),'PATCH',{base_revision:r.revision,changes:{classification:'REG'}});
  await call(path(r.uid),'PATCH',{base_revision:stale,changes:{subtype:'unpaid'}},[409]);
  const staleFile=await mx.api.post(`/mx/v1/records/${r.uid}/attachments/fields/${files.uid}`,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{base_revision:String(leaveRevision),files:{name:'stale.txt',mimeType:'text/plain',buffer:Buffer.from('Stale controller')}}});assert.equal(staleFile.status(),400,await staleFile.text());
  results.api.push('Relationship-driven conditions; nested groups; multiple option copies; typed zero/false; conditional requirements, hidden/disabled writes, dependent options and uploads enforced; branch clears preserve files/history; stale controller denial; normal restore');

  const base=r.revision;await Promise.all(users.map((user,i)=>call(path(r.uid),'PATCH',{base_revision:base,changes:{[`note${i}`]:`Eight writer ${i}`}},[200],user)));r=await call(path(r.uid));assert.equal(r.revision,base+8);for(let i=0;i<8;i++)assert.equal(r.values[`note${i}`],`Eight writer ${i}`);
  const clearBase=r.revision;r=await call(path(r.uid),'PATCH',{base_revision:clearBase,changes:{reason:'Someone else edited the detail'}},[200],users[0]);
  const conflict=await call(path(r.uid),'PATCH',{base_revision:clearBase,changes:{request_type:'off'}},[409],users[1]);const detailConflict=conflict.conflicts.find(f=>f.field_key==='reason');assert.ok(detailConflict);assert.equal(detailConflict.current_value,'Someone else edited the detail');assert.equal(detailConflict.your_value,null);assert.equal(detailConflict.related_values.request_type,'leave');assert.equal((await call(path(r.uid))).values.request_type,'leave');
  const raceBase=r.revision;const race=await Promise.all(users.map(user=>mx.call(path(r.uid),user.access_token,'PATCH',{base_revision:raceBase,changes:{request_type:'off'}},[200,409])));assert.equal(race.filter(a=>a.status===200).length,1);assert.equal(race.filter(a=>a.status===409).length,7);r=await call(path(r.uid));
  await call(path(r.uid),'PUT',{base_revision:r.revision,values:{employee:alice.uid,request_type:'leave',reason:'Full replacement',amount:1,classification:'REG',code:'LV',paid:true}});r=await call(path(r.uid));assert.equal(r.values.reason,'Full replacement');
  const versionBeforeRule=(await call(`${path(r.uid)}/versions`)).versions[0].uid;
  await call(`/mx/v1/admin/schema/fields/${extra.uid}`,'PUT',{config:{form_rules:{required_when:when('request_type','leave')}}});
  await call(`${path(r.uid)}/versions/${versionBeforeRule}`,'POST',undefined,[409]);
  await call(`/mx/v1/admin/schema/fields/${extra.uid}`,'PUT',{config:{form_rules:{visible_when:when('request_type','leave')}}});
  results.api.push('Eight-account unrelated merges; stale detail cannot be cleared; branch conflict includes recoverable current controls; one winner/seven conflicts; PUT rules and old-version/current-rule rejection');
  // Hold a real multipart storage part after admission, switch the branch,
  // then ensure final metadata admission checks the new controller revision.
  const uploadRace=await create(cases.uid,{request_type:'leave',reason:'Upload race',amount:0});
  const objectsBefore=mx.n1.objects.size;let releasePart;const partGate=new Promise(resolve=>releasePart=resolve);let held=false;
  mx.n1.control.beforePart=async()=>{held=true;await partGate;};
  const pendingUpload=mx.api.post(`/mx/v1/records/${uploadRace.uid}/attachments/fields/${files.uid}`,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{base_revision:String(uploadRace.revision),files:{name:'race.bin',mimeType:'application/octet-stream',buffer:Buffer.alloc(17*1024*1024,7)}}});
  try{await until(()=>held,'Multipart reached storage');await call(path(uploadRace.uid),'PATCH',{base_revision:uploadRace.revision,changes:{request_type:'off'}});}finally{releasePart();mx.n1.control.beforePart=undefined;}
  const racedUpload=await pendingUpload;assert.equal(racedUpload.status(),409,await racedUpload.text());assert.ok((await racedUpload.json()).response.includes('Files is not applicable'));assert.equal((await call(path(uploadRace.uid))).attached_files.length,0);assert.equal(mx.n1.objects.size,objectsBefore,'Rejected upload cleans only its own storage object');assert.deepEqual(mx.n1.failures,[]);
  results.api.push('Real 17 MiB multipart upload held in N1 fixture; branch switches during transfer; final transaction rejects stale admission and cleans its own orphan, with existing files retained');

  const bad=await call(`/mx/v1/admin/schema/fields/${type.uid}`,'PUT',{config:{...typeConfig,form_rules:{visible_when:when('code','LV')}}},[400]);assert.ok(bad.response.includes('Circular'));
  await call(`/mx/v1/admin/schema/fields/${reason.uid}`,'PUT',{config:{form_rules:{visible_when:when('missing','x')}}},[400]);
  await call(`/mx/v1/admin/schema/fields/${files.uid}`,'PUT',{config:{...fileConfig,form_rules:{required_when:when('request_type','leave')}}},[400]);
  const blocks=await call(`/mx/v1/admin/schema/fields/${type.uid}/archive-check`);assert.ok(blocks.blockers.some(b=>b.kind==='form_rule'&&b.field_uid===reason.uid));await call(`/mx/v1/admin/schema/fields/${type.uid}`,'PUT',{active:false},[400]);
  assert.ok((await call(`/mx/v1/admin/schema/fields/${code.uid}/archive-check`)).blockers.some(b=>b.kind==='form_rule'));
  await call(`/mx/v1/admin/schema/fields/${reason.uid}`,'PUT',{config:{}},[403],users[0]);
  const checks=(await call('/mx/v1/admin/modules','POST',{name:'Equipment',singular_name:'Equipment'},[201])).module;
  const group={form_rules:{exclusive_group:'Delivery method',exclusive_required:true}};
  await field(checks.uid,'collect','Collect','boolean',group);await field(checks.uid,'deliver','Deliver','boolean',group);
  const address=await field(checks.uid,'address','Address','text',{form_rules:{visible_when:when('deliver',true),required_when:when('deliver',true)}});
  let e=await create(checks.uid,{collect:true});e=await call(`/mx/v1/modules/${checks.uid}/records/${e.uid}`,'PATCH',{base_revision:e.revision,changes:{deliver:true,address:'Office'}});assert.equal(e.values.collect,false);assert.equal(e.values.deliver,true);
  await call(`/mx/v1/modules/${checks.uid}/records/${e.uid}`,'PATCH',{base_revision:e.revision,changes:{deliver:false}},[400]);
  await call(`/mx/v1/modules/${checks.uid}/records`,'POST',{values:{collect:true,deliver:true}},[400]);
  results.api.push('No attendance hardcoding: second module Boolean group; configuration cycles/missing sources/file requirements denied; named archive dependencies and administrator-only writes');

  for(const engine of (process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
    await call(`/mx/v1/admin/schema/fields/${type.uid}`,'PUT',{config:typeConfig});
    const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await ({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:{...process.env,PULSE_SERVER:`unix:${mx.directory}/no-audio.sock`,PIPEWIRE_REMOTE:'mx_rules_disconnected',ALSA_CONFIG_PATH:fileURLToPath(new URL('./alsa-null.conf',import.meta.url)),SDL_AUDIODRIVER:'dummy',ALSOFT_DRIVERS:'null'},...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1440,height:950}}),errors=[];page.on('pageerror',err=>errors.push(err.message));
    await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
    const origin=`https://127.0.0.1:${mx.port}`;
    await page.goto(`${origin}/#module/${cases.uid}`);await page.getByRole('button',{name:'+ New case',exact:true}).click();const choiceDialog=page.locator('.record-dialog');await choiceDialog.getByRole('checkbox',{name:'Off duty',exact:true}).check();await choiceDialog.getByRole('checkbox',{name:'Leave',exact:true}).check();assert.equal(await choiceDialog.getByRole('checkbox',{name:'Off duty',exact:true}).isChecked(),false);assert.equal(await choiceDialog.getByRole('checkbox',{name:'Leave',exact:true}).isChecked(),true);await choiceDialog.getByRole('button',{name:'Clear form',exact:true}).click();await choiceDialog.getByRole('button',{name:'Cancel',exact:true}).click();
    await page.goto(`${origin}/#admin`);await page.getByRole('button',{name:'Modules & fields',exact:true}).click();await page.getByRole('button',{name:`Configure ${cases.name}`,exact:true}).click();
    await page.getByRole('navigation',{name:'Module configuration sections',exact:true}).getByRole('button',{name:'Form rules',exact:true}).click();
    await page.getByRole('button',{name:'Configure rules for Request type',exact:true}).click();
    const ruleDialog=page.getByRole('dialog',{name:'Form rules · Request type',exact:true});await ruleDialog.waitFor();assert.ok(await ruleDialog.evaluate(node=>node.contains(document.activeElement)));
    await ruleDialog.getByLabel('Show field when',{exact:true}).check();await ruleDialog.getByRole('button',{name:'Add condition',exact:true}).click();await ruleDialog.getByRole('combobox',{name:'Control field',exact:true}).selectOption('code');await ruleDialog.getByRole('textbox',{name:'Value',exact:true}).fill('LV');await ruleDialog.getByRole('button',{name:'Save form rules',exact:true}).click();
    await ruleDialog.getByRole('alert').waitFor();assert.ok((await ruleDialog.getByRole('alert').innerText()).includes('Circular'));assert.equal(await page.locator('.notice.error').evaluateAll(nodes=>nodes.filter(n=>!n.closest('.rules-dialog')).length),0);
    await ruleDialog.getByLabel('Show field when',{exact:true}).uncheck();
    await ruleDialog.getByRole('combobox',{name:'Display as',exact:true}).selectOption('radio');
    const off=ruleDialog.locator('.option-rule').filter({hasText:'Stored value: off'});await off.locator('summary').click();await off.getByRole('textbox',{name:'Display label',exact:true}).fill('Off duty');
    await off.getByRole('combobox',{name:'Add destination for off',exact:true}).selectOption('note7');await off.getByRole('textbox',{name:'Note 7',exact:true}).fill('Configured option value');
    const leave=ruleDialog.locator('.option-rule').filter({hasText:'Stored value: leave'});await leave.locator('summary').click();await leave.getByRole('combobox',{name:'Add destination for leave',exact:true}).selectOption('note7');await leave.getByRole('textbox',{name:'Note 7',exact:true}).fill('Configured option value');
    for(const [theme,width] of [['light',1440],['dark',1440],['dark',390]]){await page.evaluate(v=>document.documentElement.dataset.theme=v,theme);await page.setViewportSize({width,height:850});assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));const box=await ruleDialog.boundingBox();assert.ok(box.x>=7&&box.x+box.width<=width-7&&box.y>=7&&box.y+box.height<=843);await page.screenshot({path:join(mx.directory,`${engine}-rules-${theme}-${width}.png`)});}
    await page.setViewportSize({width:1440,height:950});await ruleDialog.getByRole('button',{name:'Save form rules',exact:true}).click();await ruleDialog.waitFor({state:'hidden'});
    const s=await call(`/mx/v1/modules/${cases.uid}/schema`);assert.equal(s.fields.find(f=>f.key==='request_type').config.presentation,'radio');
    await page.goto(`${origin}/#module/${cases.uid}`);await page.getByRole('button',{name:'+ New case',exact:true}).click();const dialog=page.locator('.record-dialog');
    await dialog.getByRole('radio',{name:'Off duty',exact:true}).click();assert.equal(await dialog.getByRole('textbox',{name:'Code',exact:true}).inputValue(),'OFF');assert.equal(await dialog.getByRole('textbox',{name:'Code',exact:true}).isDisabled(),true);assert.equal(await dialog.getByRole('textbox',{name:/^Reason/}).count(),0);
    await dialog.getByRole('radio',{name:'Leave',exact:true}).check();await dialog.getByRole('textbox',{name:/^Reason/}).fill(`UI ${engine}`);await dialog.getByRole('spinbutton',{name:/^Amount/}).fill('0');
    assert.equal(await dialog.getByRole('textbox',{name:'Note 7',exact:true}).inputValue(),'Configured option value');
    const choose=dialog.getByRole('combobox',{name:'Choose Employee',exact:true});await until(async()=>await choose.locator(`option[value="${alice.uid}"]`).count()===1,'Relationship loaded');await choose.selectOption(alice.uid);await until(async()=>await dialog.getByRole('combobox',{name:'Classification',exact:true}).inputValue()==='REG','Linked class filled');
    const category=dialog.getByRole('combobox',{name:'Leave category',exact:true});assert.equal(await category.locator('option[value="annual"]').count(),1);assert.equal(await category.locator('option[value="unpaid"]').count(),0);await category.selectOption('annual');
    await dialog.getByRole('radio',{name:'Off duty',exact:true}).click();const confirm=page.getByRole('alertdialog',{name:'Clear details from the previous choice?',exact:true});await confirm.waitFor();await confirm.getByRole('button',{name:'Cancel',exact:true}).click();assert.equal(await dialog.getByRole('radio',{name:'Leave',exact:true}).isChecked(),true);assert.equal(await dialog.getByRole('textbox',{name:/^Reason/}).inputValue(),`UI ${engine}`);
    await dialog.getByRole('button',{name:'Save Case',exact:true}).click();await dialog.waitFor({state:'hidden'});const list=await call(`/mx/v1/modules/${cases.uid}/records?q=${encodeURIComponent(`UI ${engine}`)}`);const saved=list.data.find(a=>a.values.reason===`UI ${engine}`);assert.ok(saved);assert.equal(saved.values.amount,0);
    await page.getByRole('searchbox',{name:'Search cases',exact:true}).fill(`UI ${engine}`);await page.getByRole('button',{name:'Search',exact:true}).click();await page.getByRole('button',{name:'Edit Case 1',exact:true}).click();await dialog.waitFor();
    await dialog.getByRole('radio',{name:'Off duty',exact:true}).click();await confirm.waitFor();await confirm.getByRole('button',{name:'Switch and clear',exact:true}).click();assert.equal(await dialog.getByRole('textbox',{name:/^Reason/}).count(),0);await dialog.getByRole('button',{name:'Update Case',exact:true}).click();await dialog.waitFor({state:'hidden'});assert.equal((await call(path(saved.uid))).values.reason??null,null);
    await page.getByRole('searchbox',{name:'Search cases',exact:true}).fill('');await page.getByRole('button',{name:'Search',exact:true}).click();await until(async()=>await page.getByRole('button',{name:'Edit Case 1',exact:true}).count()===1,'Rows refreshed');
    await page.getByRole('button',{name:'Edit Case 1',exact:true}).click();await dialog.waitFor();await dialog.getByRole('radio',{name:'Leave',exact:true}).check();await dialog.getByRole('textbox',{name:/^Reason/}).fill(`Inline ${engine}`);await dialog.getByRole('spinbutton',{name:/^Amount/}).fill('2');await dialog.getByRole('button',{name:'Update Case',exact:true}).click();await dialog.waitFor({state:'hidden'});
    assert.equal((await call(path(saved.uid))).values.reason,`Inline ${engine}`);
    await page.getByRole('button',{name:'Edit Case 1',exact:true}).click();await dialog.getByRole('textbox',{name:'Note 0',exact:true}).fill(`Local preserved ${engine}`);await dialog.getByRole('radio',{name:'Off duty',exact:true}).click();await confirm.getByRole('button',{name:'Switch and clear',exact:true}).click();
    const concurrent=await call(path(saved.uid));await call(path(saved.uid),'PATCH',{base_revision:concurrent.revision,changes:{reason:`Remote ${engine}`}},[200],users[0]);
    await dialog.getByRole('button',{name:'Update Case',exact:true}).click();const reasonConflict=dialog.locator('.conflict-row').filter({hasText:'Reason'});await reasonConflict.waitFor();await reasonConflict.getByRole('button',{name:'Use latest',exact:true}).click();assert.equal(await dialog.getByRole('radio',{name:'Leave',exact:true}).isChecked(),true);assert.equal(await dialog.getByRole('textbox',{name:/^Reason/}).inputValue(),`Remote ${engine}`);assert.equal(await dialog.getByRole('textbox',{name:'Note 0',exact:true}).inputValue(),`Local preserved ${engine}`);await dialog.getByRole('button',{name:'Update Case',exact:true}).click();await dialog.waitFor({state:'hidden'});assert.equal((await call(path(saved.uid))).values.note0,`Local preserved ${engine}`);
    const numeric=await create(cases.uid,{request_type:'off',threshold:1,threshold_detail:'Retain until confirmed',note5:`Numeric ${engine}`});await page.getByRole('searchbox',{name:'Search cases',exact:true}).fill(`Numeric ${engine}`);await page.getByRole('button',{name:'Search',exact:true}).click();await page.getByRole('button',{name:'Edit Case 1',exact:true}).click();await dialog.getByRole('spinbutton',{name:'Threshold',exact:true}).fill('0');assert.equal(await dialog.getByRole('textbox',{name:'Threshold details',exact:true}).count(),0);await dialog.getByRole('button',{name:'Update Case',exact:true}).click();await confirm.waitFor();assert.ok((await confirm.innerText()).includes('Threshold details'));await confirm.getByRole('button',{name:'Cancel',exact:true}).click();assert.equal((await call(path(numeric.uid))).values.threshold_detail,'Retain until confirmed');await dialog.getByRole('button',{name:'Update Case',exact:true}).click();await confirm.getByRole('button',{name:'Switch and clear',exact:true}).click();await dialog.waitFor({state:'hidden'});assert.equal((await call(path(numeric.uid))).values.threshold_detail??null,null);
    await page.goto(`${origin}/#module/${checks.uid}`);await page.getByRole('button',{name:'+ New equipment',exact:true}).click();await dialog.getByRole('checkbox',{name:'Collect',exact:true}).check();await dialog.getByRole('checkbox',{name:'Deliver',exact:true}).check();assert.equal(await dialog.getByRole('checkbox',{name:'Collect',exact:true}).isChecked(),false);await dialog.getByRole('textbox',{name:/^Address/}).fill('Office');await dialog.getByRole('button',{name:'Save Equipment',exact:true}).click();await dialog.waitFor({state:'hidden'});
    assert.deepEqual(errors,[]);results.browsers[engine]='Focused rule builder, named cycle error inside modal, radio/checkbox choices, immediate multi-cell fill, relationship-driven options, conditional required details, cancel/confirm clear, zero-value save, full-form branch completion, Boolean groups, light/dark/390px';console.log(`${engine}: form rules passed`);await page.close();await browser.close();
  }
  await writeFile(join(mx.directory,'form-rules-results.json'),JSON.stringify(results,null,2));console.log(JSON.stringify(results,null,2));
}catch(error){for(const [i,browser]of browsers.entries())for(const context of browser.contexts())for(const page of context.pages())await page.screenshot({path:join(mx.directory,`failure-${i}.png`)}).catch(()=>{});throw error;}
finally{await Promise.all(browsers.map(b=>b.close().catch(()=>{})));await mx.close();}
