// Record attachment Office viewer: real MX/N1 fixture and the bundled engine.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join, isAbsolute } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { request as httpsRequest } from 'node:https';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';
import { officeFixture } from './office-fixtures.mjs';

assert.ok(isAbsolute(process.env.MX_TEST_ARTIFACTS || ''), 'Set a disposable artifact root');
assert.ok(!process.env.MX_TEST_N1_URL, 'Never use live N1');
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
const proxy = createServer((incoming, outgoing) => {
  const upstream = httpsRequest({ hostname: '127.0.0.1', port: mx.port, path: incoming.url, method: incoming.method, headers: incoming.headers, rejectUnauthorized: false }, response => { outgoing.writeHead(response.statusCode, response.headers); response.pipe(outgoing); });
  upstream.on('error', () => { outgoing.writeHead(502); outgoing.end(); }); incoming.pipe(upstream);
});
await new Promise(resolve => proxy.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${proxy.address().port}`, browsers = [], results = { browsers: {} };
const call = async (path, user, method = 'GET', data, expected = [200]) => (await mx.call(path, user?.access_token, method, data, expected)).body;
const until = async (check, label, timeout = 120000) => { const end = Date.now() + timeout; while (Date.now() < end) { if (await check()) return; await new Promise(resolve => setTimeout(resolve, 100)); } throw Error(label); };
let inspected;
try {
  console.log(`Record Office artifacts: ${mx.directory}`);
  const password = `Disposable!${randomUUID()}`;
  await call('/mx/v1/auth/create', { access_token: mx.signupKey }, 'POST', { email: 'owner@recordoffice.test', name: 'Owner', password }, [201]);
  const owner = await call('/mx/v1/auth/authenticate', null, 'POST', { email: 'owner@recordoffice.test', password });
  const users = [];
  for (const name of ['Reader', 'Outsider']) {
    const email = `${name.toLowerCase()}@recordoffice.test`;
    await call('/mx/v1/user/create', owner, 'POST', { email, name, password, access_level: 2 }, [201]);
    users.push(await call('/mx/v1/auth/authenticate', null, 'POST', { email, password }));
  }
  const [reader, outsider] = users;
  const module = (await call('/mx/v1/admin/modules', owner, 'POST', { name: 'Office records', singular_name: 'Office record' }, [201])).module;
  for (const [key, type, config] of [['name', 'text', {}], ['files', 'attachments', { multiple: true }], ['private_files', 'attachments', { access_scope: 'administrators', multiple: true }]]) {
    await call(`/mx/v1/admin/modules/${module.uid}/fields`, owner, 'POST', { key, label: key === 'name' ? 'Name' : key === 'files' ? 'Documents' : 'Private documents', field_type: type, config, table_visible: true }, [201]);
  }
  const fields = (await call(`/mx/v1/modules/${module.uid}/schema`, owner)).fields;
  for (const user of users) {
    const access = await call(`/mx/v1/admin/accounts/${user.uid}/modules`, owner);
    await call(`/mx/v1/admin/accounts/${user.uid}/modules`, owner, 'PUT', { revision: access.revision, grants: [{ module_uid: module.uid, can_read: user === reader, can_create: false, can_update: false, can_delete: false, can_configure: false, can_report: false, can_attachments: user === reader }] });
  }
  let record = await call(`/mx/v1/modules/${module.uid}/records`, owner, 'POST', { values: { name: 'An Office record' } }, [201]);
  const recordPath = `/mx/v1/modules/${module.uid}/records/${record.uid}`;
  const upload = async (key, name, bytes) => {
    const response = await mx.api.post(`/mx/v1/records/${record.uid}/attachments/fields/${fields.find(field => field.key === key).uid}`, { headers: { Authorization: `Bearer ${owner.access_token}` }, multipart: { base_revision: String(record.revision), files: { name, mimeType: 'application/octet-stream', buffer: bytes } } });
    assert.equal(response.status(), 201, await response.text()); record = await call(recordPath, owner);
  };
  await upload('files', 'Module-letter.docx', officeFixture('.docx'));
  await upload('files', 'Module-budget.xlsx', officeFixture('.xlsx'));
  await upload('files', 'Module-notes.txt', Buffer.from('Visible text preview'));
  await upload('private_files', 'Administrator-only.docx', officeFixture('.docx'));
  const original = structuredClone(record), history = await call(`${recordPath}/versions`, owner);
  const secret = record.attached_files.find(file => file.file_name === 'Administrator-only.docx');
  const publicFiles = record.attached_files.filter(file => file.attachment_field_uid === fields.find(field => field.key === 'files').uid);
  await call(`/mx/v1/records/${record.uid}/attachments/${secret.uid}/preview-ticket`, reader, 'POST', undefined, [403, 404]);
  await call(`/mx/v1/records/${record.uid}/attachments/${publicFiles[0].uid}/preview-ticket`, outsider, 'POST', undefined, [403, 404]);

  for (const engine of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
    const fallback = (process.env.MX_TEST_OFFICE_FALLBACK_BROWSERS || '').split(',').includes(engine);
    const executablePath = engine === 'brave' ? '/usr/bin/brave-browser' : engine === 'chromium' ? '/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome' : engine === 'firefox' ? '/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox' : '/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser = await (engine === 'brave' ? chromium : { chromium, firefox, webkit }[engine]).launch({ headless: true, executablePath, env: disconnectedBrowserAudio(mx.directory), ...(['chromium', 'brave'].includes(engine) ? { args: ['--no-sandbox', '--mute-audio'] } : {}) }); browsers.push(browser);
    const context = await browser.newContext({ viewport: { width: 1440, height: 950 }, acceptDownloads: true });
    await context.addInitScript(({ tokens, origin }) => { if (location.origin !== origin) return; sessionStorage.setItem('mx_access_token', tokens.access_token); sessionStorage.setItem('mx_refresh_token', tokens.refresh_token); }, { tokens: reader, origin });
    const errors = [], writes = [], conversions = [], external = [];
    context.on('page', page => page.on('pageerror', error => errors.push(error.message)));
    await context.route('**/*', route => {
      const request = route.request(), url = new URL(request.url());
      if (['http:', 'https:'].includes(url.protocol) && url.origin !== origin) { external.push(url.href); return route.abort(); }
      if (/\/attachments\/[^/]+\/preview$/.test(url.pathname)) conversions.push(url.pathname);
      if (['PATCH', 'PUT', 'DELETE'].includes(request.method()) && /\/records|\/drive/.test(url.pathname)) writes.push(url.pathname);
      if (request.method() === 'POST' && /\/drive\/uploads|\/attachments\/fields\//.test(url.pathname)) writes.push(url.pathname);
      return route.continue();
    });
    const page = await context.newPage(); inspected = page;
    await page.goto(`${origin}/#module/${module.uid}?record=${record.uid}`);
    const form = page.getByRole('dialog', { name: 'View Office record', exact: true }); await form.waitFor();
    assert.equal(await form.getByRole('button', { name: 'Preview Administrator-only.docx', exact: true }).count(), 0, 'Hidden Office fields are not exposed in the record');
    for (const extension of ['.docx', '.xlsx']) {
      const file = publicFiles.find(file => file.file_name.endsWith(extension));
      const [viewer] = await Promise.all([context.waitForEvent('page'), form.getByRole('button', { name: `Preview ${file.file_name}`, exact: true }).first().click()]);
      inspected = viewer; await viewer.waitForURL(/record=.*attachment=/);
      if (fallback) await viewer.getByRole('region', { name: 'Office loading progress', exact: true }).getByText('cannot provide WebGL in an Office worker', { exact: false }).waitFor();
      else await until(async () => { if (await viewer.locator('.office-loading.failed').count()) throw Error(await viewer.locator('.office-loading.failed').innerText()); return await viewer.locator('#qtcanvas').getAttribute('data-native-focused') === 'true'; }, 'The actual record attachment opens in Office');
      assert.equal(await viewer.getByRole('button', { name: 'Save to MX', exact: true }).count(), 0);
      const returned = viewer.getByRole('link', { name: 'Return to record', exact: true });
      assert.ok((await returned.getAttribute('href')).includes(module.uid));
      const [download] = await Promise.all([viewer.waitForEvent('download'), viewer.locator('main[data-office-source="record"] > header').getByRole('button', { name: 'Download original', exact: true }).click()]);
      const chunks = [], stream = await download.createReadStream(); for await (const chunk of stream) chunks.push(chunk); assert.deepEqual(Buffer.concat(chunks), officeFixture(extension));
      await viewer.screenshot({ path: join(mx.directory, `${engine}-record-office-${extension.slice(1)}.png`) });
      await returned.click(); await viewer.getByRole('dialog', { name: 'View Office record', exact: true }).waitFor();
      await viewer.close(); inspected = page;
      assert.ok(await form.isVisible(), 'The original record stays open');
    }
    // A popup-blocked Office viewer must have a usable fallback, not a dead conversion.
    await page.evaluate(() => { window.__realOpen = window.open; window.open = () => null; });
    await form.getByRole('button', { name: 'Preview Module-budget.xlsx', exact: true }).first().click();
    const preview = page.getByRole('dialog', { name: 'Module-budget.xlsx', exact: true }); await preview.getByRole('button', { name: 'Open in MX Office', exact: true }).waitFor();
    await preview.getByText('The browser blocked the viewer tab.', { exact: false }).waitFor();
    await preview.getByRole('button', { name: 'Close preview', exact: true }).click(); await page.evaluate(() => window.open = window.__realOpen);
    await form.getByRole('button', { name: 'Preview Module-notes.txt', exact: true }).first().click();
    const text = page.getByRole('dialog', { name: 'Module-notes.txt', exact: true }); await text.locator('pre').waitFor(); assert.equal(await text.locator('pre').innerText(), 'Visible text preview');
    await text.getByRole('button', { name: 'Close preview', exact: true }).click();
    assert.deepEqual(await call(recordPath, owner), original); assert.deepEqual(await call(`${recordPath}/versions`, owner), history);
    assert.deepEqual(writes, []); assert.deepEqual(conversions, []); assert.deepEqual(external, []); assert.deepEqual(errors, []);
    results.browsers[engine] = { passed: true, rendering: fallback ? 'Known worker-WebGL limitation: error/fallback/download verified, not rendering' : 'Actual DOCX/XLSX rendering', checks: ['Stored attachments open directly from the module', 'Record/module return context', 'Original bytes unchanged', 'Private field and outsider denial', 'No writable Drive identity', 'No record/history changes or server PDF conversion', 'Blocked popup fallback and normal text preview'] };
    console.log(`${engine}: record Office ${fallback ? 'graphics fallback/download' : 'rendering'} passed`);
    await context.close(); await browser.close();
  }
  results.passed = true;
} catch (error) {
  results.error = error.stack; if (inspected && !inspected.isClosed()) await inspected.screenshot({ path: join(mx.directory, 'record-office-failure.png') }).catch(() => {}); throw error;
} finally {
  await writeFile(join(mx.directory, 'record-office-results.json'), JSON.stringify(results, null, 2));
  await Promise.all(browsers.map(browser => browser.close().catch(() => {}))); await mx.close(); await new Promise(resolve => proxy.close(resolve));
}
