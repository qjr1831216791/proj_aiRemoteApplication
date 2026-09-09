//! UI 命令层（T13/T15：plan §5.1 剩余命令的薄封装）。
//!
//! 原则：命令层薄，逻辑在既有模块（orchestrator/scripts/urls/lang）。
//! - `get_status` / `get_urls`：快照读取（启动兜底；此后状态走事件）
//! - `start_all` / `start_one`：编排器立即返回（线程化），状态经 `status://changed` 推进
//! - `stop_all` / `stop_one`：停止管线 10s 预算 → `spawn_blocking` 不阻塞 UI 线程
//! - `open_external` / `run_tool` / `open_logs_dir` / `scripts_availability`：低频操作区

use crate::autostart::AutostartContext;
use crate::lang::{self, LanguageState};
use crate::network::{self, NetMonitor, NetStatus};
use crate::orchestrator::{ComponentStatus, Orchestrator};
use crate::probe::ComponentId;
use crate::scripts::{self, ToolKind, ToolOpts};
use crate::stop::StopOutcome;
use crate::urls::{self, ExternalKind};
use serde::Serialize;

/// 当前全组件状态快照（启动时兜底；此后以 `status://changed` 事件为准）
#[tauri::command]
pub fn get_status(orch: tauri::State<'_, Orchestrator>) -> Vec<ComponentStatus> {
    orch.statuses()
}

/// 三端访问地址（AC19 地址区）
#[tauri::command]
pub fn get_urls() -> urls::AccessUrls {
    urls::access_urls()
}

/// 一键启动（AC1/AC3/AC6）：立即返回，状态经事件推进；部分失败按组件实况展示
#[tauri::command]
pub fn start_all(orch: tauri::State<'_, Orchestrator>) {
    orch.start_all();
}

/// 组件级启动/重试（AC6）
#[tauri::command]
pub fn start_one(orch: tauri::State<'_, Orchestrator>, id: ComponentId) -> Result<(), String> {
    orch.start_one(id).map_err(|e| e.to_string())
}

/// 一键停止（AC2）：三组件并行（单组件 10s 预算）；失败组件汇总为 Err 交前端提示。
/// 停止前自动取消/等待在途启动（spec §4.4 竞态消除，见 orchestrator.stop_one）。
#[tauri::command]
pub async fn stop_all(
    orch: tauri::State<'_, Orchestrator>,
    lang_state: tauri::State<'_, LanguageState>,
) -> Result<(), String> {
    let orch = orch.inner().clone();
    let outcomes = tauri::async_runtime::spawn_blocking(move || orch.stop_all())
        .await
        .map_err(|e| lang::err_texts(lang_state.current()).stop_join_failed(&e.to_string()))?;
    summarize_stop(outcomes)
}

/// 组件级停止
#[tauri::command]
pub async fn stop_one(
    orch: tauri::State<'_, Orchestrator>,
    lang_state: tauri::State<'_, LanguageState>,
    id: ComponentId,
) -> Result<(), String> {
    let orch = orch.inner().clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || orch.stop_one(id))
        .await
        .map_err(|e| lang::err_texts(lang_state.current()).stop_join_failed(&e.to_string()))?;
    summarize_stop(vec![(id, outcome)])
}

/// 停止结论 → 汇总（全部成功 Ok；失败/超时组件的原因合并为 Err，AC2）
fn summarize_stop(outcomes: Vec<(ComponentId, StopOutcome)>) -> Result<(), String> {
    let failures: Vec<String> = outcomes
        .into_iter()
        .filter_map(|(id, o)| match o {
            StopOutcome::Failed(detail) | StopOutcome::TimedOut(detail) => {
                Some(format!("[{}] {detail}", id.as_str()))
            }
            StopOutcome::Stopped | StopOutcome::AlreadyStopped => None,
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// 用默认浏览器打开目标（workbench/local/lan/domain/ddns_admin，AC19）
#[tauri::command]
pub fn open_external(
    kind: ExternalKind,
    lang_state: tauri::State<'_, LanguageState>,
) -> Result<(), String> {
    let url = urls::external_url(kind, &urls::access_urls());
    scripts::open_url(&url).map_err(|code| lang::shell_error_text(code, lang_state.current()))
}

/// 低频工具派发（AC19/20）：提权类 runas 可见窗、install-client 普通可见交互窗。
/// UAC 拒绝/启动失败 → Err（前端明确提示，不崩溃）。
/// ShellExecuteW(runas) 在 UAC 弹窗期间可能不返回 → 后台线程执行不冻结 UI。
#[tauri::command]
pub async fn run_tool(
    ctx: tauri::State<'_, AutostartContext>,
    lang_state: tauri::State<'_, LanguageState>,
    kind: ToolKind,
    opts: ToolOpts,
) -> Result<(), String> {
    let lang = lang_state.current();
    let Some(dir) = ctx.scripts_dir.clone() else {
        // spec §4.5：脚本目录不可用 → 禁用原因透传
        let reason = ctx
            .scripts_disabled_reason
            .clone()
            .unwrap_or_else(|| lang::detail_texts(lang).scripts_dir_unavailable());
        return Err(reason);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let plan = scripts::tool_plan(kind, opts, &dir, lang);
        if plan.elevated {
            scripts::shell_execute(Some("runas"), "powershell.exe", &plan.params)
        } else {
            scripts::open_visible("powershell.exe", &plan.params)
        }
    })
    .await
    .map_err(|e| lang::err_texts(lang).tool_join_failed(&e.to_string()))?
    .map_err(|code| lang::shell_error_text(code, lang))
}

/// 脚本可用性（spec §4.5：按钮禁用 + 原因透传）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptsAvailability {
    pub available: bool,
    pub reason: Option<String>,
}

#[tauri::command]
pub fn scripts_availability(ctx: tauri::State<'_, AutostartContext>) -> ScriptsAvailability {
    ScriptsAvailability {
        available: ctx.scripts_dir.is_some(),
        reason: ctx.scripts_disabled_reason.clone(),
    }
}

/// 用资源管理器打开日志目录（设置页入口，§4.3）
#[tauri::command]
pub fn open_logs_dir(
    ctx: tauri::State<'_, AutostartContext>,
    lang_state: tauri::State<'_, LanguageState>,
) -> Result<(), String> {
    let dir = &ctx.log_dir;
    // 首次打开前目录可能尚无文件：先建目录再定位
    std::fs::create_dir_all(dir)
        .map_err(|e| lang::err_texts(lang_state.current()).log_dir_create_failed(&e.to_string()))?;
    scripts::open_dir(dir).map_err(|code| lang::shell_error_text(code, lang_state.current()))
}

/// 网络环境快照（spec 002 AC1/AC2）：即时探测（成功同时刷新监测器缓存，
/// 变化经 `net://changed` 去重发声）；探测失败回上次状态，无缓存则 None（AC4）
#[tauri::command]
pub async fn get_net_status(
    monitor: tauri::State<'_, NetMonitor>,
) -> Result<Option<NetStatus>, String> {
    let m = monitor.inner().clone();
    tauri::async_runtime::spawn_blocking(move || m.refresh())
        .await
        .map_err(|e| format!("网络探测线程失败：{e}"))
}

/// 网络归类调整（spec 002 AC5/AC6/AC7）：UAC 提权派发、立即返回，
/// 生效以 `net://changed` 复测为准；非法归类/UAC 拒绝 → Err 明确提示。
/// 定位以网络名优先、接口序号兜底（序号随适配器重枚举漂移，真机实测）。
#[tauri::command]
pub async fn set_network_category(
    lang_state: tauri::State<'_, LanguageState>,
    name: String,
    if_index: u32,
    category: String,
) -> Result<(), String> {
    let lang = lang_state.current();
    let params = network::set_category_params(&name, if_index, &category)?;
    tauri::async_runtime::spawn_blocking(move || {
        scripts::shell_execute(Some("runas"), "powershell.exe", &params)
    })
    .await
    .map_err(|e| format!("提权派发线程失败：{e}"))?
    .map_err(|code| lang::shell_error_text(code, lang))
}

// ── 穿透通道（spec 004：AC5/6/7/11/12/13 命令层；逻辑在 tunnel/settings）────

use crate::settings::SettingsPatch;
use crate::tunnel::{
    judge_dns, switch_actions, DnsAlignment, FrpcOps, SwitchAction, SwitchReject, TunnelManager,
    TunnelStatus,
};

/// 隧道状态快照（启动兜底；此后以 `tunnel://status` 事件为准）。
/// State 泛型须与 lib.rs manage 的类型精确一致（Arc 包裹），
/// 否则 Tauri 找不到状态（"state not managed"）。
#[tauri::command]
pub fn get_tunnel_status(mgr: tauri::State<'_, std::sync::Arc<TunnelManager>>) -> TunnelStatus {
    mgr.status()
}

/// 通道切换（AC5/6/7）：前置就绪校验（无副作用拒绝）→ 按状态机动作序执行 →
/// 持久化。「先起新再停旧」序保证起新失败时旧通道无恙（tunnel.rs 状态机注释）。
#[tauri::command]
pub async fn switch_channel(
    orch: tauri::State<'_, Orchestrator>,
    mgr: tauri::State<'_, std::sync::Arc<TunnelManager>>,
    settings: tauri::State<'_, crate::settings::SettingsState>,
    target: crate::settings::AccessChannel,
) -> Result<crate::settings::Settings, String> {
    use crate::tunnel::WindowsFrpcOps;

    let cur = settings.current();
    // 前置就绪：穿透配置（settings.tunnel）∧ frpc 二进制 ∧ 访问密钥（AC7/AC14）
    let ops = WindowsFrpcOps;
    let tunnel_ready = cur.tunnel.is_some() && ops.exe_exists() && ops.access_key().is_some();
    let actions = switch_actions(cur.access_channel, target, tunnel_ready).map_err(|e| match e {
        SwitchReject::NotConfigured { .. } => {
            "穿透未就绪：请先在穿透设置中填写隧道 ID 与节点域名，并按指引配置 frpc.exe 与访问密钥"
                .to_string()
        }
        SwitchReject::AlreadyOnTarget => "已处于目标通道".to_string(),
    })?;

    for action in actions {
        match action {
            SwitchAction::StartFrpc => mgr.start()?,
            SwitchAction::StopFrpc => mgr.stop(),
            SwitchAction::StartDdnsGo => orch
                .start_one(ComponentId::DdnsGo)
                .map_err(|e| format!("ddns-go 启动派发失败：{e}"))?,
            SwitchAction::StopDdnsGo => {
                // 停止管线有 10s 预算 → spawn_blocking 不阻塞 UI 线程（沿 stop_all）
                let orch2 = orch.inner().clone();
                let outcome = tauri::async_runtime::spawn_blocking(move || {
                    orch2.stop_one(ComponentId::DdnsGo)
                })
                .await
                .map_err(|e| format!("停止线程失败：{e}"))?;
                if !matches!(outcome, StopOutcome::Stopped | StopOutcome::AlreadyStopped) {
                    return Err(format!(
                        "ddns-go 停止未确认（{outcome:?}）：frpc 已启动，请检查 ddns-go 状态后重试"
                    ));
                }
            }
            SwitchAction::Persist(ch) => {
                settings
                    .patch(&SettingsPatch { access_channel: Some(ch), ..Default::default() })
                    .map_err(|e| format!("通道持久化失败：{e}"))?;
            }
        }
    }
    Ok(settings.current())
}

/// 穿透启用/停用（AC11）：非穿透通道下仅改开关（守护循环按通道×开关收敛，
/// 不在此处拉起/停止，避免与守护竞争）
#[tauri::command]
pub fn set_tunnel_enabled(
    settings: tauri::State<'_, crate::settings::SettingsState>,
    enabled: bool,
) -> Result<crate::settings::Settings, String> {
    settings.patch(&SettingsPatch {
        tunnel_enabled: Some(enabled),
        ..Default::default()
    })
}

/// DNS 对齐检测（AC12/13）：权威 NS 上的 CNAME/A 实况 → 对齐结论。
/// `Err` = 查询本身失败（网络/解析器异常），前端如实显示"检测失败"。
#[tauri::command]
pub async fn check_dns_alignment(
    settings: tauri::State<'_, crate::settings::SettingsState>,
) -> Result<DnsAlignment, String> {
    use crate::consts::{DOMAIN, DOMAIN_ROOT};
    let node_domain = settings
        .current()
        .tunnel
        .map(|t| t.node_domain)
        .ok_or("穿透未配置，无需 DNS 对齐检测")?;
    let probe = tauri::async_runtime::spawn_blocking(move || run_dns_probe(DOMAIN_ROOT, DOMAIN))
        .await
        .map_err(|e| format!("DNS 检测线程失败：{e}"))??;
    Ok(judge_dns(probe.cname.as_deref(), probe.has_a, &node_domain))
}

/// Resolve-DnsName 三连查（NS → 权威 CNAME/A）的合成输出
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DnsProbeResult {
    ns: Option<String>,
    cname: Option<String>,
    has_a: bool,
}

/// PowerShell 采集（沿 network.rs 先例：UTF8 输出 + CREATE_NO_WINDOW，一次
/// 进程调用完成三查）。CNAME/A 无记录时字段为 null/false；NS 失败 = 权威
/// 不可达 = 整体 Err。
fn run_dns_probe(domain_root: &str, domain: &str) -> Result<DnsProbeResult, String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let script = format!(
        r#"[Console]::OutputEncoding=[Text.Encoding]::UTF8;
$ErrorActionPreference='SilentlyContinue';
$ns=@(Resolve-DnsName -Name {root} -Type NS -Server 223.5.5.5 -ErrorAction SilentlyContinue | Where-Object {{$_.Type -eq 2}} | Select-Object -First 1).NameHost;
if(-not $ns){{ Write-Output '{{"ns":null,"cname":null,"hasA":false}}'; exit }};
$cn=@(Resolve-DnsName -Name {dom} -Type CNAME -Server $ns -ErrorAction SilentlyContinue | Where-Object {{$_.Type -eq 5}} | Select-Object -First 1).NameHost;
$aa=@(Resolve-DnsName -Name {dom} -Type A -Server $ns -ErrorAction SilentlyContinue | Where-Object {{$_.Type -eq 1}});
[pscustomobject]@{{ns=$ns;cname=[string]$cn;hasA=($aa.Count -gt 0)}} | ConvertTo-Json -Compress"#,
        root = domain_root,
        dom = domain,
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(crate::scripts::CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("PowerShell 拉起失败：{e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: DnsProbeResult = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("DNS 探测输出解析失败：{e}（输出：{:.200}）", stdout.trim()))?;
    if parsed.ns.is_none() {
        return Err("权威 DNS 查询失败：无法获取 NS 记录".into());
    }
    Ok(parsed)
}

// ── 单元测试（纯逻辑：停止汇总）────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn summarize_stop_passes_on_clean_outcomes() {
        assert_eq!(
            summarize_stop(vec![
                (ComponentId::CloudCli, StopOutcome::Stopped),
                (ComponentId::Caddy, StopOutcome::AlreadyStopped),
            ]),
            Ok(())
        );
    }

    #[test]
    fn summarize_stop_collects_failure_details() {
        // AC2：失败/超时组件的原因合并上报，其余组件不受影响
        let err = summarize_stop(vec![
            (ComponentId::CloudCli, StopOutcome::Failed("复核未通过".into())),
            (ComponentId::Caddy, StopOutcome::Stopped),
            (ComponentId::DdnsGo, StopOutcome::TimedOut("超时".into())),
        ])
        .unwrap_err();
        assert!(err.contains("[cloudcli]") && err.contains("复核未通过"), "{err}");
        assert!(err.contains("[ddnsgo]") && err.contains("超时"), "{err}");
    }

    #[test]
    fn arc_orchestrator_is_shareable_for_blocking() {
        // 编译期语义：Orchestrator 可跨线程共享（spawn_blocking 前提）
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Arc<Orchestrator>>();
    }
}
