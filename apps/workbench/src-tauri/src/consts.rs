//! 实例常量（plan §4：端口/路径为编译期常量，不进设置文件；
//! 与 sprint0 部署知识对齐，修改须重跑安装脚本，故对用户只读展示）。
//!
//! spec 013 起域名口径变更：DOMAIN/WORKBENCH_URL 降级为「回落默认值」
//! （settings.domain 未配置时生效），运行时消费点经
//! `settings::effective_domain / effective_workbench_url` 取值，不得直读本文件。

/// CloudCLI 控制端口（本机 127.0.0.1）
pub const CLOUDCLI_PORT: u16 = 3001;
/// Caddy HTTPS 端口
pub const CADDY_PORT: u16 = 443;

/// HTTPS 栈部署目录**默认值**（spec 004 起可由用户在设置中配置，
/// 持久化于 Settings.stack_dir，运行时各组件读配置；重启生效）
pub const DEFAULT_STACK_DIR: &str = r"D:\Software\cloudcli-https";
/// Caddy 配置文件（setup-autostart.ps1 契约：caddy.exe run --config）
pub const CADDYFILE_PATH: &str = r"D:\Software\cloudcli-https\Caddyfile";

/// 对外域名**回落默认值**（spec 013：运行时生效域名见 settings::effective_domain）
pub const DOMAIN: &str = "ai.jackqi.cn";
/// 工作台页面地址**回落默认值**（spec 013：运行时见 settings::effective_workbench_url）
#[allow(dead_code)] // 与 DOMAIN_ROOT 同口径：仅测试与文档对照消费，无运行时直读
pub const WORKBENCH_URL: &str = "https://ai.jackqi.cn/";

/// CloudCLI 监听进程的可执行名：run-server-hidden.ps1 经 npm 全局拉起 node.exe，
/// 路径不定 → 身份按文件名匹配（AC7 双重判据）
pub const CLOUDCLI_EXE_NAME: &str = "node.exe";

/// 栈目录环境文件（腾讯云密钥唯一载体，spec 008 凭据单源化：Caddy DNS-01
/// 续期与 dns_api 调和均读此处；历史名 FRPC_ENV_FILE 随穿透通道退役）
pub const STACK_ENV_FILE: &str = ".env";
/// 对外域名根（spec 013 后运行时根域由 `settings::effective_root` 动态派生，
/// 本常量仅作回落基准供测试与文档对照，无运行时消费方）
#[allow(dead_code)]
pub const DOMAIN_ROOT: &str = "jackqi.cn";

// ── 组网通道（spec 007：EasyTier v2.6.4 随包分发，版本+SHA256 锁定）──────────

/// easytier 主程序（Windows 服务承载，spec 007 plan §3/§4.4）
pub const EASYTIER_CORE_EXE_NAME: &str = "easytier-core.exe";
/// easytier 状态探询 CLI（RPC 仅绑 127.0.0.1，plan §3.1）
pub const EASYTIER_CLI_EXE_NAME: &str = "easytier-cli.exe";
/// TUN 虚拟网卡驱动库（easytier 官方包随附；栈目录落位必须同带，否则服务起不来）
pub const WINTUN_DLL_NAME: &str = "wintun.dll";
/// WinDivert 用户态库（easytier-core 的动态依赖——缺它进程直接拒绝启动，
/// T2 探测实测：`error while loading shared libraries: packet.dll`）
pub const PACKET_DLL_NAME: &str = "packet.dll";
/// WinDivert 内核驱动（easytier 包过滤/子网代理用；与 Packet.dll 成对落位）
pub const WINDIVERT_SYS_NAME: &str = "WinDivert64.sys";

// ── 组网默认参数（spec 007 plan §4.1 MeshConfig 默认值）────────────────────

/// 默认网络名（EasyTier network_name；成员以此 + network_secret 相认）
pub const DEFAULT_MESH_NETWORK_NAME: &str = "ai-remote";
/// 默认宿主机虚拟 IP（config.toml 顶层 ipv4，dhcp=false 静态持有）
pub const DEFAULT_MESH_VIRTUAL_IP: &str = "10.126.126.1";
/// 默认虚拟网段（网段冲突检测输入；EasyTier 出厂冷门段，plan §7-R7）
pub const DEFAULT_MESH_VIRTUAL_CIDR: &str = "10.126.126.0/24";
/// 默认对端节点（社区公益节点，腾讯云上海；无 SLA，多对端可编辑——plan §7-R1。
/// T2 实测 2026-09-10 连通 46ms/0% 丢包，legacy 形态）
pub const DEFAULT_MESH_PEERS: &[&str] = &["tcp://sh.vomiku.com:7910"];
/// 内置候选中继节点池（spec 014：仅候选不生效，探测把关后才可入 peers）。
/// 社区节点无 SLA——vomiku 于 2026-09-24 起停止服务（外部多点探测 refused），
/// 正是本池存在的原因；清单随版本维护，用户可在设置区自定义候选兜底。
/// us01：2026-09-25 真机实测连通（美国公共服务器，EasyTier 2.5.0）。
pub const RELAY_POOL_BUILTIN: &[&str] = &[
    "tcp://us01.225284.xyz:11010",
    "tcp://sh.vomiku.com:7910",
];
/// 期望 SHA256：官方 Release easytier-windows-x86_64-v2.6.4.zip 内件
/// （2026-09-10 经 gh 官方通道下载，zip 完整性 unzip -t 通过；镜像通道文件
/// 与官方不符已弃用——plan §7-R6 教训）
pub const EASYTIER_CORE_SHA256: &str =
    "da7eb2d24b5416f3d3407636949e964a0750e3f9dc53a828cb6799a57ead445d";
pub const EASYTIER_CLI_SHA256: &str =
    "d8783e851e944b44a9b71b39fd02f227ec0a2a82b3165c55ead5dd32dcde53a1";
pub const WINTUN_DLL_SHA256: &str =
    "e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce";
pub const PACKET_DLL_SHA256: &str =
    "c7c03a87eac7243ccbe331554624b18803010b740e311fc8cfddb573096eacac";
pub const WINDIVERT_SYS_SHA256: &str =
    "8da085332782708d8767bcace5327a6ec7283c17cfb85e40b03cd2323a90ddc2";
