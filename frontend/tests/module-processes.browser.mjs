// Real HTTP, SQLite and built UI. All processes are opt-in; no live services/data.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
import {workflowTemplate,workflowKindNames} from '../src/lib/util/workflowTemplates.ts';

assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/PROJECT-DISK-1TB/'),'Only HDD artifacts');
assert.ok(process.env.TMPDIR?.startsWith('/mnt/PROJECT-DISK-1TB/'),'Only HDD temporary files');
assert.ok(!process.env.MX_TEST_N1_URL,'Never use live N1');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const results={passed:false,checks:[],browsers:{}};
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const until=async(check,label)=>{const deadline=Date.now()+20000;while(Date.now()<deadline){if(await check())return;await new Promise(r=>setTimeout(r,80));}throw Error(label);};
const path=(m,r='')=>`/mx/v1/modules/${m.uid}${r?`/records/${r}`:''}`;
const action=async(m,r,user,key,note='Verified by reviewer',operation_uid=randomUUID(),expected=[200])=>{
  const s=await call(`${path(m,r)}/workflow`,user);
  return call(`${path(m,r)}/workflow/actions`,user,'POST',{operation_uid,action:key,note,record_revision:s.record_revision,workflow_revision:s.workflow_revision,configuration_revision:s.configuration_revision},expected);
};
let owner,editor,viewer,page,browser;
try{
  console.log(`Module processes artifacts: ${mx.directory}`);
  const password=`Disposable!${randomUUID()}`;
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@process.test',name:'Owner',password},[201]);
  owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@process.test',password});
  assert.deepEqual((await call('/mx/v1/modules',owner)).modules,[],'Fresh installation remains fully blank');
  for(const [name,level] of [['Editor',2],['Viewer',3]])await call('/mx/v1/user/create',owner,'POST',{email:`${name.toLowerCase()}@process.test`,name,password,access_level:level},[201]);
  editor=await call('/mx/v1/auth/authenticate',null,'POST',{email:'editor@process.test',password});
  viewer=await call('/mx/v1/auth/authenticate',null,'POST',{email:'viewer@process.test',password});
  const modules=[];
  const createModule=async(name)=>{
    const m=(await call('/mx/v1/admin/modules',owner,'POST',{name,singular_name:'Request'},[201])).module;
    const title=await call(`/mx/v1/admin/modules/${m.uid}/fields`,owner,'POST',{key:'title',label:'Title',field_type:'text'},[201]);
    const amount=await call(`/mx/v1/admin/modules/${m.uid}/fields`,owner,'POST',{key:'amount',label:'Amount',field_type:'decimal'},[201]);
    modules.push(m);
    for(const user of [editor,viewer]){const grants=await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner);await call(`/mx/v1/admin/accounts/${user.uid}/modules`,owner,'PUT',{revision:grants.revision,grants:modules.map(m=>({module_uid:m.uid,can_read:true,can_create:user===editor,can_update:user===editor,can_delete:false,can_configure:false,can_report:true,can_attachments:user===editor}))});}
    return {...m,title,amount};
  };
  const create=async(m,user,values)=>call(`${path(m)}/records`,user,'POST',{operation_uid:randomUUID(),values},[200,201]);
  const configure=async(m,kind,changes={})=>call(`${path(m)}/workflow`,owner,'PUT',{revision:0,definition:{...workflowTemplate(kind),enabled:true,title_field:'title',...changes},confirm_existing_records:true});
  const protectedModule=await createModule('Protected process');
  await configure(protectedModule,'erp');
  assert.equal((await call(`${path(protectedModule)}/workflow`,viewer)).existing_records,null,'Read access must not reveal an administrator-only impact count');
  await call(`${path(protectedModule)}/workflow`,viewer,'PUT',{revision:1,definition:workflowTemplate('erp'),confirm_existing_records:true},[403]);
  const r=await create(protectedModule,editor,{title:'Server checks',amount:500});
  await action(protectedModule,r.uid,editor,'submit');
  await action(protectedModule,r.uid,editor,'approve','forged approval',randomUUID(),[403]);
  await action(protectedModule,r.uid,viewer,'approve','forged approval',randomUUID(),[403]);
  await action(protectedModule,r.uid,owner,'approve','',randomUUID(),[400]);
  await action(protectedModule,r.uid,owner,'approve');
  const stale=await call(`${path(protectedModule,r.uid)}/workflow`,editor);
  await call(path(protectedModule,r.uid),editor,'PATCH',{base_revision:r.revision,changes:{amount:750}});
  const reset=await call(`${path(protectedModule,r.uid)}/workflow`,owner);assert.equal(reset.state,'draft');assert.equal(reset.restarted,true);
  await call(`${path(protectedModule,r.uid)}/workflow/actions`,editor,'POST',{operation_uid:randomUUID(),action:'submit',note:'',configuration_revision:stale.configuration_revision,workflow_revision:stale.workflow_revision,record_revision:stale.record_revision},[409]);
  const request={operation_uid:randomUUID(),action:'submit',note:'',configuration_revision:reset.configuration_revision,workflow_revision:reset.workflow_revision,record_revision:reset.record_revision};
  const concurrent=await Promise.all(Array.from({length:8},()=>call(`${path(protectedModule,r.uid)}/workflow/actions`,editor,'POST',request)));
  assert.equal(new Set(concurrent.map(v=>v.event_uid)).size,1,'Concurrent same-operation replay commits once');
  await call(`${path(protectedModule,r.uid)}/workflow/actions`,editor,'POST',{...request,note:'different payload'},[409]);
  const own=await create(protectedModule,owner,{title:'Own approval',amount:1});await action(protectedModule,own.uid,owner,'submit');await action(protectedModule,own.uid,owner,'approve','',randomUUID(),[403]);
  const current=(await call('/mx/v1/modules',owner)).modules.find(m=>m.uid===protectedModule.uid);
  await call(`/mx/v1/admin/modules/${protectedModule.uid}`,owner,'PUT',{config:{...current.config,record_visibility:'creator'}});
  await call(`${path(protectedModule,r.uid)}/workflow`,viewer,'GET',undefined,[404]);
  await call(`${path(protectedModule,r.uid)}/workflow/history`,viewer,'GET',undefined,[404]);
  assert.deepEqual((await call(`${path(protectedModule)}/work-queue`,viewer)).items,[]);
  results.checks.push('Fresh blank deployment; role/module grants; creator isolation; separate reviewer; notes; edit invalidation; stale revision; eight concurrent replay requests; payload binding');
  for(const engine of (process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
    const executablePath=engine==='brave'?'/usr/bin/brave-browser':engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    browser=await(engine==='brave'?chromium:{chromium,firefox,webkit}[engine]).launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(['chromium','brave'].includes(engine)?{args:['--no-sandbox','--mute-audio']}: {})});
    const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1440,height:1000}});
    await context.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
    page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
    for(const kind of ['crm','erp','accounting']){
      const m=await createModule(`${engine} ${kind} workflow`);
      const record=await create(m,editor,{title:`${kind.toUpperCase()} acceptance`,amount:1250});
      assert.equal((await call(`${path(m,record.uid)}/workflow`,owner)).enabled,false);
      await page.goto(`https://127.0.0.1:${mx.port}/#admin`);await page.getByRole('button',{name:'Modules & fields',exact:true}).click();
      await page.getByRole('searchbox',{name:'Find a module',exact:true}).fill(m.name);
      await page.getByRole('button',{name:`Configure ${m.name}`,exact:true}).click();
      await page.getByRole('button',{name:'Process & actions',exact:true}).click();
      const admin=page.getByRole('region',{name:'Module workflow configuration',exact:true});
      await admin.getByRole('button',{name:workflowKindNames[kind],exact:true}).click();
      await admin.getByRole('combobox',{name:'Queue title field',exact:true}).selectOption('title');
      const first=admin.locator('.action-card').first();await first.locator('summary').click();await first.getByRole('checkbox',{name:'Title',exact:true}).check();
      await admin.getByRole('checkbox',{name:`Enable this process for ${m.name}`,exact:true}).check();
      await admin.getByRole('checkbox',{name:/Apply to the 1 existing records/}).check();
      await admin.getByRole('button',{name:'Save process',exact:true}).click();
      await until(async()=>(await call(`${path(m)}/workflow`,owner)).definition.enabled,'Process saved through real administration UI');
      const saved=(await call(`${path(m)}/workflow`,owner)).definition;assert.equal(saved.kind,kind);assert.deepEqual(saved.actions[0].required_fields,['title']);assert.ok(saved.actions.slice(1).every(a=>a.required_fields.length===0),'Requirements do not bleed across action controls');
      const blocker=await call(`/mx/v1/admin/schema/fields/${m.title.uid}/archive-check`,owner);assert.equal(blocker.can_archive,false);assert.ok(blocker.blockers.some(b=>b.section==='workflow'));
      await page.goto(`https://127.0.0.1:${mx.port}/#module/${m.uid}`);
      await page.getByRole('button',{name:'Work queue',exact:true}).click();
      const queue=page.getByRole('dialog',{name:workflowTemplate(kind).name,exact:true});await queue.waitFor();
      await queue.getByRole('heading',{name:`${kind.toUpperCase()} acceptance`,exact:true}).waitFor();
      await queue.getByRole('button',{name:'Review record',exact:true}).click();
      const dialog=page.getByRole('dialog',{name:'View Request',exact:true});await dialog.waitFor();
      const process=dialog.getByRole('region',{name:'Record process',exact:true});
      const keys=kind==='crm'?['qualify','propose','win']:['submit','approve'];
      for(const key of keys){
        const a=workflowTemplate(kind).actions.find(a=>a.key===key);
        const loseAcknowledgements=engine==='chromium'&&kind==='crm'&&key==='qualify';const lost=[];
        if(loseAcknowledgements)await page.route(`**${path(m,record.uid)}/workflow/actions`,async route=>{lost.push(route.request().postDataJSON());await route.fetch();await route.abort('failed');});
        await process.getByRole('button',{name:a.name,exact:true}).click();
        if(a.note_required){await process.getByRole('textbox',{name:'Action note (required)',exact:true}).fill('Verified during browser acceptance test');await process.getByRole('button',{name:'Refresh process',exact:true}).click();assert.equal(await process.getByRole('textbox',{name:'Action note (required)',exact:true}).inputValue(),'Verified during browser acceptance test','A harmless refresh preserves an action note');}
        await process.getByRole('button',{name:`Confirm ${a.name.toLowerCase()}`,exact:true}).click();
        if(loseAcknowledgements){await process.getByRole('button',{name:'Check and retry action',exact:true}).waitFor();assert.equal(lost.length,2);assert.equal(lost[0].operation_uid,lost[1].operation_uid);await page.unroute(`**${path(m,record.uid)}/workflow/actions`);await process.getByRole('button',{name:'Check and retry action',exact:true}).click();await process.getByText('MX confirmed the previously submitted action. It was not applied twice.',{exact:true}).waitFor();assert.equal((await call(`${path(m,record.uid)}/workflow/history`,owner)).items.length,1);}
        await until(async()=>(await call(`${path(m,record.uid)}/workflow`,owner)).state===a.to,'Action committed by real record dialog');
      }
      await process.getByRole('button',{name:'Decision history',exact:true}).click();await process.getByRole('button',{name:'View decision snapshot',exact:true}).first().waitFor();
      await process.getByRole('button',{name:'View decision snapshot',exact:true}).first().click();await page.getByRole('dialog',{name:'Request history',exact:true}).waitFor();
      await page.getByRole('button',{name:'Back to request',exact:true}).click();
      await page.screenshot({path:join(mx.directory,`${engine}-${kind}-record.png`)});
      await dialog.getByRole('button',{name:'Close',exact:true}).click();
      // Both large text and narrow displays retain usable queue controls.
      if(kind==='accounting'){
        await page.setViewportSize({width:390,height:900});await page.evaluate(()=>document.documentElement.style.fontSize='24px');
        await page.getByRole('button',{name:'Work queue',exact:true}).click();await queue.waitFor();
        const bounds=await queue.boundingBox();assert.ok(bounds.x>=-1&&bounds.x+bounds.width<=391,'Enlarged-text queue fits narrow viewport');
        await queue.getByRole('button',{name:'Close queue',exact:true}).click();
        await page.setViewportSize({width:1440,height:1000});await page.evaluate(()=>document.documentElement.style.fontSize='');
      }
      // Edits have explicit process semantics, without erasing decision history.
      const latest=await call(path(m,record.uid),owner);await call(path(m,record.uid),editor,'PATCH',{base_revision:latest.revision,changes:{amount:1500}});
      const next=await call(`${path(m,record.uid)}/workflow`,owner);assert.equal(next.state,kind==='crm'?'won':'draft');assert.equal(next.restarted,kind!=='crm');
      assert.equal((await call(`${path(m,record.uid)}/workflow/history`,owner)).items.length,keys.length);
      const empty=await create(m,editor,{amount:1});const unavailable=await call(`${path(m,empty.uid)}/workflow`,owner);assert.ok(unavailable.actions[0].missing_fields.includes('Title'));await action(m,empty.uid,owner,keys[0],'',randomUUID(),[400]);
      if(kind==='crm'){
        const configuration=await call(`${path(m)}/workflow`,owner);await call(`${path(m)}/workflow`,owner,'PUT',{revision:configuration.revision,definition:{...configuration.definition,enabled:false},confirm_existing_records:false});
        await page.goto(`https://127.0.0.1:${mx.port}/#module/${m.uid}?record=${record.uid}`);
        const stopped=page.getByRole('region',{name:'Record process',exact:true});await stopped.getByText('Process disabled · Won',{exact:true}).waitFor();await stopped.getByRole('button',{name:'Decision history',exact:true}).click();await stopped.getByRole('button',{name:'View decision snapshot',exact:true}).first().waitFor();await page.getByRole('dialog',{name:'View Request',exact:true}).getByRole('button',{name:'Close',exact:true}).click();
        if(engine==='chromium'){
          let release,intercepted=false;const gate=new Promise(resolve=>release=resolve);const target=`**${path(m,record.uid)}`;
          await page.route(target,async route=>{const response=await route.fetch();intercepted=true;await gate;await route.fulfill({response});});
          try{
          await page.goto(`https://127.0.0.1:${mx.port}/#module/${m.uid}?record=${record.uid}`);await until(()=>intercepted,'Repeated record-link intent must start a fresh read');
          await page.getByRole('button',{name:'+ New request',exact:true}).click();await page.getByRole('dialog',{name:'New Request',exact:true}).waitFor();
          const settled=page.waitForResponse(response=>response.url().endsWith(path(m,record.uid)));release();await settled;await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
          assert.ok(await page.getByRole('dialog',{name:'New Request',exact:true}).isVisible(),'A late linked-record response cannot replace a new record form');
          await page.getByRole('dialog',{name:'New Request',exact:true}).getByRole('button',{name:'Close',exact:true}).click();
          }finally{release();await page.unroute(target);}
        }
      }
    }
    assert.deepEqual(errors,[],'No unhandled browser errors');results.browsers[engine]={crm:true,erp:true,accounting_approvals:true,queue:true,decision_snapshots:true,narrow_enlarged_text:true};
    await browser.close();browser=null;
  }
  const before=await call(`${path(protectedModule,r.uid)}/workflow/history`,owner);await mx.restart();const after=await call(`${path(protectedModule,r.uid)}/workflow/history`,owner);assert.deepEqual(after,before,'Decision history persists across server restart');
  results.checks.push('Required field validation; dependency checks; real admin configuration; CRM/ERP/expense actions; decision snapshots; browser queue; revision invalidation; restart persistence; lost acknowledgement retry; disabled history; same-record reopen; obsolete open response cannot replace a new form');
  results.passed=true;console.log(JSON.stringify(results));
}catch(error){results.error=String(error?.stack||error);if(page)await page.screenshot({path:join(mx.directory,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{await writeFile(join(mx.directory,'module-processes-results.json'),JSON.stringify(results,null,2));if(browser)await browser.close();await mx.close();}
