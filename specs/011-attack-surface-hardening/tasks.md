# 011-attack-surface-hardening · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 进行中
- **最后更新**: 2026-09-13（T1/T2/T4 完成，T3 配置半完成）

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成

## 阶段 1: 工作台命令面硬化（P1）

- [x] T1 输入校验双闸 + sink 包裹（测试先行）：`validate_domain`/`validate_stack_dir` 纯函数 + `tool_plan` Result 化接线 + `normalize_stack_dir` 写入即拒 + `visible_script_params` 值参数单引号包裹（参数名裸 token，包裹形态单测断言）（验收: AC1/AC2）→ 763d261，新增 10 测试；附带修正 `service_action_params` 双重包裹隐患
- [x] T2 `prepare_stack` 去短路：exe 已存在同样走 `verify_easytier_binaries`（测试先行：预置篡改文件拒绝 apply）（验收: AC3）→ a9c24eb，新增 2 测试（逐文件篡改拒绝 + 完好不误伤）
- [~] T3 CSP 基线：tauri.conf.json 设置指令集 ✅ 521dddd（cargo check 编译期解析通过）；**余 dev/build 双启动功能回归**（dev 受拦走 devCsp 出口，见 plan §7）——并入 T7 真机清单执行（验收: AC4）

## 阶段 2: 供应链指纹锁定（P2）

- [x] T4 **首步门控**：caddyserver.com 版本参数与字节可复现性实证（URL 断言 + 两次独立下载比对），当次定 A/B；install-https.ps1 加 `$CaddyCoreVersion`/`$CaddySha256` 常量、**校验严格先于落位**、拒装引导与 `-CaddyZip` 信任转移提示（resources 副本同步归 T8，不在本任务手写）（验收: AC5/AC6）→ 5818743，**门控通过定方案 A**：钉版方式 `p=github.com/caddyserver/caddy/v2@版本`（v2.11.4），三次下载哈希恒定；隔离演练四场景全过；上游只伺服最新版的风险已记 plan §7
- [ ] T5 `tools/upgrade-component.ps1`：caddy/easytier 双组件一条命令升级（下载→预检→按行锚定改写锁定值→cargo test→新旧对照输出 + 官方校验和来源打印）；consts.rs 折行感知正则；真机演练一次（依赖: T4——改写其引入的常量对）（验收: AC7）

## 阶段 3: 443 入口账号密码（P3）

- [ ] T6 访问账号多账号管理（测试先行，工作台发起 + 脚本执行）：新增 `tools/sprint0/bin/set-https-account.ps1`（add/set/remove 三动作：Read-Host 隐藏回显取密两次+一致/长度校验 → 栈目录 caddy.exe hash-password stdin → `auth-accounts.json` 读写 → 标记段再生（存量首插/多账号形态/空列表回空段）+ .bak 备份回滚 + caddy validate/reload）；Rust 侧 `https_auth_list`（只读用户名）+ `ToolKind::SetHttpsAccount` 派发（ToolOpts 非敏感 `auth_action`/`auth_user` 白名单校验+值包裹，单测）；前端设置区「访问账号」界面（列表展示/发起新增或改密（界面只收用户名）/移除确认）；install-https.ps1 生成端带空标记段；build.ps1 $ScriptSubset 登记；menu.ps1 直启 .env 注入缺失顺手修；未设账号引导提示（验收: AC9/AC11）
- [ ] T7 真机验证：多账号各自凭证 401/进入（含 WS 终端重连专项）、移除账号后新请求 401 且重连被拒、存量装机（v0.6.0 升级形态）首次管理自动插入标记段且重启后仍生效、localhost:3001 不受影响（依赖: T6）（验收: AC8/AC10 真机半）

## 阶段 4: 收尾

- [ ] T8 cargo test 全绿 + `scripts/build.ps1` 打包复核（$ScriptSubset 含 set-https-account.ps1；resources 副本与 manifest 由 build 同步段自动刷新）（依赖: T4~T7）
- [ ] T9 对照 [spec.md](./spec.md) 逐条验证验收标准并勾选；CHANGELOG Unreleased 登记；tools/README 工具表补 upgrade-component.ps1；MOC 状态流转
- [ ] T10 真机手工清单回填 acceptance-manual（沿 007/009/010 先例），需求方签收

## 完成标志（DoD 检查）

- [ ] spec.md 中所有 AC 已逐条验证通过
- [ ] 自动化测试全部通过
- [ ] 相关文档已更新
- [ ] 本文件全部任务勾选完毕
