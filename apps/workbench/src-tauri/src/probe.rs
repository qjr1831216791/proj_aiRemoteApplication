//! 组件状态探测（AC5/7）。
//!
//! 双重判据（spec §4.3）：
//! 1. 端口处于 TCP Listen（netstat2 = GetExtendedTcpTable）；
//! 2. 监听 PID 的可执行身份匹配期望（sysinfo PID→exe）。
//! 两判据皆中 → running（含外部渠道启动，AC5）；仅端口被占 → port-held
//! 并给出占用进程名（AC7，不误报"运行中"）；无监听 → stopped。
//!
//! 结构：`classify` 为纯函数（单测核心）；`StatusProbe` trait 隔离 Windows
//! API 采集层（netstat2 + sysinfo），mock/真实实现可替换。

use crate::consts::{CADDY_PORT, CLOUDCLI_EXE_NAME, CLOUDCLI_PORT};
use std::net::IpAddr;

/// 组件标识（与前端 plan §4 TS 类型 "cloudcli" | "caddy" 对齐；
/// ddnsgo 已随直连通道退役——spec 008）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentId {
    CloudCli,
    Caddy,
}

impl ComponentId {
    /// 前端序列化名
    pub fn as_str(&self) -> &'static str {
        match self {
            ComponentId::CloudCli => "cloudcli",
            ComponentId::Caddy => "caddy",
        }
    }

    /// 期望监听端口（编译期常量，plan §4）
    pub fn port(&self) -> u16 {
        match self {
            ComponentId::CloudCli => CLOUDCLI_PORT,
            ComponentId::Caddy => CADDY_PORT,
        }
    }
}

/// 监听者身份判据：
/// - ExeName：按可执行文件名匹配（CloudCLI = npm 全局 node.exe，路径不定）
/// - ExePath：按完整路径匹配（Caddy 固定居于栈目录，
///   与 setup-autostart.ps1 的 Check 判据一致）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identity {
    ExeName(String),
    ExePath(String),
}

/// 探测结论（plan §4 ComponentState 的数据来源；starting/failed 由编排层叠加）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeState {
    /// 端口无监听
    Stopped,
    /// 双判据皆中；listen_addrs 供 UI 展示本机/局域网可达性（0.0.0.0/:: 即局域网可达）
    Running { listen_addrs: Vec<IpAddr> },
    /// 端口 Listen 但身份不符：给出占用进程名（AC7）
    PortHeld { process_name: String },
}

/// 一个监听进程的可执行信息（采集层输出；exe 可能取不到，如权限不足）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeProcess {
    pub pid: u32,
    /// 完整路径（ExePath 判据用；None = 取不到）
    pub exe: Option<String>,
    /// 文件名（ExeName 判据 + port-held 展示用）
    pub exe_name: Option<String>,
}

/// 端口监听快照（采集层输出 → classify 输入）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortHolders {
    /// Listen 套接字的本地地址（可能多条：IPv4/IPv6/多网卡）
    pub listen_addrs: Vec<IpAddr>,
    /// 关联监听进程（可能多个或空）
    pub processes: Vec<ProbeProcess>,
}

/// 组件期望身份（集中定义；修改与部署知识同步）。
/// `stack_dir` 为用户可配置的栈目录（spec 004：Settings.stack_dir，重启生效）
pub fn expected_identity(id: ComponentId, stack_dir: &str) -> Identity {
    match id {
        // run-server-hidden.ps1 经 npm 全局拉起 node.exe，安装路径不定 → 按名
        ComponentId::CloudCli => Identity::ExeName(CLOUDCLI_EXE_NAME.into()),
        // caddy 固定居于栈目录（setup-autostart.ps1 的 Check 判据）
        ComponentId::Caddy => Identity::ExePath(format!(r"{stack_dir}\caddy.exe")),
    }
}

/// 纯分类：监听快照 + 期望身份 → 三态（单测核心，Windows API 不进此函数）
pub fn classify(holders: &PortHolders, identity: &Identity) -> ProbeState {
    // 判据一：端口 Listen
    if holders.listen_addrs.is_empty() {
        return ProbeState::Stopped;
    }
    // 判据二：任一监听进程身份匹配 → running（AC5：与启动渠道无关）
    let matched = holders.processes.iter().any(|p| match identity {
        Identity::ExeName(name) => p
            .exe_name
            .as_deref()
            .map(|n| n.eq_ignore_ascii_case(name))
            .unwrap_or(false),
        Identity::ExePath(expect) => p
            .exe
            .as_deref()
            .map(|actual| paths_equal(actual, expect))
            .unwrap_or(false),
    });
    if matched {
        return ProbeState::Running { listen_addrs: holders.listen_addrs.clone() };
    }
    // 端口被占但身份不符 → port-held，展示占用进程名（AC7）
    let process_name = holders
        .processes
        .iter()
        .find_map(|p| p.exe_name.clone())
        .unwrap_or_else(|| "unknown".into());
    ProbeState::PortHeld { process_name }
}

/// 探测接口：`port_holders` 为采集（实现注入便于 mock），`probe` 复用纯 classify。
/// `stack_dir` 由编排器传入（用户可配置，spec 004）。Send + Sync：编排器跨线程持有。
pub trait StatusProbe: Send + Sync {
    /// 采集指定端口的监听快照（Listen 套接字 + 各监听 PID 的 exe）
    fn port_holders(&self, port: u16) -> PortHolders;

    /// 单组件探测 = 采集 + 分类（默认实现即 trait 存在的意义）
    fn probe(&self, id: ComponentId, stack_dir: &str) -> ProbeState {
        let holders = self.port_holders(id.port());
        classify(&holders, &expected_identity(id, stack_dir))
    }
}

// ── Windows 采集实现（netstat2 + sysinfo）────────────────────────────────────

/// Windows 真实探测：GetExtendedTcpTable 取 TCP Listen，PID→exe 走 sysinfo
#[cfg(windows)]
pub struct WindowsProbe;

#[cfg(windows)]
impl StatusProbe for WindowsProbe {
    fn port_holders(&self, port: u16) -> PortHolders {
        use netstat2::{
            AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState,
        };

        // 采集：GetExtendedTcpTable 全量 TCP 项中筛出本端口的 Listen 套接字
        let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
        let mut listen_addrs: Vec<IpAddr> = Vec::new();
        let mut pids: Vec<u32> = Vec::new();
        match netstat2::get_sockets_info(af, ProtocolFlags::TCP) {
            Ok(sockets) => {
                for si in sockets {
                    if let ProtocolSocketInfo::Tcp(tcp) = si.protocol_socket_info {
                        if tcp.local_port == port && tcp.state == TcpState::Listen {
                            listen_addrs.push(tcp.local_addr);
                            pids.extend(si.associated_pids);
                        }
                    }
                }
            }
            Err(e) => {
                // 采集失败按"无监听"处理：宁可显示 stopped 也不误报运行中；
                // 失败留日志供排查（UI 可经状态事件感知刷新异常）
                log::error!("netstat2 采集 TCP 表失败（port={port}）：{e}");
            }
        }
        PortHolders { listen_addrs, processes: exe_of_pids(&pids) }
    }
}

/// PID → 可执行信息（sysinfo；独立函数便于将来按需缓存/复用）
#[cfg(windows)]
fn exe_of_pids(pids: &[u32]) -> Vec<ProbeProcess> {
    use sysinfo::{Pid, ProcessesToUpdate, System};

    // 去重：同端口多 Listen 套接字会给出重复 PID（caddy 双栈 0.0.0.0:443 +
    // [::]:443 实测两份）。sysinfo 0.33 的 Some(pids) 在 remove_dead_processes
    // =true 时对每个 pid 逐一 switch_updated：重复项第二次读到 false 会被当
    // 死进程移除，导致富集全空（caddy 永远误判 port-held）。
    let mut unique: Vec<u32> = pids.to_vec();
    unique.sort_unstable();
    unique.dedup();
    let targets: Vec<Pid> = unique.iter().map(|&p| Pid::from_u32(p)).collect();

    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&targets), true);
    // 输出仍按原始 pids 映射（与监听条目一一对应；classify 只做 any 匹配）
    pids.iter()
        .map(|&p| {
            let proc = sys.process(Pid::from_u32(p));
            let exe = proc
                .and_then(|pr| pr.exe())
                .map(|path| path.to_string_lossy().into_owned());
            // exe 取不到（权限/已退出）时退回进程名；两者皆无 → None（展示 unknown）
            let exe_name = exe
                .as_deref()
                .and_then(file_name_of)
                .or_else(|| proc.map(|pr| pr.name().to_string_lossy().into_owned()));
            ProbeProcess { pid: p, exe, exe_name }
        })
        .collect()
}

// ── 纯工具（路径比较/取文件名，Windows 大小写与分隔符不敏感）──────────────────

/// 取路径文件名（兼容 / 与 \），无分隔符返回 None
pub fn file_name_of(path: &str) -> Option<String> {
    path.rsplit(['\\', '/']).next().filter(|s| !s.is_empty()).map(String::from)
}

/// Windows 路径相等：忽略大小写、统一分隔符、剥离 \\?\ 前缀
pub fn paths_equal(a: &str, b: &str) -> bool {
    fn norm(p: &str) -> String {
        let p = p.strip_prefix(r"\\?\").unwrap_or(p);
        p.replace('/', "\\").to_lowercase()
    }
    norm(a) == norm(b)
}

// ── 单元测试 ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts::DEFAULT_STACK_DIR;
    use std::net::{IpAddr, Ipv4Addr};

    fn addr(a: [u8; 4]) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a[0], a[1], a[2], a[3]))
    }

    fn holder(addrs: &[IpAddr], procs: &[(u32, Option<&str>, Option<&str>)]) -> PortHolders {
        PortHolders {
            listen_addrs: addrs.to_vec(),
            processes: procs
                .iter()
                .map(|&(pid, exe, name)| ProbeProcess {
                    pid,
                    exe: exe.map(String::from),
                    exe_name: name.map(String::from),
                })
                .collect(),
        }
    }

    /// 固定快照的 mock 探测（采集层替身）
    struct FixedProbe(PortHolders);

    impl StatusProbe for FixedProbe {
        fn port_holders(&self, _port: u16) -> PortHolders {
            self.0.clone()
        }
    }

    #[test]
    fn component_ids_ports_and_names() {
        // plan §4：3001 / 443；前端字符串对齐（ddnsgo 随直连通道退役，spec 008）
        assert_eq!(ComponentId::CloudCli.port(), 3001);
        assert_eq!(ComponentId::Caddy.port(), 443);
        assert_eq!(ComponentId::CloudCli.as_str(), "cloudcli");
        assert_eq!(ComponentId::Caddy.as_str(), "caddy");
    }

    #[test]
    fn expected_identity_matches_deployment() {
        // cloudcli = node.exe 按名；caddy = 栈目录全路径（与
        // setup-autostart.ps1 的 Check 判据一致）
        assert_eq!(expected_identity(ComponentId::CloudCli, DEFAULT_STACK_DIR), Identity::ExeName("node.exe".into()));
        assert_eq!(
            expected_identity(ComponentId::Caddy, DEFAULT_STACK_DIR),
            Identity::ExePath(r"D:\Software\cloudcli-https\caddy.exe".into())
        );
        // 与 consts 同源（DEFAULT_STACK_DIR 变更时此处兜底提醒）
        assert!(matches!(expected_identity(ComponentId::Caddy, DEFAULT_STACK_DIR), Identity::ExePath(p) if p.starts_with(DEFAULT_STACK_DIR)));
    }

    #[test]
    fn classify_stopped_when_no_listener() {
        let h = holder(&[], &[(1234, Some(r"C:\x\node.exe"), Some("node.exe"))]);
        assert_eq!(classify(&h, &Identity::ExeName("node.exe".into())), ProbeState::Stopped);
        let h = PortHolders::default();
        assert_eq!(classify(&h, &Identity::ExePath("whatever".into())), ProbeState::Stopped);
    }

    #[test]
    fn classify_running_when_exe_name_matches() {
        // AC5：外部渠道启动的 cloudcli（node.exe）也判 running
        let h = holder(
            &[addr([127, 0, 0, 1]), addr([0, 0, 0, 0])],
            &[(4321, Some(r"C:\Program Files\nodejs\node.exe"), Some("node.exe"))],
        );
        match classify(&h, &Identity::ExeName("node.exe".into())) {
            ProbeState::Running { listen_addrs } => assert_eq!(listen_addrs.len(), 2),
            other => panic!("应为 Running，实际 {other:?}"),
        }
        // 大小写不敏感
        let h2 = holder(&[addr([127, 0, 0, 1])], &[(1, Some(r"C:\n\NODE.EXE"), Some("NODE.EXE"))]);
        assert!(matches!(
            classify(&h2, &Identity::ExeName("node.exe".into())),
            ProbeState::Running { .. }
        ));
    }

    #[test]
    fn classify_running_when_exe_path_matches() {
        // caddy 全路径判据：大小写/分隔符/\\?\ 前缀差异均应匹配
        let expect = r"D:\Software\cloudcli-https\caddy.exe";
        for actual in [
            expect.to_string(),
            r"d:\software\CLOUDCLI-HTTPS\caddy.exe".into(),
            r"D:/Software/cloudcli-https/caddy.exe".into(),
            r"\\?\D:\Software\cloudcli-https\caddy.exe".into(),
        ] {
            let name = file_name_of(&actual).unwrap();
            let h = holder(&[addr([0, 0, 0, 0])], &[(99, Some(actual.as_str()), Some(name.as_str()))]);
            assert!(
                matches!(
                    classify(&h, &Identity::ExePath(expect.into())),
                    ProbeState::Running { .. }
                ),
                "路径 {actual} 应匹配 {expect}"
            );
        }
    }

    #[test]
    fn classify_port_held_shows_process_name() {
        // AC7：无关进程占 3001 → port-held（进程名），不是"运行中"
        let h = holder(
            &[addr([127, 0, 0, 1])],
            &[(77, Some(r"C:\Windows\System32\svchost.exe"), Some("svchost.exe"))],
        );
        assert_eq!(
            classify(&h, &Identity::ExeName("node.exe".into())),
            ProbeState::PortHeld { process_name: "svchost.exe".into() }
        );
        // 同机不同路径的 node.exe 不匹配 ExePath 判据时同样 port-held
        let h2 = holder(
            &[addr([127, 0, 0, 1])],
            &[(78, Some(r"E:\elsewhere\caddy.exe"), Some("caddy.exe"))],
        );
        assert_eq!(
            classify(&h2, &Identity::ExePath(r"D:\Software\cloudcli-https\caddy.exe".into())),
            ProbeState::PortHeld { process_name: "caddy.exe".into() }
        );
    }

    #[test]
    fn classify_port_held_fallback_names() {
        // exe 完整路径取不到：用 exe_name 兜底展示；两者皆无 → unknown（含 PID 语义留给 UI）
        let h = holder(&[addr([127, 0, 0, 1])], &[(55, None, Some("mystery.exe"))]);
        assert_eq!(
            classify(&h, &Identity::ExeName("node.exe".into())),
            ProbeState::PortHeld { process_name: "mystery.exe".into() }
        );
        let h2 = holder(&[addr([127, 0, 0, 1])], &[(56, None, None)]);
        assert_eq!(
            classify(&h2, &Identity::ExeName("node.exe".into())),
            ProbeState::PortHeld { process_name: "unknown".into() }
        );
    }

    #[test]
    fn trait_probe_composes_collect_and_classify() {
        // 默认 probe()：采集 → 分类（经 mock 验证组合逻辑与端口路由）
        let probe = FixedProbe(holder(
            &[addr([127, 0, 0, 1])],
            &[(9, Some(r"C:\Program Files\nodejs\node.exe"), Some("node.exe"))],
        ));
        assert!(matches!(probe.probe(ComponentId::CloudCli, DEFAULT_STACK_DIR), ProbeState::Running { .. }));
        // 同一快照对 caddy 组件（全路径判据）→ port-held
        match probe.probe(ComponentId::Caddy, DEFAULT_STACK_DIR) {
            ProbeState::PortHeld { process_name } => assert_eq!(process_name, "node.exe"),
            other => panic!("应为 PortHeld，实际 {other:?}"),
        }
    }

    #[test]
    fn path_utils() {
        assert_eq!(file_name_of(r"D:\a\b\caddy.exe").as_deref(), Some("caddy.exe"));
        assert_eq!(file_name_of("D:/a/easytier-cli.exe").as_deref(), Some("easytier-cli.exe"));
        assert_eq!(file_name_of("bare").as_deref(), Some("bare"));
        assert!(paths_equal(r"D:\A\b.exe", r"d:\a\B.EXE"));
        assert!(!paths_equal(r"D:\A\b.exe", r"D:\A\c.exe"));
        assert!(paths_equal(r"\\?\C:\x\y.exe", r"C:\x\y.exe"));
    }

    /// 回归（真机发现）：同一端口的多个 Listen 套接字（如 caddy 双栈
    /// 0.0.0.0:443 + [::]:443）会给出重复 PID；sysinfo 0.33 的
    /// `Some(pids)` + remove_dead_processes=true 会对每个 pid 逐一
    /// switch_updated，重复项第二次读到 false 被当死进程移除 → exe 富集
    /// 全空 → caddy 永远误判 port-held（unknown）。本测试以自身进程验证
    /// 重复 PID 不再丢失身份。
    #[test]
    #[cfg(windows)]
    fn exe_of_pids_survives_duplicate_pids() {
        let me = std::process::id();
        let name =
            file_name_of(&std::env::current_exe().unwrap().to_string_lossy()).expect("exe 应有文件名");
        // 重复 PID（caddy 双栈形态）
        let dup = exe_of_pids(&[me, me]);
        assert_eq!(dup.len(), 2, "输出与输入条目一一对应");
        for p in &dup {
            assert_eq!(p.pid, me);
            assert!(p.exe.is_some(), "重复 PID 不应导致富集失败：{p:?}");
            assert_eq!(p.exe_name.as_deref(), Some(name.as_str()));
        }
        // 单次出现（cloudcli 单套接字形态）回归无损
        let single = exe_of_pids(&[me]);
        assert!(single[0].exe.is_some() && single[0].exe_name.is_some());
    }

    /// Windows 集成（轻量）：真实起一个监听走完整 netstat2+sysinfo 链路，
    /// 覆盖 running / port-held / stopped 三分支，结束即清理（无残留进程）。
    #[test]
    #[cfg(windows)]
    fn windows_probe_real_listener_lifecycle() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("临时监听 bind 失败");
        let port = listener.local_addr().unwrap().port();
        let probe = WindowsProbe;

        // 监听者 = 本测试进程：以自身 exe 文件名为身份判据 → running
        let self_exe = std::env::current_exe().expect("取自身路径失败");
        let self_name = file_name_of(&self_exe.to_string_lossy()).expect("exe 应有文件名");
        let holders = probe.port_holders(port);
        assert!(!holders.listen_addrs.is_empty(), "netstat2 应捕获真实 Listen（port={port}）");
        assert!(
            holders.processes.iter().any(|p| p.exe_name.as_deref() == Some(self_name.as_str())),
            "sysinfo 应把监听 PID 归到本测试进程：{:?}",
            holders.processes
        );
        assert!(matches!(
            classify(&holders, &Identity::ExeName(self_name.clone())),
            ProbeState::Running { .. }
        ));

        // 身份判据换成 node.exe（不符）→ port-held 且进程名是本测试进程
        match classify(&holders, &Identity::ExeName("node.exe".into())) {
            ProbeState::PortHeld { process_name } => assert_eq!(process_name, self_name),
            other => panic!("应为 PortHeld，实际 {other:?}"),
        }

        // 释放监听 → stopped
        drop(listener);
        std::thread::sleep(std::time::Duration::from_millis(300)); // 端口表刷新缓冲
        let holders2 = probe.port_holders(port);
        assert!(holders2.listen_addrs.is_empty(), "释放后端口应无监听：{holders2:?}");
        assert_eq!(
            classify(&holders2, &Identity::ExeName("node.exe".into())),
            ProbeState::Stopped
        );
    }
}
