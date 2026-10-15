// Dependency-free guard for packaging matching MX/backend/frontend releases.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';

const read=path=>readFile(new URL(path,import.meta.url),'utf8');
const cargo=await read('../../Cargo.toml');
const packageBlock=cargo.match(/^\[package\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1];
const version=packageBlock?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
assert.ok(version,'Cargo package version is missing');
const frontend=JSON.parse(await read('../package.json'));
const lock=JSON.parse(await read('../package-lock.json'));
const cargoLock=await read('../../Cargo.lock');
const backendLock=cargoLock.split('[[package]]').find(block=>/^name\s*=\s*"litiaina-mx"\s*$/m.test(block));
assert.equal(frontend.version,version,'Frontend/backend versions must match');
assert.equal(lock.version,version,'Frontend lockfile version must match');
assert.equal(lock.packages[''].version,version,'Frontend lockfile root version must match');
assert.equal(backendLock?.match(/^version\s*=\s*"([^"]+)"/m)?.[1],version,'Cargo lockfile root version must match');
const readme=await read('../../README.md');
const label=version.split('+')[0].includes('-')?'candidate':'release';
assert.ok(readme.includes(`**Current ${label}: ${version}.**`),'README current version/channel must match the package');
console.log(`MX release metadata verified: ${version} (${label}).`);
