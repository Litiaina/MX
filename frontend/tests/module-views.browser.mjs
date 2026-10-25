// Disposable real MX, SQLite, fixture N1 and rendered views. Never deploys or releases.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/PROJECT-DISK-1TB/'));
assert.ok(process.env.TMPDIR?.startsWith('/mnt/PROJECT-DISK-1TB/'));
assert.ok(!process.env.MX_TEST_N1_URL,'Use fixture storage only');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const results={passed:false,checks:[],browsers:{}};let browser,page;
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const until=async(check,label)=>{const end=Date.now()+30000;while(Date.now()<end){if(await check())return;await new Promise(r=>setTimeout(r,75));}throw Error(label);};
try{
 console.log(`Module workspace artifacts: ${mx.directory}`);
 const password=`Disposable!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@views.test',name:'Owner',password},[201]);
 const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@views.test',password});
 assert.deepEqual((await call('/mx/v1/modules',owner)).modules,[]);
 await call('/mx/v1/user/create',owner,'POST',{email:'reader@views.test',name:'Reader',password,access_level:3},[201]);
 const reader=await call('/mx/v1/auth/authenticate',null,'POST',{email:'reader@views.test',password});
 const m=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Configurable workspace',singular_name:'Opportunity'},[201])).module;
 const base=`/mx/v1/modules/${m.uid}`;
 assert.equal((await call(`${base}/workspace`,owner)).revision,0);
 const add=async(key,type='text',config={})=>call(`/mx/v1/admin/modules/${m.uid}/fields`,owner,'POST',{key,label:key==='title'?'Opportunity':key==='files'?'Supporting documents':key,field_type:type,config},[201]);
 const title=await add('title');await add('email');await add('amount','decimal',{table_summary:'sum'});const secret=await add('secret','text',{access_scope:'administrators'});const files=await add('files','attachments');
 const records=[];for(let i=0;i<46;i++)records.push(await call(`${base}/records`,owner,'POST',{operation_uid:randomUUID(),values:{title:`Opportunity ${String(i).padStart(3,'0')}`,email:`client${i}@example.test`,amount:10,secret:'SECRET-VIEW-TEST'}},[201]));
 const response=await mx.api.post(`/mx/v1/records/${records[0].uid}/attachments/fields/${files.uid}`,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{base_revision:String(records[0].revision),files:{name:'Supporting-note.txt',mimeType:'text/plain',buffer:Buffer.from('WORKSPACE-DOCUMENT-PREVIEW')}}});assert.equal(response.status(),201,await response.text());
 const definition={enabled:true,name:'Sales pipeline',kind:'crm',initial_state:'lead',title_field:'title',restart_on_edit:false,states:[{key:'lead',name:'Lead'},{key:'won',name:'Won'}],actions:[{key:'win',name:'Win',from:'lead',to:'won',access_levels:[0],creator_policy:'any',required_fields:[],note_required:false}]};
 await call(`${base}/workflow`,owner,'PUT',{revision:0,definition,confirm_existing_records:true});
 const books=await call(`${base}/books`,owner);await call(`${base}/books`,owner,'PUT',{revision:books.revision,settings:{...books.settings,accounting:true,inventory:true,currency:'PHP'}});
 const v=(id,kind,name)=>({id,kind,name,title_field:'title',fields:['email','amount'],summary_fields:kind==='overview'?['amount']:[],description:''});
 const layout={default_view:'pipeline',views:[v('pipeline','board','Pipeline'),v('customers','cards','Customers'),v('operations','overview','Operations'),v('documents','documents','Documents'),v('accounting','ledger','Accounting'),v('stock','inventory','Stock'),v('records','records','All records')],sections:[{name:'Customer information',fields:['title','email']},{name:'Commercial details',fields:['amount']}]};
 // Board titles follow process configuration; unused fields are left empty.
 layout.views[0].title_field='';layout.views[0].fields=[];
 let saved=await call(`${base}/workspace`,owner,'PUT',{revision:0,layout});assert.equal(saved.revision,1);
 await call(`${base}/workspace`,owner,'PUT',{revision:0,layout},[409]);
 const invalid=structuredClone(layout);invalid.views[1].title_field='secret';await call(`${base}/workspace`,owner,'PUT',{revision:1,layout:invalid},[400]);
 const invalidDefault=structuredClone(layout);invalidDefault.default_view='missing';await call(`${base}/workspace`,owner,'PUT',{revision:1,layout:invalidDefault},[400]);
 const archive=await call(`/mx/v1/admin/schema/fields/${title.uid}/archive-check`,owner);assert.ok(archive.blockers.some(b=>b.section==='views'));
 const grant=await call(`/mx/v1/admin/accounts/${reader.uid}/modules`,owner);await call(`/mx/v1/admin/accounts/${reader.uid}/modules`,owner,'PUT',{revision:grant.revision,grants:[{module_uid:m.uid,can_read:true,can_create:false,can_update:false,can_report:false,can_attachments:true,can_configure:false,can_delete:false}]});
 await call(`${base}/workspace`,reader,'PUT',{revision:1,layout},[403]);
 const totals=await call(`${base}/work-queue?counts_only=true`,reader);assert.equal(totals.total,46);assert.equal(totals.items.length,0);assert.equal(totals.stage_totals.find(s=>s.state==='lead').total,46);
 assert.equal((await call(`${base}/work-queue?counts_only=true&q=Opportunity%20001`,reader)).total,1);
 assert.equal((await call(`${base}/work-queue?counts_only=true&actionable=true`,reader)).total,0);
 const before=(await call(`${base}/records/${records[1].uid}`,owner));assert.equal(before.revision,1);assert.equal(before.values.amount,10);
 results.checks.push('No starter modules/workspaces; revision conflict; restricted field rejection; named archive dependencies; configure denial; exact permission-filtered counts/search; unchanged record values/revisions');
 for(const engine of(process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
  const executablePath=engine==='brave'?'/usr/bin/brave-browser':engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
  browser=await(engine==='brave'?chromium:{chromium,firefox,webkit}[engine]).launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(['brave','chromium'].includes(engine)?{args:['--no-sandbox','--mute-audio']}: {})});
  const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1440,height:1000}});await context.addInitScript(t=>{sessionStorage.setItem('mx_access_token',t.access_token);sessionStorage.setItem('mx_refresh_token',t.refresh_token);},owner);page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(`https://127.0.0.1:${mx.port}/#module/${m.uid}`);const views=page.getByRole('navigation',{name:'Module views',exact:true});await views.waitFor();const board=page.getByRole('region',{name:'Sales pipeline',exact:true});const lead=board.getByRole('region',{name:'Lead',exact:true});
  await until(async()=>await lead.locator('.pipeline-card').count()===40,'40 bounded cards on first stage page');assert.equal(await page.locator('.records-table').count(),0);assert.equal(await lead.getByLabel('Lead count',{exact:true}).innerText(),'46');
  await lead.getByRole('button',{name:'Next',exact:true}).click();await until(async()=>await lead.locator('.pipeline-card').count()===6,'Stage next page replaces old cards');await lead.getByRole('button',{name:'Previous',exact:true}).click();await until(async()=>await lead.locator('.pipeline-card').count()===40,'Stage previous page');
  await page.getByRole('searchbox',{name:'Search pipeline titles',exact:true}).fill('Opportunity 001');await page.getByRole('search',{exact:true}).getByRole('button',{name:'Search',exact:true}).click();await until(async()=>await board.locator('.pipeline-card').count()===1,'Pipeline title search');
  await board.locator('.pipeline-card').getByRole('button',{name:'Open & review',exact:true}).click();const record=page.locator('.record-dialog');await record.getByRole('heading',{name:'Customer information',exact:true}).waitFor();await record.getByRole('heading',{name:'Commercial details',exact:true}).waitFor();await record.getByRole('button',{name:'Close',exact:true}).click();await page.screenshot({path:join(mx.directory,`${engine}-pipeline.png`)});
  await views.getByRole('button',{name:'Customers',exact:true}).click();await until(async()=>await page.locator('.collection-grid article').count()===24,'Directory bounded at 24 records');await page.locator('.module-view-body').getByRole('button',{name:'Next',exact:true}).click();await until(async()=>await page.locator('.collection-grid article').count()===22,'Directory real next page');await page.screenshot({path:join(mx.directory,`${engine}-directory.png`)});
  await views.getByRole('button',{name:'Operations',exact:true}).click();await page.getByRole('heading',{name:'Action inbox',exact:true}).waitFor();await until(async()=>await page.locator('.metric-grid').innerText().then(t=>t.includes('460.00')),'Whole-module summary tile, not first six records');await page.screenshot({path:join(mx.directory,`${engine}-operations.png`)});
  await views.getByRole('button',{name:'Documents',exact:true}).click();await page.getByRole('button',{name:'Supporting-note.txt',exact:true}).click();const preview=page.getByRole('dialog').filter({has:page.getByText('WORKSPACE-DOCUMENT-PREVIEW',{exact:true})});await preview.waitFor();await preview.getByRole('button',{name:'Close preview',exact:true}).click();
  await views.getByRole('button',{name:'Accounting',exact:true}).click();await page.getByRole('region',{name:'Accounting',exact:true}).getByRole('button',{name:'New journal',exact:true}).waitFor();assert.equal(await page.locator('.book-overlay').count(),0);await page.screenshot({path:join(mx.directory,`${engine}-accounting.png`)});
  await views.getByRole('button',{name:'Stock',exact:true}).click();await page.getByRole('region',{name:'Stock',exact:true}).getByRole('button',{name:'Add stock item',exact:true}).waitFor();await page.screenshot({path:join(mx.directory,`${engine}-stock.png`)});
  await views.getByRole('button',{name:'All records',exact:true}).click();await page.locator('.records-table tbody tr[data-record-uid]').first().waitFor();assert.ok(await page.locator('.records-table tbody tr[data-record-uid]').count()<=50);
  await page.goto(`https://127.0.0.1:${mx.port}/#admin?module=${m.uid}&section=views`);await page.getByRole('heading',{name:'Views & layout',exact:true}).waitFor();assert.ok((await page.locator('.module-workbench-scope').innerText()).includes(m.name));
  await page.getByLabel('View name',{exact:true}).nth(1).fill(`Customers ${engine}`);await page.getByRole('button',{name:'Save workspace layout',exact:true}).click();await until(async()=>{const next=await call(`${base}/workspace`,owner);return next.layout.views[1].name===`Customers ${engine}`;},'Layout saved using administrator UI');
  // Restore fixture between browser engines, retaining real revision checks.
  saved=await call(`${base}/workspace`,owner);await call(`${base}/workspace`,owner,'PUT',{revision:saved.revision,layout});
  await page.goto(`https://127.0.0.1:${mx.port}/#module/${m.uid}`);await page.getByRole('navigation',{name:'Module views',exact:true}).waitFor();await page.setViewportSize({width:390,height:844});await page.addStyleTag({content:'html{font-size:24px!important}'});await until(async()=>await page.locator('.sidebar').evaluate(n=>n.getBoundingClientRect().right<=1),'Mobile sidebar finishes closing after viewport resize');await page.screenshot({path:join(mx.directory,`${engine}-enlarged.png`),animations:'disabled'});assert.ok(await page.locator('.module-workspace').evaluate(n=>{const r=n.getBoundingClientRect();return r.left>=0&&r.right<=innerWidth+1;}),'Enlarged layout remains within viewport');assert.ok(await page.locator('.pipeline-board').evaluate(n=>n.clientHeight>200&&n.scrollWidth>n.clientWidth),'Board retains useful area and horizontal scrolling');
  assert.deepEqual(errors,[]);await context.close();
  const limited=await browser.newContext({ignoreHTTPSErrors:true});await limited.addInitScript(t=>{sessionStorage.setItem('mx_access_token',t.access_token);sessionStorage.setItem('mx_refresh_token',t.refresh_token);},reader);page=await limited.newPage();await page.goto(`https://127.0.0.1:${mx.port}/#module/${m.uid}?record=${records[1].uid}`);await page.locator('.record-dialog').waitFor();assert.ok(!(await page.locator('.record-dialog').innerText()).includes('SECRET-VIEW-TEST'));await page.locator('.record-dialog').getByRole('button',{name:'Close',exact:true}).click();await page.getByRole('navigation',{name:'Module views',exact:true}).getByRole('button',{name:'Accounting',exact:true}).click();await page.getByRole('heading',{name:'Book access restricted',exact:true}).waitFor();await limited.close();
  results.browsers[engine]={passed:true,checks:['Pipeline is the main view, not a table','Per-stage bounded paging and search','Named record sections','Directory paging','Whole-module operations totals','Document preview','Full-page accounting/inventory','Original paged table preserved','Correct module selected in admin','Layout saved in UI','390px / enlarged text','Restricted fields and whole-book access']};await browser.close();browser=null;
 }
 await mx.restart();assert.equal((await call(`${base}/workspace`,owner)).layout.default_view,'pipeline');results.passed=true;console.log(JSON.stringify(results));
}catch(e){results.error=e.message;if(page&&!page.isClosed()){await page.screenshot({path:join(mx.directory,'workspace-failure.png')}).catch(()=>{});await writeFile(join(mx.directory,'workspace-failure-aria.txt'),await page.locator('body').ariaSnapshot().catch(()=>''));}throw e;}finally{if(browser)await browser.close();await writeFile(join(mx.directory,'module-views-results.json'),JSON.stringify(results,null,2));await mx.close();}
