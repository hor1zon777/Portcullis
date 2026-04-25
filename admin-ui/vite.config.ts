import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

// Vite 配置在 Node 上下文运行；admin-ui 没有装 @types/node，这里最小声明即可。
declare const process: { cwd(): string };

// 后端默认监听 127.0.0.1:8787（dev.sh / dev.ps1 设置 CAPTCHA_BIND）
// 这里所有需要后端的路径统一反向代理过去，开发模式只暴露 5173 一个端口。
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const BACKEND = env.CAPTCHA_DEV_BACKEND || 'http://127.0.0.1:8787';

  return {
    plugins: [react()],
    resolve: {
      alias: {
        '@': '/src',
      },
    },
    base: '/admin/',
    server: {
      host: '127.0.0.1',
      port: 5173,
      strictPort: true,
      proxy: {
        // admin 后台 API
        '/admin/api': { target: BACKEND, changeOrigin: true },
        // 公开 PoW API（/api/v1/challenge、/verify、/siteverify 等）
        '/api': { target: BACKEND, changeOrigin: true },
        // 内嵌 SDK 静态资源（manifest、版本化路径、wasm）
        '/sdk': { target: BACKEND, changeOrigin: true },
        // 本地演示页面（examples/demo.html，由后端内联吐出）
        '/demo': { target: BACKEND, changeOrigin: true },
        // 健康检查
        '/healthz': { target: BACKEND, changeOrigin: true },
        // Prometheus 指标（启用时）
        '/metrics': { target: BACKEND, changeOrigin: true },
      },
    },
    build: {
      outDir: 'dist',
      sourcemap: true,
      rollupOptions: {
        output: {
          manualChunks: {
            recharts: ['recharts'],
            react: ['react', 'react-dom', 'react-router-dom'],
          },
        },
      },
    },
  };
});
