# 0001-tauri2-desktop-stack

- 状态: accepted
- 日期: 2026-09-08
- 关联: specs/001-desktop-console/plan.md

## 背景与问题

Sprint 1 要为 sprint0 的三组件（CloudCLI / Caddy / ddns-go）交付一个 Windows 桌面控制台：托盘常驻、开机自启、单实例、以子进程方式编排 PowerShell 脚本、从非提权进程触发 UAC 提权、蓝白清爽 UI，且必须轻量（安装包 ≤15MB、常驻内存含 WebView2 ≤150MB）。单人开发、AI 辅助编码、Rust 经验有限。这是项目首个图形界面技术栈选型，影响全部后续桌面端工作。

## 决策

桌面端采用 **Tauri 2**（Rust 核心 + 系统 WebView2 渲染 + Web 前端），前端用 Vite + TypeScript + Preact 手写样式，不引入 UI 组件库。

## 理由

2026-09-08 专项评审（检索 2025-2026 年资料）结论"成立但有条件"：

- 六项硬性能力（托盘 / 自启 / 单实例 / 子进程管理 / 退出清理 / UAC 触发）全部是 Tauri 2 官方一等能力；托盘已入核心，single-instance / autostart 为官方插件。
- 实测体积：同类应用 NSIS 安装包 8.6 MiB（对照 Electron 244 MiB）；WebView2 在 Win10 1803+/Win11 预装，零额外运行时。
- AI 训练语料充足（2024-10 GA 至今近两年），单人 + AI 辅助开发可控。

放弃的备选：

- **Electron**：体积 / 内存全面劣势，直接违反轻量目标。
- **.NET 8 WPF**：真正的第二名（C# 生态对单人最友好、Win32 调用更顺手），但自包含发布 100MB+ 或要求预装运行时，XAML 手写蓝白主题效率低。**保留为 Plan B**：Rust 侧进程管理两周内无法推进即切换，UI 层可平移。
- **Wails**：v3 仍 beta；**Avalonia**：Windows-only 场景无优势；**WinUI 3**：部署链复杂；**Photino 等轻壳**：语料少，AI 辅助质量差。

## 后果

- 收益：体积内存达标、官方插件覆盖需求、CSS 实现主题最省力。
- 代价：Rust 学习曲线（集中在 Win32 交互处）；未签名分发有 SmartScreen/Defender 误报（自用定位可接受，安装文档写引导；公开分发再议签名）。
- 约束落地：子进程一律 `CREATE_NO_WINDOW` + `-NoProfile -NonInteractive` + 显式管道 stdio（不用 `-WindowStyle Hidden`，避免 conhost 闪烁）；single-instance 必须是首个注册的插件；窗口 `visible: false` 创建、页面就绪后 show（规避 WebView2 首帧白屏，tauri#5170）；不用已归档的 `runas` crate，UAC 用 `windows` crate 直调 `ShellExecuteW(verb="runas")`；**不使用** `tauri-plugin-autostart`（其走注册表 Run 键，有被安全软件静默禁用风险，见 ADR-0002 的自启通道决策）。
