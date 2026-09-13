# 012-install-client-retirement · 技术方案（plan）

> 导航：[spec.md](./spec.md) · [tasks.md](./tasks.md) · 返回 [MOC](../MOC.md)

- **状态**: reviewed
- **关联需求**: [spec.md](./spec.md)
- **最后更新**: 2026-09-13

## 1. 方案概述

纯删除型变更，无新增逻辑：沿「前端按钮 → 类型/枚举 → 派发映射 → 打包登记 → 脚本本体 → 文档」的引用链逐层摘除，每层摘除后以编译与测试兜底。唯一需要改写（而非删除）的是两份 tools README 的客户端指引与 menu.ps1 的菜单项。

## 2. 摘除清单（按层）

| 层 | 文件 | 动作 |
|----|------|------|
| 前端 | `apps/workbench/src/components/ToolsSection.tsx` | 删 `install_client` 条目（47-53 行一带）+ 头部注释同步 |
| 前端 | `apps/workbench/src/i18n/zh.ts` / `en.ts` | 删 `tools.installClient` / `tools.installClientDesc` 词条 |
| 前端 | `apps/workbench/src/types.ts` | ToolId 联合类型删 `"install_client"`（164 行一带） |
| Rust | `apps/workbench/src-tauri/src/scripts.rs` | 删 `Script::InstallClient`（枚举/文件名映射/visibility/timeout 分支）、`ToolKind::InstallClient`（枚举/映射）、相关测试（885/985 行一带） |
| Rust | `apps/workbench/src-tauri/src/commands.rs` | 101 行注释中的 install-client 表述同步 |
| 脚本 | `tools/sprint0/bin/menu.ps1` | 摘除 install-client 菜单项（132 行一带），保持剩余选项可用 |
| 打包 | `scripts/build.ps1` | `$ScriptSubset` 删 `install-client.ps1`（54 行一带） |
| 本体 | `tools/sprint0/bin/install-client.ps1` / `install-client.bat` | 删除 |
| 打包副本 | `apps/workbench/src-tauri/resources/bin/install-client.ps1` | 删除；`manifest.json` 同步删条目（T8 打包时 build.ps1 会再全量刷新，属双保险） |
| 文档 | `tools/README.md` | 删 install-client 两行工具表条目 |
| 文档 | `tools/sprint0/README.md` | 客户端章节改写：EasyTier 组网 + 浏览器访问 `https://<域名>`；删 bat 相关步骤 |
| 文档 | `specs/011-attack-surface-hardening/spec.md` | §6 开放问题回填：install-client 退役 → 本 spec 承接 |
| 文档 | `CHANGELOG.md` | Unreleased 区登记移除 |

## 3. 风险与对策

| 风险 | 影响 | 对策 |
|------|------|------|
| 删除枚举后残留测试/映射引用 | 编译失败 | cargo check/test 兜底，编译器即检索器 |
| menu.ps1 摘项后选项编号/分支错位 | 菜单点选错位 | 摘除后通读菜单分支一遍 + 语法解析检查（Parser::ParseFile） |
| 历史文档记载失真 | 无（历史事实保留） | spec 明确不追溯修改；011 开放问题做指向性回填 |

## 变更记录

| 日期 | 变更内容 | 原因 |
|------|----------|------|
| 2026-09-13 | 初稿 | 依 spec.md 与全仓引用盘点（2026-09-13 grep）定摘除清单 |
