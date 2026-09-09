# 0002-control-data-plane-split

- 状态: accepted
- 日期: 2026-09-08
- 关联: specs/001-desktop-console/spec.md · specs/001-desktop-console/plan.md · docs/research/sprint0-cloudcli-lan-deploy.md

## 背景与问题

001-desktop-console 最初设计（v1）为"程序 = 服务生命周期所有者"：退出程序默认停止全部三组件（"程序在 = 服务在"）。2026-09-08 三方评审指出结构性缺陷：该产品价值主张是"手机遥控开发机"，把远程可达性挂在一个本地 GUI 进程上，GUI 成为单点故障——开发机上误退托盘或被清理软件连带杀掉，手机端立即断线且无法自救；且"退出即收摊"在崩溃（服务保留）、正常退出（服务停止）、OS 关机（Windows 仅给约 5s，spawn PowerShell 停三组件极易超时）三条路径上行为不一致。项目自己的调研文档也明确"CloudCLI 定位是基础设施，应常驻、与 claude 会话解耦"。

## 决策

**控制面 / 数据面分离**（Tailscale、Docker Desktop 同款模式）：三组件（数据面）经登录计划任务常驻、独立于桌面程序存活；桌面程序（控制面）只做状态监测、幂等启停、自启托管与操作入口。退出默认保留服务，「停止服务并退出」为托盘显式收摊操作（设置可反转默认）。自启通道一律用**登录计划任务**（`ExecutionTimeLimit Zero`），不用注册表 Run 键。

## 理由

- 消除单点故障：程序崩溃 / 被杀 / 升级重启期间远程链路照常（业界对照组：Tailscale 的 `tailscaled` Windows 服务常驻、GUI 仅是客户端，官方 issue 明确"退出 GUI 不停服务"；Docker Desktop 前后端分离同理）。
- 关机路径不再需要收摊：会话内用户级进程由系统统一终止，5 秒约束自然绕开。
- 计划任务 vs Run 键：Run 键受 StartupApproved 状态管控，任务管理器/优化软件可静默禁用（有实证案例）且无延迟选项；计划任务支持 `ExecutionTimeLimit Zero`（默认 72h 会杀常驻服务，sprint0 §9.5-⑩ 实测踩坑）、延迟触发，且与 sprint0 现有三条任务机制同构，可无缝"接管"而非"移除重建"。
- 放弃的备选：v1 的"程序独占生命周期 + Job Object(KILL_ON_JOB_CLOSE)"——一致性确实完美（任一死法都收摊），但单点故障缺陷无法弥补，评审后被需求方否决。

## 后果

- 收益：远程可达性不再依赖 GUI 存活；崩溃/关机语义自然正确；sprint0 存量三条自启任务被程序直接接管（幂等守卫兼容双通道），无需破坏性迁移。
- 代价：退出后服务可能继续运行，需要用户理解"显式收摊"操作（托盘菜单 + 可配置默认值缓解）；程序对"外部启动的服务"只能按端口 + 进程身份探测接管，无法拥有句柄级控制。
- 对实现的影响：三组件**不得**挂入程序的 kill-on-close Job Object；程序自启注册为自身专属登录计划任务（带 `--hidden` 参数），不走 `tauri-plugin-autostart`。
