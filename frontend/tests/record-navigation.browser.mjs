// Release regression: empty setup, paged/virtual layout parity and cheap mode changes.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join, isAbsolute} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {chromium, firefox, webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
assert.ok(isAbsolute(process.env.MX_TEST_ARTIFACTS || ''));
assert.ok(!process.env.MX_TEST_N1_URL);
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
const browsers = [], results = [];
let page, module;
async function until(check, label) {
  const end = Date.now() + 20000;
  while (Date.now() < end) { if (await check()) return; await new Promise(resolve => setTimeout(resolve, 50)); }
  throw Error(`Timeout: ${label}`);
}
try {
  console.log(`Record navigation artifacts: ${mx.directory}`);
  const credentials = {name: 'Navigation tester', email: 'navigation@test.invalid', password: `Test-only!${randomUUID()}`};
  await mx.call('/mx/v1/auth/create', mx.signupKey, 'POST', credentials, [201]);
  const user = (await mx.call('/mx/v1/auth/authenticate', null, 'POST', credentials)).body;
  const call = async (path, method = 'GET', data, expected = [200]) => (await mx.call(path, user.access_token, method, data, expected)).body;
  assert.deepEqual((await call('/mx/v1/modules')).modules, [], 'Fresh installation has no starter module');
  await mx.restart();
  assert.deepEqual((await call('/mx/v1/modules')).modules, [], 'Restart does not invent a default module');
  for (const engine of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
    const executablePath = engine === 'chromium' ? '/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome'
      : engine === 'firefox' ? '/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox'
      : '/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser = await ({chromium, firefox, webkit})[engine].launch({headless: true, executablePath,
      env: disconnectedBrowserAudio(mx.directory), ...(engine === 'chromium' ? {args: ['--no-sandbox', '--mute-audio']} : {})});
    browsers.push(browser);
    page = await browser.newPage({ignoreHTTPSErrors: true, viewport: {width: 1440, height: 900}});
    const errors = [], schemaRequests = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('request', request => { if (/\/schema(?:\?|$)/.test(new URL(request.url()).pathname)) schemaRequests.push(request.url()); });
    await page.addInitScript(tokens => {sessionStorage.setItem('mx_access_token', tokens.access_token);sessionStorage.setItem('mx_refresh_token', tokens.refresh_token);}, user);
    const origin = `https://127.0.0.1:${mx.port}`;
    if (!module) {
      await page.goto(`${origin}/#records`, {waitUntil: 'networkidle'});
      await page.getByRole('heading', {name: 'Create your first module', exact: true}).waitFor();
      await page.getByRole('button', {name: 'Open Modules & fields', exact: true}).click();
      await page.getByRole('button', {name: 'New module', exact: true}).click();
      const dialog = page.getByRole('dialog');
      await dialog.getByLabel('Module name', {exact: true}).fill('Temporary setup');
      await dialog.getByLabel('Singular name', {exact: true}).fill('Entry');
      await dialog.getByRole('button', {name: 'Create new module', exact: true}).click();
      await dialog.waitFor({state: 'hidden'});
      await page.getByRole('button', {name: 'Module details', exact: true}).click();
      await page.locator('.module-builder').getByRole('button', {name: 'Delete', exact: true}).click();
      const deletion = page.getByRole('alertdialog');
      await deletion.getByLabel('Type Temporary setup to confirm').fill('Temporary setup');
      await deletion.getByLabel('Your administrator password').fill(credentials.password);
      await deletion.getByRole('button', {name: 'Authenticate and delete', exact: true}).click();
      await deletion.waitFor({state: 'hidden'});
      assert.deepEqual((await call('/mx/v1/modules')).modules, [], 'Deleting the last module returns to a blank installation');
      assert.equal(await page.getByRole('alertdialog').count(), 0, 'No nonexistent default-module error');
      await page.getByRole('button', {name: 'New module', exact: true}).click();
      await dialog.getByLabel('Module name', {exact: true}).fill('Navigation records');
      await dialog.getByLabel('Singular name', {exact: true}).fill('Entry');
      await dialog.getByRole('button', {name: 'Create new module', exact: true}).click();
      await dialog.waitFor({state: 'hidden'});
      const modules = (await call('/mx/v1/modules')).modules;
      assert.equal(modules.length, 1, 'Only the operator-created module exists');
      module = modules[0];
      assert.equal((await call(`/mx/v1/modules/${module.uid}/schema`)).fields.length, 0, 'No business fields are seeded');
      const fields = [{key: 'date', label: 'Date', field_type: 'date'}, {key: 'number', label: 'Number', field_type: 'integer'},
        {key: 'description', label: 'Description', field_type: 'long_text'},
        ...Array.from({length: 8}, (_, index) => ({key: `text_${index}`, label: `Custom detail ${index + 1}`, field_type: 'text'}))];
      for (const field of fields) await call(`/mx/v1/admin/modules/${module.uid}/fields`, 'POST', {...field, table_visible: true}, [201]);
      for (let offset = 0; offset < 140; offset += 8) {
        await Promise.all(Array.from({length: Math.min(8, 140 - offset)}, (_, index) => call(`/mx/v1/modules/${module.uid}/records`, 'POST', {
          operation_uid: randomUUID(), values: {date: '2026-10-16', number: offset + index, description: 'The full text remains readable with stable widths.',
            ...Object.fromEntries(Array.from({length: 8}, (_, index) => [`text_${index}`, 'Operator-defined information']))}
        }, [201])));
      }
    }
    const slowInitial = async route => {if (!new URL(route.request().url()).searchParams.has('window')) await new Promise(resolve => setTimeout(resolve, 800));await route.continue().catch(() => {});};
    await page.route(`**/mx/v1/modules/${module.uid}/records?*`, slowInitial);
    await page.goto(`${origin}/#module/${module.uid}`);
    const table = page.locator('.records-table');
    const navigation = page.getByRole('combobox', {name: 'Record navigation', exact: true});
    const bar = page.getByRole('scrollbar', {name: 'Scroll record columns', exact: true});
    assert.equal(await navigation.inputValue(), 'false', 'Pages is the fresh default');
    await navigation.selectOption({label: 'Continuous scroll'});
    await until(() => table.getAttribute('aria-busy').then(value => value === 'false'), 'early switch before the first page arrives');
    assert.ok(await table.locator('thead th').nth(3).evaluate(node => node.getBoundingClientRect().width >= 399), 'Early switch uses readable long-text widths, not placeholder widths');
    await page.unroute(`**/mx/v1/modules/${module.uid}/records?*`, slowInitial);
    await navigation.selectOption({label: 'Pages'});
    await until(() => table.getAttribute('aria-busy').then(value => value === 'false'), 'initial page');
    assert.equal(await table.locator('[data-record-uid]').count(), 50);
    const widths = await table.locator('thead th').evaluateAll(nodes => nodes.map(node => node.getBoundingClientRect().width));
    await table.evaluate(node => node.scrollLeft = 450);
    await until(() => table.evaluate(node => node.scrollLeft >= 449), 'initial horizontal position');
    const initialSchemas = schemaRequests.length;
    const started = performance.now();
    await navigation.selectOption({label: 'Continuous scroll'});
    await until(() => table.getAttribute('aria-busy').then(value => value === 'false'), 'continuous view');
    const elapsed = Math.round(performance.now() - started);
    assert.equal(schemaRequests.length, initialSchemas, 'Changing view does not reload schema');
    const continuousWidths = await table.locator('thead th').evaluateAll(nodes => nodes.map(node => node.getBoundingClientRect().width));
    continuousWidths.forEach((width, index) => assert.ok(Math.abs(width - widths[index]) < 2, `Column ${index} keeps its paged width`));
    assert.ok(Math.abs(await table.evaluate(node => node.scrollLeft) - 450) < 2, 'Switch preserves horizontal position');
    await until(() => bar.isVisible(), 'consistent horizontal scrollbar in continuous mode');
    const box = await table.boundingBox();
    assert.ok(box.height > 500 && box.y + box.height <= 901, 'Continuous table uses the remaining viewport');
    await bar.focus();await page.keyboard.press('Home');await page.keyboard.press('ArrowRight');
    await until(() => table.evaluate(node => Math.abs(node.scrollLeft - 64) < 2), 'continuous scrollbar keyboard control');
    for (let index = 0; index < 2; index++) {
      await table.evaluate(node => node.scrollTop = node.scrollHeight - node.clientHeight);
      await page.waitForTimeout(250);
    }
    assert.ok(await table.locator('[data-record-uid]').count() < 75, 'Continuous DOM is bounded');
    const slow = async route => {await new Promise(resolve => setTimeout(resolve, 400));await route.continue().catch(() => {});};
    await page.route(`**/mx/v1/modules/${module.uid}/records?*`, slow);
    await navigation.selectOption({label: 'Pages'});
    assert.ok(await table.locator('[data-record-uid]').count() <= 50, 'Switch never mounts the entire continuous cache');
    await until(() => table.getAttribute('aria-busy').then(value => value === 'false'), 'paged view after delayed request');
    for (const label of ['Continuous scroll', 'Pages', 'Continuous scroll', 'Pages']) await navigation.selectOption({label});
    await until(() => table.getAttribute('aria-busy').then(value => value === 'false'), 'latest rapid view selection');
    await page.unroute(`**/mx/v1/modules/${module.uid}/records?*`, slow);
    assert.equal(await table.evaluate(node => node.classList.contains('continuous-records')), false);
    assert.ok(await table.locator('[data-record-uid]').count() <= 50);
    assert.equal(schemaRequests.length, initialSchemas, 'Rapid changes do not refetch schema');
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
    await page.screenshot({path: join(mx.directory, `${engine}-record-navigation.png`)});
    assert.deepEqual(errors, []);
    results.push({engine, passed: true, mode_switch_milliseconds: elapsed, schema_refetches: 0, widths_and_scroll_preserved: true, bounded_dom: true});
    await browser.close();
  }
  await writeFile(join(mx.directory, 'record-navigation-results.json'), JSON.stringify({passed: true, blank_deployment: true, results}, null, 2));
  console.log(JSON.stringify({passed: true, directory: mx.directory, results}));
} catch (error) {
  if (page && !page.isClosed()) await page.screenshot({path: join(mx.directory, 'record-navigation-failure.png')}).catch(() => {});
  throw error;
} finally {
  await Promise.all(browsers.map(browser => browser.close().catch(() => {})));
  await mx.close();
}
