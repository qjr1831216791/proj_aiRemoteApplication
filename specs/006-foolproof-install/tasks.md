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

- [x] T3 `install-https.ps1` 改造：Caddy 下载源改按需构建 API（锁 `tencentcloud@v0.4.3`，`-CaddyZip` 兜底与手动下载指引）+ Caddyfile 模板改插件式（`{env.*}` 引用）；既有 Caddyfile 幂等不覆盖；`build.ps1` 子集登记（依赖: T2）（验收: AC6）
- [x] T4 `set-tencent-key.ps1`：不回显输入 SecretId/Key → 两次确认 → 原位写栈 `.env`（其余行保留）→ 复核显示 ID 末 4 位；入库 + `tools/sprint0/bin` 副本（依赖: T2）（验收: AC4）
- [x] T5 `config-ddnsgo.ps1`：从 `.env` 读凭证 → 生成 `ddns-go.yaml`（tencentcloud / url 模式取 IP / 域名）→ 启动 ddns-go（依赖: T4）（验收: AC8）
- [x] T6 `{env.*}` 凭证注入全链路：CommandSpec 增 SecretEnv（Debug 屏蔽）+ caddy_run 注入；新增 `run-caddy-hidden.ps1`（计划任务拉起：读 .env → 注入 → 前台 run）；setup-autostart Caddy action 换 wrapper（依赖: T2）（验收: AC7）

## 阶段 3: Rust 编排层

- [x] T7 `wizard.rs` 状态机与持久化：WizardState、wizard-state.json 加载/损坏恢复/原子写（settings.rs 同款语义）+ 探针纯函数 + 8 项单测（依赖: T2）（验收: AC1/2/12）
- [x] T8 腾讯云校验编排：复用 `dns_api.rs` TC3 签名做"密钥有效+域名存在"探测、记录对齐与出口形态警示（warn_* 稳定码）、错误码映射 + 单测（依赖: T7）（验收: AC4/5/8）
- [x] T9 commands 与接线：`wizard_get_state/detect/set_domain/set_branch/complete` 五命令 + `wizard://changed` 事件 + `lib.rs` 注册；set_branch 复用 004 通道收敛；ToolKind 增 set_tencent_key/config_ddnsgo + ToolOpts.domain 透传（派发复用 run_tool，plan 变更记录 ②③）（依赖: T7/T8）（验收: AC3/9/10/11）

## 阶段 4: 前端

- [x] T10 `WizardView`：第三视图 + 竖向阶段清单（四态 chip）+ 阶段面板（外部办理模式/凭证脚本拉起/分支单选卡/收尾清单）+ `api.ts`/`types.ts` 扩展 + zh/en 词典补全（依赖: T9）（验收: §4.1 全部交互规则）
- [x] T11 低频栏瘦身（AC13/14）：ToolsSection 移除装机三件套与「打开工作台网页」、更名运维工具；MainView 装机引导条 + 组件卡「去安装」改跳向导（依赖: T10）
- [x] T15 收尾阶段调整（需求方反馈）：移除「注册开机自启」入口（自启注册统一走设置页开关，AC11 修订）、目标达成清单呈现（去项目符号 + 标签与状态点分列对齐 + 拉开行距）（依赖: T10）（验收: AC11）
- [~] T16 栈目录权限加固（安全修复）：`install-https.ps1` 建目录后调用 `Protect-StackDir`——断开 `D:\` 默认继承（原继承把 `Authenticated Users` 授为可修改），只授「运行账户 ∪ 交互登录用户 + SYSTEM + Administrators」；取 SID 而非账户名规避中文编码坑，失败如实报红不静默放行（依赖: T3）（验收: 交付物安全属性——栈目录含 `.env` 与证书私钥，不得对本机其他账户可读写）。脚本改动 + 语法检查 + 真机权限实测已完成，**管理员端到端重跑待 T13 一并覆盖**

## 阶段 5: 测试与收尾

- [x] T12 自动化收口：`cargo test` 164 项全绿 + `npm run build` 通过（依赖: T3~T11）
- [ ] T13 手工验收清单 `acceptance-manual.md`：GUI 交互项 + 真机装机全流程（直连/穿透各一遍）+ 旧机重跑无漂移抽查；对照 spec 逐条验证 AC 并勾选（依赖: T12）
- [ ] T14 文档收尾：CHANGELOG Unreleased 登记、MOC 状态流转、`docs/research/sprint0-cloudcli-lan-deploy.md` §9.3 增补"新机请用装机向导"指引（依赖: T13）

## 完成标志（DoD 检查）

- [ ] spec.md 中所有 AC 已逐条验证通过
- [ ] 自动化测试全部通过
- [ ] 相关文档已更新
- [ ] 本文件全部任务勾选完毕
