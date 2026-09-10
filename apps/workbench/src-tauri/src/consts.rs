//! 实例常量（plan §4：端口/路径/域名为编译期常量，不进设置文件；
//! 与 sprint0 部署知识对齐，修改须重跑安装脚本，故对用户只读展示）。

/// CloudCLI 控制端口（本机 127.0.0.1）
pub const CLOUDCLI_PORT: u16 = 3001;
/// Caddy HTTPS 端口
pub const CADDY_PORT: u16 = 443;
/// ddns-go 管理 UI 端口
pub const DDNSGO_PORT: u16 = 9876;

/// HTTPS 栈部署目录**默认值**（spec 004 起可由用户在设置中配置，
/// 持久化于 Settings.stack_dir，运行时各组件读配置；重启生效）
pub const DEFAULT_STACK_DIR: &str = r"D:\Software\cloudcli-https";
/// Caddy 配置文件（setup-autostart.ps1 契约：caddy.exe run --config）
pub const CADDYFILE_PATH: &str = r"D:\Software\cloudcli-https\Caddyfile";
/// ddns-go 配置文件（setup-autostart.ps1 契约：ddns-go.exe -c）
pub const DDNSGO_CONFIG_PATH: &str = r"D:\Software\cloudcli-https\ddns-go.yaml";
/// ddns-go 监听参数（-l）
pub const DDNSGO_LISTEN: &str = ":9876";
/// ddns-go 同步间隔秒数（-f）
pub const DDNSGO_INTERVAL_SECS: u32 = 300;

/// 对外域名（HTTPS 反代入口）
pub const DOMAIN: &str = "ai.jackqi.cn";
/// 工作台页面地址
pub const WORKBENCH_URL: &str = "https://ai.jackqi.cn/";
/// CloudCLI 本机地址（打开工作台/健康检查用）
pub const CLOUDCLI_LOCAL_URL: &str = "http://127.0.0.1:3001/";
/// ddns-go 管理 UI 地址
pub const DDNSGO_UI_URL: &str = "http://127.0.0.1:9876/";

/// CloudCLI 监听进程的可执行名：run-server-hidden.ps1 经 npm 全局拉起 node.exe，
/// 路径不定 → 身份按文件名匹配（AC7 双重判据）
pub const CLOUDCLI_EXE_NAME: &str = "node.exe";

// ── 穿透通道（spec 004 §4：与栈目录部署知识对齐，二进制不入库不入打包）──────

/// frpc 可执行名（随包分发于 resources/bin，栈目录回退；spec 004 plan §4.3）
pub const FRPC_EXE_NAME: &str = "frpc.exe";
/// 访问密钥所在环境文件（栈目录 `.env`，spec 004 §4.2）
pub const FRPC_ENV_FILE: &str = ".env";
/// 访问密钥变量名（`.env` 内，宪法 §3：不入日志/事件/设置文件）
pub const SAKURA_KEY_VAR: &str = "SAKURA_FRP_KEY";
/// frpc 运行日志（stdio 追加重定向，失败摘要来源；沿 ddns-go-run.log 先例）
pub const FRPC_LOG_FILE: &str = "frpc-run.log";
/// 对外域名根（权威 NS 查询起点，AC12/13 权威比对口径）
pub const DOMAIN_ROOT: &str = "jackqi.cn";
