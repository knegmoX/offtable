import { test, expect } from '@playwright/test';
import { startRelay, type RelayInfo } from './relay';
import { clickPoint, moves, ready } from './board';

let relay: RelayInfo;
test.beforeAll(async () => {
  relay = await startRelay();
});
test.afterAll(() => {
  relay.proc.kill();
});

test('人机：落子后 AI 应答，悔棋回退两步', async ({ page }) => {
  await page.goto(relay.host_url.replace('&game=gomoku', ''));
  await page.getByTestId('game-gomoku').click();
  await page.getByTestId('mode-ai').click();
  await page.getByTestId('level-1').click();
  await expect(page.getByTestId('status')).toHaveText('轮到你了');
  await ready(page);
  await clickPoint(page, 7, 7);
  await expect.poll(() => moves(page), { timeout: 10_000 }).toBe(2);
  await expect(page.getByTestId('status')).toHaveText('轮到你了');
  await page.getByTestId('undo').click();
  await expect.poll(() => moves(page)).toBe(0);
});

test('同屏双人：黑方连五获胜', async ({ page }) => {
  await page.goto(relay.host_url);
  await page.getByTestId('mode-local').click();
  await page.getByTestId('restart').click();
  await expect.poll(() => moves(page)).toBe(0);
  await ready(page);
  const seq = [
    [3, 7], [3, 8], [4, 7], [4, 8], [5, 7], [5, 8], [6, 7], [6, 8], [7, 7],
  ];
  for (const [i, [x, y]] of seq.entries()) {
    await ready(page);
    await clickPoint(page, x, y);
    await expect.poll(() => moves(page)).toBe(i + 1);
  }
  await expect(page.getByTestId('status')).toHaveText('黑方获胜');
});

test('存档：刷新后恢复人机对局', async ({ page }) => {
  await page.goto(relay.host_url);
  await page.getByTestId('mode-ai').click();
  await page.getByTestId('restart').click();
  await expect.poll(() => moves(page)).toBe(0);
  await ready(page);
  await clickPoint(page, 6, 6);
  await expect.poll(() => moves(page), { timeout: 10_000 }).toBe(2);
  await page.reload();
  await expect.poll(() => moves(page), { timeout: 10_000 }).toBe(2);
});

test('连点不会用到过期状态：无"还没轮到你"提示，AI 照常应答', async ({ page }) => {
  await page.goto(relay.host_url);
  await page.getByTestId('mode-ai').click();
  await page.getByTestId('restart').click();
  await ready(page);
  await clickPoint(page, 7, 7);
  await clickPoint(page, 8, 6); // 紧接着第二下：应被忽略
  await clickPoint(page, 9, 6);
  await expect.poll(() => moves(page), { timeout: 10_000 }).toBe(2);
  await expect(page.locator('.toast')).toHaveCount(0);
});
