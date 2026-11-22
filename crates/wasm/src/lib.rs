//! game.wasm 门面：所有游戏共用一个 `GameSession` JS 类，数据以 JSON 字符串跨边界。
//! 在 Web Worker 中运行（App / PWA 本地对局，以及局域网主机的权威房间）。
#![forbid(unsafe_code)]

use offtable_core::dynamic::{DynSession, Session};
use offtable_core::{Level, Seat};
use wasm_bindgen::prelude::*;

fn err(code: &str) -> JsError {
    JsError::new(code)
}

fn open(game: &str, cfg_json: &str, seats: u8, seed: u64) -> Result<Box<dyn DynSession>, JsError> {
    match game {
        #[cfg(feature = "gomoku")]
        "gomoku" => {
            let cfg = if cfg_json.is_empty() {
                Default::default()
            } else {
                serde_json::from_str(cfg_json).map_err(|_| err("bad_config"))?
            };
            let s = Session::<offtable_gomoku::Gomoku, _>::new(&cfg, seats, seed, offtable_gomoku::GomokuBot)
                .map_err(|e| err(e.0))?;
            Ok(Box::new(s))
        }
        _ => Err(err("unknown_game")),
    }
}

fn reopen(game: &str, snapshot: &str) -> Result<Box<dyn DynSession>, JsError> {
    match game {
        #[cfg(feature = "gomoku")]
        "gomoku" => Ok(Box::new(
            Session::<offtable_gomoku::Gomoku, _>::restore(snapshot, offtable_gomoku::GomokuBot)
                .map_err(|e| err(e.0))?,
        )),
        _ => Err(err("unknown_game")),
    }
}

fn seat_opt(seat: i32) -> Option<Seat> {
    (seat >= 0).then_some(seat as Seat)
}

#[wasm_bindgen]
pub struct GameSession {
    inner: Box<dyn DynSession>,
}

#[wasm_bindgen]
impl GameSession {
    /// 新开一局。seed 以两个 u32 传入（JS number 无法精确表示 u64）。
    #[wasm_bindgen(constructor)]
    pub fn new(
        game: &str,
        cfg_json: &str,
        seats: u8,
        seed_hi: u32,
        seed_lo: u32,
    ) -> Result<GameSession, JsError> {
        let seed = ((seed_hi as u64) << 32) | seed_lo as u64;
        Ok(GameSession {
            inner: open(game, cfg_json, seats, seed)?,
        })
    }

    /// 从快照恢复（主机重启续局 / 本地存档）。
    pub fn restore(game: &str, snapshot: &str) -> Result<GameSession, JsError> {
        Ok(GameSession {
            inner: reopen(game, snapshot)?,
        })
    }

    #[wasm_bindgen(js_name = gameId)]
    pub fn game_id(&self) -> String {
        self.inner.game_id().into()
    }

    pub fn seats(&self) -> u8 {
        self.inner.seats()
    }

    /// JSON：{"t":"seat","seat":0} | {"t":"done"} | ...
    #[wasm_bindgen(js_name = toAct)]
    pub fn to_act(&self) -> String {
        serde_json::to_string(&self.inner.to_act()).unwrap_or_default()
    }

    /// JSON：null | {"t":"win","seats":[0]} | {"t":"draw"}
    pub fn outcome(&self) -> String {
        serde_json::to_string(&self.inner.outcome()).unwrap_or_default()
    }

    /// seat < 0 = 旁观 / 法官视图。
    pub fn view(&self, seat: i32) -> String {
        self.inner.view_json(seat_opt(seat))
    }

    pub fn legal(&self, seat: u8) -> String {
        self.inner.legal_json(seat)
    }

    /// 成功返回事件 JSON；非法着法抛出错误码（字符串）。
    pub fn apply(&mut self, seat: u8, mv_json: &str) -> Result<String, JsError> {
        self.inner.apply_json(seat, mv_json).map_err(|e| err(e.0))
    }

    /// AI 为 seat 选一步（不落子）。level: 0 易 / 1 中 / 2 难。无可走时返回空串。
    #[wasm_bindgen(js_name = botMove)]
    pub fn bot_move(&mut self, seat: u8, level: u8) -> String {
        self.inner
            .bot_json(seat, Level::from_u8(level))
            .unwrap_or_default()
    }

    pub fn snapshot(&self) -> String {
        self.inner.snapshot_json()
    }
}

/// 构建信息（访客与主机比对，不一致则刷新）。
#[wasm_bindgen(js_name = buildInfo)]
pub fn build_info() -> String {
    format!(
        "{}+{}",
        env!("CARGO_PKG_VERSION"),
        option_env!("OFFTABLE_BUILD").unwrap_or("dev")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_roundtrip_native() {
        let mut s = open("gomoku", "", 2, 42).unwrap();
        assert_eq!(s.game_id(), "gomoku");
        s.apply_json(0, r#"{"x":7,"y":7}"#).unwrap();
        let snap = s.snapshot_json();
        let mut r = reopen("gomoku", &snap).unwrap();
        assert_eq!(r.view_json(None), s.view_json(None));
        assert!(r.apply_json(1, r#"{"x":7,"y":7}"#).is_err());
        let mv = r.bot_json(1, Level::Medium).unwrap();
        assert!(r.apply_json(1, &mv).is_ok());
    }
}
