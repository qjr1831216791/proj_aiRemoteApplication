# 001-desktop-console · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 未开始 <!-- 未开始 | 进行中 | 已完成 -->
- **最后更新**: 2026-09-08

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成
> 测试纪律（宪法 §1）：T5~T12 逻辑类任务**先写失败测试再实现**；GUI 类走文末手工验收清单。

## 阶段 1: 骨架与地基

- [ ] T1 Tauri 2 工程脚手架：`create-tauri-app`（Vite + TS）接入 Preact；蓝白主题基础样式（设计令牌：色板/圆角/间距）；**zh/en 词条词典 + `t()` helper 骨架**（全 UI 文案自始双语，杜绝后期补翻）；窗口 `visible:false` 创建、页面就绪后 show；`npm run dev` 可出窗口（crates.io 访问慢则按 plan §7 配 rsproxy 镜像）（验收: 约束 §5）
- [ ] T2 单实例（`tauri-plugin-single-instance`，`Global\` 互斥体、**首个注册**）+ 托盘骨架（菜单占位：启动/停止/打开工作台/显示主界面/停止并退出/退出）+ `RunEvent::ExitRequested` 退出钩子挂接（依赖: T1）（验收: AC18 骨架）
- [ ] T3 里程碑 1 内存实测：托盘 + 隐藏主窗 + 真实空页，任务管理器记录"程序 + 全部 msedgewebview2 子进程"合计（启动后 5 分钟采样），基线记入本文件备注（验收: 约束 §5；完成标志: 数据落档且 ≤150MB，超限则先减依赖再继续）
- [ ] T4 文档回填：CLAUDE.md 目录树（src/ + src-tauri/ 职责）与"常用命令"（dev/build/test）、README 快速开始补桌面版入口、CHANGELOG Unreleased（验收: DoD 文档项）

## 阶段 2: 核心逻辑（测试先行）

- [ ] T5 `settings` 模块：Settings 结构、默认值、加载/补丁保存、损坏恢复（改名 `.bad-<ts>` + 回退默认 + `settings://repaired` 事件）——先写失败单测（验收: AC21/23/24 逻辑部分；完成标志: `cargo test settings` 绿）
- [ ] T6 `probe` 模块：`StatusProbe` trait + Windows 实现（netstat2 端口 Listen + sysinfo PID→exe 身份匹配；区分 running / port-held(进程名) / stopped；LAN 可达性仅按监听地址展示）——mock 单测（验收: AC5/7；完成标志: `cargo test probe` 绿）
- [ ] T7 `scripts` 模块：ScriptLocator（设置覆盖 → 内置副本 → 开发态仓库路径，缺失返回禁用原因）；CommandBuilder（`-Lang` 对齐、`-NoProfile -NonInteractive`、CREATE_NO_WINDOW、stdio→日志文件、超时）；Elevator（ShellExecuteW runas 可见窗口）；exit code→UI 语义映射——命令行字符串断言单测（验收: AC19/20 逻辑部分、plan §5.2；完成标志: `cargo test scripts` 绿）
- [ ] T8 `orchestrator` 启动侧：组件守卫（已运行跳过）、CloudCLI 走 `run-server-hidden.ps1`、Caddy/ddns-go 原生守卫拉起（stdout→日志）、starting→running 就绪轮询（2s）、60s 超时→failed 附 `%TEMP%\cloudcli.log` 尾部、在途操作跟踪、`status://changed` 事件——状态机单测（依赖: T5~T7）（验收: AC1/3/6 逻辑部分；完成标志: `cargo test orchestrator` 绿）

## 阶段 3: 停止与退出

- [ ] T9 停止管线：CloudCLI（定位 3001 监听 PID→身份校验→`taskkill /T` 树杀→复核）、Caddy（`caddy stop` 5s→443+路径校验兜底强杀→复核）、ddns-go（可执行路径匹配→强杀→复核）；单组件 10s 超时记日志放行；取消/等待在途启动——调用顺序断言单测（依赖: T6/T7）（验收: AC2、spec §4.4；完成标志: `cargo test stop` 绿）
- [ ] T10 退出流：托盘「退出」=按 `exitAction`（keep 直退 / stop 先收摊）；「停止服务并退出」=显式收摊（总超时 30s 放行）；关机/注销路径不做收摊——分支单测（依赖: T8/T9）（验收: AC13~17；完成标志: `cargo test exit_flow` 绿）

## 阶段 4: 自启体系

- [ ] T11 `autostart` 模块：服务自启开/关复用 `setup-autostart.ps1`（含 `-Remove`）+ 存量任务接管检测（返回 tookOver）；程序自身登录任务注册/移除（PowerShell `Register-ScheduledTask`、`--hidden`、ExecutionTimeLimit Zero、exe 取自身路径）——命令构造单测（依赖: T7）（验收: AC8/9/10；完成标志: `cargo test autostart` 绿）
- [ ] T12 登录联动：`--hidden` 启动判定（不弹主窗、不开浏览器），按 `linkStartServices` 执行补齐启动（依赖: T8/T11）（验收: AC11/12；完成标志: 单测 + 本机注销重登演练通过）

## 阶段 5: 前端界面

- [ ] T13 主界面：三组件状态卡（五态 + 端口/耗时/失败原因）、总开关（启动/停止，进行中禁用）、组件级「重试」、各端访问地址区（本机/局域网/域名，一键复制/打开）（依赖: T8）（验收: AC1/4/6 展示层）
- [ ] T14 设置页与语言体系：五项行为开关 + 语言设置（跟随系统/zh/en；切换**立即生效含托盘菜单重建**，并同步所有脚本 `-Lang`；`auto` 判定为纯函数——系统显示语言 zh→中文否则英文，单测覆盖）；端口/路径/域名只读卡（复制 + "修改须重跑安装脚本"指引）+ 打开日志目录（依赖: T5/T2/T7）（验收: AC21/22/25）
- [ ] T15 低频操作区：安装/重装、升级 CloudCLI(-Update)、HTTPS 栈装机（install-https）、HTTPS 环境配置、客户端配置（可见交互窗）、ddns-go 管理页、打开工作台页面；UAC 拒绝/脚本失败的明确提示（依赖: T7）（验收: AC19/20）

## 阶段 6: 验收与发布

- [ ] T16 执行文末"手工验收清单"并逐条记录结果（验收: 全部 GUI 类 AC）
- [ ] T17 一键构建与双形态分发产物：`scripts/build.ps1`（同步 `resources/bin` 脚本副本并校验与 app 版本对齐 → `tauri build`（NSIS，**embedBootstrapper**）→ 便携 zip（exe + resources，目录结构与安装版一致，附简要说明）→ 产物命名 `<app>_<版本>_<arch>`、体积记录入文末备注）；干净机验证三路径：安装器双击安装、便携包解压即用、**覆盖安装升级（设置保留）**；SmartScreen"仍要运行"引导写入安装说明（验收: 目标/约束 §5 打包分发项、plan §7 脚本耦合；完成标志: 双形态产物落档 + 三路径验证记录）
- [ ] T18 收尾：对照 [spec.md](./spec.md) 逐条勾选 AC；tasks 全勾；README/CHANGELOG/MOC 状态流转（in-progress → done）；CHANGELOG 发布节 + `vX.Y.Z` 标签准备

## 手工验收清单（GUI 类 AC，宪法 §1 窄例外的测试载体）

> 执行环境：开发机（Win11，sprint0 已部署）。每条执行后在本清单打勾并记异常。

- [ ] **AC1**：三组件全停 → 点「启动」→ 状态依次 starting→running（典型 ≤30s），地址区显示本机/局域网/域名；拔掉 cloudcli（临时改名模拟未装）再启动 → failed 且展示日志尾部与"未安装"提示
- [ ] **AC2**：全运行 → 点「停止」→ 三端口（3001/443/9876）释放（`netstat -ano | findstr` 验证）、状态全灰
- [ ] **AC3**：仅停 Caddy → 点「启动」→ 只补 Caddy，CloudCLI/ddns-go 无新进程（任务管理器对比 PID）
- [ ] **AC4**：界面开着 → 任务管理器强杀 ddns-go → ≤10s 内状态变"未运行"
- [ ] **AC6**：占住 443（临时起个监听）→ 启动 → Caddy 标红带原因、其余绿、总开关无"启动成功"字样 → 释放 443 → 点 Caddy「重试」转绿
- [ ] **AC7**：用无关进程占 3001 → 状态显示"端口被占（进程名 X）"而非"运行中"
- [ ] **AC10/11**：开启程序自启+联动 → 注销重登 → 托盘出现、无主窗、无浏览器、三组件运行
- [ ] **AC12**：关闭联动（仅程序自启）→ 重登 → 托盘出现、服务未被本程序拉起
- [ ] **AC8/9**：任务计划程序中查看三条 `* Sprint0 autostart` 被接管（同名重建）；程序内关闭 → 三任务消失、运行中进程不受影响
- [ ] **AC13**：默认设置 → 托盘「退出」→ 手机/另机仍可访问 `https://ai.jackqi.cn`
- [ ] **AC14**：托盘「停止服务并退出」→ 三组件停止后程序消失（构造一个停不掉的场景验证 30s 放行 + 日志）
- [ ] **AC15**：设置改"退出即停止" → 普通退出 → 服务全停
- [ ] **AC16**：服务运行中 → 任务管理器强杀本程序 → 服务仍在、手机端不断线 → 重启程序状态正确接管
- [ ] **AC17**：服务运行中 → 直接关机/注销 → 无"阻止关机"弹窗、无半停残留（重启后按 AC10 恢复）
- [ ] **AC18**：点主窗 X → 最小化到托盘，程序与服务均存活
- [ ] **AC19/20**：逐个点低频操作（装机/升级/install-https/enable-https/客户端配置/ddns-go 页/打开工作台）→ UAC 弹窗正常、install-https 结尾手工步骤提示完整可读；UAC 点"否" → 程序提示且不崩溃
- [ ] **AC21/22/25**：设置逐项修改 → 保存即生效、重启保持；**语言**：中文系统首启全中文（含托盘菜单）；切英文立即生效（界面 + 托盘菜单，无需重启）；显式选择后改系统语言不再跟随；删设置文件回"跟随系统"；日志抽查脚本输出语言与程序一致；端口/路径只读可复制
- [ ] **AC23/24**：删设置文件 → 默认值启动；写入损坏 JSON → 回退默认 + 提示 + `.bad-*` 留档
- [ ] **加固（托盘）**：任务管理器重启 explorer.exe → 托盘图标自动恢复；主窗隐藏时任务栏无残留按钮；`--hidden` 自启路径托盘图标正常

## 完成标志（DoD 检查）

- [ ] spec.md 中所有 AC 已逐条验证通过（自动化 + 手工清单双轨）
- [ ] 自动化测试全部通过（`cargo test` + 前端构建）
- [ ] 相关文档已更新（CLAUDE.md / README / CHANGELOG / MOC / ADR）
- [ ] 本文件全部任务勾选完毕

---

**备注（T3 内存基线）**：（执行时填写：日期 / 进程树合计 / 采样时点 / 结论）

**备注（T17 分发产物）**：（执行时填写：日期 / 双形态产物名与体积 / 三路径验证结论）
