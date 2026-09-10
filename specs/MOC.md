# Specs MOC（Map of Content · 内容地图）

> 所有 Spec 的唯一导航入口。**新建 Spec 必须在此登记，状态变更必须同步更新本文件。**

## 按状态导航

### 🟡 草稿（draft）
- [007-mesh-access](./007-mesh-access/spec.md) — 私有组网访问通道：EasyTier（secure-mode）替代 frp 零公网暴露 + 停用直连/穿透旧通道（2026-09-10 立项，调研依据 [docs/research/secure-access-alternatives.md](../docs/research/secure-access-alternatives.md)）

### 🔵 已确认（reviewed）
- （暂无）
<!-- 006-foolproof-install 已实际进入实现（自动化全绿、真机验收中），状态字段与本节均按 in-progress 归位，不再重复登记于本区 -->

### 🟠 实现中（in-progress）
- [005-domain-heartbeat](./005-domain-heartbeat/spec.md) — 域名心跳检测：60s 周期探测 + 地址区红绿标记（2 次防抖）+ 失败分类 + 设置开关（核心已实现，AC 待真机验收）
- [006-foolproof-install](./006-foolproof-install/spec.md) — 傻瓜式装机向导：五阶段检测驱动（基础/腾讯云前置/HTTPS 栈/访问通道/收尾），断点续跑+办后校验；TLS 改 Caddy tencentcloud 插件自治（ADR-0003）；低频栏瘦身（自动化 164 项全绿，真机/AC 验收进行中）

### ✅ 已完成（done）
- [001-desktop-console](./001-desktop-console/spec.md) — Windows 桌面控制台：sprint0 能力 GUI 化 + 自启托管与一键收摊（控制面/数据面分离，Tauri 2；v0.2.0 已发布，GUI 手工验收回填记录见其 acceptance-manual.md）
- [002-network-profile](./002-network-profile/spec.md) — 网络防火墙归类反馈与调整：换网被拦截主动提示 + 用户决策改公用/专用（风险提示先行）
- [003-ddnsgo-password-reset](./003-ddnsgo-password-reset/spec.md) — ddns-go 密码重置：sprint0 交互式脚本 + APP 低频操作入口（密码不进 APP/IPC/日志）
- [004-tunnel-access](./004-tunnel-access/spec.md) — 内网穿透双通道：直连（DDNS，默认）⇄ 穿透（SakuraFrp 海外节点）一键切换（DNS 自动暂停/激活）、frpc 随包分发 + 守护自愈、通道体检；手工验收记录 `acceptance-manual.md`（3 项日常观察遗留已登记）

### 📦 已归档（archived）
- 见 [archive/](./archive/)

## 按迭代导航

| 迭代 | Spec | 交付目标 |
|------|------|----------|
| Sprint 1 | [001-desktop-console](./001-desktop-console/spec.md) | Windows 桌面程序：一键启停三组件、自启托管（含菜单 7/8 回归）、显式收摊、sprint0 全能力入口（**v0.2.0** 已发布，2026-09-09） |
| Sprint 2 | [002-network-profile](./002-network-profile/spec.md) | 网络防火墙归类：被拦截主动反馈 + 公用/专用调整入口（用户决策，风险提示先行） |
| Sprint 2 | [003-ddnsgo-password-reset](./003-ddnsgo-password-reset/spec.md) | ddns-go 密码重置脚本与 APP 入口（交互式控制台，密码不落 APP） |
| Sprint 3 | [004-tunnel-access](./004-tunnel-access/spec.md) | 内网穿透双通道（直连⇄穿透一键切换、DNS 自动暂停/激活、frpc 随包+守护自愈、通道体检）（2026-09-10 验收 done） |
| Sprint 3 | [005-domain-heartbeat](./005-domain-heartbeat/spec.md) | 域名心跳检测（红绿标记 + 失败分类 + 自愈联动；核心已实现，AC 收口随 Sprint 4） |
| Sprint 4 | [006-foolproof-install](./006-foolproof-install/spec.md) | 傻瓜式装机向导（APP 编排 + 专项脚本 + Caddy 插件化证书，in-progress） |
| Sprint 5 | [007-mesh-access](./007-mesh-access/spec.md) | 私有组网访问通道（EasyTier 替代 frp + 停用直连/穿透旧通道，draft） |

## 按主题导航

- 桌面工作台：[001-desktop-console](./001-desktop-console/spec.md)
- 网络环境：[002-network-profile](./002-network-profile/spec.md) · [007-mesh-access](./007-mesh-access/spec.md)
- 凭证维护：[003-ddnsgo-password-reset](./003-ddnsgo-password-reset/spec.md)
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
