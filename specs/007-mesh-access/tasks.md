# 007-mesh-access · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 进行中 <!-- 未开始 | 进行中 | 已完成 -->
- **最后更新**: 2026-09-10

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成

## 阶段 1: 资源与前置实测

- [x] T1 easytier v2.6.4 二进制入库：下载 windows-x64 官方 Release（easytier-core.exe + easytier-cli.exe + **wintun.dll**——TUN 驱动库随包，落位必须同带），落 `resources/bin/`，consts.rs 登记版本 + 三文件 SHA256，`mesh.rs` 校验函数 + 单测 3 项（真文件/缺失/篡改）。（验收: AC1 前置；完成标志：cargo 168 全绿 ✓。下载经 gh 官方通道；镜像通道文件与官方不符已弃用——供应链教训记 plan §7-R6）
- [ ] T2 外部事实真机实测（依赖: T1；验收: AC1/AC9 前提 + spec §5 验收前提约束）：①临时 config + 手动服务方式跑通社区节点 `tcp://sh.vomiku.com:7910`（移动网络 + WiFi 双环境）；②Android 官方 App 以 legacy 身份加入 secure-mode 网络，验证连通（plan §3.1 两段加密模型的实证）；③A 记录临时指私网 IP 后公共递归（223.5.5.5 / 8.8.8.8 / 运营商默认）是否照常返回（bogon 风险，异常则启用 hosts/Split DNS 兜底并回填 spec）；④本机物理网卡网段与 10.126.126.0/24 冲突排查。完成标志：四项结论记录于本文件附注（不达标即触发升级路径评估，停实施）

## 阶段 2: Rust 核心（测试先行）

- [ ] T3 settings 扩展：`AccessChannel::Mesh` + `MeshConfig`（network_name/virtual_ip/virtual_cidr/peers 默认值）+ `tunnel_disabled`/`direct_disabled` 标记；旧 settings.json 兼容加载测试（样板：old_settings_file_without_channel_fields）（验收: AC11；完成标志：兼容与默认值单测通过）
- [ ] T4 `mesh.rs` 模块（依赖: T3；验收: AC9/AC11）：config.toml 渲染器（toml crate，唯一渲染路径，`[secure_mode] enabled=true` 模板硬编码无开关）+ 渲染产物断言单测 + 启动前校验（`--check-config` 调用 + 非 secure 配置拒绝，篡改模拟单测）+ 网段冲突检测纯函数（物理网卡 IPv4 × virtual_cidr 重叠判定）+ MeshConfig 校验（非空/IP/网段/peers URI 格式）
- [ ] T5 `dns_api.rs` 调和扩展：`RecordOp` 新增 `UpdateValue`（A 记录改值）与 `Delete`（CNAME 彻底删除——停用语义）；reconcile 单测覆盖 upsert/清理终态（验收: AC5/AC6/AC7 逻辑部分；完成标志：终态记录集断言通过）
- [ ] T6 通道编排扩展（依赖: T5；验收: AC1/AC5/AC6）：`switch_actions` 三通道矩阵（→mesh：StopFrpc→StopDdnsGo→渲染校验→服务重启→CNAME 删+A upsert 虚拟 IP→Persist；mesh→direct/tunnel 反向）；`disable_tunnel`/`disable_direct` 停用编排（前置校验非现役、CNAME 删除、disabled 标记持久化、A 记录按通道态处理）；mock 组件动作序列单测
- [ ] T7 服务管理（验收: AC3 前置）：`mesh-service.ps1`（install/uninstall/start/stop/restart/status；sc create delayed-auto + failure recovery restart/60000×3；UAC 自提权，install-https.ps1 惯例）+ Rust 侧栈目录落位（exe 复制 + manifest 哈希校验 + `<stack>/easytier/` 布局）+ 服务 binPath 构造纯函数（**断言参数不含 secret**——AC8）（完成标志：binPath 构造与落位单测通过；BOM+CRLF/Parser 校验）
- [ ] T8 状态探询与生命周期整合（依赖: T7；验收: AC3/AC4）：`easytier-cli peer --rpc 127.0.0.1:15888 -o json` 探询封装 + `mesh_state` 判定纯函数（service × peers → online/connecting/offline/not_configured + detail 不含 secret）+ 守护 tick 探询退化（IO 锁外纪律沿用）+ orchestrator 三处 bool 化通道判定重构为枚举匹配（is_tunnel_channel:371 / ChannelSource tunnel.rs:398 / start_all tunnel_mode:411）+ 自启/收摊整合（收摊走提权停服务）
- [ ] T9 Tauri 命令层（依赖: T4/T6/T8；验收: AC1/AC4/AC5）：`mesh_status`/`mesh_apply_config`/`mesh_install_service`/`mesh_uninstall_service`/`disable_legacy_channel`/`clear_frp_key` + `switch_channel` 扩展 mesh + `channel_health` 体检两项重定义（组网服务/对端、DNS 对齐→A=虚拟 IP）

## 阶段 3: 凭证脚本

- [ ] T10 `set-mesh-secret.ps1`（Read-Host -AsSecureString ×2 不回显、直写 `<stack>/easytier/network-secret`、复核仅显末 4 位，set-frp-key.ps1 惯例）+ `clear-frp-key.ps1`（从栈 `.env` 移除 SAKURA_FRP_KEY 行、复核显示已移除）+ **build.ps1 $ScriptSubset 登记两个新脚本**（006 坑：漏登记打包即删）（验收: AC8/AC10；完成标志：BOM+CRLF、Parser 校验、真机跑一遍脚本流程）

## 阶段 4: 前端

- [ ] T11 通道与设置 UI（依赖: T9；验收: AC1/AC4/AC5/AC6/AC11）：`types.ts` AccessChannel 加 "mesh" + MeshConfigView；`TunnelCard.tsx` 三通道单选 + 停用态（「已停用」chip + 重新启用安全警示确认）；`SettingsView.tsx` 组网设置卡（网络名/IP/网段/对端列表行内编辑即时校验，无密钥输入框仅脚本指引）；`api.ts` 对接新命令
- [ ] T12 i18n 双语同步（验收: AC4/AC11/AC12 呈现层）：zh.ts/en.ts 扁平键新增 `mesh.*`/`channel.disabled.*`/`wizard.channel.mesh*` 等，两侧同步无缺键
- [ ] T13 向导组网分支（依赖: T9/T10；验收: AC12）：`WizardView.tsx` channel 阶段组网分支（默认推荐）：装服务（UAC）→ 成员设备客户端指引（官方下载地址）→ set-mesh-secret.ps1 → apply + 在线校验 → A 记录 upsert + 解析校验；移除 SakuraFrp 分支；直连分支加安全警示；收尾页接停用入口（wizard.rs 检测步骤同步）

## 阶段 5: 验收与收尾

- [ ] T14 真机手工验收（依赖: 全部）：建 `acceptance-manual.md` 清单，逐 AC 验证（AC7 双通道口径：外部非成员探测不可达 + 成员访问正常；AC10 换钥吊销真机实测；AC2 换网重连；AC3 杀进程 SCM 自愈 ≤60s）
- [ ] T15 文档同步收尾：CHANGELOG Unreleased 登记、`.env.example` SAKURA_FRP_KEY 标注停用后可清除、MOC 状态流转、spec.md AC 勾选与状态 done、README 通道描述核对；对照 DoD 清单收尾

## 完成标志（DoD 检查）

- [ ] spec.md 中所有 AC 已逐条验证通过
- [ ] 自动化测试全部通过（含新增：渲染/校验/编排/调和/状态判定/binPath 无密钥断言）
- [ ] 相关文档已更新（plan/tasks/spec/MOC/CHANGELOG/调研报告事实修正）
- [ ] 本文件全部任务勾选完毕

---

## 附注：T2 前置实测结论（实施时回填）

- ①社区节点连通（移动网络 / WiFi）：
- ②Android legacy + secure-mode：
- ③bogon 公共解析：
- ④网段冲突排查：
