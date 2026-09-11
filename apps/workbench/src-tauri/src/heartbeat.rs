//! 域名心跳（spec 005：周期探测 + 变化发声 + 防抖 + 口径如实标注）。
//!
//! - 60s 周期 HTTPS 探测访问域名（ureq，8s 超时），失败分类：DNS / 连接 / TLS /
//!   超时 / HTTP 状态码
//! - **防抖**（AC6）：连续 ≥2 次失败才判"不可达"，成功 1 次即恢复（尽快解除误报）
//! - **每轮发事件**（载荷含连续失败计数），前端据 healthy 渲染红/绿点；
//!   结论是**本机视角**（AC5：hairpin/CGNAT 下不冒充外部视角，文案如实标注）
//! - 开关（AC7）：enabled 源实时读取，关闭则休眠不发探测

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// 心跳事件名（载荷 [`DomainHealth`]）
pub const EVENT_DOMAIN_HEALTH: &str = "domain://health";
/// 探测周期（spec 005 §4：默认 60s）
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);
/// 单次探测超时
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
/// 标红阈值（AC6 防抖：连续失败次数）
pub const FAILURE_THRESHOLD: u32 = 2;

/// 单次探测结论分类（序列化为纯字符串——此前误用 tag 形态，
/// 前端收到 `{"kind":{"kind":"dns"}}` 渲染成 [object Object]，2026-09-10 真机发现）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthKind {
    /// HTTPS 2xx/3xx
    Ok,
    /// DNS 解析失败
    Dns,
    /// TCP 连接失败
    Connect,
    /// TLS 握手/证书失败
    Tls,
    /// 超时
    Timeout,
    /// HTTP 4xx/5xx（服务在但行为异常）
    Status,
}

/// 心跳快照（事件载荷；healthy 由防抖阈值判定）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainHealth {
    pub healthy: bool,
    pub kind: HealthKind,
    /// HTTP 状态码（kind=status 时有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<u16>,
    pub latency_ms: u64,
    /// 连续失败次数（当前轮）
    pub failures: u32,
    /// 进入当前健康状态的时间戳（epoch ms）
    pub since: u64,
}

/// 单次探测结果（原始，未防抖）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeOutcome {
    pub kind: HealthKind,
    pub code: Option<u16>,
    pub latency_ms: u64,
}

impl ProbeOutcome {
    pub fn is_ok(&self) -> bool {
        self.kind == HealthKind::Ok
    }
}

/// 单次 HTTPS 探测（分层分类：先 DNS，后 HTTP；Windows 采集层隔离便于 mock）
pub fn probe_once(url: &str) -> ProbeOutcome {
    use std::net::ToSocketAddrs;
    // 层 1：DNS 解析（显式做一次，失败归 Dns 而非笼统连接失败）
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string();
    let started = std::time::Instant::now();
    let dns_ok = format!("{host}:443").to_socket_addrs().is_ok();
    if !dns_ok {
        return ProbeOutcome { kind: HealthKind::Dns, code: None, latency_ms: elapsed(started) };
    }
    // 层 2：HTTPS 全链路
    let result = ureq::AgentBuilder::new()
        .timeout(PROBE_TIMEOUT)
        .timeout_connect(PROBE_TIMEOUT)
        .build()
        .get(url)
        .call();
    let latency = elapsed(started);
    match result {
        Ok(resp) => {
            let code = resp.status();
            // 立即关闭 body（只关心可达性，不读内容）
            let _ = resp.into_string();
            if (200..400).contains(&code) {
                ProbeOutcome { kind: HealthKind::Ok, code: None, latency_ms: latency }
            } else {
                ProbeOutcome { kind: HealthKind::Status, code: Some(code), latency_ms: latency }
            }
        }
        Err(ureq::Error::Status(code, resp)) => {
            let _ = resp.into_string();
            if (200..400).contains(&code) {
                ProbeOutcome { kind: HealthKind::Ok, code: None, latency_ms: latency }
            } else {
                ProbeOutcome { kind: HealthKind::Status, code: Some(code), latency_ms: latency }
            }
        }
        Err(_) => {
            // Transport 细分：字符串匹配超时特征（ureq 的 TransportKind 不含 Timeout 变体）
            let text = format!("{result:?}");
            let kind = if text.contains("TimedOut") || text.contains("timed out") {
                HealthKind::Timeout
            } else if text.contains("Dns") {
                HealthKind::Dns
            } else if text.contains("Tls") {
                HealthKind::Tls
            } else {
                HealthKind::Connect
            };
            ProbeOutcome { kind, code: None, latency_ms: latency }
        }
    }
}

fn elapsed(started: std::time::Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

/// 事件出口（装配层接 Tauri emit）
pub trait HealthSink: Send + Sync {
    fn emit_health(&self, health: &DomainHealth);
}

/// 跨模块共享的最近心跳快照（monitor 每轮写入；隧道守护读取做会话卡死自愈）
pub type SharedHealth = Arc<std::sync::Mutex<Option<DomainHealth>>>;

/// 心跳监测器：周期探测 + 防抖 + 变化即发（每轮都发，载荷轻）。
/// 支持 poke：外部（如「通道体检」）触发即时补测，保持状态点与体检结论一致。
pub struct HealthMonitor {
    url: String,
    stop: Arc<AtomicBool>,
    poke: Arc<AtomicBool>,
}

impl HealthMonitor {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            stop: Arc::new(AtomicBool::new(false)),
            poke: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 唤醒心跳立即补测一轮（不等下个周期；通道体检后调用，防红绿不一致）
    pub fn poke(&self) {
        self.poke.store(true, Ordering::Relaxed);
    }

    /// 分段等待：总时长 interval，期间每 100ms 检查一次 poke 提前唤醒
    fn wait_interval(&self) {
        let steps = (HEARTBEAT_INTERVAL.as_millis() / 100) as u32;
        for _ in 0..steps.max(1) {
            if self.stop.load(Ordering::Relaxed) || self.poke.swap(false, Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// 启动心跳线程（装配层调用；enabled 源实时读设置，false 时本轮跳过——AC7；
    /// shared 每轮写入最新快照，供隧道守护自愈判定）
    pub fn spawn(
        self: &Arc<Self>,
        sink: Arc<dyn HealthSink>,
        enabled: Arc<dyn Fn() -> bool + Send + Sync>,
        shared: SharedHealth,
    ) {
        let me = Arc::clone(self);
        std::thread::Builder::new()
            .name("wb-domain-heartbeat".into())
            .spawn(move || {
                let mut failures: u32 = 0;
                let mut healthy = true;
                let mut since = now_ms();
                loop {
                    if me.stop.load(Ordering::Relaxed) {
                        return;
                    }
                    if enabled() {
                        let outcome = probe_once(&me.url);
                        if outcome.is_ok() {
                            failures = 0;
                        } else {
                            failures = failures.saturating_add(1);
                        }
                        // AC6 防抖：连续 ≥2 次失败才判不健康；恢复即时
                        let now_healthy = failures < FAILURE_THRESHOLD;
                        if now_healthy != healthy {
                            healthy = now_healthy;
                            since = now_ms();
                        }
                        let snapshot = DomainHealth {
                            healthy,
                            kind: outcome.kind,
                            code: outcome.code,
                            latency_ms: outcome.latency_ms,
                            failures,
                            since,
                        };
                        if let Ok(mut slot) = shared.lock() {
                            *slot = Some(snapshot.clone());
                        }
                        sink.emit_health(&snapshot);
                    }
                    me.wait_interval();
                }
            })
            .expect("心跳线程创建失败");
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── 单元测试（防抖判定为纯逻辑；HTTP 不覆盖）────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_threshold_two_failures() {
        // AC6：1 次失败仍是健康（防抖）；连续 2 次才判不健康
        let mut failures = 0;
        let mut healthy;
        for outcome in ["Ok", "Status", "Connect"] {
            if outcome == "Ok" {
                failures = 0;
            } else {
                failures += 1;
            }
            healthy = failures < FAILURE_THRESHOLD;
            match outcome {
                "Ok" => assert!(healthy),
                "Status" => assert!(healthy, "第 1 次失败防抖内仍健康"),
                _ => assert!(!healthy, "连续 2 次失败应判不健康"),
            }
        }
        // 恢复即时
        failures = 0;
        healthy = failures < FAILURE_THRESHOLD;
        assert!(healthy);
    }
}
