//! offtable-core —— 所有游戏共享的抽象。
//!
//! 设计约束（见 docs/PLAN.md §4）：
//! - 规则只在这里定义一次，编译成 WASM 在 App / PWA / 主机 Worker 内运行；
//! - `view(seat)` 是唯一的信息出口：局域网下发与 AI 玩家上下文共用它，隐藏信息必须在此剔除；
//! - 随机性只来自显式传入的 [`GameRng`]（ChaCha8），保证每日一题、回放、跨端结果一致。
#![forbid(unsafe_code)]

pub mod dynamic;
pub mod rng;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub use rng::GameRng;

/// 座位号，0 起。
pub type Seat = u8;

/// 当前该谁行动。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Actor {
    Seat {
        seat: Seat,
    },
    /// 同时行动（投票等），bit i = 座位 i 尚未提交。
    Simultaneous {
        mask: u32,
    },
    /// 系统行动（发牌、掷骰等，由主机推进）。
    System,
    Done,
}

/// 终局结果。`Win.seats` 支持团队获胜。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Outcome {
    Win { seats: Vec<Seat> },
    Draw,
}

/// 非法操作，携带稳定的机器可读错误码（UI 负责本地化）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Illegal(pub &'static str);

impl core::fmt::Display for Illegal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Illegal {}

/// AI 难度档位：强度由节点预算决定（不按时间），保证低端机上"困难"不会退化。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Easy,
    Medium,
    Hard,
}

impl Level {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Level::Easy,
            1 => Level::Medium,
            _ => Level::Hard,
        }
    }
}

/// 一局游戏的规则。实现者必须保证：
/// - `apply` 只接受 `legal` 中的着法，否则返回 [`Illegal`] 且不改变状态；
/// - 同一初始种子与着法序列在任何平台上产生相同状态（禁止浮点、HashMap 迭代序、系统随机）。
pub trait Game: Clone + Serialize + DeserializeOwned {
    /// 稳定的游戏 ID（URL、存档、协议中使用）。
    const ID: &'static str;
    type Config: Clone + Default + Serialize + DeserializeOwned;
    type Move: Clone + PartialEq + core::fmt::Debug + Serialize + DeserializeOwned;
    type View: Serialize;
    type Event: Clone + Serialize;

    fn new(cfg: &Self::Config, seats: u8, rng: &mut GameRng) -> Result<Self, Illegal>;
    fn seats(&self) -> u8;
    fn to_act(&self) -> Actor;
    fn legal(&self, seat: Seat, out: &mut Vec<Self::Move>);
    fn apply(&mut self, seat: Seat, mv: &Self::Move, rng: &mut GameRng) -> Result<Vec<Self::Event>, Illegal>;
    fn outcome(&self) -> Option<Outcome>;
    /// 给某个座位看的视图；`None` = 旁观者 / 法官。
    fn view(&self, seat: Option<Seat>) -> Self::View;
    /// 事件按座位投影；完全信息游戏原样下发，隐藏信息游戏必须覆盖。
    fn project(ev: &Self::Event, _to: Option<Seat>) -> Option<Self::Event> {
        Some(ev.clone())
    }
}

/// AI 玩家：只能看到自己座位的视图（与局域网访客同一信息边界）。
pub trait Bot<G: Game> {
    fn act(&mut self, view: &G::View, seat: Seat, level: Level, rng: &mut GameRng) -> Option<G::Move>;
}
