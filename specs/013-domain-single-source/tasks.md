# 013-domain-single-source · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 进行中 <!-- 未开始 | 进行中 | 已完成 -->
- **最后更新**: 2026-09-15

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成

## 阶段 1: Rust 基础（单一来源与解析）

- [x] T1 `settings.rs` 新增 `domain` 字段（`#[serde(default)]` + `SettingsPatch` + 归一与 `validate_domain` 校验），旧格式文件兼容；单测覆盖：缺字段反序列化、空值、非法值拒绝写入（验收: AC3）✅ 2026-09-15（+`normalize_domain` 纯函数；4 新测试 + 2 既有测试扩充，cargo test 258 绿）
- [x] T2 生效域名解析纯函数集（`effective_domain` / 根域派生 / `workbench_url_of`），回落 `consts::DOMAIN`；单测覆盖：配置值、空回落、非法值容错回落、根域/URL 派生（依赖: T1）（验收: AC3，支撑全部 AC）✅ 2026-09-15（落 `settings.rs`，回落态与 `WORKBENCH_URL` 形态一致性有断言）
- [x] T3 `wizard_set_domain` 双写 settings（同命令内原子完成，写入前过 T1 校验链）；单测断言 wizard-state 与 settings 同值（依赖: T1）（验收: AC2 前置）✅ 2026-09-15（抽 `apply_domain_to` 核心：settings 侧归一+011 闸，Err 即两处均不写；命令加 `SettingsState` 注入，前端 invoke 不受影响）

## 阶段 2: 消费点接线（Rust）

- [x] T4 `urls.rs`/`get_urls` 改经生效域名下发；`mesh_sync_dns`、`check_dns_alignment`、`check_domain_health_now`、`mesh_diagnostics` 四命令取参改生效域名；`tray.rs` 托盘「打开工作台」事件时解析生效域名（核实无菜单构建期固化文案）；更新既有测试断言（依赖: T2）（验收: AC1、AC2、AC4、AC6）✅ 2026-09-15（另接线 `open_external`（地址区「打开」按钮同源）；`build_urls` 加 domain_url 参数；consts.rs 口径注释更新；托盘无构建期固化，菜单仅含固定 id——261 测试绿零 warning）
- [x] T5 `heartbeat.rs` 的 url 改 provider 闭包（`Arc<dyn Fn() -> String>`，对称 `enabled` 先例），`lib.rs` 装配处接 settings；单测：闭包返回变化后下一探测目标随之变化（依赖: T2）（验收: AC5）✅ 2026-09-15（真实驱动 spawn 循环的测试：provider 记录序列断言新值被消费；测试探测 127.0.0.1 未监听端口零外网流量；262 绿）

## 阶段 3: 前端与脚本

- [x] T6 前端接线：`SettingsView.tsx` 只读卡域名改经 `get_urls` 下发解析（`READONLY.domain` 字面量退役）；i18n zh/en 三对键参数化 `{domain}` 并更新调用点传参；`npm run build` 通过（依赖: T4）（验收: AC1、AC7）✅ 2026-09-15（域名经 App→SettingsView / MainView→MeshCard→DnsNotice props 链下发；向导文案取 `state.domain`（含回落默认）；src/ 全仓 grep `jackqi` 零残留；tsc+vite 构建过）
- [x] T7 脚本参数化：`install-server.ps1` 加 `-Domain`（缺省不输出域名行）、`uninstall-legacy.ps1` 加 `-Domain`（缺省跳过 CNAME 检测段）；grep 全部派发点接线透传；PS 解析检查断言参数与缺省分支（依赖: T4）（验收: AC8）✅ 2026-09-15（`tool_plan` 透传扩至 InstallServer（值经 011 sink 单引号包裹，测试断言）；前端 ToolsSection 接 settings 透传；Parser::ParseFile 双脚本 OK；262 绿 + 构建过）

## 阶段 4: 回归与收尾

- [~] T8 回归：`cargo test` 全绿 + `npm run build` 通过；`CHANGELOG.md` Unreleased 区登记；对照 spec.md 逐条验证 AC 并勾选（自动化项以测试为证，GUI/真机项执行手工清单：地址区显示、同步 DNS 真机（需求方自有域名）、托盘、文案走查）；spec/tasks/MOC 状态流转（依赖: T1~T7）（验收: 全部 AC）
  - ✅ 2026-09-15 自动化侧完成：cargo test 262 绿（新增 8 项：settings domain 4 + 生效解析 1 + wizard 双写 1 + 心跳 provider 1 + tool_plan 透传扩 1）+ tsc/vite 构建过 + PS Parser 双脚本 OK + `src/` 与 `resources/bin` 域名字面量清零（consts 回落默认值除外）+ CHANGELOG 已登记
  - ⏳ 待需求方真机验收（下方手工清单）→ 全过后勾选 spec.md AC → 状态流转 done

## 手工验收清单（GUI/真机类 AC 的测试载体）

1. 向导录入自有域名后，看板「访问地址 · 域名」行与设置页只读卡均显示该域名（AC1）
2. 点击「同步 DNS」（看板 + 向导③两入口），DNSPod 对自有根域/子域生效，无 `NoPermissionToOperateDomain`（AC2 真机部分）
3. 运行中向导改域名，下一心跳周期红绿标记按新域名刷新，无需重启（AC5 真机部分）
4. 托盘「打开工作台」打开新域名；mesh 诊断第⑥项报文指向新域名（AC6）
5. 中英文界面各走查同步 DNS 引导、向导步骤③文案，域名为生效值非 `ai.jackqi.cn` 字面量（AC7）
6. 装机编排收尾输出与卸载编排 CNAME 探测作用于自有域名（AC8 真机部分）

## 完成标志（DoD 检查）

- [ ] spec.md 中所有 AC 已逐条验证通过
- [ ] 自动化测试全部通过
- [ ] 相关文档已更新（CHANGELOG / MOC / consts.rs 注释口径）
- [ ] 本文件全部任务勾选完毕
