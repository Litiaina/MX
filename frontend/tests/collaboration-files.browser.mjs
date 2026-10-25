// Real MX/N1 contract storage and real bundled Office engine. No live deployment.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join, isAbsolute } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { request as httpsRequest } from 'node:https';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';
import { officeFixture } from './office-fixtures.mjs';

assert.ok(isAbsolute(process.env.MX_TEST_ARTIFACTS || ''), 'Set an explicit disposable artifact root');
assert.ok(!process.env.MX_TEST_N1_URL, 'Never use live N1');
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
// Trusted-by-browser HTTP loopback, without changing workstation certificate trust.
const proxy = createServer((incoming, outgoing) => {
  const upstream = httpsRequest({ hostname: '127.0.0.1', port: mx.port, path: incoming.url, method: incoming.method, headers: incoming.headers, rejectUnauthorized: false }, response => { outgoing.writeHead(response.statusCode, response.headers); response.pipe(outgoing); });
  upstream.on('error', () => { outgoing.writeHead(502); outgoing.end(); }); incoming.pipe(upstream);
});
await new Promise(resolve => proxy.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${proxy.address().port}`, browsers = [], results = { browsers: {}, limitations: 'Cross-engine clipboard/drop coverage uses browser DataTransfer events with independent File wrappers. Chromium/Brave additionally use their isolated headless clipboard and native Ctrl+V/Ctrl+C; this is not physical Windows clipboard certification. Office uses the actual bundled engine, not a mocked renderer.' };
let inspected;
const call = async (path, user, method = 'GET', data, expected = [200]) => (await mx.call(path, user?.access_token, method, data, expected)).body;
const until = async (check, label, timeout = 30000) => { const end = Date.now() + timeout; while (Date.now() < end) { if (await check()) return; await new Promise(resolve => setTimeout(resolve, 100)); } throw Error(label); };
try {
  console.log(`Collaboration file artifacts: ${mx.directory}`);
  const password = `Disposable!${randomUUID()}`;
  await call('/mx/v1/auth/create', { access_token: mx.signupKey }, 'POST', { email: 'owner@chatfiles.test', name: 'Owner', password }, [201]);
  const owner = await call('/mx/v1/auth/authenticate', null, 'POST', { email: 'owner@chatfiles.test', password });
  const users = [];
  for (const name of ['Recipient', 'Outsider']) {
    const email = `${name.toLowerCase()}@chatfiles.test`;
    await call('/mx/v1/user/create', owner, 'POST', { email, name, password, access_level: 2 }, [201]);
    users.push(await call('/mx/v1/auth/authenticate', null, 'POST', { email, password }));
  }
  const [recipient, outsider] = users;
  const channel = await call('/mx/v1/collaboration/direct', owner, 'POST', { user_uid: recipient.uid }, [200, 201]);
  const other = await call('/mx/v1/collaboration/direct', owner, 'POST', { user_uid: outsider.uid }, [200, 201]);

  for (const engine of (process.env.MX_TEST_BROWSERS || 'chromium,firefox,webkit').split(',')) {
    const fallbackGraphics = (process.env.MX_TEST_OFFICE_FALLBACK_BROWSERS || '').split(',').includes(engine);
    const executablePath = engine === 'brave' ? '/usr/bin/brave-browser' : engine === 'chromium' ? '/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome' : engine === 'firefox' ? '/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox' : '/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const launcher = engine === 'brave' ? chromium : ({ chromium, firefox, webkit })[engine];
    const browser = await launcher.launch({ headless: true, executablePath, env: disconnectedBrowserAudio(mx.directory), ...(['chromium', 'brave'].includes(engine) ? { args: ['--no-sandbox', '--mute-audio'] } : {}) }); browsers.push(browser);
    const context = await browser.newContext({ viewport: { width: 1440, height: 950 }, acceptDownloads: true });
    await context.addInitScript(({ tokens, origin }) => { if (location.origin !== origin) return; sessionStorage.setItem('mx_access_token', tokens.access_token); sessionStorage.setItem('mx_refresh_token', tokens.refresh_token); }, { tokens: owner, origin });
    const errors = [], external = [], unsafe = [], legacyConversions = [], engineResources = [];
    context.on('page', page => page.on('pageerror', error => errors.push(error.message)));
    const guard = route => {
      const request = route.request(), url = new URL(request.url());
      if (['http:', 'https:'].includes(url.protocol) && url.origin !== origin) { external.push(url.href); return route.abort(); }
      if (/\/collaboration\/files\/[^/]+\/preview$/.test(url.pathname)) legacyConversions.push(url.pathname);
      if (url.pathname.includes('/office/vendor/soffice')) engineResources.push(url.pathname);
      if (/\/drive\/(?:uploads|items)/.test(url.pathname) && request.frame().page().url().includes('message_file=')) unsafe.push({ method: request.method(), path: url.pathname });
      return route.continue();
    };
    await context.route('**/*', guard);
    const page = await context.newPage(); inspected = page;
    const initial = await page.goto(`${origin}/#collaboration?channel=${channel.uid}`);
    assert.equal(initial.status(), 200, 'The frozen production frontend must be available');
    const composer = page.getByRole('textbox', { name: 'Message', exact: true }); await composer.waitFor();
    await page.locator('.conversation-title').getByRole('button', { name: 'Recipient', exact: true }).waitFor();
    const payloads = [
      { name: `Clipboard-${engine}.docx`, type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', bytes: [...officeFixture('.docx')] },
      { name: `Budget-${engine}.xlsx`, type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet', bytes: [...officeFixture('.xlsx')] },
      { name: `Clipboard-${engine}.pdf`, type: 'application/pdf', bytes: [...Buffer.from('%PDF-1.4\n%%EOF\n')] },
      { name: `Notes-${engine}.txt`, type: 'text/plain', bytes: [...Buffer.from(`Notes from ${engine}`)] },
      { name: 'image.png', type: 'image/png', bytes: [...Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j1ioAAAAASUVORK5CYII=', 'base64')] }
    ];
    const paste = async (target, values) => page.evaluate(({ target, values }) => {
      const transfer = new DataTransfer(), list = new DataTransfer();
      for (const item of values) {
        transfer.items.add(new File([new Uint8Array(item.bytes)], item.name, { type: item.type, lastModified: 1 }));
        list.items.add(new File([new Uint8Array(item.bytes)], item.name, { type: item.type, lastModified: 2 }));
      }
      // Real clipboard getters may manufacture separate File wrappers. Reusing
      // one File object/mtime for both surfaces hid the single-paste duplication.
      Object.defineProperty(transfer, 'files', { value: list.files });
      const event = new ClipboardEvent('paste', { bubbles: true, cancelable: true, clipboardData: transfer });
      if (event.clipboardData?.files.length !== transfer.files.length) Object.defineProperty(event, 'clipboardData', { value: transfer });
      document.querySelector(target).dispatchEvent(event); return event.defaultPrevented;
    }, { target, values });
    assert.equal(await paste('.message-composer textarea[aria-label="Message"]', payloads), true);
    await until(async () => await page.locator('.pending-shares button[aria-label^="Remove "]').count() > 0, 'Clipboard attachments queued');
    assert.equal(await page.locator('.pending-shares button[aria-label^="Remove "]').count(), 5, 'One paste queues each original once even when items/files timestamps differ');
    assert.equal(await paste('.message-composer', payloads.slice(0, 4)), true);
    assert.equal(await page.locator('.pending-shares button').count(), 5, 'Items/files and repeated paste do not duplicate files');
    assert.ok((await page.locator('.pending-shares').innerText()).includes('Screenshot-'), 'Generic clipboard image receives a meaningful filename');
    assert.equal(await page.evaluate(() => {
      const data = new DataTransfer(); data.setData('text/plain', 'Plain text');
      const event = new ClipboardEvent('paste', { bubbles: true, cancelable: true }); Object.defineProperty(event, 'clipboardData', { value: data });
      document.querySelector('.message-composer textarea').dispatchEvent(event); return event.defaultPrevented;
    }), false, 'Ordinary text paste stays native');
    assert.equal(await page.evaluate(() => {
      const data = new DataTransfer(); data.setData('text/plain', 'A dragged text selection');
      const event = new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: data }); document.querySelector('.conversation-main').dispatchEvent(event); return event.defaultPrevented;
    }), false, 'Text drags are not hijacked');
    const dropped = { name: `Dropped-${engine}.txt`, type: 'text/plain', bytes: [...Buffer.from('Dropped content')] };
    assert.equal(await page.evaluate(item => {
      const data = new DataTransfer(); data.items.add(new File([new Uint8Array(item.bytes)], item.name, { type: item.type, lastModified: 1 }));
      const node = document.querySelector('.message-list');
      for (const type of ['dragenter', 'dragover']) node.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: data }));
      const event = new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: data }); node.dispatchEvent(event); return event.defaultPrevented;
    }, dropped), true);
    await until(async () => await page.locator('.pending-shares button').count() === 6, 'Dropped file queued');
    assert.equal(await page.locator('.conversation-drop-hint').count(), 0);
    assert.ok(page.url().includes('#collaboration'), 'Dropping never navigates away');

    // Rejection feedback is beside the queued files, not hidden above the thread.
    await page.evaluate(() => {
      const data = new DataTransfer(), file = new File(['x'], 'Too-large.dat');
      Object.defineProperty(file, 'size', { value: 1024 ** 3 }); data.items.add(file);
      const event = new ClipboardEvent('paste', { bubbles: true, cancelable: true }); Object.defineProperty(event, 'clipboardData', { value: data });
      document.querySelector('.message-composer textarea').dispatchEvent(event);
    });
    await page.locator('.composer-file-notice').filter({ hasText: 'larger than' }).waitFor();
    assert.equal(await page.locator('.pending-shares button').count(), 6);
    await page.getByRole('button', { name: 'Dismiss attachment notice', exact: true }).click();

    // Draft files belong to their intended conversation, not whichever chat opens next.
    await page.goto(`${origin}/#collaboration?channel=${other.uid}`);
    await page.locator('.conversation-title').getByRole('button', { name: 'Outsider', exact: true }).waitFor();
    assert.equal(await page.locator('.pending-shares').count(), 0);
    await page.goto(`${origin}/#collaboration?channel=${channel.uid}`);
    await page.locator('.conversation-title').getByRole('button', { name: 'Recipient', exact: true }).waitFor();
    await until(async () => await page.locator('.pending-shares button').count() === 6, 'Returning preserves the six intended files');
    const messageText = `Files regression ${engine}`;
    await composer.fill(messageText);
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    let posted;
    await until(async () => { const messages = (await call(`/mx/v1/collaboration/channels/${channel.uid}/messages`, owner)).messages; posted = messages.find(message => message.body === messageText); return posted?.files.length === 6; }, 'Files uploaded through MX and N1');
    await until(async () => await page.locator('.pending-shares').count() === 0, 'Successful send clears attachments');
    const recipientMessage = (await call(`/mx/v1/collaboration/channels/${channel.uid}/messages`, recipient)).messages.find(message => message.uid === posted.uid);
    assert.equal(recipientMessage.files.length, 6);
    assert.equal(recipientMessage.files.filter(file => file.mime_type === 'image/png').length, 1, 'The recipient receives exactly one pasted screenshot');
    for (const expected of [...payloads, dropped]) {
      const file = posted.files.find(file => expected.name === 'image.png' ? file.mime_type === 'image/png' : file.file_name === expected.name); assert.ok(file);
      const response = await mx.api.get(`/mx/v1/collaboration/files/${file.uid}/download`, { headers: { Authorization: `Bearer ${recipient.access_token}` } });
      assert.equal(response.status(), 200); assert.deepEqual([...await response.body()], expected.bytes);
      await call(`/mx/v1/collaboration/files/${file.uid}/preview-ticket`, outsider, 'POST', undefined, [404]);
    }
    if (['chromium', 'brave'].includes(engine)) {
      // Exercise a genuine paste event, not only a constructed DataTransfer.
      // This is a fresh headless browser, not the operator's desktop session.
      await context.grantPermissions(['clipboard-read', 'clipboard-write'], { origin });
      await page.evaluate(async () => {
        const canvas = document.createElement('canvas'); canvas.width = 3; canvas.height = 3;
        const paint = canvas.getContext('2d'); paint.fillStyle = '#1d4ed8'; paint.fillRect(0, 0, 3, 3);
        const blob = await new Promise(resolve => canvas.toBlob(resolve, 'image/png'));
        await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
      });
      await composer.focus(); await page.keyboard.press('Control+v');
      await until(async () => await page.locator('.pending-shares button').count() > 0, 'Native Ctrl+V queues a clipboard image');
      assert.equal(await page.locator('.pending-shares button').count(), 1, 'One native Ctrl+V queues exactly one screenshot');
      const pastedBytes = await page.locator('.pending-image-share img').evaluate(async image => [...new Uint8Array(await (await fetch(image.src)).arrayBuffer())]);
      const nativeText = `Native keyboard screenshot ${engine}`; await composer.fill(nativeText);
      await page.getByRole('button', { name: 'Send', exact: true }).click();
      let nativeMessage;
      await until(async () => { nativeMessage = (await call(`/mx/v1/collaboration/channels/${channel.uid}/messages`, recipient)).messages.find(message => message.body === nativeText); return nativeMessage?.files.length > 0; }, 'Recipient receives the native clipboard image');
      assert.equal(nativeMessage.files.length, 1);
      const nativeDownload = await mx.api.get(`/mx/v1/collaboration/files/${nativeMessage.files[0].uid}/download`, { headers: { Authorization: `Bearer ${recipient.access_token}` } });
      assert.equal(nativeDownload.status(), 200); assert.deepEqual([...await nativeDownload.body()], pastedBytes);
      await until(async () => await page.locator('.pending-shares').count() === 0, 'Native image send clears the queue');
      const copiedText = `Ordinary Ctrl+C text ${engine}`; await composer.fill(copiedText); await composer.press('Control+a'); await composer.press('Control+c'); await composer.fill(''); await composer.press('Control+v');
      await until(async () => await composer.inputValue() === copiedText, 'Native text Ctrl+C/Ctrl+V remains unchanged');
      assert.equal(await page.locator('.pending-shares').count(), 0); await composer.fill('');
    }
    for (const extension of ['.docx', '.xlsx']) {
      const file = posted.files.find(file => file.file_name.endsWith(extension));
      const popupPromise = context.waitForEvent('page');
      await page.getByTitle(`Preview ${file.file_name}`, { exact: true }).click();
      const viewer = await popupPromise; inspected = viewer;
      await viewer.waitForURL(/message_file=/); await viewer.waitForLoadState('domcontentloaded');
      if (fallbackGraphics) {
        await viewer.getByRole('region', { name: 'Office loading progress', exact: true }).getByText('cannot provide WebGL in an Office worker', { exact: false }).waitFor();
        await viewer.getByRole('button', { name: 'Return to Collaboration', exact: true }).waitFor();
      }
      else await until(async () => { if (await viewer.locator('.notice.error').count()) throw Error(await viewer.locator('.notice.error').innerText()); return await viewer.locator('#qtcanvas').getAttribute('data-native-focused') === 'true'; }, `Real ${extension} chat Office viewer`, 180000);
      assert.ok(await viewer.getByText('Read-only attachment', { exact: true }).isVisible());
      assert.equal(await viewer.getByRole('button', { name: 'Save to MX', exact: true }).count(), 0, 'Chat copies do not masquerade as editable Drive files');
      if (!fallbackGraphics) {
        await viewer.keyboard.press('Control+s');
        await viewer.getByText('This chat attachment is read-only.', { exact: false }).waitFor();
        await viewer.getByRole('button', { name: 'Dismiss document notice', exact: true }).click();
      }
      const pendingDownload = viewer.waitForEvent('download');
      const downloadStarted = Promise.all([pendingDownload, viewer.locator('main[data-office-source="collaboration"] > header').getByRole('button', { name: 'Download original', exact: true }).click()]);
      const [download] = await downloadStarted, stream = await download.createReadStream(), chunks = []; for await (const chunk of stream) chunks.push(chunk);
      assert.deepEqual(Buffer.concat(chunks), officeFixture(extension), 'Read-only document downloads unchanged');
      await viewer.screenshot({ path: join(mx.directory, `${engine}-chat-office-${extension.slice(1)}.png`) });
      await viewer.close(); inspected = page;
    }
    // A blocked popup must keep a usable Open/Download fallback, never start conversion.
    await page.evaluate(() => { window.__realOpen = window.open; window.open = () => null; });
    const word = posted.files.find(file => file.file_name.endsWith('.docx'));
    await page.getByTitle(`Preview ${word.file_name}`, { exact: true }).click();
    const fallback = page.getByRole('dialog', { name: word.file_name, exact: true });
    await fallback.getByRole('button', { name: 'Open in MX Office', exact: true }).waitFor();
    await fallback.getByRole('button', { name: 'Open in MX Office', exact: true }).click();
    await page.locator('.composer-file-notice').filter({ hasText: 'Allow popups' }).waitFor();
    await fallback.getByRole('button', { name: 'Close preview', exact: true }).click();
    await page.evaluate(() => window.open = window.__realOpen);
    // Non-Office previews remain in chat and close when changing conversation.
    await page.getByTitle(`Preview Notes-${engine}.txt`, { exact: true }).click();
    const preview = page.getByRole('dialog', { name: `Notes-${engine}.txt`, exact: true }); await preview.locator('pre').waitFor();
    assert.equal(await preview.locator('pre').innerText(), `Notes from ${engine}`);
    await page.evaluate(uid => location.hash = `collaboration?channel=${uid}`, other.uid);
    await preview.waitFor({ state: 'hidden' });

    // A real Drive share keeps its identity/permissions, and opens without leaving chat.
    const documentBytes = officeFixture('.docx'), operation = randomUUID();
    await call('/mx/v1/drive/uploads', owner, 'POST', { operation_uid: operation, file_name: `Shared-Drive-${engine}.docx`, mime_type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', size: documentBytes.length, last_modified: 1 });
    // The isolated storage contract intentionally rejects its first part. Like
    // Drive's real client, retry only this idempotent index with identical bytes.
    let part;
    for (let attempt = 0; attempt < 3; attempt++) {
      part = await mx.api.put(`/mx/v1/drive/uploads/${operation}/parts/0`, { headers: { Authorization: `Bearer ${owner.access_token}`, 'Content-Type': 'application/octet-stream' }, data: documentBytes });
      if (part.status() !== 502) break;
    }
    assert.equal(part.status(), 200, await part.text());
    const drive = await call(`/mx/v1/drive/uploads/${operation}/finish`, owner, 'POST');
    await call(`/mx/v1/drive/items/${drive.uid}/sharing`, owner, 'POST', { user_uid: recipient.uid, role: 'viewer' });
    const receiverContext = await browser.newContext({ viewport: { width: 1440, height: 950 } });
    receiverContext.on('page', page => page.on('pageerror', error => errors.push(error.message)));
    await receiverContext.route('**/*', guard);
    await receiverContext.addInitScript(({ tokens, origin }) => { if (location.origin !== origin) return; sessionStorage.setItem('mx_access_token', tokens.access_token); sessionStorage.setItem('mx_refresh_token', tokens.refresh_token); }, { tokens: recipient, origin });
    const receiver = await receiverContext.newPage(); inspected = receiver;
    await receiver.goto(`${origin}/#collaboration?channel=${channel.uid}`);
    const directOffice = receiver.getByRole('button', { name: `Open ${drive.name} in MX Office`, exact: true }); await directOffice.waitFor();
    const sharedPopup = receiverContext.waitForEvent('page'); await directOffice.click(); const sharedEditor = await sharedPopup; inspected = sharedEditor;
    await sharedEditor.waitForURL(new RegExp(`file=${drive.uid}`));
    if (fallbackGraphics) await sharedEditor.getByText('cannot provide WebGL in an Office worker', { exact: false }).first().waitFor();
    else await until(async () => { if (await sharedEditor.locator('.alert').count()) throw Error(await sharedEditor.locator('.alert').innerText()); return await sharedEditor.locator('#qtcanvas').getAttribute('data-native-focused') === 'true'; }, 'Drive card opens the actual viewer-permitted Office file', 180000);
    assert.equal(await sharedEditor.getByRole('button', { name: 'Save to MX', exact: true }).isDisabled(), true, 'The direct action cannot bypass Drive viewer permission');
    assert.ok(receiver.url().includes('#collaboration'), 'The original conversation stays open');
    assert.equal((await call(`/mx/v1/drive/items/${drive.uid}`, owner)).revision, 1);
    await sharedEditor.close(); await receiverContext.close(); inspected = page;
    assert.deepEqual(external, []); assert.deepEqual(unsafe, []); assert.deepEqual(legacyConversions, []); assert.deepEqual(errors, []);
    if (fallbackGraphics) assert.deepEqual(engineResources, [], 'Unsupported worker graphics must be detected before downloading the large runtime');
    results.browsers[engine] = { passed: true, native_keyboard_clipboard: ['chromium', 'brave'].includes(engine), office_rendering: fallbackGraphics ? 'Known host limitation: no worker WebGL; explicit fallback and byte-exact original downloads, not rendering qualification' : 'Actual DOCX/XLSX rendering and Drive read-only viewer', checks: ['Multi-format clipboard files and image naming', 'Single paste with different item/file-list timestamps queues and sends each file once', 'Deduplication/text paste and text drag preserved', 'Conversation file drop/no navigation', 'Local rejection feedback and conversation-specific drafts', 'Real N1 upload/recipient download byte equality', 'Outsider ticket denied', 'Office source/read-only boundary and unchanged download', 'Blocked popup Open/Download fallback', 'Text preview and conversation-switch dismissal', 'Drive share opens directly with viewer permission intact', 'No internet or legacy server conversion'] };
    console.log(`${engine}: collaboration files passed; ${fallbackGraphics ? 'known graphics fallback/download verified, not rendering' : 'actual Office rendering passed'}`);
    await context.close(); await browser.close();
  }
  assert.deepEqual(mx.n1.failures, []);
  results.passed = true;
} catch (error) {
  if (inspected && !inspected.isClosed()) await inspected.screenshot({ path: join(mx.directory, 'collaboration-file-failure.png') }).catch(() => {});
  results.error = error.stack; throw error;
} finally {
  await writeFile(join(mx.directory, 'collaboration-files-results.json'), JSON.stringify(results, null, 2));
  await Promise.all(browsers.map(browser => browser.close().catch(() => {}))); await mx.close(); await new Promise(resolve => proxy.close(resolve));
}
