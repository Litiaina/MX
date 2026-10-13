// Assert component layout, not just input metrics: the withdrawn polish passed
// input tests while breaking Drive's grid via CSS layer precedence.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';
import { disconnectedBrowserAudio } from './silent-audio.mjs';
assert.ok(!process.env.MX_TEST_N1_URL,'Use disposable storage only');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const browsers=[],results=[];let activePage;
const call=async(path,token,method='GET',data,expected=[200])=>(await mx.call(path,token,method,data,expected)).body;
const settle=page=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
try {
  console.log(`Cascade regression artifacts: ${mx.directory}`);
  const password=`Test-only!${randomUUID()}`;
  await call('/mx/v1/auth/create',mx.signupKey,'POST',{email:'cascade@test.invalid',name:'Cascade test',password},[201]);
  const user=await call('/mx/v1/auth/authenticate',null,'POST',{email:'cascade@test.invalid',password});
  for(const name of ['References','A second folder with a longer name','Documents'])await call('/mx/v1/drive/folders',user.access_token,'POST',{operation_uid:randomUUID(),name});
  for(const engine of(process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')) {
    const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1440,height:950}});activePage=page;const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},user);
    const origin=`https://127.0.0.1:${mx.port}`;await page.goto(`${origin}/#drive`);await page.locator('.file-row').first().waitFor();await settle(page);
    for(const theme of ['light','dark'])for(const width of [1440,1024,390,320]) {
      await page.setViewportSize({width,height:950});await page.evaluate(theme=>{document.documentElement.dataset.theme=theme;document.documentElement.dataset.reducedMotion='true';},theme);await settle(page);
      const geometry=await page.locator('.files .file-row').evaluateAll(nodes=>nodes.map(node=>{
        const style=getComputedStyle(node),children=[...node.children].filter(n=>getComputedStyle(n).display!=='none').map(n=>({class:n.className,x:n.getBoundingClientRect().x,right:n.getBoundingClientRect().right}));
        return {display:style.display,columns:style.gridTemplateColumns.split(' ').length,children,width:node.getBoundingClientRect().width};
      }));
      for(const row of geometry){assert.equal(row.display,'grid',`${engine}/${width}: Drive list must remain a grid, not the shared attachment flex row`);assert.equal(row.columns,width>1200?5:width>760?4:3);assert.ok(row.children.every(n=>n.right<=width+1),`${engine}/${width}: columns fit`);}
      if(width>760){const edges=geometry.map(r=>r.children.map(c=>c.x));for(const row of edges)row.forEach((x,i)=>assert.ok(Math.abs(x-edges[0][i])<1,'File columns line up across rows'));}
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'No page overflow');
      await page.screenshot({path:join(mx.directory,`${engine}-drive-${theme}-${width}.png`)});
    }
    await page.setViewportSize({width:1440,height:950});await page.getByRole('button',{name:'More actions for References',exact:true}).click();await page.getByRole('menuitem',{name:'Share',exact:true}).click();const share=page.getByRole('dialog',{name:'Share file or folder'});await share.waitFor();
    assert.ok((await share.boundingBox()).width>=500,'Share dialog keeps its intended width');await page.getByRole('button',{name:'Close dialog',exact:true}).click();
    await page.goto(`${origin}/#admin`);await page.getByRole('button',{name:'Deployment',exact:true}).click();await page.getByLabel('Display name',{exact:true}).waitFor();
    for(const theme of ['light','dark'])for(const width of [1440,1024,390,320]) {
      await page.setViewportSize({width,height:950});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);await settle(page);
      const layout=await page.locator('.settings-grid').evaluate(node=>{
        const branding=node.querySelector('fieldset'),terminology=branding.nextElementSibling;
        const a=branding.getBoundingClientRect(),b=terminology.getBoundingClientRect();
        const fields=[...node.querySelectorAll("input:not([type='checkbox']),select")].map(n=>({height:n.getBoundingClientRect().height,x:n.getBoundingClientRect().x,right:n.getBoundingClientRect().right}));
        return {gap:b.top-a.bottom,x:a.x,termX:b.x,fields};
      });
      assert.ok(layout.gap>=12&&layout.gap<=24,`Branding → Terminology has one normal gap, not an Appearance-sized hole: ${JSON.stringify(layout)}`);assert.ok(Math.abs(layout.x-layout.termX)<1,'Branding and Terminology share the same column');
      assert.ok(layout.fields.every(f=>Math.abs(f.height-layout.fields[0].height)<1&&f.x>=0&&f.right<=width+1),'Deployment controls have equal heights and stay inside viewport');
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'Deployment does not overflow');await page.screenshot({path:join(mx.directory,`${engine}-deployment-${theme}-${width}.png`)});
    }
    assert.deepEqual(errors,[]);results.push({engine,passed:true});console.log(`${engine}: Drive row grids/columns and Deployment gap regression passed`);await browser.close();
  }
  await writeFile(join(mx.directory,'ui-cascade-results.json'),JSON.stringify({passed:true,results},null,2));
}catch(error){if(activePage&&!activePage.isClosed())await activePage.screenshot({path:join(mx.directory,'cascade-failure.png')}).catch(()=>{});throw error;}
finally{await Promise.all(browsers.map(b=>b.close().catch(()=>{})));await mx.close();}
