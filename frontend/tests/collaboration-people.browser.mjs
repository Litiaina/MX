// Fresh accounts and real MX: an absent chat must never count as a selection.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { isAbsolute, join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';

assert.ok(isAbsolute(process.env.MX_TEST_ARTIFACTS || ''), 'Set a disposable artifact directory');
assert.ok(!process.env.MX_TEST_N1_URL, 'Do not use live N1');
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
const browsers = [], results = { browsers: {} };
const call = async (path, user, method = 'GET', data, expected = [200]) =>
  (await mx.call(path, user?.access_token, method, data, expected)).body;
try {
  console.log(`Collaboration people artifacts: ${mx.directory}`);
  const password = `Disposable!${randomUUID()}`;
  await call('/mx/v1/auth/create', { access_token: mx.signupKey }, 'POST', { email: 'owner@people.test', name: 'Owner', password }, [201]);
  const owner = await call('/mx/v1/auth/authenticate', null, 'POST', { email: 'owner@people.test', password });
  for (const name of ['Ana', 'Ben', 'Mei']) {
    await call('/mx/v1/user/create', owner, 'POST', { email: `${name.toLowerCase()}@people.test`, name, password, access_level: 2 }, [201]);
  }
  const origin = `https://127.0.0.1:${mx.port}`;
  for (const engine of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
    // Each browser starts as a different untouched account with no chats.
    const email = `${{ chromium: 'ana', firefox: 'ben', webkit: 'mei' }[engine]}@people.test`;
    const user = await call('/mx/v1/auth/authenticate', null, 'POST', { email, password });
    const executablePath = engine === 'chromium' ? '/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome' : engine === 'firefox' ? '/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox' : '/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser = await ({ chromium, firefox, webkit })[engine].launch({ headless: true, executablePath, env: disconnectedBrowserAudio(mx.directory), ...(engine === 'chromium' ? { args: ['--no-sandbox', '--mute-audio'] } : {}) });
    browsers.push(browser);
    const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width: 1440, height: 950 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(tokens => {
      sessionStorage.setItem('mx_access_token', tokens.access_token);
      sessionStorage.setItem('mx_refresh_token', tokens.refresh_token);
    }, user);
    await page.goto(`${origin}/#collaboration`);
    const views = page.getByRole('navigation', { name: 'Collaboration views', exact: true });
    await views.getByRole('button', { name: 'People', exact: true }).click();
    const directory = page.getByLabel('Coworker directory', { exact: true });
    await directory.getByRole('button', { name: 'View Owner', exact: true }).waitFor();
    assert.equal(await directory.locator('.person-directory-row').count(), 3);
    assert.equal(await directory.locator('.person-directory-row.active').count(), 0, `${engine}: untouched people must not all be highlighted`);
    await directory.getByRole('button', { name: 'View Owner', exact: true }).click();
    const card = page.getByRole('dialog');
    await card.getByRole('button', { name: 'Start private chat', exact: true }).waitFor();
    assert.equal(await directory.locator('.person-directory-row.active').count(), 0, 'Viewing a profile does not select a chat');
    await card.getByRole('button', { name: 'Close', exact: true }).click();
    await directory.getByRole('button', { name: 'Start private chat with Owner', exact: true }).click();
    await page.locator('.conversation-title').getByRole('button', { name: 'Owner', exact: true }).waitFor();
    await directory.getByRole('button', { name: 'Open private chat with Owner', exact: true }).waitFor();
    assert.equal(await directory.locator('.person-directory-row.active').count(), 1, 'Exactly the selected chat is highlighted');
    assert.ok((await directory.locator('.person-directory-row.active').innerText()).includes('Owner'));
    for (const theme of ['light', 'dark']) {
      await page.evaluate(theme => document.documentElement.dataset.theme = theme, theme);
      await page.screenshot({ path: join(mx.directory, `${engine}-people-${theme}.png`) });
      assert.equal(await directory.locator('.person-directory-row.active').count(), 1);
    }
    const search = page.getByRole('textbox', { name: 'Search people', exact: true });
    await search.fill(engine === 'chromium' ? 'Ben' : 'Ana');
    assert.equal(await directory.locator('.person-directory-row').count(), 1);
    assert.equal(await directory.locator('.person-directory-row.active').count(), 0, 'Filtering out the selected chat does not select another person');
    await search.fill('');
    assert.equal(await directory.locator('.person-directory-row.active').count(), 1);
    await views.getByRole('button', { name: 'Chats', exact: true }).click();
    await views.getByRole('button', { name: 'People', exact: true }).click();
    assert.equal(await directory.locator('.person-directory-row.active').count(), 1, 'View switching retains only the real selected person');
    assert.deepEqual(errors, []);
    results.browsers[engine] = 'Fresh directory unselected; profile card unselected; one actual chat highlighted; other accounts unselected; search and view switching; light/dark';
    await browser.close();
  }
  await writeFile(join(mx.directory, 'collaboration-people-results.json'), JSON.stringify(results, null, 2));
  console.log(JSON.stringify(results, null, 2));
} finally {
  await Promise.all(browsers.map(browser => browser.close().catch(() => {})));
  await mx.close();
}
