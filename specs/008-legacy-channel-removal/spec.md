# 008-legacy-channel-removal · 需求规格（spec）

> 导航：[plan.md](./plan.md) · [tasks.md](./tasks.md) · 返回 [MOC](../MOC.md)

- **状态**: done <!-- draft | reviewed | in-progress | done | archived -->
- **迭代**: Sprint 6
- **创建日期**: 2026-09-11
- **最后更新**: 2026-09-11（需求方指令签收全部验收项 → done；AC1 静态部分 T20 实测（acceptance-manual 附录 A），AC2 自动化五态覆盖，AC10/AC11 真机卸载按签收指令免单独实测——`uninstall-legacy.ps1` 随包保留，真机执行时脚本内置组网在线与凭证双重前置校验）

## 1. 背景与问题

007 已交付组网通道（EasyTier legacy 模式，零公网暴露）并实现直连/frp 的「停用不删除」。需求方 2026-09-11 终局决策：远程访问**只保留两条方案**——

1. **局域网 IP 直访**：`http://<局域网IP>:3001`（sprint0 原始形态，3001 防火墙规则 Private 范围）；
2. **EasyTier 组网**：`https://ai.jackqi.cn` → A 记录（私网段虚拟 IP）→ TUN → Caddy:443 → 3001。

域名 HTTPS 链（Caddy + DNS-01 + 腾讯密钥）**保留**（组网访问形态选型"分支 A"，需求方 2026-09-11 经选项确认锁定）。直连（DDNS/ddns-go）与穿透（frp/SakuraFrp）两条旧通道从**代码、分发、真机**三层面彻底移除。

全仓审核结论（2026-09-11，四路并行盘点 + 前端外观走查，要点沉淀于 [plan.md](./plan.md) §2/§8）：

- **ddns-go 移除**：组网态职责归零（A 记录指向固定虚拟 IP，由工作台 `sync_to_mesh` 接管），且是 A 记录回写公网 IP 的互踩源；
- **frpc 移除**：007 §1 威胁模型的质疑源头（账户级 key、命令行明文、第三方节点可见元数据）；
- **Caddy 保留**：全仓唯一 TLS 终结点，组网访问 `10.126.126.1:443` 由其服务，DNS-01 签发续期免公网入站；
- **腾讯密钥保留**：DNS-01 续期 + DNS 调和（A=虚拟 IP）仍消费；`ddns-go.yaml` 明文副本随卸载销毁，凭据单源化到栈 `.env`；
- **3001 防火墙规则保留**：它就是局域网方案的载体；443 规则随 Caddy 保留。

007 §2 非目标写明「彻底移除留待组网通道稳定后的后续版本」——本 Spec 即该后续版本；007 的「停用」编排（disable_legacy_channel 等）被本 Spec 的「移除」取代。

## 2. 目标与非目标

### 目标（Goals）

- **代码与分发层移除直连与穿透通道的全部实现**：Rust 后端（守护/切换/停用编排/命令）、前端（通道切换/停用 UI/设置卡/向导分支）、脚本与资源（key 脚本/ddns 配置脚本/frpc.exe/NSIS 钩子）、打包清单（$ScriptSubset/manifest）。
- **通道模型收敛为组网单通道**：`AccessChannel` 收敛为 `Mesh` 单变体，旧值（`direct`/`tunnel`）加载时迁移映射为 `mesh`，其余设置字段不丢；持久化契约变更登记 ADR。
- **前端适应性改造**（非纯删除）：通道卡重构为组网卡（服务态/对端/虚拟 IP/DNS 同步常驻入口），看板组件卡 3→2，设置页删穿透卡与停用卡，向导通道阶段去分支化，低频工具区 4→2；**地址区本机/局域网/域名三行保留**——局域网 IP 方案在此保持第一等呈现。
- **真机一次性卸载编排**：`uninstall-legacy.ps1`（幂等）——停 frpc/ddns-go 进程、注销 ddns-go 自启任务、清除栈目录 `ddns-go.exe`/`ddns-go.yaml`/`frpc.exe`/`frpc-run.log` 与 `.env` 中 `SAKURA_FRP_KEY` 行；执行前置校验腾讯密钥就绪（防删 yaml 后 DNS-01 断链）。
- **文档同步**：003/004 归档、MOC 流转、CHANGELOG Removed、`.env.example` SAKURA 段移除、ADR-0004。

### 非目标（Non-Goals · 本期明确不做）

- **不动 HTTPS 栈**：Caddy、Caddyfile 渲染、DNS-01 签发续期、上游 3001 均不变（spec 007 非目标的延续）。
- **不动 EasyTier**：服务/TUN/密钥脚本/对端配置/状态探询均为现役资产。
- **不动 005 心跳 / 002 网络归类**：心跳经域名（→虚拟 IP）本机探测在组网态照常有效；网络归类同时守护 3001（局域网）与 443（组网）两条 Private 规则，职责加重不改。
- **不做卸载的 UI 入口**：一次性操作走脚本（YAGNI；003「低频外部操作给入口不代劳」哲学的极形式）。
- **不改公网 DNS 终态语义**：A→虚拟 IP 维持（分支 A），CNAME 残留由既有 `mesh_sync_dns` 清理，卸载脚本只做检测与指引。
- **不清理历史文档**：CHANGELOG 历史版本、调研报告中的 frp/直连记录属历史存档，不回改。

## 3. 用户故事与验收标准

### US1: 作为服务端用户，我希望旧通道从代码与分发中彻底消失，以便维护面收敛到组网单通道。

- [x] **AC1**: Given 仓库与打包产物 When 全文检索 frp/frpc/SakuraFrp/ddns-go/直连通道标识 Then 无残留实现代码与资源（frpc.exe 不随包分发；`$ScriptSubset`/manifest.json/resources 三者一致，构建产物无被删脚本的陈旧副本）。
- [x] **AC2**: Given 旧版 settings.json（`accessChannel` 为 `"direct"` 或 `"tunnel"`，含 `tunnel`/`tunnelEnabled`/`tunnelDisabled`/`directDisabled` 字段）When 新版本加载 Then `accessChannel` 迁移为 `"mesh"`，其余设置（语言/栈目录/自启/组网配置等）原样保留，被删字段在下次保存时自然消失；不触发损坏修复流程（不回默认、不改名 .bad）。

### US2: 作为服务端用户，我希望看板只呈现现役组件与组网通道，以便状态一目了然。

- [x] **AC3**: Given 工作台运行 When 查看主看板 Then 组件状态卡仅 CloudCLI/Caddy 两张，无 ddns-go 卡及其停用提示文案。
- [x] **AC4**: Given 工作台运行 When 查看访问通道区 Then 呈现组网卡（组网服务态/对端列表/虚拟 IP/同步 DNS 与密钥指引入口），无通道切换单选、无停用 chip、无重新启用确认框、无隧道状态行与重启按钮。
- [x] **AC5**: Given 工作台运行 When 查看地址区 Then 本机/局域网（`http://<ip>:3001`）/域名三行齐备可复制可打开，域名行心跳点照常（001 AC19 口径不回退）。

### US3: 作为服务端用户，我希望设置页与装机向导不再出现旧通道踪迹，以便新机部署路径唯一。

- [x] **AC6**: Given 打开装机向导 When 进入通道阶段 Then 无分支选择器，仅组网步骤（装服务 → 成员客户端指引 → 密钥 → 应用与在线校验 → 同步 DNS）；收尾页无停用旧通道入口。
- [x] **AC7**: Given 打开设置页 When 查看 Then 无穿透设置卡（隧道 ID/节点域名/密钥/白名单/下载 frpc）、无旧通道停用卡、无 ddns-go 端口行；组网设置卡（网络名/虚拟 IP/网段/对端列表/密钥脚本指引）功能不回退。
- [x] **AC8**: Given 组网在线 When 执行体检 Then 组网服务/对端项与 DNS 对齐项（A=虚拟 IP）正常工作，无穿透/直连专属体检项与指引。
- [x] **AC9**: Given 低频工具区 When 展开 Then 仅「升级 CloudCLI」「安装客户端」两项（ddns-go 密码重置/管理页入口消失）。

### US4: 作为服务端用户，我希望有一条一次性脚本把真机上的旧通道残留清干净，以便暴露面与磁盘归零。

- [x] **AC10**: Given 007 验收通过的机器 When 执行 `uninstall-legacy.ps1` Then frpc/ddns-go 进程终止、`ddns-go Sprint0 autostart` 计划任务注销、栈目录 `ddns-go.exe`/`ddns-go.yaml`/`frpc.exe`/`frpc-run.log` 与 `.env` 的 `SAKURA_FRP_KEY` 行清除；重复执行幂等（已清理项跳过、退出码 0）；卸载后**组网访问与局域网 IP 访问双通道复测正常**（成员设备 + 同网设备）。
- [x] **AC11**: Given 栈 `.env` 缺腾讯密钥且 `ddns-go.yaml` 存有密钥的机器 When 执行卸载脚本 Then 脚本拒绝删除 yaml 并提示先运行 `set-tencent-key.ps1`（不自动搬运凭证——密钥经用户交互输入，不进脚本参数/日志）。

### US5: 作为维护者，我希望文档如实反映这次退役，以便规格与实现不漂移。

- [x] **AC12**: Given 本 Spec 验收 When 核对文档 Then specs/003 与 004 已归档并带取代注记、MOC 已流转、CHANGELOG Unreleased 有 Removed 条目、`.env.example` 无 SAKURA_FRP_KEY 段、`docs/adr/0004` 登记 settings 契约变更。

## 4. 功能需求与边界

- **settings 迁移**：`AccessChannel` 自定义反序列化——`"mesh"`→Mesh，`"direct"`/`"tunnel"`→Mesh（迁移映射），其余值报错走既有损坏修复路径；字段默认值与新装机默认统一 Mesh；被删字段（tunnel 三项/direct_disabled）由 serde 忽略未知字段语义自然丢弃。
- **tunnel.rs 解体**：frp 专属（FrpcOps/TunnelManager/守护线程/退避/日志分类/flush_dns/事件）与通道编排（switch_actions/disable_actions）全删；幸存者（`DnsAlignment` mesh 判定、`judge_dns_mesh`）迁入 `dns_api.rs`；`tunnel://status` 事件随守护消亡。
- **命令层收敛**：删 `get_tunnel_status`/`switch_channel`/`set_tunnel_enabled`/`restart_tunnel`/`clear_frp_key`/`get_defender_exclusion_cmd`/`download_frpc`/`disable_legacy_channel`/`wizard_set_branch` 九命令；`check_dns_alignment` 收敛 mesh-only；`run_dns_probe`/`mesh_*` 族保留。完整契约表见 plan §5。
- **凭据单源化**：`dns_api::read_credential` 删 `ddns-go.yaml` 回退链（`parse_yaml_creds` 删），唯一来源栈 `.env`。
- **编排器收缩**：`COMPONENT_ORDER` 三元→二元（CloudCLI/Caddy）；`channel_source`/`with_channel_source` 及 start_all 的 ddns-go 跳过逻辑随 ddns-go 消亡（mesh 服务由 SCM 承载，本就不在 COMPONENT_ORDER）。
- **向导收缩**：通道阶段去分支化（mesh 单线步骤）；`derive_direct`/`derive_tunnel`/`direct_detail_with_dns`/`cgnat_classify`/`fetch_public_ipv4` 删；HTTPS 阶段的 `ddns-go.exe` 落位必要条件判定删（否则新装机永久 pending）；向导状态中的 branch 字段退役（旧值忽略）。
- **脚本与分发**：删 `set-frp-key.ps1`/`clear-frp-key.ps1`/`config-ddnsgo.ps1`/`reset-ddns-password.ps1`（resources/bin 与 tools/sprint0/bin 双份）、`frpc.exe`、`installer-hooks.nsh`（连带 tauri.conf.json `installerHooks` 引用）；`setup-autostart.ps1` 删 ddns-go 组件条目；`install-https.ps1` 删 `-DdnsZip`/`Install-StackComponent`/ddns-go 下载与拉起步骤（Caddy 构建与 Caddyfile 生成、`Protect-StackDir` 保留——easytier 目录 ACL 依赖后者）；`build.ps1` `$ScriptSubset` 与 manifest.json 同步收缩（006 坑：漏登记/漏清理即打包漂移）。
- **卸载编排**：`uninstall-legacy.ps1`（BOM+CRLF、`-Lang` 双语惯例沿 sprint0 规范）：前置校验（组网服务在运行/在线可选校验、`.env` 腾讯密钥就绪——AC11）→ 停进程 → 注销任务 → 删文件 → 清 key 行 → CNAME 残留检测（存在则提示经工作台「同步 DNS」清理，不直接调 API）→ 汇总报告。
- **前端**：`TunnelCard.tsx`→`MeshCard.tsx` 改名重构（007 保留旧名是为避免无谓 diff，008 大动一次改净）；i18n zh/en 同步删键补键（缺键即 tsc 挂的既有纪律兜底）。

## 5. 约束与假设

- **时序闸门（真机执行侧）**：`uninstall-legacy.ps1` 的真机执行与本版本安装，前置 **007 T14 验收签收 + O1（TUN 网卡防火墙归类）闭合 + 组网稳定观察期**；代码实施与闸门并行不悖（仓库先行，真机殿后）。
- **测试纪律**：迁移映射/编排收缩/DNS 判定等逻辑类 AC 自动化测试先行；GUI 呈现类 AC 走手工验收清单（宪法 §1 窄例外）。
- **回归红线**：每任务组提交前 `cargo test` 与 `npm run build` 必须全绿（宪法 §1）。
- 假设：需求方真机已切组网通道（007 真机联调已打通）；存量 `settings.json` 中 `"direct"`/`"tunnel"` 值均经迁移映射兜底，无手工构造的异常值。

## 6. 开放问题

- [x] 组网访问形态（域名 HTTPS vs 纯 IP）→ **域名 HTTPS（分支 A）**：Caddy/腾讯密钥/心跳/DNS 对齐保留（需求方 2026-09-11 经选项锁定）。
- [x] 卸载编排入口（UI 按钮 vs 纯脚本）→ **纯脚本**：一次性操作，YAGNI。
- [x] `TunnelCard.tsx` 是否改名 → **改**（`MeshCard.tsx`）：008 本就重构该卡，一次改净避免永久性名不副实。
- [x] ddns-go.yaml 中的腾讯密钥是否自动迁移到 `.env` → **不自动**：脚本拒绝并指引用户经 `set-tencent-key.ps1` 交互输入（密钥不经脚本参数/日志，003 哲学）。
- [x] 网络归类卡（002）hint 文案是否升级（点明"同时守护 3001/443 两条 Private 规则"）→ **升级**（T20）：现成键 `net.alert`/`net.riskPublic` 双语改写即点明双规则，无需新增键。

## 7. 变更记录

| 日期 | 变更内容 | 原因 |
|------|----------|------|
| 2026-09-11 | 初稿（直接 in-progress） | 需求方终局决策"只留局域网 IP 直访 + EasyTier 组网两条方案"，经四路全仓审核（frp/ddns-go 触点、Caddy 与 DNS 凭据依赖、卸载能力与文档影响面）与前端外观审核定稿范围；组网访问形态经选项确认锁定分支 A（保留域名 HTTPS）。状态跳过 draft→reviewed 常规流转，依据需求方同日「起草，拆分，实施」的明确指示 |
