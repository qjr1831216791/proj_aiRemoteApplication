# Sprint 0 试用部署方案（局域网版）：CloudCLI on Windows

> - **日期**：2026-09-05
> - **前提**：仅局域网部署（手机/其他 PC 与开发机同一 WiFi），**公网（Tailscale）阶段本次不做**，作为后续独立步骤。
> - **拓扑**：`[手机/PC 浏览器] ─局域网→ [Windows 开发机: CloudCLI(3001) + Claude Code(CC Switch)]`
> - **决策依据**：[remote-solutions.md](./remote-solutions.md) §7 决策记录（D0 先试用 / Q1 开发机 / Q2 Tailscale / Q3 需推送）
> - **预计耗时**：全新环境约 30~40 分钟

---

## 快速路径：脚本化安装（推荐）

部署文档中可自动化的部分已脚本化，位于 `tools/sprint0/`（**新手直接看 [tools/sprint0/README.md](../../../tools/sprint0/README.md) 的上路引导**，本文是细节底册）：

| 脚本 | 跑在哪 | 覆盖的手工章节 | 用法 |
|------|--------|----------------|------|
| `install-server.ps1` | **服务端**（运行 AI 实例的电脑，需管理员） | §0 Node 检查/安装、§1 安装 CloudCLI、§3.2 防火墙、§3.3 IP 探测、§4 电源常开 | `powershell -ExecutionPolicy Bypass -File .\install-server.ps1`（慢网加 `-UseMirror`，端口非默认加 `-Port`） |
| `install-client.ps1` | **客户端**（远程操控的电脑，无需管理员） | §3.4 连通性验证 + 桌面快捷方式 | `powershell -ExecutionPolicy Bypass -File .\install-client.ps1 -Url http://<服务端IP>:3001` |

脚本管不到、仍需手动的三件事：① Claude Code + CC Switch 前置（脚本只检查，不代装）；② 页面内开启工具开关（§2.2）；③ 手机没有脚本——同 WiFi 浏览器直接输地址，可"添加到主屏幕"。

**双击运行**：同目录提供成对的 `.bat` 启动器（`.bat` 与 `.ps1` 必须同文件夹，整个 `sprint0/` 目录拷走即可复用）：

| 启动器 | 用法 |
|--------|------|
| `install-server.bat` | 直接双击 → 自动弹 UAC 提权 → 执行 `install-server.ps1`；需要国内镜像时右键编辑，把 `PS_ARGS=` 改为 `PS_ARGS=-UseMirror` |
| `install-client.bat` | 右键编辑顶部 `SERVER_URL=` 填入服务端地址（如 `http://192.168.x.x:3001`）→ 双击；为空时运行中手动输入亦可 |

## 服务的启动、停止与自启

**手动启动**：终端运行 `cloudcli`，保持窗口开启（窗口 = 服务生命周期）。或双击 `tools/sprint0/start-server.bat`——已在运行则直接打开浏览器，否则在当前窗口启动。

**停止**：前台实例 Ctrl+C 或关窗；后台/自启实例用（结束占用 3001 端口的进程）：

```powershell
Get-NetTCPConnection -LocalPort 3001 -State Listen | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force }
```

**推荐：登录自启**（CloudCLI 定位是基础设施，应常驻、与 claude 会话解耦）：

```powershell
powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1          # 注册（当前用户登录时隐藏窗口自启）
powershell -NoProfile -ExecutionPolicy Bypass -File .\run-server-hidden.ps1   # 注册后立即启动一次，不等重新登录
powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1 -Remove  # 移除
```

- 幂等：`run-server-hidden.ps1` 检测到 3001 已监听即退出，重复触发无副作用。
- 日志：`%TEMP%\cloudcli.log`（隐藏窗口无控制台输出，排障看这里）。
- 计划任务已设 `ExecutionTimeLimit=0`：默认的 72 小时强制结束已取消，服务可常驻。

**备选：Claude settings.json 的 SessionStart hook**（可行，但不默认推荐）——在 `~/.claude/settings.json` 增加 hooks 键：

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File \"D:/document/RekTec/AIProj/proj_aiRemoteApplication/tools/sprint0/run-server-hidden.ps1\""
          }
        ]
      }
    ]
  }
}
```

不推荐的理由与注意事项：

1. **依赖方向倒置**：CloudCLI 是宿主（它 spawn `claude`），由 claude 会话反向拉起属倒置；CloudCLI 网页自己 spawn 的每个 claude 会话也会触发该 hook（靠幂等守卫兜底为无操作）。
2. **CC Switch 同文件风险**：`~/.claude/settings.json` 正是 CC Switch 写 `env`（`ANTHROPIC_BASE_URL` 等）的文件。合并时保留其 `env` 键；并在 CC Switch 切换一次供应商后复查 `hooks` 是否幸存——若被重写丢失，请改用上面的登录自启方案。
3. 覆盖面偏差：只有"开了 claude 会话"服务才被拉起，不碰 claude 的时段服务不在。

## 0. 前置条件自查

> 架构事实：**客户端 = 浏览器，本机零安装**。客户端脚本只是"验证可达 + 建快捷方式"，跨多台电脑复用时把 `tools/sprint0/` 拷过去运行即可。

## 0. 前置条件自查

| # | 项目 | 检查命令 | 要求 |
|---|------|----------|------|
| P1 | Node.js | `node -v` | ≥ 20（官网口径；README 建议 22 LTS） |
| P2 | Claude Code | `claude --version` | 已安装，终端可正常对话 |
| P3 | CC Switch | 打开 CC Switch 确认当前供应商 | 已配置且 `claude` 走国产模型 |
| P4 | 网络 | 开发机与手机连同一 WiFi | 手机不在访客网络（访客网络常开 AP 隔离） |

P1 不满足时的安装建议（任选其一）：

```powershell
# 方式一：winget（Windows 11 自带）
winget install OpenJS.NodeJS.LTS

# 方式二：nvm-windows（便于多版本切换，推荐长期使用）
# 从 https://github.com/coreybutler/nvm-windows/releases 安装后：
nvm install 22
nvm use 22
```

npm 拉包慢可先换国内镜像（可选）：

```powershell
npm config set registry https://registry.npmmirror.com
```

## 1. 安装与启动 CloudCLI

```powershell
# 方式 A：全局安装（推荐，日常一个命令启动）
npm install -g @cloudcli-ai/cloudcli
cloudcli

# 方式 B：临时体验（首次会下载包，稍慢）
npx @cloudcli-ai/cloudcli
```

- 默认监听 **3001** 端口；被占用时（`netstat -ano | findstr 3001`）先释放端口，改端口的参数以 `cloudcli --help` 输出为准。
- 启动后在**开发机浏览器**打开 `http://localhost:3001` 先做本机验证。

## 2. 本机验证（局域网放行前先把功能跑通）

1. **会话发现**：首页会话列表应能看到 `~/.claude` 里的既有会话（按项目目录归组）。若为空，先在终端对本仓库跑一次 `claude` 产生会话再刷新。
2. **工具开关**：CloudCLI 出于安全**默认禁用所有工具**。进入设置，仅开启试用需要的：
   - ✅ 文件读取/浏览（File Explorer）
   - ✅ 文件写入/编辑（手机改代码的场景）
   - ✅ Git 面板（查看 diff / 提交）
   - ⬜ 集成 shell 终端（PC 上有用；手机上意义不大，可先不开）
3. **模型链路**：新建会话发一句消息，确认响应来自 CC Switch 当前供应商（可在会话里输入 `/status` 查看实际连接的 API 配置）。
4. **文件浏览**：File Explorer 打开本仓库 `D:\document\RekTec\AIProj\proj_aiRemoteApplication`，确认文件树、语法高亮、编辑保存正常。

## 3. 局域网放行（手机/其他 PC 接入的关键，多数卡壳都在这）

### 3.1 确认网络配置文件为"专用"

`设置 → 网络和 Internet → WiFi → 当前网络` → 网络配置文件选 **专用**。
（"公用"配置文件下防火墙默认拦得更死，后续规则也建议只挂在专用网络上。）

### 3.2 防火墙放行 3001

首选：首次启动 CloudCLI 时 Windows 弹出的防火墙提示，勾选**专用网络**并允许。
若当时点掉了/没弹，用管理员 PowerShell 补一条规则（仅专用网络、仅 TCP 3001）：

```powershell
New-NetFirewallRule -DisplayName "CloudCLI LAN 3001" -Direction Inbound -Protocol TCP -LocalPort 3001 -Action Allow -Profile Private
```

### 3.3 查开发机局域网 IP

```powershell
ipconfig
```

取**实际在用网卡**（网线看以太网适配器、WiFi 看无线适配器）的 IPv4 地址，形如 `192.168.x.x`。

### 3.4 多端接入验证

| 设备 | 操作 | 预期 |
|------|------|------|
| 另一台 PC | 浏览器开 `http://192.168.x.x:3001` | 打开 CloudCLI 界面 |
| 安卓手机（同 WiFi） | Chrome 打开同一地址 | 响应式移动布局，触摸导航正常 |
| 安卓手机（锁屏后重开） | 重新亮屏刷新页面 | 会话状态仍在 |

## 4. 开发机常开设置（姿势 1 的前提）

```powershell
# 电源：插电状态永不睡眠（屏幕仍可自动关闭）
powercfg /change standby-timeout-ac 0
```

- 可选：设备管理器 → 无线网卡 → 电源管理 → 取消勾选"允许计算机关闭此设备以节约电源"，避免 WiFi 休眠掉线。
- 试用期**手动启动**即可；想开机自启再做一个 Task Scheduler 任务或 `shell:startup` 放一个 `cloudcli` 的 .bat，暂不展开（YAGNI）。

## 5. 试用期观察点（对应调研报告 §7）

| # | 观察点 | 关联决策 |
|---|--------|----------|
| 1 | AI 等待输入/任务完成时，CloudCLI 是否有通知？锁屏能否收到？ | Q3（最关键：无推送 = 自研最强理由） |
| 2 | 手机端看代码体验：小屏滚动、语法高亮、git diff 可读性 | C2 |
| 3 | 锁屏重连后，会话/文件页面状态是否完好 | 可用性底线 |
| 4 | CC Switch 切换供应商后，CloudCLI 新会话是否无缝跟随 | C1 |
| 5 | 开发机常开一夜挂任务，早上状态如何 | 姿势 1 稳定性 |

试用结论（哪怕几句话）回填 [remote-solutions.md](./remote-solutions.md) 的"试用记录"节，再走三岔口决策（继续用 / 瘦自研 / 参考式自研）。

## 6. 故障排查

| 症状 | 排查顺序 |
|------|----------|
| 本机 localhost 能开，手机/其他 PC 打不开 | ① 网络配置文件是否"专用" → ② 防火墙规则是否只挂了 Public → ③ 启动输出里监听地址是 `0.0.0.0` 还是 `127.0.0.1`（后者需查 host 参数） → ④ 路由器 AP 隔离（PC 间互 ping 也不通即是） |
| 端口冲突 | `netstat -ano | findstr 3001` 找到 PID，任务管理器结束或换端口 |
| npx/npm 下载极慢或失败 | 换 npmmirror 镜像（§0），或改用全局安装重试 |
| npm install 原生模块（better-sqlite3）编译失败，报 `npm.taobao.org` 证书错误 | 机器环境变量残留已停服的旧淘宝镜像（`NODEJS_ORG_MIRROR` 等约 9 个）：重跑 install-server，步骤 3 会检测并提示一键迁移；手动法 `setx NODEJS_ORG_MIRROR https://npmmirror.com/mirrors/node/`（其余 `NVMW_*`/`NODIST_*`/`IOJS_*` 同理），新开终端重试 |
| 会话列表为空 | 先在终端对目标项目目录跑一次 `claude`；CloudCLI 按 `~/.claude/projects` 归组发现会话 |
| 消息无响应/模型报错 | 回终端直接跑 `claude` 对照：终端也不通 = CC Switch/供应商问题，与 CloudCLI 无关 |
| 手机时连时不连 | WiFi 频段切换（2.4G/5G 通常同网段无碍）、网卡节能（§4）、路由器 DHCP 租约 |

## 7. 安全基线（局域网 ≠ 绝对安全）

- **3001 无认证**（调研报告 §5.3）：同一局域网内任何设备都可操作。家庭网络确保 WiFi 为 WPA2/WPA3 且密码非默认；公司/公共网络**不要**放行使用，用完即关 CloudCLI。
- 工具开关保持最小化（§2.2），尤其 shell 终端按需开启。
- 这些风险在局域网试用期内**已知并接受**；进入公网阶段（Tailscale，Q2 已决策）后服务不再暴露给局域网外。

## 8. 回滚与清理

```powershell
npm uninstall -g @cloudcli-ai/cloudcli   # 卸载
Remove-NetFirewallRule -DisplayName "CloudCLI LAN 3001"   # 删防火墙规则（可选保留）
```

CloudCLI 不改动 `~/.claude` 既有数据，卸载无残留顾虑。

---

## 变更记录

| 日期 | 说明 |
|------|------|
| 2026-09-05 | 初版：局域网部署（公网 Tailscale 阶段另行成文） |
| 2026-09-05 | 新增"快速路径：脚本化安装"：`tools/sprint0/install-server.ps1` 与 `install-client.ps1` |
| 2026-09-05 | 新增"服务的启动、停止与自启"：`start-server.bat` / `setup-autostart.ps1` / `run-server-hidden.ps1` + settings.json SessionStart hook 备选方案 |
