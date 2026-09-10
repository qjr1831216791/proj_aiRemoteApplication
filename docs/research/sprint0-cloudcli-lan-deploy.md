# Sprint 0 试用部署方案（局域网版）：CloudCLI on Windows

> - **日期**：2026-09-05
> - **前提**：仅局域网部署（手机/其他 PC 与开发机同一 WiFi），**公网（Tailscale）阶段本次不做**，作为后续独立步骤。
> - **拓扑**：`[手机/PC 浏览器] ─局域网→ [Windows 开发机: Caddy(443) → CloudCLI(3001) + Claude Code(CC Switch)]`（HTTPS 未部署时手机直连 3001，见 §9）
> - **决策依据**：[remote-solutions.md](./remote-solutions.md) §7 决策记录（D0 先试用 / Q1 开发机 / Q2 Tailscale / Q3 需推送）
> - **预计耗时**：全新环境约 30~40 分钟

---

## 快速路径：脚本化安装（推荐）

部署文档中可自动化的部分已脚本化，位于 `tools/sprint0/`（**新手直接看 [tools/sprint0/README.md](../../../tools/sprint0/README.md) 的上路引导**，本文是细节底册）：
2026-09-08 起日常入口是双击 **`tools/sprint0/start-here.bat`** 总控菜单（启动/停止/状态/安装/HTTPS/自启开关全在菜单里），脚本本体收纳于 `tools/sprint0/bin/`：

| 脚本 | 跑在哪 | 覆盖的手工章节 | 用法 |
|------|--------|----------------|------|
| `bin\install-server.ps1` | **服务端**（运行 AI 实例的电脑，需管理员） | §0 Node 检查/安装、§1 安装 CloudCLI、§3.2 防火墙、§3.3 IP 探测、§4 电源常开 | `powershell -ExecutionPolicy Bypass -File .\bin\install-server.ps1`（慢网加 `-UseMirror`，端口非默认加 `-Port`） |
| `bin\install-client.ps1` | **客户端**（远程操控的电脑，无需管理员） | §3.4 连通性验证 + 桌面快捷方式 | `powershell -ExecutionPolicy Bypass -File .\bin\install-client.ps1 -Url http://<服务端IP>:3001` |

脚本管不到、仍需手动的三件事：① Claude Code + CC Switch 前置（脚本只检查，不代装）；② 页面内开启工具开关（§2.2）；③ 手机没有脚本——同 WiFi 浏览器直接输地址，可"添加到主屏幕"。

**双击运行**：同目录提供成对的 `.bat` 启动器（`.bat` 与 `.ps1` 必须同文件夹，整个 `sprint0/` 目录拷走即可复用）：

| 启动器 | 用法 |
|--------|------|
| `bin\install-server.bat` | 直接双击 → 自动弹 UAC 提权 → 执行 `install-server.ps1`；需要国内镜像时右键编辑，把 `PS_ARGS=` 改为 `PS_ARGS=-UseMirror` |
| `bin\install-client.bat` | 右键编辑顶部 `SERVER_URL=` 填入服务端地址（如 `http://192.168.x.x:3001`）→ 双击；为空时运行中手动输入亦可 |

## 服务的启动、停止与自启

**手动启动**：终端运行 `cloudcli`，保持窗口开启（窗口 = 服务生命周期）。或双击 `tools/sprint0/bin/start-server.bat`——已在运行则直接打开浏览器，否则在当前窗口启动。整栈（CloudCLI + Caddy + ddns-go）一键启停用总控菜单 `start-here.bat` 的选项 1/2。

**停止**：前台实例 Ctrl+C 或关窗；后台/自启实例双击 `bin\stop-server.bat`，或手动结束占用 3001 端口的进程：

```powershell
Get-NetTCPConnection -LocalPort 3001 -State Listen | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force }
```

**推荐：登录自启**（CloudCLI 定位是基础设施，应常驻、与 claude 会话解耦）。双击 `bin\autostart-on.bat` 开启、`bin\autostart-off.bat` 关闭（即菜单选项 7/8），等价命令行：

```powershell
powershell -ExecutionPolicy Bypass -File .\bin\setup-autostart.ps1          # 注册三组件登录自启（CloudCLI + Caddy + ddns-go）
powershell -NoProfile -ExecutionPolicy Bypass -File .\bin\run-server-hidden.ps1   # 注册后立即启动 CloudCLI 一次，不等重新登录
powershell -ExecutionPolicy Bypass -File .\bin\setup-autostart.ps1 -Remove  # 全部移除
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
            "command": "powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File \"D:/Document/AI_CodeStudyProj/proj_aiRemoteApplication/tools/sprint0/bin/run-server-hidden.ps1\""
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
4. **文件浏览**：File Explorer 打开本仓库 `D:\Document\AI_CodeStudyProj\proj_aiRemoteApplication`，确认文件树、语法高亮、编辑保存正常。

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

## 9. HTTPS/域名版（ai.jackqi.cn）：手机可"装成 App"的进阶形态

> **性质澄清**：这**不是公网版**——域名只为证书可信服务，访问仍限局域网（同一 WiFi）。
> **动机**：PWA 完整安装（独立 App 窗口、无浏览器地址栏）要求**安全上下文（HTTPS）**，明文 HTTP 下浏览器只给书签式快捷方式。Service Worker / PWA 安装仅允许 HTTPS 或 localhost，此为浏览器硬性规则。

### 9.1 架构与运行时布局

```
手机 ──https://ai.jackqi.cn:443──> Caddy(TLS终结) ──> 127.0.0.1:3001 CloudCLI
      │
      ├ DNS: ai.jackqi.cn 的 A 记录 ← ddns-go 每 5 分钟跟随本机局域网 IP
      └ 证书: Let's Encrypt（DNS-01 验证，经腾讯云 CAM API），acme.sh 计划任务自动续期
```

| 路径 | 内容 | 分发注意 |
|------|------|----------|
| `D:\Software\cloudcli-https\` | caddy.exe、ddns-go.exe、Caddyfile、ddns-go.yaml、certs\ | **勿分发**（ddns-go.yaml 含 API 密钥、certs 含私钥） |
| `C:\Users\<user>\.acme.sh\` | acme.sh 安装、账户与证书源文件（含密钥） | 勿分发 |
| 项目根 `.env` | 凭证**录入入口**（TENCENT_SECRET_ID/KEY） | 勿分发；运行时无脚本读取它（凭证已固化于上两处） |

### 9.2 前置条件

1. **域名**（腾讯云购买，购买流程强制实名；未实名 .cn 不给解析）。示例：`jackqi.cn`，用子域 `ai.jackqi.cn`
2. **腾讯云 CAM API 密钥**：https://console.cloud.tencent.com/cam/capi → 新建密钥。规范做法是建子用户仅授 `QcloudDNSPodFullAccess` 再为其建密钥
   - ⚠️ 老体系"DNSPod Token"（console.dnspod.cn → 密钥管理）**已被 acme.sh 新版淘汰**（`dns_dnspod` 插件已删除），不要走错门
3. 项目根 `.env` 录入：`TENCENT_SECRET_ID=` / `TENCENT_SECRET_KEY=`（模板见仓库 `.env.example`）

### 9.3 部署步骤（对应 `tools/sprint0/bin/`）

> **新机部署请优先使用工作台「装机向导」**（spec 006，2026-09-10）：装 APP → 按五阶段提示输入参数即完成（含插件版 Caddy 自动签证，免本节的手工 acme.sh 步骤）。以下手工步骤保留作为脚本级兜底与旧机（acme.sh 证书链）维护参考；旧机不受 006 插件化影响（既有 Caddyfile 幂等不覆盖，ADR-0003）。

0. **一键装栈**：双击 `bin\install-https.bat`（管理员）——自动下载 `caddy.exe` / `ddns-go.exe`（GitHub Release 最新版，按资产名正则匹配，幂等可重跑；直连失败可 `-CaddyZip` / `-DdnsZip` 指向手动下载的 zip，升级加 `-Update`）、生成步骤 4 的 Caddyfile、代跑步骤 5 的 enable-https、拉起 ddns-go 并打开管理页。做完本步，下面只剩 2 的密钥配置与 3 的证书签发两件手工活
1. **DNS 记录**：手动加一条 `ai` 的 A 记录 → 当前服务端 IP；或跳过手动，直接配 ddns-go 自动创建
2. **ddns-go**：（步骤 0 已拉起）浏览器 `127.0.0.1:9876` → 服务商选腾讯云、填 SecretId/Key、IPv4 取"网卡"WLAN、域名 `ai.jackqi.cn` → 保存即更新记录；`-f 300` = 每 5 分钟校正（换热点/换 WiFi 全自动跟随）
   ⚠️ 不要设置"HTTP 绑定网卡"（`httpinterface`）相关选项——见 §9.5-⑧
3. **证书**（先导入凭证环境变量 `Tencent_SecretId` / `Tencent_SecretKey`）：
   ```bash
   acme.sh --issue --dns dns_tencent -d ai.jackqi.cn --server letsencrypt
   acme.sh --install-cert -d ai.jackqi.cn --ecc \
     --fullchain-file <certs>/ai.jackqi.cn.fullchain.cer \
     --key-file <certs>/ai.jackqi.cn.key \
     --reloadcmd "<caddy> reload --config <Caddyfile>"
   ```
   ⚠️ **必须传全名 `--dns dns_tencent`**——踩坑实录 §9.5-①
4. **Caddy**：（Caddyfile 已由步骤 0 生成）交互式终端可 `caddy.exe start --config Caddyfile`（内容：`ai.jackqi.cn:443 { tls <证书> <私钥>; reverse_proxy 127.0.0.1:3001 }`，顶部 `auto_https disable_redirects`——80 端口可能被 Hyper-V 排除）。⚠️ 计划任务里必须用长驻的 `run` 而非 `start`，原因见 §9.5-⑩（自启脚本已内置正确写法）
5. **双击 `bin\enable-https.bat`**（即菜单选项 5）：防火墙放行 TCP 443（仅专用网络）+ 全部网络配置文件改"专用"（新网络默认 Public，否则规则不生效）+ hosts 钉定 `dnspod.tencentcloudapi.com` 的 IPv4（§9.5-⑧）
6. **验证**：PC `curl https://ai.jackqi.cn` → 200；手机同 WiFi 打开 → 无警告锁标 → Chrome"添加到主屏幕"装成独立 App

### 9.4 换机迁移指南（服务端坏了/退役，换新机）

关键认知：**域名、DDNS、证书续期全部绑在"云账户身份"上，不绑机器**——所以迁移很轻。

| 资产 | 迁移策略 |
|------|----------|
| 域名 + A 记录 | 不动；新机 ddns-go 配好后自动把 A 记录刷成新机 IP（切换的临门一脚） |
| CAM 密钥（`.env`） | 直接复用（自己的账户） |
| 证书/私钥 | **推荐不拷**：新机用同一 CAM 密钥重新签（免费，LE 每域名每周 5 张限额足够）；拷旧 `~/.acme.sh` 也可无缝续期 |
| `D:\Software\cloudcli-https` 的 exe | 可拷；`ddns-go.yaml` 含自己密钥，拷给自己没问题 |
| Claude Code 会话/项目 | 拷 `~/.claude\`（要历史才拷）+ 项目目录（路径尽量一致） |

步骤：新机装 Claude Code + CC Switch（唯一的手工活）→ 拷项目与 `~/.claude\`（可顺带拷 `D:\Software\cloudcli-https` 的两个 exe，或由脚本重新下载）→ `bin\install-server.bat` → `bin\install-https.bat`（装栈 + 环境配置，已包含原 enable-https 步骤）→ 配 ddns-go → 重签证书 → `bin\autostart-on.bat` → 验证 → 旧机 `bin\autostart-off.bat` 善后。

### 9.5 踩坑实录（知识库精华，全是实测踩过）

1. **acme.sh `--dns tencent` 必须传全名**：acme.sh 以 `_startswith $_currentRoot "dns"` 判定挑战类型（issue 流程）；传 `tencent` 不以 "dns" 开头 → 被误判为 http-01 → LE 去连 80 端口 → 域名 A 记录是私有 IP 被判 "no valid A records"。旧插件名 `dnspod` 恰好以 dns 开头纯属侥幸
2. **DNSPod Token 体系已被淘汰**：acme.sh 新版删除 `dns_dnspod`，统一 `dns_tencent`（CAM SecretId/SecretKey）。老的 console.dnspod.cn Token 与腾讯云 CAM 密钥是两套体系，别混
3. **PS 5.1 无 BOM 文件按系统 ANSI（GBK）解码**：UTF-8 中文 3 字节被 GBK 两两吞并时会把换行符一起吃掉 → 行合并 → 花括号悬空解析错。**所有 `.ps1` 必须 UTF-8 带 BOM**；`.bat` 必须纯 ASCII（中文提示下沉到 ps1），否则 GBK 码页机器上同样炸
4. **PowerShell 字符串 `"$Port:"` 会被解析为作用域变量语法**（冒号后跟非法字符报错）→ 变量后紧跟冒号要写 `${Port}:`
5. **`$ErrorActionPreference='Stop'` 时，原生命令的 stderr 重定向（`2>$null`）会变成终止错误** → 需要吞 stderr 时放进 `cmd /c "… 2>nul"` 内部执行
6. **新版 npm 拒绝 `npm config set disturl`**（"not a valid npm option"）→ 需要 disturl 时直接维护用户 `.npmrc`
7. **停服的 npm.taobao.org 镜像残留**（9 个环境变量）导致 node-gyp 拉头文件证书报错 → `install-server.ps1` 步骤 3 自动体检并经确认迁移 npmmirror
8. **ddns-go 配置了 `httpinterface`（HTTP 绑定网卡）时，API 请求被绑到接口的全局 IPv6 上**，而本机 v6 到 API 目标路由不通 → 每次更新报 `dial tcp: address [本机v6]:0: no suitable address found` → **清空 ddns-go 配置里的 `httpinterface`**（留空让系统正常路由）即恢复；hosts 钉定 API 域名 IPv4（enable-https 步骤 3）作为额外保险可保留，但不是本问题根因
9. **DNS 变更后沿途缓存最长 TTL 600s**（10 分钟）才一致：权威 NS 立即生效，公共/路由器 DNS 有滞后；手机开关飞行模式强制重新查询
10. **计划任务结束时 Windows 会连带杀死同作业的子进程**：自启任务里跑 `caddy start`（fork 子进程后父退出）→ 任务结束 → 子 caddy 被杀，443 从未真正起来。**自启任务必须直接运行长驻进程本体**（Caddy 用 `run`；CloudCLI 用 cmd 分离启动的 run-server-hidden.ps1）。症状特征：任务状态 Ready（已结束）而端口未监听
11. **已运行的 PowerShell 脚本不会因文件被修改而更新**（脚本在启动时一次性加载）：改了 `menu.ps1`/其他脚本后，必须关掉旧窗口重新打开才能生效。症状特征：改完代码行为依旧——先怀疑旧进程

### 9.6 卸载（HTTPS 部分）

`bin\autostart-off.bat`（关自启）+ 菜单选项 2（停整栈：CloudCLI + Caddy + ddns-go）+ `Remove-NetFirewallRule -DisplayName "CloudCLI LAN HTTPS 443"` + 可选删 `D:\Software\cloudcli-https\` 与 `~/.acme.sh`（`acme.sh --remove -d ai.jackqi.cn`）。CloudCLI 本体卸载见 §8。

### 9.7 移动端/浏览器报错速查

| 看到的现象 | 病因 | 处理 |
|------------|------|------|
| App 内显示 **"Offline, Please check your connection"** | 这是 PWA 的离线兜底页（Service Worker 缓存），表示**后端/Caddy 够不着**，不是手机断网 | 服务端菜单查状态；起服务后**完全关闭 App 重开**或下拉刷新（清掉缓存的离线页） |
| 浏览器报 **找不到服务器 / DNS_PROBE** | DNS 缓存未过期（记录 TTL 最长 10 分钟） | PC：`ipconfig /flushdns`；手机：开关一次飞行模式；浏览器开无痕窗口复测 |
| 浏览器**一直转圈超时** | 到服务端 IP 的连接被防火墙拦或服务没起 | 服务端菜单选 3 看状态；确认 enable-https 已跑、网络为"专用" |
| 浏览器报**证书警告**（红叉） | 证书过期/续期失败，或访问的不是本方案的域名 | 服务端跑 `acme.sh --renew -d ai.jackqi.cn --ecc --force`，看报错 |

---

## 变更记录

| 日期 | 说明 |
| 2026-09-05 | 初版：局域网部署（公网 Tailscale 阶段另行成文） |
| 2026-09-05 | 新增"快速路径：脚本化安装"：`tools/sprint0/install-server.ps1` 与 `install-client.ps1` |
| 2026-09-05 | 新增"服务的启动、停止与自启"：`start-server.bat` / `setup-autostart.ps1` / `run-server-hidden.ps1` + settings.json SessionStart hook 备选方案 |
| 2026-09-08 | 新增 §9 HTTPS/域名版（ai.jackqi.cn：Caddy + acme.sh + ddns-go，PWA 可安装）；§6 排障表增补镜像源体检与 HTTPS 条目；工具脚本迁移至 `bin/`、新增总控入口 `start-here.bat`（菜单化全部操作）；新增镜像源自动体检（§9.5 踩坑实录同步沉淀） |
| 2026-09-08 | 重启实战修复：计划任务子进程连带杀死（§9.5-⑩，Caddy 自启改 `run`）；菜单选项 1/2 升级为整栈启停；§9.5-⑪ 旧进程不热更新；新增 §9.7 移动端/浏览器报错速查表 |
| 2026-09-08 | 全量复核（v0.1.0 发布前）：合并重复的 §0 标题；用法示例统一 `bin\` 前缀；修正示例中过时的仓库旧路径；§9.3-4 补 `run`/`start` 使用边界；停止/卸载指引改为指向菜单与 stop-server.bat |
| 2026-09-08 | §9.3/§9.4 部分脚本化：新增 `bin\install-https.bat` / `.ps1`（GitHub Release 自动下载 caddy.exe / ddns-go.exe、生成 Caddyfile、复用 enable-https、拉起 ddns-go；`-CaddyZip` / `-DdnsZip` 手动 zip 兜底、`-Update` 升级、幂等可重跑），§9.3 增设步骤 0，§9.4 迁移步骤同步替换 |
