import {afterAll, expect, it} from 'vitest';
import {mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';

const checker=fileURLToPath(new URL('../../../scripts/check-release.mjs',import.meta.url));
const fixtures:string[]=[];
const version='1.0.0-rc.1';
function fixture(change:(files:Record<string,string>)=>void=()=>{}) {
  const root=mkdtempSync(join(tmpdir(),'mx-release-metadata-'));fixtures.push(root);
  const files:Record<string,string>={
    'Cargo.toml':`[package]\nname = "litiaina-mx"\nversion = "${version}"\n[dependencies]\nother = "9.9.9"\n`,
    'Cargo.lock':`[[package]]\nname = "other"\nversion = "9.9.9"\n[[package]]\nname = "litiaina-mx"\nversion = "${version}"\n`,
    'frontend/package.json':JSON.stringify({version}),
    'frontend/package-lock.json':JSON.stringify({version,packages:{'':{version}}}),
    'README.md':`**Current candidate: ${version}.**`,
  };
  change(files);mkdirSync(join(root,'frontend/scripts'),{recursive:true});
  for(const [name,contents] of Object.entries(files))writeFileSync(join(root,name),contents);
  const script=join(root,'frontend/scripts/check-release.mjs');writeFileSync(script,readFileSync(checker));return script;
}
const run=(script:string)=>execFileSync(process.execPath,[script],{encoding:'utf8',stdio:'pipe'});
afterAll(()=>{for(const directory of fixtures)rmSync(directory,{recursive:true,force:true});});

it('checks the actual repository version owners without editing them',()=>{
  expect(run(checker)).toContain('MX release metadata verified:');
});
it('selects the MX Cargo package rather than an unrelated dependency',()=>{
  expect(run(fixture())).toContain(`${version} (candidate)`);
});
it.each(['frontend/package.json','frontend/package-lock.json','Cargo.lock','README.md'])('rejects a mismatched version in %s',name=>{
  const script=fixture(files=>files[name]=files[name].replaceAll(version,'4.1.1'));
  expect(()=>run(script)).toThrow();
});
it('checks the lockfile package root as well as its top-level version',()=>{
  const script=fixture(files=>files['frontend/package-lock.json']=JSON.stringify({version,packages:{'':{version:'4.1.1'}}}));
  expect(()=>run(script)).toThrow();
});
it('does not let a candidate be advertised as a final README release',()=>{
  expect(()=>run(fixture(files=>files['README.md']=`**Current release: ${version}.**`))).toThrow();
});
