// Production UI + disposable MX/SQLite. No deployment writes or live N1.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join, isAbsolute } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { DatabaseSync } from 'node:sqlite';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';

assert.ok(isAbsolute(process.env.MX_TEST_ARTIFACTS || ''), 'Set MX_TEST_ARTIFACTS explicitly');
assert.ok(!process.env.MX_TEST_N1_URL, 'Field tests must not use live N1');
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
const browsers = [];
const results = { api: [], browsers: {} };
let owner;
const call = async (path, method='GET', data, expected=[200]) => (await mx.call(path, owner?.access_token, method, data, expected)).body;
const module = async name => (await call('/mx/v1/admin/modules','POST',{name,singular_name:'Record'},[201])).module;
const field = (uid,key,label='Issuer',type='text',expected=[201]) => call(`/mx/v1/admin/modules/${uid}/fields`,'POST',{key,label,field_type:type},expected);
try {
  console.log(`Field error artifacts: ${mx.directory}`);
  const credentials = { email:'field-errors@test.invalid', name:'Field error tester', password:`Test-only!${randomUUID()}` };
  await mx.call('/mx/v1/auth/create',mx.signupKey,'POST',credentials,[201]);
  owner = (await mx.call('/mx/v1/auth/authenticate',null,'POST',credentials)).body;
  const first = await module('Field errors');
  const archived = await field(first.uid,'issuer');
  const original = await call(`/mx/v1/modules/${first.uid}/records`,'POST',{values:{issuer:'Keep historical data'}},[201]);
  await call(`/mx/v1/admin/schema/fields/${archived.uid}`,'PUT',{active:false});
  await field(first.uid,'issuer_2','Another issuer');
  const before = (await call(`/mx/v1/modules/${first.uid}/schema`)).revision;
  const duplicate = await field(first.uid,'ISSUER','Replacement issuer','date',[409]);
  assert.equal(duplicate.error,'field_key_conflict');
  assert.deepEqual(duplicate.field_key_conflict.field,{uid:archived.uid,key:'issuer',label:'Issuer',field_type:'text',active:false,module_uid:first.uid,module_name:first.name});
  assert.equal(duplicate.field_key_conflict.suggested_key,'issuer_3');
  assert.ok(duplicate.response.includes('archived'));
  assert.equal((await call(`/mx/v1/modules/${first.uid}/schema`)).revision,before,'Rejected creation changes no schema');
  await field(first.uid,duplicate.field_key_conflict.suggested_key,'Replacement issuer','date');
  const activeDuplicate = await field(first.uid,'issuer_3','Duplicate replacement','text',[409]);
  assert.equal(activeDuplicate.field_key_conflict.field.active,true);
  const other = await module('Independent module');
  await field(other.uid,'issuer');
  const long = 'a'.repeat(64);
  await field(first.uid,long,'Long key');
  const truncated = await field(first.uid,'a'.repeat(63)+'b','Truncated storage collision','text',[409]);
  assert.equal(truncated.field_key_conflict.field.key,long);
  await field(first.uid,truncated.field_key_conflict.suggested_key,'Corrected long key');
  // Fresh installations have no implicit legacy module. Never manufacture one
  // just to satisfy the previous dropdown-era test assumption.
  await call('/mx/v1/admin/schema/fields','POST',{key:'reserved_default',label:'No implicit module',field_type:'text'},[404]);
  // The availability check and creation share one immediate write transaction.
  const concurrent = await Promise.all(Array.from({length:8},() => mx.call(`/mx/v1/admin/modules/${first.uid}/fields`,owner.access_token,'POST',{key:'race_key',label:'Race field',field_type:'text'},[201,409])));
  assert.equal(concurrent.filter(result=>result.status===201).length,1);
  assert.equal(concurrent.filter(result=>result.status===409&&result.body.error==='field_key_conflict').length,7);
  const schema = await call(`/mx/v1/modules/${first.uid}/schema`);
  assert.equal(schema.fields.filter(f=>f.key==='race_key').length,1);
  const database = new DatabaseSync(join(mx.directory,'test.db'),{readOnly:true});
  assert.equal(database.prepare('SELECT value_text FROM mx_record_values WHERE record_uid=? AND field_uid=?').get(original.uid,archived.uid).value_text,'Keep historical data');
  database.close();
  results.api.push('Archived and active/case-insensitive keys, available suggestions, per-module isolation, legacy long-key storage collisions, no implicit starter module, eight concurrent creates (one winner), rejected schema unchanged, archived history preserved');

  for (const engine of (process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')) {
    const current = await module(`Popup ${engine}`);
    const source = await field(current.uid,'issuer');
    await call(`/mx/v1/admin/schema/fields/${source.uid}`,'PUT',{active:false});
    await field(current.uid,'issuer_2','Already reserved suffix');
    const editable = await field(current.uid,'editable','Editable field');
    const executablePath = engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser = await ({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});
    browsers.push(browser);
    const page = await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1440,height:900}});
    const errors = []; page.on('pageerror',error=>errors.push(error.message));
    await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
    await page.goto(`https://127.0.0.1:${mx.port}/#admin`);
    await page.getByRole('button',{name:'Modules & fields',exact:true}).click();
    await page.getByRole('button',{name:`Configure ${current.name}`,exact:true}).click();
    const form = page.getByRole('form',{name:'Add field',exact:true});
    await form.getByRole('textbox',{name:'Label',exact:true}).fill('Issuer');
    await form.getByRole('combobox',{name:'Type',exact:true}).selectOption('date');
    assert.ok((await form.getByRole('status').innerText()).includes('archived'),'Reserved key explained at input before submit');
    await form.getByRole('button',{name:`Add field to ${current.name}`,exact:true}).click();
    const popup = page.getByRole('alertdialog',{name:'Could not create field',exact:true}); await popup.waitFor();
    assert.ok((await popup.innerText()).includes('Archived · history preserved'));
    assert.ok((await popup.innerText()).includes('text'));
    assert.ok(await popup.evaluate(node=>node.contains(document.activeElement)),'Focus inside popup');
    await popup.locator('button').last().focus(); await page.keyboard.press('Tab');
    assert.ok(await popup.locator('button').first().evaluate(node=>node===document.activeElement),'Tab trapped');
    await page.keyboard.press('Shift+Tab'); assert.ok(await popup.locator('button').last().evaluate(node=>node===document.activeElement),'Reverse Tab trapped');
    for (const [theme,width] of [['light',1440],['dark',1440],['dark',390],['light',320]]) {
      await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme); await page.setViewportSize({width,height:800});
      const bounds = await popup.boundingBox();
      assert.ok(bounds.x>=7&&bounds.y>=7&&bounds.x+bounds.width<=width-7&&bounds.y+bounds.height<=793,'Popup fits viewport');
      assert.ok(await popup.evaluate(node=>node.scrollWidth<=node.clientWidth+1),'No horizontal popup overflow');
      await page.screenshot({path:join(mx.directory,`${engine}-field-error-${theme}-${width}.png`)});
    }
    await page.keyboard.press('Escape'); await popup.waitFor({state:'hidden'});
    assert.equal(await form.getByRole('textbox',{name:'Label',exact:true}).inputValue(),'Issuer');
    assert.equal(await form.getByRole('combobox',{name:'Type',exact:true}).inputValue(),'date');
    assert.ok(await form.getByRole('textbox',{name:'Field key (optional)',exact:true}).evaluate(node=>node===document.activeElement),'Return to relevant input, not top');
    assert.ok((await form.getByRole('alert').innerText()).includes('archived'),'Error remains beside draft');
    assert.equal(await page.locator('.notice.error').evaluateAll(nodes=>nodes.filter(node=>!node.closest('form[aria-label="Add field"]')).length),0,'No hidden top-of-page banner');
    await page.setViewportSize({width:1440,height:900});
    await form.getByRole('button',{name:`Add field to ${current.name}`,exact:true}).click(); await popup.waitFor();
    await popup.getByRole('button',{name:'View existing field',exact:true}).click(); await popup.waitFor({state:'hidden'});
    const card = page.locator(`[data-schema-field="${source.uid}"]`);
    await page.waitForFunction(uid=>document.activeElement?.getAttribute('data-schema-field')===uid,source.uid);
    assert.ok(await card.getByRole('button',{name:'Restore',exact:true}).isVisible());
    await form.getByRole('button',{name:`Add field to ${current.name}`,exact:true}).click(); await popup.waitFor();
    await popup.getByRole('button',{name:'Use key issuer_3',exact:true}).click(); await popup.waitFor({state:'hidden'});
    assert.equal(await form.getByRole('textbox',{name:'Field key (optional)',exact:true}).inputValue(),'issuer_3');
    assert.equal(await form.getByRole('combobox',{name:'Type',exact:true}).inputValue(),'date');
    assert.equal(await form.getByRole('alert').count(),0);
    // Another administrator takes the suggestion before the operator submits it.
    await field(current.uid,'issuer_3','Concurrent reservation');
    await form.getByRole('button',{name:`Add field to ${current.name}`,exact:true}).click(); await popup.waitFor();
    await popup.getByRole('button',{name:'Use key issuer_3_2',exact:true}).click(); await popup.waitFor({state:'hidden'});
    await form.getByRole('button',{name:`Add field to ${current.name}`,exact:true}).click();
    await page.waitForFunction(()=>[...document.querySelectorAll('.schema-card small')].some(node=>node.textContent==='issuer_3_2 · date'));
    const saved = (await call(`/mx/v1/modules/${current.uid}/schema`)).fields;
    assert.equal(saved.find(f=>f.uid===source.uid).active,false);
    assert.equal(saved.find(f=>f.key==='issuer_3_2').field_type,'date');
    // Invalid relationship config is a popup too, not a duplicate-key-only fix.
    await form.getByRole('textbox',{name:'Label',exact:true}).fill('Broken relationship');
    await form.getByRole('combobox',{name:'Type',exact:true}).selectOption('relationship');
    await form.getByRole('button',{name:`Add field to ${current.name}`,exact:true}).click(); await popup.waitFor();
    assert.ok((await popup.innerText()).includes('Linked module'));
    await popup.getByRole('button',{name:'Back to settings',exact:true}).click(); await popup.waitFor({state:'hidden'});
    assert.equal(await form.getByRole('textbox',{name:'Label',exact:true}).inputValue(),'Broken relationship');
    // An existing field save error gets the same immediate popup and keeps edits.
    const editCard = page.locator(`[data-schema-field="${editable.uid}"]`);
    await editCard.getByRole('textbox',{name:'Label',exact:true}).fill('Unsaved label');
    await page.route(`**/schema/fields/${editable.uid}`,route=>route.fulfill({status:400,contentType:'application/json',body:JSON.stringify({response:'Specific configuration error for this field'})}));
    await editCard.getByRole('button',{name:'Save field',exact:true}).click();
    const savePopup = page.getByRole('alertdialog',{name:'Could not save Unsaved label',exact:true}); await savePopup.waitFor();
    assert.ok((await savePopup.innerText()).includes('Specific configuration error'));
    await savePopup.getByRole('button',{name:'Back to settings',exact:true}).click(); await savePopup.waitFor({state:'hidden'});
    assert.equal(await editCard.getByRole('textbox',{name:'Label',exact:true}).inputValue(),'Unsaved label');
    await page.waitForFunction(uid=>document.activeElement?.getAttribute('data-schema-field')===uid,editable.uid);
    assert.deepEqual(errors,[]);
    results.browsers[engine] = 'Visible archived-key popup; four light/dark/desktop/mobile views; keyboard focus/escape; draft preserved; exact archived-field jump; available-key action; concurrent suggestion recheck; successful new datatype; relationship and existing-field errors';
    console.log(`${engine}: field errors passed`);
    await page.close(); await browser.close();
  }
  await writeFile(join(mx.directory,'field-errors-results.json'),JSON.stringify(results,null,2));
  console.log(JSON.stringify(results,null,2));
} catch(error) {
  for(const [i,browser] of browsers.entries()) for(const context of browser.contexts()) for(const page of context.pages()) await page.screenshot({path:join(mx.directory,`field-errors-failure-${i}.png`)}).catch(()=>{});
  throw error;
} finally { await Promise.all(browsers.map(browser=>browser.close().catch(()=>{}))); await mx.close(); }
