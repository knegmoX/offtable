//! 类型擦除的对局会话：WASM 门面与主机 Worker 通过 JSON 字符串与任意游戏交互。
//!
//! JSON 跨边界的开销对回合制游戏可以忽略，换来一个对所有游戏统一的 JS 接口。

use crate::{Actor, Bot, Game, GameRng, Illegal, Level, Outcome, Seat};
use serde::{Deserialize, Serialize};

/// 对局快照：规则状态 + 随机源，足以在主机重启后续局。
#[derive(Deserialize)]
struct Snapshot<G> {
    game: G,
    rng: GameRng,
}

#[derive(Serialize)]
struct SnapshotRef<'a, G> {
    game: &'a G,
    rng: &'a GameRng,
}

/// 对象安全的会话接口。
pub trait DynSession {
    fn game_id(&self) -> &'static str;
    fn seats(&self) -> u8;
    fn to_act(&self) -> Actor;
    fn outcome(&self) -> Option<Outcome>;
    /// `seat = None` 旁观 / 法官视图。
    fn view_json(&self, seat: Option<Seat>) -> String;
    fn legal_json(&self, seat: Seat) -> String;
    /// 成功时返回该操作产生的事件（未投影，供本地 UI 使用）。
    fn apply_json(&mut self, seat: Seat, mv_json: &str) -> Result<String, Illegal>;
    /// 让 AI 为 `seat` 选一步（不落子），返回着法 JSON。
    fn bot_json(&mut self, seat: Seat, level: Level) -> Option<String>;
    fn snapshot_json(&self) -> String;
}

/// 把一个具体游戏 + 它的 AI 包装成 [`DynSession`]。
pub struct Session<G: Game, B: Bot<G>> {
    game: G,
    /// 规则随机源（骰子、洗牌）：只被 `apply` 消耗，保证"种子 + 着法序列"即可回放。
    rng: GameRng,
    /// AI 自己的随机源：AI 的思考不能改变规则随机序列。
    bot_rng: GameRng,
    bot: B,
}

const BOT_SEED_SALT: u64 = 0x9E37_79B9_7F4A_7C15;

impl<G: Game, B: Bot<G>> Session<G, B> {
    pub fn new(cfg: &G::Config, seats: u8, seed: u64, bot: B) -> Result<Self, Illegal> {
        let mut rng = GameRng::from_seed(seed);
        let game = G::new(cfg, seats, &mut rng)?;
        Ok(Self {
            game,
            rng,
            bot_rng: GameRng::from_seed(seed ^ BOT_SEED_SALT),
            bot,
        })
    }

    pub fn restore(snapshot_json: &str, bot: B) -> Result<Self, Illegal> {
        let s: Snapshot<G> = serde_json::from_str(snapshot_json).map_err(|_| Illegal("bad_snapshot"))?;
        let bot_rng = GameRng::from_seed(s.rng.seed() ^ BOT_SEED_SALT ^ s.rng.drawn());
        Ok(Self {
            game: s.game,
            rng: s.rng,
            bot_rng,
            bot,
        })
    }

    pub fn game(&self) -> &G {
        &self.game
    }
}

impl<G: Game, B: Bot<G>> DynSession for Session<G, B> {
    fn game_id(&self) -> &'static str {
        G::ID
    }

    fn seats(&self) -> u8 {
        self.game.seats()
    }

    fn to_act(&self) -> Actor {
        self.game.to_act()
    }

    fn outcome(&self) -> Option<Outcome> {
        self.game.outcome()
    }

    fn view_json(&self, seat: Option<Seat>) -> String {
        serde_json::to_string(&self.game.view(seat)).unwrap_or_else(|_| "null".into())
    }

    fn legal_json(&self, seat: Seat) -> String {
        let mut out = Vec::new();
        self.game.legal(seat, &mut out);
        serde_json::to_string(&out).unwrap_or_else(|_| "[]".into())
    }

    fn apply_json(&mut self, seat: Seat, mv_json: &str) -> Result<String, Illegal> {
        let mv: G::Move = serde_json::from_str(mv_json).map_err(|_| Illegal("bad_move"))?;
        let events = self.game.apply(seat, &mv, &mut self.rng)?;
        Ok(serde_json::to_string(&events).unwrap_or_else(|_| "[]".into()))
    }

    fn bot_json(&mut self, seat: Seat, level: Level) -> Option<String> {
        let view = self.game.view(Some(seat));
        let mv = self.bot.act(&view, seat, level, &mut self.bot_rng)?;
        serde_json::to_string(&mv).ok()
    }

    fn snapshot_json(&self) -> String {
        serde_json::to_string(&SnapshotRef {
            game: &self.game,
            rng: &self.rng,
        })
        .unwrap_or_else(|_| "null".into())
    }
}
