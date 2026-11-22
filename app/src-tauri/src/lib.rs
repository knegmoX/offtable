//! Tauri 薄胶水：只负责启动/停止 relay，并在 relay 与主机 WebView 之间搬运消息。
//! 规则与 AI 都在 WebView 的 Worker（game.wasm）里，这里与具体游戏无关。

use offtable_relay::RoomConfig;
use offtable_relay::server::{AssetSource, RelayHandle, ServerOptions, random_hex, start};
use std::borrow::Cow;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager as _, Runtime, State};

/// 固定端口优先：主机 App 重启后访客的旧地址与令牌仍然有效（主机恢复）。
const PREFERRED_PORT: u16 = 47800;

/// 访客页资源直接来自 App 包内的同一份 Web 构建（AssetResolver）。
struct BundledAssets<R: Runtime>(AppHandle<R>);

impl<R: Runtime> AssetSource for BundledAssets<R> {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>> {
        let asset = self.0.asset_resolver().get(path.to_string())?;
        // AssetResolver 找不到文件时会静默回退到 index.html（主机外壳）：识别并当作 404
        if !path.ends_with(".html") && asset.mime_type.contains("html") {
            return None;
        }
        Some(Cow::Owned(asset.bytes))
    }
}

#[derive(Default)]
struct Relay(Mutex<Option<RelayHandle>>);

#[tauri::command]
async fn relay_start<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Relay>,
    on_event: Channel<String>,
) -> Result<serde_json::Value, String> {
    if let Some(old) = state.0.lock().unwrap().take() {
        old.stop();
    }
    let assets = Arc::new(BundledAssets(app.clone()));
    let build = assets
        .get("build.txt")
        .map(|b| String::from_utf8_lossy(&b).trim().to_string())
        .unwrap_or_else(|| "dev".into());
    let key = random_hex(8);
    let opts = || ServerOptions {
        assets: assets.clone(),
        bridge_secret: None,
        dev_shell: false,
    };
    let any = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
    let relay = match start(
        any,
        PREFERRED_PORT,
        RoomConfig::new(key.clone(), build.clone()),
        opts(),
    )
    .await
    {
        Ok(r) => r,
        Err(_) => start(any, 0, RoomConfig::new(key.clone(), build.clone()), opts())
            .await
            .map_err(|e| e.to_string())?,
    };
    let mut rx = relay.attach_host();
    tauri::async_runtime::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if on_event.send(msg).is_err() {
                break;
            }
        }
    });
    let info = serde_json::json!({ "urls": relay.urls, "key": key, "port": relay.port });
    *state.0.lock().unwrap() = Some(relay);
    Ok(info)
}

#[tauri::command]
fn relay_send(state: State<'_, Relay>, msg: String) {
    if let Some(r) = state.0.lock().unwrap().as_ref() {
        r.host_send(&msg);
    }
}

#[tauri::command]
fn relay_stop(state: State<'_, Relay>) {
    if let Some(r) = state.0.lock().unwrap().take() {
        r.stop();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Relay::default())
        .invoke_handler(tauri::generate_handler![relay_start, relay_send, relay_stop])
        .run(tauri::generate_context!())
        .expect("error while running offtable");
}
