// Real production UI, authentication, permission changes and SQLite. Only a
// disposable N1 HTTP contract fixture is configured; no deployment data/audio.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';

const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
const browsers = [], results = [], base = '/mx/v1/drive';
const silentEnvironment = { ...process.env,
  PULSE_SERVER: `unix:${join(mx.directory, 'no-audio.sock')}`,
  PIPEWIRE_REMOTE: `mx_drive_dropdowns_${randomUUID()}`,
  ALSA_CONFIG_PATH: fileURLToPath(new URL('./alsa-null.conf', import.meta.url)),
  SDL_AUDIODRIVER: 'dummy', ALSOFT_DRIVERS: 'null' };
const call = async (path, user, method = 'GET', data, expected = [200]) =>
  (await mx.call(path, user?.access_token, method, data, expected)).body;
async function until(check, label) {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    if (await check()) return;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw Error(`Dropdown test timeout: ${label}`);
}
async function visibleDropdowns(page, label) {
  const controls = await page.locator('select:visible').evaluateAll(nodes => nodes.map(node => {
    const style = getComputedStyle(node), rect = node.getBoundingClientRect();
    return { name: node.getAttribute('aria-label'), appearance: style.appearance,
      backgroundImage: style.backgroundImage, backgroundRepeat: style.backgroundRepeat,
      position: style.backgroundPositionX, iconSize: style.backgroundSize,
      inset: Number.parseFloat(style.getPropertyValue('--mx-select-inset')) * Number.parseFloat(getComputedStyle(document.documentElement).fontSize),
      paddingEnd: Number.parseFloat(style.paddingInlineEnd), forcedColors: matchMedia('(forced-colors: active)').matches,
      options: node.options.length,
      width: rect.width, height: rect.height, value: node.value };
  }));
  assert.ok(controls.length > 0, label);
  for (const control of controls) {
    if (control.forcedColors) {
      assert.equal(control.appearance, 'auto', 'High contrast uses native arrows');
      assert.equal(control.backgroundImage, 'none', 'High contrast never overlays a painted arrow');
    } else {
      assert.equal(control.appearance, 'none', `${label}: ${JSON.stringify(control)}`);
      assert.equal((control.backgroundImage.match(/url\(/g) || []).length, 1, 'Exactly one painted arrow');
      assert.equal(control.backgroundRepeat, 'no-repeat', 'Arrow never tiles behind the value');
      assert.equal(control.inset, 12, 'Arrow inset is 12px');
      assert.ok(control.position.includes('12px'), `Inset applies at the right border: ${JSON.stringify(control)}`);
      assert.equal(control.iconSize, '12px 12px');
      assert.ok(control.paddingEnd >= 32, 'Selected text stays clear of the arrow');
    }
    assert.ok(control.options > 0 && control.width >= 40 && control.height >= 28, JSON.stringify(control));
  }
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), `${label}: no body overflow`);
  return controls;
}
async function shareMenu(page, name) {
  await page.getByRole('button', { name: `More actions for ${name}`, exact: true }).click();
  await page.getByRole('menuitem', { name: 'Share', exact: true }).click();
  await page.getByRole('dialog', { name: 'Share file or folder' }).waitFor();
}
async function coreDropdowns(page, browser, owner) {
  const modes = [
    { theme: 'light', system: 'light' }, { theme: 'dark', system: 'light' },
    { theme: 'system', system: 'light' }, { theme: 'system', system: 'dark' }
  ];
  const pages = [
    { name: 'dashboard', open: async () => {
      await page.goto(`https://127.0.0.1:${mx.port}/#dashboard`);
      await page.getByRole('combobox', { name: 'Reporting period', exact: true }).waitFor();
    } },
    { name: 'schema', open: async () => {
      await page.goto(`https://127.0.0.1:${mx.port}/#admin`);
      await page.getByRole('button', { name: 'Modules & fields', exact: true }).click();
      await page.locator('.module-builder').getByRole('combobox', { name: 'Navigation icon', exact: true }).waitFor();
    } },
    { name: 'report-builder', open: async () => {
      await page.locator('.admin-subnav').getByRole('button', { name: 'Dashboard', exact: true }).click();
      await page.getByRole('combobox', { name: 'Show the answer as' }).waitFor();
    } },
    { name: 'accounts', open: async () => {
      await page.locator('.admin-subnav').getByRole('button', { name: 'Accounts', exact: true }).click();
      await page.getByRole('button', { name: 'Create account', exact: true }).click();
      await until(() => page.locator('select:visible').count().then(count => count > 0), 'Account controls loaded');
    } }
  ];
  const checked = [];
  for (const screen of pages) {
    await screen.open();
    for (const mode of modes) {
      await page.emulateMedia({ colorScheme: mode.system });
      await page.evaluate(theme => {
        document.documentElement.dataset.theme = theme;
        document.documentElement.dataset.reducedMotion = 'true';
      }, mode.theme);
      for (const width of [1440, 390]) {
        await page.setViewportSize({ width, height: width === 390 ? 850 : 1000 });
        const label = `${browser}/${screen.name}/${mode.theme}/${mode.system}/${width}`;
        const controls = await visibleDropdowns(page, label);
        checked.push({ screen: screen.name, ...mode, width, count: controls.length });
        await page.screenshot({ path: join(mx.directory, `${browser}-${screen.name}-${mode.theme}-${mode.system}-${width}.png`) });
      }
    }
    await page.setViewportSize({ width: 1440, height: 1000 });
    if (screen.name === 'dashboard') {
      await page.emulateMedia({ forcedColors: 'active' });
      await visibleDropdowns(page, `${browser}/dashboard/high-contrast`);
      const highContrast = await page.evaluate(() => matchMedia('(forced-colors: active)').matches);
      checked.push({ screen: 'dashboard', forcedColors: highContrast });
      await page.screenshot({ path: join(mx.directory, `${browser}-dashboard-high-contrast.png`) });
      await page.emulateMedia({ forcedColors: 'none' });
      const period = page.getByRole('combobox', { name: 'Reporting period', exact: true });
      await period.focus(); await page.keyboard.press('Home'); await page.keyboard.press('ArrowDown'); await page.keyboard.press('Enter');
      assert.equal(await period.inputValue(), 'today', 'Reporting period responds to actual keyboard selection');
      await period.selectOption('custom');
      await page.getByLabel('From', { exact: true }).waitFor();
      await page.getByRole('button', { name: 'Reset', exact: true }).click();
      assert.equal(await period.inputValue(), '30d');
    }
    if (screen.name === 'schema') {
      const form = page.locator('.module-builder'), icon = form.getByRole('combobox', { name: 'Navigation icon', exact: true });
      const moduleUid = await page.getByRole('combobox', { name: 'Change module', exact: true }).inputValue();
      const next = await icon.inputValue() === 'folder' ? 'database' : 'folder';
      await icon.selectOption(next);
      await form.getByRole('button', { name: 'Save changes', exact: true }).click();
      await page.getByText('Module identity, navigation, and access matrix saved.', { exact: true }).waitFor();
      assert.equal((await call('/mx/v1/modules', owner)).modules.find(module => module.uid === moduleUid).icon, next, 'Selected navigation icon saved on server');
    }
  }
  await page.emulateMedia({ colorScheme: 'light' });
  await page.evaluate(() => document.documentElement.dataset.theme = 'light');
  return checked;
}
let activePage, failure;
try {
  console.log(`Drive dropdown artifacts: ${mx.directory}`);
  const password = `Test-only!${randomUUID()}`;
  await call('/mx/v1/auth/create', { access_token: mx.signupKey }, 'POST', { email: 'dropdown-owner@test.invalid', name: 'Owner', password }, [201]);
  const owner = await call('/mx/v1/auth/authenticate', null, 'POST', { email: 'dropdown-owner@test.invalid', password });
  await call('/mx/v1/user/create', owner, 'POST', { email: 'dropdown-member@test.invalid', name: 'Dropdown Member', password, access_level: 2 }, [201]);
  const member = await call('/mx/v1/auth/authenticate', null, 'POST', { email: 'dropdown-member@test.invalid', password });
  for (const name of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
    const folder = await call(`${base}/folders`, owner, 'POST', { operation_uid: randomUUID(), name: `Dropdown ${name}` });
    const groupName = `Dropdown Group ${name}`;
    const group = await call('/mx/v1/collaboration/channels', owner, 'POST', { kind: 'group', name: groupName, member_uids: [member.uid] }, [201]);
    for (let start = 0; start < 55; start += 8) await Promise.all(Array.from({ length: Math.min(8, 55 - start) }, (_, i) =>
      call(`${base}/folders`, owner, 'POST', { operation_uid: randomUUID(), parent_uid: folder.uid, name: `Item ${String(start + i).padStart(3, '0')}` })));
    const executablePath = name === 'chromium' ? '/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome' : name === 'firefox' ? '/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox' : '/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser = await ({ chromium, firefox, webkit })[name].launch({ headless: true, executablePath, env: silentEnvironment, ...(name === 'chromium' ? { args: ['--mute-audio'] } : {}) });
    browsers.push(browser);
    const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width: 1440, height: 1000 } }); activePage = page;
    const errors = []; page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(tokens => { sessionStorage.setItem('mx_access_token', tokens.access_token); sessionStorage.setItem('mx_refresh_token', tokens.refresh_token); }, owner);
    const coreControls = await coreDropdowns(page, name, owner);
    await page.goto(`https://127.0.0.1:${mx.port}/#drive`);
    await page.getByRole('heading', { name: 'MX Drive', exact: true }).waitFor();
    await shareMenu(page, folder.name);
    const role = page.getByRole('combobox', { name: 'Access to grant', exact: true });
    const expiry = page.getByRole('combobox', { name: 'Guest link expiry', exact: true });
    assert.deepEqual(await role.locator('option').allTextContents(), ['Viewer', 'Editor']);
    assert.deepEqual(await expiry.locator('option').allTextContents(), ['Expires in 1 day', 'Expires in 7 days', 'Expires in 30 days', 'No expiration']);
    assert.equal(await page.getByRole('combobox', { name: 'Owner', exact: true }).count(), 0, 'Owner is a fixed role, not an editable dropdown');
    await role.click(); await page.keyboard.press('Escape');
    assert.ok(await page.getByRole('dialog', { name: 'Share file or folder' }).isVisible(), 'Dismissing a native dropdown does not close Share');
    await role.focus(); await page.keyboard.press('ArrowDown'); await page.keyboard.press('Enter');
    await until(() => role.inputValue().then(value => value === 'editor'), 'Keyboard chooses Editor');
    await page.getByRole('textbox', { name: 'Find an MX account' }).fill('Dropdown Member');
    await page.getByRole('button', { name: 'Find', exact: true }).click();
    // Wait for the actual commit response before checking the newly granted
    // user's ACL; a read during the in-flight POST correctly returns 404.
    const granted = page.waitForResponse(response => response.url().endsWith(`${base}/items/${folder.uid}/sharing`) && response.request().method() === 'POST');
    await page.getByRole('button', { name: 'Share with Dropdown Member', exact: true }).click();
    assert.equal((await granted).status(), 200, 'New user grant acknowledged');
    await until(async () => (await call(`${base}/items/${folder.uid}`, member)).permission === 'editor', 'Keyboard-selected grant persisted');
    const access = page.getByRole('combobox', { name: 'Access for Dropdown Member', exact: true });
    await until(() => access.isEnabled(), 'Access role ready');
    await access.focus(); await page.keyboard.press('ArrowUp'); await page.keyboard.press('Enter');
    await until(async () => (await call(`${base}/items/${folder.uid}`, member)).permission === 'viewer', 'Keyboard downgrade persisted');
    await page.getByRole('button', { name: 'Spaces & groups', exact: true }).click();
    await page.getByRole('textbox', { name: 'Find a collaboration space' }).fill(groupName);
    await page.getByRole('button', { name: 'Find', exact: true }).click();
    await page.getByRole('button', { name: `Share with space ${groupName}`, exact: true }).click();
    const space = page.getByRole('combobox', { name: `Space access for ${groupName}`, exact: true });
    await until(() => space.isEnabled(), 'Group role ready');
    await space.focus(); await page.keyboard.press('ArrowUp'); await page.keyboard.press('Enter');
    await until(async () => (await call(`${base}/items/${folder.uid}/sharing`, owner)).spaces.some(entry => entry.space_uid === group.uid && entry.role === 'viewer'), 'Group role change persisted');
    for (const theme of ['light', 'dark']) {
      await page.evaluate(theme => document.documentElement.dataset.theme = theme, theme);
      for (const width of [1440, 390]) {
        await page.setViewportSize({ width, height: width === 390 ? 850 : 1000 });
        await visibleDropdowns(page, `${name}/${theme}/${width}`);
        await page.screenshot({ path: join(mx.directory, `${name}-dropdown-${theme}-${width}.png`) });
      }
    }
    await page.setViewportSize({ width: 1440, height: 1000 });
    // Exercise all expiry values using keyboard input, then verify the actual
    // server metadata rather than only the dropdown's displayed selection.
    for (let index = 0; index < 4; index++) {
      await expiry.focus(); await page.keyboard.press('Home');
      for (let step = 0; step < index; step++) await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Enter');
      const days = [1, 7, 30, 0][index]; assert.equal(await expiry.inputValue(), String(days));
      const previous = (await call(`${base}/items/${folder.uid}/sharing`, owner)).links.map(link => link.uid), started = Date.now();
      await page.getByRole('button', { name: 'Create link', exact: true }).click();
      let created;
      await until(async () => { created = (await call(`${base}/items/${folder.uid}/sharing`, owner)).links.find(link => !previous.includes(link.uid)); return !!created; }, `Created ${days}-day link`);
      if (days) assert.ok(Math.abs(created.expires_at - (started + days * 86400000)) < 10000, JSON.stringify(created));
      else assert.equal(created.expires_at, null);
      await until(() => page.getByRole('button', { name: 'Create link', exact: true }).isEnabled(), 'Link request complete');
    }
    await page.getByRole('button', { name: 'Close dialog', exact: true }).focus();
    await page.keyboard.press('Escape');
    assert.equal(await page.getByRole('dialog', { name: 'Share file or folder' }).count(), 0, 'Escape outside a select still closes Share');
    await page.locator('.file-name').filter({ hasText: folder.name }).dblclick();
    const sort = page.getByRole('combobox', { name: 'Sort Drive files' });
    for (const value of ['modified_desc', 'modified_asc', 'name_asc', 'name_desc']) {
      const response = page.waitForResponse(r => r.url().includes(`${base}?`) && new URL(r.url()).searchParams.get('sort') === value && r.status() === 200);
      // Ensure the first value differs, so it produces a real change event.
      if (await sort.inputValue() === value) { await sort.selectOption(value === 'modified_desc' ? 'modified_asc' : 'modified_desc'); }
      await sort.selectOption(value); await response;
      await until(() => page.locator('.file-row').count().then(count => count === 50), 'Sorted bounded page');
    }
    const perPage = page.getByRole('combobox', { name: 'Drive items per page' });
    for (const value of ['25', '50', '100']) {
      await until(() => perPage.isEnabled(), 'Page-size ready'); await perPage.selectOption(value);
      await until(() => page.locator('.file-row').count().then(count => count === Math.min(55, Number(value))), `Page size ${value}`);
      assert.equal(await perPage.inputValue(), value);
    }
    for (const theme of ['light', 'dark']) {
      await page.evaluate(theme => document.documentElement.dataset.theme = theme, theme);
      for (const width of [1440, 390]) {
        await page.setViewportSize({ width, height: width === 390 ? 850 : 1000 });
        await visibleDropdowns(page, `${name}/listing/${theme}/${width}`);
        await page.screenshot({ path: join(mx.directory, `${name}-listing-${theme}-${width}.png`) });
      }
    }
    assert.deepEqual(errors, []);
    results.push({ browser: name, passed: true, keyboard: 'reporting period, proposed/user/group roles and all guest expiries', server: 'navigation icon + ACL and expiry metadata', listing: 'all sort choices + 25/50/100 pages', layouts: 'light/dark/system light/system dark at 1440/390', coreControls, nativeControls: true, arrowInsetPx: 12, singleNonRepeatingChevron: true });
    console.log(JSON.stringify(results.at(-1))); await browser.close();
  }
} catch (error) {
  failure = error.stack; process.exitCode = 1;
  await activePage?.screenshot({ path: join(mx.directory, 'dropdown-failure.png'), fullPage: true }).catch(() => {});
} finally {
  for (const browser of browsers) await browser.close().catch(() => {});
  const report = { passed: !failure, results, failure, storage: 'isolated HTTP contract fixture; no live N1', productionQualified: false };
  await writeFile(join(mx.directory, 'dropdown-results.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ ...report, artifacts: mx.directory })); await mx.close();
}
