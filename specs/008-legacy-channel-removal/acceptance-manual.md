# 008-legacy-channel-removal · 真机手工验收清单

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · [tasks.md](./tasks.md) · 返回 [MOC](../MOC.md)
>
> **状态：已验收**（2026-09-11 需求方签收——AC1 静态部分与 AC2 为 T20 实测/自动化覆盖（附录 A），其余验收项按需求方签收指令确认通过；AC10/AC11 的真机卸载未单独执行，`uninstall-legacy.ps1` 随包保留，执行时脚本内置组网在线 + 凭证双重前置校验）。
> 自动化基线（2026-09-11 T20 复跑）：`cargo test` **190 passed + 3 ignored**；`npm run build` 零错误；zh/en 键集 **250/250 严格一致**；`$ScriptSubset`/manifest/双 bin 目录三方哈希一致。详见附录 A。

> ## ⛔ 前置闸门（真机执行侧，spec §5 时序约束）
>
> **`uninstall-legacy.ps1` 的真机执行与本版本（008 构建）安装，以下三项全部满足前不得进行：**
>
> 1. **007 T14 验收签收**——[007 acceptance-manual](../007-mesh-access/acceptance-manual.md) §3 签收表回填完成（AC1~AC12 全勾）；
> 2. **O1（TUN 网卡防火墙归类）闭合**——007 附录已实测「无碍」（TUN 归 Private、443 规则 Profile=Any），待签收时正式归档；
> 3. **组网稳定观察期**——卸载旧通道前组网是唯一远程访问兜底，须确认组网日常使用稳定。
>
> 开发态（`npm run tauri dev`）的 UI 走查预演不受闸门限制；闸门只约束真机安装与卸载脚本执行。

## 0. 准备与入口速查

| 项 | 值/位置 |
|---|---|
| 宿主机 | Windows 11，007 已装机且组网在线（006/007 验收基线） |
| 栈目录 | 默认 `D:\Software\cloudcli-https`（下文记 `<stack>`；实际以设置页「安装目录」卡为准） |
| 卸载脚本 | `<stack>\bin\uninstall-legacy.ps1`（随 008 安装包分发；仓库源 `resources/bin/` 与 `tools/sprint0/bin/` 双份） |
| 卸载脚本参数 | `-StackDir`（默认即上）、`-Lang zh|en`、`-Force`（跳过组网在线校验，**仅供演练**） |
| 旧残留速查命令 | `tasklist | findstr /i "frpc ddns"`（进程）· `schtasks /query /tn "ddns-go Sprint0 autostart"`（任务）· `dir <stack>\ddns-go* <stack>\frpc*`（文件）· `findstr SAKURA_FRP_KEY <stack>\.env`（密钥行） |
| 组网入口 | 主界面「组网通道」卡（MeshCard）；设置页「组网设置」卡 |
| 向导入口 | 主界面「装机向导」视图 → 访问通道阶段 |

## 1. 逐 AC 验收

### US1 · 代码与分发移除

**AC1 全文检索无残留 + 三处分发一致**

- AC 原文（spec §3）：*Given 仓库与打包产物 When 全文检索 frp/frpc/SakuraFrp/ddns-go/直连通道标识 Then 无残留实现代码与资源（frpc.exe 不随包分发；`$ScriptSubset`/manifest.json/resources 三者一致，构建产物无被删脚本的陈旧副本）。*
- 步骤：
  1. **静态部分（T20 已完成，见附录 A）**：仓库源码扫描——命中仅三类白名单（退役注记注释、迁移测试合法旧值夹具、退役保持负测）；`$ScriptSubset` 11 条与 manifest.json、`tools/sprint0/bin`、`resources/bin` 三方哈希一致；frpc.exe/ddns-go.exe 不在分发目录。
  2. **真机补充（闸门后）**：跑 `powershell scripts/build.ps1` 出 008 安装包 → 解包便携 zip（或安装后）核对：`resources\bin` 下无 `frpc.exe`/`ddns-go.exe`/`set-frp-key.ps1`/`clear-frp-key.ps1`/`config-ddnsgo.ps1`/`reset-ddns-password.ps1`；`manifest.json` 条目恰 11 条。
  3. 安装目录（NSIS 形态）同样核对一遍。
- 预期：两形态产物均无被删脚本与 frpc.exe 陈旧副本。
- 实测记录：＿＿＿＿；结论：☐通过 ☐不通过

**AC2 旧设置迁移不触发损坏修复**

- AC 原文：*Given 旧版 settings.json（`accessChannel` 为 `"direct"` 或 `"tunnel"`，含 `tunnel`/`tunnelEnabled`/`tunnelDisabled`/`directDisabled` 字段）When 新版本加载 Then `accessChannel` 迁移为 `"mesh"`，其余设置原样保留，被删字段在下次保存时自然消失；不触发损坏修复流程（不回默认、不改名 .bad）。*
- 步骤：
  1. 关闭工作台。备份 `%APPDATA%\ai-remote-workbench\settings.json`。
  2. 手工把其中 `accessChannel` 改为 `"tunnel"`（或 `"direct"`），确认含 `tunnel`/`tunnelEnabled` 等旧字段。
  3. 启动 008 版工作台 → 界面正常进入（无「设置文件损坏已恢复默认」提示、设置未回默认）。
  4. 随便改一项设置（如语言）触发保存 → 重开文件核对：`accessChannel` 为 `"mesh"`，`tunnel` 三字段与 `directDisabled` 已消失，语言/栈目录/组网配置原样。
- 预期：四步全中。
- 自动化已覆盖：settings.rs 迁移五态单测（direct/tunnel/异常值/无字段/字段保留）——真机验证文件级端到端。
- 实测记录：＿＿＿＿；结论：☐通过 ☐不通过

### US2 · 看板呈现

**AC3 看板组件卡 3→2**

- AC 原文：*Given 工作台运行 When 查看主看板 Then 组件状态卡仅 CloudCLI/Caddy 两张，无 ddns-go 卡及其停用提示文案。*
- 步骤：主界面走查：组件卡数量与名称；「启动全部」提示文案为两组件口径。
- 预期：恰两张卡；无 ddns-go 任何踪迹（含 i18n 文案）。
- 实测记录：＿＿＿＿；结论：☐通过 ☐不通过

**AC4 组网卡（无旧通道控件）**

- AC 原文：*Given 工作台运行 When 查看访问通道区 Then 呈现组网卡（组网服务态/对端列表/虚拟 IP/同步 DNS 与密钥指引入口），无通道切换单选、无停用 chip、无重新启用确认框、无隧道状态行与重启按钮。*
- 步骤：
  1. 主界面「组网通道」卡：核对头部（虚拟 IP + 在线成员数）、状态行（组网状态四态文案）、成员列表、DNS 指引条、常驻操作（写入组网密钥 / 安装·刷新服务 / 应用配置 / 同步 DNS）。
  2. 全卡搜旧控件：通道三选一单选组、「已停用」chip、「重新启用」按钮、隧道 ID/节点域名行、「重启穿透」按钮——均应不存在。
  3. 点「同步 DNS」→ toast 显示「DNS 同步完成（n 条记录操作）」；`nslookup ai.jackqi.cn 223.5.5.5` 返回虚拟 IP。
- 预期：新能力齐备、旧控件绝迹、同步动作端到端有效。
- 实测记录：＿＿＿＿；结论：☐通过 ☐不通过

**AC5 地址区三行齐备**

- AC 原文：*Given 工作台运行 When 查看地址区 Then 本机/局域网（`http://<ip>:3001`）/域名三行齐备可复制可打开，域名行心跳点照常（001 AC19 口径不回退）。*
- 步骤：核对三行（局域网行形态 `http://<内网IP>:3001`）；逐行复制/打开；域名行心跳点颜色与实际可达性一致。
- 预期：三行齐备；同网设备（手机连同一 WiFi）实测 `http://<ip>:3001` 可开。
- 实测记录：＿＿＿＿；结论：☐通过 ☐不通过

### US3 · 设置页与向导

**AC6 向导去分支化**

- AC 原文：*Given 打开装机向导 When 进入通道阶段 Then 无分支选择器，仅组网步骤（装服务 → 成员客户端指引 → 密钥 → 应用与在线校验 → 同步 DNS）；收尾页无停用旧通道入口。*
- 步骤：向导走查通道阶段（①密钥 → ②装服务 → ③成员指引/下载入口 → 应用与在线校验 → 同步 DNS）；收尾页核对无「停用穿透/直连」入口、组网完成提示含 uninstall-legacy.ps1 残留清理指引。
- 预期：无分支选择器与直连/穿透操作块；步骤序与文案为组网单线。
- 自动化已覆盖：wizard.rs derive_mesh 检测矩阵。
- 实测记录：＿＿＿＿；结论：☐通过 ☐不通过

**AC7 设置页无旧卡**

- AC 原文：*Given 打开设置页 When 查看 Then 无穿透设置卡（隧道 ID/节点域名/密钥/白名单/下载 frpc）、无旧通道停用卡、无 ddns-go 端口行；组网设置卡功能不回退。*
- 步骤：设置页走查：组网设置卡（网络名/虚拟 IP/网段/对端/密钥指引/服务管理）逐项核对仍在；只读区无 ddns-go 端口行；「打开栈目录」按钮在部署目录卡可用；自启描述为两组件口径。
- 预期：旧三处（穿透卡/停用卡/端口行）绝迹，组网卡与部署目录卡功能完整。
- 实测记录：＿＿＿＿；结论：☐通过 ☐不通过

**AC8 体检组网口径**

- AC 原文：*Given 组网在线 When 执行体检 Then 组网服务/对端项与 DNS 对齐项（A=虚拟 IP）正常工作，无穿透/直连专属体检项与指引。*
- 步骤：组网卡「开始体检」：六项逐条核对（DNS 对齐/网络归类 N/A/组网服务/Caddy/上游 3001/域名全链路）；无穿透/直连专属项。
- 预期：六项口径正确，全绿（或如实黄标）。
- 自动化已覆盖：judge_dns_mesh 判定矩阵。
- 实测记录：＿＿＿＿；结论：☐通过 ☐不通过

**AC9 工具区 4→2**

- AC 原文：*Given 低频工具区 When 展开 Then 仅「升级 CloudCLI」「安装客户端」两项（ddns-go 密码重置/管理页入口消失）。*
- 步骤：展开运维工具区清点条目。
- 预期：恰两项。
- 实测记录：＿＿＿＿；结论：☐通过 ☐不通过

### US4 · 真机一次性卸载（⛔ 前置闸门后执行）

**AC10 卸载编排幂等 + 双通道复测**

- AC 原文：*Given 007 验收通过的机器 When 执行 `uninstall-legacy.ps1` Then frpc/ddns-go 进程终止、`ddns-go Sprint0 autostart` 计划任务注销、栈目录 `ddns-go.exe`/`ddns-go.yaml`/`frpc.exe`/`frpc-run.log` 与 `.env` 的 `SAKURA_FRP_KEY` 行清除；重复执行幂等（已清理项跳过、退出码 0）；卸载后组网访问与局域网 IP 访问双通道复测正常（成员设备 + 同网设备）。*
- 步骤：
  1. 前置自检（速查表四命令）记录清理前状态；确认组网在线（看板「组网/在线」）。
  2. `powershell -NoProfile -ExecutionPolicy Bypass -File <stack>\bin\uninstall-legacy.ps1` → 观察台账输出（每步 Done/Skipped/Failed）与退出码 0。
  3. 四命令复测：进程无、任务报不存在、文件组消失、`.env` 无 SAKURA 行（**其余行原样**——重点核对 TENCENT 两行还在）。
  4. **幂等重跑**：再执行一次 → 全部步 Skipped、退出码 0。
  5. **双通道复测**：成员设备（蜂窝 + EasyTier）开 `https://ai.jackqi.cn` 正常；同网设备开 `http://<ip>:3001` 正常。
  6. 若台账报 CNAME 残留 → 工作台组网卡「同步 DNS」清理后复测。
- 预期：2~6 全中。
- 自动化已覆盖：T13 临时目录端到端演练（幂等重跑）；真机验证真实进程/任务/文件与访问链。
- 实测记录：＿＿＿＿；结论：☐通过 ☐不通过

**AC11 缺密钥拒删 yaml**

- AC 原文：*Given 栈 `.env` 缺腾讯密钥且 `ddns-go.yaml` 存有密钥的机器 When 执行卸载脚本 Then 脚本拒绝删除 yaml 并提示先运行 `set-tencent-key.ps1`（不自动搬运凭证）。*
- 步骤（在 AC10 之前的机器上演示，或演练机）：
  1. 备份 `<stack>\.env` → 临时注释/删除两行 TENCENT_SECRET_ID/KEY（演练后还原）。
  2. 执行卸载脚本 → 观察：ddns-go.yaml 步显示**拒绝删除** + 指引「先运行 set-tencent-key.ps1」；脚本不自动把 yaml 密钥写进 .env。
  3. 还原 `.env` 后重跑 → yaml 正常删除。
- 预期：拒绝 + 指引 + 不自动搬运；还原后清理不受阻。
- 实测记录：＿＿＿＿；结论：☐通过 ☐不通过

### US5 · 文档核对

**AC12 文档五项**

- AC 原文：*Given 本 Spec 验收 When 核对文档 Then specs/003 与 004 已归档并带取代注记、MOC 已流转、CHANGELOG Unreleased 有 Removed 条目、`.env.example` 无 SAKURA_FRP_KEY 段、`docs/adr/0004` 登记 settings 契约变更。*
- 步骤：逐项核对：`specs/archive/003-*`/`004-*` 存在且 spec.md 头部有取代注记；MOC 的 008 行/迭代表/主题导航与实际一致；CHANGELOG Unreleased 含 Removed（穿透/直连）与 Changed（组网单通道）条目；`.env.example` 仅存「已退役」注记段；`docs/adr/0004-settings-channel-migration.md` 状态 accepted。
- 预期：五项全中（T19 已完成落地，签收时终核）。
- 实测记录：＿＿＿＿；结论：☐通过 ☐不通过

## 2. 验收记录与签收

| AC | 结论 | 实测要点/日期 |
|----|------|---------------|
| AC1 | ✅ | 静态部分 T20 实测（附录 A：三方哈希一致 + 白名单外清零）+ 签收指令 2026-09-11 |
| AC2 | ✅ | 自动化五态矩阵覆盖 + 签收指令 2026-09-11 |
| AC3 | ✅ | 需求方签收指令 2026-09-11 |
| AC4 | ✅ | 需求方签收指令 2026-09-11（MeshCard 呈现随 008 日常使用） |
| AC5 | ✅ | 需求方签收指令 2026-09-11 |
| AC6 | ✅ | 需求方签收指令 2026-09-11（derive_mesh 矩阵自动化覆盖） |
| AC7 | ✅ | 需求方签收指令 2026-09-11 |
| AC8 | ✅ | 需求方签收指令 2026-09-11（judge_dns_mesh 矩阵自动化覆盖） |
| AC9 | ✅ | 需求方签收指令 2026-09-11 |
| AC10 | ✅ | 需求方签收指令 2026-09-11——真机卸载未单独实测，脚本随包保留（幂等台账已 T13 临时目录端到端演练） |
| AC11 | ✅ | 需求方签收指令 2026-09-11——拒删逻辑同上，执行时脚本自校验 |
| AC12 | ✅ | T19/T20 核对（归档/MOC/CHANGELOG/.env.example/ADR-0004 五项落地）+ 签收指令 2026-09-11 |

- 验收人：JackQi（需求方）　日期：2026-09-11
- **签收依据**：需求方 2026-09-11 指令「所有验收项标记成功」——版本固定 v0.4.0 时一并签收。真机卸载（AC10/AC11 实操）保留为可选动作：需要时按 §0 速查命令核验清理前后状态，脚本幂等可重复。
- 全部 ☐通过 后：spec.md 状态 → done、MOC 同步、tasks.md DoD 清单收尾。
- 发现不通过项：登记问题描述与复现步骤 → 修复 → 本文件追加「验收期修复记录」（004 惯例）。

## 附录 A · T20 回归基线实录（2026-09-11）

代码层全量回归与一致性校验（真机项见上，闸门后执行）：

| 项 | 结果 |
|---|---|
| `cargo test` | **190 passed / 0 failed / 3 ignored**（与 007 基线持平：008 删除的多为旧通道测试，新增迁移/收缩/负测相抵） |
| `npm run build` | 绿（tsc 类型门 + vite 产物零错误） |
| i18n 键集 | zh/en 各 **250 键**严格一致（tsx 双向差集为空） |
| 三处分发一致 | `$ScriptSubset` 11 条 = manifest.json 条目 = 双 bin 目录共同文件；三方 sha256 逐一吻合；manifest 对 sprint0 源全量复核 PASS |
| 分发目录残留 | `resources/bin` 与 `tools/sprint0/bin` 无 frpc.exe/ddns-go.exe/四被删脚本；仅存 exe 为 easytier-core/cli（007 资产） |
| 全仓残留扫描 | 白名单外清零。存留三类均为有意保留：① 退役注记注释（「已随通道退役——spec 008」类，20 处）② 迁移/负测夹具合法旧值（settings.rs 五态矩阵、orchestrator `registry_contains_no_ddnsgo`）③ 历史文档（CHANGELOG 历史版本、ADR-0001/0003、已交付 spec 001/005/006、调研报告、MOC/product 退役描述） |
| 扫描期连带修正 | Cargo.toml 包描述去 ddns-go；`scripts.rs` 凭证注释改 D3 单源口径；`stop.rs` 死代码 `pids_by_exe`（trait 方法+双实现+mock 字段）删除；`mesh.rs` FrpcOps 引用注释清理；`set-tencent-key.ps1` 头注释/` .env` 标记行/收尾指引去 ddns-go 与 config-ddnsgo 表述；`set-mesh-secret.ps1`/`enable-https.ps1` 注释同步；测试夹具 SAKURA_FRP_KEY/frp-can.com 改中性名；i18n 死键 `wizard.code.missing_ddnsgo` 删 + `no_records` 文案改组网口径 + `net.alert`/`net.riskPublic` 点明 3001/443 双规则（spec §6 Q5 闭合）——修正后 cargo test 复跑全绿、双脚本 Parser 校验过、BOM+CRLF 形态零漂移 |
