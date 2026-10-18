// Real N1 RC.1 + production MX. All roots, credentials and data are disposable.
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,readFile,writeFile,open} from 'node:fs/promises';
import {join,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFile,spawn} from 'node:child_process';
import {promisify} from 'node:util';
import {randomUUID,createHmac} from 'node:crypto';
import {createServer} from 'node:net';
import {request} from 'playwright';

const exec=promisify(execFile),frontend=fileURLToPath(new URL('../',import.meta.url));
const binary=process.env.MX_TEST_N1_BINARY;
assert.ok(binary?.startsWith('/mnt/ST1000LM049/tests/'),'Use an external disposable N1 binary');
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/tests/'));
assert.ok(process.env.MX_TEST_BINARY?.startsWith('/mnt/ST1000LM049/'));
assert.ok(!process.env.MX_TEST_N1_URL&&!process.env.MX_TEST_N1_CREDENTIALS_FILE,'Never reuse live storage credentials');
const version=(await exec(binary,['--version'])).stdout.trim();
assert.match(version,/^litiaina-n1 1\.0\.0-RC\.1\nstorage_format 1\.0\.0-RC\.1\nstorage_encoding 4\nnoa v1\nnative_protocol n1\/0\.5$/);
const directory=await mkdtemp(join(process.env.MX_TEST_ARTIFACTS,'mx-n1-rc1-'));
const engine=join(directory,'engine');await mkdir(engine);await mkdir(join(engine,'storage'));await mkdir(join(engine,'control'));
async function freePort(){const server=createServer();await new Promise(r=>server.listen(0,'127.0.0.1',r));const port=server.address().port;await new Promise(r=>server.close(r));return port;}
const port=await freePort(),nativePort=await freePort();
const jwt=randomUUID()+randomUUID(),master=randomUUID()+randomUUID(),secret=randomUUID()+randomUUID();
await writeFile(join(engine,'n1.env'),`JWT_SECRET=${jwt}\nMASTER_KEY=${master}\nAUTH_KEY=${randomUUID()}\nADMIN_KEY=${randomUUID()}\n`,{mode:0o600});
await exec('openssl',['req','-x509','-newkey','rsa:2048','-nodes','-keyout',join(engine,'key.pem'),'-out',join(engine,'cert.pem'),'-days','1','-subj','/CN=localhost']);
const overrides=`[server]
host=127.0.0.1
https_port=${port}
native_port=${nativePort}
cert_path=${join(engine,'cert.pem')}
key_path=${join(engine,'key.pem')}
[disk_config]
fs_storage_path=${join(engine,'control')}
[storage_pool.A]
generation=1
roots=${join(engine,'storage')}
data_shards=1
parity_shards=0
sealed=false
[index_config]
partition_count=64
[garbage_collector]
cleanup_interval_seconds=3600
file_ttl_seconds=3600
trash_retention_seconds=2592000
[auto_healing]
enabled=false
topology_recovery_stability_seconds=0
[set_limits]
max_file_size=9223372036854775807
[jwt_token_config]
login_token_expiration=900
[rate_limiter_config]
enable_rate_limiting=false
[upload_config]
max_concurrent_io=2
upload_part_strict_durability_default=true
stream_max_concurrency=8
max_part_size=52428800
stripe_block_size=262144
`;
// Keep N1's complete default schema, including mandatory disabled-feature
// settings. Override only this disposable engine's roots, ports and budgets.
const template=await readFile(process.env.MX_TEST_N1_CONFIG_TEMPLATE||join(dirname(binary),'n1_config.default.ini'),'utf8');
const sections=new Map();
for(const source of [template,overrides]){
 let section;
 for(const raw of source.split(/\r?\n/)){
  const line=raw.trim();if(!line||line.startsWith('#')||line.startsWith(';'))continue;
  if(line.startsWith('[')){section=line.slice(1,-1);if(!sections.has(section))sections.set(section,new Map());continue;}
  const split=line.indexOf('=');assert.ok(section&&split>0);sections.get(section).set(line.slice(0,split),line.slice(split+1));
 }
}
await writeFile(join(engine,'n1_config.ini'),[...sections].map(([name,values])=>`[${name}]\n${[...values].map(([key,value])=>`${key}=${value}`).join('\n')}\n`).join('\n'));
const log=await open(join(engine,'server.log'),'w');
const api=await request.newContext({baseURL:`https://127.0.0.1:${port}`,ignoreHTTPSErrors:true,timeout:15000});
const environment={...process.env,JWT_SECRET:jwt,MASTER_KEY:master,AUTH_KEY:randomUUID(),ADMIN_KEY:randomUUID(),N1_REFRESH_STORE_ROOT:join(engine,'refresh'),N1_ACCEL_READ_URL:'',RUST_LOG:'warn'};
const child=spawn(binary,[],{cwd:engine,env:environment,stdio:['ignore',log.fd,log.fd]});
const results=[];
try{
 console.log(`N1 RC.1 compatibility artifacts: ${directory}`);
 const deadline=Date.now()+45000;
 while(true){try{if((await api.get('/noa/v1/ping',{timeout:1000})).ok())break;}catch{}assert.equal(child.exitCode,null,'N1 exited during startup');assert.ok(Date.now()<deadline,'N1 startup timeout');await new Promise(r=>setTimeout(r,100));}
 // Provision only this engine using its newly generated filesystem authority.
 const header=Buffer.from(JSON.stringify({alg:'HS256',typ:'JWT'})).toString('base64url');
 const payload=Buffer.from(JSON.stringify({path:engine,read:true,write:true,modify:true,delete:true,exp:Math.floor(Date.now()/1000)+3600})).toString('base64url');
 const token=`${header}.${payload}.${createHmac('sha256',jwt).update(`${header}.${payload}`).digest('base64url')}`;
 const response=await api.post('/noa/v1/fragments',{headers:{Authorization:`Bearer ${token}`},data:{name:`mx-compat-${randomUUID()}`,key:secret,secret:master}});
 assert.ok(response.ok(),`Isolated fragment creation: HTTP ${response.status()}`);
 const fragment=(await response.json()).fragment_hash;
 const credentials=join(directory,'test-fragment.env');await writeFile(credentials,`N1_FRAGMENT_HASH=${fragment}\nN1_MX_SECRET=${secret}\n`,{mode:0o600});
 async function suite(script,browsers){
  console.log(`Starting ${script}: ${browsers}`);
  const status=await new Promise((resolve,reject)=>{const test=spawn(process.execPath,['--no-warnings',join(frontend,'tests',script)],{cwd:frontend,env:{...process.env,MX_TEST_ARTIFACTS:directory,MX_TEST_N1_URL:`https://127.0.0.1:${port}`,MX_TEST_N1_CREDENTIALS_FILE:credentials,MX_TEST_OWNED_N1_DIR:engine,MX_TEST_OWNED_N1_PID:String(child.pid),MX_TEST_BROWSERS:browsers},stdio:'inherit'});test.on('error',reject);test.on('exit',code=>resolve(code));});
  assert.equal(status,0,`${script} failed`);results.push({suite:script,browsers:browsers.split(','),passed:true});
 }
 const selected=(process.env.MX_TEST_N1_SUITES||'drive,office').split(',');
 assert.ok(selected.length&&new Set(selected).size===selected.length&&selected.every(name=>['drive','office'].includes(name)));
 if(selected.includes('drive'))await suite('drive.browser.mjs',process.env.MX_TEST_DRIVE_BROWSERS||'chromium,firefox,webkit');
 if(selected.includes('office'))await suite('office-durable.browser.mjs',process.env.MX_TEST_OFFICE_BROWSERS||'chromium,brave,firefox');
 await writeFile(join(directory,'n1-rc1-results.json'),JSON.stringify({passed:true,n1Version:version,mxVersion:(await exec(process.env.MX_TEST_BINARY,['--version'])).stdout.trim(),results,storage:'Isolated real N1, 1D+0P single-root functional compatibility; not EC/power-loss qualification',productionWrites:0,existingTestDeploymentChanged:false},null,2));
 console.log(`N1 RC.1 compatibility passed: ${directory}/n1-rc1-results.json`);
}finally{
 await api.dispose();if(child.exitCode===null){const exit=new Promise(r=>child.once('exit',r));child.kill('SIGTERM');let timer;await Promise.race([exit,new Promise(r=>timer=setTimeout(()=>{child.kill('SIGKILL');r();},10000))]);clearTimeout(timer);}await log.close();
}
