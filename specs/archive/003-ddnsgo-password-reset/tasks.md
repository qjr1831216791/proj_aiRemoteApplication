# 003-ddnsgo-password-reset · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../../MOC.md)

- **状态**: 进行中
- **最后更新**: 2026-09-09

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成

## 阶段 1: 脚本本体

- [x] T1 新增 `tools/sprint0/bin/reset-ddns-password.ps1`：停进程 → 展示用户名 → 二次输入新密码（不回显）→ `-resetPassword`（带 `-c`）→ 隐藏重启 → 端口复核；双语 `T()`/`-Lang`、UTF-8 BOM+CRLF（验收: AC1~AC6）
- [x] T2 `scripts/build.ps1` 的 `ScriptSubset` 增补新脚本（manifest 哈希自动重算）（验收: AC7 分发前提）

## 阶段 2: APP 入口

- [x] T3 Rust：`Script::ResetDdnsPassword`（file_name/visibility/timeout）+ `ToolKind::ResetDdnsPassword` + `interpret_exit` 臂 + 单测（tool_plan 可见性/参数/退出码）（验收: AC7、AC8）
- [x] T4 前端：`types.ts` ToolKind、i18n zh/en 词条、`ToolsSection.tsx` defs 增按钮（验收: AC7、AC8）

## 阶段 3: 验证与收尾

- [x] T5 自动化验证：`cargo test` 全绿、`npm run build` 通过、脚本 PowerShell 语法解析检查（验收: AC7/AC8 自动化部分）
  - 附：`-resetPassword` 语义经 yaml 副本实测（重置后进程自动退出、exit 0、哈希变化）
- [x] T6 手工验收（真实重置 E2E，须用户在控制台键入新密码）：AC1~AC6 逐条对照（AC1/AC2/AC3/AC4/AC5/AC6/AC9）
  - 2026-09-09 需求方真机验收通过（完整重置流程 + 新密码登录生效），原话"没有问题"
- [x] T7 对照 [spec.md](./spec.md) 逐条验证 AC 并勾选；CHANGELOG 登记；状态流转（验收: 全部）

## 完成标志（DoD 检查）

- [x] spec.md 中所有 AC 已逐条验证通过
- [x] 自动化测试全部通过
- [x] 相关文档已更新
- [x] 本文件全部任务勾选完毕
