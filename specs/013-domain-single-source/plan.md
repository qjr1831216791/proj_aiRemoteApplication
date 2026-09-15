# 013-domain-single-source · 技术方案（plan）

> 导航：[spec.md](./spec.md) · [tasks.md](./tasks.md) · 返回 [MOC](../MOC.md)

- **状态**: reviewed
- **关联需求**: [spec.md](./spec.md)
- **最后更新**: 2026-09-15

## 1. 方案概述

复用 spec 001 已验证的「编译期常量 → 默认值 + 配置覆盖」模式（`stack_dir` 先例）：`settings.json` 新增可选 `domain` 字段作为运行时单一来源，`consts::DOMAIN/DOMAIN_ROOT/WORKBENCH_URL` 降级为回落默认值。Rust 侧新增统一的「生效域名解析」纯函数集（域名 → 根域/URL 派生），全部消费点改经它取值；`wizard_set_domain` 保存向导域名时双写 settings（唯一写入口）；心跳从「启动固化 URL」改为「探测时经闭包实时取」（对称复用既有 `enabled` 闭包先例）。前端只读卡与 i18n 文案改为后端下发/参数化；两处脚本加 `-Domain` 参数并在派发链透传。

## 2. 技术选型

| 领域 | 选型 | 理由 | 放弃的备选及原因 |
|------|------|------|------------------|
| 域名存储 | `settings.json` 新增可选字段 | 与 `stack_dir` 同款成熟模式（Patch 写入/原子落盘/SettingsState 全都有）；语义上属运行配置 | 直接读 `wizard-state.json`：向导状态文件语义是「向导进度」而非「运行配置」，且 WizardHolder 生命周期与向导耦合 |
| 生效解析 | `settings.rs` 内纯函数集 `effective_domain/effective_root/workbench_url_of` | 无 IO、易单测、消费点集中 import；沿用既有 `split_domain`/`subdomain_of` 拆分算法 | 新建 domain.rs 模块：体量不足以立模块 |
| 心跳实时化 | `HealthMonitor` 的 url 字段改 `Arc<dyn Fn() -> String>` provider | 对称复用既有 `enabled: Arc<dyn Fn() -> bool>` 先例（heartbeat.rs:190-195），改动面最小 | 重启生效：违背 AC5，用户体验差 |
| 前端取数 | `get_urls` 下发 URL（值变为生效域名），只读卡经 `new URL(...).hostname` 解析显示 | 不改任何 Tauri 命令签名，契约零变更 | `get_settings` 载荷扩字段再区分「配置值/生效值」：引入两个字段语义分歧，无必要 |

> settings.json 加可选字段属向后兼容演进（旧文件缺字段按空处理），不立 ADR，理由记录于此。对外 Tauri 命令签名全部不变。

## 3. 架构设计

```mermaid
flowchart LR
    W[装机向导 腾讯云前置<br/>wizard_set_domain] -->|唯一写入口·双写| S[(settings.json<br/>domain 字段)]
    S --> E[effective_domain 解析<br/>settings.rs 纯函数]
    E --> C1[mesh_sync_dns 同步DNS]
    E --> C2[check_dns_alignment 对齐检测]
    E --> C3[get_urls 地址区]
    E --> C4[HealthMonitor 心跳·探测时取]
    E --> C5[托盘打开工作台]
    E --> C6[mesh_diagnostics 诊断⑥]
    C3 --> F1[MainView 地址区]
    C3 --> F2[SettingsView 只读卡<br/>READONLY.domain 退役]
    E --> I[i18n 6 处文案 · {domain} 参数化]
```

数据流：向导录入（经 DNSPod 存在性校验）→ settings 落盘 → 各消费点启动/每周期/每次调用时解析生效域名 → 未配置或非法值一律回落 `consts::DOMAIN`。

## 4. 数据模型

`settings.json`（`%APPDATA%\ai-remote-workbench\settings.json`）新增字段：

```json
{
  "domain": "",              // 新增：生效域名，空 = 未配置（消费点回落默认）；camelCase 与现有一致
  "stackDir": "...",         // 既有，不动
  "mesh": { ... }            // 既有，不动
}
```

- `Settings.domain: String`（`#[serde(default)]`，旧文件无此字段可反序列化）；`SettingsPatch.domain: Option<String>`。
- 写入归一：trim → 去尾部 `.` → 小写（与 `wizard_set_domain` 现行归一一致）→ `validate_domain`（spec 011 注入防线，`scripts.rs:424`）通过才落盘，非法值拒绝写入并报错（读取侧另行容错：含非法值时按未配置回落，不 panic）。
- 根域/子域派生沿用既有算法（wizard `split_domain` / dns_api `subdomain_of`），多级后缀（如 `.com.cn`）的既有限制不变（spec 非目标），向导 DescribeRecordList 校验兜底报错。

## 5. 接口契约

全部 Tauri 命令签名不变，仅值语义变化：

| 命令 | 变化 |
|------|------|
| `get_urls` | 返回结构不变；`domain` 字段值 = 生效域名的 `https://<域名>/` |
| `get_settings` | 返回结构新增 `domain` 字段（其余不变） |
| `wizard_set_domain` | 签名/返回不变；行为增加双写 settings |
| `mesh_sync_dns` / `check_dns_alignment` / `check_domain_health_now` / `mesh_diagnostics` | 签名/返回不变；操作目标变为生效域名 |

随包脚本新增可选参数：`install-server.ps1 -Domain <域名>`（收尾输出用，缺省不输出域名行）、`uninstall-legacy.ps1 -Domain <域名>`（CNAME 残留探测目标，缺省跳过该检测段）。

## 6. 测试策略

| AC | 覆盖方式 |
|----|----------|
| AC1 地址区+只读卡 | 自动化：urls.rs 单测（生效域名下发）；GUI 实际显示走手工清单 |
| AC2 同步 DNS | 自动化：root/sub 派生自生效域名的纯函数单测 + mesh_sync_dns 取参路径单测；DNSPod 真实调用属外部依赖，真机手工验收（需求方自有域名实测） |
| AC3 回落默认 | 自动化：domain 为空/字段缺失/含非法值三态单测 |
| AC4 对齐检测/体检目标 | 自动化：run_dns_probe 取参单测；轮询行为真机复核 |
| AC5 心跳免重启 | 自动化：provider 闭包变化后下一探测目标随之变化的单测 |
| AC6 托盘/诊断 | 编译兜底 + 手工清单（系统集成行为，自动化成本不成比例） |
| AC7 i18n 文案 | npm build 通过 + 手工走查（前端无既有单测框架，不为此新增） |
| AC8 脚本参数化 | 自动化：PS 解析检查（`Parser::ParseFile` 先例）断言 `-Domain` 参数与缺省跳过分支存在；派发链透传以代码走查 + 手工 |

回归红线：`cargo test` 全绿 + `npm run build` 通过；禁止 skip。

## 7. 风险与对策

| 风险 | 影响 | 对策 |
|------|------|------|
| 向导/settings 双写不一致 | 两处域名漂移 | 双写在 `wizard_set_domain` 同一命令内完成 + 单测断言两处同值；向导为唯一写入口，无并发写 |
| 旧 settings.json 缺 domain 字段 | 反序列化失败 | `#[serde(default)]` + 单测覆盖旧格式文件 |
| 心跳 provider 闭包锁竞争 | 周期探测阻塞 | 对称复用 `enabled` 闭包既有模式（同锁粒度），不引入新锁 |
| 托盘菜单若在构建期固化域名文案 | 菜单显示旧域名 | 实现时核实 tray 菜单构建逻辑：仅在菜单事件处理时取生效域名即可；若存在构建期文案则一并改为事件时解析 |
| 脚本派发链透传点遗漏 | 向导装机仍传旧域名 | 实现时 grep `install-server`/`uninstall-legacy` 全部派发点（scripts.rs ToolKind/Script 映射 + 向导编排）逐一接线，编译器+测试兜底 |
| 多级后缀域名拆分错误 | 同步 DNS 作用于错误根域 | 既有算法限制（spec 非目标），向导 DescribeRecordList 校验对用户即时报错，不静默错写 |

## 8. 影响范围

- **Rust**：`settings.rs`（字段+patch+解析函数）、`wizard.rs`（双写）、`commands.rs`（4 命令取参）、`urls.rs`（签名内加参）、`lib.rs`（心跳装配）、`tray.rs`、`heartbeat.rs`（provider 化）及各自测试。
- **前端**：`SettingsView.tsx`（READONLY.domain 退役）、`i18n/zh.ts`+`en.ts`（3 键×2 参数化）及调用方传参（MeshCard/WizardView/MainView 中相关 t() 调用点）。
- **脚本**：`src-tauri/resources/bin/install-server.ps1`、`uninstall-legacy.ps1` 及其 Rust 派发映射；`tools/sprint0/bin/` 副本不动（spec 非目标）。
- **文档**：`consts.rs` 头注释口径更新；`CHANGELOG.md` Unreleased 登记；`specs/MOC.md` 状态同步。

## 变更记录

| 日期 | 变更内容 | 原因 |
|------|----------|------|
| 2026-09-15 | 初稿 | 依 spec.md 与全仓域名消费点盘点（2026-09-15 双代理调查+评审取证）定方案 |
