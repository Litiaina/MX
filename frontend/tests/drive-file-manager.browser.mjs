// Focused production-UI regression suite. Live N1 is optional, as in drive.browser.mjs.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {isolatedBrowserAudio} from './silent-audio.mjs';
import {verifyFileManager} from './drive-file-manager.mjs';
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const browsers=[];const results=[];const failures=[];const base='/mx/v1/drive';
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
try {
  console.log(`File-manager artifacts: ${mx.directory}`);const password=`Test-only!${randomUUID()}`;
  const created=await mx.api.post('/mx/v1/auth/create',{headers:{Authorization:`Bearer ${mx.signupKey}`},data:{email:'manager@test.invalid',name:'Manager',password}});assert.equal(created.status(),201);
  const user=await call('/mx/v1/auth/authenticate',null,'POST',{email:'manager@test.invalid',password});
  const environment=isolatedBrowserAudio();
  for(const engine of(process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')) {
    const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:environment,...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}});const page=await context.newPage();const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},user);
    await call(`${base}/folders`,user,'POST',{operation_uid:randomUUID(),name:`UI ${engine}`});await call(`${base}/folders`,user,'POST',{operation_uid:randomUUID(),name:`References-${engine}`});
    await page.goto(`https://127.0.0.1:${mx.port}/#drive`);await page.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();
    const image=Buffer.from(await page.evaluate(()=>{const canvas=document.createElement('canvas');canvas.width=64;canvas.height=48;const c=canvas.getContext('2d');c.fillStyle='#3768a6';c.fillRect(0,0,64,48);return canvas.toDataURL('image/png').split(',')[1];}),'base64');
    await page.locator('.drive input[type=file]:not([webkitdirectory])').first().setInputFiles([{name:`preview-${engine}.png`,mimeType:'image/png',buffer:image},{name:`notes-${engine}.txt`,mimeType:'text/plain',buffer:Buffer.from('MX Drive preview text')}]);
    await page.getByRole('button',{name:`Download notes-${engine}.txt`,exact:true}).waitFor();
    await verifyFileManager({page,engine,call,base,user,directory:mx.directory});
    assert.deepEqual(errors,[]);results.push({engine,passed:true});console.log(JSON.stringify(results.at(-1)));await context.close();await browser.close();
  }
}catch(error){failures.push(error.stack);process.exitCode=1;for(const browser of browsers)for(const context of browser.contexts())for(const page of context.pages())await page.screenshot({path:join(mx.directory,`failure-${browsers.indexOf(browser)}.png`),fullPage:true}).catch(()=>{});}
finally{for(const browser of browsers)await browser.close().catch(()=>{});await writeFile(join(mx.directory,'file-manager-results.json'),JSON.stringify({passed:!failures.length,results,failures},null,2));console.log(JSON.stringify({passed:!failures.length,results,failures,artifacts:mx.directory}));await mx.close();}
