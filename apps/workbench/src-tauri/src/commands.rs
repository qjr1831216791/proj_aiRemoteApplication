//! UI 命令层（T13/T15：plan §5.1 剩余命令的薄封装）。
//!
//! 原则：命令层薄，逻辑在既有模块（orchestrator/scripts/urls/lang）。
//! - `get_status` / `get_urls`：快照读取（启动兜底；此后状态走事件）
//! - `start_all` / `start_one`：编排器立即返回（线程化），状态经 `status://changed` 推进
//! - `stop_all` / `stop_one`：停止管线 10s 预算 → `spawn_blocking` 不阻塞 UI 线程
//! - `open_external` / `run_tool` / `open_logs_dir` / `scripts_availability`：低频操作区

use crate::autostart::AutostartContext;
use crate::heartbeat;
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

/// 一键停止（AC2）：两组件并行（单组件 10s 预算）；失败组件汇总为 Err 交前端提示。
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

/// 用默认浏览器打开目标（workbench/local/lan/domain，AC19；ddns_admin 已随
/// 直连通道退役——spec 008）
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
    let stack_dir = ctx
        .stack_dir
        .clone()
        .unwrap_or_else(|| crate::consts::DEFAULT_STACK_DIR.to_string());
    tauri::async_runtime::spawn_blocking(move || {
        let plan = scripts::tool_plan(kind, opts, &dir, lang, &stack_dir);
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

// ── 组网通道（spec 007 T9：plan §5.1 命令层；逻辑在 mesh/dns_api）──────────

use crate::dns_api::{judge_dns_mesh, DnsAlignment};
use crate::mesh::MeshStatus;

/// UAC 派发（ShellExecuteW runas）：UAC 弹窗期间可能不返回 → 后台线程执行
/// 不冻结 UI（run_tool 同纪律）；elevate 已把结果码转为可读文案（5=UAC 被拒）
async fn dispatch_elevated(params: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || scripts::elevate("powershell.exe", &params))
        .await
        .map_err(|e| format!("提权派发线程失败：{e}"))?
}

/// 组网状态快照（启动兜底；此后以 `mesh://status` 事件为准——AC4 四态 + 失败
/// 摘要 detail 稳定码，构造上不含密钥）
#[tauri::command]
pub fn mesh_status(
    monitor: tauri::State<'_, std::sync::Arc<crate::mesh::MeshMonitor>>,
) -> MeshStatus {
    monitor.status()
}

/// 组网配置生效（AC1/AC9）：密钥就绪检查 → 渲染 config.toml → 二进制缺失时
/// 落位 → `--check-config` 校验 → 按服务实况 UAC 派发 install/restart。
#[tauri::command]
pub async fn mesh_apply_config(
    ctx: tauri::State<'_, AutostartContext>,
    settings: tauri::State<'_, crate::settings::SettingsState>,
    lang_state: tauri::State<'_, LanguageState>,
) -> Result<(), String> {
    apply_mesh_effective(&ctx, &settings, lang_state.current()).await
}

/// mesh 生效内核：磁盘段（prepare_stack，spawn_blocking）→ 服务实况选动作
/// → UAC 组合派发（服务动作 + ensure-whitelist 同窗，spec 010 AC9）。任一步
/// Err 均含可读指引且不含密钥值（AC8/AC9）。
async fn apply_mesh_effective(
    ctx: &tauri::State<'_, AutostartContext>,
    settings: &tauri::State<'_, crate::settings::SettingsState>,
    lang: crate::lang::Lang,
) -> Result<(), String> {
    use crate::mesh::MeshOps;

    prepare_mesh_stack(ctx, settings).await?;
    let Some(dir) = ctx.scripts_dir.clone() else {
        return Err("脚本目录不可用：无法派发组网服务动作（mesh-service.ps1）".into());
    };
    let cur = settings.current();
    let stack = cur.stack_dir;
    let ops = crate::mesh::WindowsMeshOps::new(ctx.scripts_dir.clone(), stack.clone());
    let action = crate::mesh::apply_service_action(ops.service_state());
    // spec 010 AC9：prepare 通过后组合派发（校验失败路径在上面 ？ 处整体短路，
    // 不产生任何派发——白名单不会被未过校验的网段刷新）
    let params = combined_apply_params(
        std::path::Path::new(&dir),
        action,
        &stack,
        &cur.mesh.virtual_cidr,
        &cur.mesh.virtual_ip,
        lang,
    );
    dispatch_elevated(params).await
}

/// apply/install 的组合派发参数（AC9 构造面，纯函数可单测）：服务动作 +
/// ensure-whitelist 段同窗顺序、单次 UAC、`-WaitTun 20`。白名单段构造失败
/// （设置异常的防御分支）→ 回退仅服务段并记日志——白名单失败不阻断服务段
/// （plan §3.5/§7-R4；失配由健康自检横幅兜底）。
fn combined_apply_params(
    scripts_dir: &std::path::Path,
    action: &str,
    stack_dir: &str,
    cidr: &str,
    virtual_ip: &str,
    lang: crate::lang::Lang,
) -> String {
    crate::mesh::service_with_whitelist_params(scripts_dir, action, stack_dir, cidr, virtual_ip, lang)
        .unwrap_or_else(|e| {
            log::warn!("ensure-whitelist 段构造失败（{e}）：回退仅服务动作（失配由健康自检兜底）");
            crate::mesh::service_action_params(scripts_dir, action, stack_dir, lang)
        })
}

/// 生效流水线磁盘段（apply/install 共享前置）：渲染 → 落位 → 写盘 → 校验
async fn prepare_mesh_stack(
    ctx: &tauri::State<'_, AutostartContext>,
    settings: &tauri::State<'_, crate::settings::SettingsState>,
) -> Result<(), String> {
    let cur = settings.current();
    let stack = cur.stack_dir.clone();
    let cfg = cur.mesh.clone();
    let src_bin = ctx.scripts_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::mesh::prepare_stack(&cfg, &stack, src_bin.as_deref()).map(|_| ())
    })
    .await
    .map_err(|e| format!("组网配置生效线程失败：{e}"))?
}

/// 安装/刷新组网服务（AC3 前置）：落位五文件 + 渲染 config（缺密钥 → Err
/// 指引）+ UAC install（幂等：已存在则刷新 binPath 与自愈配置——显式装服务
/// 入口强制 install 动作，与 apply 的「已装即 restart」分流）
#[tauri::command]
pub async fn mesh_install_service(
    ctx: tauri::State<'_, AutostartContext>,
    settings: tauri::State<'_, crate::settings::SettingsState>,
    lang_state: tauri::State<'_, LanguageState>,
) -> Result<(), String> {
    prepare_mesh_stack(&ctx, &settings).await?;
    let lang = lang_state.current();
    let Some(dir) = ctx.scripts_dir.clone() else {
        return Err("脚本目录不可用：无法安装组网服务（mesh-service.ps1）".into());
    };
    let cur = settings.current();
    // spec 010 AC9：install 同样组合派发（新装/重装时白名单一并就位，单 UAC）
    let params = combined_apply_params(
        std::path::Path::new(&dir),
        "install",
        &cur.stack_dir,
        &cur.mesh.virtual_cidr,
        &cur.mesh.virtual_ip,
        lang,
    );
    dispatch_elevated(params).await
}

/// 卸载组网服务（停用 mesh 的清理路径；服务承载进程一并消失）
#[tauri::command]
pub async fn mesh_uninstall_service(
    ctx: tauri::State<'_, AutostartContext>,
    settings: tauri::State<'_, crate::settings::SettingsState>,
    lang_state: tauri::State<'_, LanguageState>,
) -> Result<(), String> {
    let lang = lang_state.current();
    let Some(dir) = ctx.scripts_dir.clone() else {
        return Err("脚本目录不可用：无法卸载组网服务（mesh-service.ps1）".into());
    };
    let stack = settings.current().stack_dir;
    let params = crate::mesh::service_action_params(&dir, "uninstall", &stack, lang);
    dispatch_elevated(params).await
}

/// 同步 DNS 到组网通道（spec 007 AC13 / spec 008 组网单通道）：
/// CNAME 全删 + A upsert 虚拟 IP。前置：腾讯云凭证存在（栈 .env 单源，spec 008）。
/// 独立入口的缘由：装机向导组网分支只写设置不做 DNS 编排，全新装机时
/// A 记录无人创建，AC13 需要显式入口兜底。
#[tauri::command]
pub async fn mesh_sync_dns(
    settings: tauri::State<'_, crate::settings::SettingsState>,
) -> Result<usize, String> {
    use crate::consts::{DOMAIN, DOMAIN_ROOT};

    let cur = settings.current();
    if cur.access_channel != crate::settings::AccessChannel::Mesh {
        return Err("组网通道未现役：请先在向导选择组网，或到主界面切换到组网".into());
    }
    let cred = crate::dns_api::read_credential(&cur.stack_dir)
        .ok_or("腾讯云凭证不存在：请先完成向导「腾讯云前置」阶段（写入密钥）")?;
    let sub = crate::dns_api::subdomain_of(DOMAIN, DOMAIN_ROOT).to_string();
    let virtual_ip = cur.mesh.virtual_ip.clone();
    let count = tauri::async_runtime::spawn_blocking(move || {
        crate::dns_api::sync_to_mesh(&cred, DOMAIN_ROOT, &sub, &virtual_ip)
    })
    .await
    .map_err(|e| format!("DNS 同步线程失败：{e}"))??;
    log::info!("组网 DNS 同步完成（{count} 条记录操作）");
    Ok(count)
}

/// 组网诊断（T16，AC13）：六项只读探测——服务态/密钥就绪/逐条对端 TCP 可达/
/// 成员清单（无虚拟 IP 标记 no_addr）/本机虚拟网卡/域名解析+443 链路。
/// 探测含子进程与网络 IO（逐对端 3s 超时），spawn_blocking 执行不堵 UI；
/// 结果为数据摘要，不含密钥（AC8）。
#[tauri::command]
pub async fn mesh_diagnostics(
    settings: tauri::State<'_, crate::settings::SettingsState>,
) -> Result<Vec<crate::mesh::DiagItem>, String> {
    let snapshot = settings.current();
    tauri::async_runtime::spawn_blocking(move || {
        crate::mesh::run_diagnostics(&snapshot.mesh, &snapshot.stack_dir, crate::consts::DOMAIN)
    })
    .await
    .map_err(|e| format!("诊断任务失败：{e}"))
}

/// 成员入网配置（spec 009 US4/AC9~AC11）：从已保存组网设置渲染 EasyTier 官方
/// 最小口径 TOML 文本（关键字段带 App 输入项行注释；network_secret 为占位符
/// + set-mesh-secret.ps1 指引——真实密钥永不进 APP 界面，spec 007 AC8 延续）。
/// 纯内存拼装无 IO；网络名/对端未配置 → Err 提示先完成组网设置。
#[tauri::command]
pub async fn mesh_member_config(
    settings: tauri::State<'_, crate::settings::SettingsState>,
) -> Result<String, String> {
    let cfg = settings.current().mesh;
    crate::mesh::render_member_config(&cfg)
}

// ── 局域网边界守卫（spec 010 T4：plan §5.2 命令层；逻辑在 lan_guard/mesh）──

use crate::lan_guard;
use crate::lan_guard::{LanGuardAction, LanGuardMonitor, LanHealth};

/// 白名单健康快照（plan §5.2）：即时探测一次（成功同时刷新监视器缓存，变化经
/// `languard://changed` 去重发声）；探测失败回上次缓存，无缓存 → None
#[tauri::command]
pub async fn lan_guard_status(
    monitor: tauri::State<'_, std::sync::Arc<LanGuardMonitor>>,
) -> Result<Option<LanHealth>, String> {
    let m = monitor.inner().clone();
    tauri::async_runtime::spawn_blocking(move || m.refresh())
        .await
        .map_err(|e| format!("白名单探测线程失败：{e}"))
}

/// 例外开关（AC6/AC7/AC8）：on → UAC 派发 exception-on → **成功后**持久化
/// {enabled:true, since:now}（规则未生效由 pending 如实呈现，plan §3.4）；off →
/// UAC 派发 exception-off → 复测无规则再清标记。UAC 拒绝（code 5）→ Err 且
/// 设置不写、状态原样（AC7）。
#[tauri::command]
pub async fn lan_guard_set_exception(
    ctx: tauri::State<'_, AutostartContext>,
    settings: tauri::State<'_, crate::settings::SettingsState>,
    monitor: tauri::State<'_, std::sync::Arc<LanGuardMonitor>>,
    lang_state: tauri::State<'_, LanguageState>,
    on: bool,
) -> Result<(), String> {
    let lang = lang_state.current();
    let Some(dir) = ctx.scripts_dir.clone() else {
        return Err(ctx.scripts_disabled_reason.clone().unwrap_or_else(|| {
            "脚本目录不可用：无法派发局域网守卫动作".into()
        }));
    };
    let action =
        if on { LanGuardAction::ExceptionOn } else { LanGuardAction::ExceptionOff };
    let params = lan_guard::dispatch_params(std::path::Path::new(&dir), action, None, None, None, lang)?;
    // UAC 拒绝/派发失败 → Err 直接返回，以下持久化不执行（AC7）
    dispatch_elevated(params).await?;

    if on {
        let patch = crate::settings::SettingsPatch {
            lan_guard: Some(crate::settings::LanGuardSettings {
                exception_enabled: true,
                exception_since_ms: lan_guard::now_ms(),
            }),
            ..Default::default()
        };
        settings.patch(&patch).map_err(|e| format!("例外标记持久化失败：{e}"))?;
    } else {
        // off：提权窗 fire-and-forget，给删除动作一个短复测确认窗——确认「规则
        // 已不在」才清标记（复测未确认 → Err，标记原样，状态机按实况收敛）
        let m = monitor.inner().clone();
        let confirmed = tauri::async_runtime::spawn_blocking(move || {
            poll_rule_absent(
                || {
                    m.refresh().map(|h| {
                        !matches!(
                            h.exception,
                            lan_guard::ExceptionState::Off | lan_guard::ExceptionState::Pending
                        )
                    })
                },
                6,
                std::time::Duration::from_millis(400),
            )
        })
        .await
        .map_err(|e| format!("例外复测线程失败：{e}"))?;
        if !confirmed {
            return Err("例外规则删除未确认（设置未改动）：请稍后重试或查看访问白名单健康状态".into());
        }
        let patch = crate::settings::SettingsPatch {
            lan_guard: Some(crate::settings::LanGuardSettings::default()),
            ..Default::default()
        };
        settings.patch(&patch).map_err(|e| format!("例外标记持久化失败：{e}"))?;
    }
    // 动作后即时刷新：标记落盘后的最终态推给前端（变化才发声）
    let m = monitor.inner().clone();
    let _ = tauri::async_runtime::spawn_blocking(move || m.refresh()).await;
    Ok(())
}

/// 复测确认循环（纯逻辑，单测零延时驱动）：闭包返回 Some(规则已不在)，任一次
/// true 即确认；首次立即探测、其后按 delay 间隔；attempts 耗尽 → 未确认
fn poll_rule_absent(
    mut rule_absent: impl FnMut() -> Option<bool>,
    attempts: u32,
    delay: std::time::Duration,
) -> bool {
    for i in 0..attempts {
        if i > 0 && !delay.is_zero() {
            std::thread::sleep(delay);
        }
        if rule_absent() == Some(true) {
            return true;
        }
    }
    false
}

/// migrate / ensure-whitelist 的提权派发（plan §5.2）：参数携 CIDR + VirtualIp
/// （T1 冻结契约）；fire-and-forget，派发成功后即时刷新（真相以 status 复测为
/// 准，plan R3）。消费方：迁移横幅/向导收尾（T7）、失配修复按钮（T6）。
async fn dispatch_lan_guard_action(
    ctx: &tauri::State<'_, AutostartContext>,
    settings: &tauri::State<'_, crate::settings::SettingsState>,
    monitor: &tauri::State<'_, std::sync::Arc<LanGuardMonitor>>,
    lang_state: &tauri::State<'_, LanguageState>,
    action: LanGuardAction,
) -> Result<(), String> {
    let lang = lang_state.current();
    let Some(dir) = ctx.scripts_dir.clone() else {
        return Err(ctx.scripts_disabled_reason.clone().unwrap_or_else(|| {
            "脚本目录不可用：无法派发局域网守卫动作".into()
        }));
    };
    let cur = settings.current();
    let params = lan_guard::dispatch_params(
        std::path::Path::new(&dir),
        action,
        Some(&cur.mesh.virtual_cidr),
        Some(&cur.mesh.virtual_ip),
        None,
        lang,
    )?;
    dispatch_elevated(params).await?;
    let m = monitor.inner().clone();
    let _ = tauri::async_runtime::spawn_blocking(move || m.refresh()).await;
    Ok(())
}

/// 存量迁移「一键收口」（AC10）：UAC 派发 migrate（幂等删两条旧规则 →
/// ensure-whitelist）；横幅/向导收尾页消费（T6/T7 接线）
#[tauri::command]
pub async fn lan_guard_migrate(
    ctx: tauri::State<'_, AutostartContext>,
    settings: tauri::State<'_, crate::settings::SettingsState>,
    monitor: tauri::State<'_, std::sync::Arc<LanGuardMonitor>>,
    lang_state: tauri::State<'_, LanguageState>,
) -> Result<(), String> {
    dispatch_lan_guard_action(&ctx, &settings, &monitor, &lang_state, LanGuardAction::Migrate).await
}

/// 白名单失配修复（AC1/AC9 判定面）：UAC 派发 ensure-whitelist；MeshCard
/// 「修复白名单」按钮消费（T6 接线）
#[tauri::command]
pub async fn lan_guard_ensure_whitelist(
    ctx: tauri::State<'_, AutostartContext>,
    settings: tauri::State<'_, crate::settings::SettingsState>,
    monitor: tauri::State<'_, std::sync::Arc<LanGuardMonitor>>,
    lang_state: tauri::State<'_, LanguageState>,
) -> Result<(), String> {
    dispatch_lan_guard_action(
        &ctx,
        &settings,
        &monitor,
        &lang_state,
        LanGuardAction::EnsureWhitelist,
    )
    .await
}

/// DNS 对齐检测（spec 008：组网单通道，AC8：A=虚拟 IP）：权威 NS 上的
/// CNAME/A 实况 → 对齐结论。残留 CNAME 判旁路暴露面（MismatchedCname），
/// A 值比对虚拟 IP（judge_dns_mesh 自 tunnel.rs 迁入 dns_api，spec 008 D2）。
/// `Err` = 查询本身失败（网络/解析器异常），前端如实显示"检测失败"。
#[tauri::command]
pub async fn check_dns_alignment(
    settings: tauri::State<'_, crate::settings::SettingsState>,
) -> Result<DnsAlignment, String> {
    use crate::consts::{DOMAIN, DOMAIN_ROOT};
    let virtual_ip = settings.current().mesh.virtual_ip.clone();
    let probe = tauri::async_runtime::spawn_blocking(move || run_dns_probe(DOMAIN_ROOT, DOMAIN))
        .await
        .map_err(|e| format!("DNS 检测线程失败：{e}"))??;
    Ok(judge_dns_mesh(
        probe.cname.as_deref(),
        probe.a_value.as_deref(),
        &virtual_ip,
    ))
}

/// Resolve-DnsName 三连查（NS → 权威 CNAME/A）的合成输出
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DnsProbeResult {
    ns: Option<String>,
    cname: Option<String>,
    /// 首条 A 记录值（比对虚拟 IP 用；无记录 → null/空；
    /// 脚本输出的 hasA 字段已无消费方，serde 忽略之）
    a_value: Option<String>,
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
$ns=@(Resolve-DnsName -Name {root} -Type NS -Server 223.5.5.5 -ErrorAction SilentlyContinue | Where-Object {{[string]$_.NameHost}} | Select-Object -First 1).NameHost;
if(-not $ns){{ Write-Output '{{"ns":null,"cname":null,"hasA":false,"aValue":null}}'; exit }};
$cn=@(Resolve-DnsName -Name {dom} -Type CNAME -Server $ns -ErrorAction SilentlyContinue | Where-Object {{[string]$_.NameHost}} | Select-Object -First 1).NameHost;
$aa=@(Resolve-DnsName -Name {dom} -Type A -Server $ns -ErrorAction SilentlyContinue | Where-Object {{[string]$_.IPAddress}});
$aVal=if($aa.Count -gt 0){{[string](@($aa | Select-Object -First 1).IPAddress)}}else{{$null}};
[pscustomobject]@{{ns=[string]$ns;cname=[string]$cn;hasA=($aa.Count -gt 0);aValue=$aVal}} | ConvertTo-Json -Compress"#,
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

// ── 域名可达性（spec 005）：打开目录 / 即时探测 ─────────────────────────────

/// 打开栈目录（穿透设置指引的「栈目录」超链接目标）
#[tauri::command]
pub fn open_stack_dir(
    settings: tauri::State<'_, crate::settings::SettingsState>,
) -> Result<(), String> {
    let dir = settings.current().stack_dir;
    scripts::open_dir(std::path::Path::new(&dir))
        .map_err(|code| format!("打开目录失败（退出码 {code}）"))
}

/// 即时域名探测（「通道体检」消费；单次 8s 超时。探测后 poke 心跳立即
/// 补测一轮，保持地址区状态点与体检结论一致，避免红绿矛盾）
#[tauri::command]
pub async fn check_domain_health_now(
    monitor: tauri::State<'_, std::sync::Arc<heartbeat::HealthMonitor>>,
) -> Result<heartbeat::ProbeOutcome, String> {
    use crate::consts::WORKBENCH_URL;
    let outcome = tauri::async_runtime::spawn_blocking(|| heartbeat::probe_once(WORKBENCH_URL))
        .await
        .map_err(|e| format!("探测线程失败：{e}"))?;
    monitor.poke();
    Ok(outcome)
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
            (ComponentId::Caddy, StopOutcome::TimedOut("超时".into())),
        ])
        .unwrap_err();
        assert!(err.contains("[cloudcli]") && err.contains("复核未通过"), "{err}");
        assert!(err.contains("[caddy]") && err.contains("超时"), "{err}");
    }

    #[test]
    fn arc_orchestrator_is_shareable_for_blocking() {
        // 编译期语义：Orchestrator 可跨线程共享（spawn_blocking 前提）
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Arc<Orchestrator>>();
    }

    // ── spec 010 T4：组合派发构造面 + 例外复测循环 ────────────────────────

    /// AC9 组合派发构造：通过路径（prepare 已过的前提由 apply_mesh_effective 的
    /// ？ 序序保证——校验失败整体短路，不产生任何派发串）产出「服务动作 +
    /// ensure-whitelist」同窗单 UAC 参数；白名单段构造失败的防御分支回退仅服务段
    #[test]
    fn combined_apply_params_carries_whitelist_segment_single_uac() {
        let p = combined_apply_params(
            std::path::Path::new(r"C:\app\resources\bin"),
            "restart",
            r"D:\Software\cloudcli-https",
            "10.126.126.0/24",
            "10.126.126.1",
            crate::lang::Lang::Zh,
        );
        assert!(p.contains("mesh-service.ps1") && p.contains("-Action restart"), "{p}");
        assert!(p.contains("-Action ensure-whitelist"), "{p}");
        assert!(p.contains("-WaitTun 20"), "{p}");
        assert_eq!(p.matches("-Command \"").count(), 1, "单窗单次 UAC：{p}");
        assert_eq!(p.matches("-NoExit").count(), 1, "-NoExit 只在外层一次：{p}");

        // 空网段（设置异常防御分支）→ 回退仅服务段：参数串不含 ensure-whitelist
        let fallback = combined_apply_params(
            std::path::Path::new(r"C:\app\resources\bin"),
            "install",
            r"D:\stack",
            "  ",
            "10.0.0.1",
            crate::lang::Lang::Zh,
        );
        assert!(fallback.contains("-Action install"), "{fallback}");
        assert!(
            !fallback.contains("ensure-whitelist"),
            "白名单段构造失败不阻断服务段（plan §3.5）：{fallback}"
        );
    }

    /// off 复测确认循环：任一次「规则已不在」即确认；全 false/None 耗尽 → 未确认
    #[test]
    fn poll_rule_absent_confirms_only_on_observed_absence() {
        let mut calls = 0u32;
        let confirmed = poll_rule_absent(
            || {
                calls += 1;
                Some(calls >= 3)
            },
            6,
            std::time::Duration::ZERO,
        );
        assert!(confirmed, "第三次观察到规则不在 → 确认");
        assert_eq!(calls, 3, "确认即止，不做多余探测");

        assert!(!poll_rule_absent(|| Some(false), 4, std::time::Duration::ZERO), "始终在 → 耗尽未确认");
        assert!(!poll_rule_absent(|| None, 3, std::time::Duration::ZERO), "探测失败（None）不算确认");
        assert!(poll_rule_absent(|| Some(true), 1, std::time::Duration::ZERO), "首查即不在 → 立即确认");
    }
}
