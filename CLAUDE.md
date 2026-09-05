# CLAUDE.md

GitHub: https://github.com/qjr1831216791/proj_aiRemoteApplication.git

## 项目概述

AI 远程应用（proj_aiRemoteApplication）。采用 **Spec 驱动的敏捷开发（Spec-Driven Development）**：先写规格再写代码，`specs/` 是需求与实现的事实来源（Single Source of Truth）。

## 开发铁律

1. **无 Spec 不编码**：新功能必须先有 `specs/NNN-<功能名>/spec.md` 并确认后才能实现。
   例外：bug 修复、不改变外部行为的重构、CI/配置调整 —— 提交信息中说明为何免 Spec。
2. **Spec 是唯一事实来源**：代码与 spec 冲突时，要么改代码，要么走变更流程改 spec，不允许默默偏离。
3. **文档不烂尾**：实现与 spec / plan / tasks 三文档保持同步；Spec 新建与状态变更必须同步更新 `specs/MOC.md`。

## Spec 工作流（每个功能固定六步）

```
澄清 → ① spec.md 需求 → ② plan.md 方案 → ③ tasks.md 拆解 → ④ 逐任务实现(测试先行) → ⑤ 对照验收 → ⑥ 归档
```

1. **澄清**：需求不清先提问；从 `specs/_templates/` 复制模板建 `specs/NNN-<功能名>/`（三位递增编号，不复用）。
2. **需求 spec.md**：用户故事 + 可测试的验收标准（Given/When/Then）+ 明确的非目标；确认后状态 `draft → reviewed`。
3. **方案 plan.md**：技术选型、架构、数据模型、接口契约、风险；影响全局的选型另立 `docs/adr/`。
4. **拆解 tasks.md**：拆为一天内可完成、有完成标志的任务，标注依赖，并可追溯到 AC 编号。
5. **实现**：按任务逐个完成，测试先行；每完成一个任务立即勾选 tasks.md，提交注明 `specs/NNN-xxx T<编号>`。
6. **验收归档**：对照 spec.md 逐条验证 AC，全过后状态 `done`；下一迭代开始前目录移入 `archive/`。

Spec 状态机：`draft → reviewed → in-progress → done → archived`（以各 spec.md 顶部状态字段为准）。

## 敏捷规范

- **迭代（Sprint）**：1~2 周一个迭代；迭代开始时从 backlog 选定本轮 Spec 并登记到 `specs/MOC.md`，迭代中途不插入新需求。
- **用户故事**：`作为<角色>，我希望<能力>，以便<价值>`。
- **验收标准**：`Given <前置> When <操作> Then <结果>`，必须黑盒可验证。
- **需求变更**：实现中改需求，先更新 spec.md 并在文末"变更记录"追加一行，再改代码。
- **完成的定义（DoD）**：AC 全部验证通过 + 测试通过 + 文档更新 + tasks.md 全部勾选（清单见 tasks 模板末尾）。

## 目录结构

```
proj_aiRemoteApplication/
├── README.md              # 项目门面：文档地图与快速开始
├── CLAUDE.md              # 本文件：AI 协作开发规范
├── CHANGELOG.md           # 版本说明与变更历史（Keep a Changelog + SemVer）
├── .env.example           # 环境变量模板（与 .env 同位，真实 .env 不入库）
├── .gitattributes         # 行尾统一（仓库内 LF，Windows 脚本保留 CRLF）
├── .editorconfig          # 跨 IDE 编辑行为基线
├── docs/                  # 跨 Spec 的长期文档
│   ├── product.md         # 产品愿景：为什么做
│   ├── constitution.md    # 开发宪法：跨 Spec 的不变约束，冲突时以此为准
│   ├── research/          # 调研报告（立项前的需求与技术调研）
│   └── adr/               # 架构决策记录（含模板与索引）
├── specs/                 # ★ Spec 驱动开发核心
│   ├── MOC.md             # 全部 Spec 的导航地图（状态/迭代/主题）
│   ├── README.md          # 工作流细则与快速索引
│   ├── _templates/        # spec / plan / tasks 模板
│   ├── NNN-<功能名>/      # 每个功能一个目录：spec.md + plan.md + tasks.md
│   └── archive/           # 已验收 Spec 的归档
├── config/                # 环境配置：非敏感默认配置（模板 .env.example 在仓库根目录）
├── src/                   # 源码（技术栈确定后细化）
├── tests/                 # 测试
├── scripts/               # 生命周期命令（dev/build/test/deploy，日常入口）
└── tools/                 # 开发辅助工具脚本（一次性任务，与生命周期脚本分离）
```

## 会话启动约定

- 每次开发会话先读：`docs/constitution.md` → `specs/MOC.md` → 当前 `in-progress` 的 Spec 三文档，再动手。
- 新功能开发一律从模板起步，不要凭空自建文档结构。

## 开发约定

- 使用中文交流；代码与标识符用英文。
- 提交信息用简洁的英文或中文，说明"为什么"而非"改了什么"，并关联 Spec 编号。
- 敏感信息（密钥、token）不要提交：真实值放 `.env`（已被 .gitignore 忽略），变量名与用途必须登记到根目录 `.env.example`。
- **结构描述必须维护**：目录增删或职责调整时，同步更新本节目录树与对应目录的 README。
- 版本遵循 SemVer（当前 0.x 阶段）：用户可感知的变更记入 `CHANGELOG.md` 的 Unreleased 区；发布时改为 `版本号 - 日期`、声明交付的 Spec 并打 `vX.Y.Z` 标签（步骤见 CHANGELOG）。
- 技术栈、构建/测试命令在首个 plan.md 确定后，回填到下方"常用命令"。

## 常用命令

待补充（项目初始化后记录构建、测试、运行命令）。
