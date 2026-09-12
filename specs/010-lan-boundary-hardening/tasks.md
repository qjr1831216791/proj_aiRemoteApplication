# 010-lan-boundary-hardening · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 未开始 <!-- 未开始 | 进行中 | 已完成 -->
- **最后更新**: 2026-09-12

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成
> 通用完成标志（每个任务组末尾不再重复）：`cargo test --manifest-path apps/workbench/src-tauri/Cargo.toml` 与 `npm run build`（cd apps/workbench）全绿 + git 提交（信息注明 `specs/010-lan-boundary-hardening T<N>`）。涉及脚本的提交须核对双目录一致（tools/sprint0/bin ↔ apps/workbench/src-tauri/resources/bin）。

## 阶段 1: 契约底座（脚本层）

- [x] **T1 取证归档与契约定稿**（验收: 全部 AC 的契约锚点；依赖: 无）
  - **契约冻结（2026-09-12 复核通过，后续任务不得擅改）**：
    - 规则名：443 白名单 = `CloudCLI Mesh HTTPS 443`（Profile Any + RemoteAddress=CIDR + InterfaceAlias=TUN）；3001 例外 = `CloudCLI LAN 3001 Exception`（**Private + RemoteAddress LocalSubnet，无接口条件**）；
    - 旧规则退役清单（migrate 幂等删除）：`CloudCLI LAN HTTPS 443` + `CloudCLI LAN 3001`；
    - 例外 TTL = **12h**（`now-since == 12h` 即到期回落）；健康轮询 = **60s**；
    - status JSON 契约（plan §4.2）：`legacy443 / legacy3001 / mesh443{present,remote,iface,profile} / exc3001{present,profile} / tun{name,ip}`，压缩 JSON + UTF8 输出，`profile` 沿实测 flags（0=Any/1=Domain/2=Private/4=Public）；
    - lan-guard 退出码：**0** 成功/幂等跳过；**1** 前置不满足（非管理员/参数非法）；**3** TUN 未解析（`-WaitTun` 耗尽，不建规则——休眠非异常）。
  - 取证复核（2026-09-12 本机只读 PowerShell）：两条旧规则实测均 Profile=Private、RemoteAddress=Any、InterfaceAlias=Any（007 O1 的 Any 漂移不可复现）；TUN=`et_8_1999`（ifIndex 60，持有 10.126.126.1/24）；新契约两规则尚不存在。回填 spec 开放问题 Q1/Q2/Q3/Q5（Q6 保持另立）。
  - 冻结契约常量定稿：`CloudCLI Mesh HTTPS 443` / `CloudCLI LAN 3001 Exception` / 旧规则退役清单 `CloudCLI LAN HTTPS 443` + `CloudCLI LAN 3001` / 12h TTL / 60s 轮询 / status JSON 契约（plan §3.2、§4.2）——写入本任务备注，后续任务不得擅改。
  - 回填 [spec.md](./spec.md) 开放问题：Q1（规则 Profile 复验）、Q2（002 去留 → plan §3.6）、Q3（迁移时机 → plan §3.7）、Q5（TUN 绑定形态 → plan §3.3）勾选并注明已决位置；Q6 保持另立不动。**不流转 spec 状态**。
  - 完成标志：spec 开放问题四处回填；本文件 T1 勾选；git 提交。
- [x] **T2 lan-guard.ps1 与装机脚本改造**（依赖: T1）（验收: AC1/AC6/AC10）——2026-09-12 完成：五动作 UAC 实跑通过（status JSON 可解析、ensure-whitelist 幂等+失配 Remove/New 还原、例外 on/off 往返、exit 3/1 分支）；**migrate 端到端移交 T7**（本机两条旧规则为其验收样本，本批仅语法+幂等逻辑走查）；实测发现并修复 CIDR 掩码存储形态坑（Windows 存 /24 为 255.255.255.0，脚本 Convert-ToCidrForm 归一，Rust 侧比对输入已是 /24）；双目录 12 文件哈希全 MATCH；单测 8 项（红测曾抓到 VirtualIp 泄入例外派发的契约偏差）
  - 测试先行：新增 `lan_guard.rs` 契约常量与派发参数构造的失败单测（动作集、参数串含 `-Cidr/-VirtualIp/-WaitTun` 且经单引号转义、exception-on 参数含 `-Profile Private -RemoteAddress LocalSubnet` 且无 TUN 条件——先红后绿，随 T3 完成实现）。
  - 新建 `tools/sprint0/bin/lan-guard.ps1`（BOM+CRLF、`-Lang` 双语 T()、管理员自检 + 拒绝提示、幂等）：五动作 status / ensure-whitelist / exception-on / exception-off / migrate，退出码契约 0/1/3（plan §5.1），status 输出 plan §4.2 压缩 JSON（UTF8 前缀）。
  - `install-server.ps1`：删除步骤 5（防火墙规则）与步骤 6（网络归类改专用），步骤重编号为 5/7。
  - `enable-https.ps1`：步骤 1 改为调同目录 `lan-guard.ps1 -Action ensure-whitelist`（`-Cidr/-VirtualIp` 可选参数带默认值，TUN 未就绪按 exit 3 跳过并如实提示）；步骤 2（全网络改专用）删除；hosts 钉定步保留；头注释同步。
  - `install-client.ps1` 排障提示与 `menu.ps1` 局域网行文案改如实措辞（局域网直访默认已收口）。
  - 打包登记（006/008 漂移教训）：`lan-guard.ps1` 登记 `scripts/build.ps1` $ScriptSubset；手工同步至 `resources/bin/` 并以 build.ps1 §2 同款逻辑重算 manifest.json 哈希；`Get-FileHash` 比对双目录一致。
  - 完成标志：五动作本地命令行实测各一轮（status JSON 可解析、exception-on/off 幂等往返、migrate 干跑——真机为本机，跑完把例外关闭恢复原状）；双目录哈希一致；全仓 grep 确认旧规则创建逻辑仅剩 lan-guard 删除清单引用；git 提交。
- [x] **T3 lan_guard.rs 模块（测试先行）**（依赖: T1；可与 T2 并行）（验收: AC1/AC5/AC8/AC9 判定面）——2026-09-12 完成：15 项新单测全绿（T2 的 8 项一并保持），全仓 cargo test 219 通过；CIDR 契约依赖（脚本 Convert-ToCidrForm 归一 /24 输出）以单测钉死（掩码形态必判 stale_cidr）；tick 探测失败轮不做回落决策（缓存回退 ≠ 实况，盲动防线，单测锁定）
  - 纯函数层：status 参数与 JSON 解析（容错缺字段，沿 parse_net_status 先例）；`judge_health` 全分支（ok/missing/stale_cidr/stale_iface/dormant × exception off/on/expired/pending × legacy_present × public_blocks_exception——覆盖 plan §3.4 状态表每行）；`exception_expired`/`exception_remaining_secs` 边界（==12h 即到期）；回落决策矩阵（enabled ∧ expired ∧ 规则在 → UAC 派发；规则不在 → 免 UAC 清标记）；派发参数构造（ps_quote 单引号翻倍、`-WindowStyle Hidden`）。
  - `LanGuardMonitor`（NetMonitor 同构：probe/sink/缓存/变化才发声 `languard://changed`、60s tick；单测以脚本化 probe 输出驱动状态机）；启动补回落自检逻辑（spawn 首轮 tick 承载，计划注释注明）。
  - 完成标志：上述单测全绿（T2 红测一并转绿）；git 提交。
- [x] **T4 settings 扩展与命令层联动**（依赖: T3）（验收: AC6/AC7/AC8/AC9）——2026-09-12 完成：四命令接线 + LanGuardMonitor 装配 spawn（首轮 tick=启动补回落自检）；组合派发单窗单 UAC（服务段在前、白名单段在后、收尾提示殿后，`-WaitTun 20` 内置）；UAC 拒绝路径 Err 且设置不写（AC7）；off 复测确认循环（规则已不在才清标记）；全仓 cargo test 224 绿 + tsc/vite build 绿
  - 测试先行：`LanGuardSettings` serde default（旧文件缺字段 → false/0）与 roundtrip；patch 整块写入；apply 组合派发构造断言（prepare 校验失败 → 参数串不含 ensure-whitelist 段；通过 → 服务动作 + ensure-whitelist 同窗顺序、单次 UAC）。
  - `settings.rs`：`lan_guard: LanGuardSettings { exception_enabled, exception_since_ms }`（#[serde(default)]）+ SettingsPatch 扩展 + 单测。
  - `commands.rs`：`lan_guard_status` / `lan_guard_set_exception` / `lan_guard_migrate` / `lan_guard_ensure_whitelist`（plan §5.2 契约：开关先派发后持久化、UAC 拒绝 code 5 → Err 且不写设置）；`lib.rs` 注册 + LanGuardMonitor 装配与 spawn。
  - `mesh_apply_config` / `mesh_install_service`：prepare 通过后派发参数追加 ensure-whitelist 段（`-WaitTun 20`）；失败不阻断服务段（plan §3.5）。白名单段构造函数 `ensure_whitelist_segment` 单点在 lan_guard.rs（T2 契约原样下沉为 script_invocation，行为逐字节不变），mesh.rs 只拼装；失效薄别名 service_install_params 移除（-BinPath 教训留档测试）。
  - 完成标志：单测全绿；git 提交。types.ts/api.ts 预落 LanHealth 序列化结构与四命令封装（UI 留 T6）；network.rs 补 NetMonitor.last() 只读访问器（活动网络注入 LanInputs，plan §3.5）。
- [ ] **T5 002 告警退役与网络卡收敛**（依赖: T3）（验收: AC4）
  - 测试先行：退役断言（network.rs 无 needs_alert/无规则 Profile 探测段）；networks-only 探测契约解析单测（替换原 detect_args 测试）。
  - `network.rs`：删 `FIREWALL_RULE_NAME`/`needs_alert`/NetStatus 的 rule_present/rule_private_only/alert；detect_args 收缩为纯 `Get-NetConnectionProfile`；轮询 15s → 60s（networks 供归类卡与 public_blocks_exception 消费）。
  - `MainView.tsx`：网络环境卡移除告警条（逐网络行 + 设为专用/公用保留）；`i18n/zh.ts`+`en.ts` 删 `net.alert` 等退役键。
  - 完成标志：全仓 grep 无 needs_alert/`CloudCLI LAN HTTPS 443` 于 network.rs 残留；git 提交。

## 阶段 2: 前端呈现与迁移

- [ ] **T6 前端白名单健康区与文案**（依赖: T4/T5）（验收: AC4/AC5/AC6/AC7/AC8）
  - 测试先行（前端侧纯函数）：地址区局域网行三态措辞映射（health → i18n 键）单测（vitest 惯例若无则组件内纯函数 + tsc/build 全绿兜底）。
  - `types.ts`/`api.ts`：`LanHealth` 与四命令封装。
  - `MeshCard.tsx`：「访问白名单」行——健康 chip（正常/休眠/待修复分类）、例外开关（风险确认模态：零认证直访风险 + 仅限信任网络 + 12h 回落预告 + 回落将再弹 UAC；剩余时长展示）、失配「修复白名单」按钮、动作失败 toast（AC7）、60s 轮询 + 动作后即时刷新。
  - `MainView.tsx` 地址区局域网行三态措辞（off 可直访已收口 / on 临时放行剩余 Xh / expired 回落未完成）（AC5/AC8）。
  - `WizardView.tsx`：收尾页白名单/旧规则检查项（消费 lan_guard_status，含「一键收口」复用）；基础阶段「局域网可达」改「本机服务就绪（局域网直访默认已收口）」如实措辞。
  - i18n `languard.*` 键族 zh/en 对称新增（缺键即构建挂）。
  - 完成标志：`npm run build` 全绿 + zh/en 键集对称断言；git 提交。
- [ ] **T7 存量迁移编排**（依赖: T4/T6）（验收: AC10）
  - 测试先行：旧规则检测解析（legacy443/legacy3001 → 横幅态）；migrate 幂等序列断言（删两旧名 → ensure-whitelist，重复派发无害）。
  - 启动自检接线：LanGuardMonitor 首轮 status 发现旧规则 → `languard://changed` 载荷 legacy_present → MeshCard 横幅「一键收口」→ `lan_guard_migrate`；收口后复测横幅消失。
  - 向导收尾页迁移入口与主看板同命令（T6 已铺 UI，本任务核对端到端）。
  - 完成标志：真机预演（本机当前两条旧规则即存量——在本机执行迁移并恢复 `CloudCLI Mesh HTTPS 443` 就位态，记录前后 `Get-NetFirewallRule` 清单差异）；全仓 grep 完成口径：旧规则名的 `New-NetFirewallRule` 创建逻辑零残留（仅 lan-guard.ps1 删除清单与 Rust 检测常量引用）；git 提交。

## 阶段 3: 收尾与验收

- [ ] **T8 文档与 ADR**（依赖: T2~T7 定稿）（验收: 宪法 §4 文档纪律）
  - 新建 `docs/adr/0005-firewall-whitelist-contract.md`（背景/决策/理由/后果：白名单契约取代 Profile 契约 + 002 告警随对象退役 + 3001 默认拒绝 + 例外 12h；模板沿 docs/adr/README.md）并登记索引。
  - `CHANGELOG.md` Unreleased：Added（白名单/例外开关/健康自检/迁移）、Changed（装机脚本契约、002 归类告警退役、地址区文案）、Removed（旧规则创建逻辑、443 归类告警条）。
  - `specs/002-network-profile/spec.md` 变更记录追加一行：US1 告警随 443 Private 语义退役（对象消亡，008 先例），US2 调整入口保留——状态不动。
  - `specs/MOC.md`：010 状态流转与描述更新。
  - 根 README「远程访问方案」段与 `tools/sprint0` README 脚本清单（补 lan-guard.ps1、删述已移除步骤）。
  - 完成标志：adr/README 索引含 0005；上述文档全部落盘；git 提交。
- [ ] **T9 自动化全绿 + 真机验收**（依赖: T1~T8）（验收: AC1~AC10 逐条）
  - 自动化：`cargo test` + `npm run build` 全绿；`scripts/build.ps1` 打包一轮确认 lan-guard.ps1 进产物（R9 防漂移复核）。
  - 手工：执行 plan §6.1 十项清单（需求方在场；清单 1 伪造源 IP 场景必须实测；清单 7 用 exceptionSince 回填法观察 12h 回落；清单 9 迁移前后规则清单比对留档）。
  - 对照 [spec.md](./spec.md) 逐条验证 AC1~AC10 并勾选；验收记录回填 spec（或 acceptance-manual，沿 007/009 惯例）。
  - 完成标志：AC 全勾 + 清单留档 + git 提交。

## 完成标志（DoD 检查）

- [ ] spec.md 中所有 AC（AC1~AC10）已逐条验证通过并勾选
- [ ] 自动化测试全部通过（cargo test + npm run build 全绿，无跳过失败测试）
- [ ] 相关文档已更新（ADR-0005、CHANGELOG、MOC、002 退役注记、README ×2、spec 开放问题回填）
- [ ] 双目录脚本一致 + build.ps1 $ScriptSubset/manifest 登记齐备（R9 复核）
- [ ] 本文件全部任务勾选完毕
