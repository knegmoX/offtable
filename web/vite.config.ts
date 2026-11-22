import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';
import { build } from './build-info.ts';

// App / PWA 外壳：现代 WebView（Android System WebView、iOS 15+ WKWebView）
export default defineConfig({
  plugins: [
    preact(),
    {
      // relay 读取 build.txt，与访客页 hello 中的构建号比对
      name: 'offtable-build-txt',
      generateBundle() {
        this.emitFile({ type: 'asset', fileName: 'build.txt', source: build });
      },
    },
  ],
  define: { __BUILD__: JSON.stringify(build) },
  base: './',
  build: {
    target: 'es2020',
    outDir: 'dist',
    emptyOutDir: true,
    assetsDir: 'assets',
    rollupOptions: { input: { index: 'index.html' } },
  },
  worker: { format: 'es' },
  server: { host: '127.0.0.1', port: 5173 },
});
