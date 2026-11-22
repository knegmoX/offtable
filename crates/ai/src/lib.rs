//! offtable-ai —— 通用博弈树搜索与"像人"的着法选择。
//!
//! - [`search`]：迭代加深 negamax + alpha-beta，按**节点预算**停止（不按时间：
//!   结果确定、低端机上难度不退化）。
//! - [`pick_softmax`]：低难度用的有温度抽样，制造自然的失误。
#![forbid(unsafe_code)]

use offtable_core::GameRng;

/// 终局分值上限；`WIN - ply` 让 AI 偏好更快的胜利、更慢的失败。
pub const WIN: i32 = 1_000_000;

/// 可搜索的局面（完全信息、两人零和、轮流行动）。
pub trait SearchState {
    type Move: Copy + Eq;

    /// 生成候选着法（已按启发式从好到坏排序）。允许只返回强制着法（如必须堵冲四）。
    fn moves(&mut self, out: &mut Vec<Self::Move>, ply: u32);
    fn make(&mut self, m: Self::Move);
    fn unmake(&mut self, m: Self::Move);
    /// 若对局已结束，返回"轮到行动的一方"视角的分值（被对手连成 = 负分）。
    fn terminal(&self, ply: u32) -> Option<i32>;
    /// 静态估值，"轮到行动的一方"视角。
    fn eval(&self) -> i32;
}

#[derive(Clone, Debug)]
pub struct SearchResult<M> {
    pub best: Option<M>,
    pub score: i32,
    /// 最后完整完成的深度。
    pub depth: u32,
    pub nodes: u32,
}

struct Ctx {
    nodes: u32,
    budget: u32,
    aborted: bool,
}

/// 迭代加深搜索：深度 1..=max_depth，节点数超出 `budget` 时停止，返回最后完整完成那一层的结果。
pub fn search<S: SearchState>(s: &mut S, max_depth: u32, budget: u32) -> SearchResult<S::Move> {
    let mut ctx = Ctx {
        nodes: 0,
        budget: budget.max(1),
        aborted: false,
    };
    let mut root: Vec<S::Move> = Vec::new();
    s.moves(&mut root, 0);
    let mut result = SearchResult {
        best: root.first().copied(),
        score: 0,
        depth: 0,
        nodes: 0,
    };
    if root.len() <= 1 {
        return result;
    }
    for depth in 1..=max_depth.max(1) {
        let mut alpha = -WIN - 1;
        let beta = WIN + 1;
        let mut best_this: Option<(S::Move, i32)> = None;
        for &m in &root {
            s.make(m);
            ctx.nodes += 1;
            let score = -negamax(s, depth - 1, 1, -beta, -alpha, &mut ctx);
            s.unmake(m);
            if ctx.aborted {
                break;
            }
            if best_this.is_none_or(|(_, b)| score > b) {
                best_this = Some((m, score));
            }
            alpha = alpha.max(score);
        }
        if ctx.aborted {
            break;
        }
        if let Some((m, sc)) = best_this {
            result = SearchResult {
                best: Some(m),
                score: sc,
                depth,
                nodes: ctx.nodes,
            };
            // 上一层的最佳着法放到下一层最先搜，提高剪枝效率。
            if let Some(pos) = root.iter().position(|x| *x == m) {
                root[..=pos].rotate_right(1);
            }
            if sc.abs() >= WIN - 1000 {
                break; // 已找到必胜/必败，无需更深
            }
        }
    }
    result.nodes = ctx.nodes;
    result
}

fn negamax<S: SearchState>(s: &mut S, depth: u32, ply: u32, mut alpha: i32, beta: i32, ctx: &mut Ctx) -> i32 {
    if let Some(t) = s.terminal(ply) {
        return t;
    }
    if depth == 0 {
        return s.eval();
    }
    if ctx.nodes >= ctx.budget {
        ctx.aborted = true;
        return 0;
    }
    let mut moves = Vec::new();
    s.moves(&mut moves, ply);
    if moves.is_empty() {
        return 0; // 无子可下：和
    }
    let mut best = -WIN - 1;
    for m in moves {
        s.make(m);
        ctx.nodes += 1;
        let score = -negamax(s, depth - 1, ply + 1, -beta, -alpha, ctx);
        s.unmake(m);
        if ctx.aborted {
            return 0;
        }
        if score > best {
            best = score;
        }
        if best > alpha {
            alpha = best;
        }
        if alpha >= beta {
            break;
        }
    }
    best
}

/// 按分值做"有温度"的抽样：`temperature` 越大越随机（与分值同量纲）。
/// 全部使用整数运算以保证跨平台确定。返回下标。
pub fn pick_softmax(scores: &[i64], temperature: i64, rng: &mut GameRng) -> usize {
    if scores.is_empty() {
        return 0;
    }
    let t = temperature.max(1);
    let max = *scores.iter().max().unwrap();
    // exp(-(max - s)/t) 的整数近似：每差 t 权重减半。
    let weights: Vec<u64> = scores
        .iter()
        .map(|&s| {
            let halvings = ((max - s) / t).min(30) as u32;
            (1u64 << 30) >> halvings
        })
        .collect();
    rng.weighted(&weights)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 取子游戏：一堆 n 个石子，每次取 1~3 个，取到最后一个者胜。n%4==0 时先手必败。
    #[derive(Clone)]
    struct Nim {
        n: u32,
        history: Vec<u32>,
    }

    impl SearchState for Nim {
        type Move = u32;
        fn moves(&mut self, out: &mut Vec<u32>, _ply: u32) {
            for k in 1..=3 {
                if k <= self.n {
                    out.push(k);
                }
            }
        }
        fn make(&mut self, m: u32) {
            self.n -= m;
            self.history.push(m);
        }
        fn unmake(&mut self, m: u32) {
            self.n += m;
            self.history.pop();
        }
        fn terminal(&self, ply: u32) -> Option<i32> {
            // 上一手取走了最后一个：轮到的人输了
            (self.n == 0 && !self.history.is_empty()).then(|| -(WIN - ply as i32))
        }
        fn eval(&self) -> i32 {
            0
        }
    }

    #[test]
    fn finds_winning_nim_move() {
        for n in [5u32, 6, 7, 9, 10, 11] {
            let mut s = Nim { n, history: vec![] };
            let r = search(&mut s, 12, 1_000_000);
            assert_eq!((n - r.best.unwrap()) % 4, 0, "n={n}");
            assert!(r.score > WIN - 100);
        }
    }

    #[test]
    fn respects_budget() {
        let mut s = Nim {
            n: 40,
            history: vec![],
        };
        let r = search(&mut s, 40, 500);
        assert!(r.nodes <= 600, "nodes={}", r.nodes);
        assert!(r.best.is_some());
    }

    #[test]
    fn softmax_prefers_best() {
        let mut rng = GameRng::from_seed(1);
        let mut hits = 0;
        for _ in 0..1000 {
            if pick_softmax(&[100, 10, 0], 20, &mut rng) == 0 {
                hits += 1;
            }
        }
        assert!(hits > 900, "hits={hits}");
    }
}
