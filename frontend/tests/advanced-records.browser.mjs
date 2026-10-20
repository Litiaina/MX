// Configure advanced features through the production administrator UI.
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
const until=async(check,label)=>{const end=Date.now()+20000;while(Date.now()<end){if(await check())return;await new Promise(r=>setTimeout(r,70));}throw Error(label);};
try {
  console.log(`Advanced UI artifacts: ${mx.directory}`);
  const password=`Disposable!${randomUUID()}`;
  await call('/mx/v1/auth/create',{access_token:mx.signupKey},'POST',{email:'owner@advanced-ui.test',name:'Owner',password},[201]);
  const owner=await call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@advanced-ui.test',password});
  const employees=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Employees',singular_name:'Employee'},[201])).module;
  const requests=(await call('/mx/v1/admin/modules',owner,'POST',{name:'Requests',singular_name:'Request'},[201])).module;
  const field=(module,key,type,config={})=>call(`/mx/v1/admin/modules/${module.uid}/fields`,owner,'POST',{key,label:key,field_type:type,config},[201]);
  for(const key of ['first','middle','last'])await field(employees,key,'text');
  await field(employees,'allowance','decimal');
  const secret=await field(employees,'secret','text');
  const employee=await call(`/mx/v1/modules/${employees.uid}/records`,owner,'POST',{values:{first:'José',last:'Dela Cruz',allowance:15,secret:'Private result'}},[201]);
  const relation=await field(requests,'employee','relationship',{target_module_uid:employees.uid,label_field:'first',multiple:false});
  await field(requests,'employee_name','text');
  const kind=await field(requests,'kind','select',{options:['SL'],option_details:[{value:'SL',number:5}]});
  await field(requests,'units','decimal');await field(requests,'charge','formula',{expression:'kind*units',decimals:2});
  await field(requests,'status','select',{options:['Pending','Approved']});
  const request=await call(`/mx/v1/modules/${requests.uid}/records`,owner,'POST',{values:{employee:employee.uid,kind:'SL',units:1,status:'Approved'}},[201]);
  for(const [engine,launcher,executablePath]of[
    ['chromium',chromium,'/home/altear/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome'],
    ['firefox',firefox,'/home/altear/.cache/ms-playwright/firefox-1543/firefox/firefox'],
    ['webkit',webkit,'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh']].filter(([name])=>(process.env.MX_TEST_BROWSERS||'chromium,firefox,webkit').split(',').includes(name))){
    const browser=await launcher.launch({headless:true,executablePath,env:disconnectedBrowserAudio(mx.directory),...(engine==='chromium'?{args:['--no-sandbox','--mute-audio']}:{})});browsers.push(browser);
    const page=await browser.newPage({ignoreHTTPSErrors:true,viewport:{width:1440,height:1000}});active=page;
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.addInitScript(tokens=>{sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},owner);
    await page.goto(`https://127.0.0.1:${mx.port}/#admin`);await page.getByRole('button',{name:'Modules & fields',exact:true}).click();
    await page.getByRole('button',{name:'Configure Employees',exact:true}).click();
    const form=page.getByRole('form',{name:'Add field',exact:true});
    const full=`full_${engine}`,used=`used_${engine}`,remaining=`remaining_${engine}`;
    await form.getByLabel('Label',{exact:true}).fill(`Full name ${engine}`);await form.getByLabel('Field key (optional)',{exact:true}).fill(full);
    await form.getByRole('combobox',{name:'Type',exact:true}).selectOption('formula');await form.getByLabel('Formula result type',{exact:true}).selectOption('text');
    for(const [i,key]of['first','middle','last'].entries()){await form.getByRole('button',{name:'Add text part',exact:true}).click();await form.getByLabel(`Combined text field ${i+1}`,{exact:true}).selectOption(key);}
    await form.getByRole('button',{name:'Add field to Employees',exact:true}).click();
    await until(async()=>(await call(`/mx/v1/modules/${employees.uid}/records/${employee.uid}`,owner)).values[full]==='José Dela Cruz','Combined name saved and existing record recalculated');
    await page.getByRole('button',{name:'All modules',exact:true}).click();await page.getByRole('button',{name:'Configure Requests',exact:true}).click();
    const relationCard=page.locator(`[data-schema-field="${relation.uid}"]`);
    await until(async()=>await relationCard.getByRole('combobox',{name:'Display field',exact:true}).locator('option').count()>3,'Linked field choices loaded');
    await relationCard.getByRole('combobox',{name:'Display field',exact:true}).selectOption(full);
    // Reset mappings from the previous engine before exercising the builder.
    const remove=relationCard.getByRole('button',{name:'Remove auto-fill mapping 1',exact:true});while(await remove.count())await remove.click();
    await relationCard.getByRole('button',{name:'Add field mapping',exact:true}).click();
    await relationCard.getByLabel('Linked source field 1',{exact:true}).selectOption(full);await relationCard.getByLabel('Fill this field 1',{exact:true}).selectOption('employee_name');
    await relationCard.getByRole('button',{name:'Save field',exact:true}).click();
    await until(async()=>(await call(`/mx/v1/relationships/${relation.uid}/preview`,owner,'POST',{value:employee.uid})).values.employee_name==='José Dela Cruz','Combined text works as auto-fill source');
    await page.getByRole('navigation',{name:'Module configuration sections',exact:true}).getByRole('button',{name:'Form rules',exact:true}).click();
    await page.getByRole('combobox',{name:'Field to configure',exact:true}).selectOption(kind.uid);await page.getByRole('button',{name:'Configure form rules',exact:true}).click();
    const rules=page.getByRole('dialog',{name:'Form rules · kind',exact:true});await rules.locator('.option-rule summary').click();
    await rules.getByLabel('Numeric value for SL',{exact:true}).fill('5');await rules.getByLabel('Display label',{exact:true}).fill('Sick leave');
    await rules.getByRole('button',{name:'Save form rules',exact:true}).click();await rules.waitFor({state:'hidden'});
    await page.getByRole('button',{name:'All modules',exact:true}).click();await page.getByRole('button',{name:'Configure Employees',exact:true}).click();
    await form.getByLabel('Label',{exact:true}).fill(`Used ${engine}`);await form.getByLabel('Field key (optional)',{exact:true}).fill(used);await form.getByRole('combobox',{name:'Type',exact:true}).selectOption('rollup');
    await form.getByLabel('Aggregate direction',{exact:true}).selectOption('incoming');await form.getByLabel('Aggregate source module',{exact:true}).selectOption(requests.uid);
    await until(async()=>await form.getByLabel('Incoming relationship',{exact:true}).locator('option').count()>1,'Incoming relationship choices');
    await form.getByLabel('Incoming relationship',{exact:true}).selectOption('employee');await form.getByLabel('Incoming numeric field',{exact:true}).selectOption('charge');
    await form.getByLabel('Only include records matching conditions',{exact:true}).check();await form.getByRole('button',{name:'Add condition',exact:true}).click();
    await form.getByRole('combobox',{name:'Control field',exact:true}).selectOption('status');await form.getByRole('combobox',{name:'Value',exact:true}).selectOption('Approved');
    await form.getByRole('button',{name:'Add field to Employees',exact:true}).click();
    await until(async()=>(await call(`/mx/v1/modules/${employees.uid}/records/${employee.uid}`,owner)).values[used]===5,'Filtered incoming total created from UI');
    await until(async()=>!await form.getByRole('button',{name:'Add field to Employees',exact:true}).isDisabled()&&(await form.getByLabel('Label',{exact:true}).inputValue())==='','Field creation and administrator refresh finished');
    await form.getByLabel('Label',{exact:true}).fill(`Remaining ${engine}`);await form.getByLabel('Field key (optional)',{exact:true}).fill(remaining);await form.getByRole('combobox',{name:'Type',exact:true}).selectOption('formula');
    await form.getByLabel('Formula expression',{exact:true}).fill(`allowance - ${used}`);assert.ok((await form.getByLabel('Available field keys',{exact:true}).innerText()).includes(used));
    await form.getByRole('button',{name:'Add field to Employees',exact:true}).click();
    await until(async()=>(await call(`/mx/v1/modules/${employees.uid}/records/${employee.uid}`,owner)).values[remaining]===10,'Remaining balance configured without code');
    await until(async()=>!await form.getByRole('button',{name:'Add field to Employees',exact:true}).isDisabled()&&(await form.getByLabel('Label',{exact:true}).inputValue())==='','Formula creation and administrator refresh finished');
    const card=page.locator(`[data-schema-field="${secret.uid}"]`);await card.getByLabel('Field access for secret',{exact:true}).selectOption('administrators');await card.getByRole('button',{name:'Save field',exact:true}).click();
    await until(async()=>(await call(`/mx/v1/modules/${employees.uid}/schema`,owner)).fields.find(f=>f.uid===secret.uid).config.access_scope==='administrators','Field access saved');
    for(const [theme,width,font]of[['light',1440,16],['dark',1440,32],['dark',390,32]]){await page.setViewportSize({width,height:1000});await page.evaluate(({theme,font})=>{document.documentElement.dataset.theme=theme;document.documentElement.style.fontSize=`${font/16*100}%`;},{theme,font});await page.evaluate(()=>{if(document.activeElement instanceof HTMLElement)document.activeElement.blur();scrollTo(0,0);});await page.waitForTimeout(350);if(!await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1))console.error(await page.evaluate(()=>({width:innerWidth,scroll:document.documentElement.scrollWidth,overflow:[...document.querySelectorAll('body *')].filter(n=>n.getBoundingClientRect().right>innerWidth+1).slice(0,20).map(n=>({tag:n.tagName,class:n.className,text:n.textContent.slice(0,80),right:n.getBoundingClientRect().right,width:n.getBoundingClientRect().width}))})));assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),`${engine}: no document overflow at ${width}/${font}`);await page.screenshot({path:join(mx.directory,`${engine}-advanced-${theme}-${width}-${font}.png`)});}
    assert.deepEqual(errors,[]);results.browsers[engine]={passed:true,checks:['text builder and Unicode name','combined-text auto-fill','numeric choices','incoming filter builder','live balance','field access','light/dark/mobile/200% text']};await browser.close();console.log(`${engine}: advanced administrator UI passed`);
  }
  assert.equal((await call(`/mx/v1/modules/${requests.uid}/records/${request.uid}`,owner)).values.kind,'SL');results.passed=true;
}catch(error){await active?.screenshot({path:join(mx.directory,'failure.png')}).catch(()=>{});if(active&&!active.isClosed())console.error(await active.evaluate(()=>({scrollX,scrollY,width:innerWidth,scroll:document.documentElement.scrollWidth,body:document.body.getBoundingClientRect().toJSON(),overflow:[...document.querySelectorAll('body *')].filter(n=>n.scrollWidth>n.clientWidth+4).slice(0,30).map(n=>({tag:n.tagName,class:n.className,text:n.textContent.slice(0,50),right:n.getBoundingClientRect().right,client:n.clientWidth,scroll:n.scrollWidth,overflow:getComputedStyle(n).overflowX}))})));throw error;}
finally{for(const browser of browsers)await browser.close();await writeFile(join(mx.directory,'advanced-ui-results.json'),JSON.stringify(results,null,2));await mx.close();}
