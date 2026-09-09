# 变更日志（Changelog）

> 本文件兼作**版本说明（Release Notes）与变更历史**：每个版本条目即该版本的发布说明。
> 格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [语义化版本 SemVer](https://semver.org/lang/zh-CN/)。

## [Unreleased]

桌面控制台（Spec 001）开工：Tauri 2 工程骨架与桌面常驻形态落地（里程碑 1 前半）。

交付进行中: specs/001-desktop-console

### Added
- `apps/workbench/` 桌面工作台工程（Tauri 2 + Preact + TypeScript + Vite）：蓝白主题骨架、zh/en 双语词条与切换（跟随系统显示语言）、页面就绪后显形的主窗口
- 桌面常驻形态：系统托盘六项菜单（随系统显示语言中英双语）、关闭主窗最小化到托盘（程序继续常驻）、退出钩子骨架（退出不携带任何服务进程）
- 单实例约束：同一会话重复启动自动激活已有主窗；跨用户会话重复启动给出明确提示后退出

## [0.1.0] - 2026-09-08

Sprint 0 试用基建：从远程方案调研到 CloudCLI 部署全脚本化（局域网 + HTTPS/域名），手机可"装成 App"遥控开发机上的 Claude Code；部署过程沉淀为知识库。

交付: Sprint 0 试用工具与文档（免 Spec：试用基建与配置，依据 CLAUDE.md 开发铁律之例外条款）

### Added
- Spec 驱动开发脚手架：`specs/`（MOC 导航、spec/plan/tasks 模板、归档区）、`docs/`（产品愿景、开发宪法、ADR）
- 调研报告 `docs/research/remote-solutions.md`：远程操作 Claude Code 的方案调研（CC Switch 约束、多端、公网化与自研评估），含决策记录与试用记录
- Sprint 0 试用部署指南 `docs/research/sprint0-cloudcli-lan-deploy.md`：局域网版全流程 + §9 HTTPS/域名版（架构、部署六步、换机迁移指南、踩坑实录、移动端报错速查）
- Sprint 0 工具集 `tools/sprint0/`：外层唯一入口 `start-here.bat` 总控菜单（启动/停止/地址/安装/HTTPS 配置/客户端配置/自启开关/ddns-go 管理页，顶部实时显示 CloudCLI 与 HTTPS 状态，选项 1/2 整栈启停）；脚本本体收纳于 `bin/`：
  - `install-server.ps1/.bat`：服务端 7 步一键部署（Node 检查/安装、Claude Code 检查、CloudCLI 安装、电源常开、防火墙、专用网络、IP 探测），含死镜像源体检（检测已停服的 npm.taobao.org 并经确认迁移 npmmirror）
  - `install-client.ps1/.bat`：客户端连通性验证 + 桌面快捷方式，记住上次成功地址
  - `start-server.ps1/.bat` / `stop-server.ps1/.bat`：前台启动（打印各端地址）/ 停止后台实例
  - `run-server-hidden.ps1`：幂等后台启动（自启任务共用）
  - `setup-autostart.ps1` + `autostart-on/off.bat`：CloudCLI + Caddy + ddns-go 三组件登录自启开关
  - `enable-https.ps1/.bat`：HTTPS 一次性环境配置（防火墙 443、网络改专用、hosts 钉定）
  - 全部提示中英双语，跟随 Windows 显示语言（`-Lang zh|en` 可强制）
- HTTPS/域名版部署形态 `https://ai.jackqi.cn`：Caddy TLS 终结反代 + acme.sh（Let's Encrypt DNS-01，自动续期）+ ddns-go 动态解析，手机浏览器可"添加到主屏幕"装成独立 App（PWA）
- 环境与工具目录：`config/`、`scripts/`（生命周期命令）、`tools/`（辅助工具）
- `CLAUDE.md`：Spec 驱动敏捷开发规范与目录结构约定
- 根 `README.md`（文档地图与快速开始）、`.gitattributes`（行尾统一）、`.editorconfig`（编辑行为基线）
- `.env.example` 固定于仓库根目录，与 `.env` 同位（`cp .env.example .env` 即用）；登记腾讯云 CAM 凭证变量（供 DDNS 与 DNS-01 签证书）
- Spec 索引唯一来源为 `specs/MOC.md`，移除 specs/README.md 中的重复索引；文档内链接统一为标准相对路径

### Changed
- Sprint 0 脚本提示全量中英双语（原中文单语），语言跟随系统
- 服务端脚本重跑语义改为幂等更新：已装组件跳过、仅刷新配置，不重复安装（`-Update` 升级 CloudCLI）
- 客户端脚本记住上次成功连接的服务端地址，重跑回车即确认
- Sprint 0 工具目录重组：脚本收纳至 `tools/sprint0/bin/`，外层仅留 `start-here.bat` 入口与 README

### Fixed
- 开机自启重启后失效：Windows 计划任务结束时会连带杀死子进程，Caddy 自启命令由 `start` 改为长驻 `run`

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
