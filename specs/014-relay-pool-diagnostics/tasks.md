# 014-relay-pool-diagnostics · 任务拆解（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: in-progress
- **创建日期**: 2026-09-25
- **最后更新**: 2026-09-25

> AC 追溯：每任务标注覆盖的 spec 验收标准编号。

## 任务清单

- [x] **T1 文档落位**（验收: spec reviewed + plan/tasks 定稿）：spec.md 需求定稿（需求方三项拍板：内置+可编辑池 / 一键应用健康节点 / 仅手动触发）、plan.md 技术方案、本文件；MOC 登记 ✓（2026-09-25）

- [ ] **T2 候选池数据层**（依赖: T1；验收: AC3/AC7/AC8 后端）：
  ① `consts::RELAY_POOL_BUILTIN`（us01 + vomiku，注释记录实测依据）；
  ② `MeshConfig.relay_pool`（serde default 空 + 旧文件兼容单测）；
  ③ `relay_pool_compose` 归一去重（active/custom/builtin 优先级、非法项保留标 invalid）单测；
  ④ `validate_mesh_config` 扩展 relay_pool 校验（scheme/host 口径同 peers）
  完成标志：`cargo test` 相关用例绿

- [ ] **T3 探测内核与命令**（依赖: T2；验收: AC1/AC2）：
  ① `RelayProbeItem` 结构（camelCase 序列化）；
  ② `relay_probe_pool` 并发探测（thread::scope，复用 probe_tcp/classify_connect）；
  ③ `relay_probe` Tauri 命令 + invoke_handler 注册；
  ④ 归类纯函数单测（refused/timeout/resolve/error/invalid 全分支）
  完成标志：单测绿 + 手动 `relay_probe` 实测 vomiku=refused、us01=ok

- [ ] **T4 应用健康节点命令**（依赖: T3；验收: AC4/AC5/AC6）：
  ① `restart_service_await` seam（MeshOps 注入，stop/start 轮询 + 超时）单测（含超时分支）；
  ② `relay_apply` 命令：空列表拒绝（AC5）→ 交叉校验 health⊂池 → settings.patch → prepare_stack（Err 回滚 peers 不重启）→ 静默重启（失败返回 dispatch_required）；
  ③ 注册命令 + 拒绝/回滚分支单测
  完成标志：单测绿；真机 apply 后 RPC peer 实况含新节点

- [ ] **T5 前端接线**（依赖: T2~T4；验收: AC1/AC3/AC4/AC5/AC7/AC8 UI 面）：
  ① `api.ts` relayProbe/relayApply 封装 + `types.ts` 类型；
  ② 诊断面板「中继节点池」块：检测按钮 / 逐节点结果（✓✗+来源徽标+原因文案）/ 应用按钮（无健康项禁用、三态反馈）；
  ③ mesh 表单自定义候选增删（非法 URI 行内红字）；
  ④ i18n zh/en `mesh.pool.*` 全键对，前端零字面量残留
  完成标志：`npm run build` 过 + 手动走查双语

- [ ] **T6 收口回归**（依赖: T2~T5；验收: 全部 AC 自动化面）：
  ① `cargo test` + `cargo check` + `npm run build` 全绿；
  ② tasks 回填验证证据、spec 状态推进；
  ③ CHANGELOG Unreleased 登记；
  ④ 真机验收清单（需求方）：检测报告双节点实测 / 一键切换后成员入网 / 自定义候选持久化 / 双语走查
  完成标志：自动化面全绿 + 真机清单交付需求方

## 完成定义（DoD）核对清单

- [ ] AC1~AC8 逐条验证（自动化覆盖面 + 真机清单标注归属）
- [ ] `cargo test` / `npm run build` 全绿
- [ ] spec / plan / tasks / MOC 状态同步
- [ ] CHANGELOG Unreleased 登记（用户可感知：诊断面板新增中继节点池体检与一键切换）
