//! 组网通道（spec 007 mesh）：EasyTier 私有组网（legacy 模式——T2 实测定案，
//! network_secret 派生传输加密；secure-mode 升级路径见 plan §7-R4）。
//!
//! 本模块承载：随包二进制完整性校验（T1，供应链防线 plan §7-R6）、
//! config.toml 渲染与前置校验（T4，唯一渲染路径 plan §4.3）、网段冲突检测、
//! `--check-config` 办后校验封装。服务管理与状态探询随 T7/T8 扩展。

use crate::consts::{
    EASYTIER_CLI_EXE_NAME, EASYTIER_CLI_SHA256, EASYTIER_CORE_EXE_NAME, EASYTIER_CORE_SHA256,
    PACKET_DLL_NAME, PACKET_DLL_SHA256, WINDIVERT_SYS_NAME, WINDIVERT_SYS_SHA256,
    WINTUN_DLL_NAME, WINTUN_DLL_SHA256,
};
use crate::dns_api::sha256_hex;
use crate::settings::MeshConfig;
use serde::Serialize;
use std::net::Ipv4Addr;
use std::path::Path;

/// 栈目录下组网子目录名（plan §4.2：`<stack>/easytier/`）
pub const MESH_DIR: &str = "easytier";
/// network_secret 文件名（set-mesh-secret.ps1 写入，AC8：不进命令行/设置/日志）
pub const NETWORK_SECRET_FILE: &str = "network-secret";
/// config.toml 文件名（工作台唯一渲染）
pub const CONFIG_FILE: &str = "config.toml";
/// 宿主机在组网内的显示名（成员 peer 表 hostname 列；T2 实测字段名）
pub const MESH_HOSTNAME: &str = "ai-remote-workbench";
/// 收敛后的监听端口（默认六协议 11010-11013 全开 → 仅 tcp/udp；
/// T2 实测同机双实例默认端口冲突 fatal 的教训固化）
pub const MESH_LISTEN_PORT: u16 = 11010;

/// 校验目录内五个随包文件（easytier-core.exe / easytier-cli.exe / wintun.dll /
/// packet.dll / WinDivert64.sys）的 SHA256 与版本锁定值一致。
///
/// 调用方：栈目录落位复制前（T7，防篡改源）、装机向导组网分支（T13，办后校验）。
/// 失败返回首个不符项的可读原因（文件缺失或哈希不符，含文件名）——不含密钥
/// 类敏感信息，可直接透出 UI（spec AC4 口径）。
pub fn verify_easytier_binaries(dir: &Path) -> Result<(), String> {
    let checks: [(&str, &str); 5] = [
        (EASYTIER_CORE_EXE_NAME, EASYTIER_CORE_SHA256),
        (EASYTIER_CLI_EXE_NAME, EASYTIER_CLI_SHA256),
        (WINTUN_DLL_NAME, WINTUN_DLL_SHA256),
        (PACKET_DLL_NAME, PACKET_DLL_SHA256),
        (WINDIVERT_SYS_NAME, WINDIVERT_SYS_SHA256),
    ];
    for (name, expected) in checks {
        let path = dir.join(name);
        let bytes = std::fs::read(&path).map_err(|e| format!("{name} 读取失败：{e}"))?;
        let actual = sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!(
                "{name} SHA256 校验不符（期望 {expected}，实际 {actual}）——疑似被篡改或版本不符"
            ));
        }
    }
    Ok(())
}

// ── config.toml 渲染（plan §4.3；T2 实测字段形态）──────────────────────────

/// TOML 渲染模板（字段顺序即产出顺序：标量在前、表在后——toml crate 要求）。
/// **无 [secure_mode] 段**：legacy 产品形态固化（T2 实测定案，plan §1 降级决策；
/// 升级路径注释见 plan §4.3——加 [secure_mode] enabled+成对 X25519 密钥即可）。
#[derive(Serialize)]
struct ConfigToml<'a> {
    hostname: &'a str,
    ipv4: &'a str,
    dhcp: bool,
    latency_first: bool,
    listeners: Vec<String>,
    network_identity: NetworkIdentityToml<'a>,
    /// serde 重命名 peer → 产出 [[peer]] 数组表（peers 逐条）
    #[serde(rename = "peer")]
    peers: Vec<PeerToml<'a>>,
}

#[derive(Serialize)]
struct NetworkIdentityToml<'a> {
    network_name: &'a str,
    network_secret: &'a str,
}

#[derive(Serialize)]
struct PeerToml<'a> {
    uri: &'a str,
}

/// 渲染 config.toml（纯函数）。调用方须先过 [`render_config_checked`]
/// （密钥就绪 + 配置校验），直接调用本函数仅用于测试。
fn render_config(cfg: &MeshConfig, secret: &str) -> Result<String, String> {
    let doc = ConfigToml {
        hostname: MESH_HOSTNAME,
        ipv4: &cfg.virtual_ip,
        dhcp: false,
        latency_first: true,
        listeners: vec![
            format!("tcp://0.0.0.0:{MESH_LISTEN_PORT}"),
            format!("udp://0.0.0.0:{MESH_LISTEN_PORT}"),
        ],
        network_identity: NetworkIdentityToml {
            network_name: &cfg.network_name,
            network_secret: secret,
        },
        peers: cfg
            .peers
            .iter()
            .map(|uri| PeerToml { uri })
            .collect::<Vec<_>>(),
    };
    toml::to_string_pretty(&doc).map_err(|e| format!("config.toml 渲染失败：{e}"))
}

/// 渲染入口（AC9 新语义：密钥就绪强制）。network_secret 缺失/为空 → 拒绝渲染
/// 并给脚本指引；配置字段非法 → 拒绝（AC11 校验）。secret 仅进入返回的文件
/// 内容（AC8：文件注入、命令行无密钥——binPath 由 T7 构造并断言）。
pub fn render_config_checked(cfg: &MeshConfig, secret: &str) -> Result<String, String> {
    let secret = secret.trim();
    if secret.is_empty() {
        return Err("组网密钥未配置：请先运行 set-mesh-secret.ps1 写入密钥（设置区有入口指引）".into());
    }
    validate_mesh_config(cfg)?;
    render_config(cfg, secret)
}

/// 读栈目录 network-secret 文件（None = 未配置 → 调用方拒绝渲染，AC9）
pub fn read_network_secret(stack_dir: &str) -> Option<String> {
    let path = Path::new(stack_dir).join(MESH_DIR).join(NETWORK_SECRET_FILE);
    let content = std::fs::read_to_string(path).ok()?;
    let trimmed = content.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

// ── MeshConfig 校验（AC11：非空/IP/网段/peers URI）─────────────────────────

/// 校验组网配置（纯函数）：网络名非空、虚拟 IP 合法且属于虚拟网段、
/// CIDR 格式合法、peers 非空且每条为 `scheme://host[:port]` 形态。
pub fn validate_mesh_config(cfg: &MeshConfig) -> Result<(), String> {
    if cfg.network_name.trim().is_empty() {
        return Err("网络名不能为空".into());
    }
    let ip: Ipv4Addr = cfg
        .virtual_ip
        .parse()
        .map_err(|_| format!("虚拟 IP 不合法：{}", cfg.virtual_ip))?;
    let cidr = parse_cidr(&cfg.virtual_cidr)
        .ok_or_else(|| format!("虚拟网段不合法（应为 x.x.x.x/nn 形态）：{}", cfg.virtual_cidr))?;
    if !cidr_contains(cidr, ip) {
        return Err(format!(
            "虚拟 IP {} 不在网段 {} 内",
            cfg.virtual_ip, cfg.virtual_cidr
        ));
    }
    if cfg.peers.is_empty() {
        return Err("对端节点列表不能为空（至少一条，默认社区节点）".into());
    }
    for uri in &cfg.peers {
        if !valid_peer_uri(uri) {
            return Err(format!("对端 URI 不合法（应为 scheme://host:port）：{uri}"));
        }
    }
    Ok(())
}

/// 对端 URI 宽松校验：`scheme://host[:port]`，无空白（格式细节由 easytier
/// --check-config 办后校验兜底，此处只拦明显错形）
fn valid_peer_uri(uri: &str) -> bool {
    match uri.split_once("://") {
        Some((scheme, rest)) => {
            !scheme.is_empty() && !rest.is_empty() && !uri.chars().any(char::is_whitespace)
        }
        None => false,
    }
}

// ── 网段冲突检测（plan §7-R7 / §6）─────────────────────────────────────────

/// 解析 CIDR 字符串 → (网络地址, 前缀长度)；非法返回 None
fn parse_cidr(s: &str) -> Option<(Ipv4Addr, u8)> {
    let (ip, prefix) = s.split_once('/')?;
    let ip: Ipv4Addr = ip.parse().ok()?;
    let prefix: u8 = prefix.parse().ok()?;
    (prefix <= 32).then_some((ip, prefix))
}

/// IP 是否落在 CIDR 内（prefix=0 全含；防 32-prefix=32 的移位溢出）
fn cidr_contains((net, prefix): (Ipv4Addr, u8), ip: Ipv4Addr) -> bool {
    if prefix == 0 {
        return true;
    }
    let mask = u32::MAX << (32 - prefix);
    u32::from(net) & mask == u32::from(ip) & mask
}

/// 虚拟网段 × 物理网卡 IPv4 冲突检测（纯函数，返回冲突的物理 IP 列表）。
///
/// 物理网卡前缀长度不可零成本获取，取保守近似双判据：
/// ① 物理 IP 落在虚拟 CIDR 内（精确，覆盖 /24 常规）；
/// ② 物理 IP 与虚拟 IP 的私有段自然前缀重合（10.x/172.16-31.x 比前两段、
///   192.168.x 比前三段——对 /16 以上大网保守报冲突）。
/// 已知漏报：物理 10.0.0.0/8 大网 + 虚拟 10.126.x（前两段不同）——个人机
/// 罕见，T2 实测本机 /24；网段可编辑兜底。
pub fn detect_subnet_conflict(
    physical: &[Ipv4Addr],
    virtual_ip: Ipv4Addr,
    virtual_cidr: &str,
) -> Vec<Ipv4Addr> {
    let Some(cidr) = parse_cidr(virtual_cidr) else {
        return Vec::new(); // 格式非法由 validate_mesh_config 报，此处不重复
    };
    physical
        .iter()
        .copied()
        .filter(|&ip| cidr_contains(cidr, ip) || same_private_prefix(ip, virtual_ip))
        .collect()
}

/// 私有段自然前缀重合判定（判据②的粒度：A/B 类私有段比前两段，C 类比前三段）
fn same_private_prefix(a: Ipv4Addr, b: Ipv4Addr) -> bool {
    let (a, b) = (u32::from(a).to_be_bytes(), u32::from(b).to_be_bytes());
    let a_private = |o: [u8; 4]| {
        if o[0] == 10 {
            2
        } else if o[0] == 172 && (16..=31).contains(&o[1]) {
            2
        } else if o[0] == 192 && o[1] == 168 {
            3
        } else {
            0
        }
    };
    match (a_private(a), a_private(b)) {
        (2, 2) => a[1] == b[1],
        (3, 3) => a[2] == b[2],
        _ => false,
    }
}

/// 本机物理网卡 IPv4 列表（生产侧采集；单测注入固定列表走纯函数）。
/// `list_afinet_netifas` 返回 (网卡名, IpAddr)，过滤回环与 IPv6。
#[cfg(windows)]
pub fn local_ipv4_addrs() -> Vec<Ipv4Addr> {
    local_ip_address::list_afinet_netifas()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, addr)| match addr {
            std::net::IpAddr::V4(v4) if !v4.is_loopback() => Some(v4),
            _ => None,
        })
        .collect()
}

// ── --check-config 办后校验（T9 mesh_apply_config 调用）────────────────────

/// 调 `easytier-core --check-config` 验证配置文件（办后校验，plan §5.1）。
/// GUI 子系统进程：stdout 不附着控制台，但 stderr 管道可读（T2 实测重定向
/// 可捕获日志）；判定以退出码为准，stderr 摘要做失败原因。
pub fn check_config(core_exe: &Path, config_path: &Path) -> Result<(), String> {
    let out = std::process::Command::new(core_exe)
        .arg("--check-config")
        .arg("-c")
        .arg(config_path)
        .output()
        .map_err(|e| format!("easytier-core 启动失败（{}）：{e}", core_exe.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&out.stderr);
        let detail = detail.lines().last().unwrap_or("").trim();
        Err(format!("配置校验未通过：{detail}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 随包目录实测：resources/bin 五文件哈希应全部通过（真文件，发布构建同源）
    #[test]
    fn bundled_binaries_pass_verification() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("bin");
        assert!(dir.is_dir(), "resources/bin 不存在：{dir:?}");
        verify_easytier_binaries(&dir).expect("随包 easytier 文件校验应通过");
    }

    /// 文件缺失 → 返回含文件名的可读错误（不 panic）
    #[test]
    fn missing_file_reports_error() {
        let dir = std::env::temp_dir().join("et-verify-missing-test");
        std::fs::create_dir_all(&dir).unwrap();
        let err = verify_easytier_binaries(&dir)
            .expect_err("空目录校验应失败");
        assert!(
            err.contains(EASYTIER_CORE_EXE_NAME),
            "错误信息应含文件名：{err}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 哈希不符（内容被篡改）→ 拒绝并给出期望/实际值
    #[test]
    fn tampered_file_fails_hash() {
        let dir = std::env::temp_dir().join("et-verify-tamper-test");
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            EASYTIER_CORE_EXE_NAME,
            EASYTIER_CLI_EXE_NAME,
            WINTUN_DLL_NAME,
            PACKET_DLL_NAME,
            WINDIVERT_SYS_NAME,
        ] {
            std::fs::write(dir.join(name), b"tampered").unwrap();
        }
        let err = verify_easytier_binaries(&dir)
            .expect_err("篡改内容校验应失败");
        assert!(err.contains("SHA256 校验不符"), "应报哈希不符：{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    // ── T4：渲染产物断言 ──────────────────────────────────────────────────

    fn sample_config() -> crate::settings::MeshConfig {
        crate::settings::MeshConfig {
            network_name: "office-net".into(),
            virtual_ip: "10.126.126.1".into(),
            virtual_cidr: "10.126.126.0/24".into(),
            peers: vec![
                "tcp://sh.vomiku.com:7910".into(),
                "udp://backup.example.com:11010".into(),
            ],
        }
    }

    /// 渲染产物：实测字段形态齐全（hostname/ipv4/dhcp/listeners/[network_identity]/
    /// [[peer]] 逐条），secret 进入文件内容（AC8：文件注入）
    #[test]
    fn rendered_config_contains_all_fields() {
        let toml = render_config_checked(&sample_config(), "  s3cret-value  \n")
            .expect("合法配置应渲染成功");
        // secret 前后空白被规整后注入
        assert!(toml.contains("network_secret = \"s3cret-value\""), "{toml}");
        assert!(toml.contains("network_name = \"office-net\""), "{toml}");
        assert!(toml.contains("hostname = \"ai-remote-workbench\""), "{toml}");
        assert!(toml.contains("ipv4 = \"10.126.126.1\""), "{toml}");
        assert!(toml.contains("dhcp = false"), "{toml}");
        assert!(toml.contains("latency_first = true"), "{toml}");
        assert!(toml.contains("tcp://0.0.0.0:11010"), "listeners 收敛 tcp：{toml}");
        assert!(toml.contains("udp://0.0.0.0:11010"), "listeners 收敛 udp：{toml}");
        assert_eq!(toml.matches("[[peer]]").count(), 2, "peers 逐条产出：{toml}");
        assert!(toml.contains("tcp://sh.vomiku.com:7910"), "{toml}");
        assert!(toml.contains("udp://backup.example.com:11010"), "{toml}");
    }

    /// legacy 产品形态固化：渲染产物不含 [secure_mode] 段（T2 实测定案，
    /// AC9 新语义落点；升级路径见 plan §4.3 注释）
    #[test]
    fn rendered_config_has_no_secure_mode_section() {
        let toml = render_config_checked(&sample_config(), "s3cret").unwrap();
        assert!(!toml.contains("secure_mode"), "legacy 形态不应含 secure_mode：{toml}");
    }

    /// AC9 新语义：secret 缺失/空白 → 拒绝渲染 + 脚本指引
    #[test]
    fn render_rejects_missing_secret_with_guidance() {
        for empty in ["", "   ", "\n"] {
            let err = render_config_checked(&sample_config(), empty)
                .expect_err("空密钥应拒绝");
            assert!(err.contains("set-mesh-secret.ps1"), "错误应含脚本指引：{err}");
        }
    }

    /// AC11 校验分支：网络名空 / IP 非法 / IP 不在网段 / peers 空 / URI 错形
    #[test]
    fn validate_mesh_config_rejects_bad_fields() {
        let mut cfg = sample_config();

        cfg.network_name = "  ".into();
        assert!(validate_mesh_config(&cfg).unwrap_err().contains("网络名"));

        cfg = sample_config();
        cfg.virtual_ip = "10.126.126.999".into();
        assert!(validate_mesh_config(&cfg).unwrap_err().contains("虚拟 IP"));

        cfg = sample_config();
        cfg.virtual_cidr = "10.200.0.0/24".into();
        assert!(validate_mesh_config(&cfg).unwrap_err().contains("不在网段"));

        cfg = sample_config();
        cfg.virtual_cidr = "10.126.126.0/33".into();
        assert!(validate_mesh_config(&cfg).unwrap_err().contains("网段不合法"));

        cfg = sample_config();
        cfg.peers = vec![];
        assert!(validate_mesh_config(&cfg).unwrap_err().contains("对端"));

        cfg = sample_config();
        cfg.peers = vec!["sh.vomiku.com:7910".into()];
        assert!(validate_mesh_config(&cfg).unwrap_err().contains("URI"));

        // 默认配置本身合法（默认值体检）
        assert!(validate_mesh_config(&crate::settings::MeshConfig::default()).is_ok());
    }

    // ── T4：网段冲突检测 ──────────────────────────────────────────────────

    fn ip(s: &str) -> Ipv4Addr {
        s.parse().unwrap()
    }

    /// 判据①：物理 IP 落在虚拟 CIDR 内（含 /24 之外的边界：/16、/32、/0）
    #[test]
    fn conflict_when_physical_ip_inside_virtual_cidr() {
        let vip = ip("10.126.126.1");
        let hits = detect_subnet_conflict(&[ip("192.168.3.13"), ip("10.126.126.5")], vip, "10.126.126.0/24");
        assert_eq!(hits, vec![ip("10.126.126.5")]);

        // /16 网段：相邻 /24 不冲突、同 /16 冲突
        assert!(detect_subnet_conflict(&[ip("10.126.200.5")], vip, "10.126.0.0/16").len() == 1);
        assert!(detect_subnet_conflict(&[ip("10.127.0.5")], vip, "10.126.0.0/16").is_empty());

        // 边界：/32 只含自身、/0 全含
        assert!(detect_subnet_conflict(&[ip("1.2.3.4")], vip, "10.126.126.1/32").is_empty());
        assert_eq!(detect_subnet_conflict(&[ip("1.2.3.4")], vip, "0.0.0.0/0").len(), 1);
    }

    /// 判据②：私有段自然前缀重合（大网保守近似）；跨私有段不误伤
    #[test]
    fn conflict_by_private_prefix_heuristic() {
        let vip = ip("10.126.126.1");
        // 物理 10.126.x（前两段同）→ 保守报冲突（真实掩码可能 /8）
        assert_eq!(
            detect_subnet_conflict(&[ip("10.126.3.5")], vip, "10.126.126.0/24"),
            vec![ip("10.126.3.5")]
        );
        // 物理 10.0.0.x（前两段不同）→ 不冲突（/8 大网漏报已知，注释声明）
        assert!(detect_subnet_conflict(&[ip("10.0.0.5")], vip, "10.126.126.0/24").is_empty());
        // 跨私有段：192.168 / 172.16 与 10.x 互不干扰
        assert!(detect_subnet_conflict(&[ip("192.168.3.13"), ip("172.17.0.1")], vip, "10.126.126.0/24").is_empty());
        // 192.168 同 C 段（虚拟若配 192.168.x）
        assert_eq!(
            detect_subnet_conflict(&[ip("192.168.3.13")], ip("192.168.3.1"), "192.168.3.0/24"),
            vec![ip("192.168.3.13")]
        );
        // T2-④ 实测形态：本机 192.168.3.13 × 默认虚拟段 → 无冲突
        assert!(detect_subnet_conflict(&[ip("192.168.3.13")], vip, "10.126.126.0/24").is_empty());
    }

    // ── T4：secret 文件读取 ───────────────────────────────────────────────

    /// 栈目录 network-secret 读取（AC8 注入路径）；缺失/空 → None
    #[test]
    fn read_network_secret_from_stack_dir() {
        let dir = std::env::temp_dir().join("et-secret-test");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read_network_secret(dir.to_str().unwrap()), None, "目录不存在 → None");

        std::fs::create_dir_all(dir.join(MESH_DIR)).unwrap();
        std::fs::write(dir.join(MESH_DIR).join(NETWORK_SECRET_FILE), "  topsecret\n").unwrap();
        assert_eq!(read_network_secret(dir.to_str().unwrap()).as_deref(), Some("topsecret"));

        std::fs::write(dir.join(MESH_DIR).join(NETWORK_SECRET_FILE), "   \n").unwrap();
        assert_eq!(read_network_secret(dir.to_str().unwrap()), None, "空白视为未配置");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── T4：--check-config 集成（真 exe + 渲染产物端到端，防字段名写错）────

    /// 渲染产物过 easytier-core --check-config（resources/bin 真 exe）：
    /// 字段名/结构若与 v2.6.4 不符在此暴露（GUI 子系统退出码判定）
    #[test]
    fn rendered_config_passes_easytier_check_config() {
        let dir = std::env::temp_dir().join("et-checkconfig-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let toml = render_config_checked(&crate::settings::MeshConfig::default(), "it-is-a-secret")
            .expect("渲染失败");
        let cfg_path = dir.join(CONFIG_FILE);
        std::fs::write(&cfg_path, &toml).unwrap();
        let core = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("bin")
            .join(EASYTIER_CORE_EXE_NAME);
        let result = check_config(&core, &cfg_path);
        let _ = std::fs::remove_dir_all(&dir);
        // 校验失败时带 easytier 的 stderr 摘要，便于诊断字段名回归
        result.unwrap_or_else(|e| panic!("渲染产物应通过 --check-config：{e}\n---\n{toml}"));
    }
}
