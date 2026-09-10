//! 装机向导（spec 006）：阶段状态机 + 阶段探针 + 持久化 + Tauri 命令。
//!
//! - 五阶段（basis/tencent/https/channel/finalize），检测驱动：探针幂等只读，
//!   前置满足 → Done（自动跳过，AC2）；不满足 → Pending/Failed（AC12 失败隔离）
//! - 编排哲学：本模块**不派发脚本**——装机类动作由前端复用既有 run_tool
//!   （可见窗口，UAC 语义沿用 AC19/20），用户完成后回向导点「校验」触发
//!   wizard_detect 重探测；通道激活复用 004 的 Settings.access_channel 守卫收敛
//! - 凭证边界（宪法 §3）：探针只判存在性，凭证值不进本模块/事件/日志
//! - detail 存稳定码（如 "auth_failed"），双语呈现由前端 i18n 承担
//! - 纯函数（derive_* / map_tc_error / split_domain / cgnat 系）与 IO 采集隔离，可单测

use crate::consts::DOMAIN;
use crate::probe::{ComponentId, ProbeState, StatusProbe};
use crate::settings::{AccessChannel, SettingsState};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 向导状态变化事件（载荷 = 全量 WizardState；前端零轮询）
pub const EVENT_WIZARD_CHANGED: &str = "wizard://changed";
/// 状态文件名（%APPDATA%\ai-remote-workbench\ 下）
pub const WIZARD_STATE_FILE: &str = "wizard-state.json";
/// schema 版本
pub const WIZARD_SCHEMA_VERSION: u32 = 1;

// ── 数据模型 ────────────────────────────────────────────────────────────────

/// 向导阶段（顺序固定）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WizardStageId {
    /// 基础：CloudCLI 装机 + 3001（完成即局域网可达，AC3）
    Basis,
    /// 腾讯云前置：CAM 密钥 + 域名存在（AC4/5；直连/穿透共用）
    Tencent,
    /// HTTPS 栈：插件版 Caddy + 插件式 Caddyfile + 443（AC6/7；共用）
    Https,
    /// 通道分支：直连（ddns-go）/ 穿透（frpc）（AC8/9/10）
    Channel,
    /// 收尾：自启注册 + 目标达成清单（AC11）
    Finalize,
}

impl WizardStageId {
    pub const ALL: [WizardStageId; 5] = [
        WizardStageId::Basis,
        WizardStageId::Tencent,
        WizardStageId::Https,
        WizardStageId::Channel,
        WizardStageId::Finalize,
    ];
}

/// 阶段态（不设 Running：脚本派发由前端复用 run_tool，其繁忙态为前端局部状态）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageState {
    Pending,
    Done,
    Failed,
    /// 分支未选中的另一分支（通道互斥的 UI 呈现）
    Skipped,
}

/// 单阶段状态（detail = 稳定码，双语归前端 i18n）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageStatus {
    pub id: WizardStageId,
    pub state: StageState,
    #[serde(default)]
    pub detail: Option<String>,
}

/// 向导全量状态（wizard-state.json 持久化；敏感凭证永不进入本结构）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WizardState {
    pub version: u32,
    pub stages: Vec<StageStatus>,
    /// 阶段④分支选择（与 Settings.access_channel 同步写入）
    pub branch: Option<AccessChannel>,
    /// 访问域名（非敏感；默认 consts::DOMAIN）
    pub domain: String,
    /// 收尾完成（AC11）
    pub done: bool,
}

impl Default for WizardState {
    fn default() -> Self {
        Self {
            version: WIZARD_SCHEMA_VERSION,
            stages: WizardStageId::ALL
                .iter()
                .map(|id| StageStatus { id: *id, state: StageState::Pending, detail: None })
                .collect(),
            branch: None,
            domain: DOMAIN.to_string(),
            done: false,
        }
    }
}

impl WizardState {
    /// 取指定阶段状态（阶段缺失时补 Pending——前向兼容旧/损坏文件）
    pub fn stage(&self, id: WizardStageId) -> StageStatus {
        self.stages
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .unwrap_or(StageStatus { id, state: StageState::Pending, detail: None })
    }

    /// 规整：恒 5 阶段、顺序固定、version 刷当前值
    pub fn normalized(mut self) -> Self {
        self.version = WIZARD_SCHEMA_VERSION;
        self.stages = WizardStageId::ALL
            .iter()
            .map(|id| self.stage(*id))
            .collect();
        self
    }
}

// ── 持久化（settings.rs 同款语义：缺失→默认；损坏→留档+回默认；原子写）──────

/// 状态文件路径（%APPDATA%\ai-remote-workbench\wizard-state.json）
pub fn wizard_state_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join(crate::settings::APP_DIR_NAME).join(WIZARD_STATE_FILE)
}

/// 加载结果
#[derive(Debug)]
pub enum LoadOutcome {
    Missing(WizardState),
    Loaded(WizardState),
    /// 损坏 → 已改名 .bad-<时间戳> 留档 + 回默认
    Repaired { state: WizardState, backup_path: PathBuf },
}

/// 从指定路径加载（路径注入便于单测）
pub fn load_from(path: &Path) -> LoadOutcome {
    let raw = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return LoadOutcome::Missing(WizardState::default());
        }
        Err(e) => {
            log::warn!("向导状态文件读取失败（{e}），按损坏处理");
            return repair(path);
        }
    };
    match serde_json::from_str::<WizardState>(&raw) {
        Ok(s) => LoadOutcome::Loaded(s.normalized()),
        Err(e) => {
            log::warn!("向导状态文件解析失败（{e}）：留档并回退默认值");
            repair(path)
        }
    }
}

/// 损坏恢复：改名 .bad-<毫秒时间戳> 留档（尽力而为），返回默认值
fn repair(path: &Path) -> LoadOutcome {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut backup: std::ffi::OsString = path.as_os_str().to_owned();
    backup.push(format!(".bad-{ts}"));
    let backup_path = path.with_file_name(backup);
    if let Err(e) = fs::rename(path, &backup_path) {
        log::error!("损坏向导状态留档改名失败：{e}（{}）", backup_path.display());
    }
    LoadOutcome::Repaired { state: WizardState::default(), backup_path }
}

/// 原子保存：同目录临时文件 rename 覆盖
pub fn save_to(path: &Path, state: &WizardState) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut json = serde_json::to_string_pretty(state)?;
    json.push('\n');
    let mut tmp: std::ffi::OsString = path.as_os_str().to_owned();
    tmp.push(format!(".tmp-{}", std::process::id()));
    let tmp_path = path.with_file_name(tmp);
    fs::write(&tmp_path, json)?;
    fs::rename(&tmp_path, path)
}

// ── 纯函数（可单测）────────────────────────────────────────────────────────

/// 域名拆根域/子域（取末两段为根域；单标签原样返回）
/// "ai.jackqi.cn" → ("jackqi.cn", "ai")；"jackqi.cn" → ("jackqi.cn", "jackqi.cn")
pub fn split_domain(domain: &str) -> (String, String) {
    let d = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = d.split('.').filter(|s| !s.is_empty()).collect();
    if labels.len() <= 1 {
        return (d.clone(), d);
    }
    let root = labels[labels.len() - 2..].join(".");
    let sub = labels.join(".");
    (root, sub)
}

/// 腾讯云 API 错误 → 稳定码（双语呈现归前端；匹配串来自 dnspod 实测错误形态）
pub fn map_tc_error(err: &str) -> &'static str {
    if err.contains("AuthFailure") || err.contains("InvalidCredential") || err.contains("AuthInfo") {
        "auth_failed"
    } else if err.contains("UnauthorizedOperation") || err.contains("PermissionDenied") {
        "permission_denied"
    } else if err.contains("NoRecord") || err.contains("NoDataOfRecord") {
        // 域名存在但子域暂无记录：ddns-go 启动后会自动创建（AC8 前置成立）
        "no_records"
    } else if err.contains("InvalidDomain") || err.contains("DomainNotFound") {
        "domain_missing"
    } else {
        "api_error"
    }
}

/// 公网出口 IP 分类（AC8 如实警示；只判"直连前提缺失"，不冒充外部可达结论）
/// None = IP 正常公网段；Some(code) = 警示码
pub fn cgnat_classify(public_ip: Option<IpAddr>) -> Option<&'static str> {
    let ip = match public_ip {
        Some(IpAddr::V4(v4)) => v4,
        Some(_) => return Some("warn_ipv6"),
        None => return Some("warn_no_public_ip"),
    };
    let o = ip.octets();
    let privateish = o[0] == 10
        || (o[0] == 172 && (o[1] & 0xf0) == 16)
        || (o[0] == 192 && o[1] == 168)
        || (o[0] == 169 && o[1] == 254)
        || (o[0] == 100 && (o[1] & 0xc0) == 64); // 100.64/10 运营商 CGNAT
    if privateish {
        Some("warn_private_range")
    } else {
        None
    }
}

// ── 阶段推导（纯函数：采集结果 → 阶段态）────────────────────────────────────

/// 腾讯云 API 校验结果（真实采集 → 枚举，便于单测）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TencentApiOutcome {
    Ok,
    NoRecords,
    Err(String),
}

/// ① 基础（AC3）：CloudCLI 3001 监听且身份匹配 → 局域网可达
pub fn derive_basis(probe: &ProbeState) -> (StageState, Option<String>) {
    match probe {
        ProbeState::Running { .. } => (StageState::Done, Some("ok".into())),
        ProbeState::PortHeld { .. } => (StageState::Failed, Some("port_held".into())),
        _ => (StageState::Pending, Some("not_installed".into())),
    }
}

/// ② 腾讯云前置（AC4/5）：凭证存在 + API 可查（密钥有效 + 域名存在一并验证）
pub fn derive_tencent(creds_present: bool, api: &TencentApiOutcome) -> (StageState, Option<String>) {
    match (creds_present, api) {
        (false, _) => (StageState::Pending, Some("missing_creds".into())),
        (true, TencentApiOutcome::Ok) => (StageState::Done, Some("ok".into())),
        (true, TencentApiOutcome::NoRecords) => (StageState::Done, Some("no_records".into())),
        (true, TencentApiOutcome::Err(e)) => (StageState::Failed, Some(map_tc_error(e).into())),
    }
}

/// ③ HTTPS 栈（AC6/7）：caddy + ddns-go 落位 + Caddyfile 存在 + 443 运行。
/// Caddyfile 分插件式/旧式两种：旧式（acme.sh 证书链）+ 运行中 = 旧机已在工作，
/// 同样判 Done（detail "legacy_ok"，迁移可选）——不把"旧机不迁移"误报为未装机
pub fn derive_https(
    caddy_exe: bool,
    ddnsgo_exe: bool,
    caddyfile_present: bool,
    caddyfile_plugin_style: bool,
    caddy_running: bool,
) -> (StageState, Option<String>) {
    if !caddy_exe {
        return (StageState::Pending, Some("missing_caddy".into()));
    }
    if !ddnsgo_exe {
        return (StageState::Pending, Some("missing_ddnsgo".into()));
    }
    if !caddyfile_present {
        return (StageState::Pending, Some("missing_caddyfile".into()));
    }
    if !caddy_running {
        return (StageState::Pending, Some("not_running".into()));
    }
    if caddyfile_plugin_style {
        (StageState::Done, Some("ok".into()))
    } else {
        (StageState::Done, Some("legacy_ok".into()))
    }
}

/// ④a 直连分支（AC8）：ddns-go 已配置 + 运行；解析对齐情况进 detail 警示码
pub fn derive_direct(yaml_present: bool, ddnsgo_running: bool) -> (StageState, Option<String>) {
    if !yaml_present {
        return (StageState::Pending, Some("missing_yaml".into()));
    }
    if ddnsgo_running {
        (StageState::Done, Some("ok".into()))
    } else {
        (StageState::Pending, Some("not_running".into()))
    }
}

/// ④b 穿透分支（AC9）：access key + 隧道配置就绪（frpc 在线由看板/004 守护呈现）
pub fn derive_tunnel(key_present: bool, tunnel_configured: bool) -> (StageState, Option<String>) {
    match (key_present, tunnel_configured) {
        (true, true) => (StageState::Done, Some("ok".into())),
        (false, _) => (StageState::Pending, Some("missing_key".into())),
        (true, false) => (StageState::Pending, Some("tunnel_unset".into())),
    }
}

// ── 采集（IO 层：探针装配消费；Windows-only，ADR-0001）──────────────────────

#[cfg(windows)]
fn file_present(path: &str) -> bool {
    Path::new(path).is_file()
}

#[cfg(windows)]
fn caddyfile_plugin_style(stack_dir: &str) -> bool {
    let path = Path::new(stack_dir).join("Caddyfile");
    std::fs::read_to_string(path)
        .map(|c| c.contains("dns tencentcloud"))
        .unwrap_or(false)
}

/// 拉取公网出口 IPv4（url 模式同源接口；失败返回 None → 警示码由 cgnat_classify 给出）
#[cfg(windows)]
fn fetch_public_ipv4() -> Option<IpAddr> {
    let resp = ureq::get("https://4.ipw.cn").timeout(std::time::Duration::from_secs(5)).call().ok()?;
    let text = resp.into_string().ok()?;
    text.trim().parse().ok()
}

/// 通道阶段探针输入（真实采集 → 纯推导）
#[cfg(windows)]
fn probe_channel_stage(state: &WizardState, settings: &crate::settings::Settings) -> (StageState, Option<String>) {
    match state.branch {
        None => (StageState::Pending, Some("branch_unset".into())),
        Some(AccessChannel::Direct) => {
            let stack = &settings.stack_dir;
            let yaml = Path::new(stack).join("ddns-go.yaml");
            let (st, mut detail) = derive_direct(yaml.is_file(), running_on(ComponentId::DdnsGo, stack));
            // AC8：解析对齐 + 出口形态如实警示（仅在 ddns-go 运行时才有意义）
            if st == StageState::Done {
                detail = direct_detail_with_dns(stack, &state.domain);
            }
            (st, detail)
        }
        Some(AccessChannel::Tunnel) => {
            let sakura = std::fs::read_to_string(Path::new(&settings.stack_dir).join(".env"))
                .map(|c| crate::tunnel::parse_env_value(&c, crate::consts::SAKURA_KEY_VAR).is_some())
                .unwrap_or(false);
            derive_tunnel(sakura, settings.tunnel.is_some())
        }
        // spec 007 T13 实现组网分支检测（服务/密钥/在线校验）；此前呈待办态
        Some(AccessChannel::Mesh) => (StageState::Pending, Some("mesh_stage_todo".into())),
    }
}

/// 直连 detail 附加 DNS 对齐/CGNAT 警示码（"ok" / "warn_xxx" / "record_mismatch"）
#[cfg(windows)]
fn direct_detail_with_dns(stack_dir: &str, domain: &str) -> Option<String> {
    let Some(cred) = crate::dns_api::read_credential(stack_dir) else {
        return Some("ok".into());
    };
    let (root, sub) = split_domain(domain);
    let Ok(records) = crate::dns_api::signed_request(
        &cred,
        "DescribeRecordList",
        &serde_json::json!({ "Domain": root, "SubDomain": sub, "Offset": 0, "Limit": 100 }),
        std::time::SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
    ) else {
        return Some("ok".into()); // 查询失败不阻断装机完成态，可达性归 005 心跳
    };
    let a_value = records
        .pointer("/Response/RecordList")
        .and_then(|v| v.as_array())
        .and_then(|list| {
            list.iter().find(|r| {
                r.get("Type").and_then(|t| t.as_str()) == Some("A")
                    && r.get("Status").and_then(|s| s.as_str()) == Some("ENABLE")
            })
        })
        .and_then(|r| r.get("Value").and_then(|v| v.as_str()).map(str::to_owned));
    let Some(a_value) = a_value else {
        return Some("warn_no_a_record".into());
    };
    let Ok(parsed) = a_value.parse::<IpAddr>() else {
        return Some("warn_no_a_record".into());
    };
    let public = fetch_public_ipv4();
    if let Some(warn) = cgnat_classify(public) {
        return Some(warn.into());
    }
    if Some(parsed) == public {
        Some("ok".into())
    } else {
        Some("record_mismatch".into())
    }
}

#[cfg(windows)]
fn running_on(id: ComponentId, stack_dir: &str) -> bool {
    matches!(crate::probe::WindowsProbe.probe(id, stack_dir), ProbeState::Running { .. })
}

/// 全量重探测（AC1/2：检测驱动 + 已完成自动跳过）
#[cfg(windows)]
pub fn detect(state: &WizardState, settings: &crate::settings::Settings, probe: &dyn StatusProbe) -> WizardState {
    let mut next = state.clone();
    let stack = &settings.stack_dir;

    // ① 基础：3001 + node 身份
    let (st, detail) = derive_basis(&probe.probe(ComponentId::CloudCli, stack));
    set_stage(&mut next, WizardStageId::Basis, st, detail);

    // ② 腾讯云前置：凭证存在 + API 校验（域名来自向导状态）
    let creds = crate::dns_api::read_credential(stack);
    let api = match &creds {
        None => TencentApiOutcome::Err("missing".into()),
        Some(cred) => {
            let (root, sub) = split_domain(&next.domain);
            match crate::dns_api::signed_request(
                cred,
                "DescribeRecordList",
                &serde_json::json!({ "Domain": root, "SubDomain": sub, "Offset": 0, "Limit": 1 }),
                SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            ) {
                Ok(_) => TencentApiOutcome::Ok,
                Err(e) if map_tc_error(&e) == "no_records" => TencentApiOutcome::NoRecords,
                Err(e) => TencentApiOutcome::Err(e),
            }
        }
    };
    let (st, detail) = derive_tencent(creds.is_some(), &api);
    set_stage(&mut next, WizardStageId::Tencent, st, detail);

    // ③ HTTPS 栈
    let (st, detail) = derive_https(
        file_present(&format!(r"{stack}\caddy.exe")),
        file_present(&format!(r"{stack}\ddns-go.exe")),
        Path::new(stack).join("Caddyfile").is_file(),
        caddyfile_plugin_style(stack),
        matches!(probe.probe(ComponentId::Caddy, stack), ProbeState::Running { .. }),
    );
    set_stage(&mut next, WizardStageId::Https, st, detail);

    // ④ 通道分支
    let (st, detail) = probe_channel_stage(&next, settings);
    set_stage(&mut next, WizardStageId::Channel, st, detail);

    // ⑤ 收尾：done 后保持 Done（不被探针回退）
    if next.done {
        set_stage(&mut next, WizardStageId::Finalize, StageState::Done, Some("ok".into()));
    }
    next.normalized()
}

#[cfg(not(windows))]
pub fn detect(_state: &WizardState, _settings: &crate::settings::Settings, _probe: &dyn StatusProbe) -> WizardState {
    unimplemented!("本项目仅面向 Windows（ADR-0001）")
}

/// 覆盖单阶段状态
fn set_stage(state: &mut WizardState, id: WizardStageId, new: StageState, detail: Option<String>) {
    if let Some(slot) = state.stages.iter_mut().find(|s| s.id == id) {
        slot.state = new;
        slot.detail = detail;
    }
}

// ── 应用态与 Tauri 命令 ─────────────────────────────────────────────────────

/// 向导状态持有者（manage；读写经互斥锁，落盘路径固定）
pub struct WizardHolder {
    path: PathBuf,
    current: Mutex<WizardState>,
}

impl WizardHolder {
    /// 启动时加载（损坏恢复语义同 settings）
    pub fn load_at(path: PathBuf) -> Self {
        let (state, repaired) = match load_from(&path) {
            LoadOutcome::Missing(s) | LoadOutcome::Loaded(s) => (s, None),
            LoadOutcome::Repaired { state, backup_path } => {
                log::warn!("向导状态损坏：留档 {} 并回退默认", backup_path.display());
                (state, Some(backup_path))
            }
        };
        // 留档事件在首次命令响应后由前端 get_state 兜底呈现，不单独发事件
        let _ = repaired;
        Self { path, current: Mutex::new(state) }
    }

    fn current(&self) -> WizardState {
        self.current.lock().expect("向导状态锁中毒").clone()
    }

    fn replace(&self, state: WizardState) -> Result<WizardState, String> {
        let mut guard = self.current.lock().expect("向导状态锁中毒");
        save_to(&self.path, &state).map_err(|e| format!("保存向导状态失败：{e}"))?;
        *guard = state.clone();
        Ok(state)
    }
}

/// 探测源（装配层 manage；detect 命令消费）
pub struct WizardDeps {
    pub probe: std::sync::Arc<dyn StatusProbe>,
}

/// 读当前状态（前端首次进入向导调用）
#[tauri::command]
pub fn wizard_get_state(holder: tauri::State<'_, WizardHolder>) -> WizardState {
    holder.current()
}

/// 全量重探测（幂等；外部办理「我已完成」按钮与脚本跑完后的统一刷新入口）
#[tauri::command]
pub async fn wizard_detect(
    holder: tauri::State<'_, WizardHolder>,
    settings: tauri::State<'_, SettingsState>,
    deps: tauri::State<'_, WizardDeps>,
    app: tauri::AppHandle,
) -> Result<WizardState, String> {
    let snapshot = (holder.current(), settings.current(), deps.probe.clone());
    let detected = tauri::async_runtime::spawn_blocking(move || {
        let (state, settings, probe) = snapshot;
        detect(&state, &settings, probe.as_ref())
    })
    .await
    .map_err(|e| format!("探测任务失败：{e}"))?;
    let saved = holder.replace(detected)?;
    emit_changed(&app, &saved);
    Ok(saved)
}

/// 设置访问域名（非敏感；AC5 校验经由 detect）
#[tauri::command]
pub fn wizard_set_domain(
    holder: tauri::State<'_, WizardHolder>,
    domain: String,
    app: tauri::AppHandle,
) -> Result<WizardState, String> {
    let d = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    if d.is_empty() || !d.contains('.') {
        return Err("域名格式不合法".into());
    }
    let mut next = holder.current();
    next.domain = d;
    let saved = holder.replace(next)?;
    emit_changed(&app, &saved);
    Ok(saved)
}

/// 选择通道分支（阶段④；同步写 Settings.access_channel——
/// frpc 拉起/ddns-go 停托管复用 004 守护收敛，不另起平行机制）
#[tauri::command]
pub fn wizard_set_branch(
    holder: tauri::State<'_, WizardHolder>,
    settings: tauri::State<'_, SettingsState>,
    branch: AccessChannel,
    app: tauri::AppHandle,
) -> Result<WizardState, String> {
    settings.patch(&crate::settings::SettingsPatch {
        access_channel: Some(branch),
        ..Default::default()
    })?;
    let mut next = holder.current();
    next.branch = Some(branch);
    let saved = holder.replace(next)?;
    emit_changed(&app, &saved);
    Ok(saved)
}

/// 收尾完成（AC11：done 置位；自启注册由前端复用 set_autostart_* 命令）
#[tauri::command]
pub fn wizard_complete(
    holder: tauri::State<'_, WizardHolder>,
    app: tauri::AppHandle,
) -> Result<WizardState, String> {
    let mut next = holder.current();
    next.done = true;
    set_stage(&mut next, WizardStageId::Finalize, StageState::Done, Some("ok".into()));
    let saved = holder.replace(next)?;
    emit_changed(&app, &saved);
    Ok(saved)
}

fn emit_changed(app: &tauri::AppHandle, state: &WizardState) {
    use tauri::Emitter;
    if let Err(e) = app.emit(EVENT_WIZARD_CHANGED, state) {
        log::error!("发送 {EVENT_WIZARD_CHANGED} 失败：{e}");
    }
}

// ── 单元测试（纯函数与持久化；IO 采集不在覆盖范围）──────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn default_state_has_five_pending_stages() {
        let s = WizardState::default();
        assert_eq!(s.stages.len(), 5);
        assert!(s.stages.iter().all(|x| x.state == StageState::Pending));
        assert_eq!(s.domain, DOMAIN);
        assert_eq!(s.branch, None);
        assert!(!s.done);
    }

    #[test]
    fn load_missing_returns_default() {
        let p = std::env::temp_dir().join(format!("wb-wiz-{}-missing.json", std::process::id()));
        let _ = fs::remove_file(&p);
        match load_from(&p) {
            LoadOutcome::Missing(s) => assert_eq!(s, WizardState::default()),
            other => panic!("应为 Missing，实际 {other:?}"),
        }
    }

    #[test]
    fn save_roundtrip_and_partial_compat() {
        let p = std::env::temp_dir().join(format!("wb-wiz-{}-roundtrip.json", std::process::id()));
        let _ = fs::remove_file(&p);
        let mut s = WizardState::default();
        s.branch = Some(AccessChannel::Tunnel);
        s.domain = "ai.example.com".into();
        set_stage(&mut s, WizardStageId::Basis, StageState::Done, Some("ok".into()));
        save_to(&p, &s).expect("保存失败");
        match load_from(&p) {
            LoadOutcome::Loaded(back) => assert_eq!(back, s),
            other => panic!("应为 Loaded，实际 {other:?}"),
        }
        // 旧/缺字段文件：缺的阶段补 Pending，未知字段忽略
        fs::write(&p, r#"{"version":1,"stages":[],"domain":"x.cn","done":false,"branch":"direct"}"#).unwrap();
        match load_from(&p) {
            LoadOutcome::Loaded(s) => {
                assert_eq!(s.stages.len(), 5, "缺阶段应补齐");
                assert_eq!(s.branch, Some(AccessChannel::Direct));
            }
            other => panic!("应为 Loaded，实际 {other:?}"),
        }
        // 损坏 → 留档回默认
        fs::write(&p, "不是 JSON").unwrap();
        match load_from(&p) {
            LoadOutcome::Repaired { state, backup_path } => {
                assert_eq!(state, WizardState::default());
                assert!(backup_path.is_file());
            }
            other => panic!("应为 Repaired，实际 {other:?}"),
        }
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn domain_split_two_labels() {
        assert_eq!(split_domain("ai.jackqi.cn"), ("jackqi.cn".into(), "ai.jackqi.cn".into()));
        assert_eq!(split_domain("Jackqi.CN."), ("jackqi.cn".into(), "jackqi.cn".into()));
        assert_eq!(split_domain("localhost"), ("localhost".into(), "localhost".into()));
    }

    #[test]
    fn tc_error_mapping() {
        assert_eq!(map_tc_error("DescribeRecordList 失败：AuthFailure.SignatureFailure（x）"), "auth_failed");
        assert_eq!(map_tc_error("...UnauthorizedOperation..."), "permission_denied");
        assert_eq!(map_tc_error("...ResourceNotFound.NoDataOfRecord..."), "no_records");
        assert_eq!(map_tc_error("...InvalidDomain..."), "domain_missing");
        assert_eq!(map_tc_error("网络超时"), "api_error");
    }

    #[test]
    fn cgnat_classification() {
        // 私网/CGNAT/链路本地段 → 警示；正常公网 → None；v6/取不到 → 警示
        assert_eq!(cgnat_classify(Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)))), Some("warn_private_range"));
        assert_eq!(cgnat_classify(Some(IpAddr::V4(Ipv4Addr::new(100, 100, 1, 1)))), Some("warn_private_range"), "100.64/10 CGNAT");
        assert_eq!(cgnat_classify(Some(IpAddr::V4(Ipv4Addr::new(100, 128, 1, 1)))), None, "100.128 不在 100.64/10");
        assert_eq!(cgnat_classify(Some(IpAddr::V4(Ipv4Addr::new(117, 182, 1, 1)))), None, "公网段");
        assert_eq!(cgnat_classify(Some(IpAddr::V4(Ipv4Addr::new(169, 254, 3, 4)))), Some("warn_private_range"));
        assert_eq!(cgnat_classify(Some("::1".parse().unwrap())), Some("warn_ipv6"));
        assert_eq!(cgnat_classify(None), Some("warn_no_public_ip"));
    }

    #[test]
    fn stage_derivations() {
        use crate::probe::ProbeState;
        // ① 基础
        assert_eq!(derive_basis(&ProbeState::Running { listen_addrs: vec![] }).0, StageState::Done);
        assert_eq!(derive_basis(&ProbeState::Stopped).0, StageState::Pending);
        assert_eq!(
            derive_basis(&ProbeState::PortHeld { process_name: "nginx.exe".into() }),
            (StageState::Failed, Some("port_held".into()))
        );
        // ② 腾讯云
        assert_eq!(derive_tencent(false, &TencentApiOutcome::Err("x".into())).0, StageState::Pending);
        assert_eq!(derive_tencent(true, &TencentApiOutcome::Ok).0, StageState::Done);
        assert_eq!(derive_tencent(true, &TencentApiOutcome::NoRecords).0, StageState::Done, "域名存在暂无记录即可过（ddns-go 会建）");
        assert_eq!(
            derive_tencent(true, &TencentApiOutcome::Err("AuthFailure.SignatureFailure".into())).1,
            Some("auth_failed".into())
        );
        // ③ HTTPS 栈（旧式 Caddyfile + 运行中 = 旧机已在工作，同样 Done）
        assert_eq!(derive_https(false, true, true, true, false).1, Some("missing_caddy".into()));
        assert_eq!(derive_https(true, false, true, true, false).1, Some("missing_ddnsgo".into()));
        assert_eq!(derive_https(true, true, false, true, false).1, Some("missing_caddyfile".into()));
        assert_eq!(derive_https(true, true, true, true, false).1, Some("not_running".into()));
        assert_eq!(derive_https(true, true, true, true, true).0, StageState::Done);
        assert_eq!(derive_https(true, true, true, true, true).1, Some("ok".into()));
        assert_eq!(
            derive_https(true, true, true, false, true),
            (StageState::Done, Some("legacy_ok".into())),
            "旧式证书链 + 443 运行中 = 旧机已在工作"
        );
        // ④ 直连 / 穿透
        assert_eq!(derive_direct(false, false).1, Some("missing_yaml".into()));
        assert_eq!(derive_direct(true, false).1, Some("not_running".into()));
        assert_eq!(derive_direct(true, true).0, StageState::Done);
        assert_eq!(derive_tunnel(false, true).1, Some("missing_key".into()));
        assert_eq!(derive_tunnel(true, false).1, Some("tunnel_unset".into()));
        assert_eq!(derive_tunnel(true, true), (StageState::Done, Some("ok".into())));
    }

    #[test]
    fn normalize_repairs_partial_stage_list() {
        let mut s = WizardState::default();
        s.stages.clear();
        let n = s.normalized();
        assert_eq!(n.stages.len(), 5);
        assert_eq!(n.stages[0].id, WizardStageId::Basis);
        assert_eq!(n.stages[4].id, WizardStageId::Finalize);
    }
}
