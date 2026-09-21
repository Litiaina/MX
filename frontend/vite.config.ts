import { defineConfig, loadEnv } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig(({ mode }) => {
  const environment = loadEnv(mode, '.', 'MX_');
  const backendOrigin = environment.MX_BACKEND_ORIGIN || 'https://127.0.0.1:21001';

  return {
    plugins: [svelte()],
    build: {
      outDir: 'dist',
      emptyOutDir: true
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
