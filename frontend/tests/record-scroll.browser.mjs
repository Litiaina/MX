// Real records/schema/auth/SQLite. Only the storage service uses a contract
// fixture; no deployment data or audio devices are accessed.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';
import { startIsolatedMx } from './isolated-mx.mjs';

const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url))),browsers=[],results=[];
// UI-only: no capture/playback. Disconnect browser processes from desktop audio
// servers and use ALSA's null fallback, without creating devices or changing the
// workstation's defaults. The unique test directory has no Pulse socket.
const silentEnvironment={...process.env,PULSE_SERVER:`unix:${join(mx.directory,'no-audio.sock')}`,PIPEWIRE_REMOTE:`mx_record_scroll_${randomUUID()}`,ALSA_CONFIG_PATH:fileURLToPath(new URL('./alsa-null.conf',import.meta.url)),SDL_AUDIODRIVER:'dummy',ALSOFT_DRIVERS:'null'};
let inspectedPage;
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
async function until(check,label){const end=Date.now()+15000;while(Date.now()<end){if(await check())return;await new Promise(resolve=>setTimeout(resolve,100));}throw Error(`Timeout: ${label}`);}
try{
  console.log(`Record scrolling artifacts: ${mx.directory}`);
  const password=`Test-only!${randomUUID()}`;
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'scroll@test.invalid',name:'Scroll Test',password},[201]);
  const admin=await call('/mx/v1/auth/authenticate',null,'POST',{email:'scroll@test.invalid',password});
  const module=(await call('/mx/v1/admin/modules',admin,'POST',{name:'Wide registrations',singular_name:'Registration'},[201])).module;
  const fields=[];
  for(let i=0;i<14;i++)fields.push(await call(`/mx/v1/admin/modules/${module.uid}/fields`,admin,'POST',{key:`column_${i}`,label:`Registration detail ${i+1}`,field_type:'text',required:false,table_visible:true,config:{}},[201]));
  for(let offset=0;offset<60;offset+=8)await Promise.all(Array.from({length:Math.min(8,60-offset)},(_,i)=>call(`/mx/v1/modules/${module.uid}/records`,admin,'POST',{operation_uid:randomUUID(),values:Object.fromEntries(fields.map((field,index)=>[field.key,`Registration ${offset+i+1} / detail ${index+1}`]))},[201])));
  for(const name of (process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
    const executablePath=name==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':name==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    const browser=await ({chromium,firefox,webkit})[name].launch({headless:true,executablePath,env:silentEnvironment,...(name==='chromium'?{args:['--mute-audio']}:{})});browsers.push(browser);
    const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1440,height:900}});inspectedPage=page;
    const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},admin);
    await page.goto(`https://127.0.0.1:${mx.port}/#module/${module.uid}`);
    const table=page.locator('.records-table'),bar=page.getByRole('scrollbar',{name:'Scroll record columns',exact:true,includeHidden:true});
    await until(()=>table.locator('tbody tr.clickable-row').count().then(count=>count===50),'real 50-record page loaded');
    await until(()=>bar.isVisible(),'wide table scrollbar reachable at top');
    assert.ok(await table.evaluate(node=>node.scrollWidth>node.clientWidth));
    assert.equal(await bar.getAttribute('aria-controls'),`records-table-${module.uid}`);
    await bar.evaluate(node=>node.scrollLeft=500);await until(()=>table.evaluate(node=>Math.abs(node.scrollLeft-500)<2),'scrollbar moves table');
    await table.evaluate(node=>node.scrollLeft=900);await until(()=>bar.evaluate(node=>Math.abs(node.scrollLeft-900)<2),'table moves scrollbar');
    await page.evaluate(()=>window.scrollTo(0,650));
    await until(()=>bar.isVisible(),'control stays reachable halfway through rows');
    const geometry=await bar.boundingBox();assert.ok(geometry.y>=880&&geometry.y+geometry.height<=900.1);
    assert.ok((await table.boundingBox()).y<0,'The table top has already scrolled out of view');
    await bar.focus();await page.keyboard.press('Home');await until(()=>table.evaluate(node=>node.scrollLeft===0),'keyboard Home');
    await page.keyboard.press('ArrowRight');await until(()=>table.evaluate(node=>Math.abs(node.scrollLeft-64)<2),'keyboard ArrowRight');
    await page.keyboard.press('End');await until(()=>table.evaluate(node=>Math.abs(node.scrollLeft-(node.scrollWidth-node.clientWidth))<2),'keyboard End reaches last column');
    await page.keyboard.press('Home');
    // Exercise the always-visible scrollbar thumb with an actual pointer drag.
    const drag=await bar.boundingBox();await page.mouse.move(drag.x+30,drag.y+drag.height-3);await page.mouse.down();await page.mouse.move(drag.x+drag.width*.6,drag.y+drag.height-3,{steps:10});await page.mouse.up();
    await until(()=>table.evaluate(node=>node.scrollLeft>100),'thumb drag moves record columns');
    for(const [theme,width] of [['light',1440],['dark',1024]]){
      await page.evaluate(value=>document.documentElement.dataset.theme=value,theme);await page.setViewportSize({width,height:900});
      await until(async()=>{const box=await bar.boundingBox(),recordBox=await table.boundingBox();return box&&Math.abs(box.x-recordBox.x-1)<2&&Math.abs(box.width-tableViewportWidth(recordBox))<2;},'scrollbar tracks table width after resize');
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
      await page.screenshot({path:join(mx.directory,`${name}-record-scroll-${theme}-${width}.png`)});
    }
    await page.evaluate(()=>window.scrollTo(0,document.documentElement.scrollHeight));
    await until(()=>bar.isHidden(),'native bottom scrollbar replaces floating duplicate');
    await table.evaluate(node=>node.scrollLeft=400);
    await page.evaluate(()=>window.scrollTo(0,500));await until(()=>bar.isVisible(),'floating scrollbar returns for middle rows');
    await until(()=>bar.evaluate(node=>Math.abs(node.scrollLeft-400)<2),'offset survives hiding/showing secondary scrollbar');
    // Exercise clipping inside a nested scrolling workspace without replacing
    // the production component or its real table/record responses.
    await page.evaluate(()=>{
      window.scrollTo(0,0);const section=document.querySelector('.records-page'),holder=document.createElement('div');
      holder.id='nested-record-scroll-test';holder.style.cssText='height:450px;overflow:auto';section.before(holder);holder.append(section);window.dispatchEvent(new Event('resize'));
    });
    await until(async()=>{const box=await bar.boundingBox();return box&&await page.locator('#nested-record-scroll-test').evaluate((holder,bottom)=>Math.abs(holder.getBoundingClientRect().top+holder.clientHeight-bottom)<2,box.y+box.height);},'scrollbar stays inside nested clipping viewport');
    await page.locator('#nested-record-scroll-test').evaluate(holder=>holder.scrollTop=350);
    await bar.focus();await page.keyboard.press('Home');await page.keyboard.press('ArrowRight');
    await until(()=>table.evaluate(node=>Math.abs(node.scrollLeft-64)<2),'nested scrollbar keyboard sync');
    await page.evaluate(()=>{const holder=document.getElementById('nested-record-scroll-test'),section=holder.querySelector('.records-page');holder.replaceWith(section);window.dispatchEvent(new Event('resize'));});
    await page.evaluate(()=>window.scrollTo(0,0));await table.evaluate(node=>node.scrollLeft=0);
    await page.getByRole('button',{name:'Edit Registration 1',exact:true}).click();
    await until(()=>page.locator('.record-dialog').isVisible(),'real edit dialog opened');
    assert.equal(await bar.isHidden(),true,'Do not overlay a record editor dialog');
    await page.getByRole('button',{name:'Cancel',exact:true}).click();await until(()=>bar.isVisible(),'record table scrolling restored after closing editor');
    await page.setViewportSize({width:390,height:900});await until(()=>bar.isHidden(),'mobile cards do not need a scrollbar');
    assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
    await page.screenshot({path:join(mx.directory,`${name}-record-scroll-mobile.png`)});
    await page.setViewportSize({width:1440,height:900});
    await page.getByText('Columns',{exact:true}).click();
    for(const field of fields.slice(1))await page.locator('.toolbar-popover').getByRole('checkbox',{name:field.label,exact:true}).uncheck();
    await until(()=>table.evaluate(node=>node.scrollWidth<=node.clientWidth+1),'column visibility removes overflow');
    assert.equal(await bar.isHidden(),true,'Fitting tables retain their existing appearance');
    await page.getByText('Columns',{exact:true}).click();
    await page.getByRole('button',{name:'Next',exact:true}).click();await until(()=>table.locator('tbody tr.clickable-row').count().then(count=>count===10),'pagination still loads the next bounded page');
    await page.goto(`https://127.0.0.1:${mx.port}/#drive`);await page.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();
    assert.equal(await page.locator('.table-scrollbar').count(),0,'Control and observers are removed outside records');
    assert.deepEqual(errors,[]);
    results.push({browser:name,passed:true,scope:'real records UI: bidirectional sync, vertical reachability, pointer/keyboard, resize/themes, native-bottom handoff, hide/show offset, nested clipping, dialogs, mobile, column visibility, pagination and navigation cleanup'});
    console.log(`${name}: reachable horizontal record scrolling passed`);await browser.close();
  }
  await writeFile(join(mx.directory,'record-scroll-results.json'),JSON.stringify({passed:true,results,n1:'HTTP contract fixture; not live N1'},null,2));
}catch(error){if(inspectedPage&&!inspectedPage.isClosed())await inspectedPage.screenshot({path:join(mx.directory,'record-scroll-failure.png')}).catch(()=>{});throw error;}
finally{for(const browser of browsers)await browser.close();await mx.close();}

function tableViewportWidth(rect){return rect.width-2;}
