// Disposable MX/SQLite only. No deployment data, live N1 or desktop audio.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/tests'));
assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url))),browsers=[],results=[];
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
let inspected;
async function until(check,label,timeout=20000){const end=Date.now()+timeout;while(Date.now()<end){if(await check())return;await new Promise(resolve=>setTimeout(resolve,80));}throw new Error(`Timeout: ${label}`);}
try {
 console.log(`Record window artifacts: ${mx.directory}`);
 const password=`Test-only!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'window@test.invalid',name:'Window Test',password},[201]);
 const user=await call('/mx/v1/auth/authenticate',null,'POST',{email:'window@test.invalid',password});
 const module=(await call('/mx/v1/admin/modules',user,'POST',{name:'Cursor records',singular_name:'Entry'},[201])).module;
 const base=`/mx/v1/modules/${module.uid}/records`;
 for(const field of [{key:'name',label:'Name',field_type:'text'},{key:'amount',label:'Amount',field_type:'integer',config:{table_summary:'sum'}},{key:'notes',label:'Notes',field_type:'long_text'}])await call(`/mx/v1/admin/modules/${module.uid}/fields`,user,'POST',{table_visible:true,...field},[201]);
 const created=[];
 for(let offset=0;offset<620;offset+=8)created.push(...await Promise.all(Array.from({length:Math.min(8,620-offset)},(_,i)=>call(base,user,'POST',{operation_uid:randomUUID(),values:{name:`Entry ${String(offset+i).padStart(4,'0')}`,amount:offset+i,notes:offset+i>=610?`Full long text\n${'A complete paragraph that must remain readable. '.repeat(35)}\nFinal visible line`:`Variable notes ${'detail '.repeat((offset+i)%9)}`}},[201]))));
 // Stable keyset navigation under insertion/deletion, both directions and scope.
 const first=await call(`${base}?window=start&limit=50`,user);
 assert.equal(first.total,620);assert.equal(first.summaries.amount.value,619*620/2);
 const added=await call(base,user,'POST',{operation_uid:randomUUID(),values:{name:'New concurrent entry',amount:9000}},[201]);
 const deleted=created[560];await call(`${base}/${deleted.uid}`,user,'DELETE',undefined,[204]);
 let cursor=first.window.cursors.at(-1),seen=[...first.data.map(row=>row.uid)],requests=0;
 while(cursor){const page=await call(`${base}?window=after&metadata=false&limit=50&cursor=${encodeURIComponent(cursor)}`,user);assert.deepEqual(page.summaries,{});seen.push(...page.data.map(row=>row.uid));requests++;cursor=page.window.has_after?page.window.cursors.at(-1):null;}
 assert.equal(new Set(seen).size,seen.length);assert.ok(!seen.includes(added.uid));assert.ok(!seen.includes(deleted.uid));assert.equal(seen.length,619);
 await call(`${base}?window=after&sort_by=amount&cursor=${encodeURIComponent(first.window.cursors[0])}`,user,'GET',undefined,[400]);
 const expected=await call(`${base}?page=1&limit=256&sort_by=amount&sort_dir=asc`,user);
 const sorted=await call(`${base}?window=start&limit=256&sort_by=amount&sort_dir=asc`,user);
 assert.deepEqual(sorted.data.map(row=>row.uid),expected.data.map(row=>row.uid));
 const names=(process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',');
 for(const name of names){
  const brave=name==='brave',type=brave?chromium:({chromium,firefox,webkit})[name];
  const executablePath=brave?'/usr/bin/brave-browser':name==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':name==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
  const browser=await type.launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...((brave||name==='chromium')?{args:['--no-sandbox','--mute-audio']}: {})});browsers.push(browser);
  const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1024,height:900}});inspected=page;const errors=[],loaded=[];
  page.on('pageerror',error=>errors.push(error.message));page.on('response',async response=>{if(response.url().includes(`${base}?`)&&response.ok()){try{loaded.push(await response.json());}catch{}}});
  await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},user);
  const slowInitialPage=async route=>{if(!new URL(route.request().url()).searchParams.has('window'))await new Promise(resolve=>setTimeout(resolve,500));await route.continue().catch(()=>{});};
  await page.route(`**${base}?*`,slowInitialPage);
  await page.goto(`https://127.0.0.1:${mx.port}/#module/${module.uid}`);
  await page.getByRole('combobox',{name:'Record navigation',exact:true}).selectOption({label:'Continuous scroll'});
  const table=page.locator('.continuous-records');await table.waitFor();await until(()=>table.locator('[data-record-uid]').count().then(count=>count>5),'initial virtual window');
  const initialColumnWidths=await table.locator('thead th').evaluateAll(nodes=>nodes.map(node=>node.getBoundingClientRect().width));
  assert.ok(initialColumnWidths[3]>=352,'long text keeps a readable width even when switching before the first page loads');
  await page.unroute(`**${base}?*`,slowInitialPage);
  const initial=await table.locator('[data-record-uid]').count();assert.ok(initial<50,`Only visible rows + buffer should render (${initial})`);
  assert.equal(await page.locator('.record-window-status').count(),0,'no permanent bottom navigation bar');
  assert.ok(await page.getByRole('group',{name:'Load record windows'}).evaluate(node=>!!node.closest('.records-view-controls')),'manual navigation shares the existing toolbar');
  const longText=table.locator('.long-text-cell .cell-edit-button > span').filter({hasText:'Final visible line'}).first();await longText.waitFor();
  const fullText=await longText.evaluate(node=>{const style=getComputedStyle(node);return{height:node.clientHeight,scrollHeight:node.scrollHeight,lineHeight:parseFloat(style.lineHeight),whiteSpace:style.whiteSpace,clamp:style.webkitLineClamp};});
  assert.ok(fullText.height>fullText.lineHeight*3,'long text expands beyond three lines');
  assert.ok(fullText.scrollHeight<=fullText.height+1,'all long text fits without hidden overflow');assert.equal(fullText.whiteSpace,'pre-wrap');assert.ok(!fullText.clamp||fullText.clamp==='none');
  await page.screenshot({path:join(mx.directory,`${name}-full-long-text.png`)});
  // Scroll past very tall, fully wrapped rows; a delayed measurement must not
  // rewind a new wheel/scrollbar destination to the previously visible record.
  await page.waitForTimeout(250);
  for(const fraction of [.65,.2,.8,.35]){
    const destination=await table.evaluate((node,fraction)=>{node.scrollTop=(node.scrollHeight-node.clientHeight)*fraction;return node.scrollTop;},fraction);
    await page.waitForTimeout(250);
    const settled=await table.evaluate(node=>node.scrollTop);
    assert.ok(Math.abs(settled-destination)<4,`fast drag retains its destination: ${JSON.stringify({fraction,destination,settled})}`);
    const stable=await table.evaluate(node=>{const rect=node.getBoundingClientRect(),header=node.querySelector('thead').getBoundingClientRect().height;const row=Array.from(node.querySelectorAll('[data-record-uid]')).find(row=>{const r=row.getBoundingClientRect();return r.bottom>rect.top+header+1&&r.top<rect.bottom;});return row&&{uid:row.dataset.recordUid,y:row.getBoundingClientRect().top-rect.top};});
    assert.ok(stable,'fast drag has visible rows, not an empty virtual spacer');await page.waitForTimeout(200);
    const later=await table.locator(`[data-record-uid="${stable.uid}"]`).evaluate(node=>node.getBoundingClientRect().top-node.closest('.records-table').getBoundingClientRect().top);
    assert.ok(Math.abs(later-stable.y)<3,'variable-height rows settle without continued anchor jumps');
  }
  const rapidDestination=await table.evaluate(async node=>{for(const fraction of [.7,.15,.85,.45]){node.scrollTop=(node.scrollHeight-node.clientHeight)*fraction;await new Promise(requestAnimationFrame);}return node.scrollTop;});
  await page.waitForTimeout(250);assert.ok(Math.abs(await table.evaluate(node=>node.scrollTop)-rapidDestination)<4,'rapid consecutive drags do not restore an obsolete destination');
  const box=await table.boundingBox();await page.mouse.move(box.x+100,box.y+120);const beforeWheel=await table.evaluate(node=>node.scrollTop);await page.mouse.wheel(0,180);
  await until(()=>table.evaluate(node=>node.scrollTop).then(top=>top>beforeWheel+100),'native wheel scrolling');await page.waitForTimeout(250);
  const afterWheel=await table.evaluate(node=>node.scrollTop);assert.ok(Math.abs(afterWheel-beforeWheel-180)<4,'wheel movement is not rewound by row measurements');
  for(let step=0;step<9;step++){
    const before=loaded.length;await table.evaluate(node=>node.scrollTop=node.scrollHeight-node.clientHeight-2);
    await until(()=>loaded.length>before,'next cursor window');await page.waitForTimeout(150);
    assert.ok(await table.locator('[data-record-uid]').count()<65,'DOM remains bounded');
    const cached=Number((await page.locator('.records-result-summary small').innerText()).split(' ')[0].replaceAll(',',''));assert.ok(cached<=250,'record cache remains bounded');
  }
  assert.ok(await table.locator('[data-record-uid]').evaluateAll(rows=>rows.every(row=>row.querySelector('.record-primary-action').getAttribute('aria-label').endsWith(` ${row.querySelector('.row-index-column span').textContent}`))),'action labels match the visible browsing positions after eviction');
  await table.evaluate(node=>node.scrollLeft=Math.min(80,node.scrollWidth-node.clientWidth));
  const anchor=await table.evaluate(node=>{const top=node.getBoundingClientRect().top+node.querySelector('thead').getBoundingClientRect().height;const row=Array.from(node.querySelectorAll('[data-record-uid]')).find(row=>row.getBoundingClientRect().top>=top-1);return{uid:row.dataset.recordUid,y:row.getBoundingClientRect().top-node.getBoundingClientRect().top,x:node.scrollLeft};});
  const row=table.locator(`[data-record-uid="${anchor.uid}"]`);
  const geometry=()=>table.evaluate(node=>({top:node.scrollTop,height:node.scrollHeight,rows:Array.from(node.querySelectorAll('[data-record-uid]')).map(row=>({uid:row.dataset.recordUid,y:row.getBoundingClientRect().top-node.getBoundingClientRect().top,height:row.getBoundingClientRect().height})),saved:Object.fromEntries(Object.entries(sessionStorage).filter(([key])=>key.startsWith('mx_record_position_')))}));
  const beforeGeometry=await geometry();
  await row.getByRole('button',{name:/Edit Entry/}).click();await page.locator('.record-dialog').waitFor();await page.getByRole('button',{name:'Cancel',exact:true}).click();
  const dialogY=await row.evaluate(node=>node.getBoundingClientRect().top-node.closest('.records-table').getBoundingClientRect().top);assert.ok(Math.abs(dialogY-anchor.y)<3,`dialog preserves anchor: ${JSON.stringify({anchor,dialogY})}`);
  // Remember by UID and relative offset, not only scrollTop. Restore after module unmount.
  await page.goto(`https://127.0.0.1:${mx.port}/#drive`);await page.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();
  await page.goto(`https://127.0.0.1:${mx.port}/#module/${module.uid}`);await table.waitFor();
  await until(()=>row.isVisible(),'anchor record restored');await page.waitForTimeout(350);
  const restoredY=await row.evaluate(node=>node.getBoundingClientRect().top-node.closest('.records-table').getBoundingClientRect().top);
  await writeFile(join(mx.directory,`${name}-anchor-geometry.json`),JSON.stringify({anchor,before:beforeGeometry,after:await geometry()},null,2));
  assert.ok(Math.abs(restoredY-anchor.y)<4,`navigation restores exact anchor offset: ${JSON.stringify({browser:name,anchor,restoredY})}`);
  assert.ok(Math.abs(await table.evaluate(node=>node.scrollLeft)-anchor.x)<2,'navigation restores horizontal position');
  const beforeBack=loaded.length;await table.evaluate(node=>node.scrollTop=0);await until(()=>loaded.length>beforeBack,'backwards cursor fetch');
  let fail=true;const interrupted=async route=>{const url=new URL(route.request().url());if(fail&&url.searchParams.get('window')==='after'){await route.fulfill({status:503,json:{response:'Test connection interrupted'}});}else await route.continue();};
  await page.route(`**${base}?*`,interrupted);
  await table.evaluate(node=>node.scrollTop=node.scrollHeight-node.clientHeight-2);await page.getByRole('button',{name:'Retry',exact:true}).waitFor();
  assert.ok(await table.locator('[data-record-uid]').count()>0,'network failure does not clear records');fail=false;await page.getByRole('button',{name:'Retry',exact:true}).click();await until(()=>page.getByRole('button',{name:'Retry',exact:true}).count().then(count=>count===0),'edge retry');await page.unroute(`**${base}?*`,interrupted);
  // Without configured totals the table should end at the slim page gutter,
  // not reserve the space formerly occupied by the bottom control bar.
  await page.locator('.toolbar-menu summary').click();await page.locator('.toolbar-menu').getByLabel('Amount',{exact:true}).uncheck();await page.waitForTimeout(200);
  assert.ok(await table.evaluate(node=>innerHeight-node.getBoundingClientRect().bottom)<18,'no blank footer reservation when totals are absent');
  await page.locator('.toolbar-menu').getByLabel('Amount',{exact:true}).check();await page.locator('.toolbar-menu summary').click();
  for(const [theme,width] of [['light',1280],['dark',1024],['dark',390],['light',320]]){
    await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);await page.setViewportSize({width,height:900});await page.waitForTimeout(250);
    assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'no page-wide horizontal overflow');
    if(width>820){const bottom=await table.evaluate(node=>node.getBoundingClientRect().bottom);assert.ok(900-bottom<70,`table uses the reclaimed footer space (${900-bottom}px below)`);}
    await page.screenshot({path:join(mx.directory,`${name}-window-${theme}-${width}.png`)});
  }
  await page.setViewportSize({width:1280,height:900});await page.getByRole('combobox',{name:'Record navigation',exact:true}).selectOption({label:'Pages'});
  await until(()=>page.locator('.records-table [data-record-uid]').count().then(count=>count===50),'paginated fallback');
  await until(()=>page.locator('.records-table').getAttribute('aria-busy').then(value=>value==='false'),'paged response has committed before reading its page number');
  const currentPage=Number(await page.locator('.records-pagination input').inputValue());
  await page.getByRole('button',{name:'Next',exact:true}).click();await until(()=>page.locator('.records-pagination input').inputValue().then(value=>Number(value)===currentPage+1),'next page preserves the current browsing page');
  assert.deepEqual(errors,[]);results.push({browser:name,virtual_dom:true,bounded_cache:true,anchor_restoration:true,bidirectional:true,network_retry:true,pagination:true,compact_controls:true,full_long_text:true,fast_drag:true});
  console.log(`${name}: bounded records, anchors, reverse navigation, retry and responsive layouts passed`);await browser.close();
 }
 await writeFile(join(mx.directory,'record-window-results.json'),JSON.stringify({passed:true,records:620,cursorRequests:requests,results},null,2));
}catch(error){if(inspected&&!inspected.isClosed())await inspected.screenshot({path:join(mx.directory,'record-window-failed.png')}).catch(()=>{});throw error;}
finally{for(const browser of browsers)await browser.close();await mx.close();}
