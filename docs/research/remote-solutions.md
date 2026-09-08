# 调研报告：远程操作 Claude Code 的可行方案

> - **日期**：2026-09-05
> - **状态**：已完成（结论待立项决策）
> - **范围**：三轮需求调研的合并结论 —— ① 开源方案盘点与底座可行性；② CC Switch 接入国产 AI 约束下的重新排序 + 文件查看需求；③ 多端（Windows + 安卓）与公网化（云服务器）路径验证。
> - **用途**：为 `specs/001-*` 立项提供事实依据；试用或自研的决策输入。

---

## 1. 需求画像

**背景**：用户在 Windows 上通过 Cmd / PowerShell / VS Code 终端使用 Claude Code，模型侧经 **CC Switch** 接入国产 AI（如智谱 GLM 的 Anthropic 兼容端点）。痛点是必须守在电脑前等待 LLM 返回、不时查看结果。

**一句话需求**：不守在电脑前，也能远程收发 AI 会话消息，并查看 AI 所在项目的文件内容。

**明确约束**（三轮澄清后固定）：

| # | 约束 | 来源 |
|---|------|------|
| C1 | 模型接入走 CC Switch（自定义 `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN`），**不经过 claude.ai 官方订阅/端点** | 第 2 轮 |
| C2 | 不止收发消息，还需**查看 AI 所在项目的文件内容** | 第 2 轮 |
| C3 | 多端：至少 **Windows PC + 安卓** | 第 3 轮 |
| C4 | 可能不在局域网，需支持**公网访问**；可提供**云服务器** | 第 3 轮 |
| C5 | 多端设计倾向 **Web**，不做原生移动 App | 第 3 轮（用户判断，已验证成立） |

## 2. 核心结论（先看这里）

1. **品类存在**：开源界已有成熟方案（CloudCLI、Happy、Omnara、code-server 等），可拿来做底座改造，但需注意许可证（CloudCLI 为 AGPL-3.0）。
2. **原始设想大体正确，但"三台机器"是误解**：用户设想的"AI 宿主机 / 消息中转服务端 / 客户端"是**三个逻辑角色**，不是三台物理机 —— 中继角色可折叠为同机进程、免费隧道（Tailscale / Cloudflare Tunnel）或一台廉价云服务器，**最少两台设备**即可（宿主机 + 手机）。
3. **官方远程功能确认不可用**：官方 Remote Control（`/remote-control`）有三条硬性要求，CC Switch 用户全部不满足（见 §3.1）。
4. **方案选择规则**：任何要求 claude.ai 登录或官方端点的方案出局；**坐在本地 CLI 之上的方案天然继承 CC Switch 配置**，全部可用。
5. **多端与公网均可行**：推荐的两个方案都是浏览器即用（Windows + 安卓零额外客户端）；公网化有"云服务器做入口"和"全家桶上云"两种姿势（见 §5）。
6. **Web 优先判断成立**：响应式 Web 覆盖全端，原生 App 的真实增量只有系统级推送与语音，MVP 不需要。
7. **自研必要性下降**：现成方案覆盖度高。建议先试用（Sprint 0），再在"继续用 / 瘦自研 MVP / 参考式自研"三岔口做决策（见 §7）。

## 3. 硬约束推导：官方远程面为什么全部出局

### 3.1 官方 Remote Control 的三条硬性要求

| 要求 | CC Switch 用户现状 |
|------|--------------------|
| Pro / Max / Team / Enterprise 订阅 + 完整 claude.ai OAuth | ❌ 使用国产 AI 供应商，无官方订阅 |
| 必须直连 `api.anthropic.com` | ❌ 流量走国产供应商端点 |
| **明确不支持**自定义 `ANTHROPIC_BASE_URL`（也不支持 Bedrock/Vertex） | ❌ CC Switch 的核心机制就是改这个变量 |

其他局限（即便可用也要注意）：约 10 分钟无活动断线超时、仅单会话。

### 3.2 同理出局的方案

- **Claude Code on the web**（claude.ai 网页版编码）：跑在 Anthropic 云沙箱里，**不是操作用户自己的机器**，场景不符 + 需官方订阅。
- **Telegram / Slack 桥接类**（JessyTsui/Claude-Code-remote 等）：消息通道被墙，国内不可稳定使用。

## 4. 方案盘点与对比

| 方案 | 形态 / 多端 | 文件查看 | 公网 | CC Switch 兼容 | 许可证 | 结论 |
|------|-------------|----------|------|----------------|--------|------|
| **CloudCLI**（siteboon/claudecodeui） | 自托管 Web UI，响应式（PC/平板/手机浏览器，另有可选桌面伴侣 App） | ⭐ File Explorer：文件树 + 语法高亮 + **在线编辑**；Git Explorer（暂存/提交/分支） | 自托管于任意可达地址；官方文档含 PM2 / 远程服务器部署；亦有托管版 cloudcli.ai | ✅ 坐在本地 CLI 之上 | AGPL-3.0 | **首选试用** |
| **VS Code Remote Tunnels**（`code tunnel` + vscode.dev） | 任意浏览器（含安卓 Chrome）+ 各平台 VS Code 客户端 | ⭐ 完整 IDE 级文件体验（最强） | 天生公网（微软隧道服务，国内连通性一般；自托管替代：code-server + Tailscale） | ✅ 终端里照常跑 claude | MIT（VS Code） | **零开发备选** |
| **Happy**（slopus/happy） | `happy` 命令包裹 claude；iOS/Android/macOS 原生 App + Web | 仅 File Mentions（无文件浏览器） | 官方免费 E2EE relay；relay **开源可自建**于云服务器 | ✅ | MIT | 消息体验最强，文件弱 → 第二梯队 |
| Omnara | Web + App，BYOK agent 指挥中心 | 弱 | 有云服务 | ✅ | — | 功能过重，淘汰 |
| Telegram 桥接类 | TG 机器人 | 无 | — | ✅ | — | 通道被墙，淘汰（§3.2） |
| tmux + SSH（Termius/Blink） | 终端 App | 无（需手动 cat） | 原生支持 | ✅ | — | 兜底方案，体验差 |

### 4.1 CloudCLI 要点（首选试用的依据）

- 技术栈 React/Vite/Tailwind/CodeMirror；`npx @cloudcli-ai/cloudcli` 即起，Node 20+（README 建议 v22+），默认端口 3001。
- 会话从 `~/.claude` **自动发现**，与本地 Claude Code 双向共享 MCP / 权限 / 设置；集成 shell 终端。
- 安全设计：所有工具默认禁用，需手动开启（但注意 §5.3 的 Web 访问认证问题）。
- **AGPL-3.0 义务**：修改后以网络服务形式提供，必须开放修改后的源码。自用无碍；若做对外产品需评估。
- 响应式多端与移动端触摸布局为官网明确特性（[官网](https://claudecodeui.siteboon.ai/)，桌面/移动截图齐全）。

### 4.2 Happy 要点（若消息体验优先）

- 双向 WebSocket 同步，经 **E2EE relay**（AES、QR 码配对交换密钥、零信任 —— relay 只见密文）；离线优先的加密 pub/sub 队列。
- 手机端 Allow/Deny **权限审批**是独门能力（AI 等待授权时手机点一下）。
- relay server 开源可自建 —— 正好部署到用户的云服务器，手机 App 填自建地址。

### 4.3 自研可依赖的官方本地原语（与网关选择无关）

这些是与 CC Switch 无冲突的"官方积木"，任何自研底座都建立在其上：

- **Headless 模式**：`claude -p --output-format stream-json --verbose --include-partial-messages`（流式输入/输出）。
- **Hooks**：`Notification`（permission_prompt / idle_prompt / agent_needs_input 等）与 `Stop`（可 decision-block），支持任意 shell `curl` 推送（官方支持）及 `type:"http"`。
- **Agent SDK**：Python `claude-agent-sdk` / TS `@anthropic-ai/claude-agent-sdk`，streaming input 模式。
- **会话持久化**：`session_id` / `--continue` / `--resume` / `--fork-session`；transcript 落盘于 `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`。
- **多智能体管理**：`claude agents`（attach / peek / reply）。

## 5. 多端与公网部署（第 3 轮验证结论）

### 5.1 多端：两个首选方案都满足，且都是"浏览器即用"

- **CloudCLI**：响应式 Web，PC Chrome/Edge + 安卓 Chrome 直接访问，零安装（可选 Windows 托盘伴侣 App）。
- **VS Code Remote Tunnels**：vscode.dev 任意浏览器可用（安卓可看文件，编辑体验一般 —— 但本需求以"看"为主，够用）。
- Happy 额外提供原生安卓 App（系统推送 + 语音是原生仅有的真实增量）。

### 5.2 公网：关键架构事实与两种姿势

**架构事实**：CloudCLI 必须与 Claude Code **同机**部署（读 `~/.claude` 会话、本地 spawn CLI 进程）。因此"上云"有两种姿势，云服务器在两种中都有用：

**姿势 1 —— 云服务器只当公网入口（AI 留在 Windows 开发机）**

```
[安卓/PC 浏览器] ─公网→ [云服务器: frps/nginx] ═frp 反向隧道═ [Windows 开发机: CloudCLI + Claude Code (CC Switch)]
```

- frp 隧道由开发机**主动出站**连接云服务器：无需公网 IP、无需路由器端口映射。
- 变体：Tailscale 组网（零公网暴露，免 VPS）或 Cloudflare Tunnel（免费，需域名）。
- 代价：开发机须保持开机不休眠（调整 Windows 电源计划）。
- Happy 同理：自建 relay 部署于云服务器，手机 App 连自建地址（数据 E2EE）。

**姿势 2 —— 全家桶上云（开发机可关机）**

```
[安卓/PC 浏览器] ─公网→ [云服务器: CloudCLI + Claude Code + 项目仓库]
```

- Claude Code + CC Switch 环境变量 + 项目代码（git clone 或 syncthing 同步）全在云上，7×24 可用。
- 这正是用户最初"三角色"设想的物理化 —— 第三台不是一台电脑，而是一台廉价云服务器。
- 代价：AI 改的是**云上代码副本**，与本地副本需靠 git / 同步工具保持一致。

### 5.3 安全必读（公网化前必做）

- CloudCLI **未确认内置 Web 访问认证**（docs.cloudcli.ai 目前不可达；社区讨论确认的仅是"各设备自带 API key / `claude /login`"的模型侧认证）。公网裸奔一个无认证的 Web IDE ≈ 把 shell 交给全网。
- 缓解（至少其一）：
  1. frp 加 token + Nginx 反代 **basic-auth + HTTPS**；
  2. **Tailscale 私网**（访问端与服务器组网，服务不暴露公网）—— 最省心；
  3. Cloudflare Access 之类零信任门面。
- 全链路国内可达组合（无海外依赖）：**国内云服务器（阿里/腾讯）+ 自托管 CloudCLI + CC Switch 指向国产模型网关 + 浏览器访问**。

### 5.4 对"Web 优先"判断的验证

**成立，且是主流选择**。CloudCLI 自身就是纯 Web 响应式一套代码覆盖全端。原生 App 的增量价值仅剩系统级推送与语音输入（Happy 的卖点）；而安卓 Chrome 对 PWA + Web Push 支持良好，自研时推送也能用 Web 技术补齐。**结论：本项目路线不造 App，v1 亦不必装 Happy。**

## 6. 自研评估

- **必要性**：现成方案覆盖度高（消息 + 文件 + 多端 + 公网全占），纯功能角度自研必要性显著下降。
- **仍值得自研的三种情形**：
  1. 想要**极简中文瘦客户端**（现成方案功能过剩、界面英文）；
  2. 本仓库以 **Spec 驱动练手**为首要目的（学习价值 > 产出价值）;
  3. **数据不出内网**的强约束（连自建 relay 都不想要）。
- **AGPL 注意**：直接 fork CloudCLI 做网络服务 → 修改部分必须开源。规避方式：**参考其架构、不抄代码**，或接受开源义务。
- **瘦 MVP 建议范围**（若走自研）：stream-json 会话桥 + 只读文件浏览 + git diff 视图 + Tailscale 内网部署，第一版不做原生 App、不做 E2EE。

## 7. 推荐路径与待决策问题

### 推荐路径

**Sprint 0（1~2 周试用）→ 三岔口决策**：

1. 试用 CloudCLI（姿势 1 frp 入口版 或 姿势 2 云上全家桶版，含 §5.3 安全加固），真实使用数日；
2. 三岔口：
   - **A. 继续用现成方案**（项目转向或归档）；
   - **B. 瘦自研 MVP**（按 §6 范围立项 `specs/001`）；
   - **C. 参考式自研**（以 CloudCLI 为架构参考，规避 AGPL）。

### 决策记录（2026-09-05 已拍板）

| # | 问题 | 决策 |
|---|------|------|
| D0 | 路径 | **先试用再立项**（Sprint 0：1~2 周真实使用 CloudCLI，结论回填本报告后再定自研范围） |
| Q1 | AI 宿主放哪？ | **Windows 开发机（姿势 1）**：代码零迁移，云服务器仅备用（如自建 DERP/headscale） |
| Q2 | 公网访问安全方案？ | **Tailscale 私网**：服务不暴露公网，各访问设备装 Tailscale |
| Q3 | 是否需要主动推送？ | **需要**（"不守在电脑前"的核心价值；试用重点验证 CloudCLI 通知能力，自研则走 hooks + Web Push） |
| Q4 | 文件查看只读还是可编辑？ | 默认 **v1 只读**（YAGNI，spec 确认时可改） |
| Q5 | 单会话还是多会话并行？ | 默认 **v1 单会话 + 可恢复历史**（YAGNI，spec 确认时可改） |

**试用期（Sprint 0）待验证点**：CloudCLI 的等待输入/完成通知能力、手机端文件查看体验、会话恢复、Windows 常开的稳定性、CC Switch 切换供应商后是否无缝跟随、断线重连。结论回填本报告"试用记录"节后，走 §7 三岔口决策。

**部署分阶段**：先局域网（步骤见 [sprint0-cloudcli-lan-deploy.md](./sprint0-cloudcli-lan-deploy.md)），公网 Tailscale 阶段试用通过后另行补充。

## 试用记录（Sprint 0）

### 2026-09-08：部署完成，正式开始观察

- **局域网 HTTP 部署完成**：手机/PC 浏览器访问 CloudCLI 正常，会话同步、shell、消息收发工作
- **HTTPS/域名升级完成**：`https://ai.jackqi.cn`（Caddy + Let's Encrypt + ddns-go 动态解析），手机已可"添加到主屏幕"装成独立 App——PWA 需 HTTPS 的限制已通过个人域名方案解除（成本：域名 ~39 元/年 + 证书 0 元）
- **已知现象（待观察）**：
  - Shell 界面偶发 `Cannot resize a pty that has already exited`——会话退出后前端仍发 resize 指令所致，**无害**，刷新页面即消；上游缺陷，候选 issue 反馈
  - 移动端布局基础可用但不算精致（试用观察项 2 持续）
- **基础设施亮点**：换服务端 = 新机重跑脚本 + ddns-go 自动改解析，迁移成本低（详见部署文档 §9.4）

## 8. 参考资料

### 官方

- Claude Code 官方文档（headless / hooks / SDK / 会话管理 / remote control 限制）：https://docs.claude.com/en/docs/claude-code（Headless & SDK、Hooks、Remote Control 各节）
- CloudCLI（Claude Code UI）官网：https://claudecodeui.siteboon.ai/
- CloudCLI GitHub：https://github.com/siteboon/claudecodeui （AGPL-3.0；README 含 PM2 / 远程服务器部署文档入口）
- Happy GitHub：https://github.com/slopus/happy （MIT；E2EE relay 可自建）

### 实践参考

- Pinggy：Remotely Manage Claude Code from Phone —— https://pinggy.io/blog/remotely_manage_claude_code_from_phone/
- CloudCLI UI 中文部署指南 —— https://takeshell.com/2025/09/11/cloudcli-ui-guide/
- Hacker News CloudCLI 讨论（8.2k+ 浏览，部署坑扫描）—— https://news.ycombinator.com/item?id=47352564
- Reddit r/ClaudeAI claudecodeui 讨论（认证模型）—— https://www.reddit.com/r/ClaudeAI/comments/1rgcctp/

---

## 变更记录

| 日期 | 说明 |
|------|------|
| 2026-09-05 | 合并三轮调研（方案盘点 / CC Switch 与文件查看约束 / 多端与公网验证）成稿 |
| 2026-09-05 | §7 决策记录：路径=先试用再立项；宿主=Windows 开发机；安全=Tailscale；推送=需要；Q4/Q5 取 YAGNI 默认 |
| 2026-09-05 | 增补部署分阶段说明：先局域网（见 sprint0-cloudcli-lan-deploy.md），公网 Tailscale 后续 |
