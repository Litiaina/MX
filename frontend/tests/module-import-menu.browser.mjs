// Menu regressions use a disposable MX database and a fixture, never live N1.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join, isAbsolute } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';

assert.ok(isAbsolute(process.env.MX_TEST_ARTIFACTS || ''), 'Set an explicit MX_TEST_ARTIFACTS directory');
assert.ok(!process.env.MX_TEST_N1_URL, 'Menu tests must not touch live N1');
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
const results = [];
let activePage;
const browsers = [];
const diagnostic = process.env.MX_MENU_DIAGNOSTIC === '1';
try {
  console.log(`Menu test artifacts: ${mx.directory}`);
  const credentials = { email: 'menu@test.invalid', name: 'Menu tester', password: `Test-only!${randomUUID()}` };
  await mx.call('/mx/v1/auth/create', mx.signupKey, 'POST', credentials, [201]);
  const user = (await mx.call('/mx/v1/auth/authenticate', null, 'POST', credentials)).body;
  const module = (await mx.call('/mx/v1/admin/modules', user.access_token, 'POST', { name: 'Registrations', singular_name: 'Registration' }, [201])).body.module;
  await mx.call(`/mx/v1/admin/modules/${module.uid}/fields`, user.access_token, 'POST', { key: 'name', label: 'Name', field_type: 'text' }, [201]);
  await mx.call(`/mx/v1/modules/${module.uid}/records`, user.access_token, 'POST', { values: { name: 'Menu fixture' } }, [201]);
  for (const engine of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
    const executablePath = engine === 'chromium' ? '/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome' : engine === 'firefox' ? '/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox' : '/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser = await ({ chromium, firefox, webkit })[engine].launch({ headless: true, executablePath, env: disconnectedBrowserAudio(mx.directory), ...(engine === 'chromium' ? { args: ['--no-sandbox', '--mute-audio'] } : {}) });
    browsers.push(browser);
    const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width: 1440, height: 950 } });
    activePage = page;
    const errors = [];
    const focused = async locator => {
      try { await page.waitForFunction(element => document.activeElement === element, await locator.elementHandle(), { timeout: 2000 }); }
      catch (failure) { await writeFile(join(mx.directory, `${engine}-focus-failure.json`), JSON.stringify(await page.evaluate(() => {
        const before = document.activeElement?.outerHTML;
        return { active: before, visibility: document.visibilityState, documentFocused: document.hasFocus(), trace: window.menuFocusTrace, frames: window.menuFrameTrace, nodes: Array.from(document.querySelectorAll('.module-import-menu button')).map(n => ({ text: n.textContent, connected: n.isConnected, tabindex: n.tabIndex, inertParent: !!n.closest('[inert]'), visibility: getComputedStyle(n).visibility })) };
      }), null, 2)); throw failure; }
    };
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(tokens => { sessionStorage.setItem('mx_access_token', tokens.access_token); sessionStorage.setItem('mx_refresh_token', tokens.refresh_token); }, user);
    await page.addInitScript(() => { window.menuFocusTrace = []; for (const type of ['focusin', 'keydown', 'keyup']) document.addEventListener(type, event => { window.menuFocusTrace.push({ type, key: event.key, target: event.target?.getAttribute?.('aria-label') || event.target?.textContent?.slice(0, 35), active: document.activeElement?.getAttribute('aria-label') || document.activeElement?.textContent?.slice(0, 35) }); if (window.menuFocusTrace.length > 70) window.menuFocusTrace.shift(); }, true); });
    if (process.env.MX_MENU_FRAME_TRACE === '1') await page.addInitScript(() => {
      const request = requestAnimationFrame.bind(window), cancel = cancelAnimationFrame.bind(window), tracked = new Set();
      window.menuFrameTrace = [];
      const record = value => { window.menuFrameTrace.push(value); if (window.menuFrameTrace.length > 80) window.menuFrameTrace.shift(); };
      window.requestAnimationFrame = callback => {
        const relevant = callback.toString().includes('module-import-menu');
        const id = request(time => { if (relevant) record({ event: 'before', id, active: document.activeElement?.textContent }); callback(time); if (relevant) record({ event: 'after', id, active: document.activeElement?.textContent }); });
        if (relevant) { tracked.add(id); record({ event: 'requested', id }); }
        return id;
      };
      window.cancelAnimationFrame = id => { if (tracked.has(id)) record({ event: 'cancelled', id, stack: new Error().stack?.slice(0, 400) }); cancel(id); };
    });
    await page.goto(`https://127.0.0.1:${mx.port}/#module/${module.uid}`);
    const trigger = page.getByRole('button', { name: 'Module actions', exact: true });
    await trigger.waitFor();
    for (const theme of ['light', 'dark']) for (const width of [1440, 1024, 390, 320]) {
      await page.setViewportSize({ width, height: 950 });
      await page.evaluate(theme => { document.documentElement.dataset.theme = theme; document.documentElement.dataset.reducedMotion = 'true'; }, theme);
      await trigger.scrollIntoViewIfNeeded();
      await page.waitForTimeout(150);
      await trigger.click();
      const menu = page.locator('.module-import-menu');
      await menu.waitFor({ state: 'visible' });
      await page.waitForTimeout(100);
      const geometry = await page.evaluate(() => {
        const rect = selector => { const n = document.querySelector(selector); const r = n.getBoundingClientRect(); return { x: r.x, y: r.y, right: r.right, bottom: r.bottom, width: r.width, height: r.height, order: getComputedStyle(n).order }; };
        const m = document.querySelector('.module-import-menu');
        const b = m.getBoundingClientRect();
        return { trigger: rect('[aria-label="Module actions"]'), refresh: rect('.toolbar > button:not(.records-new-button):not([aria-label])'), search: rect('.records-search-input'), menu: { left: b.left, top: b.top, right: b.right, bottom: b.bottom }, viewport: { width: innerWidth, height: innerHeight }, hit: m.contains(document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2)), pageWidth: document.documentElement.scrollWidth };
      });
      results.push({ engine, theme, width, geometry });
      await page.screenshot({ path: join(mx.directory, `${engine}-module-menu-${theme}-${width}.png`) });
      if (!diagnostic) {
        assert.ok(Math.abs(geometry.trigger.height - geometry.refresh.height) <= 1, 'Three-dot button matches toolbar control height');
        assert.ok(Math.abs(geometry.trigger.width - geometry.trigger.height) <= 1, 'Three-dot control is square');
        if (width >= 1024) assert.ok(geometry.trigger.x >= geometry.refresh.right, 'Module menu follows Refresh, not before search');
        if (width <= 760) assert.ok(Math.abs(geometry.trigger.y - geometry.refresh.y) <= 1 && geometry.trigger.x >= geometry.refresh.right, 'Refresh and module actions stay together on mobile');
        assert.ok(geometry.menu.left >= 7 && geometry.menu.right <= width - 7 && geometry.menu.top >= 7 && geometry.menu.bottom <= 943, 'Menu stays inside viewport');
        assert.ok(geometry.hit, 'Menu is not covered by records or filters');
        assert.ok(geometry.pageWidth <= width + 1, 'Menu does not create horizontal overflow');
        assert.equal(await trigger.getAttribute('aria-expanded'), 'true');
      }
      await page.keyboard.press('Escape');
      await menu.waitFor({ state: 'hidden' });
      if (!diagnostic) await focused(trigger);
    }
    if (!diagnostic) {
      await page.setViewportSize({ width: 1440, height: 950 });
      await trigger.focus(); await page.keyboard.press('ArrowDown');
      const menu = page.getByRole('menu', { name: 'Module actions', exact: true });
      const start = menu.getByRole('menuitem', { name: 'Import records', exact: true });
      const history = menu.getByRole('menuitem', { name: 'Import history', exact: true });
      await start.waitFor(); await focused(start);
      await page.keyboard.press('ArrowDown'); await focused(history);
      await page.keyboard.press('Home'); await focused(start);
      await page.keyboard.press('End'); await focused(history);
      await page.keyboard.press('Escape'); await trigger.focus(); await page.keyboard.press('ArrowUp');
      await history.waitFor(); await focused(history);
      await page.keyboard.press('Enter');
      const dialog = page.getByRole('dialog', { name: 'Import records', exact: true });
      await dialog.waitFor(); await dialog.getByRole('heading', { name: 'Import history', exact: true }).waitFor();
      await dialog.getByRole('button', { name: 'Close import', exact: true }).click();
      await trigger.click(); await start.click(); await dialog.waitFor();
      await dialog.getByRole('heading', { name: 'Choose your spreadsheet', exact: true }).waitFor();
      await dialog.getByRole('button', { name: 'Close import', exact: true }).click();
      await trigger.click(); await page.getByRole('button', { name: 'Refresh', exact: true }).click(); await menu.waitFor({ state: 'hidden' });
      await trigger.click(); await page.keyboard.press('Tab'); await menu.waitFor({ state: 'hidden' });
      await trigger.click(); await page.getByLabel('More actions for Registration 1', { exact: true }).click(); await menu.waitFor({ state: 'hidden' });
      await page.keyboard.press('Escape');
    }
    assert.deepEqual(errors, []);
    await browser.close();
    console.log(`${engine}: module menu ${diagnostic ? 'baseline recorded' : 'layout, viewport, keyboard and both import actions passed'}`);
  }
  await writeFile(join(mx.directory, 'module-menu-results.json'), JSON.stringify({ passed: !diagnostic, diagnostic, results }, null, 2));
} catch (error) {
  if (activePage && !activePage.isClosed()) await activePage.screenshot({ path: join(mx.directory, 'module-menu-failure.png') }).catch(() => {});
  throw error;
} finally { await Promise.all(browsers.map(browser => browser.close().catch(() => {}))); await mx.close(); }
