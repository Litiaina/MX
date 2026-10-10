import { defineConfig, loadEnv } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import officeManifest from './public/office/vendor/manifest.json' with { type: 'json' };
import { readFile } from 'node:fs/promises';
import { brotliDecompressSync, gzipSync } from 'node:zlib';

export default defineConfig(({ mode }) => {
  const environment = loadEnv(mode, '.', 'MX_');
  const backendOrigin = environment.MX_BACKEND_ORIGIN || 'https://127.0.0.1:21001';

  return {
    plugins: [svelte(), {
      name: 'mx-office-offline-manifest',
      async generateBundle(_options, bundle) {
        const code = Object.values(bundle).filter(asset => /\.(?:js|css)$/.test(asset.fileName));
        // Adapter, worker and shell changes must invalidate offline caches too.
        const adapters = await Promise.all(['office-thread.js','offline-worker.js'].map(name => readFile(new URL(`./public/office/${name}`, import.meta.url), 'utf8')));
        const fingerprint = JSON.stringify(officeManifest) + adapters.join('') + Object.values(bundle).map(asset => asset.type === 'chunk' ? asset.code : String(asset.source)).join('');
        const digest = [...new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(fingerprint)))].map(byte => byte.toString(16).padStart(2,'0')).join('');
        const generation = `mx-office-assets-${digest.slice(0,16)}`;
        // Browsers check worker SCRIPT bytes for updates, not precache.json.
        // A changed shell/adapter must install a new worker even when its
        // lifecycle source code and registration URL did not change.
        this.emitFile({type:'asset',fileName:'office/offline-worker.js',source:`${adapters[1]}\n// MX build generation: ${generation}\n`});
        this.emitFile({ type: 'asset', fileName: 'office/precache.json', source: JSON.stringify({
          cache: generation,
          assets: ['/office/index.html', ...code.map(asset => `/${asset.fileName}`), '/office/office-thread.js',
            '/office/vendor/soffice.js', '/office/vendor/soffice.wasm', '/office/vendor/soffice.data',
            '/office/vendor/soffice.data.js.metadata', '/office/vendor/zeta.js']
        }) });
        // WebKit does not advertise Brotli on some LAN/loopback connections.
        // Bundle a gzip alternative rather than needing a runtime decompressor
        // or shipping 250 MiB of uncompressed engine files.
        for (const name of ['soffice.wasm','soffice.data']) {
          const compressed=await readFile(new URL(`./public/office/vendor/${name}.br`,import.meta.url));
          this.emitFile({type:'asset',fileName:`office/vendor/${name}.gz`,source:gzipSync(brotliDecompressSync(compressed))});
        }
      }
    }],
    build: {
      outDir: 'dist',
      emptyOutDir: true,
      rolldownOptions: { input: { main: 'index.html', office: 'office/index.html' } }
    },
    server: {
      host: '127.0.0.1',
      port: 5173,
      proxy: {
        '/mx': {
          target: backendOrigin,
          changeOrigin: true,
          secure: false,
          ws: true
        },
        '/ping': {
          target: backendOrigin,
          changeOrigin: true,
          secure: false
        }
      }
    }
  };
});
