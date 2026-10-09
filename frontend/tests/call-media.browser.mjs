import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { createServer } from 'vite';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isolatedBrowserAudio } from './silent-audio.mjs';

const projectRoot = fileURLToPath(new URL('../', import.meta.url));
const browserAudio = isolatedBrowserAudio();
const artifacts = await mkdtemp(join(tmpdir(), 'mx-call-media-'));
const server = await createServer({ root: projectRoot, logLevel: 'error', server: { host: '127.0.0.1', port: 5187, strictPort: true, hmr: false } });
let browser;
try {
  await server.listen();
  browser = await chromium.launch({ executablePath: process.env.MX_BROWSER_EXECUTABLE || undefined, headless: true, env: browserAudio,
    args: ['--no-sandbox', '--mute-audio', '--autoplay-policy=no-user-gesture-required', '--disable-background-timer-throttling', '--disable-renderer-backgrounding',
      '--auto-select-tab-capture-source-by-title=MX Self Share Test', '--auto-accept-this-tab-capture', '--allow-http-screen-capture'] });
  const pages = new Map();
  const errors = [];
  const sharer = 'a';
  const realCapture = process.env.MX_TEST_CAPTURE_SOURCE !== 'synthetic';
  const sourceParams = realCapture ? '&real=1' : '';
  for (const uid of ['a', 'm', 'z']) {
    const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
    pages.set(uid, page);
    page.on('pageerror', (error) => { errors.push(`${uid}: ${error.message}`); console.error(error.stack); });
    await page.exposeBinding('relay', (_source, recipient, signal) => {
      setTimeout(() => pages.get(recipient).evaluate((value) => window.testCall.signal(value), signal).catch((error) => errors.push(error.message)), recipient === 'z' ? 200 : 0);
    });
    await page.route('**/mx/**', (route) => route.fulfill({ status: 404 }));
    await page.route('**/mx-call-test?*', (route) => route.fulfill({ contentType: 'text/html', body: `<html><head><title>${uid === sharer ? 'MX Self Share Test' : 'MX Viewer Test'}</title></head><body><div id="app"></div><script type="module">import { mount } from "/node_modules/.vite/deps/svelte.js"; import Harness from "/src/lib/call/fixtures/CallGroupHarness.svelte"; mount(Harness,{target:document.getElementById("app")});</script></body></html>` }));
    await page.goto(`http://127.0.0.1:5187/mx-call-test?user=${uid}${uid === sharer ? sourceParams : ''}`);
    await page.waitForFunction(() => !!window.testCall);
    await page.evaluate(() => window.testCall.start());
  }
  await Promise.all([...pages.values()].map((page) => page.evaluate(() => window.testCall.connect())));
  await Promise.all([...pages.values()].map((page) => page.waitForFunction(() => window.testCall.state().peers.length === 2 && window.testCall.state().peers.every((peer) => peer.state === 'connected'))));
  const source = pages.get(sharer);
  const viewers = [...pages.entries()].filter(([uid]) => uid !== sharer);
  await Promise.all([...pages.values()].map((page) => page.evaluate((uid) => window.testCall.source(uid, true), sharer)));
  await source.locator('#share').click();
  await source.waitForFunction(() => window.testCall.state().sharing, { timeout: 10000 });
  const sourceState = await source.evaluate(() => window.testCall.state());
  if (realCapture) assert.equal(sourceState.screenSettings.displaySurface, 'browser');
  console.log(JSON.stringify({ source: sourceState }));
  await source.waitForFunction(() => !document.querySelector('[data-peer="a"] video').srcObject && document.querySelector('[data-peer="a"] .call-self-share-status'));
  for (const [uid, page] of viewers) {
    await page.waitForFunction(() => document.querySelector('[data-peer="a"] video.active')?.readyState >= 2);
    assert.equal(await page.locator('[data-peer="a"] video').count(), 1);
    const stats = await page.evaluate(() => {
      const tile = document.querySelector('[data-peer="a"]');
      window.originalMicStream = tile.querySelector('audio').srcObject;
      return { frames: tile.querySelector('video').getVideoPlaybackQuality().totalVideoFrames, clock: document.querySelector('#clock').textContent };
    });
    await page.waitForFunction((first) => document.querySelector('[data-peer="a"] video').getVideoPlaybackQuality().totalVideoFrames > first + 4, stats.frames);
    console.log(JSON.stringify({ viewer: uid, videoPlaying: true, oneVideo: true }));
  }
  // Enable a local preview, then enter native fullscreen: it must stop local
  // rendering while capture and remote playback stay alive.
  await source.getByRole('button', { name: 'Show my preview' }).click();
  await source.waitForFunction(() => document.querySelector('[data-peer="a"] video.active')?.readyState >= 2);
  await source.locator('[data-peer="a"] button[title="View full screen"]').click();
  await source.waitForFunction(() => !!document.fullscreenElement && !document.querySelector('[data-peer="a"] video').srcObject);
  assert.equal(await source.locator('[data-peer="a"] video').count(), 1);
  await source.screenshot({ path: join(artifacts, `fullscreen-${realCapture ? 'real' : 'synthetic'}.png`) });
  await source.locator('[data-peer="a"] button[title="Exit full screen"]').click();
  await source.waitForFunction(() => !document.fullscreenElement && document.querySelector('[data-peer="a"] video.active')?.readyState >= 2);
  await source.getByRole('button', { name: 'Hide my preview' }).click();
  await source.waitForFunction(() => !document.querySelector('[data-peer="a"] video').srcObject);
  for (let round = 0; round < 4; round++) {
    await source.locator('#stop').click();
    await source.waitForFunction(() => !window.testCall.state().sharing);
    await Promise.all([...pages.values()].map((page) => page.evaluate((uid) => window.testCall.source(uid, false), sharer)));
    await Promise.all(viewers.map(([, page]) => page.waitForFunction(() => !document.querySelector('[data-peer="a"] video').srcObject)));
    await Promise.all([...pages.values()].map((page) => page.evaluate((uid) => window.testCall.source(uid, true), sharer)));
    await source.locator('#share').click();
    await source.waitForFunction(() => window.testCall.state().sharing);
    await Promise.all(viewers.map(([, page]) => page.waitForFunction(() => document.querySelector('[data-peer="a"] video.active')?.readyState >= 2)));
    for (const [, page] of viewers) assert.ok(await page.evaluate(() => document.querySelector('[data-peer="a"] audio').srcObject === window.originalMicStream));
    await source.evaluate(() => { window.testCall.microphone(false); window.testCall.microphone(true); });
    console.log(JSON.stringify({ round: round + 1, restarted: true, audioBindingRetained: true }));
  }
  await source.getByRole('button', { name: 'Show my preview' }).click();
  for (let index = 0; index < 8; index++) {
    await source.locator('[data-peer="a"] button[title="View full screen"]').click();
    await source.waitForFunction(() => !!document.fullscreenElement && !document.querySelector('[data-peer="a"] video').srcObject);
    await source.locator('[data-peer="a"] button[title="Exit full screen"]').click();
    await source.waitForFunction(() => !document.fullscreenElement);
  }
  for (const [uid, page] of viewers) {
    await page.locator('[data-peer="a"] button[title="View full screen"]').click();
    await page.waitForFunction(() => !!document.fullscreenElement && document.querySelector('[data-peer="a"] video.active')?.readyState >= 2);
    await page.locator('[data-peer="a"] button[title="Exit full screen"]').click();
    console.log(JSON.stringify({ viewer: uid, remoteFullscreenWorking: true }));
  }
  assert.deepEqual(errors, []);
  assert.deepEqual(await source.evaluate(() => window.testCall.state().errors), []);
  console.log(JSON.stringify({ passed: true, actualTabCapture: realCapture, errors, artifacts }));
} finally { await browser?.close(); await server.close(); }
