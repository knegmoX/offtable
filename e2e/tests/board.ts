import type { Page } from '@playwright/test';

/** 点击五子棋棋盘交点 (x, y)（棋盘 15 路，viewBox = 160）。 */
export async function clickPoint(page: Page, x: number, y: number, size = 15) {
  const board = page.getByTestId('gomoku-board');
  const box = (await board.boundingBox())!;
  const total = (size - 1) * 10 + 20;
  await page.mouse.click(box.x + ((10 + x * 10) / total) * box.width, box.y + ((10 + y * 10) / total) * box.height);
}

export async function moves(page: Page): Promise<number> {
  return Number(await page.getByTestId('gomoku-board').getAttribute('data-moves'));
}

/** 等棋盘可交互（新局初始化 / AI 思考结束）。 */
export async function ready(page: Page) {
  await page.getByTestId('gomoku-board').and(page.locator('[aria-busy="false"]')).waitFor();
}
