// Real workspace UI/ACL and disposable storage; never starts the Office engine.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {officeFixture} from './office-fixtures.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
if(process.env.MX_TEST_N1_URL)throw Error('Use disposable storage only.');
if(!process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/tests'))throw Error('Keep artifacts on the external test drive.');
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url))),results=[];
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
let browser,inspected;
async function measure(page,prompt){
 return prompt.evaluate(node=>{
  const box=node.getBoundingClientRect(),content=node.closest('.preview-content').getBoundingClientRect(),icon=node.querySelector('svg').getBoundingClientRect();
  const buttons=[...node.querySelectorAll('button')].map(button=>button.getBoundingClientRect()),rows=[];
  for(const box of buttons){let row=rows.find(row=>Math.abs(row.top-box.top)<2);if(!row){row={top:box.top,left:box.left,right:box.right};rows.push(row);}else{row.left=Math.min(row.left,box.left);row.right=Math.max(row.right,box.right);}}
  return{center:(box.left+box.right)/2,contentCenter:(content.left+content.right)/2,iconCenter:(icon.left+icon.right)/2,rows:rows.map(row=>({...row,center:(row.left+row.right)/2})),fits:node.scrollWidth<=node.clientWidth+1&&box.left>=content.left-1&&box.right<=content.right+1,buttonsFit:buttons.every(button=>button.left>=box.left&&button.right<=box.right),viewportFits:document.documentElement.scrollWidth<=innerWidth+1};
 });
}
try{
 console.log(`Office prompt layout artifacts: ${mx.directory}`);
 const password=`Test-only!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{name:'Prompt Owner',email:'prompt-owner@test.invalid',password},[201]);
 const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'prompt-owner@test.invalid',password});
 await call('/mx/v1/user/create',owner,'POST',{name:'Shared Editor',email:'prompt-editor@test.invalid',password,access_level:2},[201]);
 const editor=await call('/mx/v1/auth/authenticate',null,'POST',{email:'prompt-editor@test.invalid',password});
 const bytes=officeFixture('.xlsx'),operation=randomUUID();
 await call('/mx/v1/drive/uploads',owner,'POST',{operation_uid:operation,file_name:'Shared budget.xlsx',mime_type:'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',size:bytes.length,last_modified:1});
 let part;for(let attempt=0;attempt<3;attempt++){part=await mx.api.put(`/mx/v1/drive/uploads/${operation}/parts/0`,{headers:{Authorization:`Bearer ${owner.access_token}`,'Content-Type':'application/octet-stream'},data:bytes});if(part.status()!==502)break;}assert.equal(part.status(),200);
 const item=await call(`/mx/v1/drive/uploads/${operation}/finish`,owner,'POST');
 await call(`/mx/v1/drive/items/${item.uid}/sharing`,owner,'POST',{user_uid:editor.uid,role:'editor'});
 const link=await call(`/mx/v1/drive/items/${item.uid}/links`,owner,'POST',{expires_at:null});
 const origin=`https://127.0.0.1:${mx.port}`;
 for(const name of(process.env.MX_TEST_BROWSERS||'brave,chromium,firefox,webkit').split(',')){
  const implementation=name==='brave'?chromium:({chromium,firefox,webkit})[name];
  const executablePath=({brave:'/opt/brave.com/brave/brave',chromium:'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome',firefox:'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox',webkit:'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh'})[name];
  browser=await implementation.launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(['brave','chromium'].includes(name)?{args:['--no-sandbox','--mute-audio']}: {})});
  const checked=[];
  for(const guest of [false,true]){
   const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}}),page=await context.newPage();inspected=page;
   if(!guest)await context.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},editor);
   const unexpected=[],errors=[],conversions=[];page.on('pageerror',error=>errors.push(error.message));
   await context.route('**/*',route=>{const url=new URL(route.request().url());if(url.origin!==origin){unexpected.push(url.href);return route.abort();}if(url.searchParams.get('office')==='true')conversions.push(url.pathname);if(url.pathname==='/office/index.html')return route.fulfill({contentType:'text/html',body:'<h1>Expected editor tab</h1>'});return route.continue();});
   await page.goto(guest?`${origin}/#drive-share/${link.token}`:`${origin}/#drive?item=${item.uid}`);
   if(guest)await page.getByRole('button').filter({hasText:item.name}).first().click();
   const prompt=page.locator('.preview-dialog .preview-fallback');await prompt.waitFor();
   assert.equal(await prompt.getByRole('button',{name:'Open in MX Office',exact:true}).count(),guest?0:1);
   for(const theme of ['light','dark'])for(const width of [1440,900,768,390,320]){
    await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);await page.setViewportSize({width,height:850});
    await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    const geometry=await measure(page,prompt),label=`${name}/${guest?'guest':'editor'}/${theme}/${width}`;
    assert.ok(Math.abs(geometry.center-geometry.contentCenter)<2,`${label}: prompt off center ${JSON.stringify(geometry)}`);
    assert.ok(Math.abs(geometry.center-geometry.iconCenter)<2,`${label}: icon off center`);
    assert.ok(geometry.rows.every(row=>Math.abs(row.center-geometry.center)<2),`${label}: action row off center ${JSON.stringify(geometry)}`);
    assert.ok(geometry.fits&&geometry.buttonsFit&&geometry.viewportFits,`${label}: overflow ${JSON.stringify(geometry)}`);
    checked.push({guest,theme,width,centered:true});
    if([900,320].includes(width))await page.screenshot({path:join(mx.directory,`${name}-${guest?'guest':'editor'}-${theme}-${width}.png`)});
   }
   if(!process.env.MX_TEST_PROMPT_BASELINE)assert.doesNotMatch(await prompt.innerText(),/PDF conversion|on your device|Guest links/,'Keep implementation details out of the launch prompt');
   const downloaded=page.waitForEvent('download');await prompt.getByRole('button',{name:'Download original',exact:true}).click();
   const stream=await(await downloaded).createReadStream(),chunks=[];for await(const chunk of stream)chunks.push(chunk);assert.deepEqual(Buffer.concat(chunks),bytes);
   if(!guest){
    const popup=context.waitForEvent('page');await prompt.getByRole('button',{name:'Open in MX Office',exact:true}).click();const tab=await popup;
    await tab.waitForURL(`**/office/index.html#file=${item.uid}`);assert.equal(await page.locator('.preview-dialog').count(),0);await tab.close();
   }else await page.getByRole('button',{name:'Close preview',exact:true}).click();
   assert.deepEqual(unexpected,[]);assert.deepEqual(errors,[]);assert.deepEqual(conversions,[]);await context.close();
  }
  results.push({browser:name,views:checked,download_unchanged:true,editor_tab:true,guest_download_only:true,no_pdf_conversion:true});console.log(`${name}: 20 centered editor/guest light/dark layouts, download and editor-tab actions pass`);
  await browser.close();browser=undefined;
 }
 assert.equal((await call(`/mx/v1/drive/items/${item.uid}`,owner)).revision,1);assert.deepEqual(mx.n1.failures,[]);
 await writeFile(join(mx.directory,'office-prompt-layout-results.json'),JSON.stringify({passed:true,results},null,2));
}catch(error){if(inspected&&!inspected.isClosed())await inspected.screenshot({path:join(mx.directory,'office-prompt-layout-failed.png')}).catch(()=>{});await writeFile(join(mx.directory,'office-prompt-layout-results.json'),JSON.stringify({passed:false,results,error:error.message},null,2));throw error;}
finally{await browser?.close();await mx.close();}
