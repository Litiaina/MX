// Real browser engines, native RTCPeerConnections and the production media
// tiles. Capture is synthetic; signaling loss is deliberate, not UDP shaping.
import assert from 'node:assert/strict';
import { chromium, firefox, webkit } from 'playwright';
import { createServer } from 'vite';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isolatedBrowserAudio } from './silent-audio.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const browserAudio = isolatedBrowserAudio();
const artifacts = await mkdtemp(join(process.env.MX_TEST_ARTIFACTS || tmpdir(), 'mx-network-'));
const server = await createServer({ root, logLevel: 'error', plugins: [{ name: 'mx-test-file-stream', configureServer(server) {
  server.middlewares.use('/network-file', (_request, response) => {
    response.setHeader('Content-Type', 'application/octet-stream');
    response.setHeader('Content-Disposition', 'attachment; filename="network-test.bin"');
    response.write('first');
    const timer = setTimeout(() => response.end('second'), 500);
    response.on('close', () => clearTimeout(timer));
  });
} }], server: { host: '127.0.0.1', port: 5196, strictPort: true, hmr: false } });
const origin = 'http://127.0.0.1:5196';
const engines = { chromium, firefox, webkit };
const browsers = new Map();
const results = [];
const html = (component, props = '{}') => `<!doctype html><html><meta name="viewport" content="width=device-width,initial-scale=1"><body><div id="app"></div><script type="module">import {mount} from '/node_modules/.vite/deps/svelte.js';import '/src/styles/tokens.css';import '/src/styles/foundation.css';import '/src/styles/app.css';import '/src/styles/workspaces.css';import Component from '${component}';mount(Component,{target:document.getElementById('app'),props:${props}});</script></body></html>`;
const session = { uid: 'test-user', name: 'Network Test', email: 'test@example.invalid', access_level: 3, access_name: 'Viewer', totp_enabled: false, totp_enrollment_pending: false, recovery_codes_remaining: 0 };
async function bounded(promise, description, timeout = 15000) {
  let timer;
  try { return await Promise.race([promise, new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error(`Browser operation stalled: ${description}`)), timeout); })]); }
  finally { clearTimeout(timer); }
}
async function until(check, description, timeout = 30000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await bounded(check(), description, Math.max(1, deadline - Date.now()))) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out: ${description}`);
}

async function startupUI(browser, engine) {
  const page = await browser.newPage(); let offline = true;
  const errors = []; page.on('pageerror', (error) => errors.push(error.message));
  await page.addInitScript(() => { sessionStorage.setItem('mx_access_token', 'test'); sessionStorage.setItem('mx_refresh_token', 'test-refresh'); });
  await page.route('**/mx/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/mx/v1/auth/session') return route.fulfill({ status: offline ? 503 : 200, json: offline ? { response: 'Temporary outage' } : session });
    if (path === '/mx/v1/modules') return route.fulfill({ json: { modules: [] } });
    if (path.endsWith('/calls')) return route.fulfill({ json: { calls: [] } });
    if (path.includes('/notifications')) return route.fulfill({ json: { notifications: [], unread_count: 0, total: 0 } });
    if (path.includes('/channels')) return route.fulfill({ json: { channels: [] } });
    return route.fulfill({ status: 503, json: { response: 'Unavailable test dependency' } });
  });
  await page.goto(`${origin}/#collaboration`);
  await page.getByRole('heading', { name: 'Connection interrupted' }).waitFor();
  assert.equal(await page.evaluate(() => sessionStorage.getItem('mx_refresh_token')), 'test-refresh');
  assert.equal(await page.locator('input[type="password"]').count(), 0);
  await page.screenshot({ path: join(artifacts, `${engine}-outage.png`) });
  offline = false;
  await page.getByRole('button', { name: 'Reconnect', exact: true }).click();
  await page.waitForFunction(() => !document.querySelector('.loading-page') && document.body.textContent.includes('Collaboration'));
  assert.equal(await page.evaluate(() => sessionStorage.getItem('mx_refresh_token')), 'test-refresh');
  assert.deepEqual(errors, []);
  results.push({ engine, test: 'startup-outage-and-reconnect-ui', passed: true });
  console.log(JSON.stringify(results.at(-1)));
  await page.close();
}

async function fileBody(browser, engine) {
  const page = await browser.newPage();
  await page.route('**/file-probe', (route) => route.fulfill({ contentType: 'text/html', body: '<html><body>File recovery probe</body></html>' }));
  await page.goto(`${origin}/file-probe`);
  const result = await page.evaluate(async () => {
    const { apiFile } = await import('/src/lib/api/client.ts');
    let stalled; let headersReceived = false;
    const originalFetch = window.fetch;
    window.fetch = async (...args) => { const response = await originalFetch(...args); headersReceived = true; return response; };
    try { await apiFile('/network-file', { timeoutMs: 100, maxAttempts: 1 }); stalled = 'incorrectly acknowledged'; }
    catch (error) { stalled = error.name; }
    finally { window.fetch = originalFetch; }
    const file = await apiFile('/network-file', { timeoutMs: 5000 });
    return { stalled, headersReceived, body: await file.blob.text(), disposition: file.headers.get('Content-Disposition') };
  });
  assert.equal(result.stalled, 'ConnectionError');
  assert.equal(result.headersReceived, true, 'Cancellation must happen after native response headers');
  assert.equal(result.body, 'firstsecond');
  assert.equal(result.disposition, 'attachment; filename="network-test.bin"');
  results.push({ engine, test: 'native-file-body-abort-and-complete-retry', passed: true });
  console.log(JSON.stringify(results.at(-1)));
  await page.close();
}

async function recordUI(browser, engine) {
  const page = await browser.newPage(); const submissions = []; let loseResponses = true;
  const field = { uid: 'name', key: 'name', label: 'Name', field_type: 'text', active: true, required: true, config: {}, position: 0, table_priority: 0, table_visible: true, unique_value: false, searchable: true, sortable: true };
  await page.route('**/record-probe', (route) => route.fulfill({ contentType: 'text/html', body: html('/src/lib/components/RecordEditor.svelte', `{schema:${JSON.stringify({revision:1,fields:[field],system_fields:[]})},record:null,canWrite:true,canDelete:false,onClose:()=>window.closedEditor=true,onSaved:async()=>window.savedEditor=true}`) }));
  await page.route('**/mx/v1/records', (route) => {
    const body = route.request().postDataJSON(); submissions.push(body);
    return loseResponses ? route.abort('failed') : route.fulfill({ status: 200, json: { uid: 'one-record', revision: 1, values: body.values, attached_files: [] } });
  });
  await page.goto(`${origin}/record-probe`);
  await page.getByLabel(/^Name/).fill('Retained entry');
  await page.getByRole('button', { name: 'Save Record', exact: true }).click();
  await page.waitForFunction(() => document.body.textContent.includes('did not acknowledge'));
  assert.equal(await page.getByLabel(/^Name/).inputValue(), 'Retained entry');
  // Reload restores both the draft and its original operation identifier.
  await page.reload();
  await page.getByLabel(/^Name/).waitFor();
  assert.equal(await page.getByLabel(/^Name/).inputValue(), 'Retained entry');
  loseResponses = false;
  await page.getByRole('button', { name: 'Save Record', exact: true }).click();
  await page.waitForFunction(() => window.savedEditor === true && window.closedEditor === true);
  assert.equal(submissions.length, 3);
  assert.ok(submissions[0].operation_uid);
  assert.ok(submissions.every((value) => JSON.stringify(value) === JSON.stringify(submissions[0])));
  results.push({ engine, test: 'record-ui-retains-draft-and-operation-after-lost-ack', passed: true });
  console.log(JSON.stringify(results.at(-1)));
  await page.close();
}

async function chatUI(browser, engine) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 850 } });
  const errors = []; page.on('pageerror', (error) => errors.push(error.message));
  const channels = ['Alpha', 'Beta'].map((name) => ({ uid: name.toLowerCase(), name, kind: 'direct', description: '', created_by: session.uid, created_at: 1, role: 'member', notification_level: 'all', invite_policy: 'admins', member_count: 2, unread_count: 0, last_message_at: null, direct_user_uid: `${name}-person` }));
  const submissions = []; const stored = new Map(); let loseAck = true; let release;
  await page.route('**/chat-probe', (route) => route.fulfill({ contentType: 'text/html', body: html('/src/lib/components/CollaborationView.svelte', `{session:${JSON.stringify(session)}}`) }));
  await page.route('**/mx/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/channels')) return route.fulfill({ json: { channels } });
    if (path.endsWith('/people')) return route.fulfill({ json: { people: [] } });
    if (path.endsWith('/presence')) return route.fulfill({ json: { accounts: [] } });
    if (path.endsWith('/call')) return route.fulfill({ json: { active: false, participants: [] } });
    if (path.endsWith('/read')) return route.fulfill({ json: { read_state: { user_uid: session.uid, user_name: session.name, last_read_at: 1, last_read_message_id: 0 } } });
    if (path.endsWith('/files') && route.request().method() === 'POST') return route.fulfill({ status: 400, json: { response: 'Test file rejected' } });
    if (path.endsWith('/messages')) {
      const channel = path.split('/').at(-2);
      if (route.request().method() === 'POST') {
        const body = route.request().postDataJSON(); submissions.push(body);
        if (!stored.has(body.operation_uid)) stored.set(body.operation_uid, { uid: `message-${stored.size}`, channel_uid: channel, sender_uid: session.uid, sender_name: session.name, body: body.body, sequence: stored.size + 1, created_at: 1, edited_at: null, deleted_at: null, reply_to_uid: null, files: [], record_links: [], mentions: [], reactions: [] });
        if (loseAck) {
          if (submissions.length === 1) await new Promise((resolve) => release = resolve);
          return route.abort('failed');
        }
        return route.fulfill({ json: { message: stored.get(body.operation_uid) } });
      }
      return route.fulfill({ json: { messages: [...stored.values()].filter((message) => message.channel_uid === channel), pinned_messages: [], read_states: [], has_more: false } });
    }
    return route.fulfill({ status: 404 });
  });
  await page.goto(`${origin}/chat-probe`);
  const composer = page.getByRole('textbox', { name: 'Message', exact: true });
  await composer.fill('Only once');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await until(() => !!release, 'first chat request');
  const conversations = page.getByRole('navigation', { name: 'Conversations' });
  await conversations.getByRole('button', { name: /Beta/ }).click();
  await page.getByText('Wait for this send to finish before switching conversations.', { exact: true }).waitFor();
  assert.equal(await page.locator('.conversation-title h2').textContent(), 'Alpha');
  release();
  await page.getByRole('button', { name: 'Retry send', exact: true }).waitFor();
  assert.equal(await composer.inputValue(), 'Only once');
  assert.equal(await composer.getAttribute('readonly'), '');
  await conversations.getByRole('button', { name: /Beta/ }).click();
  await composer.fill('Separate draft');
  await conversations.getByRole('button', { name: /Alpha/ }).click();
  assert.equal(await composer.inputValue(), 'Only once');
  loseAck = false;
  await page.getByRole('button', { name: 'Retry send', exact: true }).click();
  await page.getByRole('button', { name: 'Send', exact: true }).waitFor();
  assert.equal(await composer.inputValue(), '');
  assert.equal(submissions.length, 3);
  assert.ok(submissions.every((value) => JSON.stringify(value) === JSON.stringify(submissions[0])));
  assert.equal(stored.size, 1, 'Lost acknowledgement must not create another logical message');
  // A file validation error happens after the base message was acknowledged.
  // Removing the failed file and retrying must reconcile that same message.
  await page.locator('.message-composer input[type="file"]').setInputFiles({ name: 'rejected.txt', mimeType: 'text/plain', buffer: Buffer.from('test') });
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await page.getByText('Test file rejected', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Remove rejected.txt', exact: true }).click();
  await page.getByRole('button', { name: 'Retry send', exact: true }).click();
  await page.getByRole('button', { name: 'Send', exact: true }).waitFor();
  assert.equal(stored.size, 2);
  assert.deepEqual(submissions[3], submissions[4]);
  await conversations.getByRole('button', { name: /Beta/ }).click();
  assert.equal(await composer.inputValue(), 'Separate draft');
  assert.deepEqual(errors, []);
  results.push({ engine, test: 'chat-lost-ack-drafts-navigation-and-file-retry-ui', passed: true });
  console.log(JSON.stringify(results.at(-1)));
  await page.close();
}

async function recordTableUI(browser, engine) {
  const page = await browser.newPage({ viewport: { width: 1100, height: 430 } });
  const errors = []; page.on('pageerror', (error) => errors.push(error.message));
  const field = { uid: 'name', key: 'name', label: 'Name', field_type: 'text', active: true, required: false, config: {}, position: 0, table_priority: 0, table_visible: true, unique_value: false, searchable: true, sortable: true };
  const rows = Array.from({ length: 12 }, (_, index) => ({ uid: `row-${index}`, revision: 1, values: { name: `Entry ${index + 1}` }, attached_files: [] }));
  const patches = [];
  let latestRevision = 1;
  await page.route('**/table-probe', (route) => route.fulfill({ contentType: 'text/html', body: html('/src/lib/components/RecordsView.svelte', `{session:${JSON.stringify(session)},accessLevel:0}`) }));
  await page.route('**/mx/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/mx/v1/schema') return route.fulfill({ json: { revision: 1, fields: [field], system_fields: [] } });
    if (route.request().method() === 'PATCH') {
      patches.push(route.request().postDataJSON());
      return route.fulfill({ status: 409, json: { current_record_revision: 2, conflicts: [{ field_key: 'name', field_uid: 'name', label: 'Name', current_value: 'Changed elsewhere', your_value: 'My draft', base_revision: 1, current_revision: 2 }] } });
    }
    return route.fulfill({ json: { data: rows.map((row) => ({ ...row, revision: latestRevision })), page: 1, total_pages: 1, total: rows.length, has_next: false, limit: 50 } });
  });
  await page.goto(`${origin}/table-probe`);
  await page.getByRole('button', { name: 'More actions for Record 12', exact: true }).waitFor();
  await page.evaluate(() => { const table = document.querySelector('.records-table'); table.style.maxHeight = '140px'; table.style.overflow = 'auto'; table.scrollTop = table.scrollHeight; });
  for (const theme of ['light', 'dark']) {
    await page.evaluate((theme) => document.documentElement.dataset.theme = theme, theme);
    const trigger = page.getByRole('button', { name: 'More actions for Record 12', exact: true });
    await trigger.click();
    const menu = page.getByRole('menu', { name: 'Record actions' });
    await menu.waitFor();
    assert.equal(await menu.evaluate((element) => element.parentElement === document.body), true);
    assert.equal(await menu.evaluate((element) => {
      const rect = element.getBoundingClientRect();
      const hit = document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
      return rect.left >= 0 && rect.top >= 0 && rect.right <= innerWidth && rect.bottom <= innerHeight && element.contains(hit);
    }), true, 'Menu must be fully on screen and not covered by pagination');
    await page.screenshot({ path: join(artifacts, `${engine}-record-menu-${theme}.png`) });
    await page.keyboard.press('Escape');
    await menu.waitFor({ state: 'detached' });
    assert.equal(await menu.count(), 0);
    assert.equal(await trigger.evaluate((element) => element === document.activeElement), true);
  }
  await page.getByRole('button', { name: 'More actions for Record 12', exact: true }).click();
  const nextTrigger = page.getByRole('button', { name: 'More actions for Record 11', exact: true });
  await nextTrigger.click();
  const switchedMenu = page.getByRole('menu', { name: 'Record actions' });
  await until(async () => {
    const anchor = await nextTrigger.boundingBox(); const menu = await switchedMenu.boundingBox();
    return anchor && menu && Math.abs(menu.y - (anchor.y + anchor.height + 4)) <= 2;
  }, 'menu follows a different row trigger');
  await page.evaluate(() => document.querySelector('.records-table').scrollTop = 0);
  await switchedMenu.waitFor({ state: 'detached' });
  await page.setViewportSize({ width: 375, height: 700 });
  const mobileTrigger = page.getByRole('button', { name: 'More actions for Record 1', exact: true });
  await mobileTrigger.click();
  const mobileMenu = page.getByRole('menu', { name: 'Record actions' });
  assert.equal(await mobileMenu.evaluate((element) => { const rect = element.getBoundingClientRect(); return rect.left >= 0 && rect.right <= innerWidth; }), true);
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await mobileMenu.waitFor({ state: 'detached' });
  assert.equal(await mobileMenu.count(), 0, 'Outside pointer must dismiss menu');
  // A background refresh while typing must not change the save's base revision.
  await page.getByRole('button', { name: 'Edit Name for row 1', exact: true }).click();
  await page.locator('[data-active-cell-editor]').fill('My draft');
  latestRevision = 2;
  await page.evaluate(() => document.querySelector('.toolbar > button:last-child').click());
  await until(async () => await page.getByRole('button', { name: 'Refresh', exact: true }).isEnabled(), 'refresh completes');
  // Ensure response has been applied without blurring the active editor.
  await page.waitForTimeout(150);
  assert.equal(await page.locator('[data-active-cell-editor]').inputValue(), 'My draft');
  await page.locator('[data-active-cell-editor]').press('Enter');
  await page.getByText('Name changed elsewhere', { exact: true }).waitFor();
  assert.equal(patches[0].base_revision, 1);
  assert.deepEqual(patches[0].changes, { name: 'My draft' });
  assert.deepEqual(errors, []);
  results.push({ engine, test: 'record-action-menu-desktop-mobile-themes-and-inline-revision', passed: true });
  console.log(JSON.stringify(results.at(-1)));
  await page.close();
}

async function presentedFrames(viewer, uid, screen) {
  await viewer.bringToFront();
  const video = viewer.locator(`[data-peer="${uid}"] video`);
  await video.waitFor();
  await viewer.waitForFunction((uid) => { const video = document.querySelector(`[data-peer="${uid}"] video`); return video.classList.contains('active') && video.readyState >= 2 && !video.paused && video.videoWidth > 0; }, uid);
  // Check changing pixels of the intended source, not an unreliable frame
  // counter or a still-moving screen incorrectly left in the camera tile.
  const hashes = new Set();
  await until(async () => {
    const frame = await video.evaluate((element) => {
      const canvas = document.createElement('canvas'); canvas.width = 320; canvas.height = 180;
      const context = canvas.getContext('2d'); context.drawImage(element, 0, 0, 320, 180);
      const pixels = context.getImageData(0, 0, 320, 180).data;
      let hash = 0; for (let index = 0; index < pixels.length; index += 4) hash = ((hash * 31) + pixels[index] + pixels[index + 1] * 3 + pixels[index + 2] * 7) | 0;
      return { hash, red: pixels[0], green: pixels[1], blue: pixels[2] };
    });
    const correctSource = screen ? frame.red < 30 && frame.green > 70 && frame.blue > 50 : frame.red > 40 && frame.green < 40 && frame.blue > 40;
    if (correctSource) hashes.add(frame.hash);
    return hashes.size >= 4;
  }, `changing ${screen ? 'screen' : 'camera'} pixels from ${uid}`, 15000);
}

async function mediaGroup(order, fault, users = ['a', 'z']) {
  const pages = new Map(); const errors = []; let dropped = false; let signals = 0;
  const signalKinds = {};
  let beginning = 0;
  try {
    for (const [index, uid] of users.entries()) {
      const page = await browsers.get(order[index % order.length]).newPage({ viewport: { width: 1280, height: 800 } });
      page.setDefaultTimeout(30000); pages.set(uid, page);
      page.on('crash', () => errors.push(`${uid}: native ${order[index % order.length]} page crashed`));
      page.on('pageerror', (error) => errors.push(`${uid}: ${error.message}`));
      await page.exposeBinding('relay', async (_source, recipient, signal) => {
        signals++;
        signalKinds[signal.kind] = (signalKinds[signal.kind] || 0) + 1;
        if (!dropped && ((signal.kind === 'offer' && ['offer-rejected', 'offer-lost'].includes(fault)) || (signal.kind === 'answer' && fault === 'answer-lost'))) {
          dropped = true; if (fault === 'offer-rejected') throw new Error('Simulated connection reset'); return;
        }
        if (fault === 'ice-lost' && signal.kind === 'ice' && Date.now() - beginning < 5000) { dropped = true; return; }
        const latency = fault === 'jitter' ? [20, 250, 800, 1500][signals % 4] : 0;
        setTimeout(() => pages.get(recipient)?.evaluate((value) => window.testCall.signal(value), signal).catch((error) => errors.push(error.message)), latency);
      });
      await page.route('**/mx/**', (route) => route.fulfill({ status: 404 }));
      await page.route('**/network-probe?*', (route) => route.fulfill({ contentType: 'text/html', body: html('/src/lib/call/fixtures/NetworkHarness.svelte') }));
      await page.goto(`${origin}/network-probe?user=${uid}&users=${users.join(',')}`);
      await page.waitForFunction(() => !!window.testCall);
      await page.locator('#start').click(); await page.waitForFunction(() => window.started);
    }
    beginning = Date.now();
    await Promise.all([...pages.values()].map((page) => page.evaluate(() => window.testCall.connect())));
    const connected = async () => Promise.all([...pages.values()].map((page) => until(async () => {
      const state = await page.evaluate(() => window.testCall.state());
      const count = users.length - 1;
      return state.peers.length === count && state.peers.every((peer) => peer.state === 'connected' && peer.signaling === 'stable')
        && state.stats.filter((stat) => stat.kind === 'audio' && stat.bytes > 0).length >= count
        && state.stats.filter((stat) => stat.kind === 'video' && stat.frames > 0).length >= count;
    }, `${order.join('/')} ${fault}: connected native audio/video`, 45000)));
    await connected();
    if (fault !== 'jitter') assert.equal(dropped, true, `${fault} must actually be injected: ${JSON.stringify(signalKinds)}`);
    for (const [uid, page] of pages) {
      if (fault === 'jitter') console.log(JSON.stringify({ order, stage: `share-${uid}` }));
      await Promise.all([...pages.values()].map((viewer) => viewer.evaluate((source) => window.testCall.source(source, true), uid)));
      await page.locator('#share').click(); await until(async () => (await page.evaluate(() => window.testCall.state())).sharing, 'screen capture started');
      await connected();
      for (const [other, viewer] of pages) if (other !== uid) await presentedFrames(viewer, uid, true);
      await bounded(page.evaluate(() => window.testCall.stop()), `stop screen: ${uid}`);
      await Promise.all([...pages.values()].map((viewer) => viewer.evaluate((source) => window.testCall.source(source, false), uid)));
      await connected();
      for (const [other, viewer] of pages) if (other !== uid) await presentedFrames(viewer, uid, false);
    }
    // Immediate leave/rejoin must use the new session and recover every peer.
    const sourceUid = users[0];
    if (fault === 'jitter') console.log(JSON.stringify({ order, stage: 'rejoin' }));
    await Promise.all([...pages].filter(([uid]) => uid !== sourceUid).map(([, page]) => page.evaluate((uid) => window.testCall.remove(uid), sourceUid)));
    await bounded(pages.get(sourceUid).evaluate(() => window.testCall.rejoin()), 'rejoin');
    await Promise.all([...pages.values()].map((page) => page.evaluate((uid) => window.testCall.connect({ [uid]: 2 }), sourceUid)));
    await connected();
    // Old-session deliveries can arrive AFTER participant.left removed its PC.
    // They must not replace the fresh rejoined peer, even during rediscovery.
    for (const [uid, page] of pages) if (uid !== sourceUid) {
      await page.evaluate((signal) => window.testCall.signal(signal), { senderUid: sourceUid, senderSessionUid: `${sourceUid}-1`, recipientSessionUid: `${uid}-1`, kind: 'offer', data: { type: 'offer', sdp: 'obsolete-session' } });
      await page.evaluate(() => window.testCall.connect());
    }
    await connected();
    const states = {};
    for (const [uid, page] of pages) { states[uid] = await bounded(page.evaluate(() => window.testCall.state()), `final stats: ${uid}`); assert.deepEqual(states[uid].errors, []); }
    assert.deepEqual(errors, []);
    results.push({ order, fault, users: users.length, signals, passed: true, states });
    console.log(JSON.stringify({ order, fault, users: users.length, passed: true }));
  } catch (error) {
    const states = {};
    for (const [uid, page] of pages) {
      states[uid] = await bounded(page.evaluate(async () => ({ ...await window.testCall.state(), videos: [...document.querySelectorAll('video')].map((video) => ({ className: video.className, width: video.videoWidth, ready: video.readyState, paused: video.paused, time: video.currentTime, quality: video.getVideoPlaybackQuality() })) })), `failure stats: ${uid}`, 5000).catch((error) => ({ diagnosticError: error.message }));
      await page.screenshot({ path: join(artifacts, `${order.join('-')}-${fault}-${uid}-failure.png`) }).catch(() => undefined);
    }
    results.push({ order, fault, passed: false, signals, signalKinds, states, errors });
    throw error;
  } finally { for (const page of pages.values()) { await bounded(page.evaluate(() => window.testCall.close()), 'cleanup', 2000).catch(() => undefined); await page.close(); } }
}

async function recordDeletionUI(browser, engine) {
  const page = await browser.newPage(); const held = []; let hold = false;
  const field = { uid: 'name', key: 'name', label: 'Name', field_type: 'text', active: true, required: false, config: {}, position: 0, table_priority: 0, table_visible: true, unique_value: false, searchable: true, sortable: true };
  const row = { uid: 'deleted-row', revision: 1, values: { name: 'Must stay deleted' }, attached_files: [] };
  const response = (rows) => ({ json: { data: rows, page: 1, total_pages: 1, total: rows.length, has_next: false, limit: 50 } });
  await page.route('**/lifecycle-probe', (route) => route.fulfill({ contentType: 'text/html', body: html('/src/lib/call/fixtures/RecordLifecycleHarness.svelte') }));
  await page.route('**/mx/**', (route) => {
    if (new URL(route.request().url()).pathname.endsWith('/schema')) return route.fulfill({ json: { revision: 1, fields: [field], system_fields: [] } });
    if (hold) { held.push(route); return; }
    return route.fulfill(response([row]));
  });
  try {
    await page.goto(`${origin}/lifecycle-probe`);
    await page.getByText('Must stay deleted', { exact: true }).waitFor();
    hold = true;
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await until(() => held.length === 1, 'pre-delete list request');
    await page.evaluate(() => window.recordLifecycle({ type: 'record.deleted', sequence: 10, payload: { record_uid: 'deleted-row', module_uid: 'mx-default-records' } }));
    // Deletion must take effect immediately, even before a slow REST refresh.
    await page.getByText('Must stay deleted', { exact: true }).waitFor({ state: 'hidden', timeout: 3000 });
    await until(() => held.length === 2, 'post-delete list request');
    await held[1].fulfill(response([])); await held[0].fulfill(response([row])); held.length = 0;
    await page.evaluate(() => window.recordLifecycle({ type: 'module.updated', sequence: 11, payload: { module_uid: 'mx-default-records' } }));
    await until(() => held.length === 1, 'module-update list request');
    // Even an old snapshot returned by an intermediary must not resurrect it.
    await held[0].fulfill(response([row, { ...row, uid: 'survivor', values: { name: 'Updated other row' } }])); held.length = 0;
    await page.getByText('Updated other row', { exact: true }).waitFor();
    assert.equal(await page.getByText('Must stay deleted', { exact: true }).count(), 0);
    await page.evaluate(() => window.recordLifecycle({ type: 'record.restored', sequence: 12, payload: { record_uid: 'deleted-row', module_uid: 'mx-default-records' } }));
    await until(() => held.length === 1, 'explicit restore refresh');
    await held[0].fulfill(response([{ ...row, revision: 2 }])); held.length = 0;
    await page.getByText('Must stay deleted', { exact: true }).waitFor();
    // An older delete event must not undo the newer explicit restore.
    await page.evaluate(() => window.recordLifecycle({ type: 'record.deleted', sequence: 10, payload: { record_uid: 'deleted-row', module_uid: 'mx-default-records' } }));
    await until(() => held.length === 1, 'out-of-order event refresh');
    await held[0].fulfill(response([{ ...row, revision: 2 }])); held.length = 0;
    await page.getByText('Must stay deleted', { exact: true }).waitFor();
    // Lost restore notifications are reconciled against an authoritative GET.
    await page.evaluate(() => window.recordLifecycle({ type: 'record.deleted', sequence: 20, payload: { record_uid: 'deleted-row', module_uid: 'mx-default-records' } }));
    await page.getByText('Must stay deleted', { exact: true }).waitFor({ state: 'hidden' });
    await until(() => held.length === 1, 'second deletion refresh'); await held[0].fulfill(response([])); held.length = 0;
    await page.evaluate(() => window.recordLifecycle({ type: 'sync.required' }));
    await until(() => held.length >= 2, 'resync GET and list');
    const get = held.find((route) => new URL(route.request().url()).pathname.endsWith('/deleted-row'));
    assert.ok(get); await get.fulfill({ json: { ...row, revision: 4 } });
    for (const route of held.filter((route) => route !== get)) await route.fulfill(response([])); held.length = 0;
    await until(() => held.length === 1, 'confirmed restore list');
    await held[0].fulfill(response([{ ...row, revision: 4 }])); held.length = 0;
    await page.getByText('Must stay deleted', { exact: true }).waitFor();
    // Server process restart resets the live sequence counter, not record
    // revisions. New lifecycle events must not be suppressed by the old epoch.
    await page.evaluate(() => window.recordLifecycle({ type: 'record.deleted', sequence: 1, payload: { record_uid: 'deleted-row', module_uid: 'mx-default-records' } }));
    await page.getByText('Must stay deleted', { exact: true }).waitFor({ state: 'hidden', timeout: 3000 });
    await until(() => held.length === 1, 'new server epoch deletion refresh');
    await held[0].fulfill(response([])); held.length = 0;
    results.push({ engine, test: 'deleted-record-stays-hidden-through-delayed-responses-and-module-updates-until-explicit-restore', passed: true });
    console.log(JSON.stringify(results.at(-1)));
  } finally { await page.close(); }
}

try {
  await server.listen();
  for (const engine of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
    const executablePath = process.env[`MX_${engine.toUpperCase()}_EXECUTABLE`] || undefined;
    const browser = await engines[engine].launch({ headless: true, env: browserAudio, executablePath, ...(engine === 'chromium' ? { args: ['--no-sandbox', '--mute-audio', '--autoplay-policy=no-user-gesture-required'] } : {}), ...(engine === 'firefox' ? { firefoxUserPrefs: { 'media.autoplay.default': 0 } } : {}) });
    browsers.set(engine, browser);
    if (process.env.MX_TEST_DELETION_ONLY) { await recordDeletionUI(browser, engine); continue; }
    if (!process.env.MX_TEST_MIXED_ONLY) {
      if (!process.env.MX_TEST_MEDIA_ONLY) {
        await startupUI(browser, engine); await recordUI(browser, engine); await recordTableUI(browser, engine); await chatUI(browser, engine); await fileBody(browser, engine);
        await recordDeletionUI(browser, engine);
      }
      if (!process.env.MX_TEST_UI_ONLY) for (const fault of (process.env.MX_TEST_FAULTS || 'offer-rejected,offer-lost,answer-lost,ice-lost').split(',')) await mediaGroup([engine], fault);
    }
  }
  if (!process.env.MX_TEST_UI_ONLY && !process.env.MX_TEST_DELETION_ONLY && browsers.size === 3) for (const order of [['chromium', 'firefox', 'webkit'], ['webkit', 'firefox', 'chromium']]) await mediaGroup(order, 'jitter', ['a', 'm', 'z']);
  await writeFile(join(artifacts, 'results.json'), JSON.stringify(results, null, 2));
  console.log(JSON.stringify({ passed: true, cases: results.length, artifacts }));
} catch (error) {
  await writeFile(join(artifacts, 'results.json'), JSON.stringify({ results, failure: error.stack }, null, 2));
  console.error(`Artifacts: ${artifacts}`); throw error;
} finally { for (const browser of browsers.values()) await browser.close(); await server.close(); }
