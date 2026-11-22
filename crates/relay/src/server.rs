//! axum 网络层：把 WebSocket / HTTP 事件喂给 [`Room`]，执行它的输出。
//!
//! 安全边界：
//! - 只认自己的 Host 头（局域网 IP:端口 / 回环），Origin 必须与 Host 同源 → 防 DNS rebinding 与跨站 WS；
//! - 访客只能拿到访客页资源（guest.html、/g/*）；主机外壳页面只在开发模式且来自回环时提供；
//! - /__bridge（开发期主机桥）只接受回环连接且需要随机口令。

use crate::room::{ConnId, Out, Room, RoomConfig};
use axum::{
    Router,
    extract::{
        ConnectInfo, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use offtable_protocol::R2H;
use std::{
    borrow::Cow,
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use tokio::sync::{mpsc, oneshot};

/// 静态资源来源：开发期读目录，App 内由 Tauri AssetResolver 提供。
pub trait AssetSource: Send + Sync + 'static {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>>;
}

pub struct DirAssets(pub PathBuf);

impl AssetSource for DirAssets {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>> {
        let rel = Path::new(path);
        if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            return None; // 拒绝 ..、绝对路径
        }
        std::fs::read(self.0.join(rel)).ok().map(Cow::Owned)
    }
}

pub fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "json" | "webmanifest" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub struct ServerOptions {
    pub assets: Arc<dyn AssetSource>,
    /// Some(口令) → 启用 /__bridge（开发 / 自动化测试）。
    pub bridge_secret: Option<String>,
    /// 回环访问时额外提供主机外壳（index.html、assets/*）。
    pub dev_shell: bool,
}

enum GuestCmd {
    Text(String),
    Close,
}

struct Shared {
    room: Mutex<Room>,
    guests: Mutex<HashMap<ConnId, mpsc::UnboundedSender<GuestCmd>>>,
    host: Mutex<Option<(u64, mpsc::UnboundedSender<String>)>>,
    next_id: AtomicU64,
    started: Instant,
    allowed_hosts: Vec<String>,
    urls: Vec<String>,
    opts: ServerOptions,
}

impl Shared {
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn room_info(&self) -> R2H {
        let cfg = self.room.lock().unwrap().config().clone();
        R2H::Room {
            urls: self.urls.clone(),
            key: cfg.key,
            build: cfg.build,
        }
    }

    fn dispatch(&self, outs: Vec<Out>) {
        if outs.is_empty() {
            return;
        }
        let guests = self.guests.lock().unwrap();
        for o in outs {
            match o {
                Out::Guest(c, s) => {
                    if let Some(tx) = guests.get(&c) {
                        let _ = tx.send(GuestCmd::Text(s));
                    }
                }
                Out::Close(c) => {
                    if let Some(tx) = guests.get(&c) {
                        let _ = tx.send(GuestCmd::Close);
                    }
                }
                Out::Host(s) => {
                    if let Some((_, tx)) = self.host.lock().unwrap().as_ref() {
                        let _ = tx.send(s);
                    }
                }
            }
        }
    }

    fn attach_host(&self) -> (u64, mpsc::UnboundedReceiver<String>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        *self.host.lock().unwrap() = Some((id, tx));
        let info = self.room_info();
        let outs = self.room.lock().unwrap().host_connected(info);
        self.dispatch(outs);
        (id, rx)
    }

    fn detach_host(&self, id: u64) {
        let mut host = self.host.lock().unwrap();
        if host.as_ref().is_some_and(|(hid, _)| *hid == id) {
            *host = None;
            drop(host);
            let outs = self.room.lock().unwrap().host_closed();
            self.dispatch(outs);
        }
    }

    fn host_text(&self, text: &str) {
        let outs = self.room.lock().unwrap().host_text(text);
        self.dispatch(outs);
    }

    /// Host 必须是自己的地址；若有 Origin，必须与 Host 同源。
    fn check_origin(&self, headers: &HeaderMap) -> bool {
        let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
            return false;
        };
        if !self.allowed_hosts.iter().any(|a| a.eq_ignore_ascii_case(host)) {
            return false;
        }
        match headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()) {
            None => true,
            Some(origin) => origin.eq_ignore_ascii_case(&format!("http://{host}")),
        }
    }
}

/// 运行中的 relay。drop 或 [`RelayHandle::stop`] 即关闭。
pub struct RelayHandle {
    shared: Arc<Shared>,
    pub port: u16,
    pub urls: Vec<String>,
    pub key: String,
    shutdown: Option<oneshot::Sender<()>>,
    host_attachment: Mutex<Option<u64>>,
}

impl RelayHandle {
    /// App 内（Tauri IPC）：接管主机侧，返回 relay→主机 的消息流。
    pub fn attach_host(&self) -> mpsc::UnboundedReceiver<String> {
        let (id, rx) = self.shared.attach_host();
        *self.host_attachment.lock().unwrap() = Some(id);
        rx
    }

    /// App 内：主机 WebView 暂停 / 重载时调用，访客会看到"房主暂离"。
    pub fn detach_host(&self) {
        if let Some(id) = self.host_attachment.lock().unwrap().take() {
            self.shared.detach_host(id);
        }
    }

    /// 主机 → relay（一行 H2R JSON）。
    pub fn host_send(&self, text: &str) {
        self.shared.host_text(text);
    }

    pub fn online_guests(&self) -> usize {
        self.shared.room.lock().unwrap().online_guests()
    }

    pub fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for RelayHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

/// 本机的局域网 IPv4（私有地址），192.168.* 优先（家用 Wi-Fi / 手机热点最常见）。
pub fn lan_ipv4() -> Vec<Ipv4Addr> {
    let mut ips: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            IpAddr::V4(v4) if v4.is_private() => Some(v4),
            _ => None,
        })
        .collect();
    ips.sort_by_key(|ip| match ip.octets() {
        [192, 168, ..] => 0,
        [10, ..] => 1,
        _ => 2,
    });
    ips.dedup();
    ips
}

pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("OS RNG");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// 启动 relay（必须在 tokio 运行时内调用）。`port = 0` 表示随机端口。
pub async fn start(
    bind: IpAddr,
    port: u16,
    cfg: RoomConfig,
    opts: ServerOptions,
) -> std::io::Result<RelayHandle> {
    let listener = tokio::net::TcpListener::bind(SocketAddr::new(bind, port)).await?;
    let port = listener.local_addr()?.port();
    let ips = if bind.is_loopback() {
        Vec::new()
    } else {
        lan_ipv4()
    };
    let urls: Vec<String> = ips.iter().map(|ip| format!("http://{ip}:{port}")).collect();
    let mut allowed_hosts: Vec<String> = ips.iter().map(|ip| format!("{ip}:{port}")).collect();
    allowed_hosts.push(format!("127.0.0.1:{port}"));
    allowed_hosts.push(format!("localhost:{port}"));
    let key = cfg.key.clone();
    let room = Room::new(cfg, Box::new(|| random_hex(16)));
    let shared = Arc::new(Shared {
        room: Mutex::new(room),
        guests: Mutex::new(HashMap::new()),
        host: Mutex::new(None),
        next_id: AtomicU64::new(1),
        started: Instant::now(),
        allowed_hosts,
        urls: urls.clone(),
        opts,
    });
    let app = Router::new()
        .route("/ws", get(ws_guest))
        .route("/__bridge", get(ws_bridge))
        .fallback(get(static_file))
        .with_state(shared.clone());
    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });
    Ok(RelayHandle {
        shared,
        port,
        urls,
        key,
        shutdown: Some(tx),
        host_attachment: Mutex::new(None),
    })
}

async fn ws_guest(State(s): State<Arc<Shared>>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    if !s.check_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.max_message_size(16 * 1024)
        .on_upgrade(move |socket| guest_loop(s, socket))
}

async fn guest_loop(s: Arc<Shared>, mut socket: WebSocket) {
    let conn = s.next_id.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = mpsc::unbounded_channel();
    s.guests.lock().unwrap().insert(conn, tx);
    let outs = s.room.lock().unwrap().guest_connected(conn, s.now_ms());
    s.dispatch(outs);
    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(t))) => {
                    let outs = s.room.lock().unwrap().guest_text(conn, t.as_str(), s.now_ms());
                    s.dispatch(outs);
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            cmd = rx.recv() => match cmd {
                Some(GuestCmd::Text(text)) => {
                    if socket.send(Message::Text(text.into())).await.is_err() { break; }
                }
                Some(GuestCmd::Close) | None => {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
            },
        }
    }
    s.guests.lock().unwrap().remove(&conn);
    let outs = s.room.lock().unwrap().guest_closed(conn);
    s.dispatch(outs);
}

#[derive(serde::Deserialize)]
struct BridgeQuery {
    s: String,
}

async fn ws_bridge(
    State(s): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<BridgeQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let ok_secret = s.opts.bridge_secret.as_deref().is_some_and(|sec| sec == q.s);
    if !ok_secret || !peer.ip().is_loopback() || !s.check_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.on_upgrade(move |socket| bridge_loop(s, socket))
}

async fn bridge_loop(s: Arc<Shared>, mut socket: WebSocket) {
    let (id, mut rx) = s.attach_host();
    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(t))) => s.host_text(t.as_str()),
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            out = rx.recv() => match out {
                Some(text) => {
                    if socket.send(Message::Text(text.into())).await.is_err() { break; }
                }
                None => break,
            },
        }
    }
    s.detach_host(id);
}

async fn static_file(
    State(s): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if !s.check_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "guest.html" } else { path };
    let guest_ok = path == "guest.html" || path.starts_with("g/");
    let shell_ok = s.opts.dev_shell
        && peer.ip().is_loopback()
        && (path == "index.html" || path == "build.txt" || path.starts_with("assets/"));
    if !(guest_ok || shell_ok) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(body) = s.opts.assets.get(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut resp = (StatusCode::OK, body.into_owned()).into_response();
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime_for(path)));
    let cache = if path.ends_with(".html") || path == "build.txt" {
        "no-cache"
    } else {
        "public, max-age=31536000, immutable"
    };
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if path == "guest.html" {
        // 访客页：只允许同源脚本与回连本机 WebSocket（ws: 兼顾不认 'self'→ws 的老浏览器）
        h.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ws:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
            ),
        );
    }
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    resp
}
