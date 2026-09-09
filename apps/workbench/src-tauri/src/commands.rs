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
