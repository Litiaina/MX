// Real HTTP/auth/SQLite/WebSocket + actual production call UI and native RTP.
// Only camera/microphone/display capture sources are deterministic fixtures.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, firefox, webkit } from 'playwright';
import { createServer } from 'vite';
import { startIsolatedMx } from './isolated-mx.mjs';
import { isolatedBrowserAudio } from './silent-audio.mjs';
import { playbackTones } from './playback-probe.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const browserAudio = process.env.MX_TEST_API_ONLY ? null : isolatedBrowserAudio();
const mx = await startIsolatedMx(root);
const results = []; const failures = []; const pages = []; const browsers = []; const network = [];
let server;
const count = 8;
async function until(check, description, timeout = 45000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    let timer;
    try { if (await Promise.race([check(), new Promise((_resolve, reject) => timer = setTimeout(() => reject(new Error(`Stalled: ${description}`)), Math.max(1, deadline - Date.now())))])) return; }
    finally { clearTimeout(timer); }
    await new Promise((resolve) => setTimeout(resolve, 150));
  }
  throw new Error(`Timed out: ${description}`);
}
function passed(test, details = {}) { const result = { test, passed: true, ...details }; results.push(result); const { states, ...summary } = result; console.log(JSON.stringify(summary)); }
const timings = [];
async function call(path, user, method = 'GET', data, expected = [200]) {
  const result = await mx.call(path, user?.access_token, method, data, expected); timings.push(result.milliseconds); return result;
}
async function state(page) { return page.evaluate(() => window.testEight.state()); }
async function connected() {
  await Promise.all(pages.map((page) => until(async () => {
    const value = await state(page);
    return value.joined && value.connected && value.peers.length === count - 1 && value.peers.every((peer) => peer.state === 'connected' && peer.signaling === 'stable')
      && value.stats.filter((stat) => stat.kind === 'audio' && stat.bytes > 0).length >= count - 1
      && value.stats.filter((stat) => stat.kind === 'video' && stat.frames > 0).length >= count - 1;
  }, 'all 56 native peer connections and inbound audio/video')));
}
async function movingVideo(page, name, screen) {
  const hashes = new Set();
  await until(async () => {
    const frame = await page.getByLabel(`${name} call video`, { exact: true }).evaluate((video) => {
      if (!video.srcObject || video.paused || video.readyState < 2 || !video.videoWidth) return null;
      const canvas = document.createElement('canvas'); canvas.width = 320; canvas.height = 180;
      const context = canvas.getContext('2d'); context.drawImage(video, 0, 0, 320, 180);
      const pixels = context.getImageData(0, 0, 320, 180).data;
      let hash = 0; for (let index = 0; index < pixels.length; index += 4) hash = (hash * 31 + pixels[index] + pixels[index + 1] * 3 + pixels[index + 2] * 7) | 0;
      return { hash, red: pixels[0], green: pixels[1], blue: pixels[2] };
    });
    if (frame && (screen ? frame.red < 30 && frame.green > 70 && frame.blue > 50 : frame.red > 40 && frame.green < 40 && frame.blue > 40)) hashes.add(frame.hash);
    return hashes.size >= 4;
  }, `four changing ${screen ? 'screen' : 'camera'} frames from ${name}`, 20000);
}
async function allViewers(source, screen) {
  await Promise.all(pages.filter((_page, index) => index !== source).map((page) => movingVideo(page, `Tester ${source}`, screen)));
}
async function advancingMedia(seconds = 3) {
  const before = await Promise.all(pages.map(state));
  const deadline = Date.now() + seconds * 1000;
  await until(async () => Date.now() >= deadline, 'media sampling interval', seconds * 1000 + 2000);
  const after = await Promise.all(pages.map(state));
  for (const [index, value] of after.entries()) {
    assert.ok(value.clock > before[index].clock + seconds * 3, `UI timer stalled for user ${index}`);
    for (let peer = 0; peer < count - 1; peer++) for (const kind of ['audio', 'video']) {
      // Stopped screen-audio transceivers retain historical stats. Require a
      // live microphone/camera flow on EVERY peer, not the retired screen flow.
      const flowing = before[index].stats.filter((stat) => stat.id.startsWith(`${peer}:`) && stat.kind === kind).some((stat) => {
        const current = value.stats.find((item) => item.id === stat.id);
        return current && current.bytes > stat.bytes && (kind !== 'video' || current.frames > stat.frames);
      });
      assert.ok(flowing, `Live RTP ${kind} stalled for user ${index}, peer ${peer}`);
    }
  }
  return after;
}

try {
  console.log(`Isolated test artifacts: ${mx.directory}`);
  const password = `Test-only!${randomUUID()}`;
  const created = await mx.api.post('/mx/v1/auth/create', { headers: { Authorization: `Bearer ${mx.signupKey}` }, data: { email: 'admin@test.invalid', name: 'Test Admin', password } });
  assert.equal(created.status(), 201, await created.text());
  const admin = (await call('/mx/v1/auth/authenticate', null, 'POST', { email: 'admin@test.invalid', password })).body;
  await Promise.all(Array.from({ length: count }, (_value, index) => call('/mx/v1/user/create', admin, 'POST', { email: `tester${index}@test.invalid`, name: `Tester ${index}`, password, access_level: 2 }, [201])));
  const users = await Promise.all(Array.from({ length: count }, async (_value, index) => (await call('/mx/v1/auth/authenticate', null, 'POST', { email: `tester${index}@test.invalid`, password })).body));
  const sessions = await Promise.all(users.map(async (user) => (await call('/mx/v1/auth/session', user)).body));
  const module = (await call('/mx/v1/admin/modules', admin, 'POST', { name: 'Concurrency test', singular_name: 'Entry' }, [201])).body;
  const moduleUid = module.uid || module.module?.uid;
  assert.ok(moduleUid, JSON.stringify(module));
  await Promise.all(users.map(async (user) => {
    const access = (await call(`/mx/v1/admin/accounts/${user.uid}/modules`, admin)).body;
    await call(`/mx/v1/admin/accounts/${user.uid}/modules`, admin, 'PUT', { revision: access.revision, grants: [{ module_uid: moduleUid, can_read: true, can_create: true, can_update: true, can_delete: false, can_configure: false, can_report: true, can_attachments: true }] });
  }));
  for (let index = 0; index < count; index++) await call(`/mx/v1/admin/modules/${moduleUid}/fields`, admin, 'POST', { key: `field_${index}`, label: `Field ${index}`, field_type: 'text' }, [201]);
  const records = `/mx/v1/modules/${moduleUid}/records`;
  const values = Object.fromEntries(users.map((_user, index) => [`field_${index}`, 'original']));
  const record = (await call(records, users[0], 'POST', { operation_uid: randomUUID(), values }, [201])).body;
  const saved = await Promise.all(users.map((user, index) => call(`${records}/${record.uid}`, user, 'PATCH', { base_revision: 1, changes: { [`field_${index}`]: `user-${index}` } })));
  const merged = (await call(`${records}/${record.uid}`, users[0])).body;
  assert.equal(merged.revision, 9);
  for (let index = 0; index < count; index++) assert.equal(merged.values[`field_${index}`], `user-${index}`);
  let versions = (await call(`${records}/${record.uid}/versions`, users[0])).body.versions;
  assert.equal(versions.length, 9); assert.equal(new Set(versions.map((version) => version.actor_uid)).size, 8);
  passed('eight authenticated users merge concurrent unrelated fields atomically', { versions: versions.length, saveMilliseconds: saved.map((value) => value.milliseconds) });

  const conflicting = await Promise.all(users.map((user, index) => call(`${records}/${record.uid}`, user, 'PATCH', { base_revision: 9, changes: { field_0: `proposal-${index}` } }, [200, 409])));
  assert.equal(conflicting.filter((result) => result.status === 200).length, 1);
  const winner = (await call(`${records}/${record.uid}`, users[0])).body;
  for (const [index, result] of conflicting.entries()) if (result.status === 409) {
    assert.equal(result.body.conflicts.length, 1);
    assert.equal(result.body.conflicts[0].current_value, winner.values.field_0);
    assert.equal(result.body.conflicts[0].your_value, `proposal-${index}`);
  }
  const loser = conflicting.findIndex((result) => result.status === 409);
  await call(`${records}/${record.uid}`, users[loser], 'PATCH', { base_revision: winner.revision, changes: { field_0: `resolved-${loser}` } });
  versions = (await call(`${records}/${record.uid}/versions`, users[0])).body.versions;
  assert.equal(versions.length, 11);
  passed('eight same-cell saves produce one winner, seven precise conflicts, and audited resolution');

  const operation = randomUUID();
  const replays = await Promise.all(users.map(async (user) => Promise.all(Array.from({ length: 8 }, () => call(records, user, 'POST', { operation_uid: operation, values }, [200, 201])))));
  assert.equal(new Set(replays.flat().map((value) => value.body.uid)).size, count);
  for (const replay of replays) { assert.equal(new Set(replay.map((value) => value.body.uid)).size, 1); assert.equal(replay.filter((value) => value.status === 201).length, 1); }
  assert.equal((await call(records, users[0])).body.total, 9);
  await call(records, users[0], 'POST', { operation_uid: operation, values: { ...values, field_0: 'changed payload' } }, [409]);
  passed('64 simultaneous real record POSTs commit exactly eight account-scoped creates');

  const channel = (await call('/mx/v1/collaboration/channels', users[0], 'POST', { name: 'Eight-user validation', member_uids: users.slice(1).map((user) => user.uid) }, [201])).body.uid;
  const channelPath = `/mx/v1/collaboration/channels/${channel}`;
  const messages = await Promise.all(users.map(async (user) => {
    const operation_uid = randomUUID();
    return Promise.all(Array.from({ length: 8 }, () => call(`${channelPath}/messages`, user, 'POST', { body: 'Concurrent test message', operation_uid }, [200, 201])));
  }));
  for (const replay of messages) assert.equal(new Set(replay.map((value) => value.body.message.uid)).size, 1);
  assert.equal((await call(`${channelPath}/messages`, users[0])).body.messages.length, 8);
  passed('64 concurrent real chat POSTs store exactly eight messages');

  const deletedOperation = randomUUID();
  const victim = (await call(records, users[0], 'POST', { operation_uid: deletedOperation, values }, [201])).body;
  await call(`${records}/${victim.uid}`, admin, 'DELETE', undefined, [204]);
  await Promise.all(users.map(async (user, index) => {
    await call(`/mx/v1/admin/modules/${moduleUid}`, admin, 'PUT', { description: `Concurrent module update ${index}` });
    await call(`${records}/${victim.uid}`, user, 'PATCH', { base_revision: 1, changes: { [`field_${index}`]: 'stale draft' } }, [404]);
    await call(`${records}/${victim.uid}`, user, 'PUT', { values }, [404]);
    assert.equal((await call(records, user)).body.data.some((row) => row.uid === victim.uid), false);
  }));
  await call(records, users[0], 'POST', { operation_uid: deletedOperation, values }, [404]);
  assert.equal((await call(records, users[0])).body.total, 9);
  passed('module updates and eight stale PATCH/PUT saves cannot resurrect a deleted record or replayed create');
  await call(`/mx/v1/admin/trash/${victim.uid}/restore`, admin, 'POST');
  const restored = (await call(`${records}/${victim.uid}`, users[0])).body;
  assert.ok(restored.revision > victim.revision, 'An explicit restore must advance the concurrency revision');
  await Promise.all(users.map((user, index) => call(`${records}/${victim.uid}`, user, 'PATCH', { base_revision: victim.revision, changes: { [`field_${index}`]: 'pre-delete draft' } }, [409])));
  passed('only explicit administrator restore revives a record; pre-delete drafts cannot silently overwrite it');

  const uploadResults = await Promise.all(users.map(async (user, index) => {
    const buffer = Buffer.alloc(18 * 1024 * 1024, index + 1);
    const response = await mx.api.post(`/mx/v1/collaboration/messages/${messages[index][0].body.message.uid}/files`, { headers: { Authorization: `Bearer ${user.access_token}` }, multipart: { files: { name: `large-${index}.bin`, mimeType: 'application/octet-stream', buffer } } });
    assert.equal(response.status(), 201, await response.text());
    return response.json();
  }));
  assert.deepEqual(mx.n1.failures, []);
  assert.equal(mx.n1.sessions.size, 8);
  for (const session of mx.n1.sessions.values()) { assert.equal(session.parameters.expected_parts, 2); assert.equal(session.failedOnce, true); }
  assert.equal(mx.n1.requests.filter((request) => request.method === 'PUT').length, 24, 'Each upload retries one failed part, then sends its remaining part');
  assert.equal(mx.n1.objects.size, 8);
  if (mx.n1.live && process.env.MX_TEST_N1_DROP_FINALIZE) {
    assert.ok([...mx.n1.objects.values()].every((object) => object.finalizeAcknowledgementDropped));
    assert.equal(mx.n1.requests.filter((request) => request.path.startsWith('/noa/v1/objects/meta/')).length, 8);
    passed('all eight lost live-N1 finalize acknowledgements reconcile exact committed versions without duplicate uploads');
  }
  for (const [index, result] of uploadResults.entries()) {
    const file = result.file;
    assert.ok(file?.uid, JSON.stringify(result));
    const downloaded = await mx.api.get(`/mx/v1/collaboration/files/${file.uid}/download`, { headers: { Authorization: `Bearer ${users[index].access_token}` } });
    assert.equal(downloaded.status(), 200);
    const data = await downloaded.body(); assert.equal(data.length, 18 * 1024 * 1024); assert.ok(data.every((byte) => byte === index + 1));
    const ticket = (await call(`/mx/v1/collaboration/files/${file.uid}/preview-ticket`, users[index], 'POST', undefined, [201])).body;
    const partial = await mx.api.get(ticket.url, { headers: { Range: 'bytes=1024-2047' } });
    assert.equal(partial.status(), 206); assert.equal(partial.headers()['content-range'], `bytes 1024-2047/${18 * 1024 * 1024}`);
    assert.deepEqual(await partial.body(), Buffer.alloc(1024, index + 1));
    const invalid = await mx.api.get(ticket.url, { headers: { Range: `bytes=${18 * 1024 * 1024}-` } });
    // Live N1's ordinary-object reader currently ignores unsatisfiable ranges.
    // Verify its complete 200 body rather than fabricating a 206/416 response.
    if (mx.n1.live && invalid.status() === 200) assert.deepEqual(await invalid.body(), Buffer.alloc(18 * 1024 * 1024, index + 1));
    else assert.equal(invalid.status(), 416);
  }
  passed('eight concurrent 18-MiB uploads; bounded 16-MiB parts, injected 503 retries, and exact downloaded bytes', { storage: mx.n1.live ? 'live N1 v4 through fault-injecting proxy' : 'isolated HTTP fixture, not live N1' });
  passed('all eight attachment preview tickets preserve exact 206 byte ranges and upstream unsatisfiable-range behavior');
  if (mx.n1.live) {
    const buffer = Buffer.alloc(94 * 1024 * 1024);
    for (let index = 0; index < buffer.length; index++) buffer[index] = index % 251;
    const uploaded = await mx.api.post(`/mx/v1/collaboration/messages/${messages[0][0].body.message.uid}/files`, { timeout: 120000, headers: { Authorization: `Bearer ${users[0].access_token}` }, multipart: { files: { name: '94-MiB-range-test.bin', mimeType: 'application/octet-stream', buffer } } });
    assert.equal(uploaded.status(), 201, await uploaded.text());
    const file = (await uploaded.json()).file;
    const session = [...mx.n1.sessions.values()].find((session) => session.parameters.total_size_bytes === buffer.length);
    assert.equal(session?.parameters.expected_parts, 6);
    const downloaded = await mx.api.get(`/mx/v1/collaboration/files/${file.uid}/download`, { timeout: 120000, headers: { Authorization: `Bearer ${users[0].access_token}` } });
    assert.equal(downloaded.status(), 200); assert.deepEqual(await downloaded.body(), buffer);
    const ticket = (await call(`/mx/v1/collaboration/files/${file.uid}/preview-ticket`, users[0], 'POST', undefined, [201])).body;
    for (const [start, end] of [[0, 65535], [buffer.length - 4096, buffer.length - 1]]) {
      const partial = await mx.api.get(ticket.url, { headers: { Range: `bytes=${start}-${end}` } });
      assert.equal(partial.status(), 206); assert.equal(partial.headers()['content-range'], `bytes ${start}-${end}/${buffer.length}`);
      assert.deepEqual(await partial.body(), buffer.subarray(start, end + 1));
    }
    passed('94-MiB live-N1 file uses six bounded parts, downloads byte-identically, and previews beginning/end without fetching the complete file');
  }
  const attachmentField = (await call(`/mx/v1/admin/modules/${moduleUid}/fields`, admin, 'POST', { key: 'attachments', label: 'Attachments', field_type: 'attachments', config: { multiple: true, storage_name: 'Attachments' } }, [201])).body;
  const fieldUid = attachmentField.uid || attachmentField.field?.uid;
  assert.ok(fieldUid, JSON.stringify(attachmentField));
  await Promise.all(users.map(async (user, index) => {
    const response = await mx.api.post(`/mx/v1/records/${victim.uid}/attachments/fields/${fieldUid}`, { headers: { Authorization: `Bearer ${user.access_token}` }, multipart: { base_revision: String(restored.revision), files: { name: `independent-${index}.bin`, mimeType: 'application/octet-stream', buffer: Buffer.alloc(1024, index) } } });
    assert.equal(response.status(), 201, await response.text());
  }));
  const withFiles = (await call(`${records}/${victim.uid}`, users[0])).body;
  assert.equal(withFiles.attached_files.length, 8); assert.equal(withFiles.revision, restored.revision + 8);
  assert.equal(new Set(withFiles.attached_files.map((file) => file.uid)).size, 8);
  passed('eight independent record attachment additions merge from one base revision');
  await call(`${records}/${victim.uid}`, users[0], 'PATCH', { base_revision: withFiles.revision, changes: { field_2: null } });
  await Promise.all(users.map((user) => call(`${records}/${victim.uid}`, user, 'PATCH', { base_revision: withFiles.revision, changes: { field_2: 'stale value' } }, [409])));
  // Full-record replacement must not bypass PATCH's protection and revive a
  // cleared value from an old snapshot, even when that snapshot supplies a base.
  await call(`${records}/${victim.uid}`, users[0], 'PUT', { base_revision: withFiles.revision, values: withFiles.values }, [409]);
  await call(`${records}/${victim.uid}`, users[0], 'PUT', { values: withFiles.values }, [428]);
  await call(`${records}/${victim.uid}`, users[1], 'PATCH', { base_revision: withFiles.revision, changes: { field_3: 'independent edit' } });
  await Promise.all(users.map((_user, index) => call(`/mx/v1/admin/modules/${moduleUid}`, admin, 'PUT', { description: `Cleared-cell refresh ${index}` })));
  const cleared = (await call(`${records}/${victim.uid}`, users[0])).body;
  assert.equal(cleared.values.field_2 ?? null, null); assert.equal(cleared.values.field_3, 'independent edit');
  const replacements = await Promise.all(users.map((user, index) => call(`${records}/${victim.uid}`, user, 'PUT', { base_revision: cleared.revision, values: { ...cleared.values, field_0: `replacement-${index}` } }, [200, 409])));
  assert.equal(replacements.filter((result) => result.status === 200).length, 1);
  assert.equal(replacements.filter((result) => result.status === 409).length, 7);
  assert.equal((await call(`${records}/${victim.uid}`, users[0])).body.values.field_2 ?? null, null);
  passed('cleared cells survive stale PATCH/PUT and module updates; full replacements require a base and only one concurrent replacement commits');
  const schema = (await call(`/mx/v1/modules/${moduleUid}/schema`, admin)).body;
  const archivedField = schema.fields.find((field) => field.key === 'field_4');
  await call(`/mx/v1/admin/schema/fields/${archivedField.uid}`, admin, 'PUT', { active: false });
  await Promise.all(users.map(async (user, index) => {
    await call(`/mx/v1/admin/modules/${moduleUid}`, admin, 'PUT', { description: `Archived-field refresh ${index}` });
    assert.equal((await call(`/mx/v1/modules/${moduleUid}/schema`, user)).body.fields.some((field) => field.uid === archivedField.uid), false);
    assert.equal(Object.hasOwn((await call(`${records}/${victim.uid}`, user)).body.values, 'field_4'), false);
  }));
  assert.equal((await call(`/mx/v1/modules/${moduleUid}/schema`, admin)).body.fields.find((field) => field.uid === archivedField.uid).active, false);
  passed('archived fields stay archived through eight module updates and remain absent from ordinary users records/schema');
  assert.deepEqual(mx.n1.failures, []);
  mx.n1.objects.clear(); mx.n1.sessions.clear();

  if (process.env.MX_TEST_API_ONLY) { passed('API-only validation complete'); }
  else {

  server = await createServer({ root, logLevel: 'error', server: { host: '127.0.0.1', port: 5198, strictPort: true, hmr: false, proxy: { '/mx': { target: `https://127.0.0.1:${mx.port}`, secure: false, ws: true } } } });
  await server.listen();
  const engines = { chromium, firefox, webkit };
  const order = (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',');
  for (const engine of order) browsers.push(await engines[engine].launch({ headless: true, env: browserAudio, executablePath: process.env[`MX_${engine.toUpperCase()}_EXECUTABLE`] || undefined,
    ...(engine === 'chromium' ? { ignoreDefaultArgs: ['--mute-audio'] } : {}),
    // Real playback is measured at the verified null sink. --mute-audio would
    // invalidate that measurement; physical/default outputs remain forbidden.
    ...(engine === 'chromium' ? { args: ['--no-sandbox', '--autoplay-policy=no-user-gesture-required', '--disable-background-timer-throttling', '--disable-renderer-backgrounding'] } : {}),
    ...(engine === 'firefox' ? { firefoxUserPrefs: { 'media.autoplay.default': 0 } } : {}) }));
  async function participantPage(index) {
    const user = users[index];
    const page = await browsers[index % browsers.length].newPage({ viewport: { width: 1280, height: 900 }, ...(order[index % browsers.length] === 'chromium' ? { permissions: ['local-network-access'] } : {}) });
    page.on('pageerror', (error) => failures.push(`user ${index}: ${error.message}`)); page.on('crash', () => failures.push(`user ${index}: native page crashed`));
    page.on('response', (response) => { if (response.url().includes('/mx/')) network.push({ user: index, path: new URL(response.url()).pathname, status: response.status() }); });
    page.on('requestfailed', (request) => network.push({ user: index, path: new URL(request.url()).pathname, error: request.failure()?.errorText }));
    page.on('websocket', (socket) => { network.push({ user: index, socket: 'created' }); socket.on('socketerror', (error) => network.push({ user: index, socketError: error })); socket.on('close', () => network.push({ user: index, socket: 'closed' })); });
    await page.addInitScript((user) => { sessionStorage.setItem('mx_access_token', user.access_token); sessionStorage.setItem('mx_refresh_token', user.refresh_token); }, user);
    await page.route('**/eight-user-probe', (route) => route.fulfill({ contentType: 'text/html', body: `<html><head><meta name="viewport" content="width=device-width,initial-scale=1"></head><body><div id="app"></div><script type="module">import{mount}from'/node_modules/.vite/deps/svelte.js';import'/src/styles/tokens.css';import'/src/styles/foundation.css';import'/src/styles/app.css';import'/src/styles/workspaces.css';import Harness from'/src/lib/call/fixtures/EightUserHarness.svelte';mount(Harness,{target:document.getElementById('app'),props:${JSON.stringify({ session: sessions[index], channelUid: channel, index })}});</script></body></html>` }));
    await page.goto('http://127.0.0.1:5198/eight-user-probe');
    await page.waitForFunction(() => !!window.testEight);
    await until(async () => (await state(page)).connected, 'real authenticated live WebSocket');
    return page;
  }
  for (let index = 0; index < count; index++) pages.push(await participantPage(index));
  let lostOffer = false;
  for (const page of pages) await page.route('**/call/signal', (route) => {
    if (!lostOffer && route.request().postDataJSON().kind === 'offer') { lostOffer = true; return route.abort('failed'); }
    return route.continue();
  });
  const joinedAt = Date.now();
  await Promise.all(pages.map((page) => page.locator('#join').click()));
  await connected(); assert.equal(lostOffer, true);
  assert.equal((await call(`${channelPath}/call`, users[0])).body.participants.length, 8);
  const initial = await advancingMedia();
  passed('eight-person mixed-browser production call UI with real HTTP/WebSocket signaling and 56 live RTP peer connections', { browsers: order, joinMilliseconds: Date.now() - joinedAt, lostOfferRecovered: true, states: initial });

  // Every sender is checked on all seven recipients, not just connection state.
  for (let index = 0; index < count; index++) {
    await pages[index].getByRole('button', { name: 'Share screen', exact: true }).click(); await allViewers(index, true);
    await pages[index].getByRole('button', { name: 'Stop screen sharing', exact: true }).click(); await allViewers(index, false);
    console.log(JSON.stringify({ stage: 'screen-camera-switch', user: index, verifiedRecipients: 7 }));
  }
  await advancingMedia();
  passed('all eight users screen-share then return to moving camera on every recipient', { verifiedSourceTransitions: 112 });

  await Promise.all(pages.slice(0, 2).map((page) => page.getByRole('button', { name: 'Share screen', exact: true }).click()));
  await Promise.all([allViewers(0, true), allViewers(1, true)]);
  // Isolate one recipient's actual output at a time, using the production
  // deafen controls. Each tone must reach the null-sink monitor through its
  // HTML audio element, not through a separate test-only decoding path.
  // Keep the source undeafened: deafen also deliberately mutes its microphone.
  // Its output contains neither of its own tones and cannot affect this probe.
  await Promise.all(pages.slice(1).map((page) => page.getByRole('button', { name: 'Deafen', exact: true }).click()));
  // The source's own output is not under test. Silence it without deafen,
  // which would also mute the very outgoing microphone being measured.
  await pages[0].locator('audio').evaluateAll((elements) => elements.forEach((element) => element.muted = true));
  async function renderedAudio(ready = () => true) {
    const readings = [];
    for (const page of pages.slice(1)) {
      await page.getByRole('button', { name: 'Undeafen', exact: true }).click();
      try {
        let reading;
        await until(async () => { reading = await playbackTones([300, 1500]); return ready(reading); }, 'actual rendered audio settles after mute/unmute', 5000);
        readings.push(reading);
      }
      finally { await page.getByRole('button', { name: 'Deafen', exact: true }).click(); }
    }
    return readings;
  }
  const both = await renderedAudio();
  assert.ok(both.every((tones) => tones.every((db) => db > -55)), `Microphone + shared audio missing: ${JSON.stringify(both)}`);
  await pages[0].getByRole('button', { name: 'Mute microphone', exact: true }).click();
  const muted = await renderedAudio(([mic, share]) => mic < -65 && share > -55);
  assert.ok(muted.every(([mic, share]) => mic < -65 && share > -55), `Mute must affect only microphone: ${JSON.stringify(muted)}`);
  await pages[0].getByRole('button', { name: 'Unmute microphone', exact: true }).click();
  const unmuted = await renderedAudio((tones) => tones.every((db) => db > -55));
  assert.ok(unmuted.every((tones) => tones.every((db) => db > -55)), `Unmuted playback missing: ${JSON.stringify(unmuted)}`);
  await Promise.all(pages.slice(1).map((page) => page.getByRole('button', { name: 'Undeafen', exact: true }).click()));
  await pages[0].locator('audio').evaluateAll((elements) => elements.forEach((element) => element.muted = false));
  await Promise.all(pages.slice(0, 2).map((page) => page.getByRole('button', { name: 'Stop screen sharing', exact: true }).click()));
  await Promise.all([allViewers(0, false), allViewers(1, false)]);
  passed('simultaneous screen sharing and actual rendered microphone/shared-audio independence on every recipient', { renderedToneDecibels: both, mutedToneDecibels: muted, unmutedToneDecibels: unmuted });

  for (const theme of ['light', 'dark']) {
    await pages[7].evaluate((theme) => document.documentElement.dataset.theme = theme, theme);
    await pages[7].getByRole('button', { name: 'Voice and video settings', exact: true }).click();
    await pages[7].getByRole('combobox', { name: /Microphone/ }).selectOption('test-mic');
    await pages[7].screenshot({ path: join(mx.directory, `call-settings-${theme}.png`) });
    await pages[7].getByRole('button', { name: 'Close device settings', exact: true }).click();
  }
  await pages[7].getByRole('button', { name: 'Minimize call', exact: true }).click();
  const compact = await pages[7].locator('.collaboration-call').boundingBox(); assert.ok(compact.height < 150, `Oversized minimized UI: ${compact.height}`);
  await pages[7].screenshot({ path: join(mx.directory, 'call-minimized.png') });
  await pages[7].getByRole('button', { name: 'Show call', exact: true }).click();
  await pages[7].getByRole('button', { name: 'Expand call window', exact: true }).click();
  await pages[7].getByRole('button', { name: 'Restore call window', exact: true }).click();
  await pages[7].getByRole('button', { name: 'View Tester 0 full screen', exact: true }).click();
  await pages[7].waitForFunction(() => !!document.fullscreenElement);
  await movingVideo(pages[7], 'Tester 0', false);
  await pages[7].getByRole('button', { name: 'Exit Tester 0 full screen', exact: true }).click();
  await pages[7].waitForFunction(() => !document.fullscreenElement);
  passed('actual call UI: compact minimize, expand/restore, native fullscreen/exit, and light/dark device controls');

  for (let round = 0; round < 3; round++) {
    const source = round;
    const old = (await call(`${channelPath}/call`, users[source])).body.participants.find((item) => item.user_uid === users[source].uid);
    await pages[source].getByRole('button', { name: 'Leave call', exact: true }).click();
    await until(async () => (await call(`${channelPath}/call`, users[7])).body.participants.length === 7, 'server acknowledges leave');
    await pages[source].locator('#join').click(); await connected();
    await call(`${channelPath}/call?session_uid=${encodeURIComponent(old.session_uid)}`, users[source], 'DELETE');
    assert.equal((await call(`${channelPath}/call`, users[7])).body.participants.length, 8);
    await allViewers(source, false); await advancingMedia();
  }
  passed('three actual UI leave/rejoins recover every audio/video peer; delayed old leaves cannot remove new sessions');

  const previousPage = pages[0];
  const oldSession = (await call(`${channelPath}/call`, users[0])).body.participants.find((item) => item.user_uid === users[0].uid).session_uid;
  const replacement = await participantPage(0); pages[0] = replacement;
  await replacement.locator('#join').click();
  await previousPage.getByText('replaced', { exact: true }).waitFor();
  await connected(); await allViewers(0, false); await advancingMedia();
  await call(`${channelPath}/call?session_uid=${encodeURIComponent(oldSession)}`, users[0], 'DELETE');
  assert.equal((await call(`${channelPath}/call`, users[0])).body.participants.length, 8);
  await previousPage.close();
  passed('same account in a second browser tab replaces its old call session without duplicating participants or breaking other media');

  await Promise.all(pages.map((page) => page.evaluate(() => window.testEight.reconnect())));
  await connected(); const finalStates = await advancingMedia(Number(process.env.MX_TEST_SOAK_SECONDS || 15));
  assert.deepEqual(failures, []);
  for (const page of pages) assert.equal(await page.locator('.call-error').count(), 0);
  passed('eight simultaneous live reconnections and uninterrupted media/UI soak', { states: finalStates });
  await Promise.all(pages.map((page) => page.getByRole('button', { name: 'Leave call', exact: true }).click()));
  await until(async () => !(await call(`${channelPath}/call`, users[0])).body.active, 'last participant clears call-in-progress');
  assert.equal((await call('/mx/v1/collaboration/calls', users[0])).body.calls.length, 0);
  passed('all eight users leave: no phantom active call remains');
  }
} catch (error) {
  failures.push(error.stack);
  for (const [index, page] of pages.entries()) {
    await page.screenshot({ path: join(mx.directory, `failure-user-${index}.png`), timeout: 5000 }).catch(() => undefined);
    let timer;
    const diagnostic = await Promise.race([state(page), new Promise((_resolve, reject) => timer = setTimeout(() => reject(new Error('Diagnostic page stalled')), 5000))]).catch((error) => ({ error: error.message }));
    clearTimeout(timer);
    results.push({ test: `failure diagnostics user ${index}`, state: diagnostic });
  }
  process.exitCode = 1;
} finally {
  const sorted = timings.toSorted((a, b) => a - b);
  await writeFile(join(mx.directory, 'results.json'), JSON.stringify({ passed: failures.length === 0, users: count, results, failures, network, requests: timings.length, latencyMilliseconds: { p50: sorted[Math.floor(sorted.length * .5)], p95: sorted[Math.floor(sorted.length * .95)], max: sorted.at(-1) } }, null, 2));
  console.log(JSON.stringify({ passed: failures.length === 0, cases: results.filter((item) => item.passed).length, artifacts: mx.directory, failures }));
  for (const browser of browsers) await browser.close(); await server?.close(); await mx.close();
}
