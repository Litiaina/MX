// Failed-row geometry on production UI, disposable MX/SQLite and fixture N1.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join, isAbsolute } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';
assert.ok(isAbsolute(process.env.MX_TEST_ARTIFACTS||''));assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url))),browsers=[],results=[];
let owner,activePage;
const call=async(path,method='GET',data,status=[200])=>(await mx.call(path,owner?.access_token,method,data,status)).body;
try{
  console.log(`Import layout artifacts: ${mx.directory}`);
  const account={email:'import-layout@test.invalid',name:'Import layout',password:`Test-only!${randomUUID()}`};
  await mx.call('/mx/v1/auth/create',mx.signupKey,'POST',account,[201]);owner=(await mx.call('/mx/v1/auth/authenticate',null,'POST',account)).body;
  const module=(await call('/mx/v1/admin/modules','POST',{name:'Import layout',singular_name:'Entry'},[201])).module;
  await call(`/mx/v1/admin/modules/${module.uid}/fields`,'POST',{key:'date',label:'Date received with a long descriptive field label',field_type:'date'},[201]);
  await call(`/mx/v1/admin/modules/${module.uid}/fields`,'POST',{key:'remarks',label:'Remarks',field_type:'long_text'},[201]);
  const base=`/mx/v1/modules/${module.uid}/imports`;
  const filename='MONTH OF JANUARY 2026 — long source worksheet reference.csv';
  const text='Date,Remarks\n'+Array.from({length:25},(_,i)=>`${i===0?'invalid-date-'.repeat(25):`1.5.${31+i}`},${'Long original source notes and unbroken_reference_'.repeat(12)}\n`).join('');
  const uploaded=await mx.api.post(base,{headers:{Authorization:`Bearer ${owner.access_token}`},multipart:{job_uid:randomUUID(),delimiter:'comma',file:{name:filename,mimeType:'text/csv',buffer:Buffer.from(text)}}});assert.equal(uploaded.status(),200);
  let job=await uploaded.json();job=await call(`${base}/${job.uid}/configure`,'PUT',{sheets:[{index:0,header_row:1,first_data_row:2,columns:[{column:1,key:'date',fill_down:false,date_format:'iso'},{column:2,key:'remarks',fill_down:false,date_format:''}]}]});
  while(job.pending)job=await call(`${base}/${job.uid}/process`,'POST');assert.equal(job.failed,25);assert.equal(job.imported,0);
  for(const engine of(process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
    const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1440,height:950}});activePage=page;const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
    await page.goto(`https://127.0.0.1:${mx.port}/#module/${module.uid}`);await page.getByRole('button',{name:'Module actions',exact:true}).click();await page.getByRole('menuitem',{name:'Import history',exact:true}).click();
    const dialog=page.getByRole('dialog',{name:'Import records',exact:true});await dialog.getByText(filename,{exact:true}).click();await dialog.getByRole('heading',{name:'Completed with issues',exact:true}).waitFor();
    await page.waitForFunction(()=>document.querySelectorAll('.import-result-row').length===20);
    assert.equal(await dialog.locator('.import-result-row').count(),20);
    for(const theme of ['light','dark'])for(const width of [1440,1024,700,390,320]){
      await page.setViewportSize({width,height:950});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);
      await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
      const rows=await dialog.locator('.import-result-row').evaluateAll(nodes=>nodes.map(node=>{
        const action=node.querySelector('.import-row-action .button').getBoundingClientRect();const issues=node.querySelector('.import-row-issues').getBoundingClientRect();
        const walker=document.createTreeWalker(node.querySelector('.import-row-issues'),NodeFilter.SHOW_TEXT);let text,overlap=false;
        while(text=walker.nextNode()){if(!text.textContent.trim())continue;const closed=text.parentElement.closest('details:not([open])');if(closed&&!closed.querySelector('summary')?.contains(text))continue;const range=document.createRange();range.selectNodeContents(text);for(const rect of range.getClientRects())if(rect.left<action.right&&rect.right>action.left&&rect.top<action.bottom&&rect.bottom>action.top)overlap=true;}
        return{overlap,rowWidth:node.clientWidth,rowScroll:node.scrollWidth,actionX:action.x,actionRight:action.right,actionTop:action.top,issuesBottom:issues.bottom,issuesRight:issues.right};
      }));
      for(const row of rows){assert.equal(row.overlap,false,`${engine}/${width}: action overlaps error or source values`);assert.ok(row.rowScroll<=row.rowWidth+1,`${engine}/${width}: row overflows`);assert.ok(row.actionX>=0&&row.actionRight<=width,`${engine}/${width}: button fits`);if(width<=700)assert.ok(row.actionTop>=row.issuesBottom,`${engine}/${width}: mobile action is below issue`);else assert.ok(row.actionX>=row.issuesRight,`${engine}/${width}: separate desktop action column`);}
      assert.ok(await dialog.locator('.import-body').evaluate(node=>node.scrollWidth<=node.clientWidth+1),'No horizontal results scrolling');
      const first=dialog.locator('.import-result-row').first();await first.locator('summary').click();
      assert.ok(await first.evaluate(node=>node.scrollWidth<=node.clientWidth+1),'Expanded source details fit');await first.locator('summary').click();
      await first.scrollIntoViewIfNeeded();await page.screenshot({path:join(mx.directory,`${engine}-import-results-${theme}-${width}.png`)});
    }
    await dialog.getByRole('button',{name:'Next',exact:true}).click();await page.waitForFunction(()=>document.querySelectorAll('.import-result-row').length===5);
    await dialog.getByRole('button',{name:'Correct and retry',exact:true}).first().click();
    const correction=page.locator('.record-dialog');await correction.waitFor();assert.ok(await page.locator('.import-backdrop').evaluate(node=>node.inert),'Import results inert behind correction');
    await correction.getByRole('button',{name:'Cancel',exact:true}).click();await correction.waitFor({state:'hidden'});
    assert.deepEqual(errors,[]);results.push({engine,passed:true,views:10,long_errors:true,expanded_details:true,pagination:true,correction_opens:true});console.log(`${engine}: failed-row geometry and correction passed`);await browser.close();
  }
  await writeFile(join(mx.directory,'import-layout-results.json'),JSON.stringify({passed:true,results},null,2));
}catch(error){if(activePage&&!activePage.isClosed())await activePage.screenshot({path:join(mx.directory,'import-layout-failure.png')}).catch(()=>{});throw error;}
finally{await Promise.all(browsers.map(browser=>browser.close().catch(()=>{})));await mx.close();}
