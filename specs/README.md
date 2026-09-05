# Specs — Spec 驱动开发工作区

本目录是项目需求与实现的**事实来源（Single Source of Truth）**。
工作流总览见根目录 [CLAUDE.md](../CLAUDE.md)，导航入口是 [MOC.md](./MOC.md)。

## 每个 Spec 的构成

```
specs/NNN-<功能名（kebab-case）>/
├── spec.md    # 需求规格：WHAT & WHY（用户故事 + 验收标准 + 非目标）
├── plan.md    # 技术方案：HOW（选型、架构、数据模型、接口契约）
└── tasks.md   # 任务拆解：可勾选、可追溯的小任务清单
```

## 新建一个 Spec 的步骤

1. 复制 `_templates/` 下三个模板到新目录 `NNN-<功能名>/`（NNN 为三位递增编号，不复用）。
2. 填写 `spec.md` → 与需求方确认（状态改为 `reviewed`）。
3. 填写 `plan.md`，据此拆解 `tasks.md`。
4. 按 tasks.md 逐任务实现（测试先行），每完成一个任务立即勾选。
5. 对照 spec.md 验收标准逐条验证，全部通过后状态改 `done`。
6. 下一个迭代开始前，将目录移入 `archive/`，并在 MOC 中更新链接。

## 规则

- **无 Spec 不编码**：新功能必须先有 `spec.md` 并确认。例外（bug 修复、不改外部行为的重构、CI/配置）需在提交信息说明理由。
- **变更先改文档**：实现中修改需求，必须先更新 `spec.md` 并在文末"变更记录"追加一行，再改代码。
- **状态唯一来源**：以各 `spec.md` 顶部的 `状态` 字段为准；`draft → reviewed → in-progress → done → archived`。
- **文档不烂尾**：spec / plan / tasks 与实现保持同步，代码与文档冲突时二者必改其一。

## Spec 索引

唯一入口是 [MOC.md](./MOC.md)。本文件**不维护第二份索引**，避免双处更新导致漂移。
