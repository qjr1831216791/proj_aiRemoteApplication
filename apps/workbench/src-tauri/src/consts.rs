//! 实例常量（plan §4：端口/路径/域名为编译期常量，不进设置文件；
//! 与 sprint0 部署知识对齐，修改须重跑安装脚本，故对用户只读展示）。

/// CloudCLI 控制端口（本机 127.0.0.1）
pub const CLOUDCLI_PORT: u16 = 3001;
/// Caddy HTTPS 端口
pub const CADDY_PORT: u16 = 443;

/// HTTPS 栈部署目录**默认值**（spec 004 起可由用户在设置中配置，
/// 持久化于 Settings.stack_dir，运行时各组件读配置；重启生效）
pub const DEFAULT_STACK_DIR: &str = r"D:\Software\cloudcli-https";
/// Caddy 配置文件（setup-autostart.ps1 契约：caddy.exe run --config）
pub const CADDYFILE_PATH: &str = r"D:\Software\cloudcli-https\Caddyfile";

/// 对外域名（HTTPS 反代入口）
pub const DOMAIN: &str = "ai.jackqi.cn";
/// 工作台页面地址
pub const WORKBENCH_URL: &str = "https://ai.jackqi.cn/";

/// CloudCLI 监听进程的可执行名：run-server-hidden.ps1 经 npm 全局拉起 node.exe，
/// 路径不定 → 身份按文件名匹配（AC7 双重判据）
pub const CLOUDCLI_EXE_NAME: &str = "node.exe";

/// 栈目录环境文件（腾讯云密钥唯一载体，spec 008 凭据单源化：Caddy DNS-01
/// 续期与 dns_api 调和均读此处；历史名 FRPC_ENV_FILE 随穿透通道退役）
pub const STACK_ENV_FILE: &str = ".env";
/// 对外域名根（权威 NS 查询起点，DNS 对齐/调和的比对口径）
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
