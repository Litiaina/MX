// A disposable, real MX server. Never reads the deployment's mx.env/mx.config.
import { mkdtemp, writeFile, open } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:net';
import { request } from 'playwright';
import { startN1Fixture } from './n1-contract-fixture.mjs';
import { startLiveN1 } from './live-n1.mjs';

async function freePort() {
  const server = createServer();
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  await new Promise((resolve) => server.close(resolve));
  return port;
}

export async function startIsolatedMx(root) {
  const directory = await mkdtemp(join(process.env.MX_TEST_ARTIFACTS || tmpdir(), 'mx-eight-user-'));
  const port = await freePort(); const redirectPort = await freePort();
  const signupKey = randomUUID();
  const n1 = process.env.MX_TEST_N1_URL ? await startLiveN1(directory) : await startN1Fixture();
  const n1Secret = n1.live ? n1.secret : 'unused-isolated-test';
  await writeFile(join(directory, 'mx.env'), `JWT_SECRET=${randomUUID()}${randomUUID()}\nAUTH_KEYS=${signupKey}\nN1_MX_SECRET=${n1Secret}\n`, { mode: 0o600 });
  await writeFile(join(directory, 'mx.config'), `[server]
host=127.0.0.1
http_port=${redirectPort}
https_port=${port}
cert_path=${join(directory, 'cert.pem')}
key_path=${join(directory, 'key.pem')}
[database]
db_path=${join(directory, 'test.db')}
pool_max_size=16
pool_min_idle=4
pool_connection_timeout_seconds=10
busy_timeout_milliseconds=10000
[jwt_token_config]
login_token_expiration=900
refresh_token_expiration=2592000
[static_site_hosting]
static_site_path=${resolve(root, 'dist')}
[n1]
base_url=${n1.url}
fragment=${n1.live ? (process.env.MX_TEST_N1_RAW_FRAGMENT ? n1Secret : n1.fragment) : 'isolated-no-external-storage'}
insecure_tls=true
attachment_max_size_mb=100
collaboration_file_max_size_mb=100
multipart_part_size_mb=16
[webrtc]
max_participants=12
`);
  execFileSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', join(directory, 'key.pem'), '-out', join(directory, 'cert.pem'), '-days', '1', '-subj', '/CN=localhost'], { stdio: 'ignore' });
  const log = await open(join(directory, 'server.log'), 'w');
  // Explicitly replace secrets inherited from a developer's shell.
  const childEnvironment = { ...process.env, JWT_SECRET: randomUUID() + randomUUID(), AUTH_KEYS: signupKey, N1_MX_SECRET: n1Secret, RUST_LOG: 'warn' };
  const launch = () => spawn(process.env.MX_TEST_BINARY || resolve(root, '../target/debug/litiaina-mx'), [], { cwd: directory, env: childEnvironment, stdio: ['ignore', log.fd, log.fd] });
  let child = launch();
  const api = await request.newContext({ baseURL: `https://127.0.0.1:${port}`, ignoreHTTPSErrors: true, timeout: 30000 });
  const deadline = Date.now() + 30000;
  while (true) {
    try { if ((await api.get('/ping', { timeout: 1000 })).ok()) break; } catch { /* startup */ }
    if (child.exitCode !== null || Date.now() > deadline) { child.kill('SIGTERM'); await api.dispose(); await log.close(); await n1.close(); throw new Error(`Isolated MX failed to start; see ${directory}/server.log`); }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  async function call(path, token, method = 'GET', data, expected = [200]) {
    const start = performance.now();
    const response = await api.fetch(path, { method, headers: token ? { Authorization: `Bearer ${token}` } : {}, ...(data === undefined ? {} : { data }) });
    const text = await response.text();
    let body; try { body = text ? JSON.parse(text) : null; } catch { throw new Error(`${method} ${path}: ${response.status()} returned non-JSON: ${text.slice(0, 200)}`); }
    if (!expected.includes(response.status())) throw new Error(`${method} ${path}: ${response.status()} ${JSON.stringify(body)}`);
    return { body, status: response.status(), milliseconds: Math.round(performance.now() - start) };
  }
  return { directory, port, api, call, signupKey, n1, async restart() {
    if (child.exitCode === null) { const exit = new Promise((resolve) => child.once('exit', resolve)); child.kill('SIGTERM'); await exit; }
    child = launch(); const deadline = Date.now() + 30000;
    while (true) { try { if ((await api.get('/ping', { timeout: 1000 })).ok()) break; } catch {} if (child.exitCode !== null || Date.now() > deadline) throw new Error('Isolated MX restart failed'); await new Promise((resolve) => setTimeout(resolve, 100)); }
  }, async close() {
    await api.dispose();
    if (child.exitCode === null) {
      const exit = new Promise((resolve) => child.once('exit', resolve));
      child.kill('SIGTERM');
      let timer; await Promise.race([exit, new Promise((resolve) => { timer = setTimeout(() => { child.kill('SIGKILL'); resolve(); }, 5000); })]); clearTimeout(timer);
    }
    await log.close();
    await n1.close();
  } };
}
