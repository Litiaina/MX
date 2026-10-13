// Real disposable MX/SQLite, fixture N1, production UI. Never deployment data.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {DatabaseSync} from 'node:sqlite';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';

if(process.env.MX_TEST_N1_URL)throw Error('Archive tests use fixture N1 only.');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const browsers=[];const results={api:[],browsers:{}};
const until=async(check,label)=>{const end=Date.now()+20000;while(Date.now()<end){if(await check())return;await new Promise(resolve=>setTimeout(resolve,80));}throw Error(label);};
const password=`Test-only!${randomUUID()}`;let owner;
const call=async(path,method='GET',data,expected=[200],user=owner)=>(await mx.call(path,user?.access_token,method,data,expected)).body;
try{
  console.log(`Archive dialog artifacts: ${mx.directory}`);
  await call('/mx/v1/auth/create','POST',{email:'owner@archive.test',name:'Owner',password},[201],{access_token:mx.signupKey});
  owner=await call('/mx/v1/auth/authenticate','POST',{email:'owner@archive.test',password},[200],null);
  await call('/mx/v1/user/create','POST',{email:'editor@archive.test',name:'Editor',password,access_level:2},[201]);
  const editor=await call('/mx/v1/auth/authenticate','POST',{email:'editor@archive.test',password},[200],null);
  const employees=(await call('/mx/v1/admin/modules','POST',{name:'Employees',singular_name:'Employee'},[201])).module;
  const inventory=(await call('/mx/v1/admin/modules','POST',{name:'Inventory',singular_name:'Inventory'},[201])).module;
  const other=(await call('/mx/v1/admin/modules','POST',{name:'Other',singular_name:'Other'},[201])).module;
  const field=async(module,key,label,type,config={})=>call(`/mx/v1/admin/modules/${module}/fields`,'POST',{key,label,field_type:type,config},[201]);
  const name=await field(employees.uid,'name','Employee name','text');
  const classification=await field(employees.uid,'classification','Classification','text');
  await field(other.uid,'name','Unrelated name','text');
  const copied=await field(inventory.uid,'classification','Classification','text');
  const issuer=await field(inventory.uid,'issuer','Issuer','relationship',{target_module_uid:employees.uid,label_field:'name',auto_fill:[{source_field:'classification',target_field:'classification'}]});
  const lookup=await field(inventory.uid,'position','Current classification','lookup',{relationship_field:'issuer',target_field:'classification'});
  await field(inventory.uid,'linked_count','Linked count','rollup',{relationship_field:'issuer',function:'count'});
  const free=await field(employees.uid,'notes','Notes','text');
  const amount=await field(employees.uid,'amount','Amount','decimal');
  const formula=await field(employees.uid,'total','Total','formula',{expression:'amount * 2',decimals:2});
  const create=(module,values)=>call(`/mx/v1/modules/${module}/records`,'POST',{operation_uid:randomUUID(),values},[201]);
  const employee=await create(employees.uid,{name:'Alice',classification:'JO',notes:'Preserved historical value',amount:10});
  const record=await create(inventory.uid,{issuer:employee.uid});
  const trashed=await create(inventory.uid,{issuer:employee.uid});await call(`/mx/v1/modules/${inventory.uid}/records/${trashed.uid}`,'DELETE',undefined,[204]);
  const preflight=uid=>call(`/mx/v1/admin/schema/fields/${uid}/archive-check`);
  const check=await preflight(classification.uid);assert.equal(check.can_archive,false);assert.equal(check.blockers.length,2);
  assert.ok(check.blockers.some(b=>b.field_uid===issuer.uid&&b.module_uid===inventory.uid&&b.kind==='auto_fill_source'));
  assert.ok(check.blockers.some(b=>b.field_uid===lookup.uid&&b.kind==='computed_source'));
  const rejected=await mx.call(`/mx/v1/admin/schema/fields/${classification.uid}`,owner.access_token,'PUT',{active:false},[400]);
  assert.equal(rejected.body.error,'field_archive_blocked');assert.ok(rejected.body.response.includes('Inventory → Issuer'));
  assert.equal(rejected.body.archive_check.blockers.length,2);
  assert.equal((await preflight(name.uid)).blockers[0].field_label,'Issuer');
  assert.equal((await preflight(copied.uid)).blockers[0].kind,'auto_fill_destination');
  const links=(await preflight(issuer.uid)).blockers.find(b=>b.kind==='stored_links');assert.equal(links.count,2);assert.ok(links.reason.includes('1 active and 1 trashed'));
  assert.ok((await preflight(amount.uid)).blockers.some(b=>b.kind==='formula_source'&&b.field_uid===formula.uid));
  const unchanged=await call(`/mx/v1/modules/${employees.uid}/schema`);const before=unchanged.revision;
  await Promise.all(Array.from({length:8},()=>preflight(name.uid)));assert.equal((await call(`/mx/v1/modules/${employees.uid}/schema`)).revision,before,'Checks do not change schema');
  await call(`/mx/v1/admin/schema/fields/${name.uid}/archive-check`,'GET',undefined,[403],editor);
  await call(`/mx/v1/admin/schema/fields/${name.uid}/archive-check`,'GET',undefined,[401],null);
  await call(`/mx/v1/admin/schema/fields/${randomUUID()}/archive-check`,'GET',undefined,[404]);
  assert.equal((await preflight(free.uid)).can_archive,true);
  await call(`/mx/v1/admin/schema/fields/${free.uid}`,'PUT',{active:false});assert.equal((await preflight(free.uid)).field.active,false);
  const database=new DatabaseSync(join(mx.directory,'test.db'),{readOnly:true});
  assert.equal(database.prepare('SELECT value_text FROM mx_record_values WHERE record_uid=? AND field_uid=?').get(employee.uid,free.uid).value_text,'Preserved historical value');database.close();
  const files=await field(inventory.uid,'files','Files','attachments',{multiple:true,storage_name:'files'});
  const upload=await mx.api.post(`/mx/v1/records/${record.uid}/attachments/fields/${files.uid}`,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{base_revision:String(record.revision),files:{name:'archive-fixture.txt',mimeType:'text/plain',buffer:Buffer.from('Preserved fixture file')}}});assert.equal(upload.status(),201);
  const fileCheck=await preflight(files.uid);assert.equal(fileCheck.blockers[0].kind,'attachment_files');assert.equal(fileCheck.blockers[0].count,1);
  const layout=await call(`/mx/v1/admin/modules/${employees.uid}/storage-layout`);
  await call(`/mx/v1/admin/modules/${employees.uid}/storage-layout`,'PUT',{folder_field_uids:[name.uid],file_prefix_field_uid:name.uid,base_revision:layout.revision});
  const storageCheck=await preflight(name.uid);assert.ok(storageCheck.blockers.some(b=>b.kind==='storage_folder'));assert.ok(storageCheck.blockers.some(b=>b.kind==='storage_prefix'));
  results.api.push('Named cross-module relationship/lookup/rollup/auto-fill/formula blockers; active/trashed links; real attachment and storage dependencies; eight read-only checks; permissions; safe archive preserves values');

  for(const engine of (process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
    const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await ({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:{...process.env,PULSE_SERVER:`unix:${mx.directory}/no-audio.sock`,PIPEWIRE_REMOTE:'mx_archive_disconnected',ALSA_CONFIG_PATH:fileURLToPath(new URL('./alsa-null.conf',import.meta.url)),SDL_AUDIODRIVER:'dummy',ALSOFT_DRIVERS:'null'},...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1440,height:950}});const errors=[];page.on('pageerror',e=>{errors.push(e.message);console.error(`${engine}: ${e.message}`);});
    await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
    await page.goto(`https://127.0.0.1:${mx.port}/#admin`);await page.getByRole('button',{name:'Modules & fields',exact:true}).click();
    const module=page.getByRole('combobox',{name:'Change module',exact:true});await module.selectOption(employees.uid);
    const sourceCard=page.locator(`[data-schema-field="${classification.uid}"]`);
    await sourceCard.getByRole('button',{name:'Archive',exact:true}).click();
    const dialog=page.getByRole('dialog',{name:'Cannot archive this field',exact:true});await dialog.waitFor();
    assert.ok((await dialog.innerText()).includes('Inventory → Issuer'));assert.ok((await dialog.innerText()).includes('Inventory → Current classification'));
    assert.equal(await dialog.getByRole('button',{name:'Archive field',exact:true}).isDisabled(),true);
    assert.ok(await dialog.evaluate(node=>node.contains(document.activeElement)),'Keyboard focus moves into the dialog');
    await dialog.locator('button:enabled').last().focus();await page.keyboard.press('Tab');assert.ok(await dialog.locator('button:enabled').first().evaluate(node=>node===document.activeElement),'Tab remains in the popup');
    await page.keyboard.press('Shift+Tab');assert.ok(await dialog.locator('button:enabled').last().evaluate(node=>node===document.activeElement),'Reverse Tab remains in the popup');
    for(const [theme,width] of [['light',1440],['dark',1440],['dark',390]]){
      await page.evaluate(v=>document.documentElement.dataset.theme=v,theme);await page.setViewportSize({width,height:850});
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
      const bounds=await dialog.boundingBox();assert.ok(bounds.x>=7&&bounds.y>=7&&bounds.x+bounds.width<=width-7&&bounds.y+bounds.height<=843,'Modal fits viewport with balanced margins');
      await page.screenshot({path:join(mx.directory,`${engine}-archive-${theme}-${width}.png`)});
    }
    await page.setViewportSize({width:1440,height:950});
    await dialog.locator('.archive-blockers li').filter({hasText:'Inventory → Issuer'}).getByRole('button',{name:'Open field settings',exact:true}).click();await dialog.waitFor({state:'hidden'});
    await until(async()=>await module.inputValue()===inventory.uid,'Jump to owning module');
    const target=page.locator(`[data-schema-field="${issuer.uid}"]`);await until(()=>target.evaluate(node=>node===document.activeElement),'Jump focuses the exact field');
    assert.ok(await target.evaluate(node=>node.classList.contains('dependency-focus')));
    await page.screenshot({path:join(mx.directory,`${engine}-archive-target.png`)});
    // A server error remains inside the dialog, with a retry action.
    await module.selectOption(employees.uid);
    const failed=route=>route.fulfill({status:400,contentType:'application/json',body:JSON.stringify({response:'Test interrupted archive check'})});
    await page.route('**/schema/fields/*/archive-check',failed);
    await sourceCard.getByRole('button',{name:'Archive',exact:true}).click();
    const anyDialog=page.locator('.field-archive-dialog');await anyDialog.getByRole('alert').waitFor();assert.ok((await anyDialog.getByRole('alert').innerText()).includes('Test interrupted'));
    assert.equal(await page.locator('.notice.error').evaluateAll(nodes=>nodes.filter(node=>!node.closest('.field-archive-dialog')).length),0,'Error is not placed above the page');
    await page.unroute('**/schema/fields/*/archive-check',failed);await anyDialog.getByRole('button',{name:'Check again',exact:true}).click();await dialog.waitFor();
    await page.keyboard.press('Escape');await anyDialog.waitFor({state:'hidden'});
    assert.ok(await sourceCard.getByRole('button',{name:'Archive',exact:true}).evaluate(node=>node===document.activeElement),'Escape restores opener focus');
    // Remove both references intentionally through their settings API, then retry.
    const safe=await field(employees.uid,`safe_${engine}`,`Safe ${engine}`,'text');
    const safeCard=page.locator(`[data-schema-field="${safe.uid}"]`);
    // Load newly created fields without changing route or retaining stale schema.
    await module.selectOption(inventory.uid);await target.waitFor();await module.selectOption(employees.uid);
    await safeCard.getByRole('button',{name:'Archive',exact:true}).click();
    const ready=page.getByRole('dialog',{name:'Archive this field?',exact:true});await ready.waitFor();
    // Another administrator adds a dependency after preflight. Commit must recheck.
    const late=await field(inventory.uid,`late_${engine}`,`Late dependency ${engine}`,'relationship',{target_module_uid:employees.uid,label_field:safe.key});
    await ready.getByRole('button',{name:'Archive field',exact:true}).click();await dialog.waitFor();assert.ok((await dialog.innerText()).includes(`Inventory → Late dependency ${engine}`));assert.equal((await preflight(safe.uid)).field.active,true);
    await call(`/mx/v1/admin/schema/fields/${late.uid}`,'PUT',{active:false});await dialog.getByRole('button',{name:'Check again',exact:true}).click();await ready.waitFor();await ready.getByRole('button',{name:'Archive field',exact:true}).click();await anyDialog.waitFor({state:'hidden'});
    assert.equal((await preflight(safe.uid)).field.active,false);await safeCard.getByRole('button',{name:'Restore',exact:true}).waitFor();
    assert.deepEqual(errors,[]);
    results.browsers[engine]='Named blocker popup, no off-screen error, light/dark/mobile, focus trap/escape/return, exact settings jump, in-dialog error retry, stale preflight recheck, safe archive';
    console.log(`${engine}: archive dialog passed`);await page.close();await browser.close();
  }
  await writeFile(join(mx.directory,'field-archive-results.json'),JSON.stringify(results,null,2));console.log(JSON.stringify(results,null,2));
}catch(error){for(const [i,browser]of browsers.entries())for(const context of browser.contexts())for(const page of context.pages())await page.screenshot({path:join(mx.directory,`failure-${i}.png`)}).catch(()=>{});throw error;}
finally{await Promise.all(browsers.map(b=>b.close().catch(()=>{})));await mx.close();}
