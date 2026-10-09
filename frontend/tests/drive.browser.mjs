// Production MX HTTP/auth/SQLite + real browser UI. Storage can be live N1.
import assert from 'node:assert/strict';
import { randomUUID, createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { writeFile, mkdir, open } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { isolatedBrowserAudio } from './silent-audio.mjs';
import { verifyFileManager } from './drive-file-manager.mjs';
const root = fileURLToPath(new URL('../', import.meta.url));
const mx = await startIsolatedMx(root); const results = []; const failures = []; const browsers = [];
const base = '/mx/v1/drive';
const pass = (test, detail = {}) => { const entry = { test, passed: true, ...detail }; results.push(entry); console.log(JSON.stringify(entry)); };
const call = async (path, user, method = 'GET', data, expected = [200]) => (await mx.call(path, user?.access_token, method, data, expected)).body;
const headers = (user) => ({ Authorization: `Bearer ${user.access_token}` });
async function until(check, message, timeout = 30000) { const deadline = Date.now() + timeout; while (Date.now() < deadline) { if (await check()) return; await new Promise((resolve) => setTimeout(resolve, 150)); } throw new Error(`Timed out: ${message}`); }
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
async function begin(user, name, bytes, extra = {}) { const metadata = { operation_uid: randomUUID(), file_name: name, mime_type: name.endsWith('.png') ? 'image/png' : 'application/octet-stream', size: bytes.length, last_modified: 12345, ...extra }; const state = await call(`${base}/uploads`, user, 'POST', metadata); if (state.operation_uid) metadata.operation_uid=state.operation_uid; return { metadata, state }; }
async function part(user, operation, index, bytes, expected = [200]) {
  for (let attempt = 0; attempt < 3; attempt++) { const response = await mx.api.put(`${base}/uploads/${operation}/parts/${index}`, { headers: { ...headers(user), 'Content-Type': 'application/octet-stream' }, data: bytes }); if (response.status() === 502 && attempt < 2) continue; assert.ok(expected.includes(response.status()), `${response.status()} ${await response.text()}`); return response; }
}
async function upload(user, name, bytes, extra = {}) { const value = await begin(user, name, bytes, extra); for (let index = 0; index < Math.max(1, Math.ceil(bytes.length / value.state.part_size)); index++) await part(user, value.metadata.operation_uid, index, bytes.subarray(index * value.state.part_size, (index + 1) * value.state.part_size)); return { item: await call(`${base}/uploads/${value.metadata.operation_uid}/finish`, user, 'POST'), ...value }; }
const db = () => new DatabaseSync(join(mx.directory, 'test.db'));
function query(sql, ...args) { const connection = db(); try { return connection.prepare(sql).all(...args); } finally { connection.close(); } }
function execute(sql, ...args) { const connection = db(); try { connection.prepare(sql).run(...args); } finally { connection.close(); } }

try {
  console.log(`Drive test artifacts: ${mx.directory}`);
  const password = `Test-only!${randomUUID()}`;
  const created = await mx.api.post('/mx/v1/auth/create', { headers: { Authorization: `Bearer ${mx.signupKey}` }, data: { email: 'admin@test.invalid', name: 'Admin', password } }); assert.equal(created.status(), 201);
  const admin = await call('/mx/v1/auth/authenticate', null, 'POST', { email: 'admin@test.invalid', password });
  await Promise.all(Array.from({ length: 8 }, (_, index) => call('/mx/v1/user/create', admin, 'POST', { email: `drive${index}@test.invalid`, name: `Drive ${index}`, password, access_level: 2 }, [201])));
  const users = await Promise.all(Array.from({ length: 8 }, (_, index) => call('/mx/v1/auth/authenticate', null, 'POST', { email: `drive${index}@test.invalid`, password })));
  const user = users[0];
  const initialQuota = await call(`${base}/admin/quotas/${user.uid}`, admin);
  assert.equal(initialQuota.quota_bytes, 10 * 1024 ** 3); assert.equal(initialQuota.assigned, false);
  await call(`${base}/admin/quotas/${user.uid}`, users[1], 'GET', undefined, [403]);
  await call(`${base}/admin/quotas/${user.uid}`, users[1], 'PUT', { base_revision: 0, quota_bytes: 1 }, [403]);
  await call(`${base}/admin/quotas/${user.uid}`, admin, 'PUT', { base_revision: 0, quota_bytes: -1 }, [400]);
  const quotaWrites = await Promise.all(users.map(() => mx.call(`${base}/admin/quotas/${user.uid}`, admin.access_token, 'PUT', { base_revision: 0, quota_bytes: 2 * 1024 ** 3 }, [200, 409])));
  assert.equal(quotaWrites.filter((result) => result.status === 200).length, 1);
  assert.equal((await call(base,user)).quota_bytes, 2 * 1024 ** 3);
  assert.equal((await call(base,users[1])).quota_bytes, 10 * 1024 ** 3);
  assert.equal((await call(base,user)).max_file_size_bytes, null);
  const beforeDenied = mx.n1.requests.length;
  await call(`${base}/uploads`,users[1],'POST',{operation_uid:randomUUID(),file_name:'15GB-quota.mp4',mime_type:'video/mp4',size:15*1024**3,last_modified:1},[413]);
  assert.equal(mx.n1.requests.length,beforeDenied,'Quota rejection performs no N1 requests or file reads');
  pass('10 GB default, administrator-only per-user quotas, eight concurrent administrator writes have one winner');
  const bytes = Buffer.alloc(18 * 1024 * 1024); for (let index = 0; index < bytes.length; index++) bytes[index] = index % 251;
  const pending = await begin(user, 'Original.MP4', bytes); await part(user, pending.metadata.operation_uid, 0, bytes.subarray(0, pending.state.part_size));
  const storedRequests = mx.n1.requests.length;
  await part(user,pending.metadata.operation_uid,0,Buffer.alloc(pending.state.part_size,99),[409]);
  await part(user,pending.metadata.operation_uid,999,Buffer.from('invalid'),[400]);
  await call(`${base}/uploads`,user,'POST',{...pending.metadata,last_modified:12346},[409]);
  assert.equal(mx.n1.requests.length,storedRequests,'Conflicting parts and changed file identity fail before touching N1');
  const before = mx.n1.requests.filter((entry) => entry.method === 'PUT').length;
  await mx.restart();
  const resumed = await call(`${base}/uploads`, user, 'POST', pending.metadata); assert.deepEqual(resumed.uploaded_parts, [0]);
  const reselected = await call(`${base}/uploads`,user,'POST',{...pending.metadata,operation_uid:randomUUID()});
  assert.equal(reselected.operation_uid,pending.metadata.operation_uid); assert.deepEqual(reselected.uploaded_parts,[0]);
  assert.equal(resumed.part_size, 4 * 1024 * 1024);
  for (let index = 1; index < Math.ceil(bytes.length / resumed.part_size); index++) await part(user, pending.metadata.operation_uid, index, bytes.subarray(index * resumed.part_size, (index+1) * resumed.part_size));
  let item = await call(`${base}/uploads/${pending.metadata.operation_uid}/finish`, user, 'POST');
  assert.equal(mx.n1.requests.filter((entry) => entry.method === 'PUT').length - before, 4);
  const replay = await call(`${base}/uploads`, user, 'POST', pending.metadata); assert.equal(replay.completed, true); assert.equal(replay.item.uid, item.uid);
  assert.equal(query('SELECT COUNT(*) AS n FROM mx_drive_versions WHERE item_uid=?', item.uid)[0].n, 1);
  const key = query('SELECT object_key FROM mx_drive_versions WHERE item_uid=?', item.uid)[0].object_key;
  assert.equal(mx.n1.requests.some(entry=>entry.path.includes('/objects/stream/') || entry.path.includes('/objects/download')),false,'Finalization must never download the uploaded object');
  assert.ok(key.endsWith('.MP4')); assert.ok(!key.includes('Original')); assert.equal('object_key' in item, false);
  const downloaded = await mx.api.get(`${base}/items/${item.uid}/download`, { headers: headers(user) }); assert.equal(hash(await downloaded.body()), hash(bytes));
  pass('resumable multipart survives MX restart, skips stored parts, verifies bytes, preserves .MP4 and idempotent metadata');

  const largeBytes = Buffer.alloc(130 * 1024 * 1024, 93);
  const large = await upload(user, 'Large-over-100MB.bin', largeBytes);
  assert.equal(large.state.part_size, 4 * 1024 * 1024); assert.equal(large.item.size, largeBytes.length);
  const largeTicket = await call(`${base}/items/${large.item.uid}/ticket`,user,'POST',{});
  const tail = await mx.api.get(largeTicket.url,{headers:{Range:`bytes=${largeBytes.length-1024}-`}});
  assert.equal(tail.status(),206); assert.deepEqual(await tail.body(),largeBytes.subarray(-1024));
  const giantMetadata = {operation_uid:randomUUID(),file_name:'Over-one-GB.MP4',mime_type:'video/mp4',size:1024**3+1,last_modified:12345};
  const giant = await call(`${base}/uploads`,user,'POST',giantMetadata);
  assert.equal(giant.part_size,50*1024*1024); assert.equal(giant.workers,4);
  await part(user,giantMetadata.operation_uid,0,Buffer.alloc(50*1024*1024,34));
  await call(`${base}/uploads/${giantMetadata.operation_uid}`,user,'DELETE');
  assert.equal((await call(base,user)).items.some((entry)=>entry.name===giantMetadata.file_name),false);
  pass('130 MiB real upload succeeds; >1 GiB initialization negotiates console 50 MiB parts and stores a full 50 MiB part before cancellation');

  const quotaPending = await begin(users[7],'Quota-changed-mid-upload.txt',Buffer.from('quota check'));
  await part(users[7],quotaPending.metadata.operation_uid,0,Buffer.from('quota check'));
  await call(`${base}/admin/quotas/${users[7].uid}`,admin,'PUT',{base_revision:0,quota_bytes:0});
  await call(`${base}/uploads/${quotaPending.metadata.operation_uid}/finish`,users[7],'POST',undefined,[413]);
  await call(`${base}/uploads`,users[7],'POST',{...quotaPending.metadata,operation_uid:randomUUID()},[413]);
  assert.equal((await call(base,users[7])).total,0);
  const reset = await call(`${base}/admin/quotas/${users[7].uid}`,admin,'PUT',{base_revision:1,quota_bytes:null});
  assert.equal(reset.quota_bytes,10*1024**3); assert.equal(reset.assigned,false);
  assert.equal(query('SELECT COUNT(*) AS n FROM mx_drive_quota_activity WHERE user_uid=?',users[7].uid)[0].n,2);
  await mx.restart(); assert.equal((await call(base,user)).quota_bytes,2*1024**3);
  pass('quota reduction blocks pending commit and new uploads; reset restores default, audit persists and assignments survive restart');

  for (const other of [users[1], admin]) { await call(`${base}/items/${item.uid}`, other, 'GET', undefined, [404]); const response = await mx.api.get(`${base}/items/${item.uid}/download`, { headers: headers(other) }); assert.equal(response.status(), 404); }
  assert.equal((await call(base, users[1])).total, 0);
  await call(`${base}/items/${item.uid}/sharing`, user, 'POST', { user_uid: users[1].uid, role: 'viewer' });
  const ticket = await call(`${base}/items/${item.uid}/ticket`, users[1], 'POST', {});
  const range = await mx.api.get(ticket.url, { headers: { Range: 'bytes=1000-1999' } }); assert.equal(range.status(), 206); assert.deepEqual(await range.body(), bytes.subarray(1000, 2000));
  await call(`${base}/items/${item.uid}`, users[1], 'PATCH', { action: 'rename', base_revision: 1, name: 'Not allowed' }, [403]);
  await call(`${base}/items/${item.uid}/sharing`, user, 'POST', { user_uid: users[1].uid, role: null }); assert.equal((await mx.api.get(ticket.url)).status(), 404);
  pass('private default-deny includes administrators; viewer grant streams exact ranges and revoked tickets stop immediately');

  const noStorage = mx.n1.requests.length;
  item = await call(`${base}/items/${item.uid}`, user, 'PATCH', { action: 'rename', base_revision: 1, name: 'Display name.webm' });
  assert.equal(mx.n1.requests.length, noStorage); assert.equal(query('SELECT object_key FROM mx_drive_versions WHERE item_uid=?', item.uid)[0].object_key, key);
  const conflicts = await Promise.all(users.map(() => mx.call(`${base}/items/${item.uid}`, user.access_token, 'PATCH', { action: 'rename', base_revision: item.revision, name: `Concurrent ${randomUUID()}.mp4` }, [200, 409])));
  assert.equal(conflicts.filter((entry) => entry.status === 200).length, 1); item = await call(`${base}/items/${item.uid}`, user);
  pass('rename is MX-only and eight simultaneous metadata saves produce one CAS winner');

  const folder = await call(`${base}/folders`, user, 'POST', { operation_uid: randomUUID(), name: 'Public folder' });
  const secret = await call(`${base}/folders`, user, 'POST', { operation_uid: randomUUID(), name: 'Private sibling' });
  item = await call(`${base}/items/${item.uid}`, user, 'PATCH', { action: 'move', base_revision: item.revision, parent_uid: folder.uid });
  const link = await call(`${base}/items/${folder.uid}/links`, user, 'POST', { expires_at: Date.now() + 86400000 });
  const guest = await call(`${base}/guest/${link.token}`, null); assert.equal(guest.items[0].uid, item.uid); assert.equal(JSON.stringify(guest).includes(user.uid), false); assert.equal(JSON.stringify(guest).includes(key), false);
  await call(`${base}/guest/${link.token}?item_uid=${secret.uid}`, null, 'GET', undefined, [404]);
  const publicRange = await mx.api.get(`${base}/guest/${link.token}/media/${item.uid}?revision=${item.revision}`, { headers: { Range: 'bytes=3-19' } }); assert.equal(publicRange.status(), 206); assert.deepEqual(await publicRange.body(), bytes.subarray(3, 20));
  assert.equal(publicRange.headers()['cache-control'], 'private, no-store');
  assert.equal((await mx.api.get(`${base}/guest/${link.token}/media/${item.uid}?revision=9999`)).status(), 409);
  await call(`${base}/items/${folder.uid}/links/${link.uid}`, user, 'DELETE'); await call(`${base}/guest/${link.token}`, null, 'GET', undefined, [404]);
  pass('public guest links are scoped to the shared subtree, omit private identity/keys, stream ranges, and revoke immediately');

  const additions = await Promise.all(users.map((account, index) => upload(account, `parallel-${index}.txt`, Buffer.from(`owner ${index}`))));
  assert.equal(new Set(additions.map((value) => value.item.uid)).size, 8);
  for (const [index, value] of additions.entries()) { await call(`${base}/items/${value.item.uid}`, users[(index + 1) % 8], 'GET', undefined, [404]); }
  pass('eight concurrent users upload and finalize independently without ownership leaks');

  const beforeCopy=mx.n1.requests.length;
  const copyOperation={operation_uid:randomUUID(),action:'copy',parent_uid:null,items:[{uid:large.item.uid,base_revision:large.item.revision}]};
  const receipts=await Promise.all(Array.from({length:8},()=>call(`${base}/transfers`,user,'POST',copyOperation)));
  assert.equal(new Set(receipts.map(r=>r.items[0].uid)).size,1,'Eight replays create exactly one copy');
  assert.equal(receipts[0].items[0].size,large.item.size);assert.equal(mx.n1.requests.length,beforeCopy,'Large copy never reads or re-uploads N1 content');
  const copiedUid=receipts[0].items[0].uid;
  const copiedKey=query('SELECT object_key FROM mx_drive_versions WHERE item_uid=?',copiedUid)[0].object_key;
  assert.equal(copiedKey,query('SELECT object_key FROM mx_drive_versions WHERE item_uid=?',large.item.uid)[0].object_key);
  const concurrentCopies=await Promise.all(additions.map((value,index)=>call(`${base}/transfers`,users[index],'POST',{operation_uid:randomUUID(),action:'copy',parent_uid:null,items:[{uid:value.item.uid,base_revision:value.item.revision}]})));
  assert.equal(new Set(concurrentCopies.map(r=>r.items[0].uid)).size,8);
  for(const [index,receipt]of concurrentCopies.entries())await call(`${base}/items/${receipt.items[0].uid}`,users[(index+1)%8],'GET',undefined,[404]);
  await call(`${base}/items/${copiedUid}`,user,'PATCH',{action:'trash',base_revision:1});await call(`${base}/items/${copiedUid}`,user,'DELETE');
  const replayedCopy=await call(`${base}/transfers`,user,'POST',copyOperation);assert.equal(replayedCopy.items[0].uid,copiedUid);await call(`${base}/items/${copiedUid}`,user,'GET',undefined,[404]);
  const liveCopyRange=await mx.api.get(largeTicket.url,{headers:{Range:'bytes=0-31'}});assert.equal(liveCopyRange.status(),206);assert.deepEqual(await liveCopyRange.body(),largeBytes.subarray(0,32));
  pass('large-file copy is metadata-only; eight concurrent replays deduplicate, eight owners remain isolated, purge cannot resurrect copies or delete referenced bytes');
  const copyPreviewTicket=await call(`${base}/items/${concurrentCopies[2].items[0].uid}/ticket`,users[2],'POST',{});
  const copyPreview=await mx.api.get(copyPreviewTicket.url);assert.equal(copyPreview.status(),200);assert.equal(await copyPreview.text(),'owner 2');
  assert.equal(query('SELECT t.version_uid FROM mx_drive_tickets t WHERE t.item_uid=?',concurrentCopies[2].items[0].uid)[0].version_uid,query('SELECT current_version_uid FROM mx_drive_items WHERE uid=?',concurrentCopies[2].items[0].uid)[0].current_version_uid,'Copied preview pins its own version, not the source version');
  const moveTarget=await call(`${base}/folders`,user,'POST',{operation_uid:randomUUID(),name:'Concurrent move target'});
  const movingCopy=concurrentCopies[0].items[0];
  const moves=await Promise.all(Array.from({length:8},()=>mx.call(`${base}/transfers`,user.access_token,'POST',{operation_uid:randomUUID(),action:'move',parent_uid:moveTarget.uid,items:[{uid:movingCopy.uid,base_revision:movingCopy.revision}]},[200,409])));
  assert.equal(moves.filter(r=>r.status===200).length,1,'Concurrent moves have exactly one CAS winner');
  assert.equal((await call(`${base}/items/${movingCopy.uid}`,user)).parent_uid,moveTarget.uid);
  await call(`${base}/items/${additions[0].item.uid}`,user,'PATCH',{action:'trash',base_revision:1});await call(`${base}/items/${additions[0].item.uid}`,user,'DELETE');
  const retainedCopyTicket=await call(`${base}/items/${movingCopy.uid}/ticket`,user,'POST',{});
  assert.equal(await(await mx.api.get(retainedCopyTicket.url)).text(),'owner 0','Purging the source preserves the independent copy');
  pass('eight simultaneous moves have one winner; copied-file tickets pin the right version and deleting the source preserves its copy');

  // Real batch multipart request, including a duplicate filename that must fail only that entry.
  const operations = [randomUUID(), randomUUID(), randomUUID()];
  const boundary = `test-${randomUUID()}`; const parts = []; const metadata = operations.map((operation_uid) => ({ operation_uid }));
  parts.push(Buffer.from(`--${boundary}\r\nContent-Disposition: form-data; name="metadata"\r\n\r\n${JSON.stringify(metadata)}\r\n`));
  for (let index = 0; index < 3; index++) parts.push(Buffer.from(`--${boundary}\r\nContent-Disposition: form-data; name="file"; filename="${index === 2 ? 'small-0.txt' : `small-${index}.txt`}"\r\nContent-Type: text/plain\r\n\r\nsmall ${index}\r\n`));
  parts.push(Buffer.from(`--${boundary}--\r\n`)); const batchBody = Buffer.concat(parts);
  if (mx.n1.live) mx.n1.control.dropBatchAcknowledgements = 1;
  const batchCalls = mx.n1.requests.filter((entry) => entry.path === '/noa/v1/upload/batch').length;
  const response = await mx.api.post(`${base}/batch`, { headers: { ...headers(user), 'Content-Type': `multipart/form-data; boundary=${boundary}` }, data: batchBody }); assert.equal(response.status(), 200, await response.text());
  const batch = await response.json(); assert.equal(batch.results.filter((value) => value.status === 200).length, 2); assert.equal(batch.results.filter((value) => value.status === 409).length, 1);
  assert.ok(mx.n1.requests.some((entry) => entry.path === '/noa/v1/upload/batch'));
  const batchReplay = await mx.api.post(`${base}/batch`, { headers: { ...headers(user), 'Content-Type': `multipart/form-data; boundary=${boundary}` }, data: batchBody }); assert.equal(batchReplay.status(), 200); assert.equal((await batchReplay.json()).results.filter((value) => value.status === 200).length, 2);
  assert.equal(mx.n1.requests.filter((entry) => entry.path === '/noa/v1/upload/batch').length - batchCalls, 1);
  pass('N1 small-file batch commits independently, reconciles a lost acknowledgement, and does not republish on replay');

  const oldTicket = await call(`${base}/items/${item.uid}/ticket`, user, 'POST', {});
  const newBytes = Buffer.from('replacement camera bytes');
  const replacement = await upload(user, 'Camera.MOV', newBytes, { item_uid: item.uid, base_revision: item.revision, parent_uid: item.parent_uid });
  assert.equal(replacement.item.revision, item.revision + 1);
  const keys = query('SELECT object_key FROM mx_drive_versions WHERE item_uid=? ORDER BY created_at', item.uid).map((value) => value.object_key);
  assert.equal(keys.length, 2); assert.ok(keys[0].endsWith('.MP4')); assert.ok(keys[1].endsWith('.MOV')); assert.notEqual(keys[0], keys[1]);
  const pinned = await mx.api.get(oldTicket.url, { headers: { Range: 'bytes=3-19' } }); assert.equal(pinned.status(), 206); assert.deepEqual(await pinned.body(), bytes.subarray(3, 20));
  const latest = await mx.api.get(`${base}/items/${item.uid}/download`, { headers: headers(user) }); assert.deepEqual(await latest.body(), newBytes);
  const versions = await call(`${base}/items/${item.uid}/history`, user); assert.equal(versions.versions.length, 2);
  const expired = await call(`${base}/items/${item.uid}/links`, user, 'POST', { expires_at: Date.now() + 60_000 }); execute('UPDATE mx_drive_links SET expires_at=1 WHERE uid=?', expired.uid); await call(`${base}/guest/${expired.token}`, null, 'GET', undefined, [404]);
  item = replacement.item;
  pass('new versions preserve each original extension; existing preview tickets remain byte-stable and expired guest links are denied');
  const restoreInput={operation_uid:randomUUID(),base_revision:item.revision,version_uid:versions.versions[1].uid};const beforeRestore=mx.n1.requests.length;
  item=await call(`${base}/items/${item.uid}/restore-version`,user,'POST',restoreInput);assert.equal(mx.n1.requests.length,beforeRestore,'Version restore must not read or upload N1 bytes');
  assert.equal((await call(`${base}/items/${item.uid}/restore-version`,user,'POST',restoreInput)).revision,item.revision,'Restore response-loss replay does not add another version');
  assert.equal((await call(`${base}/items/${item.uid}/history`,user)).versions.length,3);
  const restoredTicket=await call(`${base}/items/${item.uid}/ticket`,user,'POST',{});
  assert.equal(query('SELECT version_uid FROM mx_drive_tickets WHERE item_uid=? ORDER BY expires_at DESC LIMIT 1',item.uid)[0].version_uid,query('SELECT current_version_uid FROM mx_drive_items WHERE uid=?',item.uid)[0].current_version_uid,'Restored preview pins the new version even when bytes are shared');
  const restoredRange=await mx.api.get(restoredTicket.url,{headers:{Range:'bytes=3-19'}});assert.equal(restoredRange.status(),206);assert.deepEqual(await restoredRange.body(),bytes.subarray(3,20));
  pass('version restoration is metadata-only, quota/revision guarded, deduplicated on replay, and pins the exact new version for media ranges');

  const doomed = await begin(user, 'Canceled.mp4', Buffer.from('cancel')); await call(`${base}/uploads/${doomed.metadata.operation_uid}`, user, 'DELETE');
  await call(`${base}/uploads/${doomed.metadata.operation_uid}/finish`, user, 'POST', undefined, [404]);
  pass('canceling a pending durable upload cannot create or resurrect a file');

  mx.n1.control.rejectDeletes = true;
  const storageBeforeTrash = mx.n1.requests.length;
  await call(`${base}/items/${folder.uid}`, user, 'PATCH', { action: 'trash', base_revision: 1 }); assert.equal(mx.n1.requests.length, storageBeforeTrash);
  const start = Date.now(); await call(`${base}/trash/empty`, user, 'POST'); assert.ok(Date.now() - start < 2000);
  await call(`${base}/items/${item.uid}`, user, 'GET', undefined, [404]);
  assert.ok(query('SELECT object_key FROM mx_drive_gc WHERE object_key=?', key).length);
  await mx.restart(); assert.ok(query('SELECT object_key FROM mx_drive_gc WHERE object_key=?', key).length);
  mx.n1.control.rejectDeletes = false; execute('UPDATE mx_drive_gc SET next_attempt_at=0');
  await until(() => query('SELECT object_key FROM mx_drive_gc WHERE object_key=?', key).length === 0, 'background soft deletion after N1 recovers');
  await call(`${base}/uploads`, user, 'POST', pending.metadata, [404, 410]);
  assert.equal(await(await mx.api.get(retainedCopyTicket.url)).text(),'owner 0','Reference-safe content remains after restart and background garbage collection');
  pass('trash is MX-only; permanent removal is fast, durable cleanup survives restart/outage, and replay cannot revive purged files');

  if (!process.env.MX_TEST_API_ONLY) {
    const module = await call('/mx/v1/admin/modules', admin, 'POST', { name: 'Drive integration', singular_name: 'Entry' }, [201]); const moduleUid = module.uid || module.module?.uid;
    const access = await call(`/mx/v1/admin/accounts/${users[2].uid}/modules`, admin);
    await call(`/mx/v1/admin/accounts/${users[2].uid}/modules`, admin, 'PUT', { revision: access.revision, grants: [{ module_uid: moduleUid, can_read: true, can_create: true, can_update: true, can_delete: false, can_configure: false, can_report: true, can_attachments: true }] });
    await call(`/mx/v1/admin/modules/${moduleUid}/fields`, admin, 'POST', { key: 'title', label: 'Title', field_type: 'text' }, [201]);
    await call(`/mx/v1/admin/modules/${moduleUid}/fields`, admin, 'POST', { key: 'files', label: 'Reference files', field_type: 'attachments', config: { multiple: true, storage_name: 'References' } }, [201]);
    const channel = await call('/mx/v1/collaboration/channels', users[2], 'POST', { name: 'Drive copies', member_uids: [users[3].uid] }, [201]);
    const environment = isolatedBrowserAudio();
    const engines = { chromium, firefox, webkit };
    for (const engine of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
      const executablePath = engine === 'chromium' ? '/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome' : engine === 'firefox' ? '/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox' : '/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
      const browser = await engines[engine].launch({ headless: true, executablePath, env: environment, ...(engine === 'chromium' ? { args: ['--no-sandbox', '--mute-audio'] } : {}) }); browsers.push(browser);
      const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 900 } }); const page = await context.newPage(); const errors = []; page.on('pageerror', (failure) => errors.push(failure.message));
      await page.addInitScript((tokens) => {
        window.mxAuthDiagnostics = [];
        const remove = Storage.prototype.removeItem;
        Storage.prototype.removeItem = function(key) {
          if (this === sessionStorage && /^mx_(access|refresh)_token$/.test(key)) window.mxAuthDiagnostics.push({ event: 'remove', key, stack: new Error().stack });
          return remove.call(this, key);
        };
        const clear = Storage.prototype.clear;
        Storage.prototype.clear = function() {
          if (this === sessionStorage) window.mxAuthDiagnostics.push({ event: 'clear', stack: new Error().stack });
          return clear.call(this);
        };
        sessionStorage.setItem('mx_access_token', tokens.access_token); sessionStorage.setItem('mx_refresh_token', tokens.refresh_token);
      }, users[2]);
      await page.goto(`https://127.0.0.1:${mx.port}/#drive`); await page.getByRole('heading', { name: 'MX Drive', exact: true }).waitFor();
      await page.getByRole('region',{name:'Drive storage',exact:true}).waitFor();
      await until(async()=>(await page.locator('.storage-summary').textContent()).includes('10.0 GB'),'Drive quota meter loaded');
      const authSnapshots=[];
      const snapshot=async(label)=>authSnapshots.push({label,...await page.evaluate(()=>({access:!!sessionStorage.getItem('mx_access_token'),refresh:!!sessionStorage.getItem('mx_refresh_token'),diagnostics:window.mxAuthDiagnostics,origin:location.origin,timeOrigin:performance.timeOrigin}))});
      await snapshot('ready');
      const panel = page.getByRole('region',{name:'Upload queue',exact:true});
      if(!process.env.MX_TEST_FILE_MANAGER_ONLY) {
      let hashWorker = false; page.on('worker',worker=> { if(worker.url().includes('fileHash.worker')) hashWorker=true; });
      const largePath = join(mx.directory,`UI-large-${engine}.bin`); const largeHandle = await open(largePath,'w'); await largeHandle.truncate(128*1024*1024); await largeHandle.close();
      await page.evaluate(()=>{Blob.prototype.arrayBuffer=function(){throw new Error('Any browser pre-scan forbidden by regression test');};});
      let partRequests=0; let acknowledged=0; let lostFinish=false;const partStatuses=[];const refreshStatuses=[];const authRequests=[];
      page.on('request',request=>{if(request.method()==='PUT' && /\/drive\/uploads\/[^/]+\/parts\//.test(request.url())) { partRequests++; authRequests.push({method:request.method(),path:new URL(request.url()).pathname,authorization:!!request.headers().authorization,matchesInitial:request.headers().authorization===`Bearer ${users[2].access_token}`}); }});
      page.on('response',response=>{if(response.request().method()==='PUT' && /\/drive\/uploads\/[^/]+\/parts\//.test(response.url())){partStatuses.push(response.status());if(response.status()===200)acknowledged++;}if(response.url().includes('/auth/refresh'))refreshStatuses.push(response.status());});
      await page.route('**/drive/uploads/*/parts/*',async route=>{await new Promise(resolve=>setTimeout(resolve,500)); await route.continue().catch(()=>{});});
      await page.route('**/drive/uploads/*/finish',async route=>{if(!lostFinish){lostFinish=true; await route.fetch(); await route.abort();}else await route.continue();});
      const selectedAt=Date.now();
      await page.locator('.drive input[type=file]:not([webkitdirectory])').first().setInputFiles(largePath);
      await until(()=>partRequests>0,'Parts begin without a file scan',15000);
      await snapshot('first-part');
      assert.equal(await page.evaluate(()=>{const event=new Event('beforeunload',{cancelable:true}); window.dispatchEvent(event); return event.defaultPrevented;}),true,'Closing/reloading warns while uploads are active');
      await snapshot('after-warning');
      assert.ok(Date.now()-selectedAt<15000,'Upload starts promptly without scanning 128 MiB');
      try { await until(async()=>acknowledged>0 && await page.locator('.telemetry').count()>0 && /[1-9]/.test(await page.locator('.telemetry').innerText()),'In-flight speed and ETA visible'); }
      catch(failure) { await snapshot('failure');await page.screenshot({path:join(mx.directory,`${engine}-telemetry-failure.png`)});await writeFile(join(mx.directory,`${engine}-telemetry-failure.json`),JSON.stringify({partRequests,acknowledged,partStatuses,refreshStatuses,authRequests,authSnapshots,authDiagnostics:await page.evaluate(()=>window.mxAuthDiagnostics),session:await page.evaluate(()=>{const token=sessionStorage.getItem('mx_access_token');if(!token)return {present:false};try{const payload=JSON.parse(atob(token.split('.')[1]));return{present:true,token_kind:payload.token_kind,exp:payload.exp,now:Math.floor(Date.now()/1000)};}catch{return{present:true,malformed:true};}}),transfers:await page.locator('.transfer').allTextContents(),errors},null,2));throw failure; }
      const box = await panel.boundingBox(); assert.ok(box.x+box.width>=1250 && box.y+box.height>=870,'Upload panel is fixed at lower right');
      await page.screenshot({path:join(mx.directory,`${engine}-upload-progress.png`)});
      assert.equal(await panel.locator('.progress-ring').count(),1,'Upload uses a circular icon progress ring');assert.equal(await panel.locator('progress').count(),0,'No linear upload bar remains');
      await page.getByRole('button',{name:`Pause UI-large-${engine}.bin`,exact:true}).click();
      await until(async()=>await page.locator('.transfer[data-status=paused]').count()===1,'Pause settles cleanly');
      const operation = query("SELECT operation_uid FROM mx_drive_uploads WHERE actor_uid=? AND name=? ORDER BY attempted_at DESC",users[2].uid,`UI-large-${engine}.bin`)[0].operation_uid;
      const paused = await call(`${base}/uploads/${operation}`,users[2]); assert.ok(paused.uploaded_parts.length>0,'Pause retains acknowledged parts');
      await page.evaluate(uid=>location.hash=`module/${uid}`,moduleUid);
      await until(async()=>await page.locator('.drive').count()===0,'Drive page unmounted on module switch');
      assert.equal(await panel.count(),1,'Queue remains outside Drive');
      await page.getByRole('button',{name:`Resume UI-large-${engine}.bin`,exact:true}).click();
      await page.evaluate(uid=>location.hash=`collaboration?channel=${uid}`,channel.uid);
      await until(async()=>await page.locator('.transfer[data-status=done]').count()===1,'Upload completes while in collaboration',120000);
      assert.equal(await page.locator('.drive').count(),0,'Completion did not require remounting Drive');
      assert.equal(lostFinish,true,'Lost browser finalization response reconciled');
      assert.equal(await page.evaluate(()=>{const event=new Event('beforeunload',{cancelable:true}); window.dispatchEvent(event); return event.defaultPrevented;}),false,'Completed uploads do not block page exit');
      await page.evaluate(()=>location.hash='drive');
      await page.getByRole('button',{name:`Download UI-large-${engine}.bin`,exact:true}).waitFor({timeout:120000});
      assert.equal(hashWorker,false,'No whole-file hashing worker exists');
      assert.equal((await call(base,users[2])).items.find(entry=>entry.name===`UI-large-${engine}.bin`).size,128*1024*1024);
      }
      await page.getByRole('button', { name: 'New folder', exact: true }).click(); await page.getByRole('dialog', { name: 'New folder', exact: true }).getByLabel('Name', { exact: true }).fill(`UI ${engine}`); await page.getByRole('button', { name: 'Save', exact: true }).click(); await page.getByRole('button', { name: `More actions for UI ${engine}`, exact: true }).waitFor();
      const image = Buffer.from(await page.evaluate(() => { const canvas = document.createElement('canvas'); canvas.width = 64; canvas.height = 48; const context = canvas.getContext('2d'); context.fillStyle = '#3768a6'; context.fillRect(0, 0, 64, 48); context.fillStyle = '#ffffff'; context.fillRect(12, 12, 40, 24); return canvas.toDataURL('image/png').split(',')[1]; }), 'base64');
      await page.locator('.drive input[type=file]:not([webkitdirectory])').first().setInputFiles([{ name: `preview-${engine}.png`, mimeType: 'image/png', buffer: image }, { name: `notes-${engine}.txt`, mimeType: 'text/plain', buffer: Buffer.from('MX Drive preview text') }]);
      await page.getByRole('button', { name: `Download preview-${engine}.png`, exact: true }).waitFor(); await until(async () => (await page.locator('.transfer').allTextContents()).every((text) => text.includes('Uploaded')), 'UI batch uploads');
      const folderName = `References-${engine}`; const directory = join(mx.directory, folderName); await mkdir(join(directory, 'nested'), { recursive: true }); await writeFile(join(directory, 'root.txt'), 'root'); await writeFile(join(directory, 'nested', 'one.txt'), 'one'); await writeFile(join(directory, 'nested', 'two.txt'), 'two');
      await page.locator('.drive input[webkitdirectory]').setInputFiles(directory);
      await until(async () => (await page.locator('.transfer').allTextContents()).length >= 5 && (await page.locator('.transfer').allTextContents()).every((text) => text.includes('Uploaded')), 'folder upload preserves paths');
      const uploadedFolder = (await call(base, users[2])).items.find((item) => item.name === folderName); assert.ok(uploadedFolder);
      const folderItems = (await call(`${base}?parent_uid=${uploadedFolder.uid}`, users[2])).items; assert.ok(folderItems.some((item) => item.name === 'root.txt'));
      const nested = folderItems.find((item) => item.name === 'nested'); assert.ok(nested); assert.deepEqual((await call(`${base}?parent_uid=${nested.uid}`, users[2])).items.map((item) => item.name).sort(), ['one.txt', 'two.txt']);
      await verifyFileManager({page,engine,call,base,user:users[2],directory:mx.directory});
      await page.locator('.file-name').filter({ hasText: `preview-${engine}.png` }).dblclick(); await page.locator('.image-preview img').waitFor(); await page.waitForFunction(() => [...document.querySelectorAll('.image-preview img')].some((image) => image.naturalWidth > 0)); await page.getByRole('button', { name: 'Close preview', exact: true }).click();
      await page.getByRole('button', { name: `More actions for preview-${engine}.png`, exact: true }).click();await page.getByRole('menuitem',{name:'Versions and activity',exact:true}).click();
      await page.getByRole('button', { name: 'New version', exact: true }).click();
      await page.locator('.drive input[type=file]:not([webkitdirectory])').last().setInputFiles({ name: 'Replacement.PNG', mimeType: 'image/png', buffer: image });
      await until(async () => await page.locator('.drive-modal .version').count() === 2, 'version dialog updates after upload');
      await page.locator('.drive-modal .version').last().getByTitle('Preview version', { exact: true }).click();
      await page.waitForFunction(() => [...document.querySelectorAll('.image-preview img')].some((image) => image.naturalWidth > 0));
      await page.getByRole('button', { name: 'Close preview', exact: true }).click();
      await page.locator('.drive-modal .version').last().getByTitle('Restore as a new version', { exact: true }).click();
      await page.getByRole('alertdialog').getByRole('button', { name: 'Restore version', exact: true }).click();
      await page.getByRole('dialog', { name: 'Versions and activity', exact: true }).waitFor({ state: 'hidden' });
      await until(async()=>{const versioned=(await call(base,users[2])).items.find(item=>item.name===`preview-${engine}.png`); return (await call(`${base}/items/${versioned.uid}/history`,users[2])).versions.length===3;},'Version restore finishes through workspace queue');
      await page.getByRole('button', { name: `More actions for preview-${engine}.png`, exact: true }).click();await page.getByRole('menuitem',{name:'Share',exact:true}).click(); await page.getByRole('button', { name: 'Create link', exact: true }).click(); const guestLink = await page.getByLabel('New guest link', { exact: true }).inputValue(); await page.getByRole('button', { name: 'Close dialog', exact: true }).click();
      for (const [theme, width] of [['light', 1280], ['dark', 390]]) {
        await page.setViewportSize({ width, height: 850 }); await page.evaluate((theme) => { document.documentElement.dataset.theme = theme; document.documentElement.style.setProperty('--primary', '#ff00ff'); }, theme);
        await page.waitForTimeout(300);
        const palette=await page.evaluate(()=>{
          const panel=document.querySelector('.upload-panel'); const before=getComputedStyle(panel).backgroundColor;
          const root=document.documentElement; const names=['--surface','--surface-2','--text','--muted','--line']; const previous=names.map(name=>root.style.getPropertyValue(name));
          names.forEach(name=>root.style.setProperty(name,'#ff00ff')); const after=getComputedStyle(panel).backgroundColor;
          names.forEach((name,index)=>previous[index]?root.style.setProperty(name,previous[index]):root.style.removeProperty(name)); return {before,after};
        });
        assert.equal(palette.before,palette.after,'Upload chrome ignores custom surface/text colors');
        const bounds=await panel.boundingBox(); assert.ok(bounds.x>=0 && bounds.y>=0 && bounds.x+bounds.width<=width+1,'Upload panel remains within mobile/desktop viewport');
        assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 2), `${engine} ${theme} horizontal overflow`);
        assert.ok(await page.locator('.drive-heading').evaluate((element) => element.getBoundingClientRect().left >= -1), `${engine} ${theme} Drive heading clipped`);
        assert.ok(await page.locator('.sidebar').evaluate((element) => innerWidth > 820 || element.getBoundingClientRect().right <= 1), `${engine} mobile navigation should be off canvas`);
        await page.screenshot({ path: join(mx.directory, `${engine}-${theme}-drive.png`), fullPage: true });
      }
      const guestContext = await browser.newContext({ ignoreHTTPSErrors: true }); const guestPage = await guestContext.newPage(); await guestPage.goto(guestLink); await guestPage.getByRole('heading', { name: `preview-${engine}.png`, exact: true }).waitFor(); assert.equal(await guestPage.locator('input[type=password]').count(), 0);
      await guestPage.locator('.name').click(); await guestPage.waitForFunction(() => [...document.querySelectorAll('.image-preview img')].some((image) => image.naturalWidth > 0));
      await page.setViewportSize({ width: 1280, height: 900 });
      await page.getByRole('button',{name:'Switch to grid',exact:true}).click();
      assert.ok(await page.locator('.drive-file-icon.large').count()>0,'Grid view has preview cards, not small list icons');
      await page.screenshot({path:join(mx.directory,`${engine}-drive-grid.png`),fullPage:true});
      await page.getByRole('button',{name:'Switch to list',exact:true}).click();
      await page.evaluate((uid) => location.hash = `collaboration?channel=${uid}`, channel.uid);
      await page.locator('.composer-action').filter({ hasText: 'MX Drive' }).click();
      await page.getByRole('dialog', { name: 'Choose from MX Drive', exact: true }).getByRole('button', { name: new RegExp(`notes-${engine}\\.txt`) }).click();
      await page.getByRole('button', { name: `Remove notes-${engine}.txt`, exact: true }).waitFor();
      const uploadBox=await panel.boundingBox(); const composerBox=await page.locator('.message-composer').boundingBox();
      assert.ok(uploadBox.y+uploadBox.height<composerBox.y,'Upload panel does not cover the collaboration composer');
      await page.getByRole('button', { name: 'Send', exact: true }).click();
      let sharedFile;
      await until(async () => { const result = await call(`/mx/v1/collaboration/channels/${channel.uid}/messages`, users[2]); sharedFile = result.messages.flatMap((message) => message.files).find((file) => file.file_name === `notes-${engine}.txt`); return sharedFile; }, 'Drive copy attached to collaboration');
      const record = await call(`/mx/v1/modules/${moduleUid}/records`, users[2], 'POST', { operation_uid: randomUUID(), values: { title: `Reference ${engine}` } }, [201]);
      await page.evaluate(({ moduleUid, recordUid }) => location.hash = `module/${moduleUid}?record=${recordUid}`, { moduleUid, recordUid: record.uid });
      await page.getByRole('button', { name: 'Edit entry', exact: true }).click();
      await page.getByRole('button', { name: 'Choose from MX Drive', exact: true }).click();
      await page.route('**/mx/v1/drive/items/*/download', async (route) => { await new Promise((resolve) => setTimeout(resolve, 700)); await route.continue(); });
      await page.getByRole('dialog', { name: 'Choose from MX Drive', exact: true }).getByRole('button', { name: new RegExp(`notes-${engine}\\.txt`) }).click();
      assert.equal(await page.locator('.record-form button[type=submit]').isDisabled(), true, 'Cannot save before the Drive copy arrives');
      await page.locator('.record-form').evaluate((form) => form.requestSubmit());
      await page.getByRole('button', { name: 'Update Entry', exact: true }).click();
      let attached;
      await until(async () => { const result = await call(`/mx/v1/modules/${moduleUid}/records/${record.uid}`, users[2]); attached = result.attached_files.find((file) => file.file_name === `notes-${engine}.txt`); return attached; }, 'Drive copy attached to record');
      const source = (await call(base, users[2])).items.find((item) => item.name === `notes-${engine}.txt`);
      const driveKey = query('SELECT object_key FROM mx_drive_versions WHERE item_uid=?', source.uid)[0].object_key;
      const chatKey = query('SELECT object_key FROM mx_message_files WHERE uid=?', sharedFile.uid)[0].object_key;
      assert.equal(new Set([driveKey, chatKey, attached.object_key]).size, 3);
      await call(`${base}/items/${source.uid}`, users[2], 'PATCH', { action: 'trash', base_revision: source.revision });
      const recordCopy = await mx.api.get(`/mx/v1/records/${record.uid}/attachments/${attached.uid}/download`, { headers: headers(users[2]) });
      const chatCopy = await mx.api.get(`/mx/v1/collaboration/files/${sharedFile.uid}/download`, { headers: headers(users[3]) });
      assert.equal(recordCopy.status(), 200); assert.equal(chatCopy.status(), 200); assert.equal(await recordCopy.text(), 'MX Drive preview text'); assert.equal(await chatCopy.text(), 'MX Drive preview text');
      const adminContext = await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}}); const adminPage=await adminContext.newPage();
      await adminPage.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},admin);
      await adminPage.goto(`https://127.0.0.1:${mx.port}/#admin`);
      await adminPage.getByRole('button',{name:'Storage for Drive 2',exact:true}).click();
      const quotaDialog=adminPage.getByRole('dialog',{name:'Drive storage allowance',exact:true}); await quotaDialog.getByLabel(/Use default allowance/).uncheck();
      await adminPage.setViewportSize({width:390,height:850});
      assert.ok(await adminPage.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+2),'Mobile account storage layout must not overflow');
      await adminPage.screenshot({path:join(mx.directory,`${engine}-admin-quota-mobile.png`),fullPage:true});
      await quotaDialog.getByLabel('Storage allowance (GB)',{exact:true}).fill('12'); await quotaDialog.getByRole('button',{name:'Save storage allowance',exact:true}).click();
      await quotaDialog.getByRole('status').waitFor(); assert.equal((await call(base,users[2])).quota_bytes,12*1024**3);
      await quotaDialog.getByLabel(/Use default allowance/).check(); await quotaDialog.getByRole('button',{name:'Save storage allowance',exact:true}).click();
      await until(async()=>!(await call(`${base}/admin/quotas/${users[2].uid}`,admin)).assigned,'UI default quota reset');
      await quotaDialog.getByRole('button',{name:'Close storage settings',exact:true}).click(); await adminContext.close();
      assert.deepEqual(errors, []); pass(`${engine}: no-prescan 128 MiB upload, live MB/s and ETA, lower-right queue, pause/resume across unmounted Drive/record/collaboration views, lost finalize acknowledgement, quotas/preview/guest/light-dark/mobile and independent attachment copies`);
      await guestContext.close(); await context.close(); await browser.close();
    }
  }
  assert.deepEqual(mx.n1.failures, []);
} catch (failure) { failures.push(failure.stack); process.exitCode = 1; for(const browser of browsers)for(const context of browser.contexts())for(const page of context.pages()){await page.screenshot({path:join(mx.directory,`failure-${browsers.indexOf(browser)}-${context.pages().indexOf(page)}.png`),fullPage:true}).catch(()=>{});} }
finally { await writeFile(join(mx.directory, 'drive-results.json'), JSON.stringify({ passed: !failures.length, storage: mx.n1.live ? 'live N1' : 'contract fixture', results, failures }, null, 2)); console.log(JSON.stringify({ passed: !failures.length, cases: results.length, artifacts: mx.directory, failures })); for (const browser of browsers) await browser.close().catch(() => {}); await mx.close(); }
