# 006-foolproof-install · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 进行中
- **最后更新**: 2026-09-10

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成

## 阶段 1: 规格与决策载体

- [x] T1 spec.md 三轮打磨（阶段重构 / §4.1 UI 形态 / US8 入口唯一化）并置 reviewed，MOC 同步（2026-09-10）（验收: 全部 AC 的需求基线）
- [x] T2 plan.md（收口 3 个开放问题）+ [ADR-0003](../../docs/adr/0003-caddy-dns-plugin-tls.md)（验收: plan reviewed）

## 阶段 2: 脚本层（专项可独立运行）

- [ ] T3 `install-https.ps1` 改造：Caddy 下载源改按需构建 API（锁 `tencentcloud@v0.4.3`，`-CaddyZip` 兜底与手动下载指引）+ Caddyfile 模板改插件式（`{env.*}` 引用）；既有 Caddyfile 幂等不覆盖；`build.ps1`/`manifest.json` 同步（依赖: T2）（验收: AC6）
- [ ] T4 `set-tencent-key.ps1`：不回显输入 SecretId/Key → 两次确认 → 原位写栈 `.env`（其余行保留）→ 复核显示 ID 末 4 位；入库 + `tools/sprint0/bin` 副本（依赖: T2）（验收: AC4）
- [ ] T5 `config-ddnsgo.ps1`：从 `.env` 读凭证 → 生成 `ddns-go.yaml`（tencentcloud / url 模式取 IP / 域名）→ 启动 ddns-go（依赖: T4）（验收: AC8）
- [ ] T6 caddy spawn 环境注入：Rust 侧启动 Caddy 前从 `.env` 读 `TENCENT_SECRET_ID/KEY` 注入进程环境（依赖: T2）（验收: AC7）

## 阶段 3: Rust 编排层

- [ ] T7 `wizard.rs` 状态机与持久化：WizardState/StageState、wizard-state.json 加载/损坏恢复/原子写（settings.rs 同款语义）+ 探针纯函数（文件/端口/凭证存在 → 阶段态）+ 单测（依赖: T2）（验收: AC1/2/12）
- [ ] T8 腾讯云校验编排：复用 `dns_api.rs` 实现"密钥有效+域名存在"探测与记录对齐探测、CGNAT 纯函数（公网 IP × 网卡集合交集）、错误码→可读文案映射 + 单测（依赖: T7）（验收: AC4/5/8）
- [ ] T9 阶段执行器与 commands：`wizard_*` 七命令 + `wizard://changed` 事件 + `lib.rs` 注册；穿透分支复用 `switch_channel`；命令构造断言单测（依赖: T7/T8）（验收: AC3/9/10/11）

## 阶段 4: 前端

- [ ] T10 `WizardView`：第三视图 + 竖向阶段清单（四态 chip）+ 阶段面板（外部办理模式/凭证脚本拉起/日志折叠/重试）+ 分支单选卡 + 收尾清单；`api.ts`/`types.ts` 扩展；zh/en 词典补全（依赖: T9）（验收: §4.1 全部交互规则）
- [ ] T11 低频栏瘦身（AC13/14）：ToolsSection 移除装机三件套与「打开工作台网页」、更名运维工具；MainView 组件卡「去安装」改跳向导（依赖: T10）

## 阶段 5: 测试与收尾

- [ ] T12 自动化收口：`cargo test` 全绿 + `npm run build` 通过；修复回归（依赖: T3~T11）
- [ ] T13 手工验收清单 `acceptance-manual.md`：GUI 交互项 + 真机装机全流程（直连/穿透各一遍）+ 旧机重跑无漂移抽查；对照 spec 逐条验证 AC 并勾选（依赖: T12）
- [ ] T14 文档收尾：CHANGELOG Unreleased 登记、MOC 状态流转、`docs/research/sprint0-cloudcli-lan-deploy.md` §9.3 增补"新机请用装机向导"指引（依赖: T13）

## 完成标志（DoD 检查）

- [ ] spec.md 中所有 AC 已逐条验证通过
- [ ] 自动化测试全部通过
- [ ] 相关文档已更新
- [ ] 本文件全部任务勾选完毕
