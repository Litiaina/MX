// Owns disposable processes and paths only. No deployment config/credentials.
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,writeFile,readFile,open,stat,statfs,readdir,copyFile,chmod} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {randomUUID,createHash} from 'node:crypto';
import {createServer} from 'node:net';
import https from 'node:https';
import {spawn,execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

export const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
export async function until(test,label,ms=30000){const end=Date.now()+ms;do{if(await test())return;await sleep(100);}while(Date.now()<end);throw Error(`Timeout: ${label}`);}
export async function freePort(){const s=createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
export const digest=data=>createHash('sha256').update(data).digest('hex');

export function client(url,{ca,timeout=30000}={}){
  const agent=new https.Agent({ca,keepAlive:true,maxSockets:1200,maxFreeSockets:100});
  let active=0,peak=0;
  const samples=[];
  const call=async(path,token,method='GET',data,expected=[200],extraHeaders={},deadlineMs=timeout)=>{
    const body=data===undefined?undefined:Buffer.isBuffer(data)?data:Buffer.from(JSON.stringify(data));
    const start=performance.now();active++;peak=Math.max(peak,active);let status='transport';
    try{
      const response=await new Promise((resolve,reject)=>{
        let deadline;
        const req=https.request(new URL(path,url),{method,agent,headers:{...(token?{Authorization:`Bearer ${token}`} :{}),...(body?{'Content-Type':Buffer.isBuffer(data)?'application/octet-stream':'application/json','Content-Length':body.length}:{}),...extraHeaders}},res=>{
          const chunks=[];res.on('data',b=>chunks.push(b));res.on('error',e=>{clearTimeout(deadline);reject(e);});res.on('end',()=>{clearTimeout(deadline);resolve({status:res.statusCode,headers:res.headers,bytes:Buffer.concat(chunks)});});
        });
        deadline=setTimeout(()=>req.destroy(Error(`${method} ${path}: request deadline (${deadlineMs}ms)`)),deadlineMs);req.on('error',e=>{clearTimeout(deadline);reject(e);});req.end(body);
      });
      status=response.status;
      const text=response.bytes.toString();let parsed;try{parsed=JSON.parse(text);}catch{}
      if(!expected.includes(status))throw Error(`${method} ${path}: HTTP ${status} ${text.slice(0,800)}`);
      return {...response,body:parsed,milliseconds:performance.now()-start};
    }finally{active--;samples.push({method,path:path.split('?')[0],status,ms:performance.now()-start});}
  };
  return {call,samples,get peak(){return peak;},close(){agent.destroy();}};
}

export async function startN1Lab(parent){
  assert.ok(resolve(parent).startsWith('/mnt/PROJECT-DISK-1TB/'),'Test data must be on the HDD');
  const dir=await mkdtemp(join(parent,'n1-'));
  const device=(await stat(parent)).dev;
  for(const name of ['control','member','refresh','tmp'])await mkdir(join(dir,name),{mode:0o700});
  const port=await freePort(),native=await freePort();
  const secret=randomUUID()+randomUUID(),master=randomUUID()+randomUUID();
  const sourceBinary=process.env.N1_TEST_BINARY;
  assert.ok(sourceBinary?.startsWith('/mnt/PROJECT-DISK-1TB/'),'Explicit HDD N1 release binary required');
  const binary=join(dir,'litiaina-n1');await copyFile(sourceBinary,binary);await chmod(binary,0o500);
  const n1Source=process.env.N1_TEST_SOURCE||fileURLToPath(new URL('../../../N1/',import.meta.url));
  let config=await readFile(join(n1Source,'shell/web/n1_config.default.ini'),'utf8');
  const set=(section,key,value)=>{
    const start=config.indexOf(`[${section}]`);assert.ok(start>=0,section);
    const end=config.indexOf('\n[',start+1);const stop=end<0?config.length:end;
    const part=config.slice(start,stop);const pattern=new RegExp(`^${key}\\s*=.*$`,'m');assert.ok(pattern.test(part),`${section}.${key}`);
    config=config.slice(0,start)+part.replace(pattern,`${key} = ${value}`)+config.slice(stop);
  };
  for(const [key,value] of Object.entries({host:'127.0.0.1',https_port:port,native_port:native,cert_path:join(dir,'cert.pem'),key_path:join(dir,'key.pem')}))set('server',key,value);
  set('disk_config','fs_storage_path',join(dir,'control'));
  for(const [key,value] of Object.entries({roots:join(dir,'member'),data_shards:1,parity_shards:0}))set('storage_pool.A',key,value);
  set('auto_healing','enabled','false');set('auto_healing','topology_recovery_stability_seconds',0);
  set('upload_config','upload_part_strict_durability_default','true');
  // Explicit operator profile for a trusted MX backend on one HDD. Keep rate
  // limiting/adaptive admission enabled; this does not increase disk throughput.
  const capacityProfile=process.env.N1_TEST_CAPACITY_PROFILE==='hdd-100';
  if(capacityProfile){
    set('io_control','foreground_wait_timeout_ms',120000);
    set('rate_limiter_config','view_limit_per_second',1000);
    set('rate_limiter_config','view_limit_burst_size',3000);
    set('rate_limiter_config','global_limit_per_second',2000);
    set('rate_limiter_config','global_limit_burst_size',5000);
  }
  await writeFile(join(dir,'n1_config.ini'),config,{mode:0o600});
  await writeFile(join(dir,'n1.env'),`MASTER_KEY=${master}\nAUTH_KEY=${randomUUID()}\nADMIN_KEY=${randomUUID()}\nJWT_SECRET=${randomUUID()+randomUUID()}\nN1_IO_BACKEND=auto\n`,{mode:0o600});
  execFileSync('openssl',['req','-x509','-newkey','ec','-pkeyopt','ec_paramgen_curve:prime256v1','-nodes','-days','2','-subj','/CN=127.0.0.1','-addext','subjectAltName=IP:127.0.0.1','-keyout',join(dir,'key.pem'),'-out',join(dir,'cert.pem')],{stdio:'ignore'});
  const ca=await readFile(join(dir,'cert.pem'));
  const api=client(`https://127.0.0.1:${port}`,{ca,timeout:90000});
  const log=await open(join(dir,'engine.log'),'w');
  const env=Object.fromEntries(Object.entries(process.env).filter(([k])=>!k.startsWith('N1_')&&!['MASTER_KEY','AUTH_KEY','ADMIN_KEY','JWT_SECRET'].includes(k)));
  Object.assign(env,{N1_REFRESH_STORE_ROOT:join(dir,'refresh'),TMPDIR:join(dir,'tmp'),RUST_LOG:'warn',RUST_BACKTRACE:'1'});
  let child;
  async function start(){assert.equal((await stat(parent)).dev,device,'HDD unmounted');child=spawn(binary,[],{cwd:dir,env,stdio:['ignore',log.fd,log.fd]});await until(async()=>{assert.equal(child.exitCode,null,`N1 exited; ${dir}/engine.log`);try{return(await api.call('/noa/v1/ping')).status===200;}catch{return false;}},'N1 startup',60000);}
  async function stop(signal='SIGTERM'){if(child&&child.exitCode===null&&child.signalCode===null){const exited=new Promise(r=>child.once('exit',r));child.kill(signal);let timer;await Promise.race([exited,new Promise(r=>timer=setTimeout(()=>{child.kill('SIGKILL');r();},10000))]);clearTimeout(timer);}}
  try{
    await start();
    const fragment=(await api.call('/noa/v1/fragments',null,'POST',{name:`mx-concurrency-${randomUUID()}`,key:secret,secret:master},[200,201])).body.fragment_hash;
    assert.ok(fragment,'N1 fragment creation');
    return {dir,binary,capacityProfile:capacityProfile?'hdd-100':'N1 defaults',fragment,secret,live:true,url:`https://127.0.0.1:${port}`,api,get pid(){return child.pid;},async restart(signal='SIGKILL'){await stop(signal);await start();},async close(){await stop();api.close();await log.close();}};
  }catch(e){await stop();api.close();await log.close();throw e;}
}

export async function resourceSample(pid){
  try{
    const text=await readFile(`/proc/${pid}/status`,'utf8');
    const io=await readFile(`/proc/${pid}/io`,'utf8');
    return {rssKiB:Number(text.match(/^VmRSS:\s+(\d+)/m)?.[1]||0),threads:Number(text.match(/^Threads:\s+(\d+)/m)?.[1]||0),fds:(await readdir(`/proc/${pid}/fd`)).length,readBytes:Number(io.match(/^read_bytes:\s+(\d+)/m)?.[1]||0),writeBytes:Number(io.match(/^write_bytes:\s+(\d+)/m)?.[1]||0)};
  }catch{return null;}
}
export async function safeDisk(path){const s=await statfs(path);assert.ok(s.bavail*s.bsize>8*1024**3,'HDD free-space safety floor (8GiB)');}
export function distribution(samples){const s=samples.slice().sort((a,b)=>a-b);return {count:s.length,p50:s[Math.floor(s.length*.5)]||0,p95:s[Math.floor(s.length*.95)]||0,p99:s[Math.floor(s.length*.99)]||0,max:s.at(-1)||0};}
