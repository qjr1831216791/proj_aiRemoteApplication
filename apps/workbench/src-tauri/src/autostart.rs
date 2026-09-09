//! 自启托管（T11：AC8/9/10，plan §2 自启通道 / §5.2 任务契约）。
//!
//! 两个独立开关（spec 决策 4，ADR-0002）：
//! - **服务自启**（AC8/9）：开 = 复用 `setup-autostart.ps1` 幂等重建（=接管
//!   存量任务，不产生双通道）；关 = 同脚本 `-Remove`（运行中进程不受影响）。
//!   接管检测：执行前以 `Get-ScheduledTask` 查询三条任务存在性（经
//!   `CommandExecutor` seam，可 mock），任一存在即 tookOver=true。
//! - **程序自身**（AC10）：PowerShell `Register-ScheduledTask` 注册登录触发
//!   任务——任务名固定、参数 `--hidden`、`ExecutionTimeLimit Zero`（取消默认
//!   72h 强杀）、Action 指向 `current_exe()`；移除 = `Unregister-ScheduledTask`。
//!
//! 任务操作均为当前用户级（登录触发 -User $env:USERNAME），与
//! setup-autostart.ps1 同构——该脚本**不自提权**，注册自身任务无需 UAC；
//! 全部命令隐藏窗口（CREATE_NO_WINDOW）执行。

use crate::lang::Lang;
use crate::scripts::{
    interpret_exit, setup_autostart, CommandExecutor, CommandSpec, ExecOutcome, Script,
    CREATE_NO_WINDOW,
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// sprint0 服务自启任务名（与 setup-autostart.ps1 组件定义逐字对齐，AC8）
pub const SERVICE_TASK_NAMES: [&str; 3] = [
    "CloudCLI Sprint0 autostart",
    "Caddy Sprint0 autostart",
    "ddns-go Sprint0 autostart",
];

/// 程序自身登录任务名（AC10，plan §5.2）
pub const APP_TASK_NAME: &str = "AI Remote Workbench Autostart";
/// 程序自启任务参数（AC10/§4.2：静默入托盘，不弹主窗）
pub const APP_TASK_ARGUMENT: &str = "--hidden";

/// 任务存在性查询超时
pub const TASK_QUERY_TIMEOUT: Duration = Duration::from_secs(10);
/// 程序自身任务注册/移除超时（plan §5.2：30s）
pub const APP_TASK_TIMEOUT: Duration = Duration::from_secs(30);

// ── PowerShell 单引号字面量（路径内嵌防注入/防断串）────────────────────────

/// PS 单引号转义：`'` → `''`（单引号字符串内唯一转义）
pub fn ps_quote(s: &str) -> String {
    s.replace('\'', "''")
}

// ── 命令构造（纯函数；单测断言字符串）──────────────────────────────────────

/// PowerShell 内联命令骨架（-Command + CREATE_NO_WINDOW + 输出落日志）
fn ps_command_spec(script: &str, timeout: Duration, log_stem: &str, log_dir: &Path) -> CommandSpec {
    CommandSpec {
        program: "powershell.exe".into(),
        args: vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-Command".into(),
            script.into(),
        ],
        creation_flags: CREATE_NO_WINDOW,
        stdout_log: Some(log_dir.join(format!("{log_stem}.log"))),
        stderr_log: Some(log_dir.join(format!("{log_stem}.err.log"))),
        timeout,
        working_dir: None,
    }
}

/// 任务存在性探测命令：exit 0 = 存在，exit 1 = 不存在（经 executor 可 mock）
pub fn task_query_spec(task_name: &str, log_dir: &Path) -> CommandSpec {
    let script = format!(
        "if (Get-ScheduledTask -TaskName '{}' -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}",
        ps_quote(task_name)
    );
    ps_command_spec(&script, TASK_QUERY_TIMEOUT, "autostart-query", log_dir)
}

/// 程序自身任务注册命令（幂等重建=接管语义；登录触发 + --hidden +
/// ExecutionTimeLimit Zero + 自身 exe 路径）。
/// try/catch 显式 exit code：0 成功 / 1 失败（注册失败含任务名冲突等）。
pub fn app_task_register_spec(exe_path: &Path, log_dir: &Path) -> CommandSpec {
    let name = ps_quote(APP_TASK_NAME);
    let exe = ps_quote(&exe_path.to_string_lossy());
    let script = format!(
        "try {{ \
         $action = New-ScheduledTaskAction -Execute '{exe}' -Argument '{APP_TASK_ARGUMENT}'; \
         $trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME; \
         $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries \
         -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero); \
         if (Get-ScheduledTask -TaskName '{name}' -ErrorAction SilentlyContinue) {{ \
         Unregister-ScheduledTask -TaskName '{name}' -Confirm:$false \
         }}; \
         Register-ScheduledTask -TaskName '{name}' -Action $action -Trigger $trigger \
         -Settings $settings | Out-Null; exit 0 \
         }} catch {{ exit 1 }}"
    );
    ps_command_spec(&script, APP_TASK_TIMEOUT, "autostart-app-register", log_dir)
}

/// 程序自身任务移除命令（不存在时幂等成功）
pub fn app_task_remove_spec(log_dir: &Path) -> CommandSpec {
    let name = ps_quote(APP_TASK_NAME);
    let script = format!(
        "try {{ \
         if (Get-ScheduledTask -TaskName '{name}' -ErrorAction SilentlyContinue) {{ \
         Unregister-ScheduledTask -TaskName '{name}' -Confirm:$false \
         }}; exit 0 \
         }} catch {{ exit 1 }}"
    );
    ps_command_spec(&script, APP_TASK_TIMEOUT, "autostart-app-remove", log_dir)
}

// ── 核心操作（依赖全注入；单测 mock executor）──────────────────────────────

/// 查询单条计划任务是否存在（查询异常返回 Err，交上层决定放行与否）
pub fn task_exists(
    executor: &dyn CommandExecutor,
    task_name: &str,
    log_dir: &Path,
) -> Result<bool, String> {
    match executor.execute(&task_query_spec(task_name, log_dir)) {
        ExecOutcome::Exited(0) => Ok(true),
        ExecOutcome::Exited(1) => Ok(false),
        ExecOutcome::Exited(code) => Err(format!(
            "任务查询异常退出（code={code}，task={task_name}），详情见程序日志"
        )),
        ExecOutcome::TimedOut => Err(format!("任务查询超时（task={task_name}）")),
        ExecOutcome::SpawnFailed(e) => Err(format!("任务查询无法执行：{e}")),
    }
}

/// 服务自启开/关（AC8/9）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServicesAutostartOutcome {
    /// 开启时是否接管了已存在的 sprint0 任务（AC8：接管而非报错）
    pub took_over: bool,
}

pub fn set_services_autostart(
    executor: &dyn CommandExecutor,
    scripts_dir: &Path,
    lang: Lang,
    log_dir: &Path,
    enable: bool,
) -> Result<ServicesAutostartOutcome, String> {
    // 接管检测（AC8）：开启前查询三条任务存在性，任一存在 = 接管存量。
    // 单条查询失败不阻断（按不存在处理，脚本本身幂等重建兼容双通道）
    let took_over = if enable {
        let mut existed = false;
        for name in SERVICE_TASK_NAMES {
            match task_exists(executor, name, log_dir) {
                Ok(true) => existed = true,
                Ok(false) => {}
                Err(e) => log::warn!("接管检测跳过 {name}：{e}"),
            }
        }
        existed
    } else {
        false
    };
    // 开/关统一走 setup-autostart.ps1（幂等重建 / -Remove；AC9：移除不影响运行中进程）
    let spec = setup_autostart(scripts_dir, lang, log_dir, !enable);
    match executor.execute(&spec) {
        ExecOutcome::Exited(0) => Ok(ServicesAutostartOutcome { took_over }),
        outcome => Err(exec_error(Script::SetupAutostart, outcome)),
    }
}

/// 程序自身自启开/关（AC10）
pub fn set_app_autostart(
    executor: &dyn CommandExecutor,
    exe_path: &Path,
    log_dir: &Path,
    enable: bool,
) -> Result<(), String> {
    let spec = if enable {
        app_task_register_spec(exe_path, log_dir)
    } else {
        app_task_remove_spec(log_dir)
    };
    match executor.execute(&spec) {
        ExecOutcome::Exited(0) => Ok(()),
        ExecOutcome::Exited(code) => Err(format!(
            "程序自启任务{}失败（code={code}），详情见程序日志目录",
            if enable { "注册" } else { "移除" }
        )),
        ExecOutcome::TimedOut => Err("程序自启任务操作超时（30s）".into()),
        ExecOutcome::SpawnFailed(e) => Err(format!("程序自启任务无法执行：{e}")),
    }
}

/// 执行结果 → 错误文案（退出码语义经 interpret_exit 分派）
fn exec_error(script: Script, outcome: ExecOutcome) -> String {
    match outcome {
        ExecOutcome::Exited(0) => unreachable!("成功路径不经此处"),
        ExecOutcome::Exited(code) => match interpret_exit(script, code) {
            crate::scripts::ScriptOutcome::Unavailable(msg) => msg.to_string(),
            crate::scripts::ScriptOutcome::Failed(code) => {
                format!("脚本异常退出（code={code}），详情见程序日志目录")
            }
            crate::scripts::ScriptOutcome::Success | crate::scripts::ScriptOutcome::TimedOut => {
                String::new()
            }
        },
        ExecOutcome::TimedOut => "脚本执行超时（已终止），详情见程序日志目录".into(),
        ExecOutcome::SpawnFailed(e) => format!("脚本无法执行：{e}"),
    }
}

// ── Tauri 命令（装配层经 lib.rs 注入上下文）────────────────────────────────

/// 自启上下文（lib.rs setup 注入：脚本目录 + 禁用原因 + 日志目录）
pub struct AutostartContext {
    pub scripts_dir: Option<PathBuf>,
    /// 脚本目录不可用时的禁用原因（spec §4.5：按钮禁用 + 定位提示透传前端）
    pub scripts_disabled_reason: Option<String>,
    pub log_dir: PathBuf,
}

/// set_autostart_services 返回载荷（plan §5.1：`-> { tookOver: bool }`）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TookOverPayload {
    pub took_over: bool,
}

/// 服务自启开关（AC8/9）：脚本目录不可用时报错禁用（spec §4.5）。
/// 脚本执行最长 60s → spawn_blocking 执行，不阻塞主线程/UI（Tauri 同步命令
/// 默认跑主线程，长命令会冻结窗口与托盘）。
#[tauri::command]
pub async fn set_autostart_services(
    ctx: tauri::State<'_, AutostartContext>,
    settings: tauri::State<'_, crate::settings::SettingsState>,
    enable: bool,
) -> Result<TookOverPayload, String> {
    let Some(dir) = ctx.scripts_dir.clone() else {
        return Err("sprint0 脚本目录不可用（spec §4.5）：无法管理服务自启任务".into());
    };
    let lang = crate::lang::resolve_setting(settings.current().language);
    let log_dir = ctx.log_dir.clone();
    let out = tauri::async_runtime::spawn_blocking(move || {
        set_services_autostart(&crate::scripts::ProcessExecutor, &dir, lang, &log_dir, enable)
    })
    .await
    .map_err(|e| format!("自启任务执行异常结束：{e}"))??;
    Ok(TookOverPayload { took_over: out.took_over })
}

/// 程序自身自启开关（AC10）：Action 取 current_exe() 路径。
/// 同上：注册/移除最长 30s → 后台线程执行。
#[tauri::command]
pub async fn set_autostart_app(
    ctx: tauri::State<'_, AutostartContext>,
    enable: bool,
) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("无法定位自身可执行文件：{e}"))?;
    let log_dir = ctx.log_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        set_app_autostart(&crate::scripts::ProcessExecutor, &exe, &log_dir, enable)
    })
    .await
    .map_err(|e| format!("自启任务执行异常结束：{e}"))?
}

// ── 单元测试（宪法 §1：先红后绿）────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::test_support::MockExecutor;

    fn logs() -> PathBuf {
        PathBuf::from(r"D:\test-logs")
    }

    #[test]
    fn service_task_names_align_with_sprint0_script() {
        // setup-autostart.ps1 组件定义逐字对齐（AC8 接管前提：任务名不能漂移）
        assert_eq!(
            SERVICE_TASK_NAMES,
            [
                "CloudCLI Sprint0 autostart",
                "Caddy Sprint0 autostart",
                "ddns-go Sprint0 autostart"
            ]
        );
    }

    #[test]
    fn app_task_name_and_argument_follow_plan() {
        // plan §5.2：任务名固定 + 参数 --hidden
        assert_eq!(APP_TASK_NAME, "AI Remote Workbench Autostart");
        assert_eq!(APP_TASK_ARGUMENT, "--hidden");
    }

    #[test]
    fn ps_quote_escapes_single_quotes() {
        assert_eq!(ps_quote(r"D:\plain\path.exe"), r"D:\plain\path.exe");
        assert_eq!(ps_quote("C:'x"), "C:''x");
        assert_eq!(ps_quote("it''s"), "it''''s");
    }

    #[test]
    fn task_query_spec_builds_powershell_probe() {
        let spec = task_query_spec("Caddy Sprint0 autostart", &logs());
        assert_eq!(spec.program, "powershell.exe");
        let line = spec.command_line();
        assert!(line.contains("Get-ScheduledTask -TaskName 'Caddy Sprint0 autostart'"), "{line}");
        assert!(line.contains("-NonInteractive"), "{line}");
        assert_eq!(spec.creation_flags, CREATE_NO_WINDOW, "任务查询不得弹窗");
        assert_eq!(spec.timeout, TASK_QUERY_TIMEOUT);
        assert!(spec.stdout_log.is_some() && spec.stderr_log.is_some(), "查询输出应落日志");
    }

    #[test]
    fn task_exists_maps_exit_codes() {
        let dir = logs();
        let exists = MockExecutor::with_exits(&[ExecOutcome::Exited(0)]);
        assert_eq!(task_exists(&exists, "X", &dir), Ok(true));
        let missing = MockExecutor::with_exits(&[ExecOutcome::Exited(1)]);
        assert_eq!(task_exists(&missing, "X", &dir), Ok(false));
        let broken = MockExecutor::with_exits(&[ExecOutcome::Exited(2)]);
        assert!(task_exists(&broken, "X", &dir).is_err(), "非 0/1 退出码应报错");
        let hung = MockExecutor::with_exits(&[ExecOutcome::TimedOut]);
        assert!(task_exists(&hung, "X", &dir).is_err(), "超时应报错");
    }

    #[test]
    fn enable_services_runs_script_and_detects_takeover() {
        // AC8：任务已存在 → 执行幂等重建（接管）并返回 tookOver=true
        let exec = MockExecutor::with_exits(&[
            ExecOutcome::Exited(0), // 查询：CloudCLI 存在
            ExecOutcome::Exited(1), // 查询：Caddy 不存在
            ExecOutcome::Exited(0), // 查询：ddns-go 存在
            ExecOutcome::Exited(0), // setup-autostart.ps1 成功
        ]);
        let out = set_services_autostart(&exec, Path::new(r"D:\scripts"), Lang::Zh, &logs(), true)
            .expect("开启应成功");
        assert!(out.took_over, "任一任务存在即视为接管");

        let executed = exec.executed.lock().unwrap();
        assert_eq!(executed.len(), 4, "3 次查询 + 1 次脚本执行");
        for spec in &executed[..3] {
            assert!(spec.command_line().contains("Get-ScheduledTask"), "前 3 步应为存在性查询");
        }
        let setup = &executed[3];
        let line = setup.command_line();
        assert!(line.contains("setup-autostart.ps1"), "第 4 步应执行自启脚本：{line}");
        assert!(line.contains("-Lang zh"), "脚本语言须与生效语言对齐：{line}");
        assert!(line.contains("-StackDir"), "{line}");
        assert!(!line.contains("-Remove"), "开启分支不得带 -Remove：{line}");
    }

    #[test]
    fn enable_without_existing_tasks_reports_no_takeover() {
        // 全新环境：三条任务都不存在 → 首次注册，非接管
        let exec = MockExecutor::with_exits(&[
            ExecOutcome::Exited(1),
            ExecOutcome::Exited(1),
            ExecOutcome::Exited(1),
            ExecOutcome::Exited(0),
        ]);
        let out = set_services_autostart(&exec, Path::new(r"D:\scripts"), Lang::En, &logs(), true)
            .expect("开启应成功");
        assert!(!out.took_over);
    }

    #[test]
    fn disable_services_passes_remove_without_queries() {
        // AC9：关闭 = setup-autostart.ps1 -Remove；不做接管检测
        let exec = MockExecutor::with_exits(&[ExecOutcome::Exited(0)]);
        let out = set_services_autostart(&exec, Path::new(r"D:\scripts"), Lang::Zh, &logs(), false)
            .expect("关闭应成功");
        assert!(!out.took_over, "关闭分支 tookOver 不适用");
        let executed = exec.executed.lock().unwrap();
        assert_eq!(executed.len(), 1, "关闭只执行脚本，零查询");
        let line = executed[0].command_line();
        assert!(line.contains("-Remove"), "关闭分支必须带 -Remove：{line}");
        assert!(!line.contains("Get-ScheduledTask"), "关闭分支不做存在性查询：{line}");
    }

    #[test]
    fn services_setup_failure_maps_sprint0_exit1() {
        // setup-autostart.ps1 exit 1 = 栈目录缺 caddy.exe/ddns-go.exe（sprint0 契约）
        let exec = MockExecutor::with_exits(&[ExecOutcome::Exited(1)]);
        let err = set_services_autostart(&exec, Path::new(r"D:\scripts"), Lang::Zh, &logs(), false)
            .expect_err("exit 1 应报错");
        assert!(err.contains("栈目录缺少"), "应映射 sprint0 exit 1 语义：{err}");
    }

    #[test]
    fn app_register_spec_contract() {
        // AC10 / plan §5.2：--hidden、ExecutionTimeLimit Zero、登录触发、自身路径、幂等重建
        let exe = Path::new(r"D:\Software\workbench\AI-Remote-Workbench.exe");
        let spec = app_task_register_spec(exe, &logs());
        assert_eq!(spec.program, "powershell.exe");
        let line = spec.command_line();
        for needle in [
            format!("-Execute '{}'", exe.display()),
            format!("-Argument '{APP_TASK_ARGUMENT}'"),
            "New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME".to_string(),
            "-ExecutionTimeLimit ([TimeSpan]::Zero)".to_string(),
            "New-ScheduledTaskSettingsSet".to_string(),
            format!("Register-ScheduledTask -TaskName '{APP_TASK_NAME}'"),
            "Unregister-ScheduledTask".to_string(), // 幂等重建 = 先注销再注册（接管语义）
        ] {
            assert!(line.contains(&needle), "注册命令缺 {needle}：{line}");
        }
        assert_eq!(spec.creation_flags, CREATE_NO_WINDOW, "注册不得弹窗");
        assert_eq!(spec.timeout, APP_TASK_TIMEOUT, "plan §5.2：注册超时 30s");
    }

    #[test]
    fn app_register_spec_escapes_path_with_spaces() {
        // 路径含空格/单引号也须保持单引号字面量完整
        let exe = Path::new(r"C:\Program Files\My App\work'bench.exe");
        let line = app_task_register_spec(exe, &logs()).command_line();
        assert!(
            line.contains("-Execute 'C:\\Program Files\\My App\\work''bench.exe'"),
            "PS 单引号须转义：{line}"
        );
    }

    #[test]
    fn app_remove_spec_contract() {
        let spec = app_task_remove_spec(&logs());
        let line = spec.command_line();
        assert!(line.contains(format!("Unregister-ScheduledTask -TaskName '{APP_TASK_NAME}'").as_str()), "{line}");
        assert!(line.contains("SilentlyContinue"), "不存在时应幂等成功：{line}");
        assert_eq!(spec.creation_flags, CREATE_NO_WINDOW);
        assert_eq!(spec.timeout, APP_TASK_TIMEOUT);
    }

    #[test]
    fn set_app_autostart_executes_register_or_remove() {
        let dir = logs();
        // 开 → 执行注册命令
        let enable = MockExecutor::with_exits(&[ExecOutcome::Exited(0)]);
        set_app_autostart(&enable, Path::new(r"D:\wb\wb.exe"), &dir, true).expect("注册应成功");
        assert!(enable.executed.lock().unwrap()[0].command_line().contains("Register-ScheduledTask"));

        // 关 → 执行移除命令
        let disable = MockExecutor::with_exits(&[ExecOutcome::Exited(0)]);
        set_app_autostart(&disable, Path::new(r"D:\wb\wb.exe"), &dir, false).expect("移除应成功");
        assert!(disable.executed.lock().unwrap()[0].command_line().contains("Unregister-ScheduledTask"));

        // 失败映射
        let failing = MockExecutor::with_exits(&[ExecOutcome::Exited(1)]);
        assert!(set_app_autostart(&failing, Path::new(r"D:\wb\wb.exe"), &dir, true).is_err());
    }

    /// 真机演练（T11 手工验证项，`cargo test -- --ignored` 显式执行）：
    /// 1) 只读验证三条 Sprint0 任务存在性（不动其原状）；
    /// 2) 程序自身任务 注册 → 查询存在 → 移除 → 查询消失。
    #[test]
    #[ignore = "真机演练：操作本机计划任务（注册后即移除，终态无残留）"]
    fn real_machine_app_task_roundtrip() {
        let exec = crate::scripts::ProcessExecutor;
        let dir = std::env::temp_dir();

        // 只读：接管检测真机路径（本机 sprint0 已部署，三条任务应存在）
        for name in SERVICE_TASK_NAMES {
            assert!(
                task_exists(&exec, name, &dir).unwrap_or(false),
                "{name} 应存在（本机已部署；不存在则本机非部署态，跳过本断言环境）"
            );
        }

        // 自身任务：注册 → 确认 → 移除 → 确认消失
        let exe = std::env::current_exe().unwrap();
        set_app_autostart(&exec, &exe, &dir, true).expect("注册应成功");
        assert_eq!(task_exists(&exec, APP_TASK_NAME, &dir), Ok(true), "注册后任务应存在");
        set_app_autostart(&exec, &exe, &dir, false).expect("移除应成功");
        assert_eq!(task_exists(&exec, APP_TASK_NAME, &dir), Ok(false), "移除后任务应消失");
    }
}
