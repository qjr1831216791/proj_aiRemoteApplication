# 014-relay-pool-diagnostics · 任务拆解（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: in-progress
- **创建日期**: 2026-09-25
- **最后更新**: 2026-09-25

> AC 追溯：每任务标注覆盖的 spec 验收标准编号。

## 任务清单

- [x] **T1 文档落位**（验收: spec reviewed + plan/tasks 定稿）：spec.md 需求定稿（需求方三项拍板：内置+可编辑池 / 一键应用健康节点 / 仅手动触发）、plan.md 技术方案、本文件；MOC 登记 ✓（2026-09-25）

- [x] **T2 候选池数据层**（依赖: T1；验收: AC3/AC7/AC8 后端）：
  ① `consts::RELAY_POOL_BUILTIN`（us01 + vomiku，注释记录实测依据）✓；
  ② `MeshConfig.relay_pool`（serde default 空 + 旧文件兼容单测 `relay_pool_field_legacy_compat_and_roundtrip`）✓；
  ③ `relay_pool_compose` 归一去重（active/custom/builtin 优先级、非法项保留标 invalid——口径对齐 `valid_peer_uri` 严格形态，单测 `relay_pool_compose_dedup_priority_and_invalid_visible`）✓；
  ④ `validate_mesh_config` 扩展 relay_pool 校验（`validate_mesh_config_rejects_bad_fields` 候选段）✓
  完成标志：`cargo test` 268 绿 ✓（2026-09-25）

- [x] **T3 探测内核与命令**（依赖: T2；验收: AC1/AC2）：
  ① `RelayProbeItem` 结构（camelCase 序列化，单测 `relay_probe_item_serialization_contract`）✓；
  ② `relay_probe_pool` 并发探测（thread::scope，复用 probe_tcp/classify_connect；invalid 判定同 compose 严格口径）✓；
  ③ `relay_probe` Tauri 命令 + invoke_handler 注册 ✓；
  ④ 归类实网单测 `relay_probe_pool_reason_codes`（本机 listener=ok / 127.0.0.1:1=refused / invalid 免探测）✓
  完成标志：单测绿 ✓；vomiku=refused / us01=ok 已于 2026-09-24~25 排障期间经同款 TCP 探测实测 ✓

- [x] **T4 应用健康节点命令**（依赖: T3；验收: AC4/AC5/AC6）：
  ① `restart_service_await` seam（run_sc + MeshOps 双注入，`restart_service_await_paths` 覆盖正常流/stop 拒绝/start 拒绝/停止超时/启动超时/1056 容忍）✓；
  ② `relay_apply` 命令：空列表拒绝（AC5 `validate_relay_apply_rejects_empty_illegal_and_foreign`）→ 交叉校验 health⊆池 → settings.patch → prepare_stack（Err 回滚 peers 不重启）→ 静默重启（失败返回 dispatch_required）✓；
  ③ 注册命令 ✓（真机 apply 后 RPC 实况验证归入真机清单 #4）
  完成标志：单测绿 ✓（270）

- [x] **T5 前端接线**（依赖: T2~T4；验收: AC1/AC3/AC4/AC5/AC7/AC8 UI 面）：
  ① `api.ts` relayProbe/relayApply 封装 + `types.ts` RelayProbeItem/RelayApplyOutcome/MeshConfig.relayPool ✓；
  ② 诊断面板「中继节点池」块：检测按钮 / 逐节点结果（✓✗+来源+原因文案）/ 应用按钮（全灭时以警示替代——AC5）✓；
  ③ mesh 表单自定义候选 textarea（`saveMesh` 同 peers 口径校验、随 mesh 补丁整块保存）✓；
  ④ i18n zh/en `mesh.pool.*` + `settings.meshPool*` 全键对，文案零字面量 ✓
  完成标志：`tsc && vite build` 过 ✓（2026-09-25；双语手动走查归真机清单 #7）

- [x] **T6 收口回归**（依赖: T2~T5；验收: 全部 AC 自动化面）：
  ① `cargo test` 270 绿 + `cargo check` 过（顺手补 013 遗留 WORKBENCH_URL dead_code 注解）+ `npm run build` 绿 + node 测试 fail 0 ✓（2026-09-25）；
  ② tasks 回填 ✓；spec 状态保持 in-progress（真机验收后 done）；
  ③ CHANGELOG Unreleased「Added」登记 ✓；
  ④ 真机验收清单已建 `acceptance-manual.md`（8 项，含 AC 归属标注）——**待需求方执行**
  完成标志：自动化面全绿 ✓ + 真机清单交付需求方 ✓

- [x] **T7 变更：分享链接导入（US4/AC9~AC11）**（依赖: T6 前提；2026-09-25 需求方追加）：
  ① spec/plan 变更记录 + US4 三 AC ✓；
  ② 内核 `relay_list_extract`（手写扫描不加 regex 依赖，白名单 7 协议，Markdown 包裹/中英文标点终止；真实 #2429 排版样例单测 `relay_list_extract_from_markdown_share_post`）+ `relay_list_fetch`（GitHub Discussions 自动转 `api.github.com` REST 正文+评论——issues API 对 discussion 恒 404 之谜实测解开；https-only + 10s 超时 + 1MB 上限）✓；
  ③ `relay_fetch_nodes` 命令注册 ✓；
  ④ 前端「从社区分享链接获取」行（预填 #2429，合并去重进候选 textarea、不直接生效）+ i18n 键对 ✓
  完成标志：`cargo test` 272 绿 + 前端构建绿 ✓（2026-09-25；真机导入走查归验收清单 #9/#10）

## 完成定义（DoD）核对清单

- [ ] AC1~AC8 逐条验证（自动化覆盖面 + 真机清单标注归属）
- [ ] `cargo test` / `npm run build` 全绿
- [ ] spec / plan / tasks / MOC 状态同步
- [ ] CHANGELOG Unreleased 登记（用户可感知：诊断面板新增中继节点池体检与一键切换）
