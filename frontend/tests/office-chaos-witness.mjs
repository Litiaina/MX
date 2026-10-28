// One real graphical editor alongside the separately counted virtual users.
// This is deliberately not advertised as 100 browser editors.
import assert from 'node:assert/strict';
import {randomUUID,createHash} from 'node:crypto';
import {createServer} from 'node:http';
import {request} from 'node:https';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {chromium} from 'playwright';
import {officeFixture,unzip} from './office-fixtures.mjs';
import {disconnectedBrowserAudio} from './silent-audio.mjs';
import {sleep} from './concurrency-fixture.mjs';

export async function officeChaosWitness({mx,owner,call,directory,seconds,signal,onReady}){
  const proxy=createServer((incoming,outgoing)=>{
    const upstream=request({hostname:'127.0.0.1',port:mx.port,path:incoming.url,method:incoming.method,headers:incoming.headers,rejectUnauthorized:false},response=>{outgoing.writeHead(response.statusCode,response.headers);response.pipe(outgoing);});
    upstream.on('error',()=>{outgoing.writeHead(502);outgoing.end();});incoming.pipe(upstream);
  });
  await new Promise(resolve=>proxy.listen(0,'127.0.0.1',resolve));
  let browser,page;
  const evidence={enabled:true,editors:1,markers:[],saveMs:[],acknowledgedVersions:[],network:[],pageErrors:[]};
  try{
    const original=officeFixture('.docx'),operation=randomUUID(),path='/mx/v1/drive/uploads';
    await call(path,owner,'POST',{operation_uid:operation,file_name:'Actual Office chaos witness.docx',mime_type:'application/vnd.openxmlformats-officedocument.wordprocessingml.document',size:original.length,last_modified:1});
    await call(`${path}/${operation}/parts/0`,owner,'PUT',original);
    const item=(await call(`${path}/${operation}/finish`,owner,'POST')).body;
    browser=await chromium.launch({executablePath:process.env.MX_LOAD_OFFICE_BROWSER,headless:true,args:['--mute-audio'],env:disconnectedBrowserAudio(directory)});
    const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:900}});
    await context.addInitScript(auth=>{sessionStorage.setItem('mx_access_token',auth.access_token);sessionStorage.setItem('mx_refresh_token',auth.refresh_token);},owner);
    page=await context.newPage();const errors=evidence.pageErrors;
    const requests=new Map();
    const tracked=request=>{const path=new URL(request.url()).pathname;return path.startsWith('/mx/v1/drive/')||path.startsWith('/mx/v1/auth/')||path==='/ping';};
    page.on('request',request=>{if(tracked(request))requests.set(request,{method:request.method(),path:new URL(request.url()).pathname,startedAtMs:Date.now()});});
    page.on('requestfinished',async request=>{const entry=requests.get(request);if(entry){const response=await request.response();evidence.network.push({...entry,status:response?.status(),finishedAtMs:Date.now()});requests.delete(request);}});
    page.on('requestfailed',request=>{const entry=requests.get(request);if(entry){evidence.network.push({...entry,error:request.failure()?.errorText,finishedAtMs:Date.now()});requests.delete(request);}});
    page.on('pageerror',e=>errors.push(e.message));page.on('dialog',d=>d.dismiss());
    const origin=`http://127.0.0.1:${proxy.address().port}`,begin=Date.now();
    await page.goto(`${origin}/office/index.html#file=${item.uid}`);
    await page.evaluate(()=>{
      window.__mxOfficeWitnessStates=[];
      setInterval(()=>{const node=document.querySelector('.save-feedback');const state={atMs:Date.now(),text:node?.textContent,title:node?.getAttribute('title'),className:node?.className};const previous=window.__mxOfficeWitnessStates.at(-1);if(!previous||previous.text!==state.text||previous.title!==state.title||previous.className!==state.className)window.__mxOfficeWitnessStates.push(state);},250);
    });
    await page.locator('#qtcanvas[data-native-focused="true"]').waitFor({timeout:60000});
    const openingMs=Date.now()-begin;
    await page.getByRole('button',{name:'Document options',exact:true}).click();
    await page.getByRole('checkbox',{name:'Automatically save to MX when idle',exact:true}).uncheck();
    await page.getByRole('button',{name:'Close document options',exact:true}).click();
    const readyAtMs=Date.now(),deadline=readyAtMs+seconds*1000;
    Object.assign(evidence,{browser:browser.version(),itemUid:item.uid,openingMs,readyAtMs});
    onReady?.({readyAtMs,itemUid:item.uid});
    const {markers,saveMs,acknowledgedVersions}=evidence;
    while(Date.now()<deadline&&!signal?.aborted){
      const marker=`Chaos verified edit ${markers.length+1}`;markers.push(marker);
      await page.locator('#qtcanvas').focus();await page.keyboard.press('Control+End');await page.keyboard.press('Enter');await page.keyboard.type(marker,{delay:10});
      const start=Date.now();await page.getByRole('button',{name:'Save to MX',exact:true}).click();
      evidence.currentSaveStartedAtMs=start;
      await page.locator('.save-feedback.saved').waitFor({timeout:60000});saveMs.push(Date.now()-start);
      const bytes=(await call(`/mx/v1/drive/items/${item.uid}/download`,owner)).bytes;
      const contents=unzip(bytes)['word/document.xml'];for(const text of markers)assert.ok(contents.includes(text),'Acknowledged Office save must contain every actual typed edit');
      const metadata=(await call(`/mx/v1/drive/items/${item.uid}/office`,owner)).body;
      assert.ok(metadata.version_uid);acknowledgedVersions.push({versionUid:metadata.version_uid,length:bytes.length,dataHash:createHash('sha256').update(bytes).digest('hex'),verifiedAtMs:Date.now()});
      await sleep(1200);
    }
    assert.deepEqual(errors,[]);
    const saved=(await call(`/mx/v1/drive/items/${item.uid}`,owner)).body;
    assert.equal(saved.revision,markers.length+1);
    const versions=(await call(`/mx/v1/drive/items/${item.uid}/history`,owner)).body;
    assert.equal(versions.versions.length,Math.min(100,markers.length+1));
    await page.screenshot({path:join(directory,'actual-office-under-chaos.png')});
    return {enabled:true,browser:`Chromium-compatible browser ${browser.version()}`,editors:1,itemUid:item.uid,readyAtMs,acknowledgedVersions,openingMs,savedVersions:markers.length,saveMs,allTypedEditsVerified:markers.length>0,pageErrors:errors};
  }catch(error){
    evidence.error=String(error.stack||error).replace(/Bearer\s+[A-Za-z0-9._-]+/g,'Bearer [redacted]');
    if(page&&!page.isClosed()){
      await page.screenshot({path:join(directory,'actual-office-under-chaos-failure.png')}).catch(()=>{});
      evidence.states=await page.evaluate(()=>window.__mxOfficeWitnessStates||[]).catch(()=>[]);
    }
    await writeFile(join(directory,'actual-office-failure.json'),JSON.stringify(evidence,null,2),{mode:0o600});
    error.officeEvidence=evidence;
    throw error;
  }finally{await browser?.close();await new Promise(resolve=>proxy.close(resolve));}
}
