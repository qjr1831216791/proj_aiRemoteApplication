# 变更日志（Changelog）

> 本文件兼作**版本说明（Release Notes）与变更历史**：每个版本条目即该版本的发布说明。
> 格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [语义化版本 SemVer](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### Added
- Spec 驱动开发脚手架：`specs/`（MOC 导航、spec/plan/tasks 模板、归档区）、`docs/`（产品愿景、开发宪法、ADR）
- 调研报告 `docs/research/remote-solutions.md`：远程操作 Claude Code 的方案调研（CC Switch 约束、多端、公网化与自研评估）
- Sprint 0 试用部署指南 `docs/research/sprint0-cloudcli-lan-deploy.md`（局域网版，CloudCLI on Windows）
- Sprint 0 安装脚本 `tools/sprint0/`：`install-server.ps1`（服务端一键部署）与 `install-client.ps1`（客户端连通性验证 + 桌面快捷方式）；配套 `install-server.bat` / `install-client.bat` 双击启动器（服务端自动提权，客户端内嵌服务端地址）；`start-server.bat` 手动启动、`setup-autostart.ps1` 登录自启（含 `run-server-hidden.ps1` 幂等后台启动器）；`sprint0/README.md` 新手上路引导
- 环境与工具目录：`config/`（含 `.env.example`）、`scripts/`（生命周期命令）、`tools/`（辅助工具）
- `CLAUDE.md`：Spec 驱动敏捷开发规范与目录结构约定
- 根 `README.md`（文档地图）、`.gitattributes`（行尾统一）、`.editorconfig`（编辑行为基线）
- `.env.example` 固定于仓库根目录，与 `.env` 同位（`cp .env.example .env` 即用）
- Spec 索引唯一来源为 `specs/MOC.md`，移除 specs/README.md 中的重复索引；文档内链接统一为标准相对路径

## 分类约定

每个版本条目内按以下分类记录（无内容的分类省略）：

| 分类 | 含义 |
|------|------|
| Added | 新功能 |
| Changed | 对既有功能的变更（含行为差异） |
| Deprecated | 即将移除的功能 |
| Removed | 本版本移除的功能 |
| Fixed | 缺陷修复 |
| Security | 安全修复 |

## 版本规则

- 版本号 `主版本.次版本.修订号`（SemVer）。当前处于 **0.x 阶段**，接口与结构可能随迭代调整，主版本 1.0 表示对外接口稳定。
- 条目**面向使用者描述影响**，不写内部实现细节；实现过程与决策见对应 `specs/NNN-*/`。
- 每个版本条目末尾用一行声明交付范围：`交付: specs/001-xxx · specs/002-xxx`。

## 发布步骤

1. 确认 Unreleased 中变更对应的 Spec 均为 `done`（对照 `specs/MOC.md`）。
2. 确定版本号，将 `## [Unreleased]` 改为 `## [X.Y.Z] - YYYY-MM-DD`，并在其上方新建空的 `## [Unreleased]`。
3. 提交 `chore(release): vX.Y.Z` 并打标签 `git tag vX.Y.Z`。
4. 发布后更新 `docs/product.md` 路线图中对应里程碑的状态。
