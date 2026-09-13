# 010-lan-boundary-hardening · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 已完成 <!-- 未开始 | 进行中 | 已完成 -->
- **最后更新**: 2026-09-13（T9 真机验收完成——需求方执行十项清单并拍板验收，AC1~AC11 全勾，spec 流转 done，发布 v0.6.0）

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
- [x] **T5 002 告警退役与网络卡收敛**（依赖: T3）（验收: AC4）——2026-09-12 完成：红绿同批（4 项退役契约测试对旧实现红 → 实现后全绿）；全仓 cargo test 227 通过（净 +3）+ tsc/vite build 绿；grep 证据：needs_alert 代码零命中、旧规则名仅存 lan_guard.rs 检测常量（契约保留）、net.alert 键零命中
  - 测试先行：`alert_chain_retired_from_source`（include_str 嵌入自身源码断言无告警判定/规则名常量/规则探测段，针串 concat 拼接防自证命中）+ `poll_interval_sixty_seconds_after_retirement` + networks-only 探测契约 `detect_args_networks_only_contract` 与载荷契约 `net_status_payload_networks_only`（替换原带规则段的 detect_args/告警解析测试）。
  - `network.rs`：删 `FIREWALL_RULE_NAME`/needs_alert 函数/NetStatus 的 rule_present/rule_private_only/alert 三字段；detect_args 收缩为纯 `Get-NetConnectionProfile`；轮询 15s → 60s（networks 供归类卡与 public_blocks_exception 消费）；逐网络行数据、set_category_params 提权路径（002 US2）、T4 的 last() 访问器全部保留。
  - 前端：`MainView.tsx` 移除告警条（逐网络行 + 设为专用/公用保留）；`types.ts` NetStatus 同步收缩；`i18n/zh.ts`+`en.ts` 删 `net.alert`（唯一消费方即告警条）；`net.riskPublic` 因仍被「设为公用」确认框消费而保留、文案改写脱离已退役规则语义（指向 3001 例外）；`net.dispatched` 15s → 1 分钟如实措辞。
  - 完成标志：全仓 grep 无 needs_alert/`CloudCLI LAN HTTPS 443` 于 network.rs 残留；git 提交。

## 阶段 2: 前端呈现与迁移

- [x] **T6 前端白名单健康区与文案**（依赖: T4/T5）（验收: AC4/AC5/AC6/AC7/AC8）——2026-09-12 完成：`tsc && vite build` 全绿（zh/en 键集对称经 tsc `Record<DictKey,string>` 断言 + 键名 diff 双重核对，languard.* 25 键对称）；cargo test 227 保持绿；`types.ts`/`api.ts` 的 LanHealth 与四命令封装为 T4 预落，本批零改动直接消费
  - 测试先行（前端侧纯函数）：地址区局域网行三态措辞映射落 `MainView.tsx` 导出纯函数 `lanAddrState(exception)`（on→addrOn+tone:open+剩余整小时 / expired→addrExpired / **off·pending·null→addrOff**——pending 为「已请求但规则未生效」，直访实际不通，按收口如实呈现、MeshCard 另行引导重开）；**项目无 vitest 前端测试基建（package.json 无测试 runner），按本条目预案以组件内纯函数 + `tsc && vite build` 全绿兜底**。
  - `MeshCard.tsx`：「访问白名单」行——健康 chip 三分类（ok=正常 / dormant=休眠+如实提示 / missing·staleCidr·staleIface=待修复+「修复白名单」→ lanGuardEnsureWhitelist 单 UAC）；`legacy_present` 横幅「检测到旧版局域网放行规则」+「一键收口」→ lanGuardMigrate（UI 本批落，端到端演练 T7）；例外开关：风险确认模态（零认证直访=潜在宿主机 shell / 仅限信任网络应急 / 12h 自动回落 / 回落将再弹 UAC，30s 未确认自动收起沿归类确认先例）→ lanGuardSetException(true)，开启中 chip 显示剩余时长，off 动作同开关（expired 态即手动回落），失败 toast（AC7）；`public_blocks_exception` 提示行；数据流 = languard://changed 事件（后端 60s 监视）+ 动作后即时复测（3.5s/12s 追加，沿归类切换 3.5s 先例）。
  - `MainView.tsx`：持有 lanHealth（启动 lanGuardStatus 兜底 + 事件订阅）下沉 MeshCard；地址区「局域网」行消费 `lanAddrState` 三态措辞（get_urls 语义不动）。
  - `WizardView.tsx`：收尾页新增「访问白名单」检查项（hb-dot + 详情：旧规则→「一键收口」、失配→「修复白名单」，与主看板同命令）；基础阶段 desc 改「本机服务就绪——局域网直访默认已收口」；收尾清单「局域网访问」标签随语义改「本机服务」。
  - i18n：`languard.*` 25 键 zh/en 对称新增；`net.riskPublic`/`net.dispatched`（T5 改写）与 `tunnel.check.netCategory`（443 放行 → 例外直访）措辞同步脱离已退役规则语义。
  - 完成标志：`npm run build` 全绿 + zh/en 键集对称断言；git 提交。
- [x] **T7 存量迁移编排**（依赖: T4/T6）（验收: AC10）——2026-09-12 完成：真机预演本机实证通过（UAC 于 21:17 获需求方批准，删旧建新幂等两轮全过，迁移前后比对见下）
  - 测试先行（3 项新单测，全仓 227 → 230 绿）：`migrate_banner_lifecycle_dedupes_after_closure`（判定层端到端序列：旧规则双在 → 横幅态 `legacyPresent=true` 发声 + 白名单 Missing 并存 → 收口复测横幅消失 + 白名单 Ok → 同态重复复测不重发声——重复派发/复测幂等无害）；`migrate_dispatch_is_deterministic_on_repeat`（重复派发参数逐字节一致）；`migrate_script_contract_sequence`（脚本契约锁：双目录逐字节一致 + 旧规则名 `New-NetFirewallRule` 行零出现 + migrate 分支「幂等删两旧名在前 → 复用 Invoke-EnsureWhitelist 在后」+ 443 白名单全脚本单点创建——AC10 grep 完成口径的自动化钉）。legacy_present 载荷 → 横幅态前端判定：T6 已铺（MeshCard `lanHealth?.legacyPresent` 直渲 + `wlChipClass/wlChipKey/wlHintKey/wlNeedsFix` 纯函数、MainView `lanAddrState` 三态、WizardView 收尾页 legacy 分支），单布尔直渲无缺失分支，项目无前端测试基建按 T6 预案以 `tsc && vite build` 兜底。
  - 启动自检接线核对（链路完整，零补建）：lib.rs 装配 LanGuardMonitor + spawn（首轮 tick=启动补回落自检，probe 输入每轮现取）→ PsLanGuardProbe 跑 status（含 legacy443/legacy3001）→ 变化才发声 `languard://changed`（TauriLanEmitter）→ MainView 持有 lanHealth 下沉 MeshCard 横幅「一键收口」（`runLan` 3.5s/12s 追加复测）→ `lan_guard_migrate` → 复测横幅消失；向导收尾页同命令入口（WizardView `runLanGuard("migrate", api.lanGuardMigrate)` + 进入探测/动作后复测）——以上经代码核对 + 判定层单测背书，UI 手工演练归 T9。
  - **真机预演（本机=真机，三轮留痕 `C:\Users\Public\lan-guard-migrate-run{1,2}.log`）**：①迁移前——`CloudCLI LAN HTTPS 443`/`CloudCLI LAN 3001` 均 Profile=Private、Remote=Any、Iface=Any；`CloudCLI Mesh HTTPS 443` 已在（T2 实跑所建）；status=`legacy443:true, legacy3001:true`（横幅态判定输入实况）。②migrate 第一轮（UAC 批准 21:17:18）——`[OK] 旧规则已删除：CloudCLI LAN HTTPS 443` + `[OK] 旧规则已删除：CloudCLI LAN 3001` + 白名单已在位幂等跳过；复测规则表仅剩 `CloudCLI Mesh HTTPS 443`（Profile=Any、Remote=10.126.126.0 掩码形态、Iface=et_8_1999），status 转 `legacy443:false, legacy3001:false`（= 判定层单测后态逐字段一致，横幅消失）。③migrate 第二轮幂等复验——`[i ] 旧规则不存在（幂等跳过）×2` + 白名单跳过，复测与第一轮零差异、无任何副作用（包装层 `$LASTEXITCODE` 捕获为空属包装脚本缺陷而非 lan-guard 行为，幂等无副作用以规则实况复测为证——plan §2「真相以 status 复测为准」的现场印证）。
  - 完成标志：全仓 grep 完成口径已达成并自动化钉死——`New-NetFirewallRule` 全仓仅 lan-guard.ps1 双目录两处（`$RuleMesh443`/`$RuleExc3001`，零旧名创建）；旧规则名仅存脚本删除清单/检测、lan_guard.rs 检测常量、历史调研文档（T8 加注）与 spec 自身文本；git 提交。

## 阶段 3: 收尾与验收

- [x] **T8 文档与 ADR**（依赖: T2~T7 定稿）（验收: 宪法 §4 文档纪律）——2026-09-12 完成：ADR-0005 落盘并登记索引；CHANGELOG Unreleased 三区（Added 3 条/Changed +2/Removed 2 条）；002 变更记录退役注记（US1 随对象消亡、US2 保留，状态 done 不动、AC 不改）；MOC 010 条目同步 T1~T8 实施进度；根 README「远程访问方案」如实化（组网主方案 + 局域网直访默认收口）；清扫三项（research 文档 §3.2 旧规则手工示例加「已退役(spec 010)」注记、install-https.ps1 头注释步骤 4 更正为白名单契约（双目录同步 + manifest 哈希重算 731bbf4b…，bundled 校验测试保持绿）、`wizard.stage.basis` 更名「基础（本机）/Basics (Local)」（zh/en 同步，tsc 键集断言保持绿））
  - 新建 `docs/adr/0005-firewall-whitelist-contract.md`（背景/决策/理由/后果：白名单契约取代 Profile 契约 + 002 告警随对象退役 + 3001 默认拒绝 + 例外 12h；模板沿 docs/adr/README.md）并登记索引。
  - `CHANGELOG.md` Unreleased：Added（白名单/例外开关/健康自检/迁移）、Changed（装机脚本契约、002 归类告警退役、地址区文案）、Removed（旧规则创建逻辑、443 归类告警条）。
  - `specs/002-network-profile/spec.md` 变更记录追加一行：US1 告警随 443 Private 语义退役（对象消亡，008 先例），US2 调整入口保留——状态不动。
  - `specs/MOC.md`：010 状态流转与描述更新。
  - 根 README「远程访问方案」段与 `tools/sprint0` README 脚本清单（补 lan-guard.ps1、删述已移除步骤）。
  - 完成标志：adr/README 索引含 0005；上述文档全部落盘；git 提交。
- [x] **T9 自动化全绿 + 真机验收**（依赖: T1~T8）（验收: AC1~AC10 逐条）——2026-09-12 自动化三项完成：①全绿 cargo test **230 passed / 0 failed / 3 ignored** + `tsc && vite build` 绿；②build.ps1 完整跑通（exit 0，release/ 双产物 2026-09-12 17:28 本次时间戳，setup.exe 12.37 MB / zip 15.42 MB）——lan-guard.ps1 进产物核验：便携 zip 内实有 `resources/bin/lan-guard.ps1`（15,423 字节）且包内 manifest.json 钉其 SHA256=8d86ec03… 与双目录源逐字一致，NSIS 侧按 tauri.conf.json `resources: ["resources/bin/*"]` + build.ps1 $ScriptSubset 同源核验；同步段 12 脚本全 `[一致]`、manifest 无变化，无 manifest 校验失败告警（唯一 warning 为 rustc 建库 linker 提示，良性）；③防火墙实况（只读）：两条旧规则 `CloudCLI LAN HTTPS 443`/`CloudCLI LAN 3001` 仍 Profile=Private Enabled=True（T7 migrate 提权窗未被批准，旧态属预期），`CloudCLI Mesh HTTPS 443` 在（Profile=Any）、`CloudCLI LAN 3001 Exception` 不存在（例外关闭），`lan-guard-migrate-run1.log`/`.code` 均未生成。**2026-09-13 收口**：需求方真机执行 plan §6.1 十项清单并拍板验收——唯一放行组合=同 WiFi+专用网络+例外放行。
  - 自动化：`cargo test` + `npm run build` 全绿；`scripts/build.ps1` 打包一轮确认 lan-guard.ps1 进产物（R9 防漂移复核）。
  - 手工（2026-09-13 完成）：真机实测项——清单 2（公用+无例外非成员 3001/443 不可达）、3/4（成员多网络可达：「各种网络情况下都可通过移动端访问」）、5（loopback 与域名 HTTP 200）、6（换网无告警，合并覆盖）、7（例外开/关全程：603 专用+例外开→手机直访 `http://<ip>:3001` 可达，首测误输 https 纠正；关闭→不可达）、9（迁移留档，随 T7 于 2026-09-12 落地）；背书项（自动化/契约，如实话注记）——清单 1（伪造源 IP，双条件规则实况背书）、8（网段联动，单测背书）、10（白名单健康：真机自然发生同型漂移 et_8_1999→et_8_2vfw 且白名单重建跟踪新名实证 + T2 exit 3 休眠实测）。逐项实况见 [acceptance-manual.md](./acceptance-manual.md) §2。
  - 对照 [spec.md](./spec.md) 逐条验证 AC1~AC11 并勾选（口径如实：真机实测 / 自动化背书分开）；验收记录回填 acceptance-manual；spec 流转 done。
  - 完成标志：AC 全勾（附口径注记）+ 清单留档 + git 提交。

- [x] **T10 程序级规则清理——AC11 旁路加固**（依赖: T2/T3）（验收: AC11）——2026-09-13 完成：验收实测暴露的程序级规则旁路（Windows 首次运行弹窗按 exe 创建「全端口 × 任意 profile」入站 Allow，绕过端口白名单契约）修复落地；cargo test **233 passed**（净 +3）+ `tsc && vite build` 绿 + lanDot 13 项绿；真机 UAC 实跑两轮 ensure-whitelist（清理 + 幂等复验），node/caddy/ddns-go 程序规则清零、easytier 收紧至 11010 双条、无关软件（wemailnode 等）原样，前后规则清单对比见下
  - 判定层（测试先行，3 项新单测）：`classify_program_rule_matrix`（清理判定纯函数矩阵——给定期望路径集合 → Delete/Tighten/Keep/Skip 分类：node 监听进程路径精确命中（大小写/分隔符/引号归一 + 符号链接解析形态）、非监听 node 版本与 wemailnode/Electron 一律 Keep（误伤红线）、ddns-go 任意目录叶名 Delete × 进程在跑 Skip、目标识别不到一律 Keep 的构造性证明）；`judge_health_bypass_risk_and_whitelist_untouched`（任一残留 → bypass_risk；nodeSkipped 非风险；旧版脚本缺 bypass 字段不报险；白名单五态与例外判定不因 bypass 改判）；`bypass_cleanup_script_contract`（脚本契约锁：双目录逐字节一致、两分支序列 ensure→清理→exit 3（清理与 TUN 解耦）、删除按稳定 Name 且识别仅取「入站 + Allow」、无按 DisplayName 全局删行、easytier 重建限定 11010+Program+全 Profile、status bypass 键在位、443 白名单创建仍单点）。
  - `lan-guard.ps1`（双目录同步 + manifest 哈希重算 1e9cf7f2…）：新增 `-StackDir` 参数（默认与 sprint0 各脚本一致）；`Invoke-BypassCleanup` 幂等清理（node 按 3001 监听进程实际路径精确识别——含 junction/symlink 真实形态双匹配（真机 nvm 场景实测：进程上报 `nodejs\node.exe` 而规则落库在 `v23.9.0\node.exe`，仅匹配单形态会空转）、caddy 按栈目录 + 443 监听路径、easytier 收紧至 11010 TCP/UDP 两条（Profile Domain,Private,Public）、ddns-go 任意路径残留删除 × 进程在跑跳过；逐条 [OK]/[SKIP] 明细）；ensure-whitelist / migrate 尾部追加清理且与 TUN 解耦（Invoke-EnsureWhitelist 改返回 bool，exit 3 移至调用方——休眠态也照常清理）；status 契约扩展 `"bypass":{"node,caddy,easytierWide,ddnsGo,nodeSkipped"}`（向后兼容）。
  - `lan_guard.rs`：`BypassTargets`/`BypassAction`/`classify_program_rule` 判定锚与脚本镜像；`BypassProbe` 解析（bypass 字段 Option 容错，沿 parse 先例）；`LanHealth.bypass_risk` 叠加位；派发参数 `-StackDir` 随 ensure-whitelist/migrate/status 透传（例外动作不透传，参数形态断言同步）；`LanInputs.stack_dir` 随设置联动（lib.rs 闭包 + PsLanGuardProbe + status_args）。
  - 前端：MeshCard 白名单行「旁路风险」警示 chip（bypassRisk 时橙警）+ 说明行 + 「修复白名单」按钮可见面扩展（wlNeedsFix ∨ bypassRisk——白名单 ok 而旁路残留时按钮仍可达）；i18n `languard.bypassChip/bypassHint` zh/en 对称新增；按钮文案确认无需改（ensure-whitelist 语义已含清理）。
  - 真机实测（本机=真机，UAC 实跑两轮：清理 + 幂等复验，日志留档 `C:\Users\Public\ac11-ensure-run.log`）：清理前只读探测证实四类旁路全真（status `bypass:{node:true,caddy:true,easytierWide:true,ddnsGo:true}`，与验收缺陷诊断一致；3001 监听进程=`D:\Software\nvm\nodejs\node.exe`（junction），规则实际落库 `nvm\v23.9.0\node.exe`——符号链接双形态匹配的实测依据）。**第一轮清理**（exit 0）：`[OK] 已删除旁路放行` node 2 条 + Caddy 4 条 + ddns-go 残留 4 条，EasyTier 全端口 Any 规则删除并重建 `CloudCLI Mesh EasyTier 11010 TCP/UDP` 两条，摘要「删除 10 条、收紧 3 处」；清理后程序规则实况——node 监听形态/Caddy/ddns-go 全部清零、easytier 栈内恰两条 11010 限定规则，status 转 `bypass` 五布尔全 false；本机 `127.0.0.1:3001` HTTP 200、`https://ai.jackqi.cn` HTTP 200（本机成员视角，非成员手机复测归验收清单）；无关规则原样（wemailnode 8 条、其他 node 版本 10 条、栈外 easytier 开发残留 2 条）。**第二轮幂等复验**（exit 0）：五目标全「已在位/无残留」跳过，摘要「无旁路残留（幂等）」。
  - 实跑首战踩坑（已修 + 回归钉）：识别函数 `return ,$out` 包装在空结果时退化为「单元素嵌套数组」，foreach 首元素的 `.Name` 为 null → `Remove-NetFirewallRule -Name` 参数校验失败（真机第一跑 [X] 实证；非空结果靠成员枚举侥幸通过，逐条 [OK] 明细亦失真）；改平铺 `return $out`（调用方 `@()` 收集即得平铺数组），脚本契约断言加钉（`return ,$out` 零出现 × `return $out` 恰二）。
  - 完成标志：三项新单测 + 全仓 233 绿；双目录哈希一致 + manifest 重算；真机前后规则清单比对留档；git 提交。

## 完成标志（DoD 检查）

- [x] spec.md 中所有 AC（AC1~AC11）已逐条验证通过并勾选（口径注记：AC1~AC6/AC10/AC11 真机实测、AC7~AC9 自动化覆盖为主并如实话标注未单独实测子场景）
- [x] 自动化测试全部通过（T9 收口时点 230 绿 → T10 后 233 passed / 0 failed；`tsc && vite build` 全绿，无跳过失败测试）
- [x] 相关文档已更新（ADR-0005、CHANGELOG——本批随 v0.6.0 定稿、MOC——本批流转 done、002 退役注记、README ×2、spec 开放问题回填）
- [x] 双目录脚本一致 + build.ps1 $ScriptSubset/manifest 登记齐备（R9 复核：T2 初登记 + T10 清理改动后哈希重算 1e9cf7f2…）
- [x] 本文件全部任务勾选完毕
