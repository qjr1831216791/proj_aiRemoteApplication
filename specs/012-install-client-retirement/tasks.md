# 012-install-client-retirement · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 已完成
- **最后更新**: 2026-09-13

> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成

- [x] T1 代码层摘除（前端按钮/i18n/types + Rust 枚举映射与测试 + commands 注释），`cargo test` 与 `npm run build` 全绿（验收: AC1/AC2）→ 253 测试绿、构建通过、引用 InstallClient 的 6 处测试改锚 SetTencentKey/SetMeshSecret/SetHttpsAccount
- [x] T2 脚本与打包层摘除（menu.ps1 菜单项 + build.ps1 $ScriptSubset + 删 install-client.ps1/.bat 与 resources 副本 + manifest 条目），menu.ps1 语法解析检查通过（验收: AC2）→ 菜单 6/7 号位前移为自启开/关，Parser::ParseFile 通过
- [x] T3 文档层：tools 两份 README 改写、011 开放问题回填、CHANGELOG Unreleased 登记、MOC 登记（验收: AC3）
- [x] T4 对照 spec 逐条 AC 验证勾选，spec 状态 → done（验收: DoD）

## 完成标志（DoD 检查）

- [x] spec.md 中所有 AC 已逐条验证通过
- [x] 自动化测试全部通过
- [x] 相关文档已更新
- [x] 本文件全部任务勾选完毕
