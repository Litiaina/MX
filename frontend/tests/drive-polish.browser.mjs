// Production UI + real HTTP/ACL + isolated SQLite. Only synthetic metadata is
// seeded directly; preview bytes are uploaded through MX into fixture/live N1.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {DatabaseSync} from 'node:sqlite';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {isolatedBrowserAudio} from './silent-audio.mjs';
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const browsers=[],results=[],failures=[];const base='/mx/v1/drive';
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const auth=user=>({Authorization:`Bearer ${user.access_token}`});
const until=async(check,label)=>{const deadline=Date.now()+30000;while(Date.now()<deadline){if(await check())return;await new Promise(r=>setTimeout(r,100));}throw new Error(`Drive polish: ${label}`);};
const fileName=index=>`File ${String(index).padStart(3,'0')}.txt`;
async function upload(user){
  const operation=randomUUID(),bytes=Buffer.alloc(512*1024,65);
  await call(`${base}/uploads`,user,'POST',{operation_uid:operation,file_name:'Source.txt',mime_type:'text/plain',size:bytes.length,last_modified:1});
  for(let attempt=0;attempt<3;attempt++){const part=await mx.api.put(`${base}/uploads/${operation}/parts/0`,{headers:{...auth(user),'Content-Type':'application/octet-stream'},data:bytes});if(part.status()===502&&attempt<2)continue;assert.equal(part.status(),200,await part.text());break;}
  return call(`${base}/uploads/${operation}/finish`,user,'POST');
}
function seed(user,folder,source){
  const c=new DatabaseSync(join(mx.directory,'test.db'));c.exec('PRAGMA foreign_keys=ON;BEGIN');
  try{const v=c.prepare('SELECT * FROM mx_drive_versions WHERE item_uid=?').get(source.uid);const items=[];
    for(let index=0;index<135;index++){const uid=randomUUID(),version=randomUUID(),name=fileName(index);items.push(uid);
      c.prepare("INSERT INTO mx_drive_items(uid,owner_uid,parent_uid,name,kind,current_version_uid,created_at,updated_at) VALUES(?,?,?,?,'file',?,1,1)").run(uid,user.uid,folder.uid,name,version);
      c.prepare('INSERT INTO mx_drive_versions(uid,item_uid,object_key,file_name,mime_type,size,sha256,actor_uid,created_at) VALUES(?,?,?,?,?,?,?,?,1)').run(version,uid,v.object_key,name,v.mime_type,v.size,v.sha256,user.uid);
    }c.exec('COMMIT');return items;
  }catch(error){c.exec('ROLLBACK');throw error;}finally{c.close();}
}
async function shareMenu(page,name){await page.getByRole('button',{name:`More actions for ${name}`,exact:true}).click();await page.getByRole('menuitem',{name:'Share',exact:true}).click();await page.getByRole('dialog',{name:'Share file or folder'}).waitFor();}
async function geometry(page,mobile){
  const g=await page.locator('.access-row').first().evaluate(node=>{const identity=node.querySelector('.access-identity'),select=node.querySelector('select'),name=identity.querySelector('strong');return{identity:identity.getBoundingClientRect().width,select:select.getBoundingClientRect().width,nameHeight:name.getBoundingClientRect().height,overflow:node.scrollWidth>node.clientWidth+1};});
  assert.ok(g.identity>(mobile?70:200),JSON.stringify(g));assert.ok(g.select<150,JSON.stringify(g));assert.ok(g.nameHeight<100,JSON.stringify(g));assert.equal(g.overflow,false);
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'No horizontal viewport overflow');
}
try{
  console.log(`Drive polish artifacts: ${mx.directory}`);const password=`Test-only!${randomUUID()}`;
  const created=await mx.api.post('/mx/v1/auth/create',{headers:{Authorization:`Bearer ${mx.signupKey}`},data:{email:'owner@test.invalid',name:'Owner',password}});assert.equal(created.status(),201);
  const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@test.invalid',password});
  await call('/mx/v1/user/create',owner,'POST',{email:'glaydejhonnmendoza@example.test',name:'Glayde Jhon Mendoza',password,access_level:2},[201]);
  const member=await call('/mx/v1/auth/authenticate',null,'POST',{email:'glaydejhonnmendoza@example.test',password});
  const source=await upload(owner);const environment=isolatedBrowserAudio();
  for(const engine of(process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
    const folder=await call(`${base}/folders`,owner,'POST',{operation_uid:randomUUID(),name:`Polish ${engine}`});const ids=seed(owner,folder,source);
    const group=await call('/mx/v1/collaboration/channels',owner,'POST',{kind:'group',name:`Operations ${engine}`,member_uids:[member.uid]},[201]);
    const startRequests=mx.n1.requests.length;const pages=await Promise.all([0,50,100].map(offset=>call(`${base}?parent_uid=${folder.uid}&limit=50&offset=${offset}&sort=name_asc`,owner)));
    assert.deepEqual(pages.map(p=>p.items.length),[50,50,35]);assert.equal(new Set(pages.flatMap(p=>p.items.map(i=>i.uid))).size,135);assert.equal(mx.n1.requests.length,startRequests,'Listing pages never reads file bytes');
    const executablePath=engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await({chromium,firefox,webkit})[engine].launch({headless:true,executablePath,env:environment,...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1440,height:1000}});const page=await context.newPage();const errors=[],requests=[];page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>requests.push({url:r.url(),method:r.method(),range:r.headers().range}));
    await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
    await page.goto(`https://127.0.0.1:${mx.port}/#drive`);await page.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();
    await page.locator('.file-row').last().waitFor();const gap=await page.evaluate(()=>document.querySelector('.files').getBoundingClientRect().bottom-[...document.querySelectorAll('.file-row')].at(-1).getBoundingClientRect().bottom);assert.ok(gap<30,`Unused gap is only the 24px selection strip: ${gap}`);
    await shareMenu(page,folder.name);await page.getByRole('textbox',{name:'Find an MX account'}).fill('Glayde');await page.getByRole('button',{name:'Find',exact:true}).click();await page.getByRole('button',{name:'Share with Glayde Jhon Mendoza',exact:true}).click();
    await until(()=>page.locator('.access-row').count().then(n=>n===1),'Person grant renders');
    for(const theme of ['light','dark']){await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);await geometry(page,false);await page.screenshot({path:join(mx.directory,`${engine}-share-${theme}.png`)});}
    await page.setViewportSize({width:390,height:850});await geometry(page,true);await page.screenshot({path:join(mx.directory,`${engine}-share-mobile.png`)});await page.setViewportSize({width:1440,height:1000});
    await page.getByRole('button',{name:'Spaces & groups',exact:true}).click();await page.getByRole('textbox',{name:'Find a collaboration space'}).fill(`Operations ${engine}`);await page.getByRole('button',{name:'Find',exact:true}).click();await page.getByRole('button',{name:`Share with space Operations ${engine}`,exact:true}).click();
    await page.getByRole('combobox',{name:`Space access for Operations ${engine}`,exact:true}).waitFor();
    assert.equal((await call(`${base}/items/${ids[0]}`,member)).permission,'viewer');
    await page.getByRole('combobox',{name:`Space access for Operations ${engine}`,exact:true}).selectOption('editor');
    await until(async()=>(await call(`${base}/items/${ids[0]}`,member)).permission==='editor','Space editor is inherited');
    await page.getByRole('button',{name:'Remove access for Glayde Jhon Mendoza',exact:true}).click();await until(()=>page.locator('.access-row').count().then(n=>n===1),'User grant removed without removing group access');
    await page.getByRole('button',{name:'Close dialog',exact:true}).click();
    await page.locator('.file-name').filter({hasText:folder.name}).dblclick();await page.getByRole('combobox',{name:'Sort Drive files'}).selectOption('name_asc');
    await until(async()=>await page.locator('.file-row').count()===50&&await page.locator('.file-name').first().innerText().then(t=>t.includes('File 000')),'50 bounded rows');
    assert.equal(await page.getByRole('combobox',{name:'Drive items per page'}).inputValue(),'50','Current page size is visibly selected');
    await page.getByRole('button',{name:'Page 3',exact:true}).click();await until(async()=>await page.locator('.file-row').count()===35,'Third page renders 35 items');
    await page.getByRole('combobox',{name:'Drive items per page'}).selectOption('25');await until(async()=>await page.locator('.file-row').count()===25,'Page-size resets to first page');
    await page.getByRole('spinbutton',{name:'Go to Drive page'}).fill('6');await page.locator('.drive-pagination').getByRole('button',{name:'Go',exact:true}).click();await until(async()=>await page.locator('.file-row').count()===10,'Page jump fetches last ten items');
    await page.getByRole('combobox',{name:'Drive items per page'}).selectOption('50');await until(async()=>await page.locator('.file-row').count()===50,'Return to page one');
    const beforePreview=mx.n1.requests.length;await page.locator('.file-name').filter({hasText:'File 049.txt'}).dblclick();await page.locator('.preview-content pre').waitFor();
    await page.getByRole('button',{name:'Next file',exact:true}).click();await page.getByRole('heading',{name:'File 050.txt',exact:true}).waitFor();await page.locator('.preview-content pre').waitFor();
    assert.equal(await page.locator('.preview-content pre').textContent().then(t=>t.length),256*1024);await page.getByRole('status').filter({hasText:'Showing the first 256 KiB'}).waitFor();
    assert.ok(requests.filter(r=>r.url.includes('/drive/media/')).every(r=>r.range==='bytes=0-262143'),'Text fetches a bounded byte range');
    assert.ok(mx.n1.requests.length-beforePreview<12,'Previewing two files never fetches all 135 files');
    await until(()=>page.getByRole('button',{name:'Next file',exact:true}).isEnabled(),'Neighbor metadata ready');await page.keyboard.press('ArrowRight');await page.getByRole('heading',{name:'File 051.txt',exact:true}).waitFor();await until(()=>page.getByRole('button',{name:'Previous file',exact:true}).isEnabled(),'Previous neighbor ready');await page.keyboard.press('ArrowLeft');await page.getByRole('heading',{name:'File 050.txt',exact:true}).waitFor();await page.locator('.preview-content pre').waitFor();
    await page.screenshot({path:join(mx.directory,`${engine}-preview-cross-page.png`)});
    await page.setViewportSize({width:390,height:850});await page.getByRole('button',{name:'Next file',exact:true}).click();await page.getByRole('heading',{name:'File 051.txt',exact:true}).waitFor();await page.locator('.preview-content pre').waitFor();
    const previewBounds=await page.locator('.preview-dialog').boundingBox();assert.ok(previewBounds.x>=0&&previewBounds.x+previewBounds.width<=391,'Mobile preview fits');assert.ok(await page.getByRole('button',{name:'Close preview',exact:true}).isVisible());
    assert.ok(await page.evaluate(()=>document.querySelector('.preview-dialog').contains(document.activeElement)),'Preview takes keyboard focus');await page.keyboard.press('Tab');assert.ok(await page.evaluate(()=>document.querySelector('.preview-dialog').contains(document.activeElement)),'Keyboard focus remains in preview');
    await page.screenshot({path:join(mx.directory,`${engine}-preview-mobile.png`)});await page.getByRole('button',{name:'Close preview',exact:true}).click();await page.setViewportSize({width:1440,height:1000});
    await page.getByRole('combobox',{name:'Drive items per page'}).selectOption('25');await page.getByRole('spinbutton',{name:'Go to Drive page'}).fill('6');await page.locator('.drive-pagination').getByRole('button',{name:'Go',exact:true}).click();await until(async()=>await page.locator('.file-row').count()===10,'Last page before deletion');
    const moved=await call(`${base}/folders`,owner,'POST',{operation_uid:randomUUID(),name:`Moved ${engine}`});
    await call(`${base}/transfers`,owner,'POST',{operation_uid:randomUUID(),action:'move',parent_uid:moved.uid,items:ids.slice(125).map(uid=>({uid,base_revision:1}))});
    await page.getByRole('button',{name:'Refresh Drive',exact:true}).click();await until(async()=>await page.locator('.file-row').count()===25,'Empty page clamps to previous page after concurrent moves');
    const link=await call(`${base}/items/${folder.uid}/links`,owner,'POST',{expires_at:null});
    const memberContext=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}});const memberPage=await memberContext.newPage();
    await memberPage.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},member);await memberPage.goto(`https://127.0.0.1:${mx.port}/#drive`);await memberPage.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();
    await memberPage.locator('.drive-sidebar').getByRole('button',{name:'Shared with me',exact:true}).click();await memberPage.locator('.file-name').filter({hasText:folder.name}).dblclick();await memberPage.getByRole('combobox',{name:'Sort Drive files'}).selectOption('name_asc');await until(async()=>await memberPage.locator('.file-row').count()===50,'Group member sees shared folder with bounded pages');
    await memberPage.locator('.file-name').filter({hasText:'File 000.txt'}).dblclick();await memberPage.locator('.preview-content pre').waitFor();await memberPage.getByRole('button',{name:'Close preview',exact:true}).click();
    await call('/mx/v1/collaboration/channels/'+group.uid+'/members/'+member.uid,member,'DELETE',undefined,[204]);
    await call(`${base}/items/${ids[0]}`,member,'GET',undefined,[404]);assert.equal((await call(`${base}?view=shared`,member)).items.some(i=>i.uid===folder.uid),false);
    await memberPage.getByRole('button',{name:'Refresh Drive',exact:true}).click();await until(async()=>await memberPage.locator('.file-row').count()===0,'Revoked folder clears stale list and returns to shared root');await memberPage.getByText('This folder is no longer available. Returned to Shared with me.',{exact:true}).waitFor();await memberContext.close();
    const guest=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}});const guestPage=await guest.newPage();
    await guestPage.goto(`https://127.0.0.1:${mx.port}/#drive-share/${link.token}`);await guestPage.getByRole('heading',{name:folder.name,exact:true}).waitFor();assert.equal(await guestPage.locator('.guest-files article').count(),50);
    await guestPage.getByRole('button',{name:'Page 3',exact:true}).click();await until(async()=>await guestPage.locator('.guest-files article').count()===25,'Guest folder page three');
    await guestPage.getByRole('button',{name:'Page 1',exact:true}).click();await guestPage.locator('.guest-files .name').filter({hasText:'File 049.txt'}).click();await guestPage.getByRole('button',{name:'Next file',exact:true}).click();await guestPage.getByRole('heading',{name:'File 050.txt',exact:true}).waitFor();
    const privateGuest=await mx.api.get(`${base}/guest/${link.token}/neighbors?item_uid=${source.uid}`);assert.equal(privateGuest.status(),404,'Guest neighbors cannot escape shared root');await guest.close();
    await page.locator('.breadcrumbs').getByRole('button',{name:'My Drive',exact:true}).click();
    await page.setViewportSize({width:390,height:850});await page.waitForFunction(()=>document.querySelector('.sidebar').getBoundingClientRect().right<=0);await page.screenshot({path:join(mx.directory,`${engine}-drive-mobile.png`),fullPage:true});assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
    assert.deepEqual(errors,[]);results.push({engine,passed:true,pagination:135,preview:'cross-page + 256 KiB range',sharing:'people + space editor + leave revocation',guest:'pagination + cross-page + scoped ACL'});console.log(JSON.stringify(results.at(-1)));await context.close();await browser.close();
  }
}catch(error){failures.push(error.stack);process.exitCode=1;for(const browser of browsers)for(const context of browser.contexts())for(const page of context.pages())await page.screenshot({path:join(mx.directory,`polish-failure-${browsers.indexOf(browser)}.png`),fullPage:true}).catch(()=>{});}
finally{for(const browser of browsers)await browser.close().catch(()=>{});await writeFile(join(mx.directory,'polish-results.json'),JSON.stringify({passed:!failures.length,results,failures},null,2));console.log(JSON.stringify({passed:!failures.length,results,failures,artifacts:mx.directory}));await mx.close();}
