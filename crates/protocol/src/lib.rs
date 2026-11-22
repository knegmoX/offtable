//! 局域网房间协议（JSON，`t` 为类型标签）。与 `web/src/net/protocol.ts` 保持一致。
//!
//! relay 与具体游戏无关：`mv`（访客意图）与 `msg`（主机下发）对 relay 都是不透明 JSON。
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTO: u16 = 1;

/// 访客 → relay
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum C2S {
    Hello {
        proto: u16,
        build: String,
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token: Option<String>,
        #[serde(default)]
        name: String,
    },
    Intent {
        cseq: u32,
        base: u64,
        mv: Value,
    },
    Ping {
        ts: u64,
    },
}

/// relay 自己生成、发给访客的消息（主机下发的消息由 relay 原样转发）。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum RelayToGuest {
    Welcome { token: String, build: String },
    Host { paused: bool },
    Reload,
    Pong { ts: u64 },
    Error { code: String },
}

/// relay → 主机
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum R2H {
    Room {
        urls: Vec<String>,
        key: String,
        build: String,
    },
    Joined {
        token: String,
        name: String,
        resumed: bool,
    },
    Left {
        token: String,
    },
    Intent {
        token: String,
        cseq: u32,
        base: u64,
        mv: Value,
    },
}

/// 主机 → relay
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum H2R {
    Send { to: String, msg: Value },
    Broadcast { msg: Value },
    Kick { token: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_matches_ts() {
        let h: C2S =
            serde_json::from_str(r#"{"t":"hello","proto":1,"build":"b","key":"k","name":"n"}"#).unwrap();
        assert!(matches!(h, C2S::Hello { token: None, .. }));
        let i: C2S = serde_json::from_str(r#"{"t":"intent","cseq":3,"base":7,"mv":{"x":1,"y":2}}"#).unwrap();
        assert_eq!(
            i,
            C2S::Intent {
                cseq: 3,
                base: 7,
                mv: serde_json::json!({"x":1,"y":2})
            }
        );
        let s = serde_json::to_string(&RelayToGuest::Host { paused: true }).unwrap();
        assert_eq!(s, r#"{"t":"host","paused":true}"#);
        let r = serde_json::to_string(&R2H::Left { token: "a".into() }).unwrap();
        assert_eq!(r, r#"{"t":"left","token":"a"}"#);
    }
}
