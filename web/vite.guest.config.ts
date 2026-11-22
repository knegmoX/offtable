import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';
import { build } from './build-info.ts';

// 局域网访客页：由主机 App 的 relay 以 http://192.168.x.x 提供（非安全上下文），
// 不含 WASM，语法降到 ES2017 以覆盖老旧安卓浏览器 / 微信 / UC 内核。
export default defineConfig({
  plugins: [preact()],
  define: { __BUILD__: JSON.stringify(build) },
  base: '/',
  build: {
    target: 'es2017',
    outDir: 'dist',
    emptyOutDir: false,
    assetsDir: 'g',
    rollupOptions: { input: { guest: 'guest.html' } },
  },
});
