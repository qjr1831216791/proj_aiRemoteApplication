# Sprint 0 工具集：新手上路

> **一句话**：在"跑 AI 的电脑"上一键装好 CloudCLI 网页控制台，之后用任意电脑或手机的浏览器遥控它。
>
> 背景：这是 [remote-solutions.md](../../docs/research/remote-solutions.md) 调研决策（D0 先试用）的落地工具；
> 完整部署文档：[sprint0-cloudcli-lan-deploy.md](../../docs/research/sprint0-cloudcli-lan-deploy.md)（遇事不决看它的 §6 排障表）。
> 适用范围：**局域网**（所有设备连同一个 WiFi/路由器）；`https://ai.jackqi.cn` 走域名证书，公网版后续另出。

---

## 第 0 步：双击 `start-here.bat`（总控菜单，记不住脚本就用它）

外层只有这一个入口。双击后输入数字即可完成全部功能：

```
  1. 启动服务（后台）并打开本机页面     6. 客户端配置（本机验证 + 桌面快捷方式）
  2. 停止服务                           7. 开机自启：全部开启
  3. 查看各端访问地址                   8. 开机自启：全部关闭
  4. 安装/重装服务端（管理员）          9. 打开 ddns-go 管理页
  5. HTTPS 环境配置（管理员）           0. 退出
```

菜单顶部实时显示 CloudCLI 与 HTTPS 的运行状态。其余说明文档照旧，下文路径均已含 `bin\`。

## 第 1 步：分清角色（最重要的一张图）

```
[客户端] 手机/其他电脑          [服务端] 跑 AI 的电脑
  浏览器打开地址  ──局域网──>  Caddy(443) ──> CloudCLI(3001)
  什么都不用装                 └─> Claude Code + CC Switch
  https://ai.jackqi.cn            └─> ddns-go + acme.sh 自动续期
```

| 角色 | 是哪台电脑 | 要做什么 |
|------|-----------|----------|
| **服务端** | 实际运行 Claude Code（AI）的那台 | 运行本目录的安装脚本（唯一要装东西的机器） |
| **客户端** | 你用来遥控的其他电脑 | 只用浏览器（脚本只是帮你验证连通 + 建快捷方式） |
| 手机 | 也是客户端 | 连脚本都不用，浏览器输地址 |

## 服务端上路（3 步）

1. **双击 `bin\install-server.bat`** → 弹 UAC 点"是"（第一个黑窗一闪而过是正常现象，提权后会开新窗口继续）。
   脚本自动完成：检查/安装 Node → 检查 Claude Code → 镜像源体检 → 装 CloudCLI → 电源永不睡眠 → 防火墙放行 → 网络改"专用"。
   - 卡在哪一步会**停下来告诉你怎么办**（比如缺 Claude Code / CC Switch 没配好），照提示做完重跑即可。
   - npm 下载慢：右键编辑该文件，把 `set "PS_ARGS="` 改成 `set "PS_ARGS=-UseMirror"` 再双击。
   - 重跑是安全的：已装好的组件自动跳过，只刷新电源/防火墙/网络配置；升级 CloudCLI 把 `PS_ARGS` 改成 `-Update`（可叠加 `-UseMirror`）再双击。
2. **记下结尾打印的"客户端访问地址"**，形如 `http://192.168.x.x:3001`（这就是给其他设备用的）。
3. **浏览器打开 `http://localhost:3001` → 设置 → 开启需要的工具**（默认全禁用是它的安全设计；建议先开文件浏览/编辑 + Git）。

**HTTPS 版（推荐，手机可装成 App）**：需要一个已实名域名（当前 `ai.jackqi.cn`）。依次：菜单 5（HTTPS 环境配置）→ 菜单 1（启动服务）→ 手机访问 `https://ai.jackqi.cn`。原理与排障见部署文档新增的 HTTPS 一节。

**开机常驻（可选但推荐）**——双击 `bin\autostart-on.bat` 开启，`bin\autostart-off.bat` 关闭：

```powershell
powershell -ExecutionPolicy Bypass -File .\bin\setup-autostart.ps1                # 注册登录自启（三组件）
powershell -NoProfile -ExecutionPolicy Bypass -File .\bin\run-server-hidden.ps1   # 立即启动一次（不等重新登录）
```

## 客户端上路（2 步）

1. 把整个 `sprint0/` 文件夹拷到客户端电脑，**双击 `bin\install-client.bat`**：
   - 首次运行会提示输入服务端地址（推荐直接填 `https://ai.jackqi.cn`），输入回车即可；
   - 连接成功后自动记住（存于脚本同目录 `.last-server-url`），之后每次双击**直接回车**确认；
   - 想预填固定地址：右键编辑 `bin\install-client.bat`，顶部 `set "SERVER_URL=https://ai.jackqi.cn"`。
2. 脚本会：验证服务端可达（不通会按顺序告诉你查什么）→ 桌面生成"AI 远程工作台"快捷方式 → 自动打开浏览器。

## 手机上路（0 步）

同 WiFi 下浏览器打开 **`https://ai.jackqi.cn`**（推荐，证书可信且可"添加到主屏幕"装成独立 App）；没有域名环境时用 `http://192.168.x.x:3001` 兜底。

## 文件清单（谁在哪台机器用）

> 所有 `.ps1` 的提示语言**跟随 Windows 显示语言**（中文系统 → 中文，其余 → 英文），可加 `-Lang zh|en` 强制指定。
> `.bat` 全部为纯 ASCII 薄启动器（真实逻辑与双语提示在对应 `.ps1`，UTF-8 带 BOM）——这是刻意设计：.bat 内混入中文在 GBK 码页机器上有换行被吞的解析风险。

**外层**：

| 文件 | 说明 |
|------|------|
| `start-here.bat` | **总控菜单（唯一日常入口）** |
| `README.md` | 本文件 |

**`bin\`（脚本区，日常不用碰）**：

| 文件 | 哪台机器 | 什么时候用 |
|------|----------|------------|
| `menu.ps1` | 服务端 | start-here.bat 的实际实现 |
| `install-server.bat` / `.ps1` | 服务端 | 装机**一次**（双击 .bat） |
| `install-client.bat` / `.ps1` | 客户端 | 每台客户端**一次**（首次输地址；换地址时重跑） |
| `start-server.bat` / `.ps1` | 服务端 | 手动启动服务（双击；打印本机/移动端地址；已在运行则直接开浏览器） |
| `stop-server.bat` / `.ps1` | 服务端 | 停止后台服务（双击；前台窗口直接 Ctrl+C 即可） |
| `run-server-hidden.ps1` | 服务端 | 后台静默启动（自启任务/hook 内部调用，一般不直接碰；地址记入日志） |
| `setup-autostart.ps1` | 服务端 | **开机自启开关**：启用/移除 CloudCLI + Caddy + ddns-go 三个登录自启（`-Remove` 全关） |
| `autostart-on.bat` / `autostart-off.bat` | 服务端 | 双击版自启开关：双击 on 启用、双击 off 关闭（免命令行） |
| `enable-https.bat` / `.ps1` | 服务端 | HTTPS 一次性配置：防火墙 443 + 网络改专用 + hosts 钉定 DNS API 域名 |

## 常见问题

| 症状 | 处理 |
|------|------|
| 双击 `install-server.bat` 后好像没反应 | 看屏幕中央是否弹了 UAC 深色对话框；被拒了就右键 → 以管理员身份运行 |
| 提示 `cloudcli` 不是内部或外部命令 | 刚装完 PATH 未刷新，新开一个终端窗口再试 |
| 客户端/手机打不开页面 | 按序查：① 服务端起没起（`start-server.bat`）② 防火墙规则（重跑 install-server）③ 是否同一 WiFi（别用访客网络）④ 路由器 AP 隔离。详表见部署文档 §6 |
| 双击 `start-server.bat` 提示端口被占 | 多半后台实例已在跑，脚本会直接帮你开浏览器，无需处理 |
| 服务端重启后服务没了 | 双击 `bin\autostart-on.bat` 注册自启 |
| 想停止服务 | 前台窗口 Ctrl+C；后台隐藏实例双击 `bin\stop-server.bat` |
| 想完全卸载 | `npm uninstall -g @cloudcli-ai/cloudcli` + `bin\autostart-off.bat` + 删防火墙规则（详见部署文档 §8） |
| 想换端口 | `install-server.ps1 -Port 3002` 重跑，客户端 URL 与防火墙规则同步改 |
| 想升级/重装 CloudCLI | 服务端编辑 `install-server.bat`，`set "PS_ARGS=-Update"` 后重跑 |
| 想换服务端地址 | 客户端双击 `install-client.bat`，提示处输入新地址（旧记录自动覆盖） |
| npm install 报 npm.taobao.org 证书错误（ERR_TLS_CERT_ALTNAME_INVALID） | 机器残留了停服的旧淘宝镜像变量；重跑 `install-server.bat`，步骤 3 会检测并提示一键迁移 npmmirror |

## 试用期你要观察什么（1~2 周）

1. **AI 等输入/完成时有没有通知推到手机锁屏？**（最关键——没有就是自研的最强理由）
2. 手机看代码的体验（滚动/高亮/diff）
3. 锁屏再打开，会话状态还在吗
4. CC Switch 换供应商后，网页会话跟不跟着变
5. 开发机常开一夜，早上状态如何

感受（几句话也行）回填 [remote-solutions.md](../../docs/research/remote-solutions.md) 的"试用记录"节 → 走三岔口决策（继续用 / 瘦自研 / 参考式自研）。
