//! 访问地址（T13/T15：get_urls 命令与 open_external 的数据源，plan §2）。
//!
//! - 局域网 IP：UDP connect 默认路由技巧（零依赖 ~10 行，plan §8 裁决）——
//!   `connect` 只让内核选路并绑定对应网卡地址，不实际发包；
//! - 地址口径与 sprint0 menu.ps1 菜单 3 对齐：
//!   本机 `http://localhost:3001` / 局域网 `http://<ip>:3001` / 域名 `https://ai.jackqi.cn`。

use crate::consts::{CLOUDCLI_PORT, DDNSGO_UI_URL, WORKBENCH_URL};
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

/// 三端访问地址（get_urls 载荷，plan §5.1 `{ local, lan, domain }`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessUrls {
    pub local: String,
    pub lan: String,
    pub domain: String,
}

/// LAN 探测目标（公网 DNS 地址，仅用于选路，不发包）
pub const LAN_PROBE_TARGET: (&str, u16) = ("8.8.8.8", 80);

/// 探测默认路由上的本机局域网 IPv4（失败返回 None：无路由/离线等）
pub fn detect_lan_ip() -> Option<Ipv4Addr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect(LAN_PROBE_TARGET).ok()?;
    match sock.local_addr().ok()? {
        SocketAddr::V4(v4) => {
            let ip = *v4.ip();
            // 未指定/回环地址无展示价值，视为探测失败
            (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
        }
        SocketAddr::V6(_) => None,
    }
}

/// 纯构造：探测结果 → 三端地址（探测失败时局域网行回落本机地址）
pub fn build_urls(lan: Option<Ipv4Addr>) -> AccessUrls {
    let local = format!("http://localhost:{CLOUDCLI_PORT}/");
    let lan = lan
        .map(|ip| format!("http://{ip}:{CLOUDCLI_PORT}/"))
        .unwrap_or_else(|| local.clone());
    AccessUrls {
        local,
        lan,
        domain: WORKBENCH_URL.to_string(),
    }
}

/// 真实入口：探测 + 构造
pub fn access_urls() -> AccessUrls {
    build_urls(detect_lan_ip())
}

// ── 外部打开目标（open_external 命令的 kind，plan §5.1）─────────────────────

/// 打开目标类别（前端序列化：snake_case 字符串）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalKind {
    /// 工作台页面（域名入口，托盘菜单共用）
    Workbench,
    /// 本机地址
    Local,
    /// 局域网地址
    Lan,
    /// 域名地址（与 Workbench 同 URL：AC19 地址区「打开」按钮）
    Domain,
    /// ddns-go 管理页
    DdnsAdmin,
}

/// 纯映射：kind → 完整 URL（单测覆盖全分支）
pub fn external_url(kind: ExternalKind, urls: &AccessUrls) -> String {
    match kind {
        ExternalKind::Workbench | ExternalKind::Domain => urls.domain.clone(),
        ExternalKind::Local => urls.local.clone(),
        ExternalKind::Lan => urls.lan.clone(),
        ExternalKind::DdnsAdmin => DDNSGO_UI_URL.to_string(),
    }
}

// ── 单元测试 ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_urls_shapes_align_with_sprint0_menu() {
        // menu.ps1 菜单 3 口径：localhost:3001 / <ip>:3001 / https://ai.jackqi.cn
        let urls = build_urls(Some(Ipv4Addr::new(192, 168, 1, 5)));
        assert_eq!(urls.local, "http://localhost:3001/");
        assert_eq!(urls.lan, "http://192.168.1.5:3001/");
        assert_eq!(urls.domain, "https://ai.jackqi.cn/");
    }

    #[test]
    fn build_urls_falls_back_to_local_when_lan_unresolved() {
        // 探测失败：局域网行回落本机地址（UI 仍可用，不显示空串）
        let urls = build_urls(None);
        assert_eq!(urls.lan, urls.local);
        assert_eq!(urls.local, "http://localhost:3001/");
    }

    #[test]
    fn detect_lan_ip_never_returns_unusable_addr() {
        // 环境相关（离线可能 None）：返回 Some 时必须可用（非未指定/回环）
        if let Some(ip) = detect_lan_ip() {
            assert!(!ip.is_unspecified() && !ip.is_loopback(), "探测到不可用地址：{ip}");
        }
    }

    #[test]
    fn external_url_maps_every_kind() {
        let urls = build_urls(Some(Ipv4Addr::new(10, 0, 0, 2)));
        assert_eq!(external_url(ExternalKind::Workbench, &urls), urls.domain);
        assert_eq!(external_url(ExternalKind::Domain, &urls), urls.domain);
        assert_eq!(external_url(ExternalKind::Local, &urls), "http://localhost:3001/");
        assert_eq!(external_url(ExternalKind::Lan, &urls), "http://10.0.0.2:3001/");
        assert_eq!(external_url(ExternalKind::DdnsAdmin, &urls), DDNSGO_UI_URL);
    }

    #[test]
    fn external_kind_deserializes_snake_case() {
        assert_eq!(
            serde_json::from_str::<ExternalKind>("\"ddns_admin\"").unwrap(),
            ExternalKind::DdnsAdmin
        );
        assert_eq!(
            serde_json::from_str::<ExternalKind>("\"workbench\"").unwrap(),
            ExternalKind::Workbench
        );
        assert!(serde_json::from_str::<ExternalKind>("\"nope\"").is_err());
    }
}
