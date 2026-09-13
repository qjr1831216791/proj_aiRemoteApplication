# 012-install-client-retirement · 需求规格（spec）

> 导航：[plan.md](./plan.md) · [tasks.md](./tasks.md) · 返回 [MOC](../MOC.md)

- **状态**: done <!-- draft | reviewed | in-progress | done | archived -->
- **迭代**: Sprint 9（随 011 同迭代，见变更记录）
- **创建日期**: 2026-09-13
- **最后更新**: 2026-09-13（当日完成摘除并验证）

## 1. 背景与问题

运维工具折叠区的「客户端配置」按钮派发 `install-client.ps1`（局域网时代产物）：验证服务端 TCP/HTTP 可达 + 桌面创建 `.url` 快捷方式。架构演进后其价值归零：

- spec 010 已将 3001 直访**默认退役**（防火墙拦死，仅 12h 例外），脚本默认端口形态指向一个默认被拦的地址；
- 现行成员接入路径是 EasyTier 组网 + `https://ai.jackqi.cn`，浏览器直接输网址即可，「连通性验证 + 快捷方式」属可有可无的糖；
- 按钮与脚本继续存在只会误导成员设备走向错误通道。

该退役项在 spec 011 立项时已列为候选（当时裁定顺延编号）。需求方 2026-09-13 指示：「运维工具-客户端配置：如果这个功能没有价值则移除」→ 评估确认无保留价值 → 「移除」。

## 2. 目标与非目标

### 目标（Goals）

- 工作台运维工具区移除「客户端配置」入口及其派发链路（前端按钮 → ToolKind/Script 枚举 → 打包登记）。
- 脚本本体（`install-client.ps1` / `install-client.bat`）与 resources 打包副本从仓库移除。
- 文档同步：tools 两份 README 的客户端指引改写为现行路径（EasyTier 组网 + 浏览器直达 https 域名）。

### 非目标（Non-Goals · 本期明确不做）

- **不动成员入网 TOML 配置展示**（spec 009 交付物，与 install-client 无关，继续保留）。
- **不改 install-client 之外任何 sprint0 脚本**；menu.ps1 仅摘除对应菜单项，其余选项与编号顺延逻辑保持简单直接。
- **不追溯修改历史文档**（spec 001/010、调研报告、CHANGELOG 已发布版本节中的 install-client 记载属历史事实，保留）。

## 3. 用户故事与验收标准

### US1: 作为需求方，我希望退役无价值的客户端配置功能，以便工具区不保留误导性入口

- [x] **AC1**: Given 打开工作台运维工具折叠区 When 查看 Then 不再出现「客户端配置」按钮，其余工具按钮功能不变（前端构建通过 ✅；界面走查随 011 T7 真机轮次一并核对）
- [x] **AC2**: Given 全仓检索 `install-client` / `install_client` / `InstallClient` When 检查 Then 代码与打包链路（src-tauri/scripts/build/manifest/resources）零残留 ✅；脚本文件 `install-client.ps1`/`.bat` 及其 resources 副本已删除 ✅（`cargo test` 253 全绿 + 检索断言 ✅）
- [x] **AC3**: Given tools/README.md 与 tools/sprint0/README.md When 阅读客户端相关章节 Then 指引为现行路径（EasyTier 组网后浏览器访问 `https://<域名>`）✅，不再出现 install-client 操作步骤 ✅

## 4. 约束与假设

- 需求方明确指示移除，本 spec 直接以 reviewed 立项（变更记录）。
- 迭代惯例「中途不插新需求」在此由需求方本人拍板豁免——功能微小且属既有候选清单项，与 011 同 Sprint 9 交付。

## 5. 变更记录

| 日期 | 变更内容 | 原因 |
|------|----------|------|
| 2026-09-13 | 初稿即以 reviewed 立项 | 需求方当日两次指示：「如果这个功能没有价值则移除」→ 评估反馈无价值 →「移除 运维工具『客户端配置』」 |
| 2026-09-13 | 状态 reviewed → done：T1~T4 当日完成——代码/脚本/打包/文档四层摘除，cargo test 253 绿 + npm build 通过 + menu.ps1 解析通过 + 全仓零残留；AC1 界面走查随 011 T7 真机轮次核对 | 需求方拍板即实施，同日闭环 |
