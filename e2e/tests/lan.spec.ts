import { test, expect, type Browser, type BrowserContext, type Page } from '@playwright/test';
import { startRelay, type RelayInfo } from './relay';
import { clickPoint, moves } from './board';

// 每个用例一个全新的 relay（App 里每次"开房间"也是新 relay + 新密钥），并关闭所有访客上下文。
let relay: RelayInfo;
const contexts: BrowserContext[] = [];
test.beforeEach(async () => {
  relay = await startRelay();
  expect(relay.urls.length, '需要至少一个局域网 IPv4').toBeGreaterThan(0);
});
test.afterEach(async () => {
  await Promise.all(contexts.splice(0).map((c) => c.close()));
  relay.proc.kill();
});

async function openHost(page: Page): Promise<string> {
  await page.goto(relay.host_url);
  await page.getByTestId('mode-lan').click();
  await page.getByTestId('host-open').click();
  const url = (await page.getByTestId('room-url').textContent())!;
  expect(url).toContain(`?k=${relay.key}`);
  return url;
}

async function guest(browser: Browser, url: string): Promise<Page> {
  const ctx = await browser.newContext();
  contexts.push(ctx);
  const p = await ctx.newPage();
  await p.goto(url);
  return p;
}

test('房主 + 访客：扫码加入、对弈、终局，帧中无多余信息', async ({ page, browser }) => {
  const url = await openHost(page);
  const g = await guest(browser, url);
  const frames: string[] = [];
  g.on('websocket', (ws) => ws.on('framereceived', (f) => frames.push(String(f.payload))));
  await expect(g.getByTestId('guest-status')).toHaveText('已加入，等待房主开始…');
  await expect(page.getByTestId('players')).toContainText('白');
  await page.getByTestId('host-start').click();

  // 房主执黑，访客执白，黑方连五
  const seq: [Page, number, number][] = [
    [page, 3, 7], [g, 3, 8], [page, 4, 7], [g, 4, 8], [page, 5, 7], [g, 5, 8], [page, 6, 7], [g, 6, 8], [page, 7, 7],
  ];
  for (const [i, [who, x, y]] of seq.entries()) {
    await expect.poll(() => moves(who)).toBe(i);
    await clickPoint(who, x, y);
    await expect.poll(() => moves(page)).toBe(i + 1);
    await expect.poll(() => moves(g)).toBe(i + 1);
  }
  await expect(page.getByTestId('status')).toHaveText('你赢了！');
  await expect(g.getByTestId('guest-status')).toHaveText('你输了');
  // relay 不应把房间密钥或其他访客令牌回传给访客
  expect(frames.some((f) => f.includes(relay.secret))).toBe(false);
});

test('访客断线重连后立即恢复棋局', async ({ page, browser }) => {
  const url = await openHost(page);
  const g = await guest(browser, url);
  await expect(g.getByTestId('guest-status')).toHaveText('已加入，等待房主开始…');
  await page.getByTestId('host-start').click();
  await clickPoint(page, 7, 7);
  await expect.poll(() => moves(g)).toBe(1);
  await g.reload(); // 令牌存在 localStorage，重连拿回白方座位
  await expect.poll(() => moves(g)).toBe(1);
  await expect(g.getByTestId('guest-seat')).toContainText('白');
  await clickPoint(g, 8, 8);
  await expect.poll(() => moves(page)).toBe(2);
});

test('错误密钥被拒绝；伪造 Host 头被拒绝', async ({ browser, request }) => {
  const bad = relay.urls[0] + '/?k=wrong';
  const g = await guest(browser, bad);
  await expect(g.getByTestId('guest-fatal')).toHaveText('房间密钥不对，请重新扫码');
  const r = await request.get(`http://127.0.0.1:${relay.port}/`, { headers: { Host: 'evil.example:80' } });
  expect(r.status()).toBe(403);
  const shell = await request.get(`${relay.urls[0]}/index.html`);
  expect(shell.status(), '主机外壳不应对局域网暴露').toBe(404);
});

test('8 名访客同时在线：前 1 人入座，其余观战', async ({ page, browser }) => {
  const url = await openHost(page);
  const guests = await Promise.all(Array.from({ length: 8 }, () => guest(browser, url)));
  for (const g of guests) await expect(g.getByTestId('guest-status')).toHaveText('已加入，等待房主开始…');
  await expect(page.getByTestId('players').locator('li')).toHaveCount(9);
  await page.getByTestId('host-start').click();
  await clickPoint(page, 7, 7);
  for (const g of guests) await expect.poll(() => moves(g)).toBe(1);
  const seated = await Promise.all(guests.map((g) => g.getByTestId('guest-seat').textContent()));
  expect(seated.filter((s) => s?.includes('白')).length).toBe(1);
  expect(seated.filter((s) => s?.includes('观战')).length).toBe(7);
});
