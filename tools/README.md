# 开发工具（tools）

> 存放**一次性 / 辅助性**的独立工具脚本：数据修复、环境自检、代码生成、批量处理等。

## 与 scripts/ 的边界

| 目录 | 放什么 | 特点 |
|------|--------|------|
| `scripts/` | 项目生命周期命令（dev / build / test / deploy） | 稳定、高频、是日常入口 |
| `tools/` | 辅助工具脚本 | 按需执行、独立成篇、非日常流程 |

拿不准放哪边时：会每天跑的放 `scripts/`，偶尔跑一次的放 `tools/`。

## 现有工具

> 新手从 [sprint0/README.md](./sprint0/README.md) 进入：角色分辨 + 服务端/客户端/手机三路上路引导 + 常见问题。

| 脚本 | 用途 |
|------|------|
| [sprint0/install-server.ps1](./sprint0/bin/install-server.ps1) | Sprint 0 服务端一键安装：CloudCLI on Windows（局域网版，需管理员） |
| [sprint0/install-client.ps1](./sprint0/bin/install-client.ps1) | Sprint 0 客户端：连通性验证 + 桌面快捷方式（浏览器即客户端，无需安装其他软件） |
| [sprint0/install-server.bat](./sprint0/bin/install-server.bat) | 服务端双击启动器：自动弹 UAC 提权并调用同名 .ps1（纯 ASCII，避免 cmd 编码问题） |
| [sprint0/install-client.bat](./sprint0/bin/install-client.bat) | 客户端双击启动器：顶部编辑 `SERVER_URL` 后双击即用（为空时运行中输入亦可） |
| [sprint0/start-server.bat](./sprint0/bin/start-server.bat) | 服务端双击启动 CloudCLI：已在运行则直接打开浏览器 |
| [sprint0/run-server-hidden.ps1](./sprint0/bin/run-server-hidden.ps1) | 幂等后台启动 CloudCLI（计划任务与 SessionStart hook 共用；日志 `%TEMP%\cloudcli.log`） |
| [sprint0/setup-autostart.ps1](./sprint0/bin/setup-autostart.ps1) | 注册/移除 CloudCLI + Caddy 两个登录自启计划任务（`-Remove` 全关；推荐的常驻方式） |

> 上表只列高频入口。`sprint0/bin/` 的全部脚本（另含 HTTPS 栈装机、腾讯云密钥写入、EasyTier 组网服务/密钥与旧通道卸载）以 [sprint0/README.md](./sprint0/README.md) 的"文件清单"为唯一来源，此处不重复维护。

## 约定

- 文件名 kebab-case；每个脚本**头部注释写清用途与用法**（支持 `--help` 更佳）。
- 脚本不得硬编码密钥或环境相关路径，一律读环境变量（模板见根目录 [.env.example](../.env.example)）。
- 有破坏性的脚本（删数据、改库）必须在头部显著标注，并支持 dry-run 优先。
- 工具本身有测试价值的，测试放在 `tests/tools/`。
