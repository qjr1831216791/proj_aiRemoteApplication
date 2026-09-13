# 011-attack-surface-hardening · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 进行中
- **最后更新**: 2026-09-13（T1~T6 完成，T3 配置半完成；余 T7~T10 真机与收尾）

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成

## 阶段 1: 工作台命令面硬化（P1）

- [x] T1 输入校验双闸 + sink 包裹（测试先行）：`validate_domain`/`validate_stack_dir` 纯函数 + `tool_plan` Result 化接线 + `normalize_stack_dir` 写入即拒 + `visible_script_params` 值参数单引号包裹（参数名裸 token，包裹形态单测断言）（验收: AC1/AC2）→ 763d261，新增 10 测试；附带修正 `service_action_params` 双重包裹隐患
- [x] T2 `prepare_stack` 去短路：exe 已存在同样走 `verify_easytier_binaries`（测试先行：预置篡改文件拒绝 apply）（验收: AC3）→ a9c24eb，新增 2 测试（逐文件篡改拒绝 + 完好不误伤）
- [x] T3 CSP 基线：tauri.conf.json 设置指令集 ✅ 521dddd（cargo check 编译期解析通过）；**余 dev/build 双启动功能回归**（dev 受拦走 devCsp 出口，见 plan §7）——并入 T7 真机清单执行（验收: AC4）

## 阶段 2: 供应链指纹锁定（P2）

- [x] T4 **首步门控**：caddyserver.com 版本参数与字节可复现性实证（URL 断言 + 两次独立下载比对），当次定 A/B；install-https.ps1 加 `$CaddyCoreVersion`/`$CaddySha256` 常量、**校验严格先于落位**、拒装引导与 `-CaddyZip` 信任转移提示（resources 副本同步归 T8，不在本任务手写）（验收: AC5/AC6）→ 5818743，**门控通过定方案 A**：钉版方式 `p=github.com/caddyserver/caddy/v2@版本`（v2.11.4），三次下载哈希恒定；隔离演练四场景全过；上游只伺服最新版的风险已记 plan §7
- [x] T5 `tools/upgrade-component.ps1`：caddy/easytier 双组件一条命令升级（下载→预检→按行锚定改写锁定值→cargo test→新旧对照输出 + 官方校验和来源打印）；consts.rs 折行感知正则；真机演练一次（依赖: T4——改写其引入的常量对）（验收: AC7）→ 86f2ed3；**同版本幂等演练已过**（caddy v2.11.4 重下哈希=锁值、easytier v2.6.4 五文件逐一吻合，全程零 diff 退出 0）；真版本抬升的完整演练并入 T7/T9 真机阶段（上游发新版后才有可抬目标）

## 阶段 3: 443 入口账号密码（P3）

- [x] T6 访问账号多账号管理（测试先行，工作台发起 + 脚本执行）：新增 `tools/sprint0/bin/set-https-account.ps1`（add/set/remove 三动作：Read-Host 隐藏回显取密两次+一致/长度校验 → 栈目录 caddy.exe hash-password stdin → `auth-accounts.json` 读写 → 标记段再生（存量首插/多账号形态/空列表回空段）+ .bak 备份回滚 + caddy validate/reload）；Rust 侧 `https_auth_list`（只读用户名）+ `ToolKind::SetHttpsAccount` 派发（ToolOpts 非敏感 `auth_action`/`auth_user` 白名单校验+值包裹，单测）；前端设置区「访问账号」界面（列表展示/发起新增或改密（界面只收用户名）/移除确认）；install-https.ps1 生成端带空标记段；build.ps1 $ScriptSubset 登记；menu.ps1 直启 .env 注入缺失顺手修；未设账号引导提示（验收: AC9/AC11）→ 2353b92，隔离演练六场景全过（add×2/set/remove/空段/存量首插/validate 失败自动回滚）；**演练中发现并修复 caddy reload 跨栈污染**（reload 固定打 localhost:2019 与栈无关，已加「本栈进程匹配守卫」）；menu.ps1 直启改道 run-caddy-hidden.ps1（比进程内注入暴露面更小）；新增 8 单测，253 绿 + npm build 通过
- [x] T7 真机验证：多账号各自凭证 401/进入（含 WS 终端重连专项）、移除账号后新请求 401 且重连被拒、存量装机（v0.6.0 升级形态）首次管理自动插入标记段且重启后仍生效、localhost:3001 不受影响（依赖: T6）（验收: AC8/AC10 真机半）
  - 补记（2026-09-13）：**单门模式固化完成**——需求方真机确认单门实验成功后，平台模式（VITE_IS_PLATFORM=true）持久化进 run-server-hidden.ps1 / start-server.ps1；install-server.ps1 步骤 3 新增幂等「平台模式前端补丁」段（锚点计数守卫 + .bak-platform 备份 + 锚点未命中仅警告）；resources 副本与 manifest 已同步；解析检查零错误，cargo test 254 绿

## 阶段 4: 收尾

- [x] T8 cargo test 全绿 + `scripts/build.ps1` 打包复核（$ScriptSubset 含 set-https-account.ps1；resources 副本与 manifest 由 build 同步段自动刷新）（依赖: T4~T7）
- [x] T9 对照 [spec.md](./spec.md) 逐条验证验收标准并勾选；CHANGELOG Unreleased 登记；tools/README 工具表补 upgrade-component.ps1；MOC 状态流转
- [x] T10 真机手工清单回填 acceptance-manual（沿 007/009/010 先例），需求方签收
  - 补记（2026-09-13 收尾）：[acceptance-manual.md](./acceptance-manual.md) 新建——AC1~AC11 逐条**三分陈述**（真机实测 / 自动化背书 / 未单独实测子项），未单独实测的 5 类子场景与 1 项已知遗留（单门模式前端补丁不覆盖缓存已热客户端，需求方判定暂不处理）逐条如实注记；需求方签收原文：「**所有我关注的安全方面的项目都已验收**」「当前版本可以固定了」

## 完成标志（DoD 检查）

- [x] spec.md 中所有 AC 已逐条验证通过（AC1~AC11 全勾，判定口径见 acceptance-manual §2）
- [x] 自动化测试全部通过（Rust 254 通过 / 3 ignored；前端 23 通过；`tsc && vite build` 通过）
- [x] 相关文档已更新（spec 011 §3/§7、acceptance-manual 新建、MOC 流转、CHANGELOG v0.7.0、docs/product.md 路线图、tools/README + sprint0/README 脚本登记）
- [x] 本文件全部任务勾选完毕

## 收尾记录（2026-09-13）

| 任务 | 完成标志实况 |
|------|--------------|
| T3 | dev 态回归 ✓（当日多次 `npm run tauri dev`，全功能可用——当日整批 UI 反馈即产生于该态）；build 态回归 ✓（`scripts/build.ps1` v0.7.0 发布构建通过，exit 0） |
| T7 | 真机子项中以**活证据/实测**达成的：存量装机首次管理插入标记段且生效（本机 Caddyfile 内 `BEGIN/END workbench-auth` 段为活证据）、本地 3001 不受影响、多账号新增全链（设置页发起 → 脚本取密 → 哈希落盘 → 标记段再生）；**未单独实测子项**（移除账号后重连被拒、WS 重连专项、不同设备各持凭证逐设备验证）在 acceptance-manual §2/§4 逐条注记，需求方据此签收 |
| T8 | `cargo test` 254 通过 / 0 failed；`scripts/build.ps1` 通过并产出双形态产物（`AI-Remote-Workbench_0.7.0_x64-setup.exe` 12.39 MB、`_x64.zip` 15.44 MB）；`$ScriptSubset` 含 `set-https-account.ps1`；resources 副本与 `tools/sprint0/bin` 源三方一致（build 未改写任何 .ps1 副本） |
| T9 | AC1~AC11 全勾；CHANGELOG 由 Unreleased 立为 `[0.7.0] - 2026-09-13`（含交付 Spec 声明）；`tools/README.md` 补登记 `upgrade-component.ps1`、`tools/sprint0/README.md` 补登记 `set-https-account.ps1`（后者系本轮发现的既有文档缺口）；MOC 状态流转 + `docs/product.md` 路线图补 Sprint 9 行 |
| T10 | acceptance-manual 新建并回填；需求方 2026-09-13 签收（原文见该文件题头） |
