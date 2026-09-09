//! 编排器（T8：启动侧 + 状态轮询；AC1/3/6 逻辑部分，plan §3.2 启动时序）。
//!
//! - 组件注册表：三组件固定顺序（cloudcli/caddy/ddnsgo），CloudCLI 走
//!   `run-server-hidden.ps1`（复用 T7 CommandSpec/退出码语义），Caddy/ddns-go
//!   原生守卫拉起（dispatch，stdout→日志）
//! - start_all/start_one：守卫检查（probe 已 running → 跳过，AC3 幂等）→
//!   置 starting → 就绪轮询（2s 间隔）至 running；60s 未就绪 → failed 并附
//!   日志尾部（AC1：CloudCLI 为 %TEMP%\cloudcli.log）
//! - 在途跟踪（`InFlightTracker`）：同组件启动进行中拒绝重复派发；提供
//!   取消句柄/等待接口（spec §4.4：停止先等待或取消在途启动，T9/T10 消费）
//! - 状态事件：变化时经 `StatusEventSink` 发 `status://changed`
//!   （ComponentStatus[]，schema 按 plan §4）；轮询器前台 2s 全组件刷新（AC4）
//!
//! 依赖全部经 trait 注入（probe/executor/event sink/log tail），单测零真实进程。

use crate::lang::Lang;
use crate::probe::{ComponentId, ProbeState, StatusProbe};
use crate::scripts::{
    caddy_run, ddns_go_run, interpret_exit, run_server_hidden, CommandExecutor, CommandSpec,
    ExecOutcome, Script, ScriptOutcome,
};
use crate::stop::{ProcessOps, StopConfig, StopOutcome};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 状态事件名（plan §5.1：payload = ComponentStatus[]）
pub const EVENT_STATUS_CHANGED: &str = "status://changed";
/// 前台轮询间隔（plan §8 轮询降频裁决：前台 2s ≤ 5s，AC4）
pub const FOREGROUND_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// 启动就绪超时（AC1：60s 未就绪判失败）
pub const START_READY_TIMEOUT: Duration = Duration::from_secs(60);
/// 失败详情附带的日志尾部行数
pub const LOG_TAIL_LINES: usize = 30;

/// 三组件注册表（固定顺序 = status 数组顺序）
pub const COMPONENT_ORDER: [ComponentId; 3] =
    [ComponentId::CloudCli, ComponentId::Caddy, ComponentId::DdnsGo];

// ── 数据模型（plan §4）──────────────────────────────────────────────────────

/// 组件五态（plan §4 ComponentState；starting/failed 由编排层叠加）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentState {
    Stopped,
    Starting,
    Running,
    /// 端口被非本组件进程占用（AC7，告警色）
    PortHeld,
    Failed,
}

/// 事件载荷（plan §4 ComponentStatus）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub id: ComponentId,
    pub state: ComponentState,
    pub port: u16,
    /// 失败原因 / 占用进程名等（缺省不序列化）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 进入该状态的时间戳（epoch ms）
    pub since: u64,
}

impl Serialize for ComponentId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ComponentId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "cloudcli" => Ok(ComponentId::CloudCli),
            "caddy" => Ok(ComponentId::Caddy),
            "ddnsgo" => Ok(ComponentId::DdnsGo),
            other => Err(serde::de::Error::custom(format!(
                "未知组件标识：{other}（合法：cloudcli/caddy/ddnsgo）"
            ))),
        }
    }
}

/// start_one 拒绝原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    /// 同组件启动进行中（在途互斥）
    AlreadyInFlight,
    /// 启动工作线程创建失败（OOM 级异常）
    ThreadSpawn(String),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::AlreadyInFlight => write!(f, "该组件已有启动操作进行中"),
            StartError::ThreadSpawn(e) => write!(f, "启动线程创建失败：{e}"),
        }
    }
}

// ── 事件与日志尾部 seam（可 mock）──────────────────────────────────────────

/// 状态事件出口（真实实现走 Tauri emit；单测 mock 断言事件序列）
pub trait StatusEventSink: Send + Sync {
    fn emit_statuses(&self, statuses: &[ComponentStatus]);
}

/// Tauri 事件实现（装配层注入 AppHandle）
pub struct TauriStatusEmitter {
    app: tauri::AppHandle,
}

impl TauriStatusEmitter {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl StatusEventSink for TauriStatusEmitter {
    fn emit_statuses(&self, statuses: &[ComponentStatus]) {
        use tauri::Emitter;
        if let Err(e) = self.app.emit(EVENT_STATUS_CHANGED, statuses.to_vec()) {
            log::error!("发送 {EVENT_STATUS_CHANGED} 失败：{e}");
        }
    }
}

/// 日志尾部读取（失败详情用；单测 mock 注入内容）
pub trait LogTailReader: Send + Sync {
    /// 返回文件末尾 max_lines 行；不可读返回 None
    fn tail(&self, path: &Path, max_lines: usize) -> Option<String>;
}

/// 文件系统实现
#[derive(Default)]
pub struct FsLogTailReader;

impl LogTailReader for FsLogTailReader {
    fn tail(&self, path: &Path, max_lines: usize) -> Option<String> {
        let content = std::fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let start = lines.len().saturating_sub(max_lines);
        let taken = &lines[start..];
        if taken.is_empty() {
            None
        } else {
            Some(taken.join("\n"))
        }
    }
}

/// CloudCLI 运行日志（run-server-hidden.ps1 契约：%TEMP%\cloudcli.log）
pub fn cloudcli_log_path() -> PathBuf {
    std::env::temp_dir().join("cloudcli.log")
}

// ── 在途操作跟踪（spec §4.4：停止先等待或取消在途启动）────────────────────

/// 组件级在途操作登记（当前仅启动）：重复派发互斥 + 取消句柄 + 等待接口。
/// T10 收摊退出流消费 `cancel`/`wait_idle` 消除"退出后组件姗姗来迟"竞态。
#[derive(Default)]
pub struct InFlightTracker {
    cancels: Mutex<HashMap<ComponentId, Arc<AtomicBool>>>,
    idle: Condvar,
}

impl InFlightTracker {
    /// 登记在途操作；已存在 → None（互斥拒绝）
    pub fn try_mark(&self, id: ComponentId) -> Option<Arc<AtomicBool>> {
        let mut map = self.cancels.lock().expect("在途锁中毒");
        if map.contains_key(&id) {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        map.insert(id, flag.clone());
        Some(flag)
    }

    /// 清除登记（操作结束；唤醒全部等待者）
    pub fn clear(&self, id: ComponentId) {
        self.cancels.lock().expect("在途锁中毒").remove(&id);
        self.idle.notify_all();
    }

    pub fn is_busy(&self, id: ComponentId) -> bool {
        self.cancels.lock().expect("在途锁中毒").contains_key(&id)
    }

    /// 请求取消（返回是否存在在途操作）
    pub fn cancel(&self, id: ComponentId) -> bool {
        let map = self.cancels.lock().expect("在途锁中毒");
        match map.get(&id) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// 等待指定组件无在途操作；超时返回 false
    pub fn wait_idle(&self, id: ComponentId, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut map = self.cancels.lock().expect("在途锁中毒");
        loop {
            if !map.contains_key(&id) {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let (guard, _) = self
                .idle
                .wait_timeout(map, deadline - now)
                .expect("在途锁中毒");
            map = guard;
        }
    }
}

// ── 编排器 ─────────────────────────────────────────────────────────────────

/// 编排配置（默认值取自上方常量；单测注入毫秒级值加速状态机）
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// 就绪轮询间隔（默认 2s，前台口径）
    pub poll_interval: Duration,
    /// 启动就绪超时（默认 60s，AC1）
    pub start_timeout: Duration,
    /// 停止管线配置（默认 5s/10s/500ms，spec §4.4/AC2）
    pub stop: StopConfig,
    /// 脚本语言（-Lang 对齐，AC25）
    pub lang: Lang,
    /// 程序日志目录（stdio 重定向与失败尾部来源）
    pub log_dir: PathBuf,
    /// sprint0 脚本目录（None = 定位失败，CloudCLI 启停禁用并给原因）
    pub scripts_dir: Option<PathBuf>,
}

impl OrchestratorConfig {
    pub fn new(lang: Lang, log_dir: PathBuf) -> Self {
        Self {
            poll_interval: FOREGROUND_POLL_INTERVAL,
            start_timeout: START_READY_TIMEOUT,
            stop: StopConfig::default(),
            lang,
            log_dir,
            scripts_dir: None,
        }
    }
}

/// 轮询器句柄（shutdown 停止后台刷新线程）
pub struct PollerHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl PollerHandle {
    /// 停止轮询并等待线程退出
    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// 编排器：启动侧状态机 + 停止管线接入 + 状态轮询 + 事件发射
/// （Clone 为廉价句柄，全 Arc 字段）
#[derive(Clone)]
pub struct Orchestrator {
    cfg: OrchestratorConfig,
    probe: Arc<dyn StatusProbe>,
    executor: Arc<dyn CommandExecutor>,
    procs: Arc<dyn ProcessOps>,
    events: Arc<dyn StatusEventSink>,
    logs: Arc<dyn LogTailReader>,
    statuses: Arc<Mutex<Vec<ComponentStatus>>>,
    tracker: Arc<InFlightTracker>,
}

impl Orchestrator {
    pub fn new(
        cfg: OrchestratorConfig,
        probe: Arc<dyn StatusProbe>,
        executor: Arc<dyn CommandExecutor>,
        procs: Arc<dyn ProcessOps>,
        events: Arc<dyn StatusEventSink>,
        logs: Arc<dyn LogTailReader>,
    ) -> Self {
        let statuses = COMPONENT_ORDER
            .iter()
            .map(|&id| ComponentStatus {
                id,
                state: ComponentState::Stopped,
                port: id.port(),
                detail: None,
                since: now_ms(),
            })
            .collect();
        Self {
            cfg,
            probe,
            executor,
            procs,
            events,
            logs,
            statuses: Arc::new(Mutex::new(statuses)),
            tracker: Arc::new(InFlightTracker::default()),
        }
    }

    /// 当前全组件状态快照（get_status 命令的数据源）
    pub fn statuses(&self) -> Vec<ComponentStatus> {
        self.statuses.lock().expect("状态锁中毒").clone()
    }

    /// 单组件启动（AC3 守卫幂等；AC1 就绪轮询；AC6 失败隔离）。
    /// 立即返回，状态经事件推进；同组件在途 → AlreadyInFlight。
    pub fn start_one(&self, id: ComponentId) -> Result<(), StartError> {
        let cancel = self.tracker.try_mark(id).ok_or(StartError::AlreadyInFlight)?;
        let me = self.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("wb-start-{}", id.as_str()))
            .spawn(move || {
                me.run_start(id, &cancel);
                me.tracker.clear(id);
            });
        match spawned {
            Ok(_) => Ok(()),
            // 线程创建失败：释放登记避免假性在途锁死组件
            Err(e) => {
                self.tracker.clear(id);
                log::error!("启动线程创建失败（{}）：{e}", id.as_str());
                Err(StartError::ThreadSpawn(e.to_string()))
            }
        }
    }

    /// 一键启动：逐组件派发（在途组件跳过，幂等重入）
    pub fn start_all(&self) {
        for id in COMPONENT_ORDER {
            // 单组件失败已隔离为其 failed 态（AC6），此处只吞"在途跳过"
            if let Err(e) = self.start_one(id) {
                log::info!("start_all 跳过组件 {}：{e}", id.as_str());
            }
        }
    }

    /// 手动取消在途启动（T10 收摊消费的取消句柄；无在途返回 false）
    pub fn cancel_start(&self, id: ComponentId) -> bool {
        self.tracker.cancel(id)
    }

    /// 等待在途启动结束（T10 收摊消费的等待接口）
    pub fn wait_start_idle(&self, id: ComponentId, timeout: Duration) -> bool {
        self.tracker.wait_idle(id, timeout)
    }

    /// 全组件刷新（前台轮询周期执行；AC4 外部停止 ≤10s 可见）。
    /// 在途组件跳过（启动线程独占其状态迁移）。
    pub fn refresh_all(&self) {
        for id in COMPONENT_ORDER {
            if self.tracker.is_busy(id) {
                continue;
            }
            let (state, detail) = match self.probe.probe(id) {
                ProbeState::Running { .. } => (ComponentState::Running, None),
                ProbeState::Stopped => (ComponentState::Stopped, None),
                ProbeState::PortHeld { process_name } => (
                    ComponentState::PortHeld,
                    Some(format!("端口 {} 被进程 {process_name} 占用", id.port())),
                ),
            };
            self.set_state(id, state, detail);
        }
    }

    /// 启动前台轮询线程（2s 周期全组件刷新；句柄 shutdown 停止）
    pub fn spawn_poller(&self) -> PollerHandle {
        let me = self.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let join = std::thread::Builder::new()
            .name("wb-status-poller".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    me.refresh_all();
                    std::thread::sleep(me.cfg.poll_interval);
                }
            })
            .expect("轮询线程创建失败");
        PollerHandle { stop, join: Some(join) }
    }

    /// 单组件停止（AC2）：先等待/取消在途启动（spec §4.4 竞态消除），再执行
    /// 停止管线；结论映射状态（复核失败/超时 → failed 附原因与排查命令）。
    pub fn stop_one(&self, id: ComponentId) -> StopOutcome {
        let cfg = &self.cfg.stop;
        // §4.4：停止先等待或取消在途启动，再判定端口
        if self.tracker.cancel(id) {
            log::info!("组件 {} 存在在途启动：已请求取消并等待其退出", id.as_str());
        }
        if !self.tracker.wait_idle(id, cfg.component_timeout) {
            log::warn!(
                "组件 {} 在途启动未在 {}s 内退出：按端口口径继续停止",
                id.as_str(),
                cfg.component_timeout.as_secs()
            );
        }
        let deadline = Instant::now() + cfg.component_timeout;
        let outcome = match id {
            ComponentId::CloudCli => crate::stop::stop_cloudcli(
                self.probe.as_ref(),
                self.procs.as_ref(),
                cfg,
                deadline,
            ),
            ComponentId::Caddy => crate::stop::stop_caddy(
                self.probe.as_ref(),
                self.executor.as_ref(),
                self.procs.as_ref(),
                cfg,
                &self.cfg.log_dir,
                deadline,
            ),
            ComponentId::DdnsGo => crate::stop::stop_ddnsgo(
                self.probe.as_ref(),
                self.procs.as_ref(),
                cfg,
                deadline,
            ),
        };
        match &outcome {
            StopOutcome::AlreadyStopped | StopOutcome::Stopped => {
                self.set_state(id, ComponentState::Stopped, None);
                log::info!("组件 {} 已停止（{:?}）", id.as_str(), outcome);
            }
            StopOutcome::Failed(detail) | StopOutcome::TimedOut(detail) => {
                // AC2：复核失败 → failed 附原因；超时 → 记日志 + 手动排查命令
                log::error!("组件 {} 停止未完全成功：{detail}", id.as_str());
                self.set_state(id, ComponentState::Failed, Some(detail.clone()));
            }
        }
        outcome
    }

    /// 一键停止：三组件并行（plan §3.2 收摊时序；单组件各自 10s 预算互不阻塞）
    pub fn stop_all(&self) -> Vec<(ComponentId, StopOutcome)> {
        let handles: Vec<_> = COMPONENT_ORDER
            .iter()
            .map(|&id| {
                let me = self.clone();
                std::thread::Builder::new()
                    .name(format!("wb-stop-{}", id.as_str()))
                    .spawn(move || (id, me.stop_one(id)))
                    .expect("停止线程创建失败")
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("停止线程崩溃"))
            .collect()
    }

    // ── 内部 ───────────────────────────────────────────────────────────

    /// 启动管线主体（工作线程内执行）：守卫 → starting → 拉起 → 就绪轮询
    fn run_start(&self, id: ComponentId, cancel: &AtomicBool) {
        let begun = Instant::now();
        // 1. 守卫（AC3 幂等）：已运行跳过；被无关进程占如实上报且不拉起（AC7）
        match self.probe.probe(id) {
            ProbeState::Running { .. } => {
                self.set_state(id, ComponentState::Running, None);
                log::info!("组件 {} 已在运行：守卫跳过（AC3 幂等）", id.as_str());
                return;
            }
            ProbeState::PortHeld { process_name } => {
                self.set_state(
                    id,
                    ComponentState::PortHeld,
                    Some(format!(
                        "端口 {} 被进程 {process_name} 占用，未拉起",
                        id.port()
                    )),
                );
                log::warn!(
                    "组件 {} 端口被 {} 占用：不拉起（AC7）",
                    id.as_str(),
                    process_name
                );
                return;
            }
            ProbeState::Stopped => {}
        }
        // 停止请求已抵达：放弃启动且不迁移状态（spec §4.4 竞态消除）
        if cancel.load(Ordering::Relaxed) {
            log::info!("组件 {} 启动在派发前被取消", id.as_str());
            return;
        }
        // 2. 置 starting（AC1：点击启动即进入"启动中"）
        self.set_state_if_active(cancel, id, ComponentState::Starting, None);
        // 3. 拉起：CloudCLI 走脚本消费 exit code（AC1）；Caddy/ddns-go 原生守卫拉起
        match id {
            ComponentId::CloudCli => {
                let Some(dir) = self.cfg.scripts_dir.clone() else {
                    self.set_state_if_active(
                        cancel,
                        id,
                        ComponentState::Failed,
                        Some(
                            "sprint0 脚本目录不可用（spec §4.5）：CloudCLI 启动已禁用，请检查脚本目录设置"
                                .into(),
                        ),
                    );
                    return;
                };
                let spec = run_server_hidden(&dir, self.cfg.lang, &self.cfg.log_dir);
                match self.executor.execute(&spec) {
                    ExecOutcome::Exited(0) => {}
                    ExecOutcome::Exited(code) => {
                        // AC1：exit 1 映射"未安装/不可用"提示
                        let detail = outcome_detail(&interpret_exit(Script::RunServerHidden, code));
                        self.set_state_if_active(cancel, id, ComponentState::Failed, Some(detail));
                        return;
                    }
                    ExecOutcome::TimedOut => {
                        self.set_state_if_active(
                            cancel,
                            id,
                            ComponentState::Failed,
                            Some("启动脚本超时未返回（run-server-hidden.ps1），详情见程序日志".into()),
                        );
                        return;
                    }
                    ExecOutcome::SpawnFailed(e) => {
                        self.set_state_if_active(
                            cancel,
                            id,
                            ComponentState::Failed,
                            Some(format!("启动脚本无法执行：{e}")),
                        );
                        return;
                    }
                }
            }
            ComponentId::Caddy => {
                if !self.dispatch_native(cancel, &caddy_run(&self.cfg.log_dir), id) {
                    return;
                }
            }
            ComponentId::DdnsGo => {
                if !self.dispatch_native(cancel, &ddns_go_run(&self.cfg.log_dir), id) {
                    return;
                }
            }
        }
        // 4. 就绪轮询：poll_interval 间隔至 running；start_timeout 未就绪 → failed（AC1）
        let deadline = Instant::now() + self.cfg.start_timeout;
        loop {
            if cancel.load(Ordering::Relaxed) {
                // 停止管线接管：不再迁移状态，由停止路径判定终态（§4.4）
                log::info!("组件 {} 启动轮询被取消（停止管线接管）", id.as_str());
                return;
            }
            match self.probe.probe(id) {
                ProbeState::Running { .. } => {
                    self.set_state_if_active(cancel, id, ComponentState::Running, None);
                    log::info!("组件 {} 就绪（耗时 {:?}）", id.as_str(), begun.elapsed());
                    return;
                }
                ProbeState::PortHeld { process_name } => {
                    // 启动窗口内端口被他人抢占：如实转 port-held（AC6/AC7）
                    self.set_state_if_active(
                        cancel,
                        id,
                        ComponentState::PortHeld,
                        Some(format!("端口 {} 被进程 {process_name} 占用", id.port())),
                    );
                    return;
                }
                ProbeState::Stopped => {
                    if Instant::now() >= deadline {
                        let detail = self.timeout_detail(id);
                        self.set_state_if_active(cancel, id, ComponentState::Failed, Some(detail));
                        log::error!("组件 {} 启动就绪超时（AC1）", id.as_str());
                        return;
                    }
                }
            }
            std::thread::sleep(self.cfg.poll_interval);
        }
    }

    /// 原生拉起（Caddy/ddns-go）：dispatch 派发即返；失败置 failed
    fn dispatch_native(&self, cancel: &AtomicBool, spec: &CommandSpec, id: ComponentId) -> bool {
        match self.executor.dispatch(spec) {
            Ok(()) => true,
            Err(e) => {
                log::error!("组件 {} 原生拉起失败：{e}", id.as_str());
                self.set_state_if_active(
                    cancel,
                    id,
                    ComponentState::Failed,
                    Some(format!("拉起失败：{e}")),
                );
                false
            }
        }
    }

    /// 就绪超时的失败详情：附组件对应日志尾部（AC1：CloudCLI 为 %TEMP%\cloudcli.log）
    fn timeout_detail(&self, id: ComponentId) -> String {
        let secs = self.cfg.start_timeout.as_secs();
        let paths: Vec<PathBuf> = match id {
            ComponentId::CloudCli => vec![cloudcli_log_path()],
            ComponentId::Caddy => vec![
                self.cfg.log_dir.join("caddy.err.log"),
                self.cfg.log_dir.join("caddy.log"),
            ],
            ComponentId::DdnsGo => vec![
                self.cfg.log_dir.join("ddns-go.err.log"),
                self.cfg.log_dir.join("ddns-go.log"),
            ],
        };
        let tails: Vec<String> = paths
            .iter()
            .filter_map(|p| {
                self.logs
                    .tail(p, LOG_TAIL_LINES)
                    .map(|t| format!("—— {} 尾部 ——\n{t}", p.display()))
            })
            .collect();
        if tails.is_empty() {
            format!(
                "启动超时（{secs}s 未就绪）。日志暂不可读：{}",
                paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join("、")
            )
        } else {
            format!("启动超时（{secs}s 未就绪）。\n{}", tails.join("\n"))
        }
    }

    /// 状态迁移 + 变化时发事件；state 不变但 detail 变化也发（如占用者更名）
    fn set_state(&self, id: ComponentId, state: ComponentState, detail: Option<String>) {
        let mut all = self.statuses.lock().expect("状态锁中毒");
        let slot = all.iter().position(|s| s.id == id).expect("组件未注册");
        let st = &mut all[slot];
        if st.state == state && st.detail == detail {
            return; // 无变化不发声
        }
        if st.state != state {
            st.since = now_ms();
        }
        st.state = state;
        st.detail = detail;
        let snapshot = all.clone();
        drop(all);
        self.events.emit_statuses(&snapshot);
    }

    /// 取消后不再迁移状态（停止管线接管，避免覆盖其判定）
    fn set_state_if_active(
        &self,
        cancel: &AtomicBool,
        id: ComponentId,
        state: ComponentState,
        detail: Option<String>,
    ) {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        self.set_state(id, state, detail);
    }
}

/// epoch 毫秒时间戳
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 脚本退出语义 → 失败详情文案
fn outcome_detail(outcome: &ScriptOutcome) -> String {
    match outcome {
        ScriptOutcome::Unavailable(msg) => format!("启动失败：{msg}"),
        ScriptOutcome::Failed(code) => {
            format!("启动脚本异常退出（code={code}），详情见程序日志目录")
        }
        // Success/TimedOut 在调用处分派，不会进入此处
        ScriptOutcome::Success | ScriptOutcome::TimedOut => String::new(),
    }
}

// ── 测试基建（orchestrator 与 stop 单测共用）──────────────────────────────

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::probe::PortHolders;
    use std::collections::VecDeque;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::atomic::AtomicUsize;

    /// 跨 trait 调用顺序日志（stop 管线断言"树收集→杀→复核"顺序的关键）
    #[derive(Default)]
    pub struct CallLog {
        entries: Mutex<Vec<String>>,
    }

    impl CallLog {
        pub fn record(&self, entry: impl Into<String>) {
            self.entries.lock().unwrap().push(entry.into());
        }

        pub fn snapshot(&self) -> Vec<String> {
            self.entries.lock().unwrap().clone()
        }

        /// 首个包含 needle 的条目序号（顺序断言用）
        pub fn position(&self, needle: &str) -> Option<usize> {
            self.snapshot().iter().position(|e| e.contains(needle))
        }
    }

    /// 可编剧本 mock 探测：启动侧按 id 队列、停止侧按端口队列；
    /// 队列耗尽后粘住最后一个返回值（建模"进程持续运行/持续占用"）。
    pub struct ScriptedProbe {
        log: Arc<CallLog>,
        states: Mutex<HashMap<ComponentId, VecDeque<ProbeState>>>,
        sticky_states: Mutex<HashMap<ComponentId, ProbeState>>,
        holders: Mutex<HashMap<u16, VecDeque<PortHolders>>>,
        sticky_holders: Mutex<HashMap<u16, PortHolders>>,
        probe_calls: AtomicUsize,
        port_calls: AtomicUsize,
    }

    impl ScriptedProbe {
        pub fn new() -> Self {
            Self {
                log: Arc::new(CallLog::default()),
                states: Mutex::new(HashMap::new()),
                sticky_states: Mutex::new(HashMap::new()),
                holders: Mutex::new(HashMap::new()),
                sticky_holders: Mutex::new(HashMap::new()),
                probe_calls: AtomicUsize::new(0),
                port_calls: AtomicUsize::new(0),
            }
        }

        /// 启动侧剧本：按序消费，耗尽粘住末值
        pub fn enqueue(&self, id: ComponentId, states: &[ProbeState]) {
            let mut q = self.states.lock().unwrap();
            q.entry(id).or_default().extend(states.iter().cloned());
        }

        /// 启动侧粘性设定（清空队列后固定返回该值）
        pub fn pin(&self, id: ComponentId, state: ProbeState) {
            self.states.lock().unwrap().remove(&id);
            self.sticky_states.lock().unwrap().insert(id, state);
        }

        /// 停止侧粘性设定（port_holders 固定返回该快照）
        pub fn pin_holders(&self, port: u16, holders: PortHolders) {
            self.holders.lock().unwrap().remove(&port);
            self.sticky_holders.lock().unwrap().insert(port, holders);
        }

        /// 停止侧剧本：按序消费（定位→…→复核 各阶段快照），耗尽粘住末值
        pub fn enqueue_holders(&self, port: u16, holders: &[PortHolders]) {
            self.holders
                .lock()
                .unwrap()
                .entry(port)
                .or_default()
                .extend(holders.iter().cloned());
        }

        pub fn probe_calls(&self) -> usize {
            self.probe_calls.load(Ordering::SeqCst)
        }

        pub fn port_calls(&self) -> usize {
            self.port_calls.load(Ordering::SeqCst)
        }

        /// 共享调用日志（与执行器/进程 mock 联合断言顺序）
        pub fn log(&self) -> Arc<CallLog> {
            self.log.clone()
        }
    }

    impl StatusProbe for ScriptedProbe {
        fn port_holders(&self, port: u16) -> PortHolders {
            self.port_calls.fetch_add(1, Ordering::SeqCst);
            self.log.record(format!("probe:port={port}"));
            let popped = self
                .holders
                .lock()
                .unwrap()
                .get_mut(&port)
                .and_then(VecDeque::pop_front);
            match popped {
                Some(h) => {
                    self.sticky_holders.lock().unwrap().insert(port, h.clone());
                    h
                }
                None => self
                    .sticky_holders
                    .lock()
                    .unwrap()
                    .get(&port)
                    .cloned()
                    .unwrap_or_default(),
            }
        }

        fn probe(&self, id: ComponentId) -> ProbeState {
            self.probe_calls.fetch_add(1, Ordering::SeqCst);
            let popped = self
                .states
                .lock()
                .unwrap()
                .get_mut(&id)
                .and_then(VecDeque::pop_front);
            match popped {
                Some(s) => {
                    self.sticky_states.lock().unwrap().insert(id, s.clone());
                    s
                }
                None => self
                    .sticky_states
                    .lock()
                    .unwrap()
                    .get(&id)
                    .cloned()
                    .unwrap_or(ProbeState::Stopped),
            }
        }
    }

    /// 事件记录 mock（断言事件序列）
    #[derive(Default)]
    pub struct MockEventSink {
        pub emissions: Mutex<Vec<Vec<ComponentStatus>>>,
    }

    impl StatusEventSink for MockEventSink {
        fn emit_statuses(&self, statuses: &[ComponentStatus]) {
            self.emissions.lock().unwrap().push(statuses.to_vec());
        }
    }

    /// 日志尾部 mock：路径 → 内容
    #[derive(Default)]
    pub struct MockLogTail {
        pub files: Mutex<HashMap<PathBuf, String>>,
    }

    impl MockLogTail {
        pub fn set(&self, path: PathBuf, content: &str) {
            self.files.lock().unwrap().insert(path, content.into());
        }
    }

    impl LogTailReader for MockLogTail {
        fn tail(&self, path: &Path, _max_lines: usize) -> Option<String> {
            self.files.lock().unwrap().get(path).cloned()
        }
    }

    /// 执行器 mock：记录规格 + 可编退出码（粘性末值）+ 可选调用日志
    pub struct MockExecutor {
        pub executed: Mutex<Vec<CommandSpec>>,
        pub dispatched: Mutex<Vec<CommandSpec>>,
        exits: Mutex<VecDeque<ExecOutcome>>,
        dispatch_err: Option<String>,
        log: Option<Arc<CallLog>>,
    }

    impl MockExecutor {
        pub fn new() -> Self {
            Self {
                executed: Mutex::new(vec![]),
                dispatched: Mutex::new(vec![]),
                exits: Mutex::new(VecDeque::new()),
                dispatch_err: None,
                log: None,
            }
        }

        /// 绑定调用日志（与 probe/procs mock 联合断言跨 trait 顺序）
        pub fn with_log(log: Arc<CallLog>) -> Self {
            let mut me = Self::new();
            me.log = Some(log);
            me
        }

        pub fn with_exits(exits: &[ExecOutcome]) -> Self {
            let me = Self::new();
            *me.exits.lock().unwrap() = exits.iter().cloned().collect();
            me
        }

        pub fn with_dispatch_err(err: &str) -> Self {
            let mut me = Self::new();
            me.dispatch_err = Some(err.into());
            me
        }
    }

    impl Default for MockExecutor {
        fn default() -> Self {
            Self::new()
        }
    }

    impl CommandExecutor for MockExecutor {
        fn execute(&self, spec: &CommandSpec) -> ExecOutcome {
            self.executed.lock().unwrap().push(spec.clone());
            if let Some(log) = &self.log {
                log.record(format!("exec:{}", spec.command_line()));
            }
            self.exits
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(ExecOutcome::Exited(0))
        }

        fn dispatch(&self, spec: &CommandSpec) -> Result<(), String> {
            self.dispatched.lock().unwrap().push(spec.clone());
            if let Some(log) = &self.log {
                log.record(format!("dispatch:{}", spec.command_line()));
            }
            match &self.dispatch_err {
                Some(e) => Err(e.clone()),
                None => Ok(()),
            }
        }
    }

    /// 进程操作 mock：可编树/路径映射，调用全部记录日志（顺序断言用）
    pub struct MockProcessOps {
        log: Option<Arc<CallLog>>,
        descendants_map: Mutex<HashMap<u32, Vec<u32>>>,
        by_exe: Mutex<HashMap<String, Vec<u32>>>,
    }

    impl MockProcessOps {
        pub fn new() -> Self {
            Self {
                log: None,
                descendants_map: Mutex::new(HashMap::new()),
                by_exe: Mutex::new(HashMap::new()),
            }
        }

        pub fn with_log(log: Arc<CallLog>) -> Self {
            let mut me = Self::new();
            me.log = Some(log);
            me
        }

        pub fn set_descendants(&self, pid: u32, children: &[u32]) {
            self.descendants_map
                .lock()
                .unwrap()
                .insert(pid, children.to_vec());
        }

        pub fn set_by_exe(&self, exe: &str, pids: &[u32]) {
            self.by_exe.lock().unwrap().insert(exe.to_string(), pids.to_vec());
        }
    }

    impl Default for MockProcessOps {
        fn default() -> Self {
            Self::new()
        }
    }

    impl crate::stop::ProcessOps for MockProcessOps {
        fn descendants(&self, pid: u32) -> Vec<u32> {
            let children = self.descendants_map.lock().unwrap().get(&pid).cloned().unwrap_or_default();
            if let Some(log) = &self.log {
                log.record(format!("procs:descendants({pid})->{children:?}"));
            }
            children
        }

        fn pids_by_exe(&self, exe: &str) -> Vec<u32> {
            let pids = self.by_exe.lock().unwrap().get(exe).cloned().unwrap_or_default();
            if let Some(log) = &self.log {
                log.record(format!("procs:pids_by_exe({exe})->{pids:?}"));
            }
            pids
        }

        fn kill(&self, pid: u32) -> Result<(), String> {
            if let Some(log) = &self.log {
                log.record(format!("procs:kill({pid})"));
            }
            Ok(())
        }
    }

    // ── 快照构造 helper ─────────────────────────────────────────────────

    pub fn addr(a: [u8; 4]) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a[0], a[1], a[2], a[3]))
    }

    pub fn holders(
        addrs: &[IpAddr],
        procs: &[(u32, Option<&str>, Option<&str>)],
    ) -> PortHolders {
        use crate::probe::ProbeProcess;
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

    pub fn running() -> ProbeState {
        ProbeState::Running { listen_addrs: vec![addr([127, 0, 0, 1])] }
    }
}

// ── 单元测试（宪法 §1：先红后绿）────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::probe::PortHolders;

    fn test_cfg() -> OrchestratorConfig {
        let mut cfg = OrchestratorConfig::new(Lang::Zh, PathBuf::from(r"D:\test-logs"));
        cfg.poll_interval = Duration::from_millis(5);
        cfg.start_timeout = Duration::from_millis(300);
        cfg.stop = StopConfig {
            caddy_stop_timeout: Duration::from_millis(500),
            component_timeout: Duration::from_secs(2),
            port_grace: Duration::from_millis(0),
        };
        cfg.scripts_dir = Some(PathBuf::from(r"D:\test-scripts"));
        cfg
    }

    fn build(
        probe: Arc<ScriptedProbe>,
        executor: Arc<MockExecutor>,
    ) -> (Orchestrator, Arc<MockEventSink>, Arc<MockLogTail>) {
        let sink = Arc::new(MockEventSink::default());
        let logs = Arc::new(MockLogTail::default());
        let orch = Orchestrator::new(
            test_cfg(),
            probe,
            executor,
            Arc::new(MockProcessOps::new()),
            sink.clone(),
            logs.clone(),
        );
        (orch, sink, logs)
    }

    /// 从事件流抽取指定组件的状态时间线（载荷为全量数组，其他组件的事件会
    /// 夹带本组件的当前状态，故折叠连续重复；set_state 不产生同态重复事件）
    fn timeline(sink: &MockEventSink, id: ComponentId) -> Vec<ComponentState> {
        let mut out: Vec<ComponentState> = Vec::new();
        for state in sink.emissions.lock().unwrap().iter().map(|batch| {
            batch
                .iter()
                .find(|s| s.id == id)
                .unwrap_or_else(|| panic!("事件缺组件 {id:?}"))
                .state
        }) {
            if out.last() != Some(&state) {
                out.push(state);
            }
        }
        out
    }

    fn state_of(orch: &Orchestrator, id: ComponentId) -> ComponentStatus {
        orch.statuses().into_iter().find(|s| s.id == id).unwrap()
    }

    // ── 常量与 schema ───────────────────────────────────────────────────

    #[test]
    fn config_defaults_follow_plan() {
        // plan §8：前台 2s；AC4 ≤5s；AC1：就绪超时 60s
        assert_eq!(FOREGROUND_POLL_INTERVAL, Duration::from_secs(2));
        assert!(FOREGROUND_POLL_INTERVAL <= Duration::from_secs(5), "AC4 要求 ≤5s");
        assert_eq!(START_READY_TIMEOUT, Duration::from_secs(60));
        let cfg = OrchestratorConfig::new(Lang::Zh, PathBuf::from(r"D:\logs"));
        assert_eq!(cfg.poll_interval, FOREGROUND_POLL_INTERVAL);
        assert_eq!(cfg.start_timeout, START_READY_TIMEOUT);
        assert_eq!(cfg.scripts_dir, None, "默认未定位脚本目录（装配层注入）");
    }

    #[test]
    fn component_status_schema_serializes_plan_shape() {
        // plan §4：id/state/port/detail?/since；state 为 kebab-case 字符串
        let st = ComponentStatus {
            id: ComponentId::Caddy,
            state: ComponentState::PortHeld,
            port: 443,
            detail: Some("端口被 x.exe 占用".into()),
            since: 1_700_000_000_000,
        };
        let v = serde_json::to_value(&st).unwrap();
        assert_eq!(v["id"], "caddy");
        assert_eq!(v["state"], "port-held");
        assert_eq!(v["port"], 443);
        assert_eq!(v["detail"], "端口被 x.exe 占用");
        assert_eq!(v["since"], 1_700_000_000_000u64);

        // 五态序列化
        for (state, expect) in [
            (ComponentState::Stopped, "stopped"),
            (ComponentState::Starting, "starting"),
            (ComponentState::Running, "running"),
            (ComponentState::PortHeld, "port-held"),
            (ComponentState::Failed, "failed"),
        ] {
            let s = ComponentStatus {
                id: ComponentId::CloudCli,
                state,
                port: 3001,
                detail: None,
                since: 0,
            };
            assert_eq!(serde_json::to_value(&s).unwrap()["state"], expect);
        }

        // detail 缺省不序列化；ComponentId 反序列化合法值
        let no_detail = ComponentStatus {
            id: ComponentId::DdnsGo,
            state: ComponentState::Stopped,
            port: 9876,
            detail: None,
            since: 0,
        };
        let json = serde_json::to_string(&no_detail).unwrap();
        assert!(!json.contains("detail"), "None 不应出现：{json}");
        assert_eq!(
            serde_json::from_str::<ComponentId>("\"cloudcli\"").unwrap(),
            ComponentId::CloudCli
        );
        assert!(serde_json::from_str::<ComponentId>("\"nginx\"").is_err());
    }

    #[test]
    fn fs_log_tail_reads_last_lines() {
        let dir = std::env::temp_dir().join(format!("wb-orch-tail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app.log");
        let content: String = (0..100).map(|i| format!("line-{i}\n")).collect();
        std::fs::write(&path, &content).unwrap();
        let tail = FsLogTailReader.tail(&path, 30).unwrap();
        let lines: Vec<&str> = tail.lines().collect();
        assert_eq!(lines.len(), 30);
        assert_eq!(lines[0], "line-70");
        assert_eq!(lines[29], "line-99");
        assert!(FsLogTailReader.tail(&dir.join("missing.log"), 10).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── 启动状态机 ─────────────────────────────────────────────────────

    #[test]
    fn start_skips_already_running_component() {
        // AC3 幂等：probe 已 running → 跳过，不派发任何命令
        let probe = Arc::new(ScriptedProbe::new());
        probe.pin(ComponentId::CloudCli, running());
        let exec = Arc::new(MockExecutor::new());
        let (orch, sink, _) = build(probe, exec.clone());
        orch.start_one(ComponentId::CloudCli).unwrap();
        assert!(orch.wait_start_idle(ComponentId::CloudCli, Duration::from_secs(2)));
        assert_eq!(state_of(&orch, ComponentId::CloudCli).state, ComponentState::Running);
        assert!(exec.executed.lock().unwrap().is_empty(), "守卫跳过时不应执行脚本");
        assert!(exec.dispatched.lock().unwrap().is_empty(), "守卫跳过时不应派发原生命令");
        assert_eq!(timeline(&sink, ComponentId::CloudCli), vec![ComponentState::Running]);
    }

    #[test]
    fn start_cloudcli_runs_script_then_reaches_running() {
        // AC1 主路径：run-server-hidden.ps1 → starting → 轮询至 running
        let probe = Arc::new(ScriptedProbe::new());
        probe.enqueue(
            ComponentId::CloudCli,
            &[ProbeState::Stopped, ProbeState::Stopped, running()],
        );
        let exec = Arc::new(MockExecutor::new());
        let (orch, sink, _) = build(probe.clone(), exec.clone());
        orch.start_one(ComponentId::CloudCli).unwrap();
        assert!(orch.wait_start_idle(ComponentId::CloudCli, Duration::from_secs(2)));

        let executed = exec.executed.lock().unwrap();
        assert_eq!(executed.len(), 1, "CloudCLI 应恰好执行一次启动脚本");
        assert!(executed[0].command_line().contains("run-server-hidden.ps1"), "{}", executed[0].command_line());
        assert!(executed[0].command_line().contains("-Lang zh"), "-Lang 应与生效语言对齐");
        assert!(executed[0].command_line().contains("-Port 3001"), "{}", executed[0].command_line());
        drop(executed);
        assert_eq!(
            timeline(&sink, ComponentId::CloudCli),
            vec![ComponentState::Starting, ComponentState::Running],
            "事件序列应为 starting→running"
        );
        assert_eq!(state_of(&orch, ComponentId::CloudCli).state, ComponentState::Running);
    }

    #[test]
    fn start_caddy_and_ddnsgo_dispatch_native_commands() {
        for (id, program, arg_needle) in [
            (ComponentId::Caddy, r"D:\Software\cloudcli-https\caddy.exe", "--config"),
            (ComponentId::DdnsGo, r"D:\Software\cloudcli-https\ddns-go.exe", "-l"),
        ] {
            let probe = Arc::new(ScriptedProbe::new());
            probe.enqueue(id, &[ProbeState::Stopped, running()]);
            let exec = Arc::new(MockExecutor::new());
            let (orch, sink, _) = build(probe, exec.clone());
            orch.start_one(id).unwrap();
            assert!(orch.wait_start_idle(id, Duration::from_secs(2)));
            let dispatched = exec.dispatched.lock().unwrap();
            assert_eq!(dispatched.len(), 1, "{id:?} 应派发一次原生命令");
            assert_eq!(dispatched[0].program, program);
            assert!(dispatched[0].args.iter().any(|a| a == arg_needle), "{:?}", dispatched[0].args);
            assert!(dispatched[0].stdout_log.is_some(), "stdout 应落日志");
            drop(dispatched);
            assert_eq!(
                timeline(&sink, id),
                vec![ComponentState::Starting, ComponentState::Running]
            );
        }
    }

    #[test]
    fn native_dispatch_failure_marks_failed() {
        let probe = Arc::new(ScriptedProbe::new());
        probe.pin(ComponentId::Caddy, ProbeState::Stopped);
        let exec = Arc::new(MockExecutor::with_dispatch_err("系统找不到指定的文件"));
        let (orch, sink, _) = build(probe, exec);
        orch.start_one(ComponentId::Caddy).unwrap();
        assert!(orch.wait_start_idle(ComponentId::Caddy, Duration::from_secs(2)));
        let st = state_of(&orch, ComponentId::Caddy);
        assert_eq!(st.state, ComponentState::Failed);
        assert!(st.detail.unwrap().contains("拉起失败"), "详情应含失败原因");
        assert_eq!(timeline(&sink, ComponentId::Caddy), vec![ComponentState::Starting, ComponentState::Failed]);
    }

    #[test]
    fn start_timeout_marks_failed_with_log_tail() {
        // AC1：60s 未就绪 → failed + 日志尾部（CloudCLI=%TEMP%\cloudcli.log）
        let probe = Arc::new(ScriptedProbe::new());
        probe.pin(ComponentId::CloudCli, ProbeState::Stopped);
        probe.pin(ComponentId::DdnsGo, ProbeState::Stopped);
        let exec = Arc::new(MockExecutor::new());
        let (orch, sink, logs) = build(probe, exec);
        logs.set(cloudcli_log_path(), "Error: Cannot find module 'cloudcli'");
        logs.set(
            PathBuf::from(r"D:\test-logs\ddns-go.err.log"),
            "panic: config parse failed",
        );

        for id in [ComponentId::CloudCli, ComponentId::DdnsGo] {
            orch.start_one(id).unwrap();
        }
        assert!(orch.wait_start_idle(ComponentId::CloudCli, Duration::from_secs(2)));
        assert!(orch.wait_start_idle(ComponentId::DdnsGo, Duration::from_secs(2)));

        let st = state_of(&orch, ComponentId::CloudCli);
        assert_eq!(st.state, ComponentState::Failed);
        let detail = st.detail.unwrap();
        assert!(detail.contains("启动超时"), "{detail}");
        assert!(detail.contains("cloudcli.log"), "应指向 CloudCLI 运行日志：{detail}");
        assert!(detail.contains("Cannot find module"), "应附日志尾部：{detail}");

        let st2 = state_of(&orch, ComponentId::DdnsGo);
        assert_eq!(st2.state, ComponentState::Failed);
        let detail2 = st2.detail.unwrap();
        assert!(detail2.contains("ddns-go.err.log"), "应指向组件日志：{detail2}");
        assert!(detail2.contains("panic"), "应附日志尾部：{detail2}");

        assert_eq!(
            timeline(&sink, ComponentId::CloudCli),
            vec![ComponentState::Starting, ComponentState::Failed]
        );
    }

    #[test]
    fn script_exit1_maps_to_unavailable_hint() {
        // AC1：脚本 exit 1 → "CloudCLI 未安装/不可用"提示
        let probe = Arc::new(ScriptedProbe::new());
        probe.pin(ComponentId::CloudCli, ProbeState::Stopped);
        let exec = Arc::new(MockExecutor::with_exits(&[ExecOutcome::Exited(1)]));
        let (orch, sink, _) = build(probe, exec);
        orch.start_one(ComponentId::CloudCli).unwrap();
        assert!(orch.wait_start_idle(ComponentId::CloudCli, Duration::from_secs(2)));
        let st = state_of(&orch, ComponentId::CloudCli);
        assert_eq!(st.state, ComponentState::Failed);
        assert!(st.detail.unwrap().contains("cloudcli 不可用"), "AC1 exit 1 语义");
        assert_eq!(
            timeline(&sink, ComponentId::CloudCli),
            vec![ComponentState::Starting, ComponentState::Failed]
        );
    }

    #[test]
    fn start_guard_marks_port_held_without_spawning() {
        // AC7/AC6：端口被无关进程占 → port-held 态（非 running），不拉起
        let probe = Arc::new(ScriptedProbe::new());
        probe.pin(
            ComponentId::Caddy,
            ProbeState::PortHeld { process_name: "svchost.exe".into() },
        );
        let exec = Arc::new(MockExecutor::new());
        let (orch, sink, _) = build(probe, exec.clone());
        orch.start_one(ComponentId::Caddy).unwrap();
        assert!(orch.wait_start_idle(ComponentId::Caddy, Duration::from_secs(2)));
        let st = state_of(&orch, ComponentId::Caddy);
        assert_eq!(st.state, ComponentState::PortHeld);
        assert!(st.detail.unwrap().contains("svchost.exe"));
        assert!(exec.dispatched.lock().unwrap().is_empty(), "被占时不应派发");
        assert_eq!(timeline(&sink, ComponentId::Caddy), vec![ComponentState::PortHeld]);
    }

    #[test]
    fn missing_scripts_dir_disables_cloudcli_start_with_reason() {
        // spec §4.5：脚本目录不可用 → 相关功能禁用并给原因（failed 态）
        let probe = Arc::new(ScriptedProbe::new());
        probe.pin(ComponentId::CloudCli, ProbeState::Stopped);
        let exec = Arc::new(MockExecutor::new());
        let sink = Arc::new(MockEventSink::default());
        let logs = Arc::new(MockLogTail::default());
        let mut cfg = test_cfg();
        cfg.scripts_dir = None;
        let orch = Orchestrator::new(
            cfg,
            probe,
            exec.clone(),
            Arc::new(MockProcessOps::new()),
            sink,
            logs,
        );
        orch.start_one(ComponentId::CloudCli).unwrap();
        assert!(orch.wait_start_idle(ComponentId::CloudCli, Duration::from_secs(2)));
        let st = state_of(&orch, ComponentId::CloudCli);
        assert_eq!(st.state, ComponentState::Failed);
        assert!(st.detail.unwrap().contains("脚本目录"), "应给出禁用原因");
        assert!(exec.executed.lock().unwrap().is_empty());
    }

    // ── 在途互斥与取消 ─────────────────────────────────────────────────

    #[test]
    fn duplicate_start_rejected_while_inflight() {
        // 同组件启动进行中不允许重复派发；其他组件不受影响
        let probe = Arc::new(ScriptedProbe::new());
        probe.pin(ComponentId::CloudCli, ProbeState::Stopped);
        probe.pin(ComponentId::Caddy, ProbeState::Stopped);
        let exec = Arc::new(MockExecutor::new());
        let (orch, _sink, _) = build(probe, exec);

        orch.start_one(ComponentId::CloudCli).unwrap();
        assert_eq!(
            orch.start_one(ComponentId::CloudCli),
            Err(StartError::AlreadyInFlight),
            "在途组件重复派发应被拒绝"
        );
        orch.start_one(ComponentId::Caddy).unwrap();

        assert!(orch.wait_start_idle(ComponentId::CloudCli, Duration::from_secs(2)));
        assert!(!orch.cancel_start(ComponentId::CloudCli), "结束后无在途可取消");
    }

    #[test]
    fn tracker_cancel_wakes_waiter() {
        // 取消句柄/等待接口语义（T10 收摊消费）
        let tracker = Arc::new(InFlightTracker::default());
        assert!(!tracker.is_busy(ComponentId::CloudCli));
        assert!(!tracker.cancel(ComponentId::CloudCli), "无在途时 cancel 返回 false");

        let flag = tracker.try_mark(ComponentId::CloudCli).unwrap();
        assert!(tracker.try_mark(ComponentId::CloudCli).is_none(), "重复登记应失败");
        assert!(tracker.is_busy(ComponentId::CloudCli));

        let waiter = {
            let tracker_ref = tracker.clone();
            std::thread::spawn(move || {
                let start = Instant::now();
                let ok = tracker_ref.wait_idle(ComponentId::CloudCli, Duration::from_secs(5));
                (ok, start.elapsed())
            })
        };
        std::thread::sleep(Duration::from_millis(50));
        assert!(tracker.cancel(ComponentId::CloudCli));
        assert!(flag.load(Ordering::Relaxed), "取消应置位在途句柄");
        tracker.clear(ComponentId::CloudCli);
        let (ok, elapsed) = waiter.join().unwrap();
        assert!(ok, "clear 后等待应立即返回");
        assert!(elapsed < Duration::from_secs(2), "等待应被唤醒而非超时：{elapsed:?}");
    }

    // ── 刷新与轮询器 ───────────────────────────────────────────────────

    #[test]
    fn refresh_all_emits_only_on_state_change() {
        let probe = Arc::new(ScriptedProbe::new());
        let exec = Arc::new(MockExecutor::new());
        let (orch, sink, _) = build(probe.clone(), exec);

        // 初态全 stopped：刷新无事件
        orch.refresh_all();
        assert_eq!(sink.emissions.lock().unwrap().len(), 0, "状态未变不应发事件");

        // cloudcli 变 running → 一次事件（全量数组）
        probe.pin(ComponentId::CloudCli, running());
        orch.refresh_all();
        {
            let emissions = sink.emissions.lock().unwrap();
            assert_eq!(emissions.len(), 1);
            assert_eq!(emissions[0].len(), 3, "事件载荷应为全组件数组（plan §4）");
        }
        orch.refresh_all();
        assert_eq!(sink.emissions.lock().unwrap().len(), 1, "重复刷新无新事件");

        // 变 port-held → 事件且 detail 携带占用进程名（AC7）
        probe.pin(
            ComponentId::CloudCli,
            ProbeState::PortHeld { process_name: "nginx.exe".into() },
        );
        orch.refresh_all();
        assert_eq!(sink.emissions.lock().unwrap().len(), 2);
        let st = state_of(&orch, ComponentId::CloudCli);
        assert_eq!(st.state, ComponentState::PortHeld);
        assert!(st.detail.unwrap().contains("nginx.exe"));
    }

    #[test]
    fn refresh_all_leaves_inflight_component_alone() {
        // 在途组件的状态迁移归启动线程独占：外部刷新不得把 starting 打回 stopped
        // （probe 持续报 Stopped，若 refresh 不跳过则会覆盖 starting → 断言即失败）
        let probe = Arc::new(ScriptedProbe::new());
        probe.pin(ComponentId::CloudCli, ProbeState::Stopped);
        let exec = Arc::new(MockExecutor::new());
        let (orch, _sink, _) = build(probe, exec);

        orch.start_one(ComponentId::CloudCli).unwrap();
        std::thread::sleep(Duration::from_millis(20)); // 让线程进入就绪轮询
        orch.refresh_all();
        assert_eq!(
            state_of(&orch, ComponentId::CloudCli).state,
            ComponentState::Starting,
            "在途组件应保持 starting（其线程会自行迁移终态）"
        );
        // 收尾：取消并等待，避免线程继续占用 mock
        orch.cancel_start(ComponentId::CloudCli);
        assert!(orch.wait_start_idle(ComponentId::CloudCli, Duration::from_secs(2)));
    }

    #[test]
    fn poller_refreshes_periodically_until_shutdown() {
        let probe = Arc::new(ScriptedProbe::new());
        let exec = Arc::new(MockExecutor::new());
        let mut cfg = test_cfg();
        cfg.poll_interval = Duration::from_millis(10);
        let sink = Arc::new(MockEventSink::default());
        let logs = Arc::new(MockLogTail::default());
        let orch = Orchestrator::new(
            cfg,
            probe.clone(),
            exec,
            Arc::new(MockProcessOps::new()),
            sink,
            logs,
        );

        let handle = orch.spawn_poller();
        std::thread::sleep(Duration::from_millis(150));
        handle.shutdown();
        // 每周期 3 次 probe（3 组件），150ms@10ms 至少完成多个周期
        assert!(probe.probe_calls() >= 6, "轮询应周期执行：{} 次", probe.probe_calls());
        let after = probe.probe_calls();
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(probe.probe_calls(), after, "shutdown 后不应再刷新");
    }

    #[test]
    fn start_all_covers_every_component() {
        // 一键启动逐组件派发（AC1）；在途组件幂等跳过
        let probe = Arc::new(ScriptedProbe::new());
        for id in COMPONENT_ORDER {
            probe.pin(id, ProbeState::Stopped);
        }
        let exec = Arc::new(MockExecutor::new());
        let (orch, _sink, _) = build(probe, exec.clone());
        orch.start_all();
        for id in COMPONENT_ORDER {
            assert!(orch.wait_start_idle(id, Duration::from_secs(2)), "{id:?} 应被派发");
        }
        assert_eq!(exec.executed.lock().unwrap().len(), 1, "仅 CloudCLI 走脚本");
        assert_eq!(exec.dispatched.lock().unwrap().len(), 2, "Caddy/ddns-go 原生派发");
        // 二次 start_all 幂等（全部结束后重入不报错）
        orch.start_all();
        for id in COMPONENT_ORDER {
            assert!(orch.wait_start_idle(id, Duration::from_secs(2)));
        }
    }

    // ── 停止接入（T9）──────────────────────────────────────────────────

    #[test]
    fn stop_one_runs_pipeline_and_updates_state() {
        // CloudCLI：定位(占) → 杀 → 复核(空) → Stopped；事件同步
        let probe = Arc::new(ScriptedProbe::new());
        probe.pin(ComponentId::CloudCli, running()); // 先处于运行态
        probe.enqueue_holders(
            3001,
            &[
                holders(&[addr([127, 0, 0, 1])], &[(100, Some(r"C:\n\node.exe"), Some("node.exe"))]),
                PortHolders::default(),
            ],
        );
        let exec = Arc::new(MockExecutor::new());
        let (orch, sink, _) = build(probe, exec);
        orch.refresh_all(); // 迁移到 Running（事件 [Running]）

        let outcome = orch.stop_one(ComponentId::CloudCli);
        assert_eq!(outcome, StopOutcome::Stopped);
        assert_eq!(
            state_of(&orch, ComponentId::CloudCli).state,
            ComponentState::Stopped
        );
        assert_eq!(
            timeline(&sink, ComponentId::CloudCli),
            vec![ComponentState::Running, ComponentState::Stopped]
        );
    }

    #[test]
    fn stop_all_isolates_component_failure() {
        // AC2：cloudcli 复核失败（端口重占）不得阻塞其余组件停止
        let probe = Arc::new(ScriptedProbe::new());
        // cloudcli：定位与复核均被 node 占 → Failed；其余组件端口默认空 → AlreadyStopped
        probe.pin_holders(
            3001,
            holders(&[addr([127, 0, 0, 1])], &[(100, Some(r"C:\n\node.exe"), Some("node.exe"))]),
        );
        let exec = Arc::new(MockExecutor::new());
        let (orch, sink, _) = build(probe, exec);

        let outcomes: std::collections::HashMap<ComponentId, StopOutcome> =
            orch.stop_all().into_iter().collect();
        assert_eq!(outcomes.len(), 3, "全部组件都应得到结论（互不阻塞）");
        assert!(matches!(outcomes[&ComponentId::CloudCli], StopOutcome::Failed(_)), "复核失败应上报");
        assert_eq!(outcomes[&ComponentId::Caddy], StopOutcome::AlreadyStopped);
        assert_eq!(outcomes[&ComponentId::DdnsGo], StopOutcome::AlreadyStopped);

        // 状态：失败组件 failed 附原因；其余 stopped
        assert_eq!(state_of(&orch, ComponentId::CloudCli).state, ComponentState::Failed);
        let detail = state_of(&orch, ComponentId::CloudCli).detail.unwrap();
        assert!(detail.contains("复核未通过") && detail.contains("netstat"), "{detail}");
        assert_eq!(state_of(&orch, ComponentId::Caddy).state, ComponentState::Stopped);
        assert_eq!(state_of(&orch, ComponentId::DdnsGo).state, ComponentState::Stopped);
        assert_eq!(
            timeline(&sink, ComponentId::CloudCli),
            vec![ComponentState::Failed]
        );
    }

    #[test]
    fn stop_cancels_inflight_start_quickly() {
        // spec §4.4：停止先等待/取消在途启动再判定端口；
        // 取消后启动线程不得再迁移状态（不会把 Stopped 打回 Starting/Failed）
        let probe = Arc::new(ScriptedProbe::new());
        probe.pin(ComponentId::CloudCli, ProbeState::Stopped); // 启动永远不就绪
        let mut cfg = test_cfg();
        cfg.start_timeout = Duration::from_secs(8); // 在途远长于停止预算
        let sink = Arc::new(MockEventSink::default());
        let logs = Arc::new(MockLogTail::default());
        let orch = Orchestrator::new(
            cfg,
            probe,
            Arc::new(MockExecutor::new()),
            Arc::new(MockProcessOps::new()),
            sink.clone(),
            logs,
        );

        orch.start_one(ComponentId::CloudCli).unwrap();
        let begun = Instant::now();
        let outcome = orch.stop_one(ComponentId::CloudCli);
        assert_eq!(outcome, StopOutcome::AlreadyStopped);
        assert!(
            begun.elapsed() < Duration::from_secs(2),
            "取消应在途启动后立即继续（实际 {:?}）",
            begun.elapsed()
        );
        assert!(orch.wait_start_idle(ComponentId::CloudCli, Duration::from_secs(2)));
        assert_eq!(
            state_of(&orch, ComponentId::CloudCli).state,
            ComponentState::Stopped,
            "取消路径的终态由停止管线决定"
        );
        // 启动线程已在 wait_idle 前退出：其后不得再出现 Starting/Failed 覆盖
        // （取消发生在 Starting 之前时时间线为空，同样合法）
        let tl = timeline(&sink, ComponentId::CloudCli);
        match tl.last() {
            None | Some(ComponentState::Stopped) => {}
            other => panic!("取消后启动线程不得再覆盖状态：{other:?}（完整 {tl:?}）"),
        }
    }
}
