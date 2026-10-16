import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {mkdtempSync,mkdirSync,readdirSync,readFileSync,statSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
const binary=process.env.MX_TEST_BINARY;
assert.ok(binary?.startsWith('/mnt/ST1000LM049/'));
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/tests'));
const directory=mkdtempSync(join(process.env.MX_TEST_ARTIFACTS,'mx-update-cli-')),results=[];
const expectedVersion=JSON.parse(readFileSync(new URL('../package.json',import.meta.url),'utf8')).version;
for(const [args,code,includes] of [
 [['--version'],0,`litiaina-mx ${expectedVersion}`],[['-V'],0,`litiaina-mx ${expectedVersion}`],
 [['--help'],0,'--update-worker'],[['-h'],0,'--update-keygen'],
 [['--misspelled-command'],2,'server was not started'],
 [['--update-worker'],1,'Usage:'],[['--update-worker','--config','missing.json'],1,'Cannot read'],
]){
 const cwd=join(directory,`case-${results.length}`);mkdirSync(cwd);const result=spawnSync(binary,args,{cwd,encoding:'utf8',timeout:10000});assert.equal(result.error,undefined);assert.equal(result.status,code,result.stderr);assert.ok((result.stdout+result.stderr).includes(includes));assert.deepEqual(readdirSync(cwd),[],'diagnostics must not initialize SQLite, config, TLS or jobs');results.push({args,exit_code:code,no_files_created:true});
}
const key=join(directory,'test-signing.pk8'),first=spawnSync(binary,['--update-keygen',key],{cwd:directory,encoding:'utf8',timeout:10000});assert.equal(first.status,0,first.stderr);assert.ok(first.stdout.includes('trusted_keys:'));assert.equal(statSync(key).mode&0o777,0o600);const bytes=readFileSync(key);const second=spawnSync(binary,['--update-keygen',key],{cwd:directory,encoding:'utf8',timeout:10000});assert.equal(second.status,1);assert.deepEqual(readFileSync(key),bytes);results.push({signing_key_mode:'0600',private_key_never_overwritten:true});
writeFileSync(join(directory,'cli-results.json'),JSON.stringify({passed:true,binary,results},null,2));console.log(`Native update CLI passed: ${directory}/cli-results.json`);
