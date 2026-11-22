//! 开发 / 自动化测试用的独立 relay：
//!   cargo run -p offtable-relay --features dev-bridge -- --static web/dist [--port 0] [--bind 0.0.0.0]
//! 启动后在 stdout 打印一行 JSON（端口、房间密钥、桥口令、主机页面地址）。
use offtable_relay::RoomConfig;
use offtable_relay::server::{DirAssets, ServerOptions, random_hex, start};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(arg(&args, "--static").unwrap_or_else(|| "web/dist".into()));
    let port: u16 = arg(&args, "--port").and_then(|p| p.parse().ok()).unwrap_or(0);
    let bind: IpAddr = arg(&args, "--bind")
        .and_then(|b| b.parse().ok())
        .unwrap_or([0, 0, 0, 0].into());
    let key = arg(&args, "--key").unwrap_or_else(|| random_hex(8));
    let secret = arg(&args, "--secret").unwrap_or_else(|| random_hex(16));
    let build = arg(&args, "--build")
        .or_else(|| {
            std::fs::read_to_string(dir.join("build.txt"))
                .ok()
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "dev".into());
    let opts = ServerOptions {
        assets: Arc::new(DirAssets(dir)),
        bridge_secret: Some(secret.clone()),
        dev_shell: true,
    };
    let relay = start(bind, port, RoomConfig::new(key.clone(), build.clone()), opts).await?;
    let info = serde_json::json!({
        "port": relay.port,
        "key": key,
        "secret": secret,
        "build": build,
        "urls": relay.urls,
        "host_url": format!("http://127.0.0.1:{}/index.html?bridge={}&game=gomoku", relay.port, secret),
    });
    println!("{info}");
    tokio::signal::ctrl_c().await?;
    relay.stop();
    Ok(())
}
