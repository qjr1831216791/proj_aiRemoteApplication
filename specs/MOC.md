# Specs MOC（Map of Content · 内容地图）

> 所有 Spec 的唯一导航入口。**新建 Spec 必须在此登记，状态变更必须同步更新本文件。**

## 按状态导航

### 🟡 草稿（draft）
- （暂无）

### 🔵 已确认（reviewed）
- [002-network-profile](./002-network-profile/spec.md) — 网络防火墙归类反馈与调整：换网被拦截主动提示 + 用户决策改公用/专用（风险提示先行）

### 🟠 实现中（in-progress）
- [003-ddnsgo-password-reset](./003-ddnsgo-password-reset/spec.md) — ddns-go 密码重置：sprint0 交互式脚本 + APP 低频操作入口（密码不进 APP/IPC/日志）

### ✅ 已完成（done）
- [001-desktop-console](./001-desktop-console/spec.md) — Windows 桌面控制台：sprint0 能力 GUI 化 + 自启托管与一键收摊（控制面/数据面分离，Tauri 2；v0.2.0 已发布，GUI 手工验收回填记录见其 acceptance-manual.md）

### 📦 已归档（archived）
- 见 [archive/](./archive/)

## 按迭代导航

| 迭代 | Spec | 交付目标 |
|------|------|----------|
| Sprint 1 | [001-desktop-console](./001-desktop-console/spec.md) | Windows 桌面程序：一键启停三组件、自启托管（含菜单 7/8 回归）、显式收摊、sprint0 全能力入口（**v0.2.0** 已发布，2026-09-09） |
| Sprint 2 | [002-network-profile](./002-network-profile/spec.md) | 网络防火墙归类：被拦截主动反馈 + 公用/专用调整入口（用户决策，风险提示先行） |
| Sprint 2 | [003-ddnsgo-password-reset](./003-ddnsgo-password-reset/spec.md) | ddns-go 密码重置脚本与 APP 入口（交互式控制台，密码不落 APP） |

## 按主题导航

- 桌面工作台：[001-desktop-console](./001-desktop-console/spec.md)
- 网络环境：[002-network-profile](./002-network-profile/spec.md)
- 凭证维护：[003-ddnsgo-password-reset](./003-ddnsgo-password-reset/spec.md)

<!-- 功能域增多后按主题分区，例如：-->
<!-- - 用户体系：[001-xxx](./001-xxx/spec.md) · [003-xxx](./003-xxx/spec.md) -->
<!-- - 远程连接：[002-xxx](./002-xxx/spec.md) -->

## 相关文档

- [产品愿景](../docs/product.md) — 我们为什么做这个产品
- [开发宪法](../docs/constitution.md) — 跨 Spec 的不变约束
- [架构决策记录](../docs/adr/) — 技术选型的为什么
- [工作流细则](./README.md) — Spec 怎么写、怎么流转
- [变更日志](../CHANGELOG.md) — 版本说明与变更历史
