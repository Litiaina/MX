// Use production CSS and a disposable MX database, never the encoding test or MCH.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { isAbsolute, join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';

assert.ok(isAbsolute(process.env.MX_TEST_ARTIFACTS || ''), 'Set MX_TEST_ARTIFACTS to an external artifact directory');
assert.ok(!process.env.MX_TEST_N1_URL, 'Sidebar tests must not use live N1');
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
const browsers = [];
const results = [];
let page;
const settle = async () => {
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  // Shell width transitions are intentionally independent of reduced-motion preferences.
  await page.waitForTimeout(200);
};
try {
  console.log(`Sidebar regression artifacts: ${mx.directory}`);
  const credentials = {email: 'sidebar@test.invalid', name: 'Sidebar tester', password: `Test-only!${randomUUID()}`};
  await mx.call('/mx/v1/auth/create', mx.signupKey, 'POST', credentials, [201]);
  const tokens = (await mx.call('/mx/v1/auth/authenticate', null, 'POST', credentials)).body;
  for (let index = 0; index < 18; index++) {
    await mx.call('/mx/v1/admin/modules', tokens.access_token, 'POST', {
      name: `Department ${String(index + 1).padStart(2, '0')} with a long navigation title`,
      singular_name: 'Entry', icon: 'database', config: {navigation_group: 'Departments'}
    }, [201]);
  }
  for (const engine of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
    const executablePath = engine === 'chromium' ? '/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome'
      : engine === 'firefox' ? '/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox'
      : '/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser = await ({chromium, firefox, webkit})[engine].launch({headless: true, executablePath,
      env: disconnectedBrowserAudio(mx.directory), ...(engine === 'chromium' ? {args: ['--no-sandbox', '--mute-audio']} : {})});
    browsers.push(browser);
    page = await browser.newPage({ignoreHTTPSErrors: true, viewport: {width: 1440, height: 900}});
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(tokens => {
      sessionStorage.setItem('mx_access_token', tokens.access_token);
      sessionStorage.setItem('mx_refresh_token', tokens.refresh_token);
    }, tokens);
    await page.goto(`https://127.0.0.1:${mx.port}/#drive`, {waitUntil: 'networkidle'});
    await page.getByRole('heading', {name: 'MX Drive', exact: true}).waitFor();
    const sidebar = page.getByRole('complementary', {name: 'Workspace navigation'});
    await sidebar.locator('.sidebar-brand img').waitFor();
    for (const theme of ['light', 'dark']) for (const factor of [.85, 1, 1.6]) {
      await page.evaluate(({theme, factor}) => {
        document.documentElement.dataset.theme = theme;
        document.documentElement.style.setProperty('--interface-factor', String(factor));
      }, {theme, factor});
      await settle();
      const geometry = await sidebar.evaluate(node => {
        const rect = element => {
          const r = element.getBoundingClientRect();
          return {x: r.x, y: r.y, right: r.right, bottom: r.bottom, width: r.width, height: r.height, center: r.x + r.width / 2};
        };
        const sidebar = rect(node), nav = node.querySelector('.main-nav');
        const icons = [...nav.querySelectorAll('button > span > svg')];
        return {sidebar, expectedCenter: sidebar.x + (sidebar.width - parseFloat(getComputedStyle(node).borderRightWidth)) / 2,
          logo: rect(node.querySelector('.sidebar-brand > img')), search: rect(node.querySelector('.sidebar-search-trigger > svg')),
          icons: icons.map(rect), profile: rect(node.querySelector('.profile-button > span')), live: rect(node.querySelector('.live-state i')),
          navWidth: nav.clientWidth, navScrollWidth: nav.scrollWidth,
          footer: rect(node.querySelector('.sidebar-foot'))};
      });
      if (factor === .85) await page.screenshot({path: join(mx.directory, `${engine}-${theme}-collapsed-85.png`)});
      for (const [name, box] of Object.entries({logo: geometry.logo, search: geometry.search, profile: geometry.profile, live: geometry.live, ...Object.fromEntries(geometry.icons.map((box, index) => [`icon-${index}`, box]))})) {
        assert.ok(Math.abs(box.center - geometry.expectedCenter) <= 1, `${engine}/${theme}/${factor}: ${name} centered: ${JSON.stringify({box, expected: geometry.expectedCenter})}`);
        assert.ok(box.x >= geometry.sidebar.x + 3 && box.right <= geometry.sidebar.right - 3, `${name} has room inside the rail`);
      }
      assert.ok(geometry.navScrollWidth <= geometry.navWidth + 1, 'Collapsed navigation has no clipped expanded-width rows');
      assert.ok(geometry.footer.bottom <= 901, 'Footer remains available with many modules');
      assert.ok(await sidebar.locator('.sidebar-brand img').evaluate(img => img.complete && img.naturalWidth > 0), 'Existing branding image is intact');
      assert.equal(await sidebar.locator('.sidebar-search-field input').isVisible(), false, 'Collapsed search is hidden');
      assert.equal(await sidebar.locator('.sidebar-search-field input').evaluate(input => {
        input.focus();
        return document.activeElement === input;
      }), false, 'Hidden search field cannot receive focus');
      await page.getByRole('button', {name: 'Expand navigation to search', exact: true}).click();
      await settle();
      await sidebar.locator('.sidebar-search-field input').waitFor({state: 'visible'});
      assert.ok(await sidebar.locator('.sidebar-search-field input').evaluate(input => document.activeElement === input), 'Opening search focuses its input');
      const expanded = await sidebar.evaluate(node => {
        const nav = node.querySelector('.main-nav');
        const r = node.getBoundingClientRect();
        return {right: r.right, navWidth: nav.clientWidth, navScrollWidth: nav.scrollWidth,
          children: [...node.querySelectorAll('.sidebar-brand > img,.sidebar-toggle,.sidebar-search-field,.main-nav button,.profile-button')].map(element => ({right: element.getBoundingClientRect().right}))};
      });
      assert.ok(expanded.navScrollWidth <= expanded.navWidth + 1, 'Expanded navigation has no horizontal clipping');
      assert.ok(expanded.children.every(child => child.right <= expanded.right - 1), 'Expanded controls fit inside the drawer');
      await page.screenshot({path: join(mx.directory, `${engine}-${theme}-expanded-${factor}.png`)});
      await sidebar.getByRole('button', {name: 'Collapse navigation', exact: true}).click();
      await settle();
      const drive = sidebar.getByRole('button', {name: 'MX Drive', exact: true});
      await drive.scrollIntoViewIfNeeded();
      await drive.focus();
      assert.ok(await drive.evaluate(button => {
        const r = button.getBoundingClientRect();
        return button.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2));
      }), 'Collapsed navigation remains clickable and keyboard focusable');
      results.push({engine, theme, factor, collapsed_and_expanded: true});
    }
    for (const width of [390, 320]) {
      await page.setViewportSize({width, height: 844});
      await page.getByRole('button', {name: 'Open navigation', exact: true}).click();
      await settle();
      await sidebar.getByRole('button', {name: 'Collapse navigation', exact: true}).waitFor();
      assert.ok((await sidebar.boundingBox()).width <= width, 'Mobile drawer fits');
      const right = (await sidebar.boundingBox()).x + (await sidebar.boundingBox()).width;
      assert.ok((await sidebar.locator('.sidebar-search-field').boundingBox()).x + (await sidebar.locator('.sidebar-search-field').boundingBox()).width <= right - 1, 'Mobile search fits');
      await sidebar.getByRole('button', {name: 'Collapse navigation', exact: true}).click();
      await settle();
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'No mobile page overflow');
      results.push({engine, width, mobile: true});
    }
    assert.deepEqual(errors, []);
    await browser.close();
  }
  await writeFile(join(mx.directory, 'sidebar-results.json'), JSON.stringify({passed: true, results}, null, 2));
  console.log(JSON.stringify({passed: true, cases: results.length, directory: mx.directory}));
} catch (error) {
  if (page && !page.isClosed()) await page.screenshot({path: join(mx.directory, 'sidebar-failure.png')}).catch(() => {});
  throw error;
} finally {
  await Promise.all(browsers.map(browser => browser.close().catch(() => {})));
  await mx.close();
}
