# 011-attack-surface-hardening · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 进行中
- **最后更新**: 2026-09-13（评审修订版）

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成

## 阶段 1: 工作台命令面硬化（P1）

- [~] T1 输入校验双闸 + sink 包裹（测试先行）：`validate_domain`/`validate_stack_dir` 纯函数 + `tool_plan` Result 化接线 + `normalize_stack_dir` 写入即拒 + `visible_script_params` 值参数单引号包裹（参数名裸 token，包裹形态单测断言）（验收: AC1/AC2）
- [ ] T2 `prepare_stack` 去短路：exe 已存在同样走 `verify_easytier_binaries`（测试先行：预置篡改文件拒绝 apply）（验收: AC3）
- [ ] T3 CSP 基线：tauri.conf.json 设置指令集，dev/build 双启动功能回归（dev 受拦走 devCsp 出口，见 plan §7）（验收: AC4）

## 阶段 2: 供应链指纹锁定（P2）

- [ ] T4 **首步门控**：caddyserver.com 版本参数与字节可复现性实证（URL 断言 + 两次独立下载比对），当次定 A/B；install-https.ps1 加 `$CaddyCoreVersion`/`$CaddySha256` 常量、**校验严格先于落位**、拒装引导与 `-CaddyZip` 信任转移提示（resources 副本同步归 T8，不在本任务手写）（验收: AC5/AC6）
- [ ] T5 `tools/upgrade-component.ps1`：caddy/easytier 双组件一条命令升级（下载→预检→按行锚定改写锁定值→cargo test→新旧对照输出 + 官方校验和来源打印）；consts.rs 折行感知正则；真机演练一次（依赖: T4——改写其引入的常量对）（验收: AC7）

## 阶段 3: 443 入口账号密码（P3）

- [ ] T6 `set-https-password.ps1`：stdin 取密→`hash-password --algorithm bcrypt`→标记段内联写入（存量装机首跑插入 + .bak 备份 + validate 自检回滚）→`caddy reload` 生效；install-https.ps1 生成端带空标记段；menu.ps1 直启路径 .env 注入缺失顺手修；`ToolKind::SetHttpsPassword` 枚举 + 设置区入口 + 未设密码引导（验收: AC9/AC11）
- [ ] T7 真机验证：401 与正确凭证进入（含 WS 终端重连专项）、存量装机（v0.6.0 升级形态）设置密码成功且重启后仍生效、localhost:3001 不受影响（依赖: T6）（验收: AC8/AC10 真机半）

## 阶段 4: 收尾

- [ ] T8 cargo test 全绿 + `scripts/build.ps1` 打包复核（$ScriptSubset 增 set-https-password.ps1；resources 副本与 manifest 由 build 同步段自动刷新）（依赖: T4~T7）
- [ ] T9 对照 [spec.md](./spec.md) 逐条验证验收标准并勾选；CHANGELOG Unreleased 登记；tools/README 工具表补 upgrade-component.ps1；MOC 状态流转
- [ ] T10 真机手工清单回填 acceptance-manual（沿 007/009/010 先例），需求方签收

## 完成标志（DoD 检查）

- [ ] spec.md 中所有 AC 已逐条验证通过
- [ ] 自动化测试全部通过
- [ ] 相关文档已更新
- [ ] 本文件全部任务勾选完毕
