# Specs MOC（Map of Content · 内容地图）

> 所有 Spec 的唯一导航入口。**新建 Spec 必须在此登记，状态变更必须同步更新本文件。**

## 按状态导航

### 🟡 草稿（draft）
- （暂无）

### 🔵 已确认（reviewed）
- [010-lan-boundary-hardening](./010-lan-boundary-hardening/spec.md) — 局域网边界收口：443 防火墙改「源网段 + TUN 接口」双条件白名单、3001 直访默认退役 + 12h 自动回落例外开关（2026-09-12 安全审查立项，同日需求方确认 reviewed）

### 🟠 实现中（in-progress）
- （暂无）

### ✅ 已完成（done）
- [001-desktop-console](./001-desktop-console/spec.md) — Windows 桌面控制台：sprint0 能力 GUI 化 + 自启托管与一键收摊（控制面/数据面分离，Tauri 2；v0.2.0 已发布，GUI 手工验收回填记录见其 acceptance-manual.md）
- [002-network-profile](./002-network-profile/spec.md) — 网络防火墙归类反馈与调整：换网被拦截主动提示 + 用户决策改公用/专用（风险提示先行）
- [006-foolproof-install](./006-foolproof-install/spec.md) — 傻瓜式装机向导：五阶段检测驱动（基础/腾讯云前置/HTTPS 栈/访问通道/收尾），断点续跑+办后校验；TLS 改 Caddy tencentcloud 插件自治（ADR-0003）；低频栏瘦身（2026-09-10 验收 done）
- [005-domain-heartbeat](./005-domain-heartbeat/spec.md) — 域名心跳检测：60s 周期探测 + 地址区红绿标记（2 次防抖）+ 失败分类 + 设置开关（2026-09-11 需求方签收 done）
- [007-mesh-access](./007-mesh-access/spec.md) — 私有组网访问通道：EasyTier（legacy 模式，network_secret 派生加密）替代 frp 零公网暴露 + 停用直连/穿透旧通道（easytier-core 以 Windows 服务承载；2026-09-11 需求方签收 done——AC1/AC2 联调实测，签收依据见其 acceptance-manual §3；随 008 一并发布 v0.4.0）
- [008-legacy-channel-removal](./008-legacy-channel-removal/spec.md) — 旧通道彻底移除：直连（ddns-go）与穿透（frp）从代码/分发、真机三层面退役，收敛为「局域网 IP 直访 + EasyTier 组网」双方案；真机卸载编排 `uninstall-legacy.ps1` 随包保留为可选动作（2026-09-11 需求方签收 done，v0.4.0 发布；真机卸载已于同日经 spec 009 T9 补执行实证）
- [009-mesh-subnet-guard](./009-mesh-subnet-guard/spec.md) — v0.5.0 调优：组网网段冲突检测接线（spec 007 §6 决议落地）+ 成员入网 TOML 配置展示（密钥占位符）+ 发版校验脚本 release-check + 真机旧通道清理收尾（2026-09-11 验收 done——AC5~AC8/AC11 实测、AC1~AC4/AC9/AC10 自动化覆盖+需求方签收，记录见其 acceptance-manual）

### 📦 已归档（archived · 交付物已退役或被取代）
- [003-ddnsgo-password-reset](./archive/003-ddnsgo-password-reset/spec.md) — ddns-go 密码重置（2026-09-11 随 008 归档：ddns-go 组件退役，功能无承载对象）
- [004-tunnel-access](./archive/004-tunnel-access/spec.md) — 内网穿透双通道（2026-09-11 随 008 归档：穿透通道退役，被 007 组网取代；工程经验由组网通道继承）
- 其余见 [archive/](./archive/)

## 按迭代导航

| 迭代 | Spec | 交付目标 |
|------|------|----------|
| Sprint 1 | [001-desktop-console](./001-desktop-console/spec.md) | Windows 桌面程序：一键启停三组件、自启托管（含菜单 7/8 回归）、显式收摊、sprint0 全能力入口（**v0.2.0** 已发布，2026-09-09） |
| Sprint 2 | [002-network-profile](./002-network-profile/spec.md) | 网络防火墙归类：被拦截主动反馈 + 公用/专用调整入口（用户决策，风险提示先行） |
| Sprint 2 | [003-ddnsgo-password-reset](./archive/003-ddnsgo-password-reset/spec.md) | ddns-go 密码重置脚本与 APP 入口（2026-09-11 随 008 归档：功能退役） |
| Sprint 3 | [004-tunnel-access](./archive/004-tunnel-access/spec.md) | 内网穿透双通道（2026-09-10 验收 done；2026-09-11 随 008 归档：通道退役） |
| Sprint 3 | [005-domain-heartbeat](./005-domain-heartbeat/spec.md) | 域名心跳检测（红绿标记 + 失败分类 + 设置开关；2026-09-11 需求方签收 done） |
| Sprint 4 | [006-foolproof-install](./006-foolproof-install/spec.md) | 傻瓜式装机向导（APP 编排 + 专项脚本 + Caddy 插件化证书；2026-09-10 验收 done） |
| Sprint 5 | [007-mesh-access](./007-mesh-access/spec.md) | 私有组网访问通道（EasyTier 替代 frp + 停用直连/穿透旧通道；2026-09-11 需求方签收 done） |
| Sprint 6 | [008-legacy-channel-removal](./008-legacy-channel-removal/spec.md) | 旧通道彻底移除（直连+frp 退役，双方案收敛；2026-09-11 需求方签收 done，随 v0.4.0 发布） |
| Sprint 7 | [009-mesh-subnet-guard](./009-mesh-subnet-guard/spec.md) | v0.5.0 调优：网段冲突检测接线 + 成员入网配置展示 + 发版校验脚本 + 真机旧通道清理（2026-09-11 验收 done） |
| Sprint 8 | [010-lan-boundary-hardening](./010-lan-boundary-hardening/spec.md) | v0.6.0 候选：局域网边界收口（443/3001 源网段白名单 + 直访默认退役 + 例外开关） |

## 按主题导航

- 桌面工作台：[001-desktop-console](./001-desktop-console/spec.md)
- 网络环境：[002-network-profile](./002-network-profile/spec.md) · [007-mesh-access](./007-mesh-access/spec.md) · [008-legacy-channel-removal](./008-legacy-channel-removal/spec.md) · [010-lan-boundary-hardening](./010-lan-boundary-hardening/spec.md)
- 装机体验：[006-foolproof-install](./006-foolproof-install/spec.md)

<!-- 功能域增多后按主题分区，例如：-->
<!-- - 用户体系：[001-xxx](./001-xxx/spec.md) · [003-xxx](./003-xxx/spec.md) -->
<!-- - 远程连接：[002-xxx](./002-xxx/spec.md) -->

## 相关文档

- [产品愿景](../docs/product.md) — 我们为什么做这个产品
- [开发宪法](../docs/constitution.md) — 跨 Spec 的不变约束
- [架构决策记录](../docs/adr/) — 技术选型的为什么
- [工作流细则](./README.md) — Spec 怎么写、怎么流转
- [变更日志](../CHANGELOG.md) — 版本说明与变更历史
