import {afterEach,beforeEach,describe,it,expect} from 'vitest';
import {mkdtemp,mkdir,writeFile,rm,readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {checkBuiltAssets} from './check-built-assets.mjs';

let directory;
beforeEach(async()=>{
  directory=await mkdtemp(join(tmpdir(),'mx-built-assets-'));
  await mkdir(join(directory,'office/vendor'),{recursive:true});
  await mkdir(join(directory,'assets'));
  const cache='mx-office-assets-123abc';
  await writeFile(join(directory,'office/precache.json'),JSON.stringify({cache,assets:['/office/index.html','/office/attachment.html','/assets/editor.js','/office/vendor/engine.wasm']}));
  for(const shell of ['index.html','office/index.html','office/attachment.html'])await writeFile(join(directory,shell),'<script src="/assets/editor.js"></script>');
  await writeFile(join(directory,'assets/editor.js'),'export {};');
  await writeFile(join(directory,'office/vendor/engine.wasm.br'),'compressed fixture');
  await writeFile(join(directory,'office/offline-worker.js'),`// MX build generation: ${cache}\n`);
});
afterEach(async()=>{if(directory)await rm(directory,{recursive:true,force:true});});
describe('built Office assets',()=>{
  it('accepts actual shell assets and compressed-only engine resources',async()=>{
    expect(await checkBuiltAssets(directory)).toMatchObject({passed:true,assets:4});
  });
  it('rejects an intermediate entry chunk removed from final output',async()=>{
    const path=join(directory,'office/precache.json'),manifest=JSON.parse(await readFile(path,'utf8'));
    manifest.assets.push('/assets/attachment-removed.js');await writeFile(path,JSON.stringify(manifest));
    await expect(checkBuiltAssets(directory)).rejects.toThrow('Built asset missing: /assets/attachment-removed.js');
  });
  it('rejects shell dependencies absent from the cache manifest',async()=>{
    await writeFile(join(directory,'assets/extra.js'),'export {};');
    await writeFile(join(directory,'office/attachment.html'),'<script src="/assets/extra.js"></script>');
    await expect(checkBuiltAssets(directory)).rejects.toThrow('Office shell asset omitted');
  });
  it('rejects mismatched worker and manifest generations',async()=>{
    await writeFile(join(directory,'office/offline-worker.js'),'// MX build generation: mx-office-assets-456def\n');
    await expect(checkBuiltAssets(directory)).rejects.toThrow('Worker/cache generation mismatch');
  });
});
