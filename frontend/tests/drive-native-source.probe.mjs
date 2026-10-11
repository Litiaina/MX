// Diagnostic control: native disk-backed File -> Blob PUT, without MX queue
// state or Playwright body interception. Isolates runtime file-I/O failures.
// Diagnostic only, not an acceptance/qualification runner.
import { webkit } from 'playwright';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { startIsolatedMx } from './isolated-mx.mjs';
const mx = await startIsolatedMx(fileURLToPath(new URL('../', import.meta.url)));
let browser;
try {
  if(mx.n1.live)throw Error('This diagnostic requires its isolated storage fixture, not live N1');
  const password = randomUUID();
  await mx.call('/mx/v1/auth/create', mx.signupKey, 'POST', { email:'probe@test.invalid', name:'Probe',password },[201]);
  const { body: user } = await mx.call('/mx/v1/auth/authenticate', null, 'POST', {email:'probe@test.invalid',password});
  const operation = randomUUID();
  const source = join(mx.directory, 'probe.MP4'); await writeFile(source, Buffer.alloc(18874368));
  browser = await webkit.launch({headless:true,executablePath:'/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP/webkit-launch.sh',env:{...process.env,PULSE_SERVER:`unix:${mx.directory}/no-audio.sock`,PIPEWIRE_REMOTE:'mx_source_probe',ALSOFT_DRIVERS:'null',SDL_AUDIODRIVER:'dummy'}});
  const page = await browser.newPage({ignoreHTTPSErrors:true});
  await page.addInitScript(tokens => {sessionStorage.setItem('mx_access_token',tokens.access_token);sessionStorage.setItem('mx_refresh_token',tokens.refresh_token);},user);
  if (process.env.MX_PROBE_INTERCEPTION) await page.route('**/drive/uploads/*/finish', route => route.continue());
  await page.goto(`https://127.0.0.1:${mx.port}/#drive`);
  await page.getByRole('heading',{name:'MX Drive',exact:true}).waitFor();
  await page.evaluate(()=>{const input=document.createElement('input');input.id='native-source';input.type='file';input.multiple=true;input.className='hidden';document.body.append(input);});
  await page.locator('#native-source').setInputFiles(source);
  console.log(JSON.stringify(await page.evaluate(async ({uid,token,xhrInit,pinSource})=>{
    const input=document.querySelector('#native-source'),file=input.files[0],results=[];
    const retained=pinSource ? file.slice() : file;
    input.value='';input.remove();await new Promise(resolve=>setTimeout(resolve,1500));
    const metadata=JSON.stringify({operation_uid:uid,file_name:file.name,mime_type:file.type,size:file.size,last_modified:file.lastModified});
    if(xhrInit)await new Promise(resolve=>{const xhr=new XMLHttpRequest();xhr.open('POST','/mx/v1/drive/uploads');xhr.setRequestHeader('Authorization',`Bearer ${token}`);xhr.setRequestHeader('Content-Type','application/json');xhr.onload=resolve;xhr.onerror=resolve;xhr.send(metadata);});
    else await fetch('/mx/v1/drive/uploads',{method:'POST',headers:{Authorization:`Bearer ${token}`,'Content-Type':'application/json'},body:metadata});
    const send=(index=0)=>new Promise(resolve=>{
      const xhr=new XMLHttpRequest();xhr.open('PUT',`/mx/v1/drive/uploads/${uid}/parts/${index}`);xhr.setRequestHeader('Authorization',`Bearer ${token}`);
      xhr.setRequestHeader('Content-Type','application/octet-stream');xhr.upload.onprogress=()=>{};xhr.upload.onload=()=>{};xhr.onprogress=()=>{};
      xhr.onload=()=>resolve({status:xhr.status,body:xhr.responseText});xhr.onerror=()=>resolve({error:'connection'});xhr.send(retained.slice(index*4194304,(index+1)*4194304));
    });
    results.push(...await Promise.all([send(),send(1)]));results.push(await send());
    let read;try{read=(await file.slice(0,8).arrayBuffer()).byteLength;}catch(error){read=String(error);}
    return{fileSize:file.size,eightByteRead:read,results};
  },{uid:operation,token:user.access_token,xhrInit:!!process.env.MX_PROBE_XHR_INIT,pinSource:!!process.env.MX_PROBE_PIN_SOURCE})));
}finally{await browser?.close();await mx.close();}
