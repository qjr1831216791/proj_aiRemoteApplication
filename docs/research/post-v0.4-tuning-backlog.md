# 调研报告：v0.4.x 调优候选——争议点与遗留项

> - **日期**：2026-09-11
> - **状态**：已确认（需求方决策：接受 1/2/3/5，搁置 4；争议点转下个小版本需求）
> - **范围**：v0.4.0 发版后全仓瘦身（merge `89b63cf`）三路审查的发现 + 发版/验收过程沉淀的遗留项
> - **用途**：下一个小版本的需求输入。含 P1 新用户可见行为建议 **v0.5.0**；仅做 P2~P4 清理则 **v0.4.1**。立项时按工作流从 `specs/_templates/` 起新目录（编号 009），本文不替代 spec。

---

## P1（建议主项）：组网虚拟网段与物理网卡重叠检测——spec 有决议、实现未接线

- **争议事实**：specs/007-mesh-access/spec.md §6 决议「默认 10.126.126.0/24 + 网段可编辑 + **保存/切换时与物理网卡网段重叠检测阻断**（plan §6/§7-R7）」；实现仅交付纯函数 `detect_subnet_conflict` / `same_private_prefix`（`mesh.rs:956`/`mesh.rs:972`）+ 2 个单测，**从未接入** save_settings / apply 路径——`validate_mesh_config` 只做格式校验。spec 007 当前剩余的 2 条编译告警即此缺口的显式提示（瘦身后刻意保留，未删除）。
- **风险（即决议动机）**：用户把虚拟网段设成与局域网同段（如 192.168.1.0/24）时产生路由歧义、静默丢包，现场极难排查。
- **建议方向（补接线）**：保存/应用组网设置时与 `NetMonitor` 当前物理网卡前缀比对，重叠则拒绝保存并返回稳定码（前端 `mesh.code.*` 或新词条 + 设置卡行内错误展示）；单测锁定三态——重叠阻断 / 不重叠放行 / 无网卡环境放行。
- **替代路径**：若判定不做，走变更流程从 spec §6 删除该决议并在「变更记录」登记。**不允许维持现状的默默偏离**（CLAUDE.md 铁律 2）。

## P2（小项，顺手清）：测试辅助器死代码告警 ×4

lib test 编译的 4 条 never-used/read 告警：

- `heartbeat.rs:248`：`healthy` 赋值后未读
- `orchestrator.rs:760`：`position` 方法
- `orchestrator.rs:822`：`port_calls` 方法
- `orchestrator.rs:1125`：`wait_for_state` 函数

处理：确认零消费后删除（或测试内收敛）；纯清理不改变行为，免 spec。

## P3（运营事项，非代码）：真机旧通道残留清理未执行

- 008 的 AC10/AC11（真机卸载）按需求方签收免实测，`uninstall-legacy.ps1` 随包保留为可选动作。
- 真机磁盘现状：栈目录仍有 frpc / ddns-go 文件与 `.env` 的 `SAKURA_FRP_KEY` 行。
- 建议：下个维护窗口在真机执行一次（脚本幂等；内置组网服务在线 + 腾讯凭证存在双闸门，缺凭证时拒删 ddns-go.yaml 并指引 `set-tencent-key.ps1`），执行结果回填 specs/008 的 acceptance-manual。

## P4（流程改进建议）：发版版本号校验自动化

- 教训：0.3.0 发布时三版本文件漏 bump（停在 0.2.0，v0.4.0 对齐跳号修正）。
- 现状：CHANGELOG 底部「发版四步」是文档性约定，无机器校验。
- 建议：`scripts/` 增加 release-check（或并入 build.ps1 前置步骤）——比对 `package.json` / `src-tauri/Cargo.toml` / `src-tauri/tauri.conf.json` 三处版本一致，并确认 CHANGELOG 存在对应版本节；不一致即非零退出。

## 搁置记录（需求方 2026-09-11 决定）

- `config/`（仅 README 模板，代码零引用）与 `tests/`（仅 `.gitkeep`）两个空目录：**搁置不删**，保留为约定占位；日后若清理需同步 CLAUDE.md「目录结构」树。

## 已随 v0.4.0 后瘦身处理（不进下版本，防止重复登记）

- menu.ps1 选项 3 访问地址分场景（域名不再标注「推荐」，独立行注明组网前提）——需求方决策第 5 项，与本报告同批提交
- MeshCard port-held 文案修复 / i18n 死键 4 项（250→246）/ Rust 死代码退役（告警 14→2）——见 CHANGELOG Unreleased 与 merge `89b63cf`
- 分支清理与 push main / tag v0.4.0——需求方决策第 1/2 项
