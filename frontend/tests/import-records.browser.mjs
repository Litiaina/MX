// All runtime artifacts MUST remain under the operator's designated directory.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join, resolve } from 'node:path';
import { readFile, writeFile } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';
import { onTestDisk } from './concurrency-fixture.mjs';
import { DatabaseSync } from 'node:sqlite';
const artifactRoot = resolve(process.env.MX_TEST_ARTIFACTS || '');
assert.ok(onTestDisk(artifactRoot) && onTestDisk(process.env.TMPDIR), 'Generated import test data must stay on the authorized test volume');
assert.ok(!process.env.MX_TEST_N1_URL, 'Never use live N1 for import tests');
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url))), browsers = [], evidence = { api: [], browsers: [], workbook: null };
let activePage, owner;
const call = async (path, method = 'GET', body, expected = [200], user = owner) => (await mx.call(path, user?.access_token, method, body, expected)).body;
const password = `Test-only!${randomUUID()}`;
const upload = async (module, name, bytes, user = owner, uid = randomUUID(), delimiter = 'comma') => {
  const response = await mx.api.post(`/mx/v1/modules/${module}/imports`, { headers: { Authorization: `Bearer ${user.access_token}` }, multipart: { job_uid: uid, delimiter, file: { name, mimeType: 'application/octet-stream', buffer: bytes } }, timeout: 120000 });
  assert.equal(response.status(), 200, await response.text()); return response.json();
};
const config = (index = 0, header_row = 1) => ({ index, header_row, first_data_row: header_row + 1, columns: [{ column: 1, key: 'name', fill_down: false, date_format: '' }, { column: 2, key: 'start', fill_down: true, date_format: 'mdy-short-dot' }, { column: 3, key: 'amount', fill_down: false, date_format: '' }, { column: 4, key: 'status', fill_down: false, date_format: '' }] });
try {
  console.log(`Import artifacts: ${mx.directory}`);
  await call('/mx/v1/auth/create', 'POST', { email: 'imports@test.invalid', name: 'Import operator', password }, [201], { access_token: mx.signupKey });
  owner = await call('/mx/v1/auth/authenticate', 'POST', { email: 'imports@test.invalid', password }, [200], null);
  const module = (await call('/mx/v1/admin/modules', 'POST', { name: 'Import cases', singular_name: 'Case' }, [201])).module;
  const base = `/mx/v1/modules/${module.uid}/imports`;
  const field = async (key, label, type, config = {}, required = false, unique_value = false) => call(`/mx/v1/admin/modules/${module.uid}/fields`, 'POST', { key, label, field_type: type, config, required, unique_value }, [201]);
  await field('name', 'Name', 'text', {}, true, true); await field('start', 'Start date', 'date'); await field('amount', 'Amount', 'decimal'); await field('status', 'Status', 'select', { options: ['Open', 'Closed'] }, true);
  const users = [];
  for (let i = 0; i < 8; i++) { const email = `import${i}@test.invalid`; await call('/mx/v1/user/create', 'POST', { email, name: `Import editor ${i}`, password, access_level: 2 }, [201]); const user = await call('/mx/v1/auth/authenticate', 'POST', { email, password }, [200], null); const grants = await call(`/mx/v1/admin/accounts/${user.uid}/modules`); await call(`/mx/v1/admin/accounts/${user.uid}/modules`, 'PUT', { revision: grants.revision, grants: [{ module_uid: module.uid, can_read: true, can_create: true, can_update: true, can_delete: false, can_configure: false, can_report: true, can_attachments: false }] }); users.push(user); }
  const bytes = Buffer.from('Name,Start date,Amount,Status\r\n001,1.5.26,5,Open\r\n002,,6,closed\r\n003,not-a-date,7,Open\r\n004,1.6.26,broken,Open\r\n005,1.6.26,8,Unknown\r\n001,1.7.26,9,Open\r\n006,1.7.26,10,Open\r\n');
  const uid = randomUUID(); let j = await upload(module.uid, 'cases.csv', bytes, owner, uid);
  assert.equal((await upload(module.uid, 'cases.csv', bytes, owner, uid)).uid, j.uid, 'Upload lost-ack replay');
  await call(`${base}/${j.uid}`, 'GET', undefined, [404], users[0]);
  j = await call(`${base}/${j.uid}/configure`, 'PUT', { sheets: [config()] }); assert.equal(j.total, 7);
  assert.equal(j.validation.ready,4);assert.equal(j.validation.invalid,3,'Preview counts invalid rows before processing; commit-time uniqueness is still checked later');
  assert.equal((await call(`${base}/${j.uid}/rows?state=issues`)).total,3,'Issues are inspectable before import, not mislabeled as an empty failed log');
  const preview = await call(`${base}/${j.uid}/rows?state=pending`); assert.equal(preview.rows[0].values.name, '001'); assert.equal(preview.rows[1].values.start, '2026-01-05'); assert.equal(preview.rows[1].values.status,'Closed');assert.ok(preview.rows[2].errors[0].message.includes('Invalid date'));
  j = await call(`${base}/${j.uid}/process`, 'POST'); assert.equal(j.imported, 3); assert.equal(j.failed, 4); assert.equal(j.processed, 7); assert.equal(j.pending, 0);
  assert.equal((await call(`${base}/${j.uid}/process`, 'POST')).imported, 3, 'Batch replay must not create duplicates');
  const failed = await call(`${base}/${j.uid}/rows?state=failed`); assert.equal(failed.total, 4);
  assert.ok(failed.rows.find(r=>r.values.name==='001').errors[0].message.includes('Name must be unique'),'Uniqueness errors identify the exact field');
  const fix = { sheet: failed.rows[0].sheet, row: failed.rows[0].row, values: { name: '003', start: '2026-01-06', amount: 7, status: 'Open' } };
  const repaired = await call(`${base}/${j.uid}/retry`, 'POST', fix); assert.ok(repaired.record_uid); assert.equal((await call(`${base}/${j.uid}/retry`, 'POST', fix)).record_uid, repaired.record_uid);
  const version = await call(`/mx/v1/modules/${module.uid}/records/${repaired.record_uid}/versions`); assert.equal(version.versions.length, 1, 'Import uses normal version history');
  await call(`${base}/${j.uid}/configure`, 'PUT', { sheets: [config()] }, [400]);
  evidence.api.push('Row-level type/date/select/uniqueness failures; text 001 retained; explicit date fill-down; corrected row and lost-ack replay; one normal version; no remap after commit; private logs');
  const concurrent = await Promise.all(users.map(async (user, i) => { const payload = Buffer.from('Name,Start date,Amount,Status\n' + Array.from({ length: 45 }, (_, n) => `u${i}-${n},1.5.26,${n},Open`).join('\n')); const job = await upload(module.uid, `eight-${i}.csv`, payload, user); return { user, job: await call(`${base}/${job.uid}/configure`, 'PUT', { sheets: [config()] }, [200], user) }; }));
  const target = concurrent[0]; await Promise.all(Array.from({ length: 8 }, () => call(`${base}/${target.job.uid}/process`, 'POST', undefined, [200], target.user))); let progress = await call(`${base}/${target.job.uid}`, 'GET', undefined, [200], target.user); assert.equal(progress.imported, 45); assert.equal(progress.failed, 0);
  for (const x of concurrent.slice(1)) x.job = await call(`${base}/${x.job.uid}/process`, 'POST', undefined, [200], x.user);
  await mx.restart();
  await Promise.all(concurrent.slice(1).map(async x => { let data = await call(`${base}/${x.job.uid}`, 'GET', undefined, [200], x.user); while (data.pending) data = await call(`${base}/${data.uid}/process`, 'POST', undefined, [200], x.user); assert.equal(data.imported, 45); assert.equal(data.failed, 0); }));
  const all = await call(`/mx/v1/modules/${module.uid}/records?limit=1`); assert.equal(all.total, 364);
  const paged = await call(`${base}/${target.job.uid}/rows?state=imported&page=2`, 'GET', undefined, [200], target.user); assert.equal(paged.rows.length, 20); assert.equal(paged.total, 45);
  const before = (await call(`/mx/v1/modules/${module.uid}/records?limit=1`)).total; await call(`${base}/${target.job.uid}`, 'DELETE', undefined, [200], target.user); assert.equal((await call(`/mx/v1/modules/${module.uid}/records?limit=1`)).total, before);
  // Revoke a grant after staging: no row may be created using stale permissions.
  const deniedJob = await upload(module.uid, 'revoke.csv', Buffer.from('Name,Start date,Amount,Status\nDenied,1.5.26,1,Open\n'), users[0]); await call(`${base}/${deniedJob.uid}/configure`, 'PUT', { sheets: [config()] }, [200], users[0]);
  const g = await call(`/mx/v1/admin/accounts/${users[0].uid}/modules`); await call(`/mx/v1/admin/accounts/${users[0].uid}/modules`, 'PUT', { revision: g.revision, grants: [] }); await call(`${base}/${deniedJob.uid}/process`, 'POST', undefined, [403], users[0]); assert.equal((await call(`/mx/v1/modules/${module.uid}/records?limit=1`)).total, before);
  evidence.api.push('Eight accounts × 45 rows; eight simultaneous process requests for one job; server restart mid-job; exactly 360 new records; 20-row pagination; permission revocation blocks pending saves; deleting logs preserves records');
  // Inject a storage failure into this disposable database only. If the
  // success marker cannot commit, neither record nor version may survive.
  let atomic = await upload(module.uid,'atomic.csv',Buffer.from('Name,Start date,Amount,Status\nAtomic-row,1.5.26,1,Open\n'));
  atomic=await call(`${base}/${atomic.uid}/configure`,'PUT',{sheets:[config()]});
  const sql = statement => {const db=new DatabaseSync(join(mx.directory,'test.db'));try{db.exec(statement);}finally{db.close();}};
  const countBefore=(await call(`/mx/v1/modules/${module.uid}/records?limit=1`)).total;
  sql(`CREATE TRIGGER import_marker_fault BEFORE UPDATE OF state ON mx_import_rows WHEN NEW.state='imported' AND NEW.job_uid='${atomic.uid}' BEGIN SELECT RAISE(FAIL,'Injected import marker fault'); END;`);
  await call(`${base}/${atomic.uid}/process`,'POST',undefined,[503]);assert.equal((await call(`/mx/v1/modules/${module.uid}/records?limit=1`)).total,countBefore);
  const stillPending=await call(`${base}/${atomic.uid}`);assert.equal(stillPending.pending,1);assert.equal(stillPending.failed,0);
  sql('DROP TRIGGER import_marker_fault;');atomic=await call(`${base}/${atomic.uid}/process`,'POST');assert.equal(atomic.imported,1);assert.equal((await call(`/mx/v1/modules/${module.uid}/records?limit=1`)).total,countBefore+1);
  evidence.api.push('Injected SQLite marker failure rolls back record, version and dedupe; infrastructure failure leaves row pending, not invalid; resuming commits exactly once');
  const employees=(await call('/mx/v1/admin/modules','POST',{name:'Import employees',singular_name:'Employee'},[201])).module;
  for(const [key,label] of [['employee_name','Employee name'],['classification','Classification']])await call(`/mx/v1/admin/modules/${employees.uid}/fields`,'POST',{key,label,field_type:'text'},[201]);
  const alice=await call(`/mx/v1/modules/${employees.uid}/records`,'POST',{values:{employee_name:'Fixture employee',classification:'JO'}},[201]);
  await field('classification','Classification','select',{options:['JO','REG']});await field('employee','Employee','relationship',{target_module_uid:employees.uid,label_field:'employee_name',auto_fill:[{source_field:'classification',target_field:'classification'}]});
  const when={match:'all',conditions:[{field:'status',operator:'eq',value:'Closed'}]};await field('note','Close note','text',{form_rules:{visible_when:when,required_when:when}});
  const relatedConfig={...config(),columns:[...config().columns,{column:5,key:'employee',fill_down:false,date_format:''},{column:6,key:'note',fill_down:false,date_format:''}]};
  let linked=await upload(module.uid,'relationships.csv',Buffer.from(`Name,Start date,Amount,Status,Employee,Close note\nLinked-good,1.5.26,1,Open,${alice.uid},\nLinked-required,1.5.26,1,Closed,${alice.uid},\nLinked-hidden,1.5.26,1,Open,${alice.uid},Hidden write\nLinked-invalid,1.5.26,1,Open,typed-name,\n`));
  linked=await call(`${base}/${linked.uid}/configure`,'PUT',{sheets:[relatedConfig]});
  const linkPreview=await call(`${base}/${linked.uid}/rows?state=pending`);assert.equal(linkPreview.rows[0].preview_values.classification,'JO');
  linked=await call(`${base}/${linked.uid}/process`,'POST');assert.equal(linked.imported,1);assert.equal(linked.failed,3);
  const linkedSaved=(await call(`${base}/${linked.uid}/rows?state=imported`)).rows[0];assert.equal((await call(`/mx/v1/modules/${module.uid}/records/${linkedSaved.record_uid}`)).values.classification,'JO');
  let restricted=await upload(module.uid,'restricted.csv',Buffer.from(`Name,Start date,Amount,Status,Employee,Close note\nRestricted-link,1.5.26,1,Open,${alice.uid},\n`),users[1]);restricted=await call(`${base}/${restricted.uid}/configure`,'PUT',{sheets:[relatedConfig]},[200],users[1]);restricted=await call(`${base}/${restricted.uid}/process`,'POST',undefined,[200],users[1]);assert.equal(restricted.imported,0);assert.equal(restricted.failed,1);
  evidence.api.push('Relationship ID auto-fill preview and stored classification; required/hidden conditional-field errors; typed names rejected; source-module denial cannot import or leak linked classification');
  const requiredFile=await field('closing_file','Closing file','attachments',{form_rules:{visible_when:when}});
  let withFile=await upload(module.uid,'requires-file.csv',Buffer.from(`Name,Start date,Amount,Status,Employee,Close note\nRequires-file,1.5.26,1,closed,${alice.uid},Ready note\n`));
  withFile=await call(`${base}/${withFile.uid}/configure`,'PUT',{sheets:[relatedConfig]});
  await call(`/mx/v1/admin/schema/fields/${requiredFile.uid}`,'PUT',{required:true,config:{form_rules:{visible_when:when}}});
  withFile=await call(`${base}/${withFile.uid}/process`,'POST');assert.equal(withFile.imported,1);assert.equal(withFile.failed,0);
  const noFiles=(await call(`${base}/${withFile.uid}/rows?state=imported`)).rows[0];assert.equal((await call(`/mx/v1/modules/${module.uid}/records/${noFiles.record_uid}`)).attached_files.length,0);
  evidence.api.push('Case-insensitive choices use canonical values for conditions; attachment requirements added after preview do not block record-value imports or fabricate files');
  const blockedModule=(await call('/mx/v1/admin/modules','POST',{name:'Required attachment recovery',singular_name:'Legacy record'},[201])).module;
  await call(`/mx/v1/admin/modules/${blockedModule.uid}/fields`,'POST',{key:'name',label:'Name',field_type:'text'},[201]);
  await call(`/mx/v1/admin/modules/${blockedModule.uid}/fields`,'POST',{key:'quantity',label:'Quantity',field_type:'integer',required:true},[201]);
  const blockedFile=await call(`/mx/v1/admin/modules/${blockedModule.uid}/fields`,'POST',{key:'evidence',label:'FILE ATTACHMENTS',field_type:'attachments',required:true},[201]);
  const blockedBase=`/mx/v1/modules/${blockedModule.uid}/imports`;
  const blockedMapping={index:0,header_row:1,first_data_row:2,columns:[{column:1,key:'name',fill_down:false,date_format:''},{column:2,key:'quantity',fill_down:false,date_format:''}]};
  let blocked=await upload(blockedModule.uid,'legacy.csv',Buffer.from('Name,Quantity\n'+Array.from({length:45},(_,n)=>`Legacy ${n},${n}`).join('\n')));
  blocked=await call(`${blockedBase}/${blocked.uid}/configure`,'PUT',{sheets:[blockedMapping]});
  assert.equal(blocked.validation.ready,45);assert.equal(blocked.validation.invalid,0);
  // Reproduce stored failures from the older importer in this disposable DB.
  // Never retry/import into the operator's actual database automatically.
  const oldError=JSON.stringify([{field:'Record validation',message:'FILE ATTACHMENTS requires a file. Spreadsheet import cannot supply it.'}]);
  sql(`UPDATE mx_import_rows SET state='failed',errors_json='${oldError}' WHERE job_uid='${blocked.uid}' AND state='pending';UPDATE mx_import_jobs SET state='completed' WHERE uid='${blocked.uid}';`);
  blocked=await call(`${blockedBase}/${blocked.uid}`);assert.equal(blocked.validation.ready,0);assert.equal(blocked.validation.invalid,45);assert.equal(blocked.validation.issues[0].rows,45);
  assert.equal((await call(`${blockedBase}/${blocked.uid}/rows?state=issues&page=2`)).rows.length,20);
  assert.equal(blocked.failed,45);assert.equal(blocked.imported,0);
  blocked=await call(`${blockedBase}/${blocked.uid}/configure`,'PUT',{sheets:[blockedMapping]});assert.equal(blocked.validation.ready,45);assert.equal(blocked.validation.invalid,0);assert.equal(blocked.failed,0);assert.equal((await call(`/mx/v1/modules/${blockedModule.uid}/records?limit=1`)).total,0,'Revalidation creates no records');
  while(blocked.pending)blocked=await call(`${blockedBase}/${blocked.uid}/process`,'POST');assert.equal(blocked.imported,45);assert.equal(blocked.failed,0);
  await call(`${blockedBase}/${blocked.uid}/process`,'POST');assert.equal((await call(`/mx/v1/modules/${blockedModule.uid}/records?limit=1`)).total,45);
  await call(`${blockedBase}/${blocked.uid}/configure`,'PUT',{sheets:[blockedMapping]},[400]);
  assert.equal((await call(`/mx/v1/modules/${blockedModule.uid}/schema`)).fields.find(f=>f.uid===blockedFile.uid).required,true,'Required attachment configuration is unchanged');
  evidence.api.push('Required attachments stay required in the schema but are excluded from import; historical all-failed job has grouped counts/paged issues, revalidates without a new upload and commits exactly once; saved jobs cannot be remapped');
  if (process.env.MX_IMPORT_SAMPLE) {
    assert.ok(resolve(process.env.MX_IMPORT_SAMPLE).startsWith('/home/altear/Temp Environment/'));
    const sample = await upload(module.uid, 'COMMUNICATION JANUARY-JULY 2026.xlsx', await readFile(process.env.MX_IMPORT_SAMPLE));
    assert.equal(sample.sheets.length, 10); assert.ok(sample.sheets.every(s => s.row_count < 17075));
    assert.equal(sample.sheets.filter(s => s.error).length,1);assert.ok(sample.sheets[9].error.includes('safety budget'));
    evidence.workbook = sample.sheets.map(s => ({ index: s.index, name: s.name, populatedRows: s.row_count, columns: s.columns, error:s.error }));
    console.log('Real workbook: nine usable sheets; pathological October header sheet flagged separately');
    const migration=(await call('/mx/v1/admin/modules','POST',{name:'Disposable workbook migration',singular_name:'Source row'},[201])).module;
    for(let i=1;i<=10;i++)await call(`/mx/v1/admin/modules/${migration.uid}/fields`,'POST',{key:`source${i}`,label:`Source column ${i}`,field_type:'text'},[201]);
    let real=await upload(migration.uid,'communication-sample.xlsx',await readFile(process.env.MX_IMPORT_SAMPLE));
    const firstSheet=real.sheets[0];const header=firstSheet.sample[0].row;
    const began=performance.now();real=await call(`/mx/v1/modules/${migration.uid}/imports/${real.uid}/configure`,'PUT',{sheets:[0,1].map(index=>({index,header_row:real.sheets[index].sample[0].row,first_data_row:real.sheets[index].sample[0].row+1,columns:Array.from({length:10},(_,i)=>({column:i+1,key:`source${index===0?i+1:((i+1)%10)+1}`,fill_down:false,date_format:''}))}))});
    const secondPreview=await call(`/mx/v1/modules/${migration.uid}/imports/${real.uid}/rows?state=pending&sheet=1`);assert.ok(secondPreview.total>1000);assert.equal(secondPreview.rows[0].values.source2,secondPreview.rows[0].cells['1']?.text||null,'Second sheet uses its own distinct mapping');
    const planned=real.total;assert.ok(planned>1000);while(real.pending)real=await call(`/mx/v1/modules/${migration.uid}/imports/${real.uid}/process`,'POST');
    assert.equal(real.imported,planned);assert.equal(real.failed,0);assert.equal((await call(`/mx/v1/modules/${migration.uid}/records?limit=1`)).total,planned);
    evidence.workbookImport={sheets:real.config.sheets.map(s=>real.sheets[s.index].name),records:planned,milliseconds:Math.round(performance.now()-began),mapping:'Ten plain-text source columns; independent mappings/header rows; isolated module only'};
    console.log(`Real January/February sheets: ${planned} records committed with normal versions in disposable MX`);
  }
  for (const engine of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
    const executablePath = engine === 'chromium' ? '/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome' : engine === 'firefox' ? '/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox' : '/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser = await ({ chromium, firefox, webkit })[engine].launch({ headless: true, executablePath, env: disconnectedBrowserAudio(mx.directory), ...(engine === 'chromium' ? { args: ['--no-sandbox', '--mute-audio'] } : {}) }); browsers.push(browser);
    const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width: 1440, height: 950 } }); activePage = page; const errors = []; page.on('pageerror', e => { errors.push(e.message); console.error(`${engine}: ${e.message}`); });
    await page.addInitScript(tokens => { sessionStorage.setItem('mx_access_token', tokens.access_token); sessionStorage.setItem('mx_refresh_token', tokens.refresh_token); }, owner);
    const origin = `https://127.0.0.1:${mx.port}`;
    await page.goto(`${origin}/#module/${module.uid}`); await page.getByRole('button', { name: 'Module actions', exact: true }).click(); await page.getByRole('menuitem', { name: 'Import records', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Import records', exact: true }); await dialog.waitFor();
    await dialog.locator('input[type=file]').setInputFiles({ name: `${engine}.csv`, mimeType: 'text/csv', buffer: Buffer.from(`Name,Start date,Amount,Status\n${engine}-good,1.5.26,4,Open\n${engine}-bad,1.5.26,broken,Open\n`) }); await dialog.getByRole('button', { name: 'Read spreadsheet', exact: true }).click(); await dialog.getByRole('button', { name: 'Validate and preview', exact: true }).waitFor();
    assert.equal(await dialog.getByRole('combobox', { name: 'Map column A', exact: true }).inputValue(), 'name'); await dialog.getByRole('combobox', { name: 'Date format for Start date', exact: true }).selectOption('mdy-short-dot');
    for (const theme of ['light', 'dark']) for (const width of [1440, 390]) { await page.setViewportSize({ width, height: 950 }); await page.evaluate(t => document.documentElement.dataset.theme = t, theme); await page.screenshot({ path: join(mx.directory, `${engine}-import-mapping-${theme}-${width}.png`) }); assert.ok(await dialog.evaluate(n => n.getBoundingClientRect().right <= innerWidth + 1)); }
    await page.setViewportSize({ width: 1440, height: 950 }); await dialog.getByRole('button', { name: 'Validate and preview', exact: true }).click();
    let lostAck=false;await page.route('**/imports/*/process',async route=>{if(!lostAck){lostAck=true;const response=await route.fetch();assert.ok(response.ok());await route.abort('failed');}else await route.continue();});
    await dialog.getByRole('button', { name: 'Start import', exact: true }).click(); await dialog.getByRole('heading', { name: 'Completed with issues', exact: true }).waitFor();assert.ok(lostAck);await page.unroute('**/imports/*/process'); await dialog.getByRole('button', { name: 'Correct and retry', exact: true }).click();
    const editor = page.locator('.record-dialog'); await editor.getByRole('spinbutton', { name: /^Amount/ }).fill('12'); await editor.getByRole('button', { name: /^Save import row/ }).click(); await editor.waitFor({ state: 'hidden' }); await dialog.getByRole('heading', { name: 'Import complete', exact: true }).waitFor();
    await dialog.getByRole('button', { name: 'Close import', exact: true }).click(); await page.getByRole('button', { name: 'Module actions', exact: true }).click(); await page.getByRole('menuitem', { name: 'Import history', exact: true }).click(); await dialog.getByText(`${engine}.csv`, { exact: true }).click(); await dialog.getByRole('heading', { name: 'Import complete', exact: true }).waitFor();
    await dialog.getByRole('button',{name:'Close import',exact:true}).click();
    await page.goto(`${origin}/#module/${blockedModule.uid}`);await page.getByRole('button',{name:'Module actions',exact:true}).click();await page.getByRole('menuitem',{name:'Import records',exact:true}).click();
    await dialog.locator('input[type=file]').setInputFiles({name:`blocked-${engine}.csv`,mimeType:'text/csv',buffer:Buffer.from('Name,Quantity,Correct quantity\nBlocked one,invalid,1\nBlocked two,invalid,2\n')});
    await dialog.getByRole('button',{name:'Read spreadsheet',exact:true}).click();await dialog.getByRole('note',{name:'Attachments are not imported'}).waitFor();
    await dialog.getByRole('button',{name:'Validate and preview',exact:true}).click();
    const alert=dialog.getByRole('alert',{name:'Import validation problems'});await alert.getByText('No rows can be imported yet',{exact:true}).waitFor();
    assert.ok((await alert.innerText()).includes('Quantity must be an integer'));
    assert.ok(!(await alert.innerText()).includes('FILE ATTACHMENTS requires'));
    assert.equal(await dialog.getByRole('button',{name:'Start import',exact:true}).isDisabled(),true);
    assert.equal(await dialog.locator('.import-result-row').count(),2,'Problem rows appear directly in preview');
    for(const theme of ['light','dark'])for(const width of [1440,390]){await page.setViewportSize({width,height:950});await page.evaluate(t=>document.documentElement.dataset.theme=t,theme);assert.ok(await dialog.locator('.import-body').evaluate(n=>n.scrollWidth<=n.clientWidth+1));await page.screenshot({path:join(mx.directory,`${engine}-import-blocked-${theme}-${width}.png`)});}
    await page.setViewportSize({width:1440,height:950});
    const stale=(await call(blockedBase)).jobs.find(j=>j.filename===`blocked-${engine}.csv`);assert.ok(stale);
    sql(`UPDATE mx_import_rows SET state='failed',errors_json='${oldError}' WHERE job_uid='${stale.uid}' AND state='pending';UPDATE mx_import_jobs SET state='completed' WHERE uid='${stale.uid}';`);
    await dialog.getByRole('button',{name:'Close import',exact:true}).click();await page.getByRole('button',{name:'Module actions',exact:true}).click();await page.getByRole('menuitem',{name:'Import history',exact:true}).click();await dialog.getByText(`blocked-${engine}.csv`,{exact:true}).click();await alert.getByText('No records were saved',{exact:true}).waitFor();
    await page.waitForFunction(()=>document.querySelectorAll('.import-result-row').length===2);await dialog.getByRole('button',{name:'Review mapping and revalidate',exact:true}).click();await dialog.getByRole('heading',{name:'1. Select sheets · 2. Match columns',exact:true}).waitFor();await dialog.getByRole('combobox',{name:'Map column B',exact:true}).selectOption('');await dialog.getByRole('combobox',{name:'Map column C',exact:true}).selectOption('quantity');
    await dialog.getByRole('button',{name:'Validate and preview',exact:true}).click();await dialog.getByRole('button',{name:'Start import',exact:true}).waitFor();await page.waitForFunction(()=>!document.querySelector('.import-warning[role=alert]'));
    assert.equal(await dialog.getByRole('button',{name:'Start import',exact:true}).isEnabled(),true);await dialog.getByRole('button',{name:'Start import',exact:true}).click();await dialog.getByRole('heading',{name:'Import complete',exact:true}).waitFor();
    assert.equal(await dialog.getByRole('button',{name:'Pause import',exact:true}).count(),0,'Completed imports must not remain in the running state');
    await dialog.getByRole('button',{name:'Close import',exact:true}).click();await page.getByRole('button',{name:'+ New legacy record',exact:true}).click();
    const normal=page.locator('.record-dialog');await normal.getByRole('textbox',{name:/^Name/}).fill('Normal form still needs files');await normal.getByRole('spinbutton',{name:/^Quantity/}).fill('1');await normal.getByRole('button',{name:/^Save Legacy record$/i}).click();
    await normal.getByText('FILE ATTACHMENTS requires at least one file.',{exact:true}).waitFor();await normal.getByRole('button',{name:'Clear form',exact:true}).click();await normal.getByRole('button',{name:'Cancel',exact:true}).click();
    assert.deepEqual(errors, []); evidence.browsers.push(engine); console.log(`${engine}: mapping, real progress, correction, history, light/dark and mobile passed`); await browser.close();
  }
  await writeFile(join(mx.directory, 'import-results.json'), JSON.stringify({ passed: true, ...evidence }, null, 2));
} catch (e) { if (activePage && !activePage.isClosed()) await activePage.screenshot({ path: join(mx.directory, 'import-failure.png') }).catch(() => {}); throw e; }
finally { await Promise.all(browsers.map(b => b.close().catch(() => {}))); await mx.close(); }
