# 001-desktop-console · 手工验收操作手册（T16 执行用）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · [tasks.md](./tasks.md) · 返回 [MOC](../MOC.md)
>
> **用途**：tasks.md 文末"手工验收清单"的可执行展开（宪法 §1 窄例外：GUI 交互类 AC 以手工清单覆盖）。
> 每条含 **前置 / 操作 / 预期 / 证据记录**；标注 **自动化背书**（单测名 / 真机日志，可免重复验证的口径）与 **纯人工**（必须人手人眼的部分：UAC 点击、注销重登、手机访问、视觉与语言切换核验）。
> 全部执行完并记录后，回到 [tasks.md](./tasks.md) 勾选 T16 与文末清单。
> §4 为 T18 汇编的 **AC 逐条验证对照表**（自动化 / 真机 / 剩余手工三列证据）——手工项全部通过后据其翻 done。

## 0. 环境与准备

- **执行机**：开发机（Win11，sprint0 已部署：`D:\Software\cloudcli-https`、cloudcli 已装、三条 `* Sprint0 autostart` 计划任务存在）。
- **程序**：安装版（NSIS 安装的 `ai-remote-workbench.exe`）或便携版均可；建议用 T17 产物（`release/` 目录）走一遍安装。
- **常用核查命令**（黑盒口径，与程序同一判定语义）：
  - 端口：`netstat -ano | findstr ":3001 :443 :9876"`（LISTENING = 在跑）
  - 计划任务：`schtasks /query | findstr Sprint0`（三条服务任务）
  - 程序日志：`%LOCALAPPDATA%\cn.jackqi.workbench\logs\AI-Remote-Workbench.log`
  - 设置文件：`%APPDATA%\ai-remote-workbench\settings.json`
- **建议顺序**：AC23/24 → AC1 → AC3 → AC4 → AC7 → AC6 → AC2 → AC5（并入 AC1 场景）→ AC8/9 → AC10/11 → AC12 → AC13~AC18 → AC19/20 → AC21/22/25 → 加固（托盘）。可按场景合并，但每条证据独立记录。
- **自动化基线**（先跑一遍确认绿，再做手工项）：
  - `cargo test`（apps/workbench/src-tauri）：**125 passed + 1 ignored**（2026-09-09 复跑实证；模块分组：orchestrator 22 / scripts 19 / autostart 15+1 ignored / stop 14 / settings 13 / lang 12 / probe 11 / exit_flow 7 / urls 5 / startup 5 / commands 3）
  - `npm run build`（apps/workbench）：零错误（2026-09-09 实证）

## 1. 总览：自动化背书与纯人工分布

| 清单项 | 逻辑层自动化背书 | 真机已有证据 | 纯人工部分 |
|---|---|---|---|
| AC1 启动 | `cargo test orchestrator`（守卫/轮询/60s 超时→failed+日志尾部/exit 1 映射） | T13：联动派发后 3001/443/9876 三端口 LISTENING | 点「启动」计时、看界面文案 |
| AC2 停止 | `cargo test stop`（树杀顺序/caddy 兜底/超时放行/复核） | T9：CloudCLI+Caddy 真实启停全链路、双端口释放 | 点「停止」、netstat 复核 |
| AC3 幂等 | `cargo test orchestrator`（已运行跳过） | T12：已在运行的 caddy/ddns-go 跳过 | 任务管理器 PID 对比 |
| AC4 轮询 | 轮询间隔常量断言（前台 2s ≤ 5s） | — | 强杀组件计时观察 |
| AC5 外部通道 | `cargo test probe`（端口+身份双口径） | T9/T13（脚本/外部启动的进程被如实识别） | start-server.bat 场景复演 |
| AC6 部分失败 | `cargo test orchestrator`（部分成功不汇总成功） | — | 占 443 场景 + 界面观察 |
| AC7 端口被占 | `cargo test probe`（port-held + 进程名） | — | 界面告警色核验 |
| AC8/9 自启托管 | `cargo test autostart`（命令构造对齐脚本语义） | T11：roundtrip 真机（注册/移除自身任务、Sprint0 三任务原状） | 任务计划程序 UI 核对、程序内开关 |
| AC10/11 自启+联动 | `cargo test startup`（--hidden/StartupPlan 正交） | T12：--hidden 真机（无主窗/托盘在/进程存活） | **注销重登** |
| AC12 关联动 | `cargo test startup`（零派发分支） | — | **注销重登** |
| AC13 退出保留 | `cargo test exit_flow`（keep 分支） | T2：关 X 存活同语义 | **手机/另机访问** |
| AC14 收摊退出 | `cargo test exit_flow`（30s 总超时放行） | — | 托盘点击、构造停不掉场景 |
| AC15 退出即停止 | `cargo test exit_flow`（decide_semantics） | — | 设置切换 + 退出 |
| AC16 强杀程序 | 设计保证（组件不挂 kill-on-close Job） | 2026-09-09 进程级验证（见 tasks.md T16 注记） | 界面接管状态核验 |
| AC17 关机不收摊 | `cargo test exit_flow`（无意图零收摊） | — | **关机/注销实弹** |
| AC18 关 X 到托盘 | — | T2：真机已证（关 X 隐藏、进程存活、托盘 UIA 可见） | 复核一次即可 |
| AC19/20 低频操作 | `cargo test scripts`（-Lang/runas/可见窗/超时/退出码映射） | T15：折叠区渲染核验 | **UAC 实弹点击、结尾提示可读性** |
| AC21/22/25 设置/语言 | `cargo test settings` + `cargo test lang`（auto 纯函数/即时切换含托盘重建） | T14：损坏恢复真机实证（12:29 留档回退） | **视觉/语言切换人眼核验** |
| AC23/24 默认/损坏 | `cargo test settings`（13 项，含 .bad-* 留档） | T14：真机实证 | 删文件复演一次 |
| 加固（托盘） | tray-icon TaskbarCreated 重挂（plan §7） | 2026-09-09 explorer 重启尝试（见 T16 注记） | 托盘图标可见性人眼 |

## 2. 逐条执行步骤

### AC1 一键启动（轮询至就绪）

- **前置**：三组件全停（`netstat` 三端口无 LISTENING）；主界面打开。
- **操作**：点总开关「启动」，秒表计时。
- **预期**：状态依次 蓝（启动中）→ 绿（运行中），典型 ≤30s；地址区显示 本机 / 局域网 IP / https://ai.jackqi.cn。再演失败分支：临时把 `cloudcli` 移出 PATH（或改名 `%APPDATA%\npm\cloudcli.cmd`）→ 点「启动」→ CloudCLI 卡标红，展示 `%TEMP%\cloudcli.log` 尾部与"未安装/不可用"提示，其余组件不受影响 → 还原。
- **证据记录**：耗时 ____s；失败分支提示文案：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test orchestrator` 全绿（状态机/超时/exit 1→"cloudcli 不可用"映射均已断言）；T13 真机联动实启三端口 LISTENING。手工仅为界面观察。

### AC2 一键停止（端口全释放）

- **前置**：三组件运行中（AC1 完成态）。
- **操作**：点「停止」；完成后执行 `netstat -ano | findstr ":3001 :443 :9876"`。
- **预期**：逐组件停止（含 CloudCLI 进程树），三端口均无 LISTENING，状态全灰；单组件卡住时 10s 超时放行并给出排查命令，不阻塞其余组件。
- **证据记录**：netstat 输出（粘贴）：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test stop` 全绿；T9 真机已证 CloudCLI/Caddy 全链路停止与端口释放。

### AC3 幂等补齐（不重复拉起）

- **前置**：仅 Caddy 停止（`taskkill` 掉 443 监听进程），CloudCLI/ddns-go 运行中；记录当前 PID。
- **操作**：点「启动」。
- **预期**：只补 Caddy；CloudCLI/ddns-go 进程 PID 不变（任务管理器对比），无新 node/caddy/ddns-go 进程。
- **证据记录**：前后 PID 对照：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：守卫幂等为 `cargo test orchestrator` 断言项；T12 真机已演（已在运行的组件跳过）。

### AC4 外部停止感知（≤10s）

- **前置**：三组件运行中，主界面开着。
- **操作**：任务管理器强杀 `ddns-go.exe`，计时观察状态卡。
- **预期**：≤10s 内 ddns-go 卡转灰（"未运行"），无需手动刷新；其余组件不变。
- **证据记录**：感知耗时 ____s；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：前台轮询间隔 2s（常量断言 ≤5s）；探测口径 `cargo test probe`。

### AC5 外部通道启动的接管

- **前置**：三组件全停。经外部通道启动：`tools/sprint0/bin/start-server.bat`（或触发一条 Sprint0 计划任务）。
- **操作**：启动本程序。
- **预期**：三组件如实显示"运行中"（不是"启动中"也不重拉）；点「停止」可正常接管停止。
- **证据记录**：初始状态显示：____________；停止后 netstat：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test probe`（外部进程端口+身份判定）；AC2 背书同用于接管停止。

### AC6 部分启动失败 + 组件重试

- **前置**：三组件全停；用无关进程占住 443（如临时 `python -m http.server 443` 或改用 9876）。
- **操作**：点「启动」；观察总开关与 Caddy 卡；释放 443 后点 Caddy 卡上「重试」。
- **预期**：CloudCLI/ddns-go 转绿，Caddy 标红带原因（端口被占/失败），总开关不出现"启动成功"类文案；重试后 Caddy 转绿。
- **证据记录**：失败原因文案：____________；重试结果：____；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test orchestrator`（部分成功不汇总、重试仅 failed/port-held 可用）。

### AC7 端口被占独立状态（双口径）

- **前置**：三组件全停；用无关进程占 3001（记下进程名，如 python.exe）。
- **操作**：打开主界面观察 CloudCLI 卡。
- **预期**：显示独立的"端口被占（进程名 python.exe）"告警色状态，**不**显示"运行中"；不产生重复进程。
- **证据记录**：状态文案：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test probe`（port-held + 监听进程名富集）。

### AC8/9 服务自启托管（接管/移除）

- **前置**：设置页「服务开机自启」当前为开。
- **操作**：① 打开 `taskschd.msc`（任务计划程序）确认三条 `* Sprint0 autostart` 任务存在；② 程序内把开关拨到 **关** → `schtasks /query | findstr Sprint0`；③ 再拨回 **开**（此时若任务曾被外部改动，应重建即"接管"，首次会 toast 提示接管）。
- **预期**：关 → 三任务消失、正在运行的三组件**不受影响**（端口仍 LISTENING）；开 → 三任务回归（幂等重建，无重复任务）。
- **证据记录**：开关前后 schtasks 输出：____________；组件是否受影响：____；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test autostart`（命令构造与 setup-autostart.ps1 逐字对齐、-Remove、接管检测）；T11 真机 roundtrip 已证注册/移除机制（含 Sprint0 三任务原状未动）。

### AC10/11 程序自启 + 联动补齐（注销重登）【纯人工】

- **前置**：设置「程序开机自启」开 +「启动时联动补齐服务」开；当前三组件全停（先关掉服务自启任务或手动全停，避免双通道干扰观察）。
- **操作**：注销 Windows → 重新登录。
- **预期**：托盘出现程序图标；**不弹主窗口、不开浏览器**；三组件被补齐（3001/443/9876 LISTENING）。若服务自启与程序自启同时开，重复拉起为无害空操作。
- **证据记录**：托盘有/无 ____；主窗有/无 ____；三端口状态：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test startup`（--hidden × linkStartServices 正交、零派发分支）；T12 真机已证 --hidden 无主窗/托盘在/进程存活。人工部分仅为"重登"这一步。

### AC12 联动关闭（仅程序自启）【纯人工】

- **前置**：「程序开机自启」开 +「联动补齐」**关**；三组件全停。
- **操作**：注销 → 重新登录。
- **预期**：托盘出现、无主窗；三组件**未被程序拉起**（三端口无 LISTENING）。
- **证据记录**：三端口状态：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test startup`（联动关 → 零派发断言）。

### AC13 退出保留服务（远程不断线）【手机访问】

- **前置**：三组件运行中；手机（或另机）已能访问 `https://ai.jackqi.cn`。
- **操作**：托盘右键 →「退出」；用手机刷新页面。
- **预期**：程序进程消失；手机端访问不受影响。
- **证据记录**：手机访问结果：____；本机 netstat：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test exit_flow`（keep 分支不收摊）；组件不挂 Job 的架构保证（ADR-0002）。

### AC14 停止服务并退出（收摊）

- **前置**：三组件运行中。
- **操作**：托盘右键 →「停止服务并退出」。
- **预期**：先停三组件（单组件 10s、总 30s 超时放行）后程序退出；端口全释放。加分项：构造停不掉场景（如临时占住 443 的无关进程混淆）验证超时放行 + 日志留痕。
- **证据记录**：netstat：____________；日志尾部（如有超时）：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test exit_flow`（30s 硬顶放行语义）。

### AC15 退出即停止（设置反转）

- **前置**：设置「退出行为」改为"停止服务"；三组件运行中。
- **操作**：托盘「退出」（普通退出）。
- **预期**：等价 AC14：先收摊再退出。
- **证据记录**：netstat：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test exit_flow`（decide_semantics 三分支）。验证后把设置改回默认"保留服务"。

### AC16 程序崩溃/强杀，服务不受影响

- **前置**：三组件运行中；程序运行中。
- **操作**：任务管理器强杀 `ai-remote-workbench.exe` → `netstat` 复核三端口；重启程序观察状态。
- **预期**：三组件继续运行（端口 LISTENING、PID 不变）；重启后程序如实显示"运行中"并可接管。
- **证据记录**：强杀后端口/PID：____________；重启后状态显示：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：**2026-09-09 已做进程级自动化验证**（taskkill 程序 → 组件存活 → 重启无重复进程接管，证据见 tasks.md T16 注记）；架构保证：组件不挂 kill-on-close Job（ADR-0002）。人工仅需补界面状态显示一眼。

### AC17 关机/注销不收摊【关机实弹】

- **前置**：三组件运行中；程序运行中。
- **操作**：直接关机（或注销）再开机登录。
- **预期**：无"阻止关机"弹窗、无半停残留；重启后按自启体系恢复（对照 AC10/11）。
- **证据记录**：关机弹窗：有/无 ____；重启后状态：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test exit_flow`（ExitRequested 无意图 → 零收摊断言，AC17 语义核心）。

### AC18 关闭主窗 = 最小化到托盘

- **前置**：主窗口显示中。
- **操作**：点主窗右上角 X。
- **预期**：窗口隐藏、程序进程存活、托盘图标在；左键托盘「显示主界面」可复现窗口。
- **证据记录**：`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：T2 真机已实证（关 X → 隐藏到托盘、进程存活、托盘 UIA 可见）。复核一次即可。

### AC19/20 低频操作与 UAC【UAC 点击实弹】

- **前置**：主界面「低频操作」折叠区展开。逐项执行并观察。
- **操作**（7 项）：
  1. 安装/重装服务端（UAC「是」）→ 窗口执行 install-server.ps1 可见；
  2. 升级 CloudCLI（勾 -Update，UAC）；
  3. HTTPS 栈装机 install-https（UAC）→ **滚到结尾确认"两步手工活"命令与踩坑警告完整可读、窗口不自动关闭**；
  4. HTTPS 环境配置 enable-https（UAC）；
  5. 客户端配置 install-client → **普通可见交互窗**（非 UAC），交互正常；
  6. ddns-go 管理页 → 默认浏览器打开 `http://localhost:9876`；
  7. 打开工作台页面 → 浏览器打开本机工作台。
- **UAC 拒绝路径**：任选一项 UAC 操作，弹窗点「否」。
- **预期**：各入口正常拉起对应窗口/页面；**UAC 被拒 → 程序给出明确提示且不崩溃**；脚本失败（非零 exit）同样有提示。
- **证据记录**：7 项逐一：□1 □2 □3 □4 □5 □6 □7；拒绝提示文案：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test scripts`（-Lang 对齐、runas、可见/隐藏、超时、退出码→UI 语义映射全部断言）；T15 渲染核验。人工聚焦 UAC 实弹与"结尾提示可读"。

### AC21/22/25 设置与语言【视觉/语言人眼核验】

- **前置**：设置页打开。
- **操作与预期**（逐项）：
  1. 五项行为开关逐个修改 → 保存即生效；重启程序保持（AC21）；
  2. 端口/路径/域名只读卡：数值正确（3001/443/9876、`D:\Software\cloudcli-https`、`ai.jackqi.cn`）、一键复制可用、附"修改须重跑安装脚本"指引（AC22）；
  3. 中文系统首启全中文（界面 + 托盘菜单 + 脚本输出）；切英文 → **立即生效**（界面 + 托盘菜单重建，无需重启）；显式选择后改系统语言不再跟随；删设置文件回"跟随系统"；日志抽查脚本 `-Lang` 与程序一致（AC25）。
- **证据记录**：各项：□AC21 □AC22 □AC25-中文 □AC25-切换即时 □AC25-显式优先 □AC25-删除回退；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test settings`（默认值/持久化/补丁）；`cargo test lang`（auto 判定纯函数、切换即时含托盘重建语义、-Lang 同步）；T14 真机已证损坏恢复。人工为人眼视觉核验。

### AC23/24 默认值与损坏恢复

- **前置**：程序退出状态。
- **操作**：① 删除 `%APPDATA%\ai-remote-workbench\settings.json` → 启动程序；② 写入损坏 JSON（如 `{ 这不是合法 JSON`）→ 启动程序。
- **预期**：① 全部默认值（语言跟随系统、服务自启开、程序自启关、联动开、退出保留）；② 回退默认 + 界面提示 + 目录下出现 `settings.json.bad-*` 留档，程序不崩溃。
- **证据记录**：默认值核对：____；留档文件名：____________；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：`cargo test settings`（13 项含 .bad-* 留档与原子写）；T14 真机 12:29 已实证。复演一次即可。

### 加固（托盘）：explorer 重启

- **前置**：程序运行中（托盘图标在）。
- **操作**：任务管理器结束 `explorer.exe` → 任务管理器「运行新任务」输入 `explorer` 重启外壳。
- **预期**：托盘图标自动恢复（TaskbarCreated 重挂）；主窗隐藏时任务栏无残留按钮；程序与服务不受影响。
- **证据记录**：图标恢复：是/否 ____；`结果：[ ] 通过 [ ] 不通过`
- **自动化背书**：依赖 tray-icon 的 TaskbarCreated 机制（plan §7）；2026-09-09 已做锁屏态进程级尝试（结论见 tasks.md T16 注记，若 UIA 不可用则此条以本手册人工执行为准）。

## 3. 结果回写

- 全部通过 → tasks.md：勾选 T16 与文末清单各条，异常记录直接写在对应行尾。
- 任一不通过 → 记录现象与复现步骤，回到对应模块修复（逻辑类补单测），修复后重验该条。
- 完成后按 DoD 检查表收尾（tasks.md 文末）。

## 4. AC 逐条验证对照表（T18 汇总 · done 判定依据）

> 2026-09-09 T18 汇编。三列口径：**自动化** = `cargo test` 常驻用例（模块数与关键用例名，全套 125 passed + 1 ignored）；**真机** = 各批次执行注记中的非交互实证（日志/端口/进程口径）；**剩余手工** = 本手册 §2 条目（执行后方可勾 spec.md 的 AC）。**不确定的证据一律标"待查"，未查证不写"已过"。**

| AC | 自动化测试证据 | 真机实证证据 | 剩余手工项 |
|----|----------------|--------------|------------|
| AC1 启动轮询至就绪 | orchestrator 22 项：`start_cloudcli_runs_script_then_reaches_running`、`start_all_covers_every_component`、`script_exit1_maps_to_unavailable_hint`（exit 1→"cloudcli 不可用"）、`start_timeout_marks_failed_with_log_tail`（60s 超时+日志尾部） | T13：联动派发后 3001/443/9876 三端口 LISTENING（整屏截图核验）；T17 路径1：安装版联动补齐 caddy 2.0s / ddns-go 2.0s / cloudcli 4.1s 就绪 | §2 AC1（点「启动」计时 + 界面文案 + 拔 cloudcli 失败分支） |
| AC2 停止与端口释放 | stop 14 项：树杀顺序 `cloudcli_collects_tree_before_killing_then_verifies`、复核失败 `cloudcli_verify_failure_reports_manual_commands`、caddy 兜底 `caddy_stop_failure_falls_back_to_path_checked_kill`、10s 超时放行 `exhausted_budget_times_out_with_manual_hint`；orchestrator `stop_one_runs_pipeline_and_updates_state`、`stop_all_isolates_component_failure` | T9：CloudCLI+Caddy 真实启停全链路通过（脚本 exit 0→就绪→树杀/优雅停→双端口释放）；ddns-go 刻意跳过（外部 DNS 副作用） | §2 AC2（点「停止」+ netstat 复核；ddns-go 停止链由此补） |
| AC3 幂等补齐 | orchestrator `start_skips_already_running_component`、`start_guard_marks_port_held_without_spawning` | T12：已在运行的 caddy/ddns-go 跳过；T16 注记②：重启程序日志"守卫跳过（AC3 幂等）"完成接管、无重复进程 | §2 AC3（任务管理器 PID 对比） |
| AC4 外部停止感知 ≤10s | orchestrator `poller_refreshes_periodically_until_shutdown` + `config_defaults_follow_plan`（前台轮询 2s ≤ 5s 常量断言）；probe 11 项（探测口径） | —（轮询为后台行为，无界面可截） | §2 AC4（强杀 ddns-go 计时观察） |
| AC5 外部通道启动接管 | probe 11 项：`classify_running_when_exe_name_matches` / `classify_running_when_exe_path_matches` / `trait_probe_composes_collect_and_classify`（含真实监听集成 `windows_probe_real_listener_lifecycle`） | T9/T13：脚本与外部启动的进程被如实识别为运行中 | §2 AC5（start-server.bat 场景复演） |
| AC6 部分失败+组件重试 | orchestrator `script_exit1_maps_to_unavailable_hint`（单组件失败其余成功）、`native_dispatch_failure_marks_failed`；前端"部分失败不出启动成功、重试仅 failed/port-held"（T13 注记，构建零错误） | — | §2 AC6（占 443 场景 + 界面观察 + 重试转绿） |
| AC7 端口被占双口径 | probe `classify_port_held_shows_process_name`、`classify_port_held_fallback_names`；orchestrator `start_guard_marks_port_held_without_spawning` | T9：caddy 双栈监听重复 PID 致误判 port-held 的真机 bug 已修复 + 回归单测 `exe_of_pids_survives_duplicate_pids` | §2 AC7（界面告警色核验） |
| AC8 自启接管 | autostart 15 项：`enable_services_runs_script_and_detects_takeover`、`service_task_names_align_with_sprint0_script`、`app_register_spec_contract` | T11 真机 roundtrip（--ignored 实跑）：三条 Sprint0 任务只读验证存在 → 自身任务注册/移除 → 确认消失，Sprint0 三任务原状未动 | §2 AC8/9（任务计划程序 UI 核对 + 程序内开关） |
| AC9 自启移除 | autostart `disable_services_passes_remove_without_queries`（-Remove、零查询、进程不受影响语义） | 同上（roundtrip 含移除路径） | §2 AC8/9 |
| AC10 程序自启静默入托盘 | autostart `app_register_spec_contract`（--hidden / ExecutionTimeLimit Zero / 登录触发）；startup `hidden_flag_parsed_from_args`、`hidden_startup_suppresses_main_window` | T12：生产 exe --hidden 启动无主窗、托盘在、进程存活；T17 路径1：--hidden 启动托盘窗口类 `tray_icon_app` 存在 | §2 AC10/11（**注销重登**，纯人工） |
| AC11 自启联动补齐 | startup `link_start_enabled_starts_all_components`、`plan_combines_args_and_link_setting`（hidden × link 正交） | T13：联动实启三组件；T17 路径1：安装版联动补齐三组件全 LISTENING | §2 AC10/11（**注销重登**） |
| AC12 联动关闭零派发 | startup `link_start_disabled_starts_nothing`（零派发断言） | T17 路径2：便携包 linkStartServices=false → 零派发、三端口静默 | §2 AC12（**注销重登**，纯人工） |
| AC13 退出保留服务 | exit_flow 7 项：`gate_is_one_shot_and_keep_does_not_arm`、`decide_semantics_three_branches`（keep 分支零收摊） | T2：关 X 后进程存活（同语义旁证；无手机访问记录） | §2 AC13（**手机/另机访问**，纯人工） |
| AC14 停止并退出（收摊） | exit_flow `shutdown_total_timeout_is_30s`、`shutdown_total_timeout_releases_exit`（30s 硬顶放行）；stop 管线全项（见 AC2） | T9：停止链路真机已过（收摊=stop_all 同管线；30s 放行场景未真机构造） | §2 AC14（托盘点击 + 构造停不掉场景验证放行） |
| AC15 退出即停止（设置反转） | exit_flow `decide_semantics_three_branches`（exitAction=stop → 收摊） | — | §2 AC15（设置切换 + 托盘退出） |
| AC16 程序强杀服务存活 | 架构保证：组件不挂 kill-on-close Job（ADR-0002）；`arc_orchestrator_is_shareable_for_blocking` 等 | **2026-09-09 进程级实证**（tasks.md T16 注记②）：三组件运行中 taskkill 程序 → 3001/443/9876 监听 PID 原样存活 → 重启程序接管无重复进程 | §2 AC16（界面状态显示一眼） |
| AC17 关机不收摊 | exit_flow `exit_requested_without_intent_skips_shutdown`（无意图零收摊，语义核心） | —（关机实弹无法自动化） | §2 AC17（**关机/注销实弹**，纯人工） |
| AC18 关 X 最小化到托盘 | —（纯 GUI，宪法窄例外） | **T2 真机已证**：关 X → 隐藏到托盘、进程存活、托盘 UIA 可见 | §2 AC18（复核一次即可） |
| AC19 低频操作入口 | scripts 19 项：`visible_params_keep_window_open_and_interactive`（-NoExit 结尾可读）、`tool_plan_install_server_default_and_options`（-Update/-UseMirror 透传）、`tool_plan_https_and_client_visibility`（runas/可见窗分派）；urls 5 项（地址映射） | T15：折叠区 7 项渲染核验（锁屏前整屏截图）；T17：安装/便携双形态脚本目录命中 | §2 AC19/20（**UAC 实弹点击** + install-https 结尾可读性） |
| AC20 UAC 拒绝/失败提示 | lang `shell_error_text_maps_uac_decline`（SE_ERR_ACCESSDENIED→明确提示）；scripts `interpret_exit_maps_sprint0_codes`（exit code→UI 语义） | —（UAC 弹窗无法无人值守） | §2 AC19/20（UAC 点「否」路径） |
| AC21 行为开关持久化 | settings 13 项：`defaults_match_plan_schema`（五默认值）、`apply_patch_only_touched_fields`、`state_patch_persists_and_updates_memory`、`save_is_atomic_and_leaves_no_temp_files` | T17 路径3：覆盖安装后设置逐字保留（持久化旁证） | §2 AC21/22/25（逐项修改 + 重启保持） |
| AC22 端口/路径只读 | probe `component_ids_ports_and_names`、`expected_identity_matches_deployment`（3001/443/9876、栈目录常量）；urls `build_urls_shapes_align_with_sprint0_menu`（地址三形态） | T13：只读卡渲染核验（截图） | §2 AC21/22/25（复制按钮 + 指引文案人眼） |
| AC23 默认值启动 | settings `load_missing_file_returns_defaults`、`defaults_match_plan_schema` | T17 路径1：安装版首启默认联动开（旁证） | §2 AC23/24（删设置文件复演） |
| AC24 损坏恢复留档 | settings `corrupt_file_repaired_with_bad_backup`（.bad-* 留档）、`state_load_reports_corrupt_repair`、`save_is_atomic_and_leaves_no_temp_files`（原子写） | **T14 真机实证**（12:29）：损坏设置留档回退成功 | §2 AC23/24（复演一次） |
| AC25 双语跟随/切换 | lang 12 项：`chinese_primary_lang_resolves_zh`/`non_chinese_resolves_en`（auto 纯函数）、`apply_language_updates_shared_state_explicit_first`（显式优先）、`tray_texts_complete_for_both_langs`；orchestrator `lang_source_overrides_config_for_script_and_detail`（切换即时含 -Lang/detail）；scripts `run_server_hidden_spec_shape`（-Lang 对齐）；T18 补齐 stop/autostart/err 词条 7 项新断言 | T2：语言探测 Zh 实测；T17 路径3：language=en 持久化生效 | §2 AC21/22/25（**语言切换人眼核验**：首启中文/切英即时/显式优先/删除回退） |
| 加固：托盘 explorer 重启 | tray-icon TaskbarCreated 机制（plan §7，库层保证） | 2026-09-09 进程级实证（T16 注记③）：explorer 强杀重启 → 程序存活、托盘窗口类仍在；图标视觉恢复因锁屏未人眼核验 | §2 加固（托盘）（图标可见性人眼） |

**非 AC 的约束项佐证**（spec 目标/约束 §5）：

| 约束 | 证据 |
|------|------|
| 常驻内存 ≤150MB | T3 备注：任务管理器口径 97.9MB（T13~T15 接入后建议复核一次） |
| 安装包 ≤15MB | T17 备注：setup.exe 2.79MB / zip 1.44MB |
| 双形态分发三路径 + 设置保留 | T17 备注：安装/便携/覆盖升级三路径进程级验证全过；产物落 `release/` |
| 脚本对外行为不变 | T17：resources/bin 为 SHA256 比对的只读副本（manifest.json 清单），tools/sprint0 原件未动 |

**统计与判定口径**（2026-09-09，T16 手工项执行前）：

- **逻辑层已由自动化全覆盖**：25 条 AC 全部有自动化用例或架构级断言背书（AC18 为唯一纯 GUI 项，已有 T2 真机实证）。
- **自动化 + 真机双证**（手工仅复核/一眼）：AC3/5/8/9/16/18/23/24（8 条）。
- **自动化过、真机部分、待手工观察**：AC1/2/4/6/7/10/11/12/14（9 条，其中 AC10/11/12 的"注销重登"与 AC14 的托盘收摊点击为核心缺口）。
- **自动化过、纯人工未做**：AC13（手机访问）、AC15、AC17（关机实弹）、AC19/20（UAC 实弹）、AC21/22/25（语言/视觉人眼）+ 加固托盘图标（6 类，对应手册 §1 的纯人工标注）。
- **待查项**：无（本表全部证据可在 tasks.md 注记与本手册 §1 溯源）。
- **done 判定**：上表"剩余手工项"列全部执行通过（记录回写 §2 各条）后，25 条 AC 即满足"自动化 + 手工双轨验证"，可翻 done。
