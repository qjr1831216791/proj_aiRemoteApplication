//! 穿透通道（spec 004）：通道状态机 + frpc 进程管理 + 守护 + DNS 对齐判定。
//!
//! - **frpc 不进 orchestrator 组件表**：生命周期绑定「通道」而非组件卡，
//!   且不监听本地端口，不适配 probe.rs 的「端口+身份」模型（plan §5 订正）。
//! - 纯函数（切换动作/守护决策/DNS 判定/.env 解析）与 Windows 采集隔离，
//!   单测不碰真实进程与网络（宪法 §1）。
//! - access key 只从栈目录 `.env` 读取，不进日志/事件/设置文件
//!   （宪法 §3；spec 004 §4.2「凭证不进 APP 数据」）。

use crate::consts::{
    FRPC_ENV_FILE, FRPC_EXE_NAME, FRPC_LOG_FILE, SAKURA_KEY_VAR, STACK_DIR,
};
use crate::settings::AccessChannel;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 隧道状态事件名（前端 T10 监听；载荷 [`TunnelStatus`]）
pub const EVENT_TUNNEL_STATUS: &str = "tunnel://status";

/// frpc 启动后宽限期：进程刚拉起时按「重连中」展示，超时仍活视为在线（AC2 口径）
pub const FRPC_STARTING_GRACE: Duration = Duration::from_secs(10);
/// 守护轮询周期（AC3：进程被杀后 ≤ 守护周期 + 退避间隔拉起）
pub const GUARD_INTERVAL: Duration = Duration::from_secs(5);
/// 会话卡死自愈阈值（spec 005 扩展：心跳连续不可达次数 + frpc 存活 → 自动重启。
/// 2026-09-10 实测故障形态：进程活着、看板"在线"，但节点登录会话已死反复 EOF）
pub const SELF_HEAL_FAILURES: u32 = 3;

// ── 通道状态机（AC5/6/7/8，纯函数）──────────────────────────────────────────

/// 切换动作集中的一个原子步骤（commands 层按序执行）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchAction {
    /// 停止 ddns-go 托管（穿透生效的必要条件：与 CNAME 抢写记录）
    StopDdnsGo,
    /// 恢复 ddns-go 托管（切回直连）
    StartDdnsGo,
    /// 启动 frpc
    StartFrpc,
    /// 停止 frpc
    StopFrpc,
    /// DNS 记录自动同步（AC12/13 语义升级）：穿透=清 A 建 CNAME；直连=清 CNAME。
    /// 凭证缺失/API 失败由执行层降级为手动指引，不阻断切换
    SyncDns(AccessChannel),
    /// 持久化通道到设置
    Persist(AccessChannel),
}

/// 切换被拒原因（AC7：未配置；AC5/6：重复切换）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchReject {
    /// 穿透未配置（tunnel 配置或访问密钥缺失）→ 前端呈引导态
    NotConfigured { missing: &'static str },
    /// 已在目标通道上
    AlreadyOnTarget,
}

/// 通道切换 → 有序动作集（纯函数，单测核心）。
/// `tunnel_ready` = 配置就绪（tunnel 配置存在 **且** 访问密钥可读）。
pub fn switch_actions(
    current: AccessChannel,
    target: AccessChannel,
    tunnel_ready: bool,
) -> Result<Vec<SwitchAction>, SwitchReject> {
    if target == current {
        return Err(SwitchReject::AlreadyOnTarget);
    }
    if target == AccessChannel::Tunnel && !tunnel_ready {
        return Err(SwitchReject::NotConfigured { missing: "tunnel" });
    }
    Ok(match target {
        // 执行序为「先起新、再停旧」：起新失败（如密钥误填）时旧通道无恙，
        // 不产生半途破碎状态；DNS 同步在机器侧就绪后执行，失败降级手动指引
        AccessChannel::Tunnel => vec![
            SwitchAction::StartFrpc,
            SwitchAction::StopDdnsGo,
            SwitchAction::SyncDns(AccessChannel::Tunnel),
            SwitchAction::Persist(AccessChannel::Tunnel),
        ],
        AccessChannel::Direct => vec![
            SwitchAction::StartDdnsGo,
            SwitchAction::StopFrpc,
            SwitchAction::SyncDns(AccessChannel::Direct),
            SwitchAction::Persist(AccessChannel::Direct),
        ],
    })
}

// ── .env 解析（spec 004 §4.2，纯函数）───────────────────────────────────────

/// 从 `.env` 内容提取 `KEY=VALUE`（忽略注释行/空行；值去首尾空白与成对引号）。
/// 首行 BOM 剥离（set-frp-key.ps1 以 UTF-8 BOM 写出，PowerShell 5.1 回读兼容）。
/// 找不到或值为空返回 None（调用方转为「未配置」引导，不 panic）。
pub fn parse_env_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim().trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        if k.trim() != key {
            continue;
        }
        let v = v.trim();
        let v = v
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(v);
        return if v.is_empty() { None } else { Some(v.to_string()) };
    }
    None
}

// ── DNS 对齐判定（AC12/13：判定为纯函数，采集走 PowerShell Resolve-DnsName）──

/// DNS 对齐结论（指引条显隐的数据源）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DnsAlignment {
    /// CNAME 已指向节点域名 → 穿透通道解析就绪（AC12 指引消失）
    AlignedTunnel,
    /// 无 CNAME 且有 A 记录 → 直连通道解析恢复（AC13 指引消失）
    AlignedDirect,
    /// 存在 CNAME 但目标不符（用户可能填错值，detail 带实际目标）
    MismatchedCname { actual: String },
    /// 既无 CNAME 也无 A（用户删了记录还没配好新值）
    NoRecord,
    /// 查询本身失败（网络/解析器异常），不构成结论
    QueryFailed,
}

/// 由「CNAME 目标（None=无 CNAME 记录）+ 是否存在 A 记录」判定对齐（纯函数）。
pub fn judge_dns(cname_target: Option<&str>, has_a_record: bool, node_domain: &str) -> DnsAlignment {
    match cname_target {
        Some(target) => {
            let norm = |s: &str| s.trim_end_matches('.').to_ascii_lowercase();
            if norm(target) == norm(node_domain) {
                DnsAlignment::AlignedTunnel
            } else {
                DnsAlignment::MismatchedCname { actual: target.to_string() }
            }
        }
        None => {
            if has_a_record {
                DnsAlignment::AlignedDirect
            } else {
                DnsAlignment::NoRecord
            }
        }
    }
}

// ── 守护决策（AC3，纯函数）──────────────────────────────────────────────────

/// 守护退避策略（plan §3.3：5s → 15s → 60s 封顶；稳定 5min 重置）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffPolicy {
    /// 退避阶梯（毫秒）
    pub steps_ms: [u64; 3],
    /// 进程稳定运行多久后重置阶梯（毫秒）
    pub stable_reset_ms: u64,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self { steps_ms: [5_000, 15_000, 60_000], stable_reset_ms: 300_000 }
    }
}

/// 守护决策输入（now/上次重启均为 epoch ms；死亡时长 = now - died_at）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardInput {
    /// 当前是否应运行（穿透模式 ∧ 已启用）
    pub should_run: bool,
    /// 进程当前是否存活
    pub alive: bool,
    /// 距上次守护重启的毫秒数（从未重启 = u64::MAX）
    pub ms_since_restart: u64,
}

/// 守护决策（纯函数）：应运行 ∧ 不存活 ∧ 已过当前档退避间隔 → 重启。
/// 「当前档」由 `consecutive_restarts`（连续重启次数）索引退避阶梯。
pub fn guard_should_restart(
    input: GuardInput,
    consecutive_restarts: u32,
    policy: &BackoffPolicy,
) -> bool {
    if !input.should_run || input.alive {
        return false;
    }
    let step = policy.steps_ms
        [(consecutive_restarts as usize).min(policy.steps_ms.len() - 1)];
    input.ms_since_restart >= step
}

/// 连续重启次数的下一档：进程稳定运行超阈值 → 归零（防长期抖动一直顶格）
pub fn next_restart_streak(stable_ms: u64, streak: u32, policy: &BackoffPolicy) -> u32 {
    if stable_ms >= policy.stable_reset_ms { 0 } else { streak + 1 }
}

// ── 进程操作（Windows 采集隔离，可 mock）────────────────────────────────────

/// frpc 进程操作接口（TunnelManager 依赖注入；Send + Sync）
pub trait FrpcOps: Send + Sync {
    /// frpc 可执行是否存在（缺失 → 前端下载指引，AC5 前置校验）
    fn exe_exists(&self) -> bool;
    /// 读取访问密钥（栈目录 `.env`；None = 未配置）
    fn access_key(&self) -> Option<String>;
    /// 启动 frpc（stdio 追加到 frpc-run.log；隐藏窗口）
    fn spawn(&self, key: &str, tunnel_id: &str) -> std::io::Result<()>;
    /// 停止 frpc（按 exe 路径定位进程并终止）
    fn kill(&self) -> std::io::Result<()>;
    /// 进程是否存活
    fn is_running(&self) -> bool;
    /// frpc 日志尾部（失败摘要来源；不可读返回 None）
    fn log_tail(&self, max_lines: usize) -> Option<String>;
}

/// Windows 真实实现。frpc 随安装包分发（resources/bin，spec 004 plan §4.3 修订）：
/// 解析顺序 = 资源目录优先（开箱自带）→ 栈目录回退（既有部署兼容）。
pub struct WindowsFrpcOps {
    /// 资源 bin 目录（exe 同级 resources/bin 或开发态 src-tauri/resources/bin；
    /// None = 定位失败，仅剩栈目录回退）
    bin_dir: Option<PathBuf>,
}

impl WindowsFrpcOps {
    pub fn new(bin_dir: Option<PathBuf>) -> Self {
        Self { bin_dir }
    }

    /// 候选路径（有序：资源目录 → 栈目录）
    fn candidate_paths(&self) -> Vec<PathBuf> {
        let mut v = Vec::new();
        if let Some(dir) = &self.bin_dir {
            v.push(dir.join(FRPC_EXE_NAME));
        }
        v.push(PathBuf::from(STACK_DIR).join(FRPC_EXE_NAME));
        v
    }

    fn resolve_exe(&self) -> Option<PathBuf> {
        self.candidate_paths().into_iter().find(|p| p.is_file())
    }

    fn env_path(&self) -> PathBuf {
        PathBuf::from(STACK_DIR).join(FRPC_ENV_FILE)
    }

    fn log_path(&self) -> PathBuf {
        PathBuf::from(STACK_DIR).join(FRPC_LOG_FILE)
    }
}

impl FrpcOps for WindowsFrpcOps {
    fn exe_exists(&self) -> bool {
        self.resolve_exe().is_some()
    }

    fn access_key(&self) -> Option<String> {
        let content = std::fs::read_to_string(self.env_path()).ok()?;
        parse_env_value(&content, SAKURA_KEY_VAR)
    }

    fn spawn(&self, key: &str, tunnel_id: &str) -> std::io::Result<()> {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        let exe = self
            .resolve_exe()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "frpc.exe 未找到"))?;
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_path())?;
        let stderr = log.try_clone()?;
        // CREATE_NO_WINDOW（沿 network.rs/scripts.rs 先例）：frpc 是控制台程序，
        // 不隐藏会闪黑窗。密钥只进进程参数（已知暴露面，plan §7），不进本函数日志。
        Command::new(exe)
            .args(["-f", &format!("{key}:{tunnel_id}")])
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr))
            .creation_flags(crate::scripts::CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
    }

    fn kill(&self) -> std::io::Result<()> {
        use sysinfo::{ProcessesToUpdate, System};
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        let mut killed = 0;
        for (_pid, proc) in sys.processes() {
            // 按可执行名匹配（frpc.exe 全局唯一，覆盖资源目录与栈目录两种来源；
            // exe 取不到时 sysinfo 的 name 即文件名，同口径命中）
            let name = proc.name().to_string_lossy();
            let by_exe = proc
                .exe()
                .map(|exe| {
                    crate::probe::file_name_of(&exe.to_string_lossy())
                        .is_some_and(|n| n.eq_ignore_ascii_case(FRPC_EXE_NAME))
                })
                .unwrap_or(false);
            if by_exe || name.eq_ignore_ascii_case(FRPC_EXE_NAME) {
                proc.kill();
                killed += 1;
            }
        }
        if killed == 0 {
            // 无进程也视为成功（幂等停止；调用方以 kill 后 is_running 复核）
            log::debug!("frpc 无匹配进程（幂等停止）");
        }
        Ok(())
    }

    fn is_running(&self) -> bool {
        use sysinfo::{ProcessesToUpdate, System};
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        sys.processes().iter().any(|(_pid, proc)| {
            proc.exe()
                .map(|exe| {
                    crate::probe::file_name_of(&exe.to_string_lossy())
                        .is_some_and(|n| n.eq_ignore_ascii_case(FRPC_EXE_NAME))
                })
                .unwrap_or(false)
                || proc.name().to_string_lossy().eq_ignore_ascii_case(FRPC_EXE_NAME)
                // exe 取不到时按文件名兜底（与 probe.rs 身份降级口径一致）
        })
    }

    fn log_tail(&self, max_lines: usize) -> Option<String> {
        let content = std::fs::read_to_string(self.log_path()).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let start = lines.len().saturating_sub(max_lines);
        if lines.is_empty() { None } else { Some(lines[start..].join("\n")) }
    }
}

// ── 隧道状态与管理器（AC1/2/3/4/11）────────────────────────────────────────

/// 隧道运行状态（看板隧道区数据源；detail 不含密钥）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum TunnelState {
    /// 未配置穿透（settings.tunnel 为 None）
    NotConfigured,
    /// 已配置但停用（tunnelEnabled=false，AC11）
    Disabled,
    /// 未启用所在通道（直连模式下 frpc 不应运行）
    Inactive,
    /// 已启动，宽限期内（连接建立中/重连中）
    Starting,
    /// 进程存活且已过宽限期
    Online,
    /// 进程应运行但已死亡（等待守护拉起）
    Offline,
}

/// 隧道状态快照（事件载荷）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    #[serde(flatten)]
    pub state: TunnelState,
    /// 失败/离线原因摘要（来自日志尾部一行，截断防泄密；不含密钥）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 进入该状态的时间戳（epoch ms）
    pub since: u64,
}

/// 通道感知配置源（装配层接 SettingsState；guard 线程每轮实时读取）
pub trait ChannelSource: Send + Sync {
    /// (当前通道, 穿透是否启用, 隧道 ID)
    fn channel_state(&self) -> (AccessChannel, bool, Option<String>);
}

/// 隧道事件出口（真实实现走 Tauri emit）
pub trait TunnelEventSink: Send + Sync {
    fn emit_tunnel_status(&self, status: &TunnelStatus);
}

/// 隧道管理器：spawn/停止入口 + 守护线程 + 状态快照
pub struct TunnelManager {
    ops: std::sync::Arc<dyn FrpcOps>,
    channel: std::sync::Arc<dyn ChannelSource>,
    events: std::sync::Arc<dyn TunnelEventSink>,
    /// 心跳快照视图（spec 005 自愈判定；None = 心跳未装配，不自愈）
    health_view: Option<Arc<dyn Fn() -> Option<u32> + Send + Sync>>,
    policy: BackoffPolicy,
    inner: Mutex<GuardState>,
    /// 守护线程停止旗标
    stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Debug)]
struct GuardState {
    status: TunnelStatus,
    /// 连续守护重启次数（退避档位索引）
    streak: u32,
    /// 上次守护重启时刻（epoch ms；None = 本轮询周期内无重启）
    last_restart_ms: Option<u64>,
    /// 上次 spawn 时刻（宽限判定用）
    last_spawn_ms: Option<u64>,
}

impl TunnelManager {
    pub fn new(
        ops: std::sync::Arc<dyn FrpcOps>,
        channel: std::sync::Arc<dyn ChannelSource>,
        events: std::sync::Arc<dyn TunnelEventSink>,
        health_view: Option<Arc<dyn Fn() -> Option<u32> + Send + Sync>>,
    ) -> Self {
        let status = TunnelStatus {
            state: TunnelState::NotConfigured,
            detail: None,
            since: now_ms(),
        };
        Self {
            ops,
            channel,
            events,
            health_view,
            policy: BackoffPolicy::default(),
            inner: Mutex::new(GuardState {
                status,
                streak: 0,
                last_restart_ms: None,
                last_spawn_ms: None,
            }),
            stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 当前状态快照（get_tunnel_status 命令数据源）
    pub fn status(&self) -> TunnelStatus {
        self.inner.lock().expect("隧道状态锁中毒").status.clone()
    }

    /// 启动 frpc（切换到穿透 / 停用后重新启用时调用）。
    /// 前置校验失败返回错误文案（可直接展示；不含密钥值）。
    pub fn start(&self) -> Result<(), String> {
        let (_, _, tunnel_id) = self.channel.channel_state();
        let Some(tunnel_id) = tunnel_id else {
            return Err("穿透未配置：请先在设置中填写隧道 ID".into());
        };
        if !self.ops.exe_exists() {
            return Err(format!("未找到 {FRPC_EXE_NAME}：请将其下载后放入 {STACK_DIR}"));
        }
        let Some(key) = self.ops.access_key() else {
            return Err(format!(
                "访问密钥未配置：请在 {STACK_DIR}\\{FRPC_ENV_FILE} 中设置 {SAKURA_KEY_VAR}"
            ));
        };
        self.ops
            .spawn(&key, &tunnel_id)
            .map_err(|e| format!("frpc 启动失败：{e}"))?;
        let now = now_ms();
        let mut g = self.inner.lock().expect("隧道状态锁中毒");
        g.last_spawn_ms = Some(now);
        g.streak = 0;
        g.last_restart_ms = None;
        self.set_state(&mut g, TunnelState::Starting, None, now);
        Ok(())
    }

    /// 停止 frpc（切回直连 / 停用穿透时调用；幂等）
    pub fn stop(&self) {
        let _ = self.ops.kill();
        let mut g = self.inner.lock().expect("隧道状态锁中毒");
        g.last_spawn_ms = None;
        g.last_restart_ms = None;
        g.streak = 0;
        self.set_state(&mut g, TunnelState::Inactive, None, now_ms());
    }

    /// 手动/自动重启 frpc：停止 → 刷新本机 DNS 缓存（切换/重连后旧解析残留会
    /// 让本机测试误判，2026-09-10 实测）→ 重新启动登录。reason 进状态 detail
    pub fn restart(&self, reason: &str) -> Result<(), String> {
        log::info!("重启 frpc：{reason}");
        let _ = self.ops.kill();
        flush_dns_cache();
        std::thread::sleep(Duration::from_millis(500));
        let result = self.start();
        if result.is_ok() {
            let mut g = self.inner.lock().expect("隧道状态锁中毒");
            g.status.detail = Some(reason.to_string());
            self.events.emit_tunnel_status(&g.status);
        }
        result
    }

    /// 启动守护线程（装配层调用一次；返回停止旗标句柄语义由进程退出兜底）
    pub fn spawn_guard(self: &std::sync::Arc<Self>) {
        let me = self.clone();
        let flag = self.stop_flag.clone();
        std::thread::Builder::new()
            .name("wb-tunnel-guard".into())
            .spawn(move || loop {
                if flag.load(Ordering::Relaxed) {
                    return;
                }
                me.guard_tick();
                std::thread::sleep(GUARD_INTERVAL);
            })
            .expect("守护线程创建失败");
    }

    /// 守护单轮：计算状态 → 必要时按退避重启 → 发事件（pub 便于单测直调）
    pub fn guard_tick(&self) {
        let (channel, enabled, tunnel_id) = self.channel.channel_state();
        let now = now_ms();

        // 阶段 1：锁外采集（sysinfo 全量刷新/日志文件读取都是重活，禁止持锁执行；
        // 2026-09-10 死锁复盘：持锁调 restart → start 二次加锁 → 守护线程卡死，
        // 全部状态查询跟着阻塞——IO 一律在锁外，锁内只做状态读写）
        let should_run = channel == AccessChannel::Tunnel && enabled && tunnel_id.is_some();
        let alive = should_run && self.ops.is_running();
        let health_failures = if alive {
            self.health_view.as_ref().and_then(|v| v())
        } else {
            None
        };
        let log_tail = if alive { self.ops.log_tail(8).unwrap_or_default() } else { String::new() };

        // 阶段 2：不应运行的收敛（AC4/AC7/AC8/AC11）——锁外 kill，锁内改状态
        if !should_run {
            if alive {
                log::info!("frpc 不应运行（未配置/非穿透通道/已停用），守护收敛停止");
                let _ = self.ops.kill();
            }
            let mut g = self.inner.lock().expect("隧道状态锁中毒");
            g.last_spawn_ms = None;
            let state = if tunnel_id.is_none() {
                TunnelState::NotConfigured
            } else if channel != AccessChannel::Tunnel {
                TunnelState::Inactive
            } else {
                TunnelState::Disabled
            };
            self.set_state(&mut g, state, None, now);
            return;
        }

        // 阶段 3：应运行——读取决策输入（锁内快照，毫秒级）
        let (streak, last_restart_ms, last_spawn_ms) = {
            let g = self.inner.lock().expect("隧道状态锁中毒");
            (g.streak, g.last_restart_ms, g.last_spawn_ms)
        };

        if !alive {
            // AC3：应运行而死亡 → 退避重启（IO 在锁外，写状态在锁内）
            let ms_since = last_restart_ms.map(|t| now.saturating_sub(t)).unwrap_or(u64::MAX);
            if guard_should_restart(
                GuardInput { should_run: true, alive: false, ms_since_restart: ms_since },
                streak,
                &self.policy,
            ) {
                match (self.ops.access_key(), tunnel_id.clone()) {
                    (Some(key), Some(id)) if self.ops.exe_exists() => {
                        if let Err(e) = self.ops.spawn(&key, &id) {
                            log::error!("守护重启 frpc 失败：{e}");
                        }
                    }
                    _ => {
                        // 密钥/二进制缺失不再盲目重启：转 Offline + 摘要（AC4）
                        let detail = self
                            .ops
                            .log_tail(1)
                            .unwrap_or_else(|| "access key 或 frpc.exe 缺失".into());
                        let mut g = self.inner.lock().expect("隧道状态锁中毒");
                        self.set_state(&mut g, TunnelState::Offline, summarize(&detail), now);
                        return;
                    }
                }
                let mut g = self.inner.lock().expect("隧道状态锁中毒");
                g.last_restart_ms = Some(now);
                g.streak = next_restart_streak(0, streak, &self.policy);
                g.last_spawn_ms = Some(now);
                self.set_state(&mut g, TunnelState::Starting, None, now);
            } else {
                let mut g = self.inner.lock().expect("隧道状态锁中毒");
                self.set_state(&mut g, TunnelState::Offline, None, now);
            }
            return;
        }

        // 存活 + 会话卡死自愈（spec 005 扩展）：frpc 存活但心跳连续 ≥3 次不可达
        // → 进程活着而隧道会话已死（EOF 卡死形态）→ **锁外**执行 restart
        //（restart 内部要拿状态锁——持锁调用即死锁，2026-09-10 真机复现）
        if let Some(f) = health_failures.filter(|f| *f >= SELF_HEAL_FAILURES) {
            log::warn!("心跳连续 {f} 次不可达且 frpc 存活：判定会话卡死，自动重启");
            if let Err(e) = self.restart("会话无响应，已自动重启（心跳持续不可达）") {
                log::error!("自愈重启失败：{e}");
            }
            return;
        }

        // 登录实况（frpc 日志判定，修「进程在=在线」的误导：登录需数秒~数十秒，
        // EOF 重试期间进程活着但隧道未通——需求方 2026-09-10 启动实测）
        let verdict = classify_log(&log_tail);
        let since_spawn = last_spawn_ms.map(|t| now.saturating_sub(t)).unwrap_or(u64::MAX);
        let (state, detail) = match verdict {
            LogVerdict::Failure => (
                TunnelState::Starting,
                Some("节点登录失败，自动重试中".to_string()),
            ),
            _ => {
                let s = if since_spawn <= FRPC_STARTING_GRACE.as_millis() as u64 {
                    TunnelState::Starting
                } else {
                    TunnelState::Online
                };
                (s, None)
            }
        };
        let mut g = self.inner.lock().expect("隧道状态锁中毒");
        if let Some(t) = last_spawn_ms {
            if now.saturating_sub(t) > self.policy.stable_reset_ms && g.streak != 0 {
                g.streak = 0;
            }
        }
        self.set_state(&mut g, state, detail, now);
    }

    fn set_state(&self, g: &mut GuardState, state: TunnelState, detail: Option<String>, now: u64) {
        let changed = g.status.state != state || g.status.detail != detail;
        if changed {
            g.status = TunnelStatus { state, detail, since: now };
            self.events.emit_tunnel_status(&g.status);
        } else {
            g.status.since = now;
        }
    }
}

/// frpc 日志尾部判定（进程存活时的登录实况；「进程在 ≠ 隧道通」，
/// 2026-09-10 启动实测：frpc 启动到登录成功间隔 67 秒，EOF 重试期间更久）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogVerdict {
    /// 最后一条关键日志是「隧道启动成功」→ 隧道在线
    Success,
    /// 最后一条关键日志是「登录节点失败」→ 登录重试中（显示重连中）
    Failure,
    /// 无关键日志（刚拉起/日志不可读）→ 按宽限期逻辑
    Unknown,
}

/// 从日志尾部逐行取**最后一条**关键记录判定登录实况（纯函数；中英文关键词
/// 都覆盖——frpc 输出语言随环境）
pub fn classify_log(tail: &str) -> LogVerdict {
    let mut verdict = LogVerdict::Unknown;
    for line in tail.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("登录节点失败") || lower.contains("login to server failed") {
            verdict = LogVerdict::Failure;
        } else if line.contains("隧道启动成功") {
            verdict = LogVerdict::Success;
        }
    }
    verdict
}

/// 日志行摘要：截断 + 防密钥意外泄漏（contains key 才整行丢弃，宁缺毋泄）
fn summarize(line: &str) -> Option<String> {    let line = line.trim();
    if line.is_empty() || line.contains(SAKURA_KEY_VAR) {
        return None;
    }
    Some(line.chars().take(160).collect())
}

/// 刷新本机 DNS 缓存（重连/切换后旧解析残留会让本机测试误判；无害操作）
fn flush_dns_cache() {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("ipconfig")
        .arg("/flushdns")
        .creation_flags(crate::scripts::CREATE_NO_WINDOW)
        .output();
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── 单元测试（纯函数；Windows 采集层不在覆盖范围，宪法 §1）──────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_direct_to_tunnel_starts_frpc_before_stopping_ddns() {
        // AC5：直连 → 穿透 = 起 frpc + 停 ddns-go + DNS 同步 + 持久化。
        // 「先起新再停旧」：起 frpc 失败（如密钥误填）时直连通道无恙。
        let actions = switch_actions(AccessChannel::Direct, AccessChannel::Tunnel, true)
            .expect("就绪时应允许切换");
        assert_eq!(
            actions,
            vec![
                SwitchAction::StartFrpc,
                SwitchAction::StopDdnsGo,
                SwitchAction::SyncDns(AccessChannel::Tunnel),
                SwitchAction::Persist(AccessChannel::Tunnel),
            ]
        );
    }

    #[test]
    fn switch_tunnel_to_direct_starts_ddns_before_stopping_frpc() {
        // AC6：穿透 → 直连 = 恢复 ddns-go + 停 frpc + DNS 同步 + 持久化（同「先起新」序）
        let actions = switch_actions(AccessChannel::Tunnel, AccessChannel::Direct, true)
            .expect("切回直连无需穿透就绪");
        assert_eq!(
            actions,
            vec![
                SwitchAction::StartDdnsGo,
                SwitchAction::StopFrpc,
                SwitchAction::SyncDns(AccessChannel::Direct),
                SwitchAction::Persist(AccessChannel::Direct),
            ]
        );
    }

    #[test]
    fn switch_rejects_unconfigured_and_noop() {
        // AC7：未配置 → NotConfigured；同通道 → AlreadyOnTarget
        assert_eq!(
            switch_actions(AccessChannel::Direct, AccessChannel::Tunnel, false),
            Err(SwitchReject::NotConfigured { missing: "tunnel" })
        );
        assert_eq!(
            switch_actions(AccessChannel::Direct, AccessChannel::Direct, true),
            Err(SwitchReject::AlreadyOnTarget)
        );
        // 切回直连永远允许（回退路径不设门槛）
        assert!(switch_actions(AccessChannel::Tunnel, AccessChannel::Direct, false).is_ok());
    }

    #[test]
    fn env_value_parsing() {
        // 常规 / 注释 / 空值 / 引号 / 无该键
        let env = "# 注释\nSAKURA_FRP_KEY=abc123\nOTHER=x\nEMPTY=\nQUOTED=\"hi there\"\n";
        assert_eq!(parse_env_value(env, "SAKURA_FRP_KEY").as_deref(), Some("abc123"));
        assert_eq!(parse_env_value(env, "OTHER").as_deref(), Some("x"));
        assert_eq!(parse_env_value(env, "EMPTY"), None, "空值视为未配置");
        assert_eq!(parse_env_value(env, "QUOTED").as_deref(), Some("hi there"));
        assert_eq!(parse_env_value(env, "MISSING"), None);
        assert_eq!(parse_env_value("SAKURA_FRP_KEY = spaced ", "SAKURA_FRP_KEY").as_deref(), Some("spaced"));
        // BOM 兼容（set-frp-key.ps1 以 UTF-8 BOM 写出 .env）
        assert_eq!(
            parse_env_value("\u{feff}# comment\nSAKURA_FRP_KEY=bommed", "SAKURA_FRP_KEY").as_deref(),
            Some("bommed"),
            "首行 BOM 不应破坏键值解析"
        );
    }

    #[test]
    fn dns_judgement_covers_four_states() {
        // AC12：CNAME 对齐 / 目标不符；AC13：A 恢复 / 无记录
        assert_eq!(
            judge_dns(Some("frp-can.com"), false, "frp-can.com"),
            DnsAlignment::AlignedTunnel
        );
        // 大小写与尾点不敏感（权威应答形态）
        assert_eq!(
            judge_dns(Some("FRP-Can.COM."), false, "frp-can.com"),
            DnsAlignment::AlignedTunnel
        );
        assert_eq!(
            judge_dns(Some("other.example.com"), true, "frp-can.com"),
            DnsAlignment::MismatchedCname { actual: "other.example.com".into() }
        );
        assert_eq!(judge_dns(None, true, "frp-can.com"), DnsAlignment::AlignedDirect);
        assert_eq!(judge_dns(None, false, "frp-can.com"), DnsAlignment::NoRecord);
    }

    #[test]
    fn guard_restarts_only_when_dead_and_elapsed() {
        let policy = BackoffPolicy::default();
        // 存活 → 不重启
        assert!(!guard_should_restart(
            GuardInput { should_run: true, alive: true, ms_since_restart: 0 },
            0,
            &policy
        ));
        // 不应运行（直连/停用）→ 不重启
        assert!(!guard_should_restart(
            GuardInput { should_run: false, alive: false, ms_since_restart: u64::MAX },
            0,
            &policy
        ));
        // 死亡但未过第一档 5s → 等待
        assert!(!guard_should_restart(
            GuardInput { should_run: true, alive: false, ms_since_restart: 4_999 },
            0,
            &policy
        ));
        // 过档 → 重启（从未重启 = u64::MAX 立即）
        assert!(guard_should_restart(
            GuardInput { should_run: true, alive: false, ms_since_restart: 5_000 },
            0,
            &policy
        ));
        assert!(guard_should_restart(
            GuardInput { should_run: true, alive: false, ms_since_restart: u64::MAX },
            0,
            &policy
        ));
        // 连续重启后档位抬升：streak=1 → 15s 档，5s 不足，15s 足
        assert!(!guard_should_restart(
            GuardInput { should_run: true, alive: false, ms_since_restart: 5_000 },
            1,
            &policy
        ));
        assert!(guard_should_restart(
            GuardInput { should_run: true, alive: false, ms_since_restart: 15_000 },
            1,
            &policy
        ));
        // 封顶：streak 再高也不越 60s
        assert!(!guard_should_restart(
            GuardInput { should_run: true, alive: false, ms_since_restart: 15_000 },
            9,
            &policy
        ));
        assert!(guard_should_restart(
            GuardInput { should_run: true, alive: false, ms_since_restart: 60_000 },
            9,
            &policy
        ));
    }

    #[test]
    fn log_verdict_takes_last_matching_line() {
        // 真实日志形态（含时间戳与中文）：以最后一条关键记录为准
        let success_tail = "2026/09/10 09:42:42 [I] 正在连接节点 [frp-can.com, tcp]\n\
                            2026/09/10 09:42:42 [W] 登录节点失败, 请检查网络连接\n\
                            HTTPS 隧道启动成功, 绑定到域名 [ai.jackqi.cn]";
        assert_eq!(classify_log(success_tail), LogVerdict::Success);
        // EOF 循环：启动成功在前、失败在后 → 以失败为准（重连中）
        let failure_tail = "HTTPS 隧道启动成功, 绑定到域名 [ai.jackqi.cn]\n\
                            2026/09/10 09:40:51 [W] 登录节点失败, 请检查网络连接. 错误信息: EOF";
        assert_eq!(classify_log(failure_tail), LogVerdict::Failure);
        assert_eq!(classify_log(""), LogVerdict::Unknown);
        assert_eq!(classify_log("2026/09/10 [I] 检查更新中..."), LogVerdict::Unknown);
    }

    #[test]
    fn restart_streak_resets_after_stable_run() {
        let policy = BackoffPolicy::default();
        assert_eq!(next_restart_streak(0, 2, &policy), 3);
        assert_eq!(next_restart_streak(299_999, 2, &policy), 3);
        assert_eq!(next_restart_streak(300_000, 2, &policy), 0, "稳定 5min 归零");
    }

    #[test]
    fn summarize_truncates_and_blocks_key_leak() {
        assert_eq!(summarize("  connect to node failed  ").as_deref(), Some("connect to node failed"));
        assert_eq!(summarize(""), None);
        assert_eq!(summarize("SAKURA_FRP_KEY=oops"), None, "含密钥变量名的行整行丢弃");
        let long = "x".repeat(500);
        assert_eq!(summarize(&long).map(|s| s.len()), Some(160), "超长行截断");
    }
}
