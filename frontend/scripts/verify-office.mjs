// Build-time verification only. Deployment never fetches an Office engine.
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const vendor = new URL('../public/office/vendor/', import.meta.url);
const manifest = JSON.parse(await readFile(new URL('manifest.json', vendor), 'utf8'));
for (const [name, expected] of Object.entries(manifest.assets)) {
  const bytes = await readFile(new URL(name, vendor));
  if (createHash('sha256').update(bytes).digest('hex') !== expected) throw new Error(`Office asset checksum mismatch: ${name}`);
}
console.log('Bundled Office engine checksums verified; no runtime CDN dependency.');
