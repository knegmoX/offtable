//! cargo run --release -p offtable-gomoku --example bench —— 测 AI 每步耗时（native）
use offtable_core::{Actor, Bot, Game, GameRng, Level};
use offtable_gomoku::{Config, Gomoku, GomokuBot};
use std::time::Instant;

fn main() {
    for level in [Level::Medium, Level::Hard] {
        let mut times = Vec::new();
        for s in 0..4u64 {
            let mut rng = GameRng::from_seed(500 + s);
            let mut g = Gomoku::new(&Config::default(), 2, &mut rng).unwrap();
            while let Actor::Seat { seat } = g.to_act() {
                let lv = if seat == 0 { level } else { Level::Medium };
                let t = Instant::now();
                let mv = GomokuBot.act(&g.view(Some(seat)), seat, lv, &mut rng).unwrap();
                if seat == 0 {
                    times.push(t.elapsed().as_secs_f64() * 1000.0);
                }
                g.apply(seat, &mv, &mut rng).unwrap();
            }
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p = |q: f64| times[((times.len() as f64 - 1.0) * q) as usize];
        println!(
            "{level:?}: moves={} p50={:.1}ms p95={:.1}ms max={:.1}ms",
            times.len(),
            p(0.5),
            p(0.95),
            times.last().unwrap()
        );
    }
}
