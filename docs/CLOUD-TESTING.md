# 云端测试方案（核查于 2026-11-22）

结论先行：**没有任何云测平台支持"两台云真机在同一局域网互访"**。因此分层：协议与 relay 在 CI 里自动化；
真机兼容性交给云真机平台；Wi-Fi / 热点的真实行为（AP 隔离、移动数据回落、厂商 Wi-Fi 切换）最后用实体手机验收。

## 第 1 层：每次提交，免费，全自动（GitHub Actions，公开仓库标准 runner 免费，含 macOS）
| 测试 | 做法 | 状态 |
|---|---|---|
| Rust 单元 / 性质测试、clippy、fmt | ubuntu-24.04 | ✅ 已上线 |
| Web 构建 + 体积预算 + e2e（1 主机 + 8 访客、重连、伪造 Host） | ubuntu-24.04 + 预装 Chrome | ✅ 已上线 |
| Android：分 ABI 的 APK + AAB、包体门禁 | ubuntu-24.04（预装 SDK，NDK 按需安装） | ✅ 已上线 |
| iOS：模拟器构建（未签名） | macos-26（Xcode 26.6） | ✅ 已上线 |
| **Android 模拟器冒烟**：安装、冷启动、截图、崩溃检查 | ubuntu-24.04 开 KVM（udev 规则；GitHub 称"技术上可行、非官方支持"）+ android-emulator-runner | 待做 |
| **App 内 relay 端到端**：模拟器里的 App 开房，`adb forward` 后由 runner 上的 Playwright 扮演访客整局对弈 | 同上 | 待做（最接近"真机局域网"的自动化） |
| **iOS 模拟器冒烟**：`xcrun simctl` 安装、启动、截图 | macos-26 | 待做 |

## 第 2 层：海外云真机（按需）
| 服务 | 适用 | 费用 | 备注 |
|---|---|---|---|
| Firebase Test Lab | Android 真机/虚拟机 Robo 自动遍历、截图、性能、崩溃；iOS XCTest | Spark：每天 10 次虚拟 + 5 次真机免费；Blaze：虚拟 $1/设备时、真机 $5/设备时 | gcloud CLI 可接 CI；建议用 Workload Identity Federation，不存服务账号密钥 |
| Samsung Remote Test Lab | 最新 Galaxy 真机远程手动操作 | 免费（时长配额未公开） | 无自动化 API |
| BrowserStack 开源计划 | **访客页**在真实手机浏览器上的兼容性（Safari / Samsung Internet / Chrome） | 开源项目免费（Live + Automate 浏览器） | **不含** App Automate；用 BrowserStack Local 隧道访问 CI 里起的 relay（WebSocket 支持待验证） |
| AWS Device Farm | 指定真机 | $0.17/设备分钟，新用户 1000 分钟试用 | 多设备组网能力未核实 |

## 第 3 层：国内 ROM（WebView 兼容性：MIUI/HyperOS、ColorOS、OriginOS、HarmonyOS）
| 服务 | 适用 | 费用 |
|---|---|---|
| **阿里云 EMAS 移动测试（原 MQC）** | — | **已不在产品目录，文档 404，视为下线**（官方产品列表与文档两处核实） |
| 腾讯 WeTest 兼容性测试 | Top100/Top300 机型安装、启动、遍历，约 1 小时出报告 | 实名后**首次免费**；标准 ¥30/台次；深度 ¥3,999/次 |
| 腾讯 WeTest 云手机 | 1000+ 真机远程（ADB）、手动测性能 | 价格未公示（兼容性套餐含 300 分钟） |
| 华为 AGC 云调试 | 华为机型远程调试 | 每天免费 360 分钟（以 HarmonyOS 为主） |
| Testin 云测 | 远程真机、兼容性测试，有"多机联动"产品 | 报价制；多机能否同网未公开 |

## 第 4 层：实体手机验收（最后一步）
见 [P0-DEVICE-CHECKLIST.md](P0-DEVICE-CHECKLIST.md)：三种网络 × 20 次联机、锁屏恢复、30 分钟稳定性、低端机冷启动与 AI 时延。

## 建议顺序
1. 第 1 层补齐模拟器冒烟 + App 内 relay 端到端（免费，自动）。
2. WeTest 首次免费兼容性测试跑一次 APK（覆盖国产 ROM WebView）。
3. Firebase Test Lab Spark 免费额度接入 CI（每晚 1 次 Robo）。
4. 实体手机按检查表验收。
