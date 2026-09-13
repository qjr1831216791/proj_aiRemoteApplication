# 011-attack-surface-hardening · 手工验收清单（acceptance-manual）

> **状态：已验收**（2026-09-13）——AC1~AC11 全过：AC1~AC3/AC6 自动化覆盖为主；
> AC4/AC5/AC7/AC9/AC11 部分真机实测 + 自动化背书（**未单独实测的子场景逐条如实注记**，§2/§4）；
> AC8/AC10 真机实测。spec 已流转 done，发版 v0.7.0。
> **验收人**：JackQi（需求方）；**签收口径**（2026-09-13 对话原文）：
> **「所有我关注的安全方面的项目都已验收」「当前版本可以固定了」**。

## 0. 验收时间线

| 日期 | 事件 |
|------|------|
| 2026-09-13 | **立项即实施**：全仓安全审查（[report](../../docs/research/security-audit-2026-09.md)）认定「密钥不泄露 ⇒ 不可入侵」在 010 收口后仍不成立（三条不经密钥的路径）→ spec/plan/tasks 当日定稿（reviewed → in-progress，分支 `spec011-attack-surface-hardening`） |
| 2026-09-13 | **T1~T6 当日完成**：输入校验双闸 + sink 值包裹（763d261，+10 测试）、`prepare_stack` 去短路（a9c24eb，+2 测试）、CSP 基线（521dddd）、Caddy 指纹锁定方案 A 定案（5818743，门控四场景全过）、`upgrade-component.ps1`（86f2ed3）、多账号管理全链（`set-https-account.ps1` + Rust 派发 + 设置区界面） |
| 2026-09-13 | **真机发现与修复（T7 环节，三处）**：①**登录死循环**——CloudCLI 前端 API 带 `Bearer` 头与 443 的 `basic_auth` 冲突，浏览器反复弹原生账密框；修复为标记段加 `@noBearer` 匹配器（`not header_regexp Authorization ^Bearer\s`），四态真机矩阵验证通过；②**心跳 401 误判红标**——探测不带凭证，认证墙 401 是预期应答而非故障，`heartbeat.rs` 状态码分类改 401/403 → 健康；③**单门模式固化**——CloudCLI 官方平台模式（`VITE_IS_PLATFORM=true`）经启动脚本持久化、前端 bundle 补丁入 install-server 幂等段，443 入口 basic_auth 成为唯一认证门（§3） |
| 2026-09-13 | **验收期 UI 批次**：设置页左侧目录、按钮全局悬停全文、访问账号卡/组网卡/诊断结果/日志目录等十余处真机反馈逐项收口（spec 001 §7 v2.8~v2.12） |
| 2026-09-13 | **需求方签收** → spec done，合并 main，发布 v0.7.0 |

## 1. 前置速查

- 自动化：`cargo test --manifest-path apps/workbench/src-tauri/Cargo.toml`（**254 通过**）、`cd apps/workbench && npm test`（**23 通过**）
- 打包与发版：`powershell -ExecutionPolicy Bypass -File scripts/build.ps1`、`scripts/release-check.ps1`
- 只读取证：`curl -s http://127.0.0.1:3001/api/auth/user`（无 token 返回首个用户 = 服务端平台模式在跑）、`curl -sk -o /dev/null -w "%{http_code}" https://ai.jackqi.cn/`（**401 = 443 密码门在位**）
- 凭证事实来源：栈目录 `auth-accounts.json`（仅 bcrypt 哈希）；Caddyfile 标记段 `# BEGIN/END workbench-auth`

## 2. AC 逐条验收记录

口径原则：**真机实测、自动化背书、需求方签收三者分开陈述**；未单独实测的子场景一律写明，不以签收代替证据。

| AC | 判定口径 | 证据 / 注记 |
|----|----------|-------------|
| **AC1** domain/stack_dir 注入拒绝 | **自动化覆盖** | T1（763d261）：`validate_domain`/`validate_stack_dir` 纯函数 + `tool_plan` Result 化接线 + `normalize_stack_dir` 写入即拒，双字段逐元字符断言，新增 10 测试；254 全绿中运行 |
| **AC2** 合法值透传 + 值参数单引号包裹 | **自动化覆盖** | T1：`visible_script_params` 对值参数一律 PowerShell 单引号字面量（内部 `'` 翻倍）、参数名裸 token；含多级域名与含空格路径用例 |
| **AC3** mesh 五文件篡改 → SHA256 拒 apply | **自动化覆盖** | T2（a9c24eb）：`prepare_stack` 去短路，exe 已存在同样走 `verify_easytier_binaries`；新增 2 测试（逐文件篡改拒绝 + 完好不误伤） |
| **AC4** CSP dev/build 双回归 + 真机逐项清单 | **部分真机 + 构建背书** | CSP 指令集已设（521dddd，编译期解析通过）。dev 态：2026-09-13 多次 `npm run tauri dev` 全功能可用（当日整批 UI 反馈均产生于该态）✓；build 态：`scripts/build.ps1` 发布构建通过背书 ✓。**「逐项手工清单」未单独走查**（启停/设置/向导/地址区/诊断/低频工具逐项核对）——如实注记 |
| **AC5** Caddy 下载 SHA256 校验先于落位 | **脚本层已落 + 真机篡改场景未单独实测** | T4（5818743）：`$CaddyCoreVersion`/`$CaddySha256` 常量对 + **校验严格先于落位** + 拒装引导（指向 `-CaddyZip` 手动兜底并明示「人工信任转移」）。**真机构造篡改场景（改哈希后装机应拒）未单独实测**——如实注记 |
| **AC6** 下载 URL 固定核心版本 | **自动化/实证覆盖** | T4 门控：URL 断言 + 三次独立下载哈希恒定（钉版 `p=github.com/caddyserver/caddy/v2@v2.11.4`），方案 A 定案；上游只伺服最新版的风险已记 plan §7 |
| **AC7** `upgrade-component.ps1` 一条命令升级 | **同版本幂等已演练；真版本抬升待上游** | T5（86f2ed3）：caddy v2.11.4 重下哈希 = 锁值、easytier v2.6.4 五文件逐一吻合，全程零 diff、退出码 0；输出新旧对照与官方校验和来源。**真版本抬升的完整演练并入 T7/T9 阶段**——上游发新版后才有可抬目标，当前无可执行样本（如实注记） |
| **AC8** 多账号各自凭证；移除即锁 | **真机实测（部分）+ 未单独实测子项** | 真机：2026-09-13 需求方在设置页实际新增多个账号（jackqi / qjr2 / qjr），脚本窗口取密 → `auth-accounts.json` 哈希落盘 → Caddyfile 标记段再生为多账号块 ✓；443 入口无凭证 401（本机探 `https://ai.jackqi.cn/` = 401 ✓）。**未单独实测**：「不同成员设备以各自凭证进入」（未逐设备验证）、**「移除账号后新请求 401 且重连被拒」**（需求方当日点开移除确认框，无完整移除记录）、WS 终端重连专项——如实注记，§4 |
| **AC9** 未加账号时空标记段；三条启动路径 | **实现层 + 部分真机** | 生成端带空标记段：`install-https.ps1` 生成即写入 `# BEGIN/END workbench-auth`（空注释段）✓；三条启动路径：**工作台托管 ✓**（当日 dev 态托管全程可用）、**开机自启 ✓**（计划任务拉起 CloudCLI/Caddy，本机实测在线）、**菜单直启未单独走**——如实注记；界面入口 ✓（设置页「访问账号」卡，含空态引导） |
| **AC10** localhost:3001 不受密码门影响 | **真机实测** | 2026-09-13 多次实测：`http://127.0.0.1:3001/` = 200、`/api/auth/user` 无 token 返回首个用户；密码门只落在 443 入口，回环与 Caddy→3001 上游均不受影响 ✓ |
| **AC11** 工作台发起 + 脚本执行 + 标记段再生 + 存量首插 | **真机实测（部分）+ 未单独实测子项** | 真机：**存量装机（v0.6.0 升级形态）首次管理自动插入标记段且生效**——本机即存量形态，Caddyfile 内 `BEGIN/END workbench-auth` 段与多账号 `basic_auth` 块为活证据 ✓；新增账号走工作台发起 + 可见窗脚本取密（密码不经程序）✓；账号与哈希持久化于 `auth-accounts.json`（仅哈希）✓。**未单独实测**：`caddy validate` 失败自动回滚、`caddy reload` 零停机生效（实现与单测在 T6，真机未构造失败场景）——如实注记 |

## 3. 单门模式专节：真机发现的认证冲突与固化

**发现**（2026-09-13 真机）：成员设备访问 `https://ai.jackqi.cn/` 出现**登录死循环**——多次点取消后再点登录仍反复弹原生账密框。

**诊断链**：隔离 caddy 复现 → 管理口三方哈希比对 → 生产路径测试账号端到端 → 前端 JS 证实 CloudCLI 的 API 调用把 `Authorization` 头写成 `Bearer <token>`，而 443 的 `basic_auth` 拒 Bearer 并回 401 + `WWW-Authenticate`，浏览器（尤其移动端/PWA）据此反复弹框。

**修复**：标记段 basic_auth 加 `@noBearer` 匹配器（`not header_regexp Authorization ^Bearer\s`）——Bearer 请求跳过 caddy 门、交由 CloudCLI 令牌把关；无头/basic 请求照常校验。四态真机矩阵（无凭证 / 错凭证 / 对凭证 / Bearer）验证通过。

**固化（单门模式）**：CloudCLI 以官方平台模式运行（`VITE_IS_PLATFORM=true` 由启动脚本注入、子进程继承），服务端跳过自身 token 认证；前端 bundle 补丁让自动登录生效（锚点 `CV={}` 单点替换 + `.bak-platform` 备份，幂等、升级后自动重打）。**443 入口的 basic_auth 成为唯一认证门**，`@noBearer` 匹配器过渡期保留（旧 PWA 缓存可能仍带 Bearer）。原实验为 SESSION-ONLY（env 仅实验进程、补丁手工替换），经需求方确认实验成功后固化进脚本。

## 4. 未单独实测子项与遗留

**未单独实测**（判定依据为自动化背书或实现层，非真机黑盒）：

1. AC4 的「dev/build 双态逐项手工清单」（启停、设置、向导、地址区、诊断、低频工具逐项核对）——以两种构建态下全功能可用背书
2. AC5 的「真机构造篡改装机应拒」（脚本内断言 + 拒装引导已实现）
3. AC8 的「不同成员设备各持凭证进入」「**移除账号后新请求 401 且重连被拒**」「WS 终端重连专项」
4. AC9 的「菜单直启」路径
5. AC11 的「`caddy validate` 失败自动回滚」「`caddy reload` 零停机生效」

**遗留（已知问题，需求方 2026-09-13 判定暂不处理）**：

- **单门模式前端补丁不覆盖「缓存已热」的客户端**：补丁原地改写同名 bundle（`index-<hash>.js`），而该资源带 `Cache-Control: immutable` 且被 PWA service worker cache-first 缓存——打过补丁前访问过的设备（如手机上已访问过的 PWA）会一直用旧 bundle，表现为「CloudCLI 自己的登录页」而非自动登录。**触发场景**：补丁上线前访问过的设备；同版本内不会自发发生（升级换 hash 即自然失效）。**未采用的修法**：改名 bundle（会牵动 44 个按需 chunk 的相对 import，有双实例求值风险）或补 sw.js（`CACHE_NAME` 换名 + `cache:'reload'`）。**处置**：暂不处理（需求方判定），受影响设备清一次站点缓存即恢复。

**过程中的额外发现与修复**（已并入交付，非遗留）：心跳探测把认证墙 401 误判为「异常」红标（`heartbeat.rs` 状态码分类改 401/403 → 健康，单测锚定）。
