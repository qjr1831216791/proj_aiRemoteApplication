# 009-mesh-subnet-guard · 需求规格（spec）

> 导航：[plan.md](./plan.md) · [tasks.md](./tasks.md) · 返回 [MOC](../MOC.md)
> （plan / tasks 于本 spec 确认后按工作流补齐）

- **状态**: in-progress <!-- draft | reviewed | in-progress | done | archived -->
- **迭代**: Sprint 7
- **创建日期**: 2026-09-11
- **最后更新**: 2026-09-11

## 1. 背景与问题

v0.4.0 发版后全仓瘦身（merge `89b63cf`）沉淀的调优候选报告（[post-v0.4-tuning-backlog.md](../../docs/research/post-v0.4-tuning-backlog.md)）确认三项进本迭代，连同需求方在 spec 确认时补充的移动端配置痛点共四项：

1. **spec 007 §6 决议未接线**：决议要求「网段可编辑 + 保存/切换时与物理网卡网段重叠检测**阻断**」，实现仅交付纯函数 `detect_subnet_conflict`/`same_private_prefix` + 单测，从未接入保存路径——用户把虚拟网段设成与局域网同段时产生路由歧义、静默丢包，现场难排查。当前 2 条编译告警即此缺口的显式提示。
2. **发版防呆缺失**：0.3.0 曾漏 bump 三版本文件（v0.4.0 跳号修正），发版四步仅是文档约定，无机器校验。
3. **008 真机收尾未执行**：AC10/AC11 按需求方签收免实测，`uninstall-legacy.ps1` 随包保留，真机磁盘仍有 frpc/ddns-go 文件与 `.env` 的 `SAKURA_FRP_KEY` 行。
4. **移动端成员入网配置麻烦**：手机 EasyTier App 入网需逐项填网络名/密钥/对端节点，无对照清单易漏项错配。调研确认（[官方配置文件文档](https://easytier.cn/guide/network/config-file.html)）：EasyTier 官方定义的配置格式为 **TOML**（`[network_identity]` + `[[peer]]`），无 JSON 配置格式；Android 端以 App 界面逐项输入为主。

## 2. 目标与非目标

### 目标（Goals）

- 组网虚拟网段与物理网卡网段重叠时，保存/应用被**阻断**并给出可理解的双语错误（spec 007 §6 决议落地）
- 发版前三处版本号一致性 + CHANGELOG 版本节存在性可被一条命令自动校验
- 真机旧通道残留完成一次实际清理并回填 008 验收记录
- 工作台可展示一份符合 EasyTier 官方 TOML 格式的成员入网配置，移动端用户对照逐项输入

### 非目标（Non-Goals · 本期明确不做）

- 不做冲突网段的自动建议/重排（仅检测 + 阻断 + 文案指引用户自改）
- 不做运行期持续监控（仅保存/应用时点检测；EasyTier 本体与组网协议不动）
- release-check 只校验不写盘（不自动 bump、不自动改 CHANGELOG）
- 成员配置**不含真实密钥**（占位符 + 指引，维持 spec 007 AC8「密钥永不进 APP 界面」）；不做配置分享链接（et:// URI）/二维码形式（分享载体需携带密钥编码，与占位符策略冲突）
- 瘦身报告 P2（测试辅助器告警 ×4 清理）随实现期顺手处理，不设 AC（纯工程清理，免 spec）
- `config/`、`tests/` 空目录维持搁置（需求方 2026-09-11 决定）

## 3. 用户故事与验收标准

### US1: 作为工作台用户，我在保存组网设置时希望系统检测虚拟网段与物理网卡网段的重叠并阻断，以避免路由歧义导致的静默丢包。

- [ ] **AC1**: Given 本机存在活动物理网卡（IPv4 网段如 192.168.1.0/24），When 用户将组网虚拟网段改为与之重叠（同段/包含）的值并保存，Then 保存被拒绝，设置卡显示网段冲突的双语错误文案（含冲突网段信息），设置未写入磁盘（重载后仍为旧值）
- [ ] **AC2**: Given 虚拟网段（如默认 10.126.126.0/24）与所有活动网卡网段均不重叠，When 保存，Then 保存成功（与现状行为一致，无误报）
- [ ] **AC3**: Given 网卡信息不可得（枚举失败或无活动网卡），When 保存，Then 不阻断、保存成功（fail-open），且检测被跳过这一事实可经日志途径观察（不静默吞掉）
- [ ] **AC4**: Given 装机向导组网分支填写的网段与物理网卡重叠，When 点击应用，Then 同一检测入口拦截并显示同类错误（与设置卡同口径，不允许绕过）

### US2: 作为维护者，我在发版前运行校验命令，希望自动发现版本号不一致或 CHANGELOG 漏收口，以避免 0.3.0 式漏 bump 再次发生。

- [ ] **AC5**: Given `apps/workbench/package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 三处版本一致，且 `CHANGELOG.md` 存在以该版本号命名的版本节，When 运行 `scripts/release-check.ps1`，Then 所有检查通过、退出码 0，输出各检查项结果汇总
- [ ] **AC6**: Given 任一版本文件与其余不一致，When 运行，Then 退出码非 0，输出明确指出各文件的版本值与不一致点；全程只读，不修改任何文件
- [ ] **AC7**: Given CHANGELOG 中不存在对应版本节（仅有 Unreleased），When 运行，Then 退出码非 0，提示先按发布步骤将 Unreleased 收口为版本节

### US3: 作为宿主机用户，我在真机执行随包的旧通道卸载脚本，清掉 frpc/ddns-go 残留，使磁盘暴露面与 v0.4.0 的代码状态一致。

- [ ] **AC8**（真机手工验收）: Given 宿主机组网服务 EasyTierMesh 在线且栈 `.env` 已含腾讯云密钥，When 执行随包 `uninstall-legacy.ps1`，Then 台账全部 Done/Skipped（无 Failed、退出码 0）：`tasklist` 无 frpc/ddns-go 进程、ddns-go 自启任务已注销、栈目录退役文件（frpc.exe/ddns-go.exe/ddns-go.yaml/frpc-run.log）已删除、`.env` 的 `SAKURA_FRP_KEY` 行已移除且其余行保留；执行台账摘要回填 specs/008 的 acceptance-manual（AC10/AC11 备注区）

### US4: 作为成员设备（手机/PC）用户，我在工作台查看一份符合 EasyTier 官方 TOML 格式的成员入网配置，以便在移动端 App 里对照逐项输入，避免漏项错配。

- [ ] **AC9**: Given 组网设置已保存（网络名与对端节点非空），When 打开组网卡的「成员入网配置」入口，Then 展示 TOML 配置文本：`network_identity.network_name` 为已保存实际值、`network_secret` 为占位符（真实密钥不出现在 APP 任何界面/日志）、每个 `[[peer]]` 为已配置对端节点，关键字段附注释说明对应移动端 App 的输入项
- [ ] **AC10**: 配置文本支持一键复制，密钥占位旁有指引文案（填入运行 `set-mesh-secret.ps1` 时设定的密钥）
- [ ] **AC11**: 生成的配置文本符合 EasyTier 官方 TOML 格式（自动化校验：Rust 侧单测将生成文本反序列化，断言字段与已保存组网设置一致）

## 4. 功能需求与边界

- **检测范围**：比对组网设置的虚拟网段（`virtualCidr` + `virtualIp`）与本机**活动（up）物理网卡**的 IPv4 网段；环回/虚拟网卡（TUN 自身、loopback）不在比对范围（口径在 plan 中定，避免 EasyTier 自身 TUN 误报）
- **检测时点**：组网设置保存（含装机向导应用）——单一入口统一拦截；**加载既有设置不拦截**（决议只约束保存/切换时点，不做存量追溯）
- **fail-open 策略**：网卡枚举失败/无网卡环境放行保存（避免把无网卡环境锁死在设置外），跳过原因写入既有日志途径
- **错误呈现**：中文可读错误文案（含冲突网段与 IP 列表），经既有保存错误通道（toast）展示——与 `validate_mesh_config` 既有错误形态保持一致；保存错误文案整体 i18n 化不在本期（避免单独双语化制造不一致，plan §2 取舍）
- **release-check 形态**：`scripts/release-check.ps1`（PowerShell 5.1 兼容、BOM+CRLF、遵循 sprint0 脚本约定），只读校验；默认从 package.json 读目标版本，支持显式传参
- **成员入网配置**：以 EasyTier 官方最小可用配置为口径（`[network_identity]` + `[[peer]]`，不堆高级 flags；成员虚拟 IP 默认由组网 DHCP 自动分配）；TOML 由 Rust 侧从已保存组网设置拼装（复用既有 toml 依赖），前端仅展示与复制，不做本地拼装
- **真机清理**：复用随包 `uninstall-legacy.ps1`（幂等、内置组网在线 + 腾讯凭证双闸门），本 spec 不改该脚本

## 5. 约束与假设

- 冲突判定算法沿用已实现的 `detect_subnet_conflict`（spec 007 交付的纯函数），本期工作是**接线**而非重写
- 网卡数据源复用现有网络监测能力（NetMonitor），不引入新依赖
- AC8 需求方在场配合真机操作（远程不可代办）
- release-check 集成进 build.ps1 为可选项，由 plan 决策

## 6. 开放问题

- [x] 错误呈现形态与稳定码归属 → **已决**（plan §2）：中文直接文案 + 既有 toast 通道，不新增稳定码
- [x] release-check 是否作为 build.ps1 前置步骤自动执行 → **已决**（plan §2）：独立脚本（CLAUDE.md 常用命令登记入口），不并入 build.ps1
- [x] 网卡枚举的"活动"判据 → **已决**（plan §2）：`list_afinet_netifas` 枚举的非回环 IPv4 即算（既有 `local_ipv4_addrs` 口径，无 OperationalStatus 过滤）

## 7. 变更记录

| 日期 | 变更内容 | 原因 |
|------|----------|------|
| 2026-09-11 | 初稿（US1 网段阻断 + US2 发版校验 + US3 真机清理） | 调优报告 P1/P3/P4 经需求方确认纳入 Sprint 7（v0.5.0） |
| 2026-09-11 | 需求方确认原 8 条 AC；新增 US4 成员入网配置展示（AC9~AC11，TOML 官方格式、密钥占位符方案）；状态 draft → reviewed | 需求方补充移动端配置痛点；密钥呈现权衡经需求方选定为占位符（维持 007 AC8） |
| 2026-09-11 | §4「错误呈现」修订为与既有保存错误一致的中文直接文案（原措辞"稳定码 + 双语"），§6 三条开放问题关闭；状态 reviewed → in-progress | plan 起草时现实校准：现有保存错误均为中文直接文案，单独双语化制造不一致（不允许默默偏离，显式登记） |
