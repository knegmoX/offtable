//! 五子棋（Gomoku）规则。黑（座位 0）先行；默认"五连及以上"获胜，可选"恰好五连"。
#![forbid(unsafe_code)]

pub mod bot;

use offtable_core::{Actor, Game, GameRng, Illegal, Outcome, Seat};
use serde::{Deserialize, Serialize};

pub use bot::GomokuBot;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pt {
    pub x: u8,
    pub y: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 棋盘边长，9..=19。
    pub size: u8,
    /// true = 恰好五连才胜（长连不算）。
    pub exact_five: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            size: 15,
            exact_five: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gomoku {
    size: u8,
    exact_five: bool,
    /// 行优先，0 空 / 1 黑 / 2 白。
    cells: Vec<u8>,
    to_move: Seat,
    history: Vec<Pt>,
    outcome: Option<Outcome>,
    win_line: Option<[Pt; 2]>,
}

/// 完全信息游戏：所有座位看到同一视图。棋盘压缩成字符串（'.' 空、'x' 黑、'o' 白）以减小帧体积。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct View {
    pub size: u8,
    pub board: String,
    pub to_move: Seat,
    pub last: Option<Pt>,
    pub moves: u16,
    pub outcome: Option<Outcome>,
    pub win_line: Option<[Pt; 2]>,
    pub exact_five: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Event {
    Placed { seat: Seat, at: Pt },
    Won { seat: Seat, line: [Pt; 2] },
    Draw,
}

const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

impl Gomoku {
    fn idx(&self, p: Pt) -> usize {
        p.y as usize * self.size as usize + p.x as usize
    }

    fn at(&self, x: i32, y: i32) -> Option<u8> {
        let n = self.size as i32;
        (x >= 0 && y >= 0 && x < n && y < n).then(|| self.cells[(y * n + x) as usize])
    }

    /// 以 p 为中心在方向 d 上的连续同色长度及两端端点。
    fn run(&self, p: Pt, d: (i32, i32), color: u8) -> (u32, Pt, Pt) {
        let (mut a, mut b) = (p, p);
        let mut len = 1;
        for sign in [1, -1] {
            let (mut x, mut y) = (p.x as i32, p.y as i32);
            loop {
                x += d.0 * sign;
                y += d.1 * sign;
                if self.at(x, y) != Some(color) {
                    break;
                }
                len += 1;
                let q = Pt {
                    x: x as u8,
                    y: y as u8,
                };
                if sign == 1 {
                    b = q;
                } else {
                    a = q;
                }
            }
        }
        (len, a, b)
    }

    pub fn history(&self) -> &[Pt] {
        &self.history
    }
}

impl Game for Gomoku {
    const ID: &'static str = "gomoku";
    type Config = Config;
    type Move = Pt;
    type View = View;
    type Event = Event;

    fn new(cfg: &Config, seats: u8, _rng: &mut GameRng) -> Result<Self, Illegal> {
        if seats != 2 {
            return Err(Illegal("seats"));
        }
        if !(9..=19).contains(&cfg.size) {
            return Err(Illegal("size"));
        }
        Ok(Self {
            size: cfg.size,
            exact_five: cfg.exact_five,
            cells: vec![0; cfg.size as usize * cfg.size as usize],
            to_move: 0,
            history: Vec::new(),
            outcome: None,
            win_line: None,
        })
    }

    fn seats(&self) -> u8 {
        2
    }

    fn to_act(&self) -> Actor {
        if self.outcome.is_some() {
            Actor::Done
        } else {
            Actor::Seat { seat: self.to_move }
        }
    }

    fn legal(&self, seat: Seat, out: &mut Vec<Pt>) {
        if self.outcome.is_some() || seat != self.to_move {
            return;
        }
        let n = self.size;
        for y in 0..n {
            for x in 0..n {
                let p = Pt { x, y };
                if self.cells[self.idx(p)] == 0 {
                    out.push(p);
                }
            }
        }
    }

    fn apply(&mut self, seat: Seat, mv: &Pt, _rng: &mut GameRng) -> Result<Vec<Event>, Illegal> {
        if self.outcome.is_some() {
            return Err(Illegal("game_over"));
        }
        if seat != self.to_move {
            return Err(Illegal("not_your_turn"));
        }
        if mv.x >= self.size || mv.y >= self.size {
            return Err(Illegal("out_of_board"));
        }
        let i = self.idx(*mv);
        if self.cells[i] != 0 {
            return Err(Illegal("occupied"));
        }
        let color = seat + 1;
        self.cells[i] = color;
        self.history.push(*mv);
        let mut events = vec![Event::Placed { seat, at: *mv }];
        for d in DIRS {
            let (len, a, b) = self.run(*mv, d, color);
            let wins = if self.exact_five { len == 5 } else { len >= 5 };
            if wins {
                self.outcome = Some(Outcome::Win { seats: vec![seat] });
                self.win_line = Some([a, b]);
                events.push(Event::Won { seat, line: [a, b] });
                return Ok(events);
            }
        }
        if self.history.len() == self.cells.len() {
            self.outcome = Some(Outcome::Draw);
            events.push(Event::Draw);
        } else {
            self.to_move = 1 - self.to_move;
        }
        Ok(events)
    }

    fn outcome(&self) -> Option<Outcome> {
        self.outcome.clone()
    }

    fn view(&self, _seat: Option<Seat>) -> View {
        View {
            size: self.size,
            board: self
                .cells
                .iter()
                .map(|c| match c {
                    1 => 'x',
                    2 => 'o',
                    _ => '.',
                })
                .collect(),
            to_move: self.to_move,
            last: self.history.last().copied(),
            moves: self.history.len() as u16,
            outcome: self.outcome.clone(),
            win_line: self.win_line,
            exact_five: self.exact_five,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(size: u8, exact: bool) -> (Gomoku, GameRng) {
        let mut rng = GameRng::from_seed(1);
        let g = Gomoku::new(
            &Config {
                size,
                exact_five: exact,
            },
            2,
            &mut rng,
        )
        .unwrap();
        (g, rng)
    }

    /// 依次落子（黑白交替）。
    fn play(g: &mut Gomoku, rng: &mut GameRng, pts: &[(u8, u8)]) -> Vec<Event> {
        let mut last = vec![];
        for &(x, y) in pts {
            let seat = match g.to_act() {
                Actor::Seat { seat } => seat,
                _ => panic!("game over early"),
            };
            last = g.apply(seat, &Pt { x, y }, rng).unwrap();
        }
        last
    }

    #[test]
    fn horizontal_five_wins() {
        let (mut g, mut r) = game(15, false);
        let ev = play(
            &mut g,
            &mut r,
            &[
                (0, 0),
                (0, 1),
                (1, 0),
                (1, 1),
                (2, 0),
                (2, 1),
                (3, 0),
                (3, 1),
                (4, 0),
            ],
        );
        assert!(matches!(ev.last(), Some(Event::Won { seat: 0, .. })));
        assert_eq!(g.outcome(), Some(Outcome::Win { seats: vec![0] }));
        assert_eq!(g.to_act(), Actor::Done);
        assert_eq!(
            g.view(None).win_line,
            Some([Pt { x: 0, y: 0 }, Pt { x: 4, y: 0 }])
        );
    }

    #[test]
    fn diagonal_five_wins_for_white() {
        let (mut g, mut r) = game(15, false);
        // 白在反对角线上连五
        let ev = play(
            &mut g,
            &mut r,
            &[
                (0, 0),
                (10, 4),
                (0, 2),
                (9, 5),
                (0, 4),
                (8, 6),
                (0, 6),
                (7, 7),
                (0, 8),
                (6, 8),
            ],
        );
        // 黑 (0,0),(0,2)... 不连续，不会先赢；白第 5 子成五
        assert!(matches!(ev.last(), Some(Event::Won { seat: 1, .. })), "{ev:?}");
    }

    #[test]
    fn overline_rules() {
        // 黑: 0,1,2,3 + 5 再补 4 → 六连
        let moves = [
            (0, 0),
            (0, 5),
            (1, 0),
            (1, 5),
            (2, 0),
            (2, 5),
            (3, 0),
            (3, 5),
            (5, 0),
            (9, 9),
            (4, 0),
        ];
        let (mut free, mut r1) = game(15, false);
        let ev = play(&mut free, &mut r1, &moves);
        assert!(matches!(ev.last(), Some(Event::Won { seat: 0, .. })));
        let (mut exact, mut r2) = game(15, true);
        // 恰好五连模式下：白方在第 9 手前已有 4 子，(9,9) 打断，黑六连不算胜
        let ev = play(&mut exact, &mut r2, &moves);
        assert!(!matches!(ev.last(), Some(Event::Won { .. })), "{ev:?}");
        assert!(exact.outcome().is_none());
    }

    #[test]
    fn illegal_moves_rejected_without_mutation() {
        let (mut g, mut r) = game(15, false);
        g.apply(0, &Pt { x: 7, y: 7 }, &mut r).unwrap();
        let before = serde_json::to_string(&g).unwrap();
        assert_eq!(g.apply(1, &Pt { x: 7, y: 7 }, &mut r), Err(Illegal("occupied")));
        assert_eq!(
            g.apply(0, &Pt { x: 1, y: 1 }, &mut r),
            Err(Illegal("not_your_turn"))
        );
        assert_eq!(
            g.apply(1, &Pt { x: 15, y: 0 }, &mut r),
            Err(Illegal("out_of_board"))
        );
        assert_eq!(serde_json::to_string(&g).unwrap(), before);
    }

    #[test]
    fn rejects_bad_config() {
        let mut r = GameRng::from_seed(0);
        assert!(
            Gomoku::new(
                &Config {
                    size: 5,
                    exact_five: false
                },
                2,
                &mut r
            )
            .is_err()
        );
        assert!(Gomoku::new(&Config::default(), 3, &mut r).is_err());
    }

    #[test]
    fn view_is_compact_and_identical_for_all_seats() {
        let (mut g, mut r) = game(9, false);
        play(&mut g, &mut r, &[(4, 4), (3, 3)]);
        let v = g.view(Some(0));
        assert_eq!(v, g.view(Some(1)));
        assert_eq!(v, g.view(None));
        assert_eq!(v.board.len(), 81);
        assert_eq!(v.board.chars().filter(|c| *c == 'x').count(), 1);
        assert_eq!(v.last, Some(Pt { x: 3, y: 3 }));
    }

    mod prop {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #![proptest_config(ProptestConfig { cases: 64, .. ProptestConfig::default() })]
            /// 随机合法对局：永不 panic；终局后无合法着法；步数不超过格数。
            #[test]
            fn random_games_are_sound(seed in any::<u64>(), size in 9u8..=15, exact in any::<bool>()) {
                let mut rng = GameRng::from_seed(seed);
                let mut g = Gomoku::new(&Config { size, exact_five: exact }, 2, &mut rng).unwrap();
                let mut legal = Vec::new();
                let mut steps = 0usize;
                while let Actor::Seat { seat } = g.to_act() {
                    legal.clear();
                    g.legal(seat, &mut legal);
                    prop_assert!(!legal.is_empty());
                    let mv = legal[rng.below(legal.len() as u64) as usize];
                    g.apply(seat, &mv, &mut rng).unwrap();
                    steps += 1;
                    prop_assert!(steps <= size as usize * size as usize);
                }
                prop_assert!(g.outcome().is_some());
                legal.clear();
                g.legal(0, &mut legal);
                g.legal(1, &mut legal);
                prop_assert!(legal.is_empty());
                // 快照往返一致
                let json = serde_json::to_string(&g).unwrap();
                let back: Gomoku = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(back.view(None), g.view(None));
            }
        }
    }
}
