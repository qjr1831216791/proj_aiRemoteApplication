# 0004-settings-channel-migration

- 状态: accepted
- 日期: 2026-09-11
- 关联: specs/008-legacy-channel-removal/spec.md · specs/008-legacy-channel-removal/plan.md §4 · specs/007-mesh-access/spec.md

## 背景与问题

访问通道自 spec 004 起是三值契约：`settings.accessChannel = "direct" | "tunnel" | "mesh"`，随行携带 `tunnel`（tunnelId/nodeDomain）、`tunnelEnabled`、`tunnelDisabled`、`directDisabled` 等伴生字段。007 引入组网通道后三通道并存；008 立项将直连（ddns-go）与穿透（frp/SakuraFrp）整体退役，收敛为「局域网 IP 直访 + EasyTier 组网」双方案。

存量装机（真机与既有分发副本）的设置文件里写着 `direct`/`tunnel`，且停用标记可能为 true。若直接把枚举删成单值并启用 `deny_unknown_fields`，旧文件反序列化会失败——按 AC24 的既有行为会触发「设置损坏恢复默认」，用户全部行为设置（自启/退出行为/语言等）被无差别抹掉，这不可接受。

## 决策

`AccessChannel` 收敛为单变体 `Mesh`，**加载期静默迁移**：自定义 `Deserialize` 把 `direct`/`tunnel` 一律映射为 `mesh`，未知值报错（不静默吞）——后者沿用「损坏恢复默认」路径。设置结构删除全部通道伴生字段（tunnel 配置/启停/停用标记），serde 不开 `deny_unknown_fields`：旧文件里的 `tunnel`/`tunnelDisabled`/`directDisabled` 同名键与向导状态文件的 `branch` 键被**忽略**（留在磁盘、不进内存、下次保存自然消失）。工作台侧同一契约：前端 `AccessChannel = "mesh"`。

## 理由

- 迁移方向唯一且无损价值：两条旧通道已无运行时承载（组件/命令/UI 全删），`direct`/`tunnel` 的唯一正确去向就是 `mesh`；不存在需要保留判别信息的回切场景（回切=走 git 历史）。
- 伴生字段不迁移：`tunnel.tunnelId` 等在 008 后无任何消费方，迁移它们等于搬运死数据；`SAKURA_FRP_KEY` 的清理由 `uninstall-legacy.ps1` 承担（它读的是栈目录 `.env`，与设置文件无关）。
- 放弃的备选（枚举保三值 + 运行时禁用两值）：保住了"反向兼容"却要拖着停用判定/文案/UI 分支走——这正是 008 要消灭的复杂度，且给用户留出"重新启用已退役通道"的歧义入口。
- 放弃的备选（`deny_unknown_fields` + 显式版本迁移脚本）：设置文件已有版本字段但历史上从未写过迁移器；为一个一次性收敛引入迁移框架不成比例，忽略未知键即可达成同样净效果。

## 后果

- 收益：契约单值化后，通道判别逻辑（Rust `current_channel`/`channel_source`、前端三通道文案选择函数、切换确认/停用/重新启用 UI）全部可删，不留禁用分支的永久维护税。
- 代价（接受）：旧文件里的退役字段在磁盘上残留到下一次保存设置才被清掉——只影响文件可读性，不影响行为。
- 代价（接受）：`direct`→`mesh` 迁移后 DNS 的 A 记录仍指向旧出口 IP（而非虚拟 IP），需用户在工作台点「同步 DNS」或跑 `uninstall-legacy.ps1`（后者含 CNAME 残留检测）；spec 008 以 AC8 的对齐检测 + MeshCard 常态轮询指引承接，不在加载期自动改 DNS（迁移保持本地、无副作用）。
- 对后续的影响：`AccessChannel` 字段在类型上仍存在（单值枚举）而非彻底删除——保留它是为了设置文件结构的稳定与向前可读（未来若再增通道，判别字段位还在）；真机执行卸载编排受前置闸门约束（007 T14 签收 + O1 闭合，见 008 tasks T20）。

## 契约变更

- `settings.json`：`accessChannel` 合法值仅 `mesh`；`direct`/`tunnel` 加载时映射为 `mesh`，其他值报错走损坏恢复。`tunnel`/`tunnelEnabled`/`tunnelDisabled`/`directDisabled` 键不再写出，读到时忽略。
- 向导状态文件：`branch` 键不再写出，读到时忽略（向导通道阶段已塌缩为组网单分支）。
- IPC 契约：`switch_channel`/`disable_legacy_channel` 等九命令删除（见 008 plan §5）；`DnsAlignment` 五变体中 `alignedMesh` 为唯一对齐态、`MismatchedCname` 判旁路暴露面。
