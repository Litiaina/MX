// Focused production-UI loading checks. Real pinned Office engine, MX/SQLite,
// isolated N1 fixture and loopback-only traffic; no physical audio devices.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { writeFile, readFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { request as httpsRequest } from 'node:https';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { officeFixture, unzip, zip } from './office-fixtures.mjs';

if (process.env.MX_TEST_N1_URL) throw new Error('Office loading tests require the isolated N1 fixture.');
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
const call = async (path, user, method = 'GET', data, expected = [200]) => (await mx.call(path, user?.access_token, method, data, expected)).body;
const base = '/mx/v1/drive', results = [], browsers = [], unblock = [];
let transfer;
const proxy = createServer((incoming, outgoing) => {
  const upstream = httpsRequest({ hostname: '127.0.0.1', port: mx.port, path: incoming.url, method: incoming.method, headers: incoming.headers, rejectUnauthorized: false }, response => {
    outgoing.writeHead(response.statusCode, response.headers);
    if (transfer && incoming.url.startsWith(transfer.path) && response.statusCode === 200) {
      const held = transfer; transfer = undefined;
      response.once('data', chunk => {
        const split = Math.max(1, Math.floor(chunk.length / 2));
        outgoing.write(chunk.subarray(0, split)); response.pause(); held.started();
        held.gate.then(() => { outgoing.write(chunk.subarray(split)); response.pipe(outgoing); response.resume(); });
      });
    } else response.pipe(outgoing);
  });
  upstream.on('error', () => { outgoing.writeHead(502); outgoing.end(); }); incoming.pipe(upstream);
});
await new Promise(resolve => proxy.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${proxy.address().port}`;
function gate() { let release; const promise = new Promise(resolve => release = resolve); unblock.push(release); return { promise, release }; }
async function upload(user, bytes) {
  const operation = randomUUID(), file_name = 'Loading test.docx';
  await call(`${base}/uploads`, user, 'POST', { operation_uid: operation, file_name, mime_type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', size: bytes.length, last_modified: 1 });
  for (let attempt = 0; attempt < 3; attempt++) {
    const response = await mx.api.put(`${base}/uploads/${operation}/parts/0`, { headers: { Authorization: `Bearer ${user.access_token}`, 'Content-Type': 'application/octet-stream' }, data: bytes });
    if (response.status() === 502 && attempt < 2) continue;
    assert.equal(response.status(), 200, await response.text()); break;
  }
  return call(`${base}/uploads/${operation}/finish`, user, 'POST');
}
async function until(check, label, timeout = 120000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) { if (await check()) return; await new Promise(resolve => setTimeout(resolve, 100)); }
  throw new Error(`Timeout: ${label}`);
}
let inspected;
try {
  console.log(`Office loading artifacts: ${mx.directory}`);
  const password = `Test-only!${randomUUID()}`;
  await call('/mx/v1/auth/create', { access_token: mx.signupKey }, 'POST', { name: 'Loading tester', email: 'loading@test.invalid', password }, [201]);
  const user = await call('/mx/v1/auth/authenticate', null, 'POST', { email: 'loading@test.invalid', password });
  const logo = await mx.api.post('/mx/v1/admin/deployment-logo', {headers:{Authorization:`Bearer ${user.access_token}`},multipart:{logo:{name:'configured-logo.png',mimeType:'image/png',buffer:await readFile(fileURLToPath(new URL('../public/images/system-icon.png',import.meta.url)))}}});
  assert.equal(logo.status(),200,await logo.text());
  const configuredLogo=(await logo.json()).logo_url;assert.ok(configuredLogo);
  const entries = unzip(officeFixture('.docx'));
  // A large ignored XML comment provides a real multi-chunk transfer without
  // making the synthetic document hundreds of pages or using personal files.
  entries['word/document.xml'] = entries['word/document.xml'].replace('</w:document>', `<!--${'local-test-only '.repeat(8000)}--></w:document>`);
  const bytes = zip(entries), item = await upload(user, bytes);
  for (const engine of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
    const executablePath = engine === 'chromium' ? '/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome' : engine === 'firefox' ? '/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox' : '/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const env = { ...process.env, PULSE_SERVER: `unix:${mx.directory}/no-audio.sock`, PIPEWIRE_REMOTE: 'mx_test_disconnected', ALSA_CONFIG_PATH: fileURLToPath(new URL('./alsa-null.conf', import.meta.url)), SDL_AUDIODRIVER: 'dummy', ALSOFT_DRIVERS: 'null' };
    const browser = await ({ chromium, firefox, webkit })[engine].launch({ headless: true, executablePath, env, ...(engine === 'chromium' ? { args: ['--no-sandbox', '--mute-audio'] } : {}) }); browsers.push(browser);
    const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 900 } });
    await context.addInitScript(tokens => {
      sessionStorage.setItem('mx_access_token', tokens.access_token); sessionStorage.setItem('mx_refresh_token', tokens.refresh_token);
      window.loadingStages = [];
      new MutationObserver(() => {
        const step = document.querySelector('.office-loading [aria-current="step"]');
        const text = step?.textContent;
        if (text && window.loadingStages.at(-1) !== text) window.loadingStages.push(text);
      }).observe(document, { childList: true, subtree: true, attributes: true, characterData: true });
    }, user);
    const unexpected = [], errors = [];
    await context.route('**/*', route => {
      const url = new URL(route.request().url());
      if (['http:', 'https:'].includes(url.protocol) && url.origin !== origin) { unexpected.push(url.href); return route.abort(); }
      return route.continue();
    });
    const metadata = gate(), resource = gate();
    let metadataSeen = false, resourceSeen = false;
    await context.route(`**${base}/items/${item.uid}/office`, async route => { const response = await route.fetch(); metadataSeen = true; await metadata.promise; await route.fulfill({ response }); });
    await context.route('**/office/vendor/soffice.js', async route => { resourceSeen = true; await resource.promise; await route.continue(); });
    // Delay only the real worker's ready/open acknowledgements. The full engine
    // and document parsing still run, giving deterministic runtime/render stages.
    await context.route('**/office/office-thread.js', async route => {
      const response = await route.fetch(), source = await response.text();
      const send = 'const send = (message) => zeta.mainPort.postMessage(message);'; assert.ok(source.includes(send));
      await route.fulfill({ response, body: source.replace(send, "const send = (message) => ['ready','opened'].includes(message.cmd) ? setTimeout(() => zeta.mainPort.postMessage(message), 800) : zeta.mainPort.postMessage(message);") });
    });
    const downloaded = gate(), started = gate();
    transfer = { path: `${base}/items/${item.uid}/download?`, gate: downloaded.promise, started: started.release };
    const page = await context.newPage(); inspected = page; page.on('pageerror', error => errors.push(error.message));
    await page.goto(`${origin}/office/index.html#file=${item.uid}`);
    await until(() => metadataSeen, 'metadata held', 20000);
    const loading = page.getByRole('region', { name: 'Office loading progress' });
    await loading.getByText('Prepare document', { exact: false }).first().waitFor();
    await until(()=>page.locator('link[rel="icon"]').getAttribute('href').then(href=>href===new URL(configuredLogo,origin).href),'Drive Office configured tab logo');
    assert.equal(await loading.getByRole('list', { name: 'Loading stages' }).getByRole('listitem').count(), 5);
    assert.equal(await loading.getByRole('progressbar').evaluate(node => node.hasAttribute('value')), false, 'Access check has no invented percentage');
    for (const [theme, width] of [['light',1280], ['dark',1280], ['dark',390]]) {
      await page.evaluate(value => document.documentElement.dataset.theme = value, theme); await page.setViewportSize({ width, height: 900 });
      assert.equal(await loading.evaluate(node=>getComputedStyle(node).backgroundColor),theme==='dark'?'rgb(9, 10, 11)':'rgb(250, 251, 248)','Office uses shared MX light/dark tokens');
      const box = await loading.locator('.loading-card').boundingBox();
      assert.ok(box.x >= 0 && box.x + box.width <= width + 1 && box.height < 862);
      assert.equal((await page.locator('.toolbar').boundingBox()).height, 38);
      await page.screenshot({ path: join(mx.directory, `${engine}-office-loading-${theme}-${width}.png`) });
    }
    await page.setViewportSize({ width: 1280, height: 900 }); await page.evaluate(() => document.documentElement.dataset.theme = 'light');
    metadata.release(); await started.promise;
    await until(() => loading.locator('.transfer-detail').innerText().then(text => parseFloat(text) > 0 && /[\d.]+ (?:B|KiB|MiB).*document/.test(text)), 'received document byte count', 10000);
    const percentage = await loading.getByRole('progressbar', { name: 'Document download progress' }).evaluate(node => node.hasAttribute('value') ? node.value : null);
    if (percentage !== null) assert.ok(percentage > 0 && percentage < 100, 'Held real transfer must not claim completion');
    await page.screenshot({ path: join(mx.directory, `${engine}-office-document-transfer.png`) }); downloaded.release();
    if (engine === 'webkit') {
      await page.getByRole('heading', { name: 'The editor could not finish opening', exact: true }).waitFor();
      assert.equal(resourceSeen, false, 'Unsupported worker graphics must not start the editor'); resource.release();
      assert.equal(await loading.getByRole('progressbar').count(), 0, 'Failed loading must stop its spinner');
      await page.getByRole('button', { name: 'Document options', exact: true }).click();
      const download = page.waitForEvent('download'); await page.getByRole('button', { name: 'Download copy', exact: true }).click();
      const stream = await (await download).createReadStream(), chunks = []; for await (const chunk of stream) chunks.push(chunk);
      assert.deepEqual(Buffer.concat(chunks), bytes, 'Unsupported browser retains the exact loaded original');
      results.push({ engine, result: 'real loading and worker-graphics rejection; original download only, not Office editing' });
    } else {
      await until(() => resourceSeen, 'real engine script held', 20000);
      await loading.getByText('Load editor resources', { exact: false }).first().waitFor();
      assert.equal(await loading.getByRole('progressbar').evaluate(node => node.hasAttribute('value')), false, 'Unmeasured engine boot must stay indeterminate');
      await page.screenshot({ path: join(mx.directory, `${engine}-office-engine-loading.png`) }); resource.release();
      await until(() => loading.locator('[aria-current="step"]').innerText().then(text => text.includes('Open and render document')), 'real document-render acknowledgement held');
      await page.screenshot({ path: join(mx.directory, `${engine}-office-document-opening.png`) });
      await loading.waitFor({ state: 'detached', timeout: 120000 });
      await until(() => page.locator('#qtcanvas').getAttribute('data-native-focused').then(value => value === 'true'), 'real native editor focus');
      const phases = await page.evaluate(() => window.loadingStages.join('\n'));
      for (const phase of ['Prepare document','Load document','Load editor resources','Start editor on this device','Open and render document']) assert.ok(phases.includes(phase), phases);
      assert.equal((await page.locator('.toolbar').boundingBox()).height, 38); assert.equal((await page.locator('.editor-stage').boundingBox()).height, 862);
      assert.equal(await page.locator('.office-loading').count(), 0, 'No loading rows remain in the ready editor');
      await page.screenshot({ path: join(mx.directory, `${engine}-office-ready.png`) });
      await page.reload(); await until(() => page.locator('#qtcanvas').getAttribute('data-native-focused').then(value => value === 'true'), 'warm reload opens real document');
      assert.equal(await page.locator('.office-loading').count(), 0);
      results.push({ engine, result: 'all five real engine stages, held multi-chunk download, ready geometry and warm reload' });
    }
    assert.equal((await call(`${base}/items/${item.uid}`, user)).revision, 1, 'Opening/reloading never creates a saved version');
    await page.close();
    const attachment=await context.newPage();
    await attachment.goto(`${origin}/office/attachment.html#message_file=${randomUUID()}`);
    await until(()=>attachment.locator('link[rel="icon"]').getAttribute('href').then(href=>href===new URL(configuredLogo,origin).href),'Attachment Office configured tab logo');
    assert.equal(await attachment.locator('link[rel="icon"]').getAttribute('type'),null,'Uploaded PNG is not labeled as an SVG');
    await attachment.close();
    const missing = await context.newPage(); inspected = missing; missing.on('pageerror', error => errors.push(error.message));
    await missing.goto(`${origin}/office/index.html#file=${randomUUID()}`);
    await missing.getByRole('heading', { name: 'The editor could not finish opening', exact: true }).waitFor();
    assert.equal(await missing.getByRole('region', { name: 'Office loading progress' }).getByRole('progressbar').count(), 0);
    await missing.getByRole('button', { name: 'Dismiss error', exact: true }).click();
    assert.equal(await missing.locator('.office-loading.failed').count(), 1, 'Dismissing an error must not restart an endless spinner');
    assert.equal(await missing.getByRole('region', { name: 'Office loading progress' }).getByRole('progressbar').count(), 0);
    await missing.screenshot({ path: join(mx.directory, `${engine}-office-loading-failed.png`) });
    assert.deepEqual(errors, []); assert.deepEqual(unexpected, []);
    console.log(JSON.stringify(results.at(-1))); await context.close(); await browser.close();
  }
} catch (error) {
  process.exitCode = 1; results.push({ failure: error.stack });
  if (inspected && !inspected.isClosed()) { await inspected.screenshot({ path: join(mx.directory, 'office-loading-failure.png') }).catch(() => {}); console.log(await inspected.locator('body').innerText()); }
} finally {
  unblock.forEach(release => release());
  for (const browser of browsers) await browser.close().catch(() => {});
  await writeFile(join(mx.directory, 'office-loading-results.json'), JSON.stringify({ passed: !process.exitCode, results }, null, 2));
  console.log(JSON.stringify({ passed: !process.exitCode, results, artifacts: mx.directory }));
  await new Promise(resolve => proxy.close(resolve)); await mx.close();
}
