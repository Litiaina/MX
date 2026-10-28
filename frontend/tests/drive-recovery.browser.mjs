// Real production UI/MX/SQLite, fault-injected isolated N1 contract fixture.
// No deployment files, live objects or desktop audio devices are used.
import assert from 'node:assert/strict';
import { randomUUID, createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
const results = [], browsers = [], unblock = []; let failure, activePage;
const silent = { ...process.env, PULSE_SERVER: `unix:${mx.directory}/no-audio.sock`, PIPEWIRE_REMOTE: `mx_drive_recovery_${randomUUID()}`,
  ALSA_CONFIG_PATH: fileURLToPath(new URL('./alsa-null.conf', import.meta.url)), SDL_AUDIODRIVER: 'dummy', ALSOFT_DRIVERS: 'null' };
const base = '/mx/v1/drive';
const call = async (path, user, method = 'GET', data, expected = [200]) => (await mx.call(path, user?.access_token, method, data, expected)).body;
async function until(check, label, timeout = 45000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await check()) return; await new Promise(resolve => setTimeout(resolve, 100)); }
  throw Error(`Recovery timeout: ${label}`);
}
function rows(sql, ...args) {
  const db = new DatabaseSync(join(mx.directory, 'test.db'));
  try { db.exec('PRAGMA busy_timeout=10000'); return db.prepare(sql).all(...args); } finally { db.close(); }
}
function execute(sql, ...args) {
  const db = new DatabaseSync(join(mx.directory, 'test.db'));
  try { db.exec('PRAGMA busy_timeout=10000'); return db.prepare(sql).run(...args); } finally { db.close(); }
}
try {
  console.log(`Drive recovery artifacts: ${mx.directory}`);
  assert.equal(Boolean(mx.n1.live), false, 'This fault-injection runner requires its isolated contract fixture');
  const password = `Test-only!${randomUUID()}`;
  await call('/mx/v1/auth/create', { access_token: mx.signupKey }, 'POST', { email: 'recovery@test.invalid', name: 'Recovery owner', password }, [201]);
  const owner = await call('/mx/v1/auth/authenticate', null, 'POST', { email: 'recovery@test.invalid', password });
  const content = Buffer.alloc(18 * 1024 ** 2);
  for (let i = 0; i < content.length; i++) content[i] = i % 251;
  const checksum = createHash('sha256').update(content).digest('hex');
  for (const name of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
    const browser = await ({ chromium, firefox, webkit })[name].launch({ headless: true, env: silent,
      executablePath: name === 'chromium' ? '/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome' : name === 'firefox' ? '/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox' : '/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh',
      ...(name === 'chromium' ? { args: ['--mute-audio'] } : {}) });
    browsers.push(browser);
    const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1440, height: 1000 } });
    const page = await context.newPage(); activePage = page;
    const errors = []; page.on('pageerror', error => errors.push(error.message)); page.on('dialog', dialog => dialog.accept());
    await page.addInitScript(tokens => { sessionStorage.setItem('mx_access_token', tokens.access_token); sessionStorage.setItem('mx_refresh_token', tokens.refresh_token); }, owner);
    let holdParts = true, releaseParts = [], lostFinish = false, releaseFinish;
    let initCount = 0, partZeroCount = 0;
    page.on('request', request => {
      if (request.method() === 'POST' && new URL(request.url()).pathname === `${base}/uploads`) initCount++;
      if (request.method() === 'PUT' && request.url().endsWith('/parts/0')) partZeroCount++;
    });
    // Inject the stall at the storage boundary, leaving native Blob PUTs
    // untouched by the browser driver's request-body interception machinery.
    mx.n1.control.beforePart = async (_session, index) => {
      if (holdParts && index > 0) await new Promise(resolve => { releaseParts.push(resolve); unblock.push(resolve); });
    };
    page.on('response', async response => {
      if (/\/drive\/uploads\/[^/]+\/parts\//.test(response.url()) && response.status() >= 400) {
        console.log(JSON.stringify({ browser: name, part: new URL(response.url()).pathname.split('/').at(-1), status: response.status(), error: await response.text().catch(() => 'unavailable') }));
        if (response.status() === 422) console.log(JSON.stringify({browser: name, source: await page.evaluate(async () => {
          const file = window.recoveryTestFile;
          try { return {size:file?.size,sliceSize:file?.slice(4194304,8388608).size,readBytes:(await file?.slice(4194304,8388608).arrayBuffer())?.byteLength}; }
          catch(error){return {error:String(error)};}
        }).catch(() => 'no page')}));
      }
    });
    await page.route('**/drive/uploads/*/finish', async route => {
      if (!lostFinish) {
        lostFinish = true;
        // Commit on MX, but hold/lose its response. UI must NOT report Uploaded.
        await route.fetch(); await new Promise(resolve => releaseFinish = resolve);
        await route.abort('failed').catch(() => {});
      } else await route.continue().catch(() => {});
    });
    const filename = `resume-${name}.MP4`, path = join(mx.directory, filename);
    await writeFile(path, content);
    await page.goto(`https://127.0.0.1:${mx.port}/#drive`);
    await page.getByRole('heading', { name: 'MX Drive', exact: true }).waitFor();
    await page.evaluate(() => document.documentElement.dataset.theme = 'dark');
    await page.locator('input[type=file]:not([webkitdirectory])').first().evaluate(input => input.addEventListener('change', () => { window.recoveryTestFile = input.files?.[0]; }, { capture: true }));
    await page.locator('input[type=file]:not([webkitdirectory])').first().setInputFiles(path);
    if (process.env.MX_TEST_RECOVERY_SOURCE_FALLBACK === '1') {
      assert.equal(name, 'webkit', 'This diagnostic mode validates only the observed WebKit source-loss guard');
      await page.locator('.transfer[data-status=needs_file]').waitFor();
      await page.getByText('Your browser could no longer read the selected file.', {exact:false}).waitFor();
      const count=initCount;
      await new Promise(resolve=>setTimeout(resolve,2500));
      assert.equal(initCount,count,'An unreadable native source must not cause endless replay');
      assert.equal(rows('SELECT COUNT(*) AS n FROM mx_drive_items WHERE name=?',filename)[0].n,0,'Empty parts must never publish a file');
      await page.screenshot({path:join(mx.directory,'webkit-source-loss-guard.png')});
      assert.deepEqual(errors,[]);
      results.push({browser:name,scenario:'unreadable native source guard only',passed:true,fullRecoveryQualified:false});
      await browser.close();continue;
    }
    await until(() => mx.n1.sessions.size && [...mx.n1.sessions.values()].some(session => session.key.endsWith('.MP4') && session.parts.has(0)), 'first part stored');
    // N1 having bytes is not the browser receiving MX's durable acknowledgement.
    // This scenario deliberately tests resuming an ACKNOWLEDGED part; switching
    // offline before its HTTP reply would correctly leave storedBytes at zero.
    await until(() => page.evaluate(uid => Object.keys(localStorage)
      .filter(key => key.startsWith(`mx:drive:pending:${uid}:`))
      .map(key => JSON.parse(localStorage.getItem(key)))
      .some(entry => entry.storedBytes >= 4 * 1024 ** 2), owner.uid), 'browser persisted the first part acknowledgement');
    // A real module transition must keep the selected File and queue alive.
    await page.evaluate(() => location.hash = '#dashboard');
    await page.getByRole('heading', { name: 'Dashboard', exact: true }).waitFor();
    await until(() => page.locator('.transfer').count().then(count => count === 1), 'queue follows module transition');
    await context.setOffline(true);
    await page.locator('.transfer[data-status=waiting]').waitFor();
    assert.equal(await page.locator('.transfer .telemetry').count(), 0, 'No bogus speed/ETA while offline');
    const cached = await page.evaluate(uid => Object.keys(localStorage).filter(key => key.startsWith(`mx:drive:pending:${uid}:`)).map(key => JSON.parse(localStorage.getItem(key))), owner.uid);
    assert.equal(cached.length, 1); assert.ok(cached[0].storedBytes >= 4 * 1024 ** 2); assert.equal('file' in cached[0], false);
    const operation = cached[0].operationUid;
    await page.evaluate(() => document.documentElement.dataset.theme = 'dark');
    const darkPalette=await page.evaluate(()=>{
      const probe=document.createElement('span');probe.style.backgroundColor='var(--collab-bg)';
      probe.style.position='absolute';probe.style.visibility='hidden';document.body.append(probe);
      const expected=getComputedStyle(probe).backgroundColor;probe.remove();
      return {actual:getComputedStyle(document.querySelector('.upload-panel')).backgroundColor,expected};
    });
    assert.equal(darkPalette.actual,darkPalette.expected,'Waiting panel follows the shared MX dark chrome token');
    assert.ok(darkPalette.actual.match(/\d+/g).slice(0,3).map(Number).every(channel=>channel<64),'Dark mode must not silently fall back to the light palette');
    await page.screenshot({ path: join(mx.directory, `${name}-waiting-dark.png`) });
    await page.getByRole('button', { name: `Pause ${filename}`, exact: true }).click();
    const priorInits = initCount;
    holdParts = false; releaseParts.splice(0).forEach(resolve => resolve());
    await context.setOffline(false);
    await page.locator('.transfer[data-status=paused]').waitFor();
    await new Promise(resolve => setTimeout(resolve, 1200)); assert.equal(initCount, priorInits, 'Online must not override manual pause');
    // Still recover after the old one-hour cutoff, including a retryable-failure timestamp.
    execute('UPDATE mx_drive_uploads SET attempted_at=? WHERE actor_uid=? AND operation_uid=?', -(Date.now() - 2 * 3600000), owner.uid, operation);
    await page.reload(); await page.locator('.transfer[data-status=needs_file]').waitFor();
    assert.equal(initCount, priorInits, 'Reload cannot upload without reselecting original bytes');
    const original = page.getByLabel(`Select original file for ${filename}`, { exact: true });
    await original.evaluate(input => input.addEventListener('change', () => { window.recoveryTestFile = input.files?.[0]; }, { capture: true }));
    const wrongPath = join(mx.directory, `wrong-${name}.MP4`); await writeFile(wrongPath, 'not original');
    await original.setInputFiles(wrongPath);
    await page.getByText('Choose the original file:', { exact: false }).waitFor();
    assert.equal(initCount, priorInits);
    await page.setViewportSize({ width: 390, height: 850 });
    await page.evaluate(() => document.documentElement.dataset.theme = 'light');
    if (await page.getByRole('button',{name:'Collapse navigation',exact:true}).isVisible()) await page.getByRole('button',{name:'Collapse navigation',exact:true}).click();
    await page.screenshot({ path: join(mx.directory, `${name}-reselect-mobile-light.png`),animations:'disabled' });
    await original.setInputFiles(path); await page.locator('.transfer[data-status=paused]').waitFor();
    await page.getByRole('button', { name: `Resume ${filename}`, exact: true }).click();
    await page.getByText('Confirming upload with MX…', { exact: true }).waitFor();
    await until(() => typeof releaseFinish === 'function', 'finalization acknowledgement held');
    assert.equal(await page.locator('.transfer[data-status=done]').count(), 0);
    assert.equal(rows('SELECT COUNT(*) AS n FROM mx_drive_versions v JOIN mx_drive_items i ON i.uid=v.item_uid WHERE i.name=?', filename)[0].n, 1);
    releaseFinish(); await page.locator('.transfer[data-status=done]').waitFor({ state: 'attached' });
    assert.equal(partZeroCount, 2, 'Only initial transient retry sends part zero; resume must skip it');
    assert.equal(rows('SELECT COUNT(*) AS n FROM mx_drive_versions v JOIN mx_drive_items i ON i.uid=v.item_uid WHERE i.name=?', filename)[0].n, 1, 'Lost finish acknowledgement must not duplicate versions');
    assert.equal(await page.evaluate(uid => Object.keys(localStorage).filter(key => key.startsWith(`mx:drive:pending:${uid}:`)).length, owner.uid), 0);
    const [stored] = rows('SELECT object_key FROM mx_drive_versions v JOIN mx_drive_items i ON i.uid=v.item_uid WHERE i.name=?', filename);
    assert.equal(createHash('sha256').update(mx.n1.objects.get(stored.object_key).bytes).digest('hex'), checksum);
    assert.deepEqual(errors, []);
    await page.reload(); assert.equal(await page.locator('.transfer').count(), 0);
    // A separate uninterrupted recovery case: no Resume click after reconnect,
    // and a real MX process restart while the browser is offline.
    const automatic = `automatic-${name}.MP4`, automaticPath = join(mx.directory, automatic);
    await writeFile(automaticPath, content); holdParts = true; partZeroCount = 0;
    await page.evaluate(() => location.hash = '#drive');
    await page.getByRole('heading', { name: 'MX Drive', exact: true }).waitFor();
    await page.locator('input[type=file]:not([webkitdirectory])').first().setInputFiles(automaticPath);
    await until(() => [...mx.n1.sessions.values()].some(session => session.parts.has(0)), 'automatic case first part');
    await until(() => page.evaluate(({uid,name}) => Object.keys(localStorage)
      .filter(key => key.startsWith(`mx:drive:pending:${uid}:`))
      .map(key => JSON.parse(localStorage.getItem(key)))
      .some(entry => entry.name === name && entry.storedBytes >= 4 * 1024 ** 2), {uid:owner.uid,name:automatic}), 'automatic case browser acknowledgement');
    await context.setOffline(true); await page.locator('.transfer[data-status=waiting]').waitFor();
    await mx.restart(); holdParts = false; releaseParts.splice(0).forEach(resolve => resolve());
    await context.setOffline(false); await page.locator('.transfer[data-status=done]').waitFor({ state: 'attached', timeout: 45000 });
    assert.equal(partZeroCount, 2, 'Automatic reconnect after MX restart skips acknowledged parts');
    const [automaticStored] = rows('SELECT object_key FROM mx_drive_versions v JOIN mx_drive_items i ON i.uid=v.item_uid WHERE i.name=?', automatic);
    assert.equal(createHash('sha256').update(mx.n1.objects.get(automaticStored.object_key).bytes).digest('hex'), checksum);
    assert.equal(rows('SELECT COUNT(*) AS n FROM mx_drive_versions v JOIN mx_drive_items i ON i.uid=v.item_uid WHERE i.name=?', automatic)[0].n, 1);
    assert.deepEqual(errors, []);
    results.push({ browser: name, passed: true, moduleNavigation: true, offline: true, manualPause: true, reloadReselection: true,
      recoverAfterTwoHours: true, automaticReconnectAfterMxRestart: true, lostFinishAck: true, noDuplicateVersion: true, storedChecksum: true, byteCount: content.length });
    console.log(JSON.stringify(results.at(-1))); await browser.close();
  }
  // Eight simultaneous resumable clients exercise server admission/quotas/CAS,
  // without eight browsers or desktop media. Each uses real HTTP/SQLite/storage.
  const users = await Promise.all(Array.from({ length: 8 }, async (_, i) => {
    const email = `recovery${i}@test.invalid`;
    await call('/mx/v1/user/create', owner, 'POST', { email, name: `Recovery ${i}`, password, access_level: 2 }, [201]);
    return call('/mx/v1/auth/authenticate', null, 'POST', { email, password });
  }));
  await Promise.all(users.map(async (user, i) => {
    const uid = randomUUID(), file_name = `parallel-${i}.bin`, data = Buffer.alloc(2 * 1024 ** 2, i);
    const state = await call(`${base}/uploads`, user, 'POST', { operation_uid: uid, file_name, mime_type: 'application/octet-stream', size: data.length, last_modified: 1 });
    execute('UPDATE mx_drive_uploads SET attempted_at=? WHERE actor_uid=? AND operation_uid=?',Date.now()-2*3600000,user.uid,uid);
    for (let attempt = 0;; attempt++) {
      const response = await mx.api.put(`${base}/uploads/${uid}/parts/0`, { headers: { Authorization: `Bearer ${user.access_token}`, 'Content-Type': 'application/octet-stream' }, data });
      if (response.status() === 200) break;
      assert.ok(attempt < 3 && [429, 502, 503].includes(response.status()), await response.text());
    }
    assert.ok(rows('SELECT attempted_at FROM mx_drive_uploads WHERE actor_uid=? AND operation_uid=?',user.uid,uid)[0].attempted_at>Date.now()-60000,'Valid part activity renews retention, even without reinitializing');
    const item = await call(`${base}/uploads/${uid}/finish`, user, 'POST');
    const replay = await call(`${base}/uploads/${uid}/finish`, user, 'POST'); assert.equal(replay.uid, item.uid);
    assert.equal(state.part_size, data.length);
    assert.equal(rows('SELECT COUNT(*) AS n FROM mx_drive_versions WHERE item_uid=?',item.uid)[0].n,1);
    const [version]=rows('SELECT object_key FROM mx_drive_versions WHERE item_uid=?',item.uid);
    assert.deepEqual(mx.n1.objects.get(version.object_key).bytes,data,'Eight-user uploads retain every byte');
    const abandoned=randomUUID();
    await call(`${base}/uploads`,user,'POST',{operation_uid:abandoned,file_name:`cancel-${i}.bin`,mime_type:'application/octet-stream',size:16,last_modified:1});
    await call(`${base}/uploads/${abandoned}`,user,'DELETE');
    await call(`${base}/uploads/${abandoned}`,user,'DELETE');
    assert.equal(rows('SELECT COUNT(*) AS n FROM mx_drive_uploads WHERE actor_uid=? AND operation_uid=?',user.uid,abandoned)[0].n,0,'Acknowledged cancellation releases the exact reservation');
    await call(`${base}/uploads/${uid}`,user,'DELETE');
    assert.equal(rows('SELECT COUNT(*) AS n FROM mx_drive_versions WHERE item_uid=?',item.uid)[0].n,1,'Cancelling a completed operation cannot delete the published file');
  }));
  results.push({ test: 'eight concurrent HTTP clients', passed: true, acknowledgedFiles: 8, versionsPerFile: 1 });
  assert.deepEqual(mx.n1.failures, []);
} catch (error) {
  failure = error.stack; process.exitCode = 1;
  await activePage?.screenshot({ path: join(mx.directory, 'recovery-failure.png'), fullPage: true }).catch(() => {});
} finally {
  unblock.forEach(resolve => resolve());
  mx.n1.control.beforePart = undefined;
  for (const browser of browsers) await browser.close().catch(() => {});
  const report = { passed: !failure, results, failure, storage: 'isolated N1 HTTP contract fixture; not live N1', productionQualified: false };
  await writeFile(join(mx.directory, 'recovery-results.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ ...report, artifacts: mx.directory })); await mx.close();
}
