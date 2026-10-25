// Inspect actual output, not intermediate bundle entries. No deployment writes.
import assert from 'node:assert/strict';
import {readFile,stat} from 'node:fs/promises';
import {resolve,sep} from 'node:path';
import {fileURLToPath} from 'node:url';

export async function checkBuiltAssets(directory) {
  const root=resolve(directory);
  const manifest=JSON.parse(await readFile(resolve(root,'office/precache.json'),'utf8'));
  assert.match(manifest.cache,/^mx-office-assets-[a-f0-9]+$/);
  assert.ok(Array.isArray(manifest.assets)&&manifest.assets.length>0);
  assert.equal(new Set(manifest.assets).size,manifest.assets.length,'Duplicate cache paths');
  async function exists(url) {
    assert.ok(url.startsWith('/')&&!url.includes('..')&&!url.includes('\\'),'Unsafe asset path');
    const path=resolve(root,'.'+url);
    assert.ok(path.startsWith(root+sep));
    for(const suffix of ['', '.br', '.gz']) {
      try {if((await stat(path+suffix)).isFile())return;} catch(error) {if(error.code!=='ENOENT')throw error;}
    }
    assert.fail(`Built asset missing: ${url}`);
  }
  for(const url of manifest.assets)await exists(url);
  for(const shell of ['index.html','office/index.html','office/attachment.html']) {
    const html=await readFile(resolve(root,shell),'utf8');
    for(const match of html.matchAll(/(?:src|href)="(\/[^\"]+)"/g)) {
      await exists(match[1]);
      if(shell.startsWith('office/')&&match[1].startsWith('/assets/')) {
        assert.ok(manifest.assets.includes(match[1]),`Office shell asset omitted from cache: ${match[1]}`);
      }
    }
  }
  assert.ok(manifest.assets.includes('/office/attachment.html'),'Attachment shell missing from cache');
  const worker=await readFile(resolve(root,'office/offline-worker.js'),'utf8');
  assert.ok(worker.includes(`// MX build generation: ${manifest.cache}`),'Worker/cache generation mismatch');
  return {passed:true,assets:manifest.assets.length,cache:manifest.cache};
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  console.log(JSON.stringify(await checkBuiltAssets(process.argv[2]||'dist')));
}
