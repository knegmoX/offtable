# P0 技术验证进度（2026-11-22）

## 已完成（本机可验证部分）
| 项 | 结果 |
|---|---|
| Rust 核心：Game/Bot trait、按座位视图、ChaCha8、类型擦除会话 | ✅ 单元测试 |
| 通用搜索：节点预算迭代加深 alpha-beta + 整数 softmax | ✅ 单元测试（取子游戏必胜、预算约束） |
| 五子棋规则（自由连五 / 恰好五连）+ 三档 AI | ✅ 13 项测试，含 proptest 随机对局、困难 ≥5/6 胜简单 |
| game.wasm 门面 | ✅ 157 KB 原始 / **55 KB brotli**（预算 120 KB） |
| Web 外壳（人机 / 同屏 / 开房）+ 访客页 | ✅ 外壳 JS 22.5 KB gzip、访客页 14.5 KB gzip（ES2017、无 WASM） |
| 与游戏无关的 relay（鉴权、令牌重连、帧缓存、限流、容量、踢人、Host/Origin 校验、访客页 CSP） | ✅ 7 项状态机测试 |
| e2e（系统 Chrome） | ✅ 8 项：人机+悔棋、同屏连五、刷新恢复存档、连点不串状态、主机+访客整局、访客重连、错误密钥/伪造 Host/外壳不对局域网暴露、1 主机 + 8 访客 |
| Tauri 2.12.7 Android 外壳 + App 内 relay（AssetResolver 托管访客页） | ✅ release 构建成功 |
| **Android arm64 包体** | **APK 7.48 MB（压缩后约 2.9 MB）**，.so 5.8 MB，16 KB 页对齐 ✅（预算：下载 ≤6 MB） |
| AI 时延（开发服务器） | 困难档 native p95 134 ms，wasm(V8) p95 157 ms；低端机需实测（门槛 p95 ≤1.5 s） |

测试安装包：`dist-test/offtable-p0-arm64.apk`（测试密钥签名，**不是**发版密钥）。

## 未完成 / 需要真机或账号
| 项 | 阻塞 |
|---|---|
| 冷启动、低端机 AI 时延、三种网络 × 20 次联机成功率、30 分钟稳定性 | 真机：见 [P0-DEVICE-CHECKLIST.md](P0-DEVICE-CHECKLIST.md) |
| iOS 构建 / TestFlight / iOS 包体 | Apple 开发者账号 + macOS runner |
| CI（构建 Android APK/AAB、iOS 模拟器包） | GitHub Actions（公开仓库）；上架内测轨需 Play / Apple 账号 |
| platform 插件（屏幕常亮、分享、评分、LocalOnlyHotspot、本地网络权限） | 未开始（下一步） |
| 主机恢复（快照落盘、App 重启续局）、App 切后台时通知访客"房主暂离" | 未开始（下一步） |
| 精简 classes.dex（去掉模板里的 Material 依赖，约可省 0.5–1 MB） | 优化项，非阻塞 |

## 过程中发现并已处理的问题
- 新局初始化期间的点击被静默丢弃 → 增加 loading 状态与 `aria-busy`。
- 渲染前连点使用过期状态，弹出误导性的"还没轮到你" → 同步锁 + 以引擎状态为准。
- Tauri `AssetResolver.get()` 找不到文件时静默回退到 `index.html` → relay 侧识别为 404，避免把主机外壳当作任意资源返回。
- Tauri 对 HTML 的 CSP 走响应头而非 meta → relay 为访客页单独下发 CSP。
- AI 思考消耗规则随机源会破坏"种子 + 着法"回放 → AI 使用独立随机源。
