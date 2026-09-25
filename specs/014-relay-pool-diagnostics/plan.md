# 014-relay-pool-diagnostics · 技术方案（plan）

> 导航：[spec.md](./spec.md) · [tasks.md](./tasks.md) · 返回 [MOC](../MOC.md)

- **状态**: in-progress
- **创建日期**: 2026-09-25
- **最后更新**: 2026-09-25

## 1. 技术选型与总体思路

零新依赖。全部增量在既有骨架上扩展：

- **池来源**：`consts.rs` 新增内置清单常量（编译期，随版本更新）+ `MeshConfig` 新增 `relay_pool` 字段（serde default 空表，向后兼容旧 settings.json）。
- **探测**：复用 `mesh.rs` 既有 `probe_tcp`（3s 超时）与 `classify_connect`（refused/timeout/resolve/error 分类——本次实战验证的根因区分口径）；全池并发用 `std::thread::scope`，总耗时 ≈ 最慢单节点。
- **应用**：复用 `SettingsState::patch`（补丁写）+ `prepare_stack`（渲染→落位→SHA256→写盘→`--check-config`）+ 服务静默重启（`sc stop`/`sc start`，007 SDDL 已授权交互用户，免提权）；不走 `mesh_apply_config` 的 UAC 组合派发（peers 变更不涉及网段白名单）。
- **UI**：`SettingsView.tsx` 诊断面板扩展（探测结果列表 + 应用按钮）；mesh 表单区新增自定义候选编辑。i18n 沿 `mesh.diag.*` 键惯例。

## 2. 架构与数据流

```
[consts::RELAY_POOL_BUILTIN]──┐
[MeshConfig.relay_pool 用户]──┼→ relay_pool_compose() 归一去重 ─→ relay_probe 命令
[MeshConfig.peers 生效中]─────┘        （标注 source）              │ 并发 probe_tcp
                                                                  ▼
前端 SettingsView 体检面板 ←──────── RelayProbeItem[]（逐节点 ok/reason）
        │ 用户点「应用健康节点」
        ▼
relay_apply 命令：交叉校验（health⊂池，011 命令面惯例）
  → settings.patch(peers=healthy) → prepare_stack（失败：回滚 peers + Err，不重启）
  → 静默重启（stop→await Stopped→start→await Running，超时回退提示手动 apply）
  → 返回新 peers + 服务态
```

## 3. 数据模型与接口契约

### 3.1 常量（consts.rs）

```rust
/// 内置候选（探测把关，不保证可用）：us01 为 2026-09-25 实测活节点；
/// vomiku 为历史默认（死亡状态，复活可被检测发现）
pub const RELAY_POOL_BUILTIN: &[&str] = &[
    "tcp://us01.225284.xyz:11010",
    "tcp://sh.vomiku.com:7910",
];
```

### 3.2 MeshConfig 扩展（settings.rs）

```rust
pub struct MeshConfig {
    // ...既有四字段...
    /// 自定义候选中继（014；仅候选不生效，relay_probe 探测 / relay_apply 才入 peers）
    #[serde(default)]
    pub relay_pool: Vec<String>,
}
```

旧文件缺字段 → 空（`#[serde(default)]` 既有惯例，兼容性单测锁定）。

### 3.3 Tauri 命令（commands.rs，注册 `invoke_handler`）

```rust
relay_probe() -> Vec<RelayProbeItem>
// RelayProbeItem { uri, source: "builtin"|"custom"|"active", active: bool, ok: bool, reason }
// reason ∈ ok | refused | timeout | resolve | error | invalid（refused=端口无监听，
// timeout=网络不可达/被拦，resolve=域名解析失败，invalid=URI 非法）

relay_apply(healthy: Vec<String>) -> RelayApplyResult
// RelayApplyResult { peers: Vec<String>, service: "running"|"start_pending"|"dispatch_required" }
// 校验：healthy 非空（空=AC5 拒绝）、逐项 valid_peer_uri、health ⊆ 池（防前端伪造）
// dispatch_required = 静默重启失败，需用户走设置区「应用配置并重启」（UAC 兜底）
```

### 3.4 mesh.rs 新增内核（纯函数优先、seam 可测）

- `relay_pool_compose(builtin, custom, active) -> Vec<(String, Source)>`：归一（trim + 小写 host + 端口缺省补 `MESH_LISTEN_PORT`）去重，source 优先级 active > custom > builtin；沿用 `valid_peer_uri` 过滤非法项（invalid 也入结果列表，让用户看到坏数据）。
- `relay_probe_pool(items) -> Vec<RelayProbeItem>`：`std::thread::scope` 并发探测，逐项套 `peer_host_port` + `probe_tcp`。
- `restart_service_await(ops, stop_wait, start_wait) -> Result<(), ()>`：sc stop → 轮询 `ops.service_state()` 至 Stopped → sc start → 轮询至 Running/StartPending；seam 注入 `MeshOps` mock 单测（超时分支覆盖）。

## 4. 前端（SettingsView.tsx + i18n）

- 诊断面板「运行诊断」下方新增独立块「中继节点池」：`检测中继节点` 按钮 → 逐节点行（✓/✗ + uri + 来源徽标 + 原因文案）→ `应用健康节点` 按钮（结果中无 ok 项则禁用并提示；应用中转圈；完成提示含重启语义）。
- mesh 表单区新增「自定义候选」标签式增删输入（沿用 peers 编辑的校验提示形态；保存随既有 mesh 补丁一起提交，非法 URI 前端即时红字 + 后端 `validate_mesh_config` 兜底扩展）。
- i18n 新键对（zh/en 对称，`mesh.pool.*` 命名空间）：标题、按钮、来源徽标、六个 reason 文案、应用三态提示、空结果提示。前端零字面量（013 惯例，文案全走 `t()`）。

## 5. 风险与对策

| 风险 | 对策 |
|------|------|
| 静默重启在旧装机（无 SDDL 授权）失败 | 返回 `dispatch_required`，前端引导走既有 UAC「应用配置」按钮（兜底路径现成） |
| prepare 校验失败时 config 已写盘但服务未重启（`prepare_stack` 既有语义） | relay_apply 在 prepare Err 时不派发重启并回滚 settings.peers，UI 明确报错 |
| 内置清单随时间失效（社区节点无 SLA） | 池设计即为此：检测把关 + 用户自定义兜底；清单随版本维护 |
| 探测被误解为"正在切换" | 探测为纯只读（spec §4.3），UI 文案与按钮语义分离（检测≠应用） |
| peers 与 relay_pool 概念混淆 | 文案区分「生效对端」与「候选节点」；应用动作是唯一从候选到生效的通道 |

## 6. 变更记录

| 日期 | 变更内容 | 原因 |
|------|----------|------|
| 2026-09-25 | 初稿 | spec reviewed 后细化；静默重启兜底与 prepare 回滚为方案要点 |
| 2026-09-25 | US4 拉取导入：复用 `ureq`（既有依赖，rustls）；`relay_list_extract` 纯函数扫描提取白名单协议 URI（无 regex 依赖，手写扫描）；GitHub Discussions 链接识别后转 `api.github.com` REST（正文+评论 per_page=100），网页不作依赖（真机直抓被拦实测）；https-only + 10s 超时 + 1MB 上限；前端拉取后直接合并进候选 textarea（可手动增删），不做勾选 UI（手动维护语义由既有编辑承载） | 需求方追加来源导入；#2429 实测为 Discussion（issues API 404 之谜解开） |
