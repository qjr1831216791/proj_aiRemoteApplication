# 013-domain-single-source · 需求规格（spec）

> 导航：[plan.md](./plan.md) · [tasks.md](./tasks.md) · 返回 [MOC](../MOC.md)

- **状态**: done <!-- draft | reviewed | in-progress | done | archived -->
- **迭代**: Sprint 10
- **创建日期**: 2026-09-15
- **最后更新**: 2026-09-25

## 1. 背景与问题

v0.7.0 安装包首次分发给其他用户使用，随即暴露**域名硬编码缺陷**：`ai.jackqi.cn` 以编译期常量写死于 `src-tauri/src/consts.rs`（`DOMAIN` / `DOMAIN_ROOT` / `WORKBENCH_URL`），其规范出处是 spec 001 plan 决策 6「域名为编译期常量，不进设置文件」——该决策成立的前提是 001 spec 假设节声明的「当前部署实例默认值而非产品常量」，仅适用于研发者自用阶段。多用户分发后，每个装机用户都有自己的域名（装机向导「腾讯云前置」阶段录入），硬编码导致：

1. **同步 DNS 报错**（真机实证）：看板/向导点击「同步 DNS： A 记录 → 虚拟 IP」时，代码拿**用户自己的** DNSPod 密钥去操作**研发者的** `jackqi.cn`，腾讯云返回 `OperationDenied.NoPermissionToOperateDomain`；
2. **访问地址区显示错误域名**：看板「访问地址 · 域名」行、设置页只读卡均显示 `https://ai.jackqi.cn/`；
3. **检测与文案同错**：DNS 对齐检测（30s 轮询）、域名心跳（60s 周期）、托盘「打开工作台」、mesh 诊断第⑥项、i18n 中英文 6 处文案，全部指向研发者域名。

关键事实：**用户在向导输入的域名已持久化**（`wizard-state.json` 的 `domain` 字段，`wizard_set_domain` 落盘，且经 DNSPod `DescribeRecordList` 校验真实存在），只是没有任何看板侧代码消费它——缺的不是存储，是「单一数据来源」的接线。

## 2. 目标与非目标

### 目标（Goals）

- 建立域名单一数据来源：装机向导「腾讯云前置」录入的域名成为**全链路唯一事实来源**，未配置时回落编译期默认值（兼容研发机与既有部署）。
- 全部消费点接线：同步 DNS、DNS 对齐检测、访问地址区、设置页只读卡、域名心跳、托盘、mesh 诊断、i18n 文案。
- 随包脚本中「以研发者域名为实际执行依据」的两处（install-server 收尾输出、uninstall-legacy CNAME 残留探测）参数化。

### 非目标（Non-Goals · 本期明确不做）

- **不做多域名管理**：单域名实例，不提供域名切换/多域名并存；改域名仍走向导录入入口。
- **不改向导交互流程**：域名仍在腾讯云前置阶段录入与校验（现有 `wizard_set_domain` + detect 校验链路不动）。
- **不动根域/子域拆分算法**：沿用既有 `split_domain` / `subdomain_of` 实现与限制。
- **不改 `install-https.ps1` 的 `$Domain` 默认值**：向导装机路径必经 `-Domain` 透传覆盖（`WizardView.tsx` → `scripts.rs`），默认值仅是直跑脚本的研发实例占位。
- **不动 `tools/sprint0/bin/` 脚本副本**：研发机自用工具，其默认域名对研发者恰好正确。
- **密钥存储不变**：DNSPod 密钥继续走栈目录 `.env`（宪法 §3），不因本 spec 挪动。
- **不追溯历史文档**：已发布 spec / CHANGELOG / 调研报告中的 `ai.jackqi.cn` 记载属历史事实，保留。

## 3. 用户故事与验收标准

### US1: 作为装机用户（分发接收方），我希望看板的域名展示与 DNS 操作使用我在装机向导录入的自己的域名，以便产品行为与我的云资源一致

- [ ] **AC1**: Given 向导腾讯云前置已录入 `ai.example.com` When 查看看板「访问地址 · 域名」行与设置页只读卡 Then 均显示 `https://ai.example.com/`，两处一致
- [ ] **AC2**: Given 向导腾讯云前置已录入 `ai.example.com` 且 DNSPod 密钥与根域 `example.com` 已就绪 When 点击「同步 DNS： A 记录 → 虚拟 IP」（看板或向导 Channel 阶段③） Then DNSPod 侧对 `example.com` 的 `ai` 子域执行 A 记录调和并返回成功，不再出现 `NoPermissionToOperateDomain`
- [ ] **AC3**: Given 未跑向导（`settings.domain` 为空） When 查看任意域名消费点 Then 回落编译期默认域名（研发机既有部署行为不变；**DNS 操作类消费点除外，见 AC9**）
- [ ] **AC9**: Given `settings.domain` 为空 When 点击「同步 DNS」 Then 不发起任何 DNSPod 调用，报错明示「域名传入为空」并指引到向导「腾讯云前置」录入（**不得回落研发者域名试错**——真机 2026-09-15：分发用户以其自有密钥操作 `ai.jackqi.cn` 报 `NoPermissionToOperateDomain`）
- [ ] **AC10**: Given 同步 DNS 的 DNSPod 调用返回错误（如对无权限域名操作） Then 报错文案以「域名：<所操作域名>，」开头，错值一眼可辨

### US2: 作为装机用户，我希望域名相关的检测、入口与文案也跟随我的域名，以便诊断结果可信、快捷入口可用

- [ ] **AC4**: Given 生效域名为用户域名 When DNS 对齐检测（30s 轮询）与通道体检即时探测执行 Then 探测目标为用户域名
- [ ] **AC5**: Given APP 运行中 When 向导改录域名 Then 后续心跳周期（60s）探测新域名，**无需重启 APP**
- [ ] **AC6**: Given 生效域名为用户域名 When 使用托盘「打开工作台」或运行 mesh 诊断 Then 前者打开用户域名，后者第⑥项「域名链路」检查用户域名
- [ ] **AC7**: Given 界面语言为中文或英文 When 查看同步 DNS 引导、向导步骤③提示等相关文案 Then 文案中的域名为当前生效域名（不再写死 `ai.jackqi.cn` 字面量）
- [ ] **AC8**: Given 随包脚本场景 When 查看装机收尾输出与卸载编排的 CNAME 残留探测 Then 二者作用于用户域名（参数化），不再以研发者域名为实际执行依据

## 4. 功能需求与边界

- **存储与写入**：`settings.json` 新增 `domain` 字段（空 = 未配置，消费点回落默认值）；`wizard_set_domain` 保存时同步写入 settings（双写），写入前经既有 `validate_domain` 校验（spec 011 注入防线）。
- **生效域名解析**：Rust 侧统一入口函数（如 `effective_domain`）：读 settings.domain → 空则回落 `consts::DOMAIN`；根域与完整 URL 由其派生，消费点不得再直读 `consts::DOMAIN/DOMAIN_ROOT/WORKBENCH_URL`。
- **前端取数路径**：设置页只读卡的域名当前是 TS 侧独立硬编码副本（`SettingsView.tsx` 的 `READONLY.domain`），改为经后端载荷（`get_urls` 或 settings 命令）下发生效域名，前端字面量退役。
- **心跳实时性**：心跳探测目标在每次探测时从配置读取（替代启动时固化），支撑 AC5。
- **异常路径**：`settings.domain` 含非法值时按未配置处理（回落默认），不得 panic 或阻断命令；**例外：同步 DNS 链路对"未配置/空"拒绝执行并明确报错（AC9），不回落默认域名试错（2026-09-15 变更）**。

## 5. 约束与假设

- **本 spec 推翻 spec 001 的域名常量化口径**（`specs/001-desktop-console/plan.md` §4 数据模型注记「端口/路径/域名为编译期常量，不进设置文件」，援引其 spec 决策 6「端口/路径设置」）中「域名」部分：001 spec 假设节本就声明 `ai.jackqi.cn` 为「实例默认值而非产品常量」，多用户分发使该假设失效，本变更是其自然延伸；`consts::DOMAIN` 降级为回落默认值保留。端口/路径仍为编译期常量（决策 6 其余部分不变）。
- 需求方已确认需求方向与录入入口（装机向导腾讯云前置），见变更记录。
- 自动化测试覆盖逻辑类 AC（域名解析、回落、派生、心跳实时读、脚本参数存在性）；GUI 视觉类（地址区/只读卡实际显示）以 tasks.md 手工验收清单覆盖（宪法 §1 修订条款）。

## 6. 开放问题

- （无）

## 7. 变更记录

| 日期 | 变更内容 | 原因 |
|------|----------|------|
| 2026-09-15 | 初稿（draft） | 分发用户真机反馈两项缺陷（同步 DNS 报 `NoPermissionToOperateDomain` + 看板域名写死研发者域名）；需求方拍板「域名应有单一数据来源（装机向导腾讯云前置录入）」并选择立 spec 走变更流程 |
| 2026-09-15 | 子代理评审 PASS-with-notes（4 条低severity 全部采纳修复：§4 补前端取数路径、AC2 Given 补自足前置、决策 6 归属表述精确化、MOC 迭代表补 Sprint 10）；状态 draft → reviewed | 需求方指示「派发子代理审核，如果没问题则下一步」 |
| 2026-09-15 | 真机验收期修订：**DNS 操作链不再回落默认域名**——新增 AC9（未配置域名点同步 → 拒绝并报「域名传入为空」）、AC10（DNSPod 报错带「域名：X」前缀）；§5 异常路径对同步 DNS 链路 carve-out。展示/心跳类消费点回落口径不变（AC3） | 需求方同事真机复现：重装后未录域名点同步 DNS，后端把回落默认 `ai.jackqi.cn` 传给 DNSPod（配用户自有密钥）报 `NoPermissionToOperateDomain`，报错又不含域名值无从排查；需求方定「不能默认传值，执行侧校验并明示域名」 |
| 2026-09-25 | 需求方真机签收：分发场景装机/域名链路全部走查通过（含验收期两项修复），状态 in-progress → **done** | AC1~AC10 全部验证通过 |
