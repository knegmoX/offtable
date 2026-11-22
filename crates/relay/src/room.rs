//! sans-IO 房间状态机：不碰网络、不读时钟（时间由调用方传入），便于确定性测试。
//!
//! 职责（与具体游戏无关）：鉴权（房间密钥、协议版本、构建号）、令牌与重连、
//! 按令牌路由主机消息、缓存最后的 lobby/view 帧（重连立即恢复）、限流与包长限制。

use offtable_protocol::{C2S, H2R, PROTO, R2H, RelayToGuest};
use serde_json::Value;
use std::collections::BTreeMap;

pub type ConnId = u64;

#[derive(Clone, Debug)]
pub struct RoomConfig {
    pub key: String,
    pub build: String,
    /// 同时在线的访客连接上限（热点通常 ≤8 台设备，留余量给观战）。
    pub max_conns: usize,
    /// 曾加入过的成员上限（含离线待重连）。
    pub max_members: usize,
    pub max_msg_bytes: usize,
    /// 令牌桶：每秒补充数、桶容量。
    pub rate_per_sec: u32,
    pub burst: u32,
}

impl RoomConfig {
    pub fn new(key: impl Into<String>, build: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            build: build.into(),
            max_conns: 12,
            max_members: 16,
            max_msg_bytes: 4096,
            rate_per_sec: 20,
            burst: 40,
        }
    }
}

/// 状态机的输出动作。
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    Guest(ConnId, String),
    Host(String),
    Close(ConnId),
}

struct Conn {
    token: Option<String>,
    /// 令牌桶余量（×1000 定点）
    allowance: u64,
    last_ms: u64,
}

struct Member {
    name: String,
    conn: Option<ConnId>,
    last_lobby: Option<String>,
    last_view: Option<String>,
}

pub struct Room {
    cfg: RoomConfig,
    conns: BTreeMap<ConnId, Conn>,
    members: BTreeMap<String, Member>,
    host_online: bool,
    token_gen: Box<dyn FnMut() -> String + Send>,
}

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn guest_err(conn: ConnId, code: &str) -> Out {
    Out::Guest(conn, json(&RelayToGuest::Error { code: code.into() }))
}

/// 常数时间比较，避免按字节提前返回泄露密钥前缀。
fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl Room {
    pub fn new(cfg: RoomConfig, token_gen: Box<dyn FnMut() -> String + Send>) -> Self {
        Self {
            cfg,
            conns: BTreeMap::new(),
            members: BTreeMap::new(),
            host_online: false,
            token_gen,
        }
    }

    pub fn config(&self) -> &RoomConfig {
        &self.cfg
    }

    pub fn host_online(&self) -> bool {
        self.host_online
    }

    pub fn online_guests(&self) -> usize {
        self.members.values().filter(|m| m.conn.is_some()).count()
    }

    pub fn guest_connected(&mut self, conn: ConnId, now_ms: u64) -> Vec<Out> {
        if self.conns.len() >= self.cfg.max_conns {
            return vec![guest_err(conn, "room_full"), Out::Close(conn)];
        }
        let full = self.cfg.burst as u64 * 1000;
        self.conns.insert(
            conn,
            Conn {
                token: None,
                allowance: full,
                last_ms: now_ms,
            },
        );
        Vec::new()
    }

    pub fn guest_closed(&mut self, conn: ConnId) -> Vec<Out> {
        let Some(c) = self.conns.remove(&conn) else {
            return Vec::new();
        };
        let Some(token) = c.token else { return Vec::new() };
        match self.members.get_mut(&token) {
            Some(m) if m.conn == Some(conn) => {
                m.conn = None;
                if self.host_online {
                    vec![Out::Host(json(&R2H::Left { token }))]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    fn rate_ok(&mut self, conn: ConnId, now_ms: u64) -> bool {
        let (rate, burst) = (self.cfg.rate_per_sec as u64, self.cfg.burst as u64 * 1000);
        let Some(c) = self.conns.get_mut(&conn) else {
            return false;
        };
        let elapsed = now_ms.saturating_sub(c.last_ms);
        c.allowance = (c.allowance + elapsed * rate).min(burst);
        c.last_ms = now_ms;
        if c.allowance >= 1000 {
            c.allowance -= 1000;
            true
        } else {
            false
        }
    }

    pub fn guest_text(&mut self, conn: ConnId, text: &str, now_ms: u64) -> Vec<Out> {
        if !self.conns.contains_key(&conn) {
            return Vec::new();
        }
        if text.len() > self.cfg.max_msg_bytes {
            return vec![guest_err(conn, "too_large"), Out::Close(conn)];
        }
        if !self.rate_ok(conn, now_ms) {
            return vec![guest_err(conn, "rate_limited")];
        }
        let Ok(msg) = serde_json::from_str::<C2S>(text) else {
            return Vec::new();
        };
        match msg {
            C2S::Hello {
                proto,
                build,
                key,
                token,
                name,
            } => self.hello(conn, proto, &build, &key, token, name),
            C2S::Intent { cseq, base, mv } => {
                let Some(token) = self.conns.get(&conn).and_then(|c| c.token.clone()) else {
                    return Vec::new();
                };
                if !self.host_online {
                    return vec![Out::Guest(conn, json(&RelayToGuest::Host { paused: true }))];
                }
                vec![Out::Host(json(&R2H::Intent {
                    token,
                    cseq,
                    base,
                    mv,
                }))]
            }
            C2S::Ping { ts } => vec![Out::Guest(conn, json(&RelayToGuest::Pong { ts }))],
        }
    }

    fn hello(
        &mut self,
        conn: ConnId,
        proto: u16,
        build: &str,
        key: &str,
        token: Option<String>,
        name: String,
    ) -> Vec<Out> {
        if self.conns.get(&conn).is_some_and(|c| c.token.is_some()) {
            return Vec::new(); // 重复 hello
        }
        if proto != PROTO {
            return vec![guest_err(conn, "bad_proto"), Out::Close(conn)];
        }
        if !ct_eq(key, &self.cfg.key) {
            return vec![guest_err(conn, "bad_key"), Out::Close(conn)];
        }
        if build != self.cfg.build {
            // 访客页由主机下发，版本不一致只可能是缓存旧页：让它刷新
            return vec![Out::Guest(conn, json(&RelayToGuest::Reload)), Out::Close(conn)];
        }
        let name: String = name.chars().filter(|c| !c.is_control()).take(24).collect();
        let mut out = Vec::new();
        let (token, resumed) = match token.filter(|t| self.members.contains_key(t)) {
            Some(t) => {
                let m = self.members.get_mut(&t).expect("checked");
                if let Some(old) = m.conn.replace(conn)
                    && old != conn
                {
                    // 同一令牌在别处打开：新连接接管，旧连接下线
                    out.push(guest_err(old, "replaced"));
                    out.push(Out::Close(old));
                    if let Some(c) = self.conns.get_mut(&old) {
                        c.token = None;
                    }
                }
                if !name.is_empty() {
                    m.name = name.clone();
                }
                (t, true)
            }
            None => {
                if self.members.len() >= self.cfg.max_members {
                    return vec![guest_err(conn, "room_full"), Out::Close(conn)];
                }
                let t = (self.token_gen)();
                self.members.insert(
                    t.clone(),
                    Member {
                        name: name.clone(),
                        conn: Some(conn),
                        last_lobby: None,
                        last_view: None,
                    },
                );
                (t, false)
            }
        };
        if let Some(c) = self.conns.get_mut(&conn) {
            c.token = Some(token.clone());
        }
        out.push(Out::Guest(
            conn,
            json(&RelayToGuest::Welcome {
                token: token.clone(),
                build: self.cfg.build.clone(),
            }),
        ));
        let m = &self.members[&token];
        // 重连：立即补发最后的 lobby / view，不必等主机
        if let Some(l) = &m.last_lobby {
            out.push(Out::Guest(conn, l.clone()));
        }
        if let Some(v) = &m.last_view {
            out.push(Out::Guest(conn, v.clone()));
        }
        if self.host_online {
            out.push(Out::Host(json(&R2H::Joined {
                token,
                name: m.name.clone(),
                resumed,
            })));
        } else {
            out.push(Out::Guest(conn, json(&RelayToGuest::Host { paused: true })));
        }
        out
    }

    /// 主机桥接上（含主机 WebView 重载后重连）。
    pub fn host_connected(&mut self, room: R2H) -> Vec<Out> {
        self.host_online = true;
        let mut out = vec![Out::Host(json(&room))];
        for (token, m) in &self.members {
            if let Some(conn) = m.conn {
                out.push(Out::Host(json(&R2H::Joined {
                    token: token.clone(),
                    name: m.name.clone(),
                    resumed: true,
                })));
                out.push(Out::Guest(conn, json(&RelayToGuest::Host { paused: false })));
            }
        }
        out
    }

    pub fn host_closed(&mut self) -> Vec<Out> {
        self.host_online = false;
        let paused = json(&RelayToGuest::Host { paused: true });
        self.members
            .values()
            .filter_map(|m| m.conn)
            .map(|c| Out::Guest(c, paused.clone()))
            .collect()
    }

    pub fn host_text(&mut self, text: &str) -> Vec<Out> {
        let Ok(msg) = serde_json::from_str::<H2R>(text) else {
            return Vec::new();
        };
        match msg {
            H2R::Send { to, msg } => self.deliver(&to, &msg).into_iter().collect(),
            H2R::Broadcast { msg } => {
                let tokens: Vec<String> = self.members.keys().cloned().collect();
                tokens.iter().filter_map(|t| self.deliver(t, &msg)).collect()
            }
            H2R::Kick { token } => {
                let mut out = Vec::new();
                if let Some(m) = self.members.remove(&token)
                    && let Some(conn) = m.conn
                {
                    out.push(guest_err(conn, "kicked"));
                    out.push(Out::Close(conn));
                    if let Some(c) = self.conns.get_mut(&conn) {
                        c.token = None;
                    }
                }
                out
            }
        }
    }

    fn deliver(&mut self, token: &str, msg: &Value) -> Option<Out> {
        let m = self.members.get_mut(token)?;
        let text = msg.to_string();
        match msg.get("t").and_then(Value::as_str) {
            Some("view") => m.last_view = Some(text.clone()),
            Some("lobby") => m.last_lobby = Some(text.clone()),
            _ => {}
        }
        m.conn.map(|c| Out::Guest(c, text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> Room {
        let mut n = 0;
        Room::new(
            RoomConfig::new("k3y", "b1"),
            Box::new(move || {
                n += 1;
                format!("tok{n}")
            }),
        )
    }

    fn hello(token: Option<&str>) -> String {
        let t = token.map(|t| format!(r#","token":"{t}""#)).unwrap_or_default();
        format!(r#"{{"t":"hello","proto":1,"build":"b1","key":"k3y","name":"amy"{t}}}"#)
    }

    fn room_info() -> R2H {
        R2H::Room {
            urls: vec!["http://192.168.1.2:4000".into()],
            key: "k3y".into(),
            build: "b1".into(),
        }
    }

    #[test]
    fn join_intent_route_and_cache() {
        let mut r = room();
        r.host_connected(room_info());
        assert!(r.guest_connected(1, 0).is_empty());
        let out = r.guest_text(1, &hello(None), 0);
        assert!(matches!(&out[0], Out::Guest(1, s) if s.contains("welcome") && s.contains("tok1")));
        assert!(
            out.iter().any(
                |o| matches!(o, Out::Host(s) if s.contains("joined") && s.contains("\"resumed\":false"))
            )
        );

        let out = r.guest_text(1, r#"{"t":"intent","cseq":1,"base":1,"mv":{"x":7,"y":7}}"#, 10);
        assert!(matches!(&out[0], Out::Host(s) if s.contains(r#""token":"tok1""#) && s.contains("intent")));

        // 主机下发 view → 送达并缓存
        let out = r.host_text(r#"{"t":"send","to":"tok1","msg":{"t":"view","sv":2,"view":{}}}"#);
        assert!(matches!(&out[0], Out::Guest(1, s) if s.contains(r#""sv":2"#)));

        // 断线重连：凭令牌恢复，立即收到缓存的 view
        r.guest_closed(1);
        r.guest_connected(2, 20);
        let out = r.guest_text(2, &hello(Some("tok1")), 20);
        assert!(
            out.iter()
                .any(|o| matches!(o, Out::Guest(2, s) if s.contains(r#""sv":2"#)))
        );
        assert!(
            out.iter()
                .any(|o| matches!(o, Out::Host(s) if s.contains("\"resumed\":true")))
        );
    }

    #[test]
    fn rejects_bad_key_proto_build() {
        let mut r = room();
        r.guest_connected(1, 0);
        let out = r.guest_text(
            1,
            r#"{"t":"hello","proto":1,"build":"b1","key":"nope","name":"x"}"#,
            0,
        );
        assert_eq!(out, vec![guest_err(1, "bad_key"), Out::Close(1)]);
        r.guest_connected(2, 0);
        let out = r.guest_text(
            2,
            r#"{"t":"hello","proto":9,"build":"b1","key":"k3y","name":"x"}"#,
            0,
        );
        assert_eq!(out[0], guest_err(2, "bad_proto"));
        r.guest_connected(3, 0);
        let out = r.guest_text(
            3,
            r#"{"t":"hello","proto":1,"build":"old","key":"k3y","name":"x"}"#,
            0,
        );
        assert!(matches!(&out[0], Out::Guest(3, s) if s.contains("reload")));
    }

    #[test]
    fn intent_before_hello_is_ignored_and_forged_tokens_get_new_identity() {
        let mut r = room();
        r.host_connected(room_info());
        r.guest_connected(1, 0);
        assert!(
            r.guest_text(1, r#"{"t":"intent","cseq":1,"base":1,"mv":{}}"#, 0)
                .is_empty()
        );
        let out = r.guest_text(1, &hello(Some("guessed-token")), 0);
        assert!(matches!(&out[0], Out::Guest(1, s) if s.contains("tok1")));
    }

    #[test]
    fn same_token_twice_newest_wins() {
        let mut r = room();
        r.host_connected(room_info());
        r.guest_connected(1, 0);
        r.guest_text(1, &hello(None), 0);
        r.guest_connected(2, 0);
        let out = r.guest_text(2, &hello(Some("tok1")), 0);
        assert!(out.contains(&Out::Close(1)));
        // 旧连接关闭不会把成员标记为离线
        assert!(r.guest_closed(1).is_empty());
        assert_eq!(r.online_guests(), 1);
    }

    #[test]
    fn rate_limit_and_size_limit() {
        let mut r = room();
        r.guest_connected(1, 0);
        let mut limited = 0;
        for _ in 0..100 {
            if r.guest_text(1, r#"{"t":"ping","ts":1}"#, 0)
                .iter()
                .any(|o| matches!(o, Out::Guest(_, s) if s.contains("rate_limited")))
            {
                limited += 1;
            }
        }
        assert_eq!(limited, 100 - 40);
        // 1 秒后补充 20 次
        let ok = (0..30)
            .filter(|_| {
                r.guest_text(1, r#"{"t":"ping","ts":1}"#, 1000)
                    .iter()
                    .any(|o| matches!(o, Out::Guest(_, s) if s.contains("pong")))
            })
            .count();
        assert_eq!(ok, 20);
        let big = format!(r#"{{"t":"ping","ts":1,"pad":"{}"}}"#, "x".repeat(5000));
        assert!(r.guest_text(1, &big, 2000).contains(&Out::Close(1)));
    }

    #[test]
    fn capacity_and_host_pause() {
        let mut r = room();
        for c in 0..12 {
            assert!(r.guest_connected(c, 0).is_empty());
        }
        assert!(r.guest_connected(99, 0).contains(&Out::Close(99)));
        // 主机未上线时 hello 会收到 paused
        let out = r.guest_text(0, &hello(None), 0);
        assert!(
            out.iter()
                .any(|o| matches!(o, Out::Guest(0, s) if s.contains(r#""paused":true"#)))
        );
        let out = r.host_connected(room_info());
        assert!(
            out.iter()
                .any(|o| matches!(o, Out::Guest(0, s) if s.contains(r#""paused":false"#)))
        );
        let out = r.host_closed();
        assert!(
            out.iter()
                .any(|o| matches!(o, Out::Guest(0, s) if s.contains(r#""paused":true"#)))
        );
    }

    #[test]
    fn kick_removes_member() {
        let mut r = room();
        r.host_connected(room_info());
        r.guest_connected(1, 0);
        r.guest_text(1, &hello(None), 0);
        let out = r.host_text(r#"{"t":"kick","token":"tok1"}"#);
        assert!(out.contains(&Out::Close(1)));
        r.guest_connected(2, 0);
        // 被踢的令牌失效，重连拿到新身份
        let out = r.guest_text(2, &hello(Some("tok1")), 0);
        assert!(matches!(&out[0], Out::Guest(2, s) if s.contains("tok2")));
    }
}
