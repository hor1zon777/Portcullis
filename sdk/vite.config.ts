import { defineConfig } from 'vite';
import { resolve } from 'node:path';

export default defineConfig({
  build: {
    lib: {
      entry: resolve(__dirname, 'src/auto-mount.ts'),
      name: 'PowCaptcha',
      formats: ['iife'],
      fileName: () => 'pow-captcha.js',
    },
    rollupOptions: {
      output: {
        inlineDynamicImports: true,
      },
    },
    target: 'es2020',
    minify: 'esbuild',
    sourcemap: true,
  },
  server: {
    port: 5173,
    fs: { allow: ['..'] },
    proxy: {
      '/api-proxy': {
        target: 'http://localhost:8787',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api-proxy/, ''),
      },
    },
  },
});
