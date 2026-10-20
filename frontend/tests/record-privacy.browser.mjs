// Access changes must purge open forms and both paged/continuous view caches.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {chromium,firefox,webkit} from 'playwright';
import {startIsolatedMx} from './isolated-mx.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/home/altear/Temp Environment/')||process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/'));
assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));
const browsers=[],results={browsers:{},passed:false};let active;
const call=async(path,user,method='GET',data,expected=[200])=>(await mx.call(path,user?.access_token,method,data,expected)).body;
const until=async(check,label)=>{const end=Date.now()+20000;while(Date.now()<end){if(await check())return;await new Promise(r=>setTimeout(r,60));}throw Error(label);};
try{
 console.log(`Record privacy artifacts: ${mx.directory}`);
 const password=`Disposable!${randomUUID()}`;
 await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@privacy-ui.test',name:'Owner',password},[201]);
 const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@privacy-ui.test',password});
 await call('/mx/v1/user/create',owner,'POST',{email:'editor@privacy-ui.test',name:'Editor',password,access_level:2},[201]);
 const editor=await call('/mx/v1/auth/authenticate',null,'POST',{email:'editor@privacy-ui.test',password});
 const module=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Private cases',singular_name:'Case'},[201])).module;
 const name=await call(`/mx/v1/admin/modules/${module.uid}/fields`,owner,'POST',{key:'name',label:'Name',field_type:'text'},[201]);
 const secret=await call(`/mx/v1/admin/modules/${module.uid}/fields`,owner,'POST',{key:'secret',label:'Private result',field_type:'text'},[201]);
 const record=await call(`/mx/v1/modules/${module.uid}/records`,owner,'POST',{values:{name:'Public name',secret:'PRIVATE-RESULT-8923'}},[201]);
 const grant=await call(`/mx/v1/admin/accounts/${editor.uid}/modules`,owner);await call(`/mx/v1/admin/accounts/${editor.uid}/modules`,owner,'PUT',{revision:grant.revision,grants:[{module_uid:module.uid,can_read:true,can_create:true,can_update:true,can_report:true,can_attachments:true,can_configure:false,can_delete:false}]});
 for(const[engine,launcher,executablePath]of[
  ['chromium',chromium,'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome'],
  ['firefox',firefox,'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox'],
  ['webkit',webkit,'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh']]){
  await call(`/mx/v1/admin/modules/${module.uid}`,owner,'PUT',{config:{record_visibility:'module'}});
  await call(`/mx/v1/admin/schema/fields/${secret.uid}`,owner,'PUT',{config:{access_scope:'module'}});
  const browser=await launcher.launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
  const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1440,height:950}});active=page;const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},editor);
  await page.goto(`https://127.0.0.1:${mx.port}/#module/${module.uid}`);
  const row=page.locator(`[data-record-uid="${record.uid}"]`);await row.waitFor();
  await row.getByRole('button',{name:'Edit Case 1',exact:true}).click();
  const dialog=page.getByRole('dialog',{name:'Edit Case',exact:true});await dialog.getByLabel('Private result',{exact:true}).fill('Unsaved private draft');
  await call(`/mx/v1/admin/schema/fields/${secret.uid}`,owner,'PUT',{config:{access_scope:'administrators'}});
  await until(async()=>await dialog.getByLabel('Private result',{exact:true}).count()===0,'Open form purges newly restricted field and its dirty draft');
  assert.equal(await dialog.getByLabel('Name',{exact:true}).inputValue(),'Public name');
  assert.ok(!await page.evaluate(()=>Object.entries(sessionStorage).some(([key,value])=>key.startsWith('mx-record-draft:')&&value.includes('Unsaved private draft'))));
  await dialog.getByRole('button',{name:'Close',exact:true}).click();
  await until(async()=>(await row.locator(`[data-field-uid="${secret.uid}"]`).innerText()).includes('Restricted'),'Paged table redacts changed access');
  await page.getByRole('combobox',{name:'Record navigation',exact:true}).selectOption({label:'Continuous scroll'});await page.locator('.continuous-records').waitFor();
  await row.getByRole('button',{name:'Edit Case 1',exact:true}).click();await dialog.waitFor();
  await call(`/mx/v1/admin/modules/${module.uid}`,owner,'PUT',{config:{record_visibility:'creator'}});
  await dialog.waitFor({state:'hidden'});await until(async()=>await page.locator('[data-record-uid]').count()===0,'Continuous cache purged when record access is revoked');
  assert.ok(!(await page.locator('body').innerText()).includes('PRIVATE-RESULT-8923'));
  assert.deepEqual(errors,[]);await page.screenshot({path:join(mx.directory,`${engine}-revoked-record.png`)});results.browsers[engine]={passed:true,checks:['open dirty form purge','restricted draft purge','paged redaction','whole-record access revocation','continuous cache purge']};await browser.close();console.log(`${engine}: live privacy revocation passed`);
 }
 results.passed=true;
}catch(error){await active?.screenshot({path:join(mx.directory,'failure.png')}).catch(()=>{});throw error;}
finally{for(const browser of browsers)await browser.close();await writeFile(join(mx.directory,'record-privacy-results.json'),JSON.stringify(results,null,2));await mx.close();}
