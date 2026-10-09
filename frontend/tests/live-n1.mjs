// Opt-in integration adapter: real N1 storage behind a fault-injecting local
// proxy. Credentials never appear in command lines, logs, or result artifacts.
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import https from 'node:https';
import { request as httpRequest } from 'node:http';
import { request } from 'playwright';
import { join } from 'node:path';
import { consolePartSize } from './n1-part-policy.mjs';

export async function startLiveN1(directory) {
  const upstream = new URL(process.env.MX_TEST_N1_URL);
  assert.ok(['https:', 'http:'].includes(upstream.protocol));
  assert.ok(process.env.MX_TEST_N1_CREDENTIALS_FILE, 'Live tests require a private credentials file');
  const credentials = Object.fromEntries((await readFile(process.env.MX_TEST_N1_CREDENTIALS_FILE, 'utf8')).trim().split(/\r?\n/).filter((line) => line && !line.startsWith('#')).map((line) => {
    const split = line.indexOf('='); return [line.slice(0, split), line.slice(split + 1)];
  }));
  const fragment = credentials.N1_FRAGMENT_HASH || createHash('sha256').update(credentials.N1_FRAGMENT).digest('hex');
  const secret = credentials.N1_MX_SECRET;
  assert.ok(secret, 'Missing test fragment secret');
  const api = await request.newContext({ baseURL: upstream.origin, ignoreHTTPSErrors: true, timeout: 60000 });
  const auth = await api.post('/noa/v1/auth/fragment', { data: { fragment, secret, read: true, write: true, modify: true, delete: true, expires_in: 3600 } });
  assert.equal(auth.status(), 200, `Live N1 authentication failed (HTTP ${auth.status()})`);
  const token = (await auth.json()).access_token;
  assert.ok(token);
  const sessions = new Map(); const objects = new Map(); const requests = []; const failures = [];
  const control = { rejectDeletes: false, dropBatchAcknowledgements: 0 };
  // Independent from the public observation maps, which runners may clear.
  const cleanupKeys = new Set();
  const server = createServer(async (incoming, outgoing) => {
    try {
      const url = new URL(incoming.url, 'http://localhost'); const path = decodeURIComponent(url.pathname);
      const chunks = []; for await (const chunk of incoming) chunks.push(chunk);
      const bytes = Buffer.concat(chunks);
      requests.push({ method: incoming.method, path, bytes: bytes.length, range: incoming.headers.range });
      if (control.rejectDeletes && incoming.method === 'DELETE' && path.startsWith('/noa/v1/objects/')) { outgoing.writeHead(503, { 'Content-Type': 'application/json' }); outgoing.end('{}'); return; }
      const init = path.includes('/multipart/durable/') ? '/noa/v1/upload/multipart/durable/init/' : '/noa/v1/upload/multipart/init/'; const oneShot = '/noa/v1/upload/one-shot/';
      let parameters; let key;
      if (path.startsWith(init)) {
        key = path.slice(init.length); parameters = JSON.parse(bytes);
        assert.equal(parameters.part_size_bytes, path.includes('/multipart/durable/') ? consolePartSize(parameters.total_size_bytes) : 16 * 1024 * 1024);
        assert.equal(parameters.expected_parts, Math.max(1, Math.ceil(parameters.total_size_bytes / parameters.part_size_bytes)));
        cleanupKeys.add(key);
      }
      if (path.startsWith(oneShot)) { key = path.slice(oneShot.length); cleanupKeys.add(key); }
      if (path === '/noa/v1/upload/batch') {
        for (const match of bytes.toString('latin1').matchAll(/Content-Disposition: form-data; name="object"; filename="([^"]+)"/g)) cleanupKeys.add(decodeURIComponent(match[1]));
      }
      const part = path.match(/^\/noa\/v1\/upload\/multipart\/(?:durable\/)?([^/]+)\/(\d+)$/);
      if (part && incoming.method === 'PUT') {
        const session = sessions.get(part[1]); assert.ok(session, 'Unknown test multipart session');
        assert.ok(bytes.length <= session.parameters.part_size_bytes);
        // Reject before forwarding: MX must retry the exact part against N1.
        if (!session.failedOnce) { session.failedOnce = true; outgoing.writeHead(503, { 'Content-Type': 'application/json' }); outgoing.end('{"error":"Injected transient test failure"}'); return; }
      }
      const target = new URL(incoming.url, upstream);
      const send = target.protocol === 'https:' ? https.request : httpRequest;
      const proxy = send(target, { method: incoming.method, rejectUnauthorized: false, headers: { ...incoming.headers, host: upstream.host } }, (response) => {
        const lostFinalize = Boolean(process.env.MX_TEST_N1_DROP_FINALIZE && path.endsWith('/finalize') && response.statusCode >= 200 && response.statusCode < 300);
        const lostBatch = Boolean(control.dropBatchAcknowledgements > 0 && path === '/noa/v1/upload/batch' && response.statusCode >= 200 && response.statusCode < 300);
        if (lostBatch) control.dropBatchAcknowledgements--;
        const lostResponse = lostFinalize || lostBatch;
        const observe = Boolean(parameters || path.startsWith(oneShot) || path.endsWith('/finalize'));
        const responseChunks = [];
        if (observe) response.on('data', (chunk) => responseChunks.push(chunk));
        if (!lostResponse) { outgoing.writeHead(response.statusCode, response.headers); response.pipe(outgoing); }
        else response.resume();
        response.on('end', () => {
          try {
            if (response.statusCode < 200 || response.statusCode >= 300) return;
            if (parameters) {
              const data = JSON.parse(Buffer.concat(responseChunks));
              sessions.set(data.version_id, { key, session_id: data.session_id, parameters, failedOnce: sessions.get(data.version_id)?.failedOnce || false });
            }
            if (path.startsWith(oneShot)) objects.set(key, { bytes: bytes.length });
            if (path.endsWith('/finalize')) objects.set(url.searchParams.get('object_key'), { finalizeAcknowledgementDropped: lostFinalize });
          } catch (error) { failures.push(error.message); }
          finally { if (lostResponse) outgoing.destroy(); }
        });
        response.on('error', () => outgoing.destroy());
      });
      proxy.setTimeout(120000, () => proxy.destroy(new Error('Live N1 transport timeout')));
      proxy.on('error', (error) => { failures.push(error.message); if (!outgoing.headersSent) outgoing.writeHead(502); outgoing.end(); });
      outgoing.on('close', () => { if (!outgoing.writableFinished) proxy.destroy(); });
      proxy.end(bytes);
    } catch (error) { failures.push(error.message); outgoing.writeHead(500); outgoing.end('Live integration adapter failed'); }
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return { live: true, fragment, secret, url: `http://127.0.0.1:${server.address().port}`, objects, sessions, requests, failures, control, async close() {
    await new Promise((resolve) => server.close(resolve));
    const cleanup = [];
    try {
      for (const key of cleanupKeys) {
        const encoded = key.split('/').map(encodeURIComponent).join('/');
        const response = await api.delete(`/noa/v1/objects/${encoded}`, { headers: { Authorization: `Bearer ${token}` } });
        const body = response.status() === 200 ? await response.json() : null;
        const complete = !body?.failed_paths?.length && !body?.conflicted_paths?.length;
        const absent = await api.get(`/noa/v1/objects/stream/${encoded}`, { headers: { Authorization: `Bearer ${token}` } });
        cleanup.push({ key, status: response.status(), complete, streamStatus: absent.status() });
      }
      await writeFile(join(directory, 'live-n1-cleanup.json'), JSON.stringify({ storage: 'live N1', objects: cleanup }, null, 2));
      assert.ok(cleanup.every((item) => [200, 204, 404].includes(item.status) && item.complete && item.streamStatus === 404), 'Live N1 test-object cleanup failed; see live-n1-cleanup.json');
    } finally { await api.dispose(); }
  } };
}
