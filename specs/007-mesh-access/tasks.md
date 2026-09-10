# 007-mesh-access · 任务清单（tasks）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · 返回 [MOC](../MOC.md)

- **状态**: 进行中 <!-- 未开始 | 进行中 | 已完成 -->
- **最后更新**: 2026-09-10

> 拆解原则：每个任务可在一天内完成、有明确完成标志、可追溯到验收标准（AC）。
> 任务状态标记：`[ ]` 待办 · `[~]` 进行中 · `[x]` 完成

## 阶段 1: 资源与前置实测

- [x] T1 easytier v2.6.4 二进制入库：下载 windows-x64 官方 Release（easytier-core.exe + easytier-cli.exe + **wintun.dll**——TUN 驱动库随包，落位必须同带），落 `resources/bin/`，consts.rs 登记版本 + 三文件 SHA256，`mesh.rs` 校验函数 + 单测 3 项（真文件/缺失/篡改）。（验收: AC1 前置；完成标志：cargo 168 全绿 ✓。下载经 gh 官方通道；镜像通道文件与官方不符已弃用——供应链教训记 plan §7-R6）
- [x] T2 外部事实真机实测（依赖: T1；验收: AC1/AC9 前提 + spec §5 验收前提约束）：①临时 config + 手动服务方式跑通社区节点 `tcp://sh.vomiku.com:7910`（移动网络 + WiFi 双环境）；②secure-mode 与 legacy 成员兼容性（Android 官方 App 无 secure UI，以双实例模拟成员互连）——**实测推翻混合组网设想，触发 spec/plan 变更（附注②）**；③A 记录临时指私网 IP 后公共递归（223.5.5.5 / 8.8.8.8 / 运营商默认）是否照常返回（bogon 风险）；④本机物理网卡网段与 10.126.126.0/24 冲突排查。完成标志：四项结论记录于本文件附注（不达标即触发升级路径评估，停实施）✓（2026-09-10，结论见附注①~④；Android 真机加入验证留 T14）

## 阶段 2: Rust 核心（测试先行）

- [x] T3 settings 扩展：`AccessChannel::Mesh` + `MeshConfig`（network_name/virtual_ip/virtual_cidr/peers 默认值）+ `tunnel_disabled`/`direct_disabled` 标记；旧 settings.json 兼容加载测试（样板：old_settings_file_without_channel_fields）（验收: AC11；完成标志：兼容与默认值单测通过）✓ 2026-09-10（5cf1b3c；两处默认刻意分离：新装机 Default=mesh、旧文件缺字段 serde 字段级=direct；004 前与 004 时代双兼容样板 + 停用标记补丁合并，170 绿）
- [x] T4 `mesh.rs` 模块（依赖: T3；验收: AC9/AC11）：config.toml 渲染器（toml crate，唯一渲染路径，模板固化 legacy 形态——**不含 [secure_mode] 段**（T2 实测定案，升级路径注释见 plan §4.3）、hostname/ipv4/listeners 字段按实测形态）+ 渲染前置校验（network_secret 缺失/为空拒绝渲染，AC9 新语义）+ 渲染产物断言单测 + 启动前校验（`--check-config` 调用）+ 网段冲突检测纯函数（物理网卡 IPv4 × virtual_cidr 重叠判定）+ MeshConfig 校验（非空/IP/网段/peers URI 格式）✓ 2026-09-10（4781d43；listeners 收敛 tcp/udp 11010；冲突检测双判据+漏报声明；渲染产物过真 exe --check-config 集成测试，178 绿）
- [x] T5 `dns_api.rs` 调和扩展：`RecordOp` 新增 `UpdateValue`（A 记录改值）与 `Delete`（CNAME 彻底删除——停用语义）；reconcile 单测覆盖 upsert/清理终态（验收: AC5/AC6/AC7 逻辑部分；完成标志：终态记录集断言通过）✓ 2026-09-10（737ee0e；reconcile 升级 DnsTarget 三通道枚举，Mesh 分支 CNAME 全删+A upsert 七态单测，179 绿）
- [x] T6 通道编排扩展（依赖: T5；验收: AC1/AC5/AC6）：`switch_actions` 三通道矩阵（→mesh：StopFrpc→StopDdnsGo→渲染校验→服务重启→CNAME 删+A upsert 虚拟 IP→Persist；mesh→direct/tunnel 反向）；`disable_tunnel`/`disable_direct` 停用编排（前置校验非现役、CNAME 删除、disabled 标记持久化、A 记录按通道态处理）；mock 组件动作序列单测 ✓ 2026-09-10（3f020e4；切 mesh「先停旧再起新」序 + MeshNotReady 前置 + disable_actions 六态矩阵，服务动作执行臂暂 Err 待 T9 接 mesh-service.ps1，182 绿。勾选漏于当次提交、本提交补记）
- [x] T7 服务管理（验收: AC3 前置）：`mesh-service.ps1`（install/uninstall/start/stop/restart/status；sc create delayed-auto + failure recovery restart/60000×3；UAC 自提权，install-https.ps1 惯例）+ Rust 侧栈目录落位（exe 复制 + manifest 哈希校验 + `<stack>/easytier/` 布局）+ 服务 binPath 构造纯函数（**断言参数不含 secret**——AC8）（完成标志：binPath 构造与落位单测通过；BOM+CRLF/Parser 校验）✓ 2026-09-10（SCM 可行性源码级取证后才落码：core main 无条件先走 service_dispatcher::start，被 SCM 拉起即进 win_service_main 并从进程命令行解析 -c（控制台启动报 ERROR 0x427 被吞走 CLI）→ sc create 直装成立；官方 easytier-cli service install 会把网络参数（含密钥）写进服务命令行，弃用（plan 变更记录第三条）。实现：PS1 走 New-Service + CIM Change 承载含引号/空格 binPath（sc.exe 经 PS5.1 传参转义易碎）、status 免提权、幂等 install=已存在则刷新 binPath 与自愈配置；Rust service_bin_path()（纯路径参数，测试断言不含 secret 字样）+ stage_easytier_binaries()（复制前后双哈希校验 + logs 预建 + 幂等），186 绿。坑：①PS 双引号串内 `$var:` 是作用域限定语法须写 `${var}`（PARSER 校验抓出）；②HEAD 的 manifest.json 里 reset-ddns-password/set-frp-key 哈希陈旧（006 后期改脚本未重跑清单），本次再生成顺带修正；③真机装服务/杀进程自愈 ≤60s 留 T14 验收）
- [x] T8 状态探询与生命周期整合（依赖: T7；验收: AC3/AC4）：`easytier-cli peer --rpc 127.0.0.1:15888 -o json` 探询封装 + `mesh_state` 判定纯函数（service × peers → online/connecting/offline/not_configured + detail 不含 secret）+ 守护 tick 探询退化（IO 锁外纪律沿用）+ orchestrator 三处 bool 化通道判定重构为枚举匹配（is_tunnel_channel:371 / ChannelSource tunnel.rs:398 / start_all tunnel_mode:411）+ 自启/收摊整合（收摊走提权停服务）✓ 2026-09-10（探询命令实测修正：全局选项 `-p/--rpc-portal` 须在子命令前（`easytier-cli -p 127.0.0.1:15888 -o json peer list`），plan 变更记录第四条；peer list 首项恒为本机（cost="Local"）→ PeerBrief.is_local + 判定排除本机项，否则单机无成员误判在线。实现：MeshState 四态+inactive（非现役通道，对齐 TunnelState::Inactive）+detail 五稳定码（secret_missing/service_missing/service_stopped/service_disabled/rpc_unreachable——构造上不含密钥，AC4）；MeshMonitor 5s 探询只观察不动手（拉起/自愈归 SCM，重启走 T9 提权入口；IO 锁外纪律沿 tunnel.rs 死锁复盘）；sc.exe STATE/START_TYPE 行数字不本地化（GBK lossy 无碍解析），Stopped 需补 sc qc 分辨 Disabled；orchestrator channel_source 枚举化（bool → AccessChannel，无源默认 Direct）+ start_all 枚举匹配跳过 ddns-go（穿透/组网双通道，004 AC8/007 §3.2）；自启整合 set_ddnsgo_autostart：切 mesh/tunnel 单条注销 ddns-go 任务（防开机拉起回写 A 记录，tunnel 方向同适用=004 补强），切回 direct 条件重建（判据=CloudCLI 任务存在即用户自启开关开启，无 settings 持久化故任务存在性是唯一不漂移判据；查询失败跳过记 warn 不阻断）；收摊整合：ExitRequested 中现役 mesh + StopServices 意图 → UAC 派发 mesh-service.ps1 -Action stop（异步不等待，拒绝则服务保持运行、下次开机 SCM 拉回）。199 绿（+13））
- [x] T9 Tauri 命令层（依赖: T4/T6/T8；验收: AC1/AC4/AC5）：`mesh_status`/`mesh_apply_config`/`mesh_install_service`/`mesh_uninstall_service`/`disable_legacy_channel`/`clear_frp_key` + `switch_channel` 扩展 mesh + `channel_health` 体检两项重定义（组网服务/对端、DNS 对齐→A=虚拟 IP）✓ 2026-09-10（体检重定义落点考证：`channel_health` 非独立命令而是前端 TunnelCard 聚合，Rust 侧 = `mesh_status` 快照命令（组网服务/对端数据源）+ `check_dns_alignment` 通道感知重定义（Mesh 判 A=虚拟 IP：DnsAlignment 增 AlignedMesh/MismatchedA + judge_dns_mesh 纯函数 + 探测脚本增 aValue 采集；Direct 态不再要求穿透配置才可检测）。实现：mesh.rs 增 prepare_stack 生效内核（密钥检查→渲染→缺失落位→写盘→--check-config，AC9 拒绝先于任何落盘）+ apply_service_action（未装→install/已装→restart）+ service_install_params（-BinPath 单引号包裹原样传递）；commands.rs 六命令中 apply/install 共享 prepare_mesh_stack 前置、switch_channel 的 RestartMeshService 复用 apply_mesh_effective 内核（同一生效路径无双源）、StopMeshService→提权 stop；disable_legacy_channel 编排臂全接（StopFrpc/停 ddns-go+取消自启/DeleteCname/DeleteA→dns_api purge_cnames·purge_a_records（指定类型全删含暂停残留）/MarkDisabled→settings.patch），DNS 清理凭证缺失降级 warn 不阻断（回退手动指引，004 惯例）；Script/ToolKind 登记 set-mesh-secret.ps1·clear-frp-key.ps1（脚本实体 T10 同会话落地）。坑：elevate 已自带结果码文案化（返回 Result<(),String>），派发 helper 无需再 shell_error_text；dns_api 的 is() 是 reconcile 内闭包，purge_ops 内联 eq_ignore_ascii_case。206 绿（+7））

## 阶段 3: 凭证脚本

- [x] T10 `set-mesh-secret.ps1`（Read-Host -AsSecureString ×2 不回显、直写 `<stack>/easytier/network-secret`、复核仅显末 4 位，set-frp-key.ps1 惯例）+ `clear-frp-key.ps1`（从栈 `.env` 移除 SAKURA_FRP_KEY 行、复核显示已移除）+ **build.ps1 $ScriptSubset 登记两个新脚本**（006 坑：漏登记打包即删）（验收: AC8/AC10；完成标志：BOM+CRLF、Parser 校验、真机跑一遍脚本流程）✓ 2026-09-10（编码关键决策：network-secret **UTF-8 无 BOM** 单行——Rust read_to_string 不剥 BOM 且 trim 不除 U+FEFF，BOM 会作为隐藏字符混进密钥值致成员握手失败；与 .env 的 BOM 惯例相反（.env 由 PowerShell 自家回读），脚本内注释固化该差异。clear-frp-key 保持 .env 的 BOM 形态（WriteAllLines + UTF8 BOM，set-frp-key 写入惯例）。实测：clear-frp-key 临时 .env 端到端验证（目标行移除、其余行+BOM 保留、幂等复跑「无可清理即成功」exit 0）；set-mesh-secret 双语/Parser/BOM+CRLF 过，交互流程（不回显×2/末 4 位复核）留 T14 真机。分发三件套齐：build.ps1 $ScriptSubset 登记、resources/bin 副本（源/副本 sha256 两两一致）、manifest.json 14 条目 python 全量复核 OK；manifest 形态考证：HEAD 内本为 BOM+LF（git text=auto 归一化），Edit 产出同形态仅 3 行净增。坑：变量名 $EnvFile 后接全角括号安全（PS 仅在 `$name:` ASCII 冒号时解析作用域——T7 坑的规避形态）。206 绿持平）

## 阶段 4: 前端

- [x] T11 通道与设置 UI（依赖: T9；验收: AC1/AC4/AC5/AC6/AC11）：`types.ts` AccessChannel 加 "mesh" + MeshConfigView；`TunnelCard.tsx` 三通道单选 + 停用态（「已停用」chip + 重新启用安全警示确认）；`SettingsView.tsx` 组网设置卡（网络名/IP/网段/对端列表行内编辑即时校验，无密钥输入框仅脚本指引）；`api.ts` 对接新命令 ✓ 2026-09-11（提交 8b1f85a。要点：切 mesh 无前端预判——mesh_status 的 inactive 态 monitor 不报密钥/服务信息，预判必假，Rust MeshNotReady 拒绝文案透出；DNS 轮询条件从「非 tunnel」改「direct 例外」防 mesh 态漏检；DnsNotice 目标值按结论 kind 取（nodeDomain/virtualIp）而非当前通道；体检网络归类 mesh 态 N/A 中性文案——TUN 网卡防火墙归类影响留 T14 真机验证；预检 ipv4ToLong 数值判 IP∈CIDR 与 Rust validate 同形。npm run build 绿）
- [x] T12 i18n 双语同步（验收: AC4/AC11/AC12 呈现层）：zh.ts/en.ts 扁平键新增 `mesh.*`/`channel.disabled.*`/`wizard.channel.mesh*` 等，两侧同步无缺键 ✓ 2026-09-11（提交 1f4a41c。T13 向导键先行（缺键即 tsc 挂，UI 与词典分提交需词典预置）；直连分支文案加安全警示；wizard.code 五稳定码命名沿 serpent 先例；穿透分支专属键随 T13 代码移除时一并清理）
- [x] T13 向导组网分支（依赖: T9/T10；验收: AC12）：`WizardView.tsx` channel 阶段组网分支（默认推荐）：装服务（UAC）→ 成员设备客户端指引（官方下载地址）→ set-mesh-secret.ps1 → apply + 在线校验 → A 记录 upsert + 解析校验；移除 SakuraFrp 分支；直连分支加安全警示；收尾页接停用入口（wizard.rs 检测步骤同步）✓ 2026-09-11（步骤序定案「密钥先行」：mesh_install_service 内部 prepare_stack→read_network_secret 拒绝，顺序①密钥②装服务③成员指引④应用（次级，修 service_stopped）⑤同步 DNS。新增 Rust 命令 `mesh_sync_dns`（plan §5.1 表外→变更记录补录）：向导分支选择只 patch channel 不做编排，新装机默认 mesh 时 switch_channel AlreadyOnTarget 短路致 A=虚拟 IP 无人创建。wizard.rs derive_mesh 矩阵（密钥>未装>停止/禁用>等成员；「运行无成员」不判 Done——漏配成员设备即收尾会无人能访问）+ mesh_detail_with_dns（ENABLE A==虚拟 IP）；存量 branch="tunnel" 向导呈迁移提示（Rust detect Tunnel 分支保留兼容，wizard.code.missing_key/tunnel_unset 键保留）；穿透面板七 UI 键两侧清理。cargo test 209 running 全绿（206+3 ignored）、npm build 绿）

## 阶段 5: 验收与收尾

- [x] T14 真机手工验收（依赖: 全部）：建 `acceptance-manual.md` 清单，逐 AC 验证（AC7 双通道口径：外部非成员探测不可达 + 成员访问正常；AC10 换钥吊销真机实测；AC2 换网重连；AC3 杀进程 SCM 自愈 ≤60s）✓ 2026-09-11 清单已建（[acceptance-manual.md](./acceptance-manual.md)：AC1~AC12 逐条步骤/预期/自动化覆盖标注/实测留白 + 附加观察项 O1~O3——TUN 网卡防火墙归类影响（T11~T13 遗留）、bogon 复测留档（并入 AC7）、社区节点质量观察），**真机执行与回填待需求方**；AC 勾选与 spec 状态翻转随回填进行
- [~] T15 文档同步收尾：CHANGELOG Unreleased 登记、`.env.example` SAKURA_FRP_KEY 标注停用后可清除、MOC 状态流转、spec.md AC 勾选与状态 done、README 通道描述核对；对照 DoD 清单收尾（先行部分 ✓ 2026-09-11：CHANGELOG Unreleased 登记 007 三 Added/一 Changed/一 Removed、`.env.example` SAKURA_FRP_KEY 停用可清除标注、MOC 007 行注记（顺带修正 secure-mode→legacy 陈旧表述）、README 核对无需改（根 README 仅 001 能力概述无通道表述，apps/workbench 无 README）；**待真机验收后收尾**：spec.md AC 勾选与状态 done、MOC 流转 done、DoD 清单勾选）

## 完成标志（DoD 检查）

- [ ] spec.md 中所有 AC 已逐条验证通过
- [ ] 自动化测试全部通过（含新增：渲染/校验/编排/调和/状态判定/binPath 无密钥断言）
- [ ] 相关文档已更新（plan/tasks/spec/MOC/CHANGELOG/调研报告事实修正）
- [ ] 本文件全部任务勾选完毕

---

## 附注：T2 前置实测结论（2026-09-10 回填，spec/plan 变更的实证依据）

### ① 社区节点连通（移动网络环境）

**✓ 通过**。legacy 模式双实例均连 `tcp://sh.vomiku.com:7910`：A↔节点 p2p 46.62ms / 0% 丢包（tcp6 隧道）、B↔节点 53.53ms / 0%；A↔B 经节点相遇后互见（0.15ms，tcp6+tcp 双隧道）。本网络 V4 到节点超时、**V6 回退成功**（移动网络 V4 出网受限的既有特征，与 frp 被拦同源——memory `frp-login-eof-network-block`）。WiFi 环境未单独测（T14 真机验收覆盖 AC2 换网场景）。

### ② secure-mode 兼容性矩阵（双实例模拟成员互连，触发 spec 变更）

**✗ 混合组网被证伪 → 产品形态定案 legacy 模式**（spec 变更记录 2026-09-10 第三条）。实测矩阵（easytier v2.6.4-8428a89d）：

| 组合 | 结果 | 报错原文 |
|---|---|---|
| legacy 客户端 → legacy 节点 | ✓（①） | — |
| **secure 客户端 → legacy 社区节点** | ✗ | `conn closed during wait handshake response`（V6 通道，TCP 可达但握手被关） |
| **legacy 客户端 → secure 宿主机** | ✗ | `secret key error: same-network peers must use the same secure mode`（TCP 连接建立后即断） |
| secure ↔ secure（直连） | ✓ | 0.40ms / 0% 丢包（升级路径依据） |

结论链：社区节点未开 secure（拒 secure 客户端）+ 同网络成员必须同为 secure + **Android 官方 App 无 secure-mode 配置 UI** → 全链 secure 缺成员不可达 → 宿主机与 Android 成员统一走 legacy（network_secret 派生加密）。官方文档「开启安全模式的服务端可以接受旧客户端连接」与 v2.6.4 实测不符（文档超前或版本差异）。

**secure-mode 配置方法（升级路径已验证，留档）**：`[secure_mode]` 段 `enabled = true` + `local_private_key` + `local_public_key` **成对显提供**——`openssl genpkey -algorithm X25519` 生成、raw hex 转 base64；实测只给私钥报 `local public key is not set`（v2.6.4 不自动派生公钥，core --help 文本与行为不符）；客户端可另在 `[[peer]]` 配 `peer_public_key` 锁定共享节点身份（本期未用）。credential 临时凭据 CLI 确实存在（`credential generate --ttl` 带 groups/allow-relay/allowed-proxy-cidrs 参数），但要求全链 secure（spec §6-Q2 决议理由已修正）。

### ③ bogon 公共解析

**✓ 通过**。`meshprobe.jackqi.cn A 10.126.126.1` 经四路公共递归（223.5.5.5 阿里 / 119.29.29.29 DNSPod / 8.8.8.8 Google / 本网运营商默认）全部照常返回私网 A 值，无过滤——Tailscale 100.x 先例的等价实证，无需 hosts/Split DNS 兜底（dns_api.rs 两个 `#[ignore]` 手动测试 `bogon_probe_create`/`bogon_probe_cleanup` 为操作入口，已清理不留残留）。

### ④ 网段冲突排查

**✓ 无冲突**。本机物理网卡仅 WLAN 192.168.3.0/24（192.168.3.13），与默认虚拟网段 10.126.126.0/24 无重叠；运行期网段变化由 T4 冲突检测函数兜底。

### 工程事实补充（实测踩坑，T4/T7/T8 实现参考）

- **动态依赖**：easytier-core 缺 `packet.dll` 拒绝启动（`error while loading shared libraries`）；`wintun.dll`/`Packet.dll`/`WinDivert64.sys` 须成套落位（已入库，5 文件 SHA256 校验）。
- **core --help 重定向文件才有输出**（Windows GUI 子系统，终端直看不显示；17KB 完整参数表）。
- **同机双实例 listeners 端口冲突 fatal**（os error 10048）——第二实例须 `listeners = []`。
- **连 127.0.0.1 的 peer 报 AddrNotAvailable**（10049，源地址绑定问题）——测试对端用物理网卡 IP。
- **`--file-log-dir` 须 Windows 原生路径**（msys `/tmp/...` 路径不落盘，`cygpath -w` 转换）；服务形态由 Rust 传原生路径无此问题。
- **TOML 字段位置实证**：`hostname` 顶层（peer 表显示名，实测生效）；`instance_name` 不进 peer 表 hostname 列；`ipv4` 顶层；默认 listeners 六协议 11010-11013 全开需收敛。
- 中文 Windows 控制台 GBK 编码：抓 easytier 日志输出需 `tr -d '\0'` / `grep -a`。
