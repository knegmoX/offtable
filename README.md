# Offtable（代号，品牌待定）

无广告、不用联网、不用账号的经典小游戏合集：体积小、单机可玩、和朋友面对面扫码联机（对方无需安装）。

> 状态：早期开发（P0 技术验证）。进度见 [docs/P0-STATUS.md](docs/P0-STATUS.md)。

## 架构一览

```
web/            Preact + Signals 界面（App 与 PWA 共用）+ 访客页（guest.html，ES2017，无 WASM）
crates/core     Game / Bot trait、按座位视图、ChaCha8 GameRng、类型擦除会话
crates/ai       节点预算的迭代加深 alpha-beta、整数 softmax 抽样
crates/games/*  每款游戏一个 crate（规则 + AI）
crates/wasm     game.wasm 门面：所有游戏共用一个 GameSession
crates/protocol 局域网协议（与 web/src/net/protocol.ts 保持一致）
crates/relay    与游戏无关的局域网中继：访客页托管、鉴权、令牌重连、缓存、限流
app/src-tauri   Tauri 2 薄外壳（Android / iOS），只负责启停 relay 与搬运消息
e2e/            Playwright（系统 Chrome）：单机、存档、1 主机 + 8 访客、重连、伪造 Host
```

规则与 AI 只在 WASM（Worker）里运行——App、PWA、局域网主机都是同一条路径；原生层与具体游戏无关。

## 本地构建

`scripts/lp` 以低优先级运行编译（CPU 上限 200% + nice + ionice），适合在共享 / 小规格机器上构建；工具链路径见 `scripts/env.sh`，均可用环境变量覆盖。

```bash
source scripts/env.sh                      # 工具链与 target 路径（可覆盖）
scripts/lp cargo test --workspace          # Rust 单元 / 性质测试
scripts/build-wasm.sh                      # → web/src/wasm/pkg（wasm-opt -Oz，打印 brotli 体积）
(cd web && pnpm install && pnpm typecheck && ../scripts/lp pnpm build)
scripts/lp cargo build -p offtable-relay --features dev-bridge --bin offtable-relay
scripts/lp npx --prefix e2e playwright test -c e2e/playwright.config.ts
```

本机试玩：`$CARGO_TARGET_DIR/debug/offtable-relay --static web/dist` 打印的 `host_url` 用浏览器打开即是主机页面；
同一局域网的手机扫码即可加入（访客页只对局域网开放，主机外壳只对回环开放）。

Android：`(cd app && ../scripts/lp pnpm tauri android build --apk --target aarch64 --ci)`（首次需 `scripts/setup-android.sh`）。

## 许可

代码以 [MPL-2.0](LICENSE) 开源。名称、图标与美术资源不在开源许可范围内。

## 约定

- 新游戏 = `crates/games/<id>`（实现 `Game` + `Bot`）+ `web/src/games/<id>` + 在 `crates/wasm` 注册 + e2e。
- 随机只用传入的 `GameRng`；禁止浮点、HashMap 迭代序、系统随机进入规则。
- AI 难度按节点预算，不按时间。
- 不引入任何第三方统计 / 广告 SDK。
