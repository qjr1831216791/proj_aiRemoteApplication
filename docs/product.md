# 产品愿景（Product）

> 面向"为什么做"。回答"给谁解决什么问题、凭什么赢"。
> 需求都应能追溯到本文件的某个目标；本文件变更需同步评审进行中的 Spec。

## 一句话定位

<AI 远程应用：一句话说清产品是什么、给谁用、核心价值是什么>

## 目标用户

| 用户群 | 核心诉求 | 使用场景 |
|--------|----------|----------|
| | | |

## 核心问题与价值主张

- <当前用户最大的痛点>
- <我们与现有方案的本质差异>

## 成功指标（North Star）

- <衡量产品成功的 1~3 个可量化指标>

## 路线图（粗粒度）

| 阶段 | 里程碑 | 对应 Spec |
|------|--------|-----------|
| Sprint 0（已完成 v0.1.0） | 远程方案调研 + CloudCLI 试用基建（局域网/HTTPS 全脚本化），试用观察中，结论回填调研报告后走三岔口决策 | 免 Spec（`tools/sprint0/` + `docs/research/`） |
| Sprint 1（已完成 v0.2.0） | 桌面控制台：sprint0 全能力 GUI 化（托盘常驻、一键启停、自启托管、可控退出、中英双语、双形态分发） | [001-desktop-console](../specs/001-desktop-console/spec.md) |
| Sprint 2+3（已完成 v0.3.0） | 网络环境反馈（002）· ddns-go 密码重置（003，已随 008 归档）· 内网穿透双通道与通道体检（004，已随 008 归档） | [002](../specs/002-network-profile/spec.md) · [003](../specs/archive/003-ddnsgo-password-reset/spec.md) · [004](../specs/archive/004-tunnel-access/spec.md) |
| Sprint 4 | 域名心跳检测（005，2026-09-11 需求方签收 done）· 傻瓜式装机向导（006，2026-09-10 验收 done） | [005](../specs/005-domain-heartbeat/spec.md) · [006](../specs/006-foolproof-install/spec.md) |
| Sprint 5 | 访问通道安全重构：EasyTier 私有组网替代 frp（零公网暴露），停用直连与穿透旧通道（2026-09-11 需求方签收 done） | [007-mesh-access](../specs/007-mesh-access/spec.md) |
| Sprint 6（已完成 v0.4.0） | 旧通道彻底移除：直连（ddns-go）与穿透（frp）从代码/分发层面退役，收敛为「局域网 IP 直访 + EasyTier 组网」双方案（2026-09-11 需求方签收 done；真机卸载脚本 `uninstall-legacy.ps1` 随包保留为可选动作） | [008-legacy-channel-removal](../specs/008-legacy-channel-removal/spec.md) |
| Sprint 7（已完成 v0.5.0） | v0.5.0 调优：组网网段冲突检测接线（spec 007 §6 决议落地）· 成员入网配置展示（密钥占位符 + 推荐 Android 客户端指引）· 发版校验脚本 release-check · 真机旧通道清理收尾（2026-09-11 验收 done） | [009-mesh-subnet-guard](../specs/009-mesh-subnet-guard/spec.md) |
| Sprint 8（已完成 v0.6.0） | 局域网边界收口：443 防火墙改「源网段 + TUN 接口」双条件白名单、3001 直访默认退役 + 12h 自动回落例外开关、程序级规则旁路加固（AC11）、443 归类告警退役（2026-09-13 验收 done——唯一放行组合 = 同一 WiFi + 专用网络 + 例外放行） | [010-lan-boundary-hardening](../specs/010-lan-boundary-hardening/spec.md) |

## 非目标（产品级）

- <明确不追逐的方向，防止战略摇摆>
