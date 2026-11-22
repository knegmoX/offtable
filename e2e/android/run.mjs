// Android 模拟器端到端：真实 App（WebView + Rust relay + Worker 内 game.wasm）开房，
// runner 上的 Chrome 扮演扫码访客，经 adb 端口转发连入，完整下一局五子棋。
// 用法（在 android-emulator-runner 的 script 中）：node e2e/android/run.mjs <apk>
import { _android, chromium } from '@playwright/test';
import { execSync } from 'node:child_process';
import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import path from 'node:path';

const APK = process.argv[2];
const PKG = 'app.offtable.games';
const OUT = path.resolve(import.meta.dirname, '../android-results');
mkdirSync(OUT, { recursive: true });
const sh = (cmd) => execSync(cmd, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const summary = [];
const log = (s) => {
  console.log(s);
  summary.push(s);
};

async function retry(fn, tries = 30, ms = 1000) {
  for (let i = 0; ; i++) {
    try {
      return await fn();
    } catch (e) {
      if (i >= tries) throw e;
      await sleep(ms);
    }
  }
}

async function clickPoint(page, x, y, size = 15) {
  const box = await page.getByTestId('gomoku-board').boundingBox();
  const total = (size - 1) * 10 + 20;
  await page.mouse.click(box.x + ((10 + x * 10) / total) * box.width, box.y + ((10 + y * 10) / total) * box.height);
}
const moves = async (page) => Number(await page.getByTestId('gomoku-board').getAttribute('data-moves'));
async function waitMoves(page, n, timeout = 15000) {
  const t0 = Date.now();
  while ((await moves(page).catch(() => -1)) !== n) {
    if (Date.now() - t0 > timeout) throw new Error(`moves != ${n}`);
    await sleep(200);
  }
}

let guestBrowser;
try {
  sh(`adb install -r ${APK}`);
  log(`installed ${path.basename(APK)}`);

  // 冷启动（模拟器数值只作趋势参考，门槛以真机为准）
  const cold = [];
  for (let i = 0; i < 3; i++) {
    sh(`adb shell am force-stop ${PKG}`);
    await sleep(1500);
    const out = sh(`adb shell am start -W -n ${PKG}/.MainActivity`);
    const m = out.match(/TotalTime:\s*(\d+)/);
    if (m) cold.push(Number(m[1]));
  }
  log(`cold start TotalTime (ms, emulator): ${cold.join(', ')}`);
  await sleep(4000);
  sh(`adb exec-out screencap -p > ${OUT}/1-launch.png`);

  // 经 Playwright 的 Android 支持连接 App 内 WebView（WebView 的调试端点不支持浏览器级上下文管理，
  // 不能用 connectOverCDP）
  const [device] = await retry(async () => {
    const ds = await _android.devices();
    if (!ds.length) throw new Error('no adb device');
    return ds;
  }, 20);
  const webview = await device.webView({ pkg: PKG }, { timeout: 60000 });
  const page = await webview.page();
  log(`host page: ${page.url()}`);

  // 主机：开房
  await page.getByTestId('game-gomoku').click();
  await page.getByTestId('mode-lan').click();
  await page.getByTestId('host-open').click();
  const url = (await page.getByTestId('room-url').textContent({ timeout: 15000 })).trim();
  const m = url.match(/^http:\/\/[^/]+:(\d+)\/\?k=([0-9a-f]+)$/);
  if (!m) throw new Error(`unexpected room url: ${url}`);
  const [, port, key] = m;
  log(`room url (in emulator): ${url}`);
  await page.screenshot({ path: `${OUT}/2-host-qr.png` });

  // 访客：runner 上的 Chrome 经端口转发连入 App 内 relay
  sh(`adb forward tcp:${port} tcp:${port}`);
  guestBrowser = await chromium.launch({ channel: 'chrome' });
  const g = await (await guestBrowser.newContext({ locale: 'en-US', viewport: { width: 390, height: 844 } })).newPage();
  await g.goto(`http://127.0.0.1:${port}/?k=${key}`);
  await g.getByTestId('guest-status').filter({ hasText: /Joined|已加入/ }).waitFor({ timeout: 20000 });
  log('guest joined via in-app relay');

  await page.getByTestId('host-start').click();
  const seq = [
    [page, 3, 7], [g, 3, 8], [page, 4, 7], [g, 4, 8], [page, 5, 7], [g, 5, 8], [page, 6, 7], [g, 6, 8], [page, 7, 7],
  ];
  for (const [i, [who, x, y]] of seq.entries()) {
    await waitMoves(who, i);
    await clickPoint(who, x, y);
    await waitMoves(page, i + 1);
    await waitMoves(g, i + 1);
  }
  const hostStatus = (await page.getByTestId('status').textContent()).trim();
  const guestStatus = (await g.getByTestId('guest-status').textContent()).trim();
  log(`final: host="${hostStatus}" guest="${guestStatus}"`);
  if (!/You win|你赢了/.test(hostStatus) || !/You lose|你输了/.test(guestStatus)) throw new Error('unexpected result');
  await page.screenshot({ path: `${OUT}/3-host-win.png` });
  await g.screenshot({ path: `${OUT}/4-guest-lose.png` });
  sh(`adb exec-out screencap -p > ${OUT}/5-device.png`);
  log('✅ Android app ⇄ browser guest: full game passed');
} catch (e) {
  log(`❌ ${e.message}`);
  process.exitCode = 1;
} finally {
  try {
    writeFileSync(`${OUT}/logcat.txt`, sh('adb logcat -d'));
    const crash = sh('adb logcat -d -b crash');
    if (crash.includes(PKG)) {
      log('❌ crash buffer mentions the app');
      writeFileSync(`${OUT}/crash.txt`, crash);
      process.exitCode = 1;
    }
  } catch {}
  await guestBrowser?.close().catch(() => {});
  await _android.devices().then((ds) => Promise.all(ds.map((d) => d.close()))).catch(() => {});
  const md = ['### Android 模拟器端到端', '', ...summary.map((s) => `- ${s}`), ''].join('\n');
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, md + '\n');
  process.exit(process.exitCode ?? 0);
}
