# 001-desktop-console · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 进行中 <!-- 未开始 | 进行中 | 已完成 -->
- **最后更新**: 2026-09-09

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成
> 测试纪律（宪法 §1）：T5~T12 逻辑类任务**先写失败测试再实现**；GUI 类走文末手工验收清单。

## 阶段 1: 骨架与地基

- [x] T1 Tauri 2 工程脚手架：`create-tauri-app`（Vite + TS）接入 Preact；蓝白主题基础样式（设计令牌：色板/圆角/间距）；**zh/en 词条词典 + `t()` helper 骨架**（全 UI 文案自始双语，杜绝后期补翻）；窗口 `visible:false` 创建、页面就绪后 show；`npm run dev` 可出窗口（crates.io 访问慢则按 plan §7 配 rsproxy 镜像）（验收: 约束 §5）<!-- 2026-09-09 完成：cargo check 绿（1m28s）；tauri dev 实测出窗口；工程收拢于 apps/workbench/（多应用层） -->
- [x] T2 单实例（`tauri-plugin-single-instance`，`Global\` 互斥体、**首个注册**）+ 托盘骨架（菜单占位：启动/停止/打开工作台/显示主界面/停止并退出/退出）+ `RunEvent::ExitRequested` 退出钩子挂接（依赖: T1）（验收: AC18 骨架）<!-- 2026-09-09 完成：插件互斥体实测仅会话内（{id}-sim 无 Global 前缀），windows crate 补 Global\ 互斥体跨会话判定；cargo test 3/3 绿；dev 实测：关 X 隐藏到托盘进程存活（AC18）、第二实例秒退并激活主窗、托盘图标 UIA 可见、语言探测 Zh -->
- [x] T3 里程碑 1 内存实测：托盘 + 隐藏主窗 + 真实空页，任务管理器记录"程序 + 全部 msedgewebview2 子进程"合计（启动后 5 分钟采样），基线记入本文件备注（验收: 约束 §5；完成标志: 数据落档且 ≤150MB，超限则先减依赖再继续）<!-- 2026-09-09 完成：任务管理器口径（私有工作集合计）97.9MB ≤150MB 达标，明细见文末备注 -->
- [x] T4 文档回填：CLAUDE.md 目录树（src/ + src-tauri/ 职责）与"常用命令"（dev/build/test）、README 快速开始补桌面版入口、CHANGELOG Unreleased（验收: DoD 文档项）<!-- 2026-09-09 完成：CLAUDE.md 常用命令补齐 + 目录树标注技术栈；README 快速开始增桌面工作台入口（sprint0 内容未动）；CHANGELOG Unreleased 记 T1/T2 用户可感知变更；src/.gitkeep 已随实文件落地清理 -->

## 阶段 2: 核心逻辑（测试先行）

- [x] T5 `settings` 模块：Settings 结构、默认值、加载/补丁保存、损坏恢复（改名 `.bad-<ts>` + 回退默认 + `settings://repaired` 事件）——先写失败单测（验收: AC21/23/24 逻辑部分；完成标志: `cargo test settings` 绿）<!-- 2026-09-09 完成：红 10 failed（todo! 桩）→ 绿 13/13；camelCase schema + 三态 scriptsDirOverride（serde deserialize_with 区分 null/缺省）+ 原子写（.tmp-<pid> rename）+ 损坏留档 settings.json.bad-<ms>；get_settings/save_settings 命令 + lib.rs setup 加载与 settings://repaired 事件接线；注：spec.md AC24 写 .bak，plan §4/tasks 写 .bad-<ts>，按 plan 实现为 .bad-<ts>（冲突已记录） -->
- [x] T6 `probe` 模块：`StatusProbe` trait + Windows 实现（netstat2 端口 Listen + sysinfo PID→exe 身份匹配；区分 running / port-held(进程名) / stopped；LAN 可达性仅按监听地址展示）——mock 单测（验收: AC5/7；完成标志: `cargo test probe` 绿）<!-- 2026-09-09 完成：红 8 failed（todo! 桩）→ 绿 10/10（含真实监听集成：bind→netstat2 捕获→sysinfo 归 PID→drop 后 stopped）；classify 纯函数 + StatusProbe trait（WindowsProbe 采集层注入）；身份判据 cloudcli=node.exe 按名 / caddy、ddns-go=栈目录全路径（与 setup-autostart.ps1 Check 一致）；新增 consts.rs 实例常量（3001/443/9876、栈目录、域名、脚本契约路径） -->
- [x] T7 `scripts` 模块：ScriptLocator（设置覆盖 → 内置副本 → 开发态仓库路径，缺失返回禁用原因）；CommandBuilder（`-Lang` 对齐、`-NoProfile -NonInteractive`、CREATE_NO_WINDOW、stdio→日志文件、超时）；Elevator（ShellExecuteW runas 可见窗口）；exit code→UI 语义映射——命令行字符串断言单测（验收: AC19/20 逻辑部分、plan §5.2；完成标志: `cargo test scripts` 绿）<!-- 2026-09-09 完成：红 13 failed（todo! 桩）→ 绿 18/18（命令行字符串断言 + ProcessExecutor 真实冒烟：stdio 落日志/超时杀/派发即返）；ScriptLocator 以哨兵脚本 run-server-hidden.ps1 判目录有效（无效覆盖目录自动降级内置/开发态，全无效返回禁用原因）；隐藏脚本 = powershell -NoProfile -NonInteractive + CREATE_NO_WINDOW(0x08000000) + stdio→<stem>.log/.err.log + 每脚本超时（15s~1800s）；caddy/ddns-go 原生 CommandSpec 参数与 setup-autostart.ps1 任务定义逐字对齐；Elevator=ShellExecuteW runas + -NoExit 可见窗（交互脚本不加 -NonInteractive），UAC 拒绝→Err；exit 1 语义按脚本分派（cloudcli 不可用/端口仍被占/栈目录缺件），其余非零 Failed；CommandExecutor trait（T8 编排 mock seam） -->
- [x] T8 `orchestrator` 启动侧：组件守卫（已运行跳过）、CloudCLI 走 `run-server-hidden.ps1`、Caddy/ddns-go 原生守卫拉起（stdout→日志）、starting→running 就绪轮询（2s）、60s 超时→failed 附 `%TEMP%\cloudcli.log` 尾部、在途操作跟踪、`status://changed` 事件——状态机单测（依赖: T5~T7）（验收: AC1/3/6 逻辑部分；完成标志: `cargo test orchestrator` 绿）<!-- 2026-09-09 完成：红 14 failed（todo! 桩）→ 绿 17/17（全套 60/60）。orchestrator.rs：ComponentStatus/五态（serde 对齐 plan §4，port-held kebab-case）+ 守卫幂等（AC3）/exit 1→不可用提示（AC1）/超时→failed 附 cloudcli.log 或组件 .err.log 尾部（AC1）/port-held 不拉起（AC7）；InFlightTracker 互斥+取消句柄+wait_idle（Condvar，T10 消费）；StatusEventSink/LogTailReader trait 可 mock + TauriStatusEmitter/FsLogTailReader 真实实现；前台轮询器 FOREGROUND_POLL_INTERVAL=2s 可断言（AC4）。前置微调：StatusProbe/CommandExecutor trait 增 Send+Sync 超trait（跨线程持有，实现方无行为变化）。starting 置位于派发前（AC1"点击启动即启动中"，plan 时序图的"拉起后置 starting"按此细化，无冲突） -->

## 阶段 3: 停止与退出

- [x] T9 停止管线：CloudCLI（定位 3001 监听 PID→身份校验→`taskkill /T` 树杀→复核）、Caddy（`caddy stop` 5s→443+路径校验兜底强杀→复核）、ddns-go（可执行路径匹配→强杀→复核）；单组件 10s 超时记日志放行；取消/等待在途启动——调用顺序断言单测（依赖: T6/T7）（验收: AC2、spec §4.4；完成标志: `cargo test stop` 绿）<!-- 2026-09-09 完成：红 11 failed（todo! 桩）→ 绿 stop:: 12/12 + orchestrator 停止接入 3/3（`cargo test stop` 17/17，全套 75/75）。stop.rs：ProcessOps seam（descendants/pids_by_exe/kill；SysinfoProcessOps=sysinfo 枚举+TerminateProcess）；CloudCLI 先收集树快照再逐杀（子先父后，防监听者先死致 claude 会话子进程脱树成孤儿——spec §4.1 不直接复用 stop-server.ps1 的原因）；Caddy caddy stop(5s)→假停兜底按 443 找监听+校验可执行路径为 StackDir\caddy.exe 才强杀；ddns-go 按可执行全路径匹配（非进程名）强杀；单组件 10s 预算超时→TimedOut+手动排查命令放行（AC2），复核失败→Failed 附原因；grace 500ms 后单次复核（对齐 stop-server.ps1）。orchestrator.stop_one/stop_all：先 cancel+wait_idle 在途启动（§4.4 竞态消除，取消后启动线程不再迁移状态）再走管线，三组件并行。真机集成后补（任务书称"真实服务运行中"与实况不符——3001/443/9876 实测均无监听，安全窗口内执行）：CloudCLI+Caddy 真实启停全链路通过（脚本 exit 0→探测就绪→树杀/优雅停→双端口释放）；借此发现并修复 T6 probe 真机 bug：caddy 双栈监听（0.0.0.0:443 + [::]:443）令端口表给出重复 PID，sysinfo 0.33 `Some(pids)`+remove_dead_processes=true 对重复项逐一 switch_updated 会误删进程 → 身份富集全空 → caddy 永远误判 port-held("unknown")；`exe_of_pids` 传参前去重修复 + 回归单测（probe 10→11，全套 76/76）。ddns-go 刻意跳过（涉外部 DNS API 副作用）；现场恢复全停原状 -->
- [x] T10 退出流：托盘「退出」=按 `exitAction`（keep 直退 / stop 先收摊）；「停止服务并退出」=显式收摊（总超时 30s 放行）；关机/注销路径不做收摊——分支单测（依赖: T8/T9）（验收: AC13~17；完成标志: `cargo test exit_flow` 绿）<!-- 2026-09-09 完成：红 6 failed（todo! 桩）→ 绿 exit_flow 7/7（全套 84/84）。exit_flow.rs：ExitGate 意图门（one-shot 取走防重入）+ decide_semantics 三分支纯函数（显式 stop_services=true 无视 keep）+ run_shutdown 总超时 30s 硬顶（工作线程 join_timeout，超时放行不等完成）+ handle_exit_requested 只认标志（无意图零收摊，AC17）。托盘两退出菜单/quit 命令统一走 request_exit（先置意图后 app.exit）。lib.rs 装配：Orchestrator（WindowsProbe/ProcessExecutor/SysinfoProcessOps/TauriStatusEmitter）+ 前台轮询器 + ExitRequested 钩子收摊；语言改经 settings 显式选择优先（lang::resolve_setting） -->

## 阶段 4: 自启体系

- [x] T11 `autostart` 模块：服务自启开/关复用 `setup-autostart.ps1`（含 `-Remove`）+ 存量任务接管检测（返回 tookOver）；程序自身登录任务注册/移除（PowerShell `Register-ScheduledTask`、`--hidden`、ExecutionTimeLimit Zero、exe 取自身路径）——命令构造单测（依赖: T7）（验收: AC8/9/10；完成标志: `cargo test autostart` 绿）<!-- 2026-09-09 完成：红 10 failed（todo! 桩）→ 绿 autostart 16/16（全套 97/97 + 1 ignored）。autostart.rs：任务名常量与 setup-autostart.ps1 逐字对齐；接管检测 = 开启前 Get-ScheduledTask 查三条存在性（任一存在 → tookOver，单条查询失败不阻断）；开关统一走 setup-autostart.ps1（幂等重建/-Remove，exit 1→栈目录缺件语义）；程序自身任务 Register-ScheduledTask（try/catch 显式 exit code、登录触发 $env:USERNAME、--hidden、ExecutionTimeLimit Zero、exe=current_exe、幂等重建=接管同语义）；全部经 CommandExecutor seam + CREATE_NO_WINDOW。setup-autostart.ps1 经读源确认**不自提权**（无 UAC 预期）。真机演练（--ignored roundtrip）：三条 Sprint0 任务只读验证存在→注册自身任务→schtasks 确认→移除→确认消失，Sprint0 三任务原状未动 -->
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

**备注（T3 内存基线）**：2026-09-09 · release 版（Tauri 2 + WebView2）实测。形态：托盘常驻 + 主窗隐藏到托盘（AC18 形态）+ T1 骨架空页；09:59:12 启动，10:06~10:09 采样（启动后 7~9 分钟，满足"5 分钟后"要求）。进程树 7 进程（workbench.exe + 6 个 msedgewebview2）：
- **判定口径（任务管理器"内存"列 = 私有工作集，逐进程求和）：97.9 MB ≤ 150MB，达标**
- 参考口径（同树同刻）：私有字节合计 158.9 MB；全工作集 WorkingSet64 求和 378.9 MB（WebView2 各进程大量共享页被重复计入，非任务管理器展示值，仅作上限参考）
- 进程明细（私有工作集）：workbench 2.7 · wv2 31.2 / 34.4 / 19.0 / 5.4 / 3.3 / 1.8 MB
- T13~T15 界面与业务接入后建议复核一次；若届时超限，按"先减依赖再继续"处理

**备注（T17 分发产物）**：（执行时填写：日期 / 双形态产物名与体积 / 三路径验证结论）
