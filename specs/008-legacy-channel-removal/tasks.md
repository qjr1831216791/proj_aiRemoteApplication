# 008-legacy-channel-removal · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 进行中 <!-- 未开始 | 进行中 | 已完成 -->
- **最后更新**: 2026-09-11

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成
> 实施序按 plan §7-R2 依赖序：基础层 → dns_api/tunnel 解体 → 命令/向导 → 脚本/前端 → 文档/回归。

## 阶段 1: 立项

- [x] T1 立项三文档 + MOC 登记 + 分支 `feat/008-legacy-channel-removal`（spec/plan/tasks 按 007 风格；MOC 登记 Sprint 6 in-progress）（验收: 全部 AC 的前置；完成标志：文档入库提交 ✓ 2026-09-11）

## 阶段 2: Rust 基础层（测试先行）

- [x] T2 `settings.rs` 迁移：`AccessChannel` 单变体 `Mesh` + 自定义 Deserialize 映射（direct/tunnel→Mesh、异常值报错）；删 `TunnelConfig`/`tunnel`/`tunnel_enabled`/`tunnel_disabled`/`direct_disabled` 字段与 Patch 臂；`default_access_channel` 统一 Mesh；迁移五态单测（direct/tunnel/异常值/无字段/字段保留）（验收: AC2；完成标志：单测过、全库编译绿——下游引用随后续任务收敛）✓ 2026-09-11
- [x] T3 基础常量与组件身份：`consts.rs` 删 FRPC/DDNSGO 常量（`FRPC_ENV_FILE` 改名 `STACK_ENV_FILE` 保留——腾讯凭证仍用）；`probe.rs` 删 `ComponentId::DdnsGo`；`stop.rs` 删 `stop_ddnsgo`；`lang.rs` 删 `refuse_ddnsgo`、`stack_dir_missing` 文案改「缺少 caddy.exe」；`urls.rs` 删 `ExternalKind::DdnsAdmin`；相关测试同步（验收: AC1；完成标志：编译绿）✓ 2026-09-11
- [x] T4 `orchestrator.rs` 收缩：`COMPONENT_ORDER` 二元化、`channel_source`/`with_channel_source`/`current_channel` 删、`start_all` 的 ddns-go 跳过逻辑删、`stop_one`/`run_start`/`timeout_detail` 的 DdnsGo 臂删、Deserialize 分支删；`lib.rs` 装配点同步；测试改写（`start_all_skips_ddnsgo_*` 删，新增无 ddns-go 断言）（验收: AC3；完成标志：cargo test 绿）✓ 2026-09-11

## 阶段 3: dns_api 收敛与 tunnel 解体

- [x] T5 `dns_api.rs` 收敛：删 `parse_yaml_creds` 与 yaml 回退链（D3，凭据单源 .env）；删 `DnsTarget::Direct`/`Tunnel` 与 reconcile 对应臂、`sync_to_direct`/`sync_to_tunnel`、`purge_a_records`；保留 Mesh 臂与 `purge_cnames`、`sha256_hex`；`DnsAlignment` + `judge_dns_mesh` 自 `tunnel.rs` 迁入（含测试迁移）；`read_credential` 缺凭证 warn 文案去 yaml 表述（验收: AC2/AC8；完成标志：单测绿）✓ 2026-09-11（偏离：`purge_cnames` 亦删——见文末附注②）
- [x] T6 `tunnel.rs` 解体：删整文件（FrpcOps/TunnelManager/守护/退避/日志分类/flush_dns/switch_actions/disable_actions/事件 EVENT_TUNNEL_STATUS/TunnelState/TunnelStatus/ChannelSource）；`lib.rs` 删 mod/装配/`spawn_guard`/退出钩子/`AppChannelSource`/`TauriTunnelEmitter`，核 `SharedHealth` 全部消费点（plan R6）；删 `examples/dns_sync_probe.rs`；`mesh.rs` 注释清理（"frpc 先例"表述）（验收: AC1；完成标志：cargo test 全绿，无 tunnel 模块残留引用）✓ 2026-09-11

## 阶段 4: 命令层与向导

- [x] T7 `commands.rs` 收缩：删九命令（plan §5 列表）+ 注册表（lib.rs invoke_handler）；`check_dns_alignment` mesh-only；`purge_dns_records`/`parse_disable_target`/`sync_ddnsgo_autostart`/`stop_ddnsgo_and_unmanage` 删；warn 文案与测试同步（验收: AC1/AC8；完成标志：cargo test 绿）✓ 2026-09-11
- [x] T8 `wizard.rs` 收缩：删 `derive_direct`/`derive_tunnel`/`direct_detail_with_dns`/`cgnat_classify`/`fetch_public_ipv4`/`probe_channel_stage` 的 Direct/Tunnel 臂；`derive_https` 删 `ddns-go.exe` 必要条件；向导 branch 字段退役（旧值忽略）与 `wizard_set_branch` 命令删；测试同步（验收: AC6；完成标志：cargo test 绿）✓ 2026-09-11
- [x] T9 `scripts.rs` 收缩：删 `Script::{SetFrpKey,ClearFrpKey,ResetDdnsPassword,ConfigDdnsGo}` 与 `ToolKind` 四值、`ddns_go_run()`、file_name/visibility/timeout/tool_plan 各臂；`SetupAutostart` exit-1 文案改；域名透传 `matches!` 收窄至 InstallHttps；测试同步（验收: AC9；完成标志：cargo test 绿）✓ 2026-09-11
- [x] T10 `autostart.rs` 收缩：删 `DDNSGO_TASK_NAME`/`SERVICE_TASK_NAMES` 收二元/`set_ddnsgo_autostart`/`service_task_remove_spec`（若无其他消费方）；测试同步（验收: AC3；完成标志：cargo test 全绿）✓ 2026-09-11

## 阶段 5: 脚本与资源（可与阶段 4 并行，文件不相交）

- [x] T11 资源删除：`resources/bin/` 与 `tools/sprint0/bin/` 删 `set-frp-key.ps1`/`clear-frp-key.ps1`/`config-ddnsgo.ps1`/`reset-ddns-password.ps1`（双份）+ `resources/bin/frpc.exe`；删 `installer-hooks.nsh` 与 `tauri.conf.json` 的 `installerHooks` 引用；`build.ps1` `$ScriptSubset` 删四条 + 产物描述改；`manifest.json` 条目收缩再生成（保持 BOM+LF 形态，007 T7 惯例）（验收: AC1；完成标志：manifest 与目录文件一致）✓ 2026-09-11（双副本 sha256 与 manifest 三方核对一致）
- [x] T12 存量脚本收缩：`setup-autostart.ps1`（双份）删 ddns-go 组件条目与提示；`install-https.ps1`（双份）删 `-DdnsZip`/`Install-StackComponent`/ddns-go 下载落位/步骤 5 拉起/汇总提示，头注释更新（Caddy 构建/Caddyfile/Protect-StackDir 保留）；`tools/sprint0` 的 `autostart-on/off.bat` 注释、`install-https.bat`、`menu.ps1` ddns 菜单项、`README.md` 同步；BOM+CRLF/纯 ASCII（bat）纪律（验收: AC1；完成标志：Parser 校验过）✓ 2026-09-11（Protect-StackDir 保留确认）
- [x] T13 新建 `uninstall-legacy.ps1`（双份 + $ScriptSubset + manifest 登记）：幂等卸载编排——前置校验（EasyTierMesh 服务在运行；`.env` 腾讯密钥就绪否则按 AC11 拒绝删 yaml 并指引 `set-tencent-key.ps1`）→ 停 frpc/ddns-go 进程 → 注销 `ddns-go Sprint0 autostart` → 删栈目录 `ddns-go.exe`/`ddns-go.yaml`/`frpc.exe`/`frpc-run.log` → `.env` 删 `SAKURA_FRP_KEY` 单行（BOM 保留）→ CNAME 残留检测（Resolve-DnsName，存在则指引工作台「同步 DNS」）→ 汇总报告；`-Lang` 双语、BOM+CRLF、PARSER 校验（验收: AC10/AC11；完成标志：临时目录端到端演练幂等重跑）✓ 2026-09-11（真机执行仍受前置闸门约束，见 T20）

## 阶段 6: 前端（依赖阶段 2~4 的命令契约定型）

- [ ] T14 装配链收敛：`types.ts`（`AccessChannel="mesh"`、删 `TunnelConfig`/`TunnelStatus`/direct·tunnel 相关类型与 ToolKind 四值/DnsAlignment 直连变体）、`api.ts`（删九封装 + `onTunnelStatus`）、`App.tsx`（删 tunnelStatus 状态/兜底/订阅/透传）、`MainView.tsx`（删 ddnsgo 停用提示块 `:161-167` 与 tunnelStatus prop）（验收: AC1/AC3；完成标志：tsc 过）
- [ ] T15 `TunnelCard.tsx`→`MeshCard.tsx` 重构（D5）：删三通道单选/切换确认/停用 chip/重新启用/隧道状态行/重启按钮/穿透与直连体检臂/DnsNotice 直连穿透分支；保留 mesh 体检两项与 DnsNotice mesh 分支；卡片头部增虚拟 IP + 在线成员数（`mesh_status` 数据源）；密钥指引/装服务/同步 DNS 常驻入口（T13 向导同款能力下沉）；六个文案选择函数塌缩（验收: AC4/AC8；完成标志：呈现走查过）
- [ ] T16 `SettingsView.tsx` 收缩：删穿透设置卡（`:539-615`）/旧通道停用卡（`:431-537`）/`saveTunnel`/`openSetFrpKey`/`runClearFrpKey`/ddns-go 端口行；自启区随任务收缩；组网设置卡不动（验收: AC7；完成标志：呈现走查过）
- [ ] T17 `WizardView.tsx` 去分支化：删分支选择器/直连操作区（`:343-362`）/tunnel 弃用提示（`:363-365`）/收尾页停用入口；通道阶段塌缩 mesh 单线步骤；`wizardSetBranch` 调用删（验收: AC6；完成标志：向导走查过）
- [ ] T18 `ToolsSection.tsx` 4→2 + i18n 全量清理：删 ddns 两工具；zh/en 删约 70 键（`tunnel.channelDirect`/`switchTo*`/`channel.disabled.*`/`tools.resetDdns*`/`openDdnsAdmin*`/`component.ddnsgo`/`settings.portDdnsgo`/`ddnsOffIn*`/`wizard.channel.direct*`/直连专属 code 键/穿透七键——007 T13 已清——核对残留）；**保留** mesh 共用键（`tunnel.channelMesh`/`tunnel.check.mesh`/`mesh.*` 等）与 `tunnel-form-*` CSS；`app.css` 删 `.wizard__tunnelState`；两侧同步缺键即挂（验收: AC9；完成标志：`npm run build` 绿）

## 阶段 7: 文档与回归

- [ ] T19 文档收尾：`docs/adr/0004-settings-channel-migration.md`（accessChannel 契约变更与迁移映射）；`specs/003`/`specs/004` 移入 `specs/archive/` 并在 spec.md 头部加取代注记；MOC 流转（008 行、主题导航、003/004 移档）；`CHANGELOG.md` Unreleased（Removed: frp 通道/直连通道与 ddns-go/相关 UI；Changed: 通道收敛组网单通道）；`.env.example` 删 SAKURA_FRP_KEY 段（TENCENT 注释去 ddns-go 表述）；`docs/product.md` 路线图补 008（验收: AC12；完成标志：核对清单过）
- [ ] T20 全量回归 + 手工验收清单：`cargo test` 全绿 + `npm run build` 绿 + `$ScriptSubset`/manifest/resources 三处一致性校验 + 全文检索 frp/ddns-go 残留扫描（白名单：历史 CHANGELOG/调研/specs 归档）；建 `acceptance-manual.md`（AC1~AC12 逐条步骤/预期/实测留白 + 真机卸载编排执行清单，显著标注前置闸门：007 T14 签收 + O1 闭合）（验收: 全部；完成标志：回归绿 + 清单入库）

## 完成标志（DoD 检查）

- [ ] spec.md 中所有 AC 已逐条验证通过（AC10/AC11 真机项待闸门满足后执行回填）
- [ ] 自动化测试全部通过（迁移矩阵/编排收缩/DNS 判定回归/manifest 一致性）
- [ ] 相关文档已更新（spec/plan/tasks/MOC/CHANGELOG/ADR-0004/.env.example/归档）
- [ ] 本文件全部任务勾选完毕

---

## 附注：审核依据（2026-09-11，任务拆解的事实底座）

- frp 触点全清单：`tunnel.rs` 1188 行（frp 核心）/`commands.rs` 九命令/`installer-hooks.nsh` 唯一职责为装前杀 frpc/`frpc.exe` 随包分发（`consts.rs:38` 注释"不入库"与现状不符，T11 删除即消除漂移）。
- ddns-go 渗透面：`COMPONENT_ORDER` 三元组、向导 HTTPS 阶段必要条件（`wizard.rs:284-294`）、`setup-autostart.ps1` Check 判据、`scripts.rs`/`autostart.rs`/`lang.rs` 的「缺少 caddy.exe / ddns-go.exe」exit-1 语义——五处必须同步收缩，否则新装机/自启校验永久 pending。
- 凭据双副本：`.env`（BOM）与 `ddns-go.yaml`（明文）同源不同步，yaml 回退链删（D3）；`set-tencent-key.ps1:11-13` 头注释列三消费方（Caddy/dns_api/ddns-go），T12 改注释。
- 前端：地址区 lan 行 = `http://<ip>:3001`（`urls.rs:100-102` 实证）——局域网 IP 方案的 UI 呈现载体已存在，008 仅保不建。
- `Protect-StackDir` 居 `install-https.ps1`，easytier 目录 ACL 依赖它（007 plan §4.2）——T12 收缩脚本时必须保留该函数。

### 实施偏离记录

1. **② `purge_cnames` 一并删除（T5）**：plan §5 原文「保留 Mesh 臂与 `purge_cnames`」，但收敛后 `purge_cnames` 在 Rust 侧的唯一消费方是 `disable_legacy_channel`（T7 删除）与 `purge_dns_records`（T7 删除）；存量 CNAME 的清理由两条更可靠的路径承接——`dns_api::sync_to_mesh`（mesh_sync_dns 命令，建 A 记录的同时删 CNAME）与 `uninstall-legacy.ps1` 的 CNAME 残留检测（指引工作台「同步 DNS」）。保留即为死代码，故删；spec AC8 的「残留 CNAME 判旁路暴露面」由 `judge_dns_mesh` 的 `MismatchedCname` 判定承接，不受影响。
2. **T1 提交时间线**：T1 文档提交（0c68f1b）在状态 `draft → reviewed` 与实施授权同日完成，依据用户指令「起草，拆分，实施」一次性放行（宪法工作流第 1/2 步的用户确认由该指令合并给出）。
