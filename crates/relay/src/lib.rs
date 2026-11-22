//! offtable-relay —— 与具体游戏无关的局域网房间中继。
//!
//! 权威对局在主机 WebView 的 Worker（game.wasm）里；relay 只做连接、鉴权、令牌、
//! 路由、缓存与限流。原生层因此不随游戏数量增长，整条链路可以在 Linux 上测试。
#![forbid(unsafe_code)]

pub mod room;
#[cfg(feature = "server")]
pub mod server;

pub use room::{Out, Room, RoomConfig};
