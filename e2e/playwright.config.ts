import { defineConfig } from '@playwright/test';

// 使用系统 Chrome（不下载浏览器）；本机经 scripts/lp 低优先级运行。
export default defineConfig({
  testDir: './tests',
  timeout: 60_000,
  workers: 1,
  reporter: [['list']],
  use: {
    channel: 'chrome',
    headless: true,
    viewport: { width: 420, height: 860 },
    locale: 'zh-CN',
  },
});
