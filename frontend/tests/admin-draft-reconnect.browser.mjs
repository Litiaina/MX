// A live reconnect must not remount an administrator's unsaved module draft.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
import {workflowKindNames} from '../src/lib/util/workflowTemplates.ts';

assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/PROJECT-DISK-1TB/'));
assert.ok(process.env.TMPDIR?.startsWith('/mnt/PROJECT-DISK-1TB/'));
assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const results={passed:false,browsers:{}};let browser,page;
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const until=async(check,label)=>{const end=Date.now()+20000;while(Date.now()<end){if(await check())return;await new Promise(r=>setTimeout(r,80));}throw Error(label);};
try{
  console.log(`Administrator reconnect artifacts: ${mx.directory}`);
  const password=`Disposable!${randomUUID()}`;
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@admin-reconnect.test',name:'Owner',password},[201]);
  const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@admin-reconnect.test',password});
  await call('/mx/v1/account/preferences',owner,'PUT',{preferences:{auto_refresh_seconds:10}});
  for(const engine of (process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',')){
    const module=(await call('/mx/v1/admin/modules',owner,'POST',{name:`${engine} reconnect`,singular_name:'Request'},[201])).module;
    await call(`/mx/v1/admin/modules/${module.uid}/fields`,owner,'POST',{key:'title',label:'Title',field_type:'text'},[201]);
    await call(`/mx/v1/modules/${module.uid}/records`,owner,'POST',{values:{title:'Existing request'}},[201]);
    const executablePath=engine==='brave'?'/usr/bin/brave-browser':engine==='chromium'?'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome':engine==='firefox'?'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox':'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh';
    browser=await(engine==='brave'?chromium:{chromium,firefox,webkit}[engine]).launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(['chromium','brave'].includes(engine)?{args:['--no-sandbox','--mute-audio']}:{})});
    const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1440,height:1000}});
    await context.addInitScript(tokens=>{
      sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);
      window.__adminSockets=[];const Native=window.WebSocket;window.WebSocket=class extends Native{constructor(...args){super(...args);window.__adminSockets.push(this);}};
      window.__adminAutoRefreshes=0;const interval=window.setInterval.bind(window);
      window.setInterval=(callback,delay,...args)=>interval(typeof callback==='function'&&delay===10000?((...values)=>{window.__adminAutoRefreshes++;callback(...values);}):callback,delay,...args);
    },owner);
    page=await context.newPage();await page.goto(`https://127.0.0.1:${mx.port}/#admin`);
    await until(()=>page.evaluate(()=>window.__adminSockets.some(s=>s.readyState===1)),'Initial live connection');
    await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
    await page.getByRole('button',{name:'Modules & fields',exact:true}).click();
    await page.getByRole('searchbox',{name:'Find a module',exact:true}).fill(module.name);
    await page.getByRole('button',{name:`Configure ${module.name}`,exact:true}).click();
    await page.getByRole('button',{name:'Process & actions',exact:true}).click();
    const admin=page.getByRole('region',{name:'Module workflow configuration',exact:true});
    await admin.getByRole('button',{name:workflowKindNames.crm,exact:true}).click();
    const draftName=`Kept draft ${engine}`;await admin.getByLabel('Process name',{exact:true}).fill(draftName);
    await admin.getByRole('checkbox',{name:`Enable this process for ${module.name}`,exact:true}).check();
    await admin.getByRole('checkbox',{name:/Apply to the 1 existing records/}).check();
    const tick=await page.evaluate(()=>window.__adminAutoRefreshes);
    await until(()=>page.evaluate(previous=>window.__adminAutoRefreshes>previous,tick),'Configured background refresh actually ran');
    assert.equal(await admin.count(),1,'Automatic refresh must preserve the open administrator panel');
    assert.equal(await admin.getByLabel('Process name',{exact:true}).inputValue(),draftName,'Automatic refresh must preserve unsaved process values');
    const before=await page.evaluate(()=>window.__adminSockets.length);
    await page.evaluate(()=>window.__adminSockets.forEach(s=>s.close(1000,'Forced isolated reconnect')));
    await until(()=>page.evaluate(n=>window.__adminSockets.length>n&&window.__adminSockets.at(-1).readyState===1,before),'Replacement live connection');
    await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
    assert.equal(await admin.count(),1,'Reconnect must preserve the selected module and process panel');
    assert.equal(await admin.getByLabel('Process name',{exact:true}).inputValue(),draftName,'Reconnect must preserve unsaved process values');
    assert.equal(await admin.getByRole('checkbox',{name:/Apply to the 1 existing records/}).isChecked(),true,'Reconnect must preserve explicit confirmation');
    await admin.getByRole('button',{name:'Save process',exact:true}).click();
    await until(async()=>(await call(`/mx/v1/modules/${module.uid}/workflow`,owner)).definition.name===draftName,'Preserved draft saves through the normal revision checks');
    await page.getByRole('button',{name:'Refresh current workspace',exact:true}).click();
    await page.getByRole('heading',{name:'Choose a module to configure',exact:true}).waitFor();
    results.browsers[engine]={automaticRefreshPreserved:true,draftPreserved:true,confirmationPreserved:true,saved:true,explicitRefreshWorks:true};console.log(`${engine}: timer/reconnect preserve the administrator draft; save and explicit Refresh work`);
    await browser.close();browser=null;
  }
  results.passed=true;
}catch(error){results.error=error.stack;if(page)await page.screenshot({path:join(mx.directory,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{await writeFile(join(mx.directory,'admin-draft-reconnect-results.json'),JSON.stringify(results,null,2));if(browser)await browser.close();await mx.close();}
