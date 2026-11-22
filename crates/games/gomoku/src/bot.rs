//! 五子棋 AI：五元组（five-tuple）增量估值 + 迭代加深 alpha-beta。
//!
//! 棋盘上每个"连续 5 格窗口"只含一方棋子时为该方贡献 V[k]（k=子数）。
//! 落子/悔子只更新经过该点的 ≤20 个窗口，估值 O(1)、走法排序 O(20)。
//! 难度：Easy = 启发式 + 高温抽样；Medium = 浅层搜索 + 偶发次优；Hard = 深搜（节点预算）。

use crate::{Gomoku, Pt, View};
use offtable_ai::{SearchState, WIN, pick_softmax, search};
use offtable_core::{GameRng, Level, Seat};

/// 窗口内有 k 枚己方子（对方 0 子）时的估值贡献。
const V: [i64; 6] = [0, 1, 8, 60, 500, 0];
/// 在窗口内再添一子的增益（走法排序用）；k=4 → 成五。
const GAIN: [i64; 5] = [1, 7, 52, 440, 10_000_000];
const BLOCK_FIVE: i64 = 5_000_000;

#[derive(Default)]
pub struct GomokuBot;

impl GomokuBot {
    pub fn new() -> Self {
        Self
    }
}

pub(crate) struct Board {
    n: usize,
    cells: Vec<u8>,
    point_wins: Vec<Vec<u16>>,
    /// cnt[w][color]，color ∈ {1,2}
    cnt: Vec<[u8; 3]>,
    score: [i64; 3],
    near: Vec<u8>,
    to_move: u8,
    stones: usize,
    last_won: bool,
    top_k_root: usize,
    top_k: usize,
}

impl Board {
    pub(crate) fn from_view(v: &View) -> Self {
        let n = v.size as usize;
        let mut win_cells = Vec::new();
        for y in 0..n as i32 {
            for x in 0..n as i32 {
                for (dx, dy) in [(1, 0), (0, 1), (1, 1), (1, -1)] {
                    let ex = x + dx * 4;
                    let ey = y + dy * 4;
                    if ex < 0 || ey < 0 || ex >= n as i32 || ey >= n as i32 {
                        continue;
                    }
                    let mut w = [0u16; 5];
                    for (i, c) in w.iter_mut().enumerate() {
                        *c = ((y + dy * i as i32) * n as i32 + (x + dx * i as i32)) as u16;
                    }
                    win_cells.push(w);
                }
            }
        }
        let mut point_wins = vec![Vec::new(); n * n];
        for (wi, w) in win_cells.iter().enumerate() {
            for &c in w {
                point_wins[c as usize].push(wi as u16);
            }
        }
        let mut b = Board {
            n,
            cells: vec![0; n * n],
            cnt: vec![[0; 3]; win_cells.len()],
            point_wins,
            score: [0; 3],
            near: vec![0; n * n],
            to_move: 1,
            stones: 0,
            last_won: false,
            top_k_root: 12,
            top_k: 9,
        };
        let mut blacks = 0usize;
        let mut whites = 0usize;
        for (i, ch) in v.board.chars().enumerate().take(n * n) {
            let color = match ch {
                'x' => 1,
                'o' => 2,
                _ => continue,
            };
            if color == 1 {
                blacks += 1
            } else {
                whites += 1
            }
            b.to_move = color;
            b.make(i as u16);
        }
        b.last_won = false;
        b.to_move = if blacks > whites { 2 } else { 1 };
        b
    }

    #[inline]
    fn contrib(c: &[u8; 3], color: usize) -> i64 {
        let other = 3 - color;
        if c[other] == 0 { V[c[color] as usize] } else { 0 }
    }

    fn opp(&self) -> u8 {
        3 - self.to_move
    }

    /// 走法优先级：己方进攻增益 + 对方在此处的增益（防守价值）。
    fn priority(&self, c: u16, me: u8) -> i64 {
        let (me, op) = (me as usize, 3 - me as usize);
        let (mut attack, mut defense) = (0i64, 0i64);
        for &w in &self.point_wins[c as usize] {
            let k = &self.cnt[w as usize];
            if k[op] == 0 {
                attack += GAIN[k[me] as usize];
            }
            if k[me] == 0 {
                defense += if k[op] == 4 {
                    BLOCK_FIVE
                } else {
                    GAIN[k[op] as usize]
                };
            }
        }
        attack * 10 + defense * 8
    }

    /// `color` 下在 c 能否立即成五。
    fn makes_five(&self, c: u16, color: u8) -> bool {
        let (me, op) = (color as usize, 3 - color as usize);
        self.point_wins[c as usize].iter().any(|&w| {
            let k = &self.cnt[w as usize];
            k[me] == 4 && k[op] == 0
        })
    }

    fn candidates(&self) -> Vec<u16> {
        if self.stones == 0 {
            let mid = (self.n / 2) as u16;
            return vec![mid * self.n as u16 + mid];
        }
        (0..(self.n * self.n) as u16)
            .filter(|&c| self.cells[c as usize] == 0 && self.near[c as usize] > 0)
            .collect()
    }

    /// 按优先级排序的候选（含强制着法处理）。
    fn ordered(&self, limit: usize) -> Vec<(u16, i64)> {
        let me = self.to_move;
        let cands = self.candidates();
        if let Some(&c) = cands.iter().find(|&&c| self.makes_five(c, me)) {
            return vec![(c, i64::MAX)];
        }
        let blocks: Vec<u16> = cands
            .iter()
            .copied()
            .filter(|&c| self.makes_five(c, self.opp()))
            .collect();
        if !blocks.is_empty() {
            return blocks.into_iter().map(|c| (c, BLOCK_FIVE)).collect();
        }
        let mut scored: Vec<(u16, i64)> = cands.into_iter().map(|c| (c, self.priority(c, me))).collect();
        // 稳定排序 + 下标兜底，保证跨平台确定
        scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.truncate(limit);
        scored
    }
}

impl SearchState for Board {
    type Move = u16;

    fn moves(&mut self, out: &mut Vec<u16>, ply: u32) {
        let k = if ply == 0 { self.top_k_root } else { self.top_k };
        out.extend(self.ordered(k).into_iter().map(|(c, _)| c));
    }

    fn make(&mut self, c: u16) {
        let color = self.to_move as usize;
        self.cells[c as usize] = color as u8;
        let mut won = false;
        for i in 0..self.point_wins[c as usize].len() {
            let w = self.point_wins[c as usize][i] as usize;
            let before1 = Self::contrib(&self.cnt[w], 1);
            let before2 = Self::contrib(&self.cnt[w], 2);
            self.cnt[w][color] += 1;
            if self.cnt[w][color] == 5 {
                won = true;
            }
            self.score[1] += Self::contrib(&self.cnt[w], 1) - before1;
            self.score[2] += Self::contrib(&self.cnt[w], 2) - before2;
        }
        self.bump_near(c, 1);
        self.stones += 1;
        self.last_won = won;
        self.to_move = 3 - self.to_move;
    }

    fn unmake(&mut self, c: u16) {
        self.to_move = 3 - self.to_move;
        let color = self.to_move as usize;
        for i in 0..self.point_wins[c as usize].len() {
            let w = self.point_wins[c as usize][i] as usize;
            let before1 = Self::contrib(&self.cnt[w], 1);
            let before2 = Self::contrib(&self.cnt[w], 2);
            self.cnt[w][color] -= 1;
            self.score[1] += Self::contrib(&self.cnt[w], 1) - before1;
            self.score[2] += Self::contrib(&self.cnt[w], 2) - before2;
        }
        self.bump_near(c, -1);
        self.cells[c as usize] = 0;
        self.stones -= 1;
        self.last_won = false;
    }

    fn terminal(&self, ply: u32) -> Option<i32> {
        if self.last_won {
            return Some(-(WIN - ply as i32));
        }
        (self.stones == self.n * self.n).then_some(0)
    }

    fn eval(&self) -> i32 {
        let me = self.to_move as usize;
        let s = self.score[me] * 12 / 10 - self.score[3 - me];
        s.clamp(-(WIN as i64 / 2), WIN as i64 / 2) as i32
    }
}

impl Board {
    fn bump_near(&mut self, c: u16, delta: i8) {
        let n = self.n as i32;
        let (cx, cy) = ((c as i32) % n, (c as i32) / n);
        for dy in -2..=2 {
            for dx in -2..=2 {
                let (x, y) = (cx + dx, cy + dy);
                if x >= 0 && y >= 0 && x < n && y < n {
                    let i = (y * n + x) as usize;
                    self.near[i] = (self.near[i] as i8 + delta) as u8;
                }
            }
        }
    }

    fn to_pt(&self, c: u16) -> Pt {
        Pt {
            x: (c as usize % self.n) as u8,
            y: (c as usize / self.n) as u8,
        }
    }
}

/// 各难度的节点预算（决定强度，与设备速度无关）。
pub fn budget(level: Level) -> (u32, u32) {
    match level {
        Level::Easy => (0, 0),
        Level::Medium => (3, 3_000),
        Level::Hard => (10, 60_000),
    }
}

impl offtable_core::Bot<Gomoku> for GomokuBot {
    fn act(&mut self, view: &View, seat: Seat, level: Level, rng: &mut GameRng) -> Option<Pt> {
        if view.outcome.is_some() || view.to_move != seat {
            return None;
        }
        let mut b = Board::from_view(view);
        let me = seat + 1;
        b.to_move = me;
        let ordered = b.ordered(12);
        let first = *ordered.first()?;
        // 强制着法（成五 / 堵五）：Easy 也必须成五，但有时会漏堵。
        let forced = first.1 >= BLOCK_FIVE;
        match level {
            Level::Easy => {
                if b.makes_five(first.0, me) {
                    return Some(b.to_pt(first.0));
                }
                if forced && rng.chance(65, 100) {
                    return Some(b.to_pt(first.0));
                }
                let pool: Vec<(u16, i64)> = if forced {
                    b.ordered_unforced(6)
                } else {
                    ordered.into_iter().take(6).collect()
                };
                let scores: Vec<i64> = pool.iter().map(|p| p.1).collect();
                let t = (scores[0] / 3).max(40);
                let i = pick_softmax(&scores, t, rng);
                Some(b.to_pt(pool[i].0))
            }
            Level::Medium => {
                if forced {
                    return Some(b.to_pt(first.0));
                }
                if rng.chance(15, 100) && ordered.len() >= 3 {
                    let scores: Vec<i64> = ordered.iter().take(3).map(|p| p.1).collect();
                    let t = (scores[0] / 6).max(20);
                    return Some(b.to_pt(ordered[pick_softmax(&scores, t, rng)].0));
                }
                let (d, n) = budget(Level::Medium);
                search(&mut b, d, n).best.map(|c| b.to_pt(c))
            }
            Level::Hard => {
                if forced {
                    return Some(b.to_pt(first.0));
                }
                let (d, n) = budget(Level::Hard);
                search(&mut b, d, n).best.map(|c| b.to_pt(c))
            }
        }
    }
}

impl Board {
    /// 忽略强制规则的纯启发式排序（Easy 漏堵时使用）。
    fn ordered_unforced(&self, limit: usize) -> Vec<(u16, i64)> {
        let me = self.to_move;
        let mut scored: Vec<(u16, i64)> = self
            .candidates()
            .into_iter()
            .map(|c| (c, self.priority(c, me)))
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.truncate(limit);
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use offtable_core::{Actor, Bot, Game, Outcome};

    fn setup(moves: &[(u8, u8)]) -> (Gomoku, GameRng) {
        let mut rng = GameRng::from_seed(3);
        let mut g = Gomoku::new(&Config::default(), 2, &mut rng).unwrap();
        for &(x, y) in moves {
            let Actor::Seat { seat } = g.to_act() else {
                panic!()
            };
            g.apply(seat, &Pt { x, y }, &mut rng).unwrap();
        }
        (g, rng)
    }

    #[test]
    fn opens_in_center() {
        let (g, mut rng) = setup(&[]);
        for level in [Level::Medium, Level::Hard] {
            let mv = GomokuBot.act(&g.view(Some(0)), 0, level, &mut rng).unwrap();
            assert_eq!(mv, Pt { x: 7, y: 7 });
        }
    }

    #[test]
    fn completes_five_at_every_level() {
        // 黑 (3..7,7) 四连，白零散；轮到黑
        let (g, mut rng) = setup(&[(3, 7), (0, 0), (4, 7), (0, 2), (5, 7), (0, 4), (6, 7), (0, 6)]);
        for level in [Level::Easy, Level::Medium, Level::Hard] {
            let mv = GomokuBot.act(&g.view(Some(0)), 0, level, &mut rng).unwrap();
            assert!(
                mv == Pt { x: 2, y: 7 } || mv == Pt { x: 7, y: 7 },
                "{level:?} {mv:?}"
            );
        }
    }

    #[test]
    fn blocks_four() {
        // 黑冲四 (3..6,7)，(7,7) 被白堵住 → 白必须堵 (2,7)
        let (g, mut rng) = setup(&[(3, 7), (7, 7), (4, 7), (10, 10), (5, 7), (10, 12), (6, 7)]);
        for level in [Level::Medium, Level::Hard] {
            let mv = GomokuBot.act(&g.view(Some(1)), 1, level, &mut rng).unwrap();
            assert_eq!(mv, Pt { x: 2, y: 7 }, "{level:?}");
        }
    }

    #[test]
    fn hard_blocks_open_three() {
        // 黑活三 (6,7)(7,7)(8,7)，两端空；白应在 (5,7) 或 (9,7) 一带应对
        let (g, mut rng) = setup(&[(7, 7), (7, 8), (6, 7), (2, 2), (8, 7)]);
        let mv = GomokuBot.act(&g.view(Some(1)), 1, Level::Hard, &mut rng).unwrap();
        assert!(
            [
                Pt { x: 5, y: 7 },
                Pt { x: 9, y: 7 },
                Pt { x: 4, y: 7 },
                Pt { x: 10, y: 7 }
            ]
            .contains(&mv),
            "{mv:?}"
        );
    }

    #[test]
    fn hard_beats_easy() {
        let mut wins = 0;
        let games = 6;
        for s in 0..games {
            let mut rng = GameRng::from_seed(100 + s);
            let mut g = Gomoku::new(&Config::default(), 2, &mut rng).unwrap();
            // Hard 轮流执黑/执白
            let hard_seat = (s % 2) as u8;
            while let Actor::Seat { seat } = g.to_act() {
                let level = if seat == hard_seat {
                    Level::Hard
                } else {
                    Level::Easy
                };
                let mv = GomokuBot.act(&g.view(Some(seat)), seat, level, &mut rng).unwrap();
                g.apply(seat, &mv, &mut rng).unwrap();
            }
            if g.outcome()
                == Some(Outcome::Win {
                    seats: vec![hard_seat],
                })
            {
                wins += 1;
            }
        }
        assert!(wins >= games - 1, "hard won {wins}/{games}");
    }

    #[test]
    fn incremental_matches_rebuild() {
        let (g, _) = setup(&[(7, 7), (7, 8), (8, 8), (6, 6), (9, 9), (5, 5)]);
        let mut b = Board::from_view(&g.view(None));
        let base = (b.score, b.cnt.clone());
        let c = (3 * 15 + 3) as u16;
        b.make(c);
        b.unmake(c);
        assert_eq!((b.score, b.cnt.clone()), base);
    }
}
