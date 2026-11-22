// 与 crates/core 的 serde 表示一致。
export type Actor =
  | { t: 'seat'; seat: number }
  | { t: 'simultaneous'; mask: number }
  | { t: 'system' }
  | { t: 'done' };

export type Outcome = { t: 'win'; seats: number[] } | { t: 'draw' };

export const Level = { Easy: 0, Medium: 1, Hard: 2 } as const;
export type LevelValue = (typeof Level)[keyof typeof Level];
