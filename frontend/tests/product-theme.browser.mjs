// Actual built application, isolated records/storage, and real browser engines.
// Never contacts the populated preview, production MX/N1, or the release updater.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {chromium, firefox, webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
import {onTestDisk} from './concurrency-fixture.mjs';

assert.ok(onTestDisk(process.env.MX_TEST_ARTIFACTS));
assert.ok(onTestDisk(process.env.TMPDIR));
assert.ok(!process.env.MX_TEST_N1_URL, 'Only isolated fixture storage is permitted');
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
const result = {passed: false, browsers: {}, boundary: 'Local visual/functional qualification, not production deployment or a capacity test.'};
const call = async (path, user, method = 'GET', data, expected = [200]) => (await mx.call(path, user?.access_token, method, data, expected)).body;
const until = async (fn, label) => {const deadline = Date.now() + 20000; while (Date.now() < deadline) {if (await fn()) return; await new Promise(r => setTimeout(r, 50));} throw Error(label);};
let browser, page;
try {
  console.log(`Product-theme evidence: ${mx.directory}`);
  const password = `Disposable!${randomUUID()}`;
  await call('/mx/v1/auth/create', {access_token: mx.signupKey}, 'POST', {email: 'design@example.test', name: 'Design reviewer', password}, [201]);
  const owner = await call('/mx/v1/auth/authenticate', null, 'POST', {email: 'design@example.test', password});
  const initial = await call('/mx/v1/deployment/config', null);
  assert.equal(initial.config.appearance.preset, 'sage');
  assert.equal(initial.config.appearance.primary_color, '#c94128');
  const module = (await call('/mx/v1/admin/modules', owner, 'POST', {name: 'Customer relationships', singular_name: 'Opportunity', icon: 'users', description: 'Full instructions remain available on demand. Configure your customer relationships without losing room for the actual work.'}, [201])).module;
  const base = `/mx/v1/modules/${module.uid}`;
  for (const [key, label, field_type] of [['name', 'Opportunity', 'text'], ['notes', 'Notes', 'long_text']]) await call(`/mx/v1/admin/modules/${module.uid}/fields`, owner, 'POST', {key, label, field_type}, [201]);
  await call(`/mx/v1/admin/modules/${module.uid}/fields`,owner,'POST',{key:'priority',label:'Priority',field_type:'select',config:{options:['Routine','Important','Urgent']}},[201]);
  await call(`/mx/v1/admin/modules/${module.uid}/fields`,owner,'POST',{key:'amount',label:'Estimated value',field_type:'decimal'},[201]);
  const samples=[
    ['Community services','Ready for the handover meeting.\nDelivery scope confirmed.'],
    ['School administration','Workspace approved for onboarding.\nTraining plan is ready.'],
    ['Inventory workspace','Proposal is with the review team.\nConfirm the stock approval process.'],
    ['Facilities requests','Mapped the request and review stages.\nAwaiting feedback from operations.'],
    ['Records workspace','Reviewed document access requirements.\nPrepare a sample filing structure.'],
    ['Clinic onboarding','Discovery meeting scheduled.\nConfirm who owns the next step.'],
    ['Citizen support desk','Collecting the team’s requirements.\nArrange an introductory walkthrough.'],
    ['Permit information system','Initial enquiry received.\nReview the existing intake form.']
  ];
  for (const [i,[name,notes]] of samples.entries()) await call(`${base}/records`, owner, 'POST', {operation_uid: randomUUID(), values: {name,notes,priority:['Routine','Important','Urgent'][i%3],amount:12500+i*2750}}, [201]);
  await call(`${base}/workflow`, owner, 'PUT', {revision: 0, confirm_existing_records: true, definition: {enabled: true, name: 'Sales pipeline', kind: 'crm', initial_state: 'new', title_field: 'name', restart_on_edit: false, states: [{key: 'new', name: 'New leads'}, {key: 'progress', name: 'In progress'}, {key: 'won', name: 'Won'}], actions: [{key: 'qualify', name: 'Qualify', from: 'new', to: 'progress', access_levels: [0], creator_policy: 'any', required_fields: [], note_required: false}, {key: 'win', name: 'Mark won', from: 'progress', to: 'won', access_levels: [0], creator_policy: 'any', required_fields: [], note_required: false}]}});
  const demoRows=(await call(`${base}/records?limit=8`,owner)).data;
  for(let i=3;i<demoRows.length;i++)for(const action of i>=6?['qualify','win']:['qualify']){
    const path=`${base}/records/${demoRows[i].uid}/workflow`,state=await call(path,owner);
    await call(path+'/actions',owner,'POST',{action,configuration_revision:state.configuration_revision,record_revision:state.record_revision,workflow_revision:state.workflow_revision,note:'Synthetic demonstration only',operation_uid:randomUUID()});
  }
  const view = (id, kind, name) => ({id, kind, name, title_field: kind === 'board' ? '' : 'name', fields: kind === 'board' ? [] : ['notes'], summary_fields: [], description: ''});
  await call(`${base}/workspace`, owner, 'PUT', {revision: 0, layout: {default_view: 'pipeline', views: [view('pipeline', 'board', 'Pipeline'), view('directory', 'cards', 'Directory'), view('operations', 'overview', 'Operations'), view('records', 'records', 'All records')], sections: []}});
  await call('/mx/v1/drive/folders', owner, 'POST', {operation_uid: randomUUID(), name: 'Team documents'}, [200, 201]);
  const origin = `https://127.0.0.1:${mx.port}`;
  for (const engine of (process.env.MX_TEST_BROWSERS || 'chromium,brave,firefox,webkit').split(',')) {
    const executablePath = engine === 'brave' ? '/usr/bin/brave-browser' : engine === 'chromium' ? '/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome' : engine === 'firefox' ? '/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox' : '/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    browser = await (engine === 'brave' ? chromium : {chromium, firefox, webkit}[engine]).launch({headless: true, executablePath, env: disconnectedBrowserAudio(mx.directory), ...(['chromium', 'brave'].includes(engine) ? {args: ['--no-sandbox', '--mute-audio']} : {})});
    const context = await browser.newContext({ignoreHTTPSErrors: true, viewport: {width: 1440, height: 950}});
    await context.addInitScript(tokens => {sessionStorage.setItem('mx_access_token', tokens.access_token); sessionStorage.setItem('mx_refresh_token', tokens.refresh_token);}, owner);
    page = await context.newPage(); const errors = []; page.on('pageerror', e => errors.push(e.message));
    await page.goto(`${origin}/#module/${module.uid}`);
    await until(async () => await page.locator('.pipeline-card').count() === 8, 'Real populated pipeline');
    await until(() => page.getByRole('button', {name: 'Refresh pipeline', exact: true}).isEnabled(), 'Every bounded pipeline stage finishes loading');
    const geometry = await page.evaluate(() => {
      const box = selector => {const r = document.querySelector(selector).getBoundingClientRect(); return {top: r.top, height: r.height, bottom: r.bottom};};
      return {header: box('.module-workspace-heading'), tabs: box('.module-view-bar'), firstCard: box('.pipeline-card'), topbar: box('.workspace-topbar'), accent: getComputedStyle(document.documentElement).getPropertyValue('--primary').trim()};
    });
    assert.equal(geometry.accent, '#c94128');
    assert.ok(geometry.header.height <= 48, `Compact module heading: ${JSON.stringify(geometry)}`);
    assert.ok(geometry.tabs.height <= 46, 'Slim view tabs');
    assert.ok(geometry.firstCard.top <= 240, `Actual work is visible near the top: ${JSON.stringify(geometry)}`);
    assert.ok(await page.locator('.module-description').isHidden(), 'Full description does not occupy the default header');
    await page.getByRole('button', {name: 'About Customer relationships', exact: true}).click();
    assert.ok(await page.locator('.module-description').isVisible());
    assert.ok((await page.locator('.module-description').innerText()).includes('Full instructions remain available'));
    await page.getByRole('button', {name: 'About Customer relationships', exact: true}).click();
    // An asynchronously loaded process must not move Close under the pointer.
    let releaseStatus;
    const statusGate = new Promise(resolve => {releaseStatus = resolve;});
    const pattern = new RegExp(`/mx/v1/modules/${module.uid}/records/[^/]+/workflow$`);
    await page.route(pattern, async route => {const response = await route.fetch(); await statusGate; await route.fulfill({response});});
    await page.locator('.pipeline-card').first().getByRole('button', {name: 'Open & review', exact: true}).click();
    const record = page.locator('.record-dialog'); await record.waitFor();
    const closeBefore = await record.getByRole('button', {name: 'Close', exact: true}).boundingBox();
    releaseStatus();
    await record.getByRole('button', {name: 'Qualify', exact: true}).waitFor();
    const closeAfter = await record.getByRole('button', {name: 'Close', exact: true}).boundingBox();
    assert.ok(Math.abs(closeAfter.y - closeBefore.y) <= 1, `Close remains anchored while process loads: ${JSON.stringify({closeBefore, closeAfter})}`);
    assert.ok(await record.evaluate(node => node.contains(document.activeElement)), 'Record takes keyboard focus');
    await record.getByRole('button', {name: 'Cancel', exact: true}).focus(); await page.keyboard.press('Tab');
    assert.ok(await record.evaluate(node => node.contains(document.activeElement)), 'Tab remains inside the record dialog');
    await record.getByRole('button', {name: 'Close', exact: true}).click();
    await record.waitFor({state: 'hidden'}); await page.unroute(pattern);
    await page.locator('.pipeline-card').first().getByRole('button', {name: 'Open & review', exact: true}).click();
    await record.waitFor(); await until(() => record.evaluate(node => node.contains(document.activeElement)), 'Record focuses after reopening');
    await page.keyboard.press('Escape'); await record.waitFor({state: 'hidden'});
    const views = page.getByRole('navigation', {name: 'Module views', exact: true});
    const captures = [];
    for (const theme of ['light', 'dark']) {
      await page.evaluate(theme => {document.documentElement.dataset.theme = theme; document.documentElement.dataset.reducedMotion = 'true';}, theme);
      for (const [name, selector] of [['Pipeline', '.pipeline-card'], ['Directory', '.collection-grid article'], ['Operations', '.metric-grid'], ['All records', '.records-table tbody tr[data-record-uid]']]) {
        await views.getByRole('button', {name, exact: true}).click(); await page.locator(selector).first().waitFor();
        if (name === 'Pipeline') await until(() => page.getByRole('button', {name: 'Refresh pipeline', exact: true}).isEnabled(), 'Pipeline ready for visual review');
        assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), `${engine}/${theme}/${name}: no page overflow`);
        await page.screenshot({path: join(mx.directory, `${engine}-${theme}-${name.replaceAll(' ', '-')}.png`), animations: 'disabled'});
        captures.push(`${theme}/${name}`);
      }
    }
    await views.getByRole('button', {name: 'Pipeline', exact: true}).click(); await page.locator('.pipeline-card').first().waitFor();
    for (const [width, font] of [[1024, 16], [390, 16], [320, 24], [1440, 32]]) {
      await page.setViewportSize({width, height: 950});
      await page.evaluate(font => {document.documentElement.style.fontSize = `${font}px`; document.documentElement.dataset.theme = 'light';}, font);
      await until(async () => await page.locator('.module-workspace').evaluate(n => n.getBoundingClientRect().right <= innerWidth + 1), 'Workspace reflow');
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), `${width}/${font}: enlarged application fits`);
      assert.ok(await page.locator('.pipeline-board').evaluate(n => n.clientHeight >= 200), 'Useful board height');
      if (width <= 390) assert.ok(await page.locator('.pipeline-board').evaluate(n => n.scrollWidth > n.clientWidth), 'Narrow board scrolls internally');
      assert.ok(await page.getByRole('button', {name: 'New opportunity', exact: true}).isVisible());
      await page.screenshot({path: join(mx.directory, `${engine}-reflow-${width}-${font}.png`), animations: 'disabled'});
    }
    await page.setViewportSize({width: 1440, height: 950}); await page.evaluate(() => document.documentElement.style.fontSize = '100%');
    await page.goto(`${origin}/#drive`); await page.locator('.file-row').first().waitFor();
    await page.screenshot({path: join(mx.directory, `${engine}-drive.png`)});
    await page.goto(`${origin}/#collaboration`); await page.getByRole('navigation', {name: 'Collaboration views', exact: true}).waitFor();
    await page.screenshot({path: join(mx.directory, `${engine}-collaboration.png`)});
    await page.goto(`${origin}/#admin`); await page.getByRole('button', {name: 'Deployment', exact: true}).click();
    await page.getByRole('combobox', {name: 'Preset', exact: true}).selectOption('blue');
    assert.equal(await page.getByLabel('Primary color', {exact: true}).inputValue(), '#1d4ed8');
    await page.getByRole('combobox', {name: 'Preset', exact: true}).selectOption('sage');
    assert.equal(await page.getByLabel('Primary color', {exact: true}).inputValue(), '#c94128');
    await page.getByRole('button', {name: 'Save identity', exact: true}).click();
    await until(async () => (await call('/mx/v1/deployment/config', null)).config.appearance.preset === 'sage', 'Saved appearance preset');
    await page.screenshot({path: join(mx.directory, `${engine}-administration.png`)});
    assert.deepEqual(errors, []);
    result.browsers[engine] = {passed: true, geometry, captures, checks: ['Real pipeline/detail navigation', 'Compact heading/tabs', 'Accessible full description', 'Directory/overview/record views', 'Narrow and 200% text reflow', 'Drive/Collaboration/admin', 'Existing blue preset and saved product preset']};
    await context.close(); await browser.close(); browser = null;
    console.log(`${engine}: product theme and compact workspaces passed`);
  }
  result.passed = true;
} catch (error) {
  result.error = error.stack;
  if (page && !page.isClosed()) {await page.screenshot({path: join(mx.directory, 'product-theme-failure.png')}).catch(() => {}); await writeFile(join(mx.directory, 'failure-aria.txt'), await page.locator('body').ariaSnapshot().catch(() => ''));}
  throw error;
} finally {
  if (browser) await browser.close();
  await writeFile(join(mx.directory, 'product-theme-results.json'), JSON.stringify(result, null, 2));
  await mx.close();
}
