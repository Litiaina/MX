// Test-only N1 v4 HTTP contract fixture, not a replacement for live N1 tests.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { randomUUID } from 'node:crypto';
import { consolePartSize } from './n1-part-policy.mjs';

export async function startN1Fixture() {
  const sessions = new Map(); const objects = new Map(); const failures = []; const requests = [];
  const control = { rejectDeletes: false };
  const server = createServer(async (request, response) => {
    try {
      const url = new URL(request.url, 'http://localhost'); const path = decodeURIComponent(url.pathname);
      const chunks = []; for await (const chunk of request) chunks.push(chunk); const bytes = Buffer.concat(chunks);
      requests.push({ method: request.method, path, bytes: bytes.length, range: request.headers.range });
      const json = (value, status = 200) => { response.writeHead(status, { 'Content-Type': 'application/json' }); response.end(JSON.stringify(value)); };
      if (path === '/noa/v1/auth/fragment') return json({ access_token: 'isolated-n1-token', expires_in: 3600 });
      assert.equal(request.headers.authorization, 'Bearer isolated-n1-token');
      if (path === '/noa/v1/upload/batch') {
        const boundary = request.headers['content-type'].match(/boundary=([^;]+)/)[1];
        const pieces = bytes.toString('latin1').split(`--${boundary}`).slice(1, -1); const results = [];
        for (const piece of pieces) {
          const split = piece.indexOf('\r\n\r\n'); const headers = piece.slice(0, split); const file = Buffer.from(piece.slice(split + 4, -2), 'latin1');
          const key = decodeURIComponent(headers.match(/filename="([^"]+)"/)[1]); const contentType = headers.match(/Content-Type: ([^\r]+)/i)[1];
          assert.ok(file.length <= 1024 * 1024); const version_id = randomUUID(); objects.set(key, { bytes: file, version_id, contentType });
          results.push({ object_key: key, version_id, status: 201, size_bytes: file.length });
        }
        return json({ results, requested: results.length, succeeded: results.length, failed: 0 });
      }
      const durable = path.includes('/multipart/durable/');
      const init = durable ? '/noa/v1/upload/multipart/durable/init/' : '/noa/v1/upload/multipart/init/';
      if (path.startsWith(init)) {
        const parameters = JSON.parse(bytes); const version_id = randomUUID(); const session_id = randomUUID();
        assert.equal(parameters.part_size_bytes, durable ? consolePartSize(parameters.total_size_bytes) : 16 * 1024 * 1024);
        assert.equal(parameters.expected_parts, Math.max(1, Math.ceil(parameters.total_size_bytes / parameters.part_size_bytes)));
        if (durable) { const existing = [...sessions].find(([_id, session]) => session.key === path.slice(init.length)); if (existing) { assert.deepEqual(existing[1].parameters, parameters); return json({ version_id: existing[0], session_id: existing[1].session_id }); } }
        sessions.set(version_id, { key: path.slice(init.length), session_id, parameters, parts: new Map(), failedOnce: false });
        return json({ version_id, session_id });
      }
      const status = path.match(/^\/noa\/v1\/upload\/multipart\/(?:durable\/)?([^/]+)$/);
      if (status) { const session = sessions.get(status[1]); if (request.method === 'DELETE') { sessions.delete(status[1]); return json({ ok: true }); } if (!session) return json({}, 404); assert.equal(url.searchParams.get('object_key'), session.key); return json({ uploaded_parts: [...session.parts.keys()] }); }
      const part = path.match(/^\/noa\/v1\/upload\/multipart\/(?:durable\/)?([^/]+)\/(\d+)$/);
      if (part && request.method === 'PUT') {
        const session = sessions.get(part[1]); assert.ok(session);
        assert.ok(bytes.length <= session.parameters.part_size_bytes);
        if (!session.failedOnce) { session.failedOnce = true; return json({ response: 'Injected transient storage failure' }, 503); }
        const index = Number(part[2]);
        if (session.parts.has(index)) assert.deepEqual(session.parts.get(index), bytes);
        session.parts.set(index, bytes); response.writeHead(200, { 'Content-Type': 'text/plain' }); response.end('Part uploaded'); return;
      }
      const finalize = path.match(/^\/noa\/v1\/upload\/multipart\/(?:durable\/)?([^/]+)\/([^/]+)\/finalize$/);
      if (finalize) {
        const session = sessions.get(finalize[1]); assert.ok(session); assert.equal(finalize[2], session.session_id); assert.equal(url.searchParams.get('object_key'), session.key);
        assert.equal(session.parts.size, session.parameters.expected_parts);
        const body = Buffer.concat(Array.from({ length: session.parts.size }, (_value, index) => session.parts.get(index)));
        assert.equal(body.length, session.parameters.total_size_bytes);
        objects.set(session.key, { bytes: body, version_id: finalize[1], contentType: session.parameters.content_type });
        session.parts.clear(); return json({ version_id: finalize[1] });
      }
      const oneShot = '/noa/v1/upload/one-shot/';
      if (path.startsWith(oneShot)) { objects.set(path.slice(oneShot.length), { bytes, version_id: randomUUID(), contentType: request.headers['content-type'] }); return json({}, 201); }
      const meta = '/noa/v1/objects/meta/';
      if (path.startsWith(meta)) { const object = objects.get(path.slice(meta.length)); return json(object ? { version_id: object.version_id, total_size_bytes: object.bytes.length } : {}, object ? 200 : 404); }
      const stream = '/noa/v1/objects/stream/';
      if (path.startsWith(stream) || path === '/noa/v1/objects/download') {
        const object = objects.get(path.startsWith(stream) ? path.slice(stream.length) : url.searchParams.get('object_key'));
        if (!object) return json({}, 404);
        const range = request.headers.range?.match(/^bytes=(\d+)-(\d*)$/);
        let start = range ? Number(range[1]) : 0; const end = range && range[2] ? Math.min(Number(range[2]), object.bytes.length - 1) : object.bytes.length - 1;
        if (start >= object.bytes.length) { response.writeHead(416, { 'Content-Range': `bytes */${object.bytes.length}` }); return response.end(); }
        response.writeHead(range ? 206 : 200, { 'Content-Type': object.contentType || 'application/octet-stream', 'Accept-Ranges': 'bytes', 'Content-Length': end - start + 1, ...(range ? { 'Content-Range': `bytes ${start}-${end}/${object.bytes.length}` } : {}) });
        return response.end(object.bytes.subarray(start, end + 1));
      }
      if (path.startsWith('/noa/v1/objects/') && request.method === 'DELETE') { if (control.rejectDeletes) return json({},503); objects.delete(path.slice('/noa/v1/objects/'.length)); return json({ affected_paths: 1, conflicted_paths: [], failed_paths: [] }); }
      throw new Error(`Unexpected N1 contract request: ${request.method} ${path}`);
    } catch (error) { failures.push(error.stack); response.writeHead(500); response.end('Test fixture contract failed'); }
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return { url: `http://127.0.0.1:${server.address().port}`, objects, sessions, failures, requests, control, close: () => new Promise((resolve) => server.close(resolve)) };
}
