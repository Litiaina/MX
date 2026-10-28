// Real document/engine timings, not a delayed or mocked rendering benchmark.
// The supplied file is read only; all server copies, profiles and evidence live on HDD.
import assert from 'node:assert/strict';
import { randomUUID, createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:http';
import { request } from 'node:https';
import { chromium, firefox } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';
import { onTestDisk } from './concurrency-fixture.mjs';
import { cpus, totalmem, release } from 'node:os';

assert.ok(onTestDisk(process.env.MX_TEST_ARTIFACTS));
assert.ok(onTestDisk(process.env.TMPDIR));
assert.ok(process.env.MX_OFFICE_BENCHMARK_FILE, 'Explicit workbook required');
assert.ok(!process.env.MX_TEST_N1_URL, 'Use disposable storage only');
const bytes = await readFile(process.env.MX_OFFICE_BENCHMARK_FILE);
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
// Disable the fixture's deliberate first-part failure for timing-only setup.
// Fault/retry behavior is covered separately, not hidden inside this measurement.
mx.n1.control.beforePart = session => { session.failedOnce = true; };
const proxy = createServer((incoming, outgoing) => {
  const upstream = request({ hostname:'127.0.0.1', port:mx.port, path:incoming.url, method:incoming.method, headers:incoming.headers, rejectUnauthorized:false }, response => { outgoing.writeHead(response.statusCode, response.headers); response.pipe(outgoing); });
  upstream.on('error', () => { outgoing.writeHead(502); outgoing.end(); }); incoming.pipe(upstream);
});
await new Promise(resolve => proxy.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${proxy.address().port}`;
const report = { passed:false, scope:'Real graphical Office, isolated storage fixture; not a server-capacity qualification', host:{cpu:cpus()[0]?.model,logicalCpus:cpus().length,ramBytes:totalmem(),kernel:release()}, inputBytes:bytes.length, inputSha256:createHash('sha256').update(bytes).digest('hex'), budgetMs:Number(process.env.MX_OFFICE_OPEN_BUDGET_MS || 60000), results:[] };
const call = async (path, token, method='GET', data, expected=[200]) => (await mx.call(path, token, method, data, expected)).body;
let browser;
console.log(`Office performance evidence: ${mx.directory}`);
try {
  const email='performance@test.invalid', password=`Temporary!${randomUUID()}`;
  await call('/mx/v1/auth/create', mx.signupKey, 'POST', { email, password, name:'Performance test' }, [201]);
  const auth=await call('/mx/v1/auth/authenticate', null, 'POST', { email, password });
  const operation=randomUUID(), base='/mx/v1/drive';
  const state=await call(base+'/uploads', auth.access_token, 'POST', { operation_uid:operation, file_name:'Workbook performance.xlsx', mime_type:'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet', size:bytes.length, last_modified:1 });
  for(let part=0;part<Math.ceil(bytes.length/state.part_size);part++) {
    const response=await mx.api.put(`${base}/uploads/${operation}/parts/${part}`, { headers:{Authorization:`Bearer ${auth.access_token}`, 'Content-Type':'application/octet-stream'}, data:bytes.subarray(part*state.part_size,(part+1)*state.part_size) });
    assert.equal(response.status(),200,await response.text());
  }
  const item=await call(`${base}/uploads/${operation}/finish`,auth.access_token,'POST');
  for(const name of (process.env.MX_TEST_BROWSERS || 'chromium,firefox').split(',')) {
    const engine=name==='firefox'?firefox:chromium;
    const executablePath=name==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':name==='brave'?'/usr/bin/brave-browser':'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome';
    browser=await engine.launch({ executablePath, headless:true, env:disconnectedBrowserAudio(mx.directory), ...(engine===chromium?{args:['--mute-audio']}:{}) });
    const context=await browser.newContext({ ignoreHTTPSErrors:true, viewport:{width:1440,height:950} });
    await context.addInitScript(auth=>{
      sessionStorage.setItem('mx_access_token',auth.access_token);sessionStorage.setItem('mx_refresh_token',auth.refresh_token);
      window.officeMeasurements={start:performance.now(),stages:[],longTasks:[]};
      new MutationObserver(()=>{
        const phase=document.querySelector('.office-loading [aria-current="step"]')?.textContent;
        const list=window.officeMeasurements.stages;
        if(phase && phase!==list.at(-1)?.phase)list.push({phase,at:performance.now()});
        if(document.querySelector('#qtcanvas')?.getAttribute('data-native-focused')==='true' && !window.officeMeasurements.ready)window.officeMeasurements.ready=performance.now();
      }).observe(document,{childList:true,subtree:true,attributes:true,characterData:true});
      try {new PerformanceObserver(list=>window.officeMeasurements.longTasks.push(...list.getEntries().map(e=>({at:e.startTime,ms:e.duration})))).observe({type:'longtask',buffered:true});}catch{}
    },auth);
    const page=await context.newPage();page.on('dialog',dialog=>dialog.dismiss());
    const errors=[],messages=[];page.on('pageerror',e=>errors.push(e.message));page.on('console',m=>{if(m.text().startsWith('MX Office:'))messages.push({message:m.text(),at:Date.now()});});
    for(const cache of (process.env.MX_OFFICE_BENCHMARK_PASSES || 'cold,warm').split(',')) {
      console.log(`${name} ${cache}: opening actual ${bytes.length} byte workbook`);
      const start=Date.now();
      if(cache==='cold')await page.goto(`${origin}/office/index.html#file=${item.uid}`);else await page.reload();
      try { await page.locator('#qtcanvas[data-native-focused="true"]').waitFor({timeout:Math.max(report.budgetMs+30000,180000)}); }
      catch(reason) { const body=await page.locator('body').innerText();throw new Error(`${name} ${cache}: ${reason.message}; UI: ${body.slice(0,2000)}`); }
      const measured=await page.evaluate(()=>({...window.officeMeasurements,resources:performance.getEntriesByType('resource').filter(e=>/soffice|download\?/.test(e.name)).map(e=>({path:new URL(e.name).pathname,duration:e.duration,encodedBytes:e.encodedBodySize,decodedBytes:e.decodedBodySize,transferBytes:e.transferSize}))}));
      const elapsed=Date.now()-start;
      const result={browser:name,cache,elapsedMs:elapsed,measurements:measured,messages:[...messages],pageErrors:[...errors]};report.results.push(result);
      await writeFile(join(mx.directory,'office-performance-results.json'),JSON.stringify(report,null,2));
      console.log(JSON.stringify({browser:name,cache,elapsedMs:elapsed,stages:measured.stages}));
      assert.deepEqual(errors,[]);
      result.withinBudget=elapsed<=report.budgetMs;
    }
    assert.equal((await call(`${base}/items/${item.uid}`,auth.access_token)).revision,1,'Opening must never modify the original');
    await context.close();await browser.close();browser=undefined;
  }
  assert.ok(report.results.every(r=>r.withinBudget),'At least one real opening exceeds the configured responsiveness budget; see measured results');
  report.passed=true;
} catch(reason) { report.failure=reason.stack;process.exitCode=1;console.error(reason); }
finally {await browser?.close();await new Promise(resolve=>proxy.close(resolve));await mx.close();await writeFile(join(mx.directory,'office-performance-results.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,artifacts:mx.directory}));}
