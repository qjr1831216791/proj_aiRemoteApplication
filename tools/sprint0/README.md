# Sprint 0 工具集：新手上路

> **一句话**：在"跑 AI 的电脑"上一键装好 CloudCLI 网页控制台，之后用任意电脑或手机的浏览器遥控它。
>
> 背景：这是 [remote-solutions.md](../../docs/research/remote-solutions.md) 调研决策（D0 先试用）的落地工具；
> 完整部署文档：[sprint0-cloudcli-lan-deploy.md](../../docs/research/sprint0-cloudcli-lan-deploy.md)（遇事不决看它的 §6 排障表）。
> 适用范围：**局域网**（所有设备连同一个 WiFi/路由器），公网版后续另出。

---

## 第 0 步：分清角色（最重要的一张图）

```
[客户端] 手机/其他电脑          [服务端] 跑 AI 的电脑
  浏览器打开地址  ──局域网──>  CloudCLI(网页控制台)
  什么都不用装                 └─> Claude Code + CC Switch
```

| 角色 | 是哪台电脑 | 要做什么 |
|------|-----------|----------|
| **服务端** | 实际运行 Claude Code（AI）的那台 | 运行本目录的安装脚本（唯一要装东西的机器） |
| **客户端** | 你用来遥控的其他电脑 | 只用浏览器（脚本只是帮你验证连通 + 建快捷方式） |
| 手机 | 也是客户端 | 连脚本都不用，浏览器输地址 |

## 服务端上路（3 步）

1. **双击 `install-server.bat`** → 弹 UAC 点"是"（第一个黑窗一闪而过是正常现象，提权后会开新窗口继续）。
   脚本自动完成：检查/安装 Node → 检查 Claude Code → 装 CloudCLI → 电源永不睡眠 → 防火墙放行 → 网络改"专用"。
   - 卡在哪一步会**停下来告诉你怎么办**（比如缺 Claude Code / CC Switch 没配好），照提示做完重跑即可。
   - npm 下载慢：右键编辑该文件，把 `set "PS_ARGS="` 改成 `set "PS_ARGS=-UseMirror"` 再双击。
   - 重跑是安全的：已装好的组件自动跳过，只刷新电源/防火墙/网络配置；升级 CloudCLI 把 `PS_ARGS` 改成 `-Update`（可叠加 `-UseMirror`）再双击。
2. **记下结尾打印的"客户端访问地址"**，形如 `http://192.168.x.x:3001`（这就是给其他设备用的）。
3. **浏览器打开 `http://localhost:3001` → 设置 → 开启需要的工具**（默认全禁用是它的安全设计；建议先开文件浏览/编辑 + Git）。

**可选但推荐**——让它开机常驻、以后不用管启动：

```powershell
powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1                # 注册登录自启
powershell -NoProfile -ExecutionPolicy Bypass -File .\run-server-hidden.ps1   # 立即启动一次（不等重新登录）
```

## 客户端上路（2 步）

1. 把整个 `sprint0/` 文件夹拷到客户端电脑，**双击 `install-client.bat`**：
   - 首次运行会提示输入服务端地址（服务端安装结尾打印的那个，形如 `http://192.168.x.x:3001`），输入回车即可；
   - 连接成功后自动记住（存于脚本同目录 `.last-server-url`），之后每次双击**直接回车**确认；
   - 想预填固定地址：右键编辑 `install-client.bat`，顶部 `set "SERVER_URL=http://192.168.x.x:3001"`。
2. 脚本会：验证服务端可达（不通会按顺序告诉你查什么）→ 桌面生成"AI 远程工作台"快捷方式 → 自动打开浏览器。

## 手机上路（0 步）

同 WiFi 下浏览器直接输 `http://192.168.x.x:3001`；Chrome 菜单里"添加到主屏幕"，下次一点就进。

## 文件清单（谁在哪台机器用）

| 文件 | 哪台机器 | 什么时候用 |
|------|----------|------------|
| `install-server.bat` / `.ps1` | 服务端 | 装机**一次**（双击 .bat） |
| `install-client.bat` / `.ps1` | 客户端 | 每台客户端**一次**（首次输地址；换地址时重跑） |
| `start-server.bat` | 服务端 | 手动启动服务（双击；已在运行则直接开浏览器） |
| `run-server-hidden.ps1` | 服务端 | 后台静默启动（自启任务/hook 内部调用，一般不直接碰） |
| `setup-autostart.ps1` | 服务端 | 注册/移除开机自启（**一次**） |
| `README.md` | — | 本文件 |

## 常见问题

| 症状 | 处理 |
|------|------|
| 双击 `install-server.bat` 后好像没反应 | 看屏幕中央是否弹了 UAC 深色对话框；被拒了就右键 → 以管理员身份运行 |
| 提示 `cloudcli` 不是内部或外部命令 | 刚装完 PATH 未刷新，新开一个终端窗口再试 |
| 客户端/手机打不开页面 | 按序查：① 服务端 `cloudcli` 起了吗（`start-server.bat`）② 防火墙规则（重跑 install-server）③ 是否同一 WiFi（别用访客网络）④ 路由器 AP 隔离。详表见部署文档 §6 |
| 双击 `start-server.bat` 提示端口被占 | 多半后台实例已在跑，脚本会直接帮你开浏览器，无需处理 |
| 服务端重启后服务没了 | 没注册自启：跑一次 `setup-autostart.ps1` |
| 想停止服务 | 前台窗口 Ctrl+C；后台实例执行 `Get-NetTCPConnection -LocalPort 3001 -State Listen \| ForEach-Object { Stop-Process -Id $_.OwningProcess -Force }` |
| 想完全卸载 | `npm uninstall -g @cloudcli-ai/cloudcli` + `setup-autostart.ps1 -Remove` + 删防火墙规则（详见部署文档 §8） |
| 想换端口 | `install-server.ps1 -Port 3002` 重跑，客户端 URL 与防火墙规则同步改 |
| 想升级/重装 CloudCLI | 服务端编辑 `install-server.bat`，`set "PS_ARGS=-Update"` 后重跑 |
| 想换服务端地址 | 客户端双击 `install-client.bat`，提示处输入新地址（旧记录自动覆盖） |

## 试用期你要观察什么（1~2 周）

1. **AI 等输入/完成时有没有通知推到手机锁屏？**（最关键——没有就是自研的最强理由）
2. 手机看代码的体验（滚动/高亮/diff）
3. 锁屏再打开，会话状态还在吗
4. CC Switch 换供应商后，网页会话跟不跟着变
5. 开发机常开一夜，早上状态如何

感受（几句话也行）回填 [remote-solutions.md](../../docs/research/remote-solutions.md) 的"试用记录"节 → 走三岔口决策（继续用 / 瘦自研 / 参考式自研）。
