# 009-mesh-subnet-guard · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 进行中 <!-- 未开始 | 进行中 | 已完成 -->
- **最后更新**: 2026-09-11（T1/T2/T4/T5/T7/T8/T9/T10 已完成；T3/T6/T9 真机手工与 T11/T12 收尾待代理交付后主会话执行）

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成

## 阶段 1: 网段冲突阻断（US1）

- [x] T1 接线层测试先行：`render_config_checked` 网段检测三态单测（重叠 Err 含冲突 IP / 不重叠 Ok / 空网卡列表 Ok），含「剔除 ==virtual_ip 的 TUN 自身地址」锚定用例（验收: AC1/AC2/AC3）
- [x] T2 实现：`render_config_checked` 在 `validate_mesh_config` 通过后调 `detect_subnet_conflict(local_ipv4_addrs() 剔除 virtual_ip, …)`，非空 → `Err` 含冲突网段与 IP 列表；空列表走 log 记录跳过（fail-open 可观察）。向导与设置卡同入口的结构性注释锚定（依赖: T1）（验收: AC1~AC4）
- [ ] T3 本机实跑验证：设置卡构造重叠网段保存 → toast 阻断、设置未落盘；正常网段保存不受影响（依赖: T2）（验收: AC1/AC2 手工复核）

## 阶段 2: 成员入网配置（US4）

- [x] T4 Rust：`render_member_config`（format! 模板：network_name 实值 + network_secret 占位 + peers 全量 + 行注释）+ 单测 `toml::from_str` 反序列化断言字段一致（验收: AC11）
- [x] T5 命令与前端：`mesh_member_config` 命令注册 + `api.meshMemberConfig` + MeshCard 设置卡折叠区（pre 展示 + 一键复制 + 密钥指引文案）+ i18n 词条 zh/en 双侧（依赖: T4）（验收: AC9/AC10）
- [ ] T6 真机 App 对照复核：手机 EasyTier App 打开配置对照措辞核对（依赖: T5）（验收: AC9 手工）

## 阶段 3: 发版校验脚本（US2）

- [x] T7 `scripts/release-check.ps1`：三处版本读取（package.json JSON / Cargo.toml 锚定 [package] 段 / tauri.conf.json JSON）+ CHANGELOG 版本节存在性；`-Version` 参数（默认 package.json）；只读、PS 5.1、BOM+CRLF、T() 双语；退出码 0/1（验收: AC5/AC6/AC7）
- [x] T8 脚本三态自测：仓库现状正例 + `-Version 9.9.9` 负例 + 临时副本版本不一致负例；Parser/BOM/CRLF 校验；CLAUDE.md 常用命令补一行（依赖: T7）（验收: AC5/AC6/AC7）

## 阶段 4: 真机清理与收尾（US3 + 通用）

- [x] T9 真机执行随包 `uninstall-legacy.ps1`（需求方在场）：核对台账全 Done/Skipped、四类残留清除、`SAKURA_FRP_KEY` 行移除且余行保留；台账摘要回填 specs/008 acceptance-manual（验收: AC8）
- [x] T10 顺手清理（免 spec）：4 条 test-only 告警——`heartbeat.rs:248` healthy、`orchestrator.rs:760` position、`:822` port_calls、`:1125` wait_for_state（验收: cargo check --tests 告警归零）
- [ ] T11 全量回归 + 手工验收清单：cargo test / npm run build / 键集双向差集 / ps1 校验全绿；新建 acceptance-manual.md 逐 AC 步骤-预期-结论（验收: 全 AC 复核）
- [ ] T12 文档收口：CHANGELOG Unreleased 登记、MOC 状态流转、spec AC 勾选与状态 done、调优报告标记已消化项（依赖: T11）

## 完成标志（DoD 检查）

- [ ] spec.md 中所有 AC 已逐条验证通过
- [ ] 自动化测试全部通过
- [ ] 相关文档已更新
- [ ] 本文件全部任务勾选完毕
