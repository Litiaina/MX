// Browser-engine control for loss investigations: deliberately does NOT use
// MX's CallController. This distinguishes native failures from MX regressions.
import assert from 'node:assert/strict';
import { chromium, firefox, webkit } from 'playwright';
import { createServer } from 'node:http';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { isolatedBrowserAudio } from './silent-audio.mjs';

const env = isolatedBrowserAudio();
const directory = await mkdtemp(join(process.env.MX_TEST_ARTIFACTS || tmpdir(), 'mx-native-control-'));
const html = `<!doctype html><button id="start">Start</button><video autoplay muted></video><script>
const pc=new RTCPeerConnection({iceServers:[]});const candidates=[];const pending=[];const context=new AudioContext();
const destination=context.createMediaStreamDestination();const oscillator=context.createOscillator();oscillator.connect(destination);oscillator.start();
const canvas=document.createElement('canvas');canvas.width=640;canvas.height=360;let frame=0;
setInterval(()=>{const p=canvas.getContext('2d');p.fillStyle='#441144';p.fillRect(0,0,640,360);p.fillStyle='white';p.fillText(String(frame++),50,50);},80);
pc.onicecandidate=e=>{if(e.candidate){const c=e.candidate.toJSON();candidates.push(c);window.relay({kind:'ice',data:c});}};
pc.ontrack=e=>{if(e.track.kind==='video')document.querySelector('video').srcObject=new MediaStream([e.track]);};
window.testNative={
offer:async()=>{await pc.setLocalDescription();await window.relay({kind:'sdp',data:pc.localDescription.toJSON()});},
restart:async()=>{pc.restartIce();await window.testNative.offer();},
signal:async s=>{if(s.kind==='ice'){if(!pc.remoteDescription)pending.push(s.data);else await pc.addIceCandidate(s.data);}else{await pc.setRemoteDescription(s.data);for(const c of pending.splice(0))await pc.addIceCandidate(c);if(s.data.type==='offer'){await pc.setLocalDescription();await window.relay({kind:'sdp',data:pc.localDescription.toJSON()});}}},
replay:async()=>{for(const c of candidates)await window.relay({kind:'ice',data:c});},
state:async()=>({state:pc.connectionState,ice:pc.iceConnectionState,signaling:pc.signalingState,context:context.state,tracks:pc.getReceivers().map(r=>({kind:r.track.kind,muted:r.track.muted})),stats:[...(await pc.getStats()).values()].filter(r=>['inbound-rtp','outbound-rtp','transport','candidate-pair'].includes(r.type))}),
close:()=>{pc.close();context.close();}
};
document.querySelector('button').onclick=async()=>{await context.resume();pc.addTrack(destination.stream.getAudioTracks()[0],destination.stream);const stream=canvas.captureStream(12);pc.addTrack(stream.getVideoTracks()[0],stream);window.started=true;};
</script>`;
const server = createServer((_req, response) => { response.setHeader('Content-Type', 'text/html'); response.end(html); });
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const results = [];
try {
  for (const engine of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
    const browser = await ({ chromium, firefox, webkit })[engine].launch({ headless: true, env, executablePath: process.env[`MX_${engine.toUpperCase()}_EXECUTABLE`] || undefined,
      ...(engine === 'chromium' ? { args: ['--no-sandbox', '--mute-audio', '--autoplay-policy=no-user-gesture-required'] } : {}),
      ...(engine === 'firefox' ? { firefoxUserPrefs: { 'media.autoplay.default': 0 } } : {}) });
    const pages = []; const errors = []; let beginning = 0; let dropped = 0;
    try {
      for (let i = 0; i < 2; i++) {
        const page = await browser.newPage(); pages.push(page);
        page.on('pageerror', (error) => errors.push(error.message));
        await page.exposeBinding('relay', (_source, signal) => {
          if (signal.kind === 'ice' && Date.now() - beginning < 5000) { dropped++; return; }
          void pages[1 - i]?.evaluate((signal) => window.testNative.signal(signal), signal).catch((error) => errors.push(error.message));
        });
        await page.goto(`http://127.0.0.1:${server.address().port}`); await page.locator('#start').click(); await page.waitForFunction(() => window.started);
      }
      beginning = Date.now(); await pages[0].evaluate(() => window.testNative.offer());
      await new Promise((resolve) => setTimeout(resolve, 6000));
      await Promise.all(pages.map((page) => page.evaluate(() => window.testNative.replay())));
      if (process.env.MX_TEST_NATIVE_RESTART) await pages[0].evaluate(() => window.testNative.restart());
      let states; const deadline = Date.now() + 30000; let passed = false;
      do {
        states = await Promise.all(pages.map((page) => page.evaluate(() => window.testNative.state())));
        passed = states.every((s) => s.state === 'connected' && s.stats.some((r) => r.type === 'inbound-rtp' && (r.kind || r.mediaType) === 'audio' && r.bytesReceived > 0) && s.stats.some((r) => r.type === 'inbound-rtp' && (r.kind || r.mediaType) === 'video' && r.framesDecoded > 0));
        if (passed) break;
        await new Promise((resolve) => setTimeout(resolve, 200));
      } while (Date.now() < deadline);
      assert.ok(dropped > 0);
      results.push({ engine, passed, dropped, states, errors });
      console.log(JSON.stringify({ engine, passed, dropped, errors }));
    } finally { await browser.close(); }
  }
} finally { await new Promise((resolve) => server.close(resolve)); await writeFile(join(directory, 'results.json'), JSON.stringify(results, null, 2)); console.log(`Native control artifacts: ${directory}`); }
assert.ok(results.every((result) => result.passed && !result.errors.length), 'Native WebRTC control failed; examine engine diagnostics');
