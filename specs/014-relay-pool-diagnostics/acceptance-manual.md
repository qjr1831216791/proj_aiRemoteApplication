# 014-relay-pool-diagnostics · 验收记录（acceptance-manual）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · [tasks.md](./tasks.md) · 返回 [MOC](../MOC.md)

- **创建日期**: 2026-09-25
- **最后更新**: 2026-09-25

## 1. 自动化覆盖面（2026-09-25，cargo 270 绿 / tsc+vite 绿 / node 绿）

| AC | 覆盖 | 证据 |
|----|------|------|
| AC1（逐节点绿/红+原因码） | 部分 | `relay_probe_pool_reason_codes`（ok/refused/invalid 实网+本机 listener）、`relay_pool_compose_dedup_priority_and_invalid_visible`（invalid 可见）；refused/timeout 语义区分由 `classify_connect` 既有单测背书 |
| AC2（3s 超时/并发 ≤6s） | 部分 | 超时值由 `probe_tcp` 复用（既有实现）；并发结构 `thread::scope` 待真机全池实测总耗时 |
| AC3（自定义同列表+来源可区分） | 部分 | compose 单测（custom/builtin 优先级与归一）；UI 呈现真机走查 |
| AC4（一键应用+一致性+RPC 实况） | 部分 | 待真机（涉及服务重启与 RPC 实况） |
| AC5（全灭拒绝应用） | 是 | `validate_relay_apply_rejects_empty_illegal_and_foreign` |
| AC6（状态事件按新配置上报） | 部分 | `restart_service_await_paths`（重启等待四分支）；事件链路待真机 |
| AC7（非法 URI 拒绝） | 是 | `validate_mesh_config_rejects_bad_fields`（候选段）+ `saveMesh` 前端同口径正则 |
| AC8（自定义持久化+内置不污染） | 是 | `relay_pool_field_legacy_compat_and_roundtrip`（旧文件兼容+roundtrip+camelCase）；内置清单为编译期常量结构性不可被用户改写 |

## 2. 真机验收清单（需求方）

前置：开发态 `npm run tauri dev` 或安装包；组网服务已安装。

1. **AC1/AC3 检测呈现**：设置页 → 组网设置 → 「检测中继节点」→ 列表出现 ≥2 节点（us01 绿 + vomiku 红带「端口无服务监听」），来源徽标「生效中/自定义/内置」正确
2. **AC2 并发耗时**：全池检测从点击到出结果 ≈ 3s（非节点数×3s）
3. **AC7 非法候选拦截**：自定义候选输入 `bad uri` 保存 → 行内红字拒绝；改回合法保存成功、重启 APP 后仍在
4. **AC4 一键应用**：检测后点「应用健康节点」→ toast 成功；`easytier-cli --rpc-portal 127.0.0.1:15888 connector` 显示新对端 Connected；设置卡「对端节点」文本框与选择一致
5. **AC5 全灭拒绝**：断网点「检测中继节点」→ 全红、无应用按钮、显示警示；原对端配置未变
6. **AC6 状态跟随**：应用后组网状态事件按新对端实况刷新（在线/连接中），无需重启 APP
7. **双语走查**：zh/en 切换后节点池块文案完整（desc/按钮/来源/五种 reason）
8. **兜底路径**（可选，旧装机）：若 apply 后 toast 提示「需管理员确认」，点上方「应用配置并重启服务」走 UAC 完成重启

## 3. 变更记录

| 日期 | 记录 |
|------|------|
| 2026-09-25 | 自动化面验证完成（270 绿）；真机清单交付需求方 |
