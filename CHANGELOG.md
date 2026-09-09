# 变更日志（Changelog）

> 本文件兼作**版本说明（Release Notes）与变更历史**：每个版本条目即该版本的发布说明。
> 格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [语义化版本 SemVer](https://semver.org/lang/zh-CN/)。

## [Unreleased]

（下一迭代起记录）

## [0.2.0] - 2026-09-09

Sprint 1 桌面控制台（Spec 001）：sprint0 全部能力的 GUI 化——托盘常驻的统一控制面，含状态总览、一键启停、自启托管、可控退出与双形态分发。GUI 手工验收清单随版交付（`specs/001-desktop-console/acceptance-manual.md`），作为发布后验收回填记录。

交付: specs/001-desktop-console（Sprint 1；自动化测试 125 项全绿 + 真机实证，纯人工观察项见验收手册）

### Added
- `apps/workbench/` 桌面工作台（Tauri 2 + Preact + TypeScript + Vite）：蓝白主题、页面就绪后显形的主窗口、系统托盘六项菜单常驻、关闭主窗最小化到托盘；单实例——同会话重复启动激活已有主窗，跨会话重复给出提示后退出
- 一键启停与状态总览：总开关补齐启动未运行组件（幂等，不重复拉起），三组件状态卡五态实时刷新（前台 2s 轮询，外部启停自动感知），组件级重试、失败原因与 `%TEMP%\cloudcli.log` 尾部展示；访问地址区（本机/局域网/域名，一键复制/打开）
- 停止管线：CloudCLI 按进程树结束（防 claude 会话子进程残留）、Caddy 优雅停 + 路径校验兜底强杀、ddns-go 按可执行路径匹配；单组件 10s 超时放行并附手动排查命令，不阻塞其余组件；「停止」前自动取消在途启动
- 退出语义：默认退出保留服务（远程访问不断线）；托盘「停止服务并退出」显式收摊（总超时 30s 放行）；设置可反转为"退出即停止"；关机/注销不收摊、不阻止关机；程序崩溃/强杀不影响三组件，重启后接管管理
- 自启托管：设置页「服务开机自启」开关接管存量 Sprint0 计划任务（幂等重建不产生双通道，关闭即移除且运行中进程不受影响，接管时提示）；「程序开机自启」开关（登录静默入托盘不弹主窗）；「启动时联动补齐服务」（手动双击与自启同样生效，可关）
- 设置持久化：五项行为开关即改即存、重启保持；端口/路径/域名只读展示（附"修改须重跑安装脚本"指引）与日志目录入口；首次运行全默认值；设置文件损坏自动回退默认、界面提示并改名留档（`.bad-<时间戳>`）
- 中英双语全量覆盖（界面、托盘菜单、脚本调用 `-Lang`、状态详情与全部用户可见错误提示）：默认跟随系统显示语言，切换立即生效（含托盘菜单重建，无需重启），显式选择优先于系统
- 低频操作区（sprint0 菜单 3~6/9 与 bin 级入口）：各端访问地址、安装/重装服务端、升级 CloudCLI（`-Update`，可叠加镜像源）、HTTPS 栈装机 `install-https`（UAC 可见窗，结尾手工步骤完整可读）、HTTPS 环境配置、客户端配置（普通可见交互窗）、ddns-go 管理页、打开工作台页面；脚本缺失时相关入口禁用并提示；UAC 被拒/脚本失败给出明确提示且不崩溃
- 一键构建与双形态分发：`scripts/build.ps1` 产出 NSIS 安装器（embedBootstrapper 离线可装，currentUser 级）与便携 zip（exe + 资源目录，解压即用），产物命名含版本号、落 `release/`；sprint0 脚本以内置副本随包分发（SHA256 校验同步）；覆盖安装升级保留设置；SmartScreen"仍要运行"引导写入包内说明

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
