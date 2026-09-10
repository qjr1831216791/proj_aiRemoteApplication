# 002-network-profile · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 已完成
- **最后更新**: 2026-09-09

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成

## 阶段 1: Rust 监测与调整

- [x] T1 `network.rs`：类型 + `detect_spec` + `parse_net_status` + `needs_alert` + `set_category_params` + 单测（解析/告警分支/参数构造）（验收: AC1/AC2/AC8 逻辑）
- [x] T2 `NetMonitor` 轮询线程（15s、变化才发 `net://changed`、失败静默保持 last）+ 单测（探测序列驱动）（验收: AC3/AC4 逻辑）
- [x] T3 接线：`commands.rs` 增 `get_net_status`/`set_network_category`（校验 + runas + UAC 拒绝文案），`lib.rs` 注册命令与 monitor 启动（验收: AC5/AC6/AC7）

## 阶段 2: 前端网络环境卡

- [x] T4 `types.ts` + `api.ts`：NetStatus 类型、getNetStatus/setNetworkCategory/onNetChanged（验收: AC1）
- [x] T5 `MainView` 网络环境卡：告警提示条（三要素）+ 逐网络行（名称/归类 chip/切换按钮两步确认含风险文案）+ 样式 + zh/en 词条（验收: AC1/AC2/AC5/AC6/AC8 展示与交互）

## 阶段 3: 验证与收尾

- [x] T6 自动化验证：`cargo test` 全绿、`npm run build` 通过；探测命令真机实测（当前 Public 访客 WiFi 应触发告警）（验收: AC1 实证）
- [x] T7 手工验收（UAC 路径）：设为专用后提示消失/域名恢复访问、改回公用、失败提示（验收: AC3/AC5/AC6/AC7）
- [x] T8 对照 [spec.md](./spec.md) 逐条验证 AC 并勾选；CHANGELOG 登记；状态流转

## 完成标志（DoD 检查）

- [x] spec.md 中所有 AC 已逐条验证通过
- [x] 自动化测试全部通过
- [x] 相关文档已更新
- [x] 本文件全部任务勾选完毕
