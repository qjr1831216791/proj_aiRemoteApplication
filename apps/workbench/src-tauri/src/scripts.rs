//! 脚本定位与命令构建（AC19/20 逻辑部分，plan §5.2 脚本契约）。
//!
//! - ScriptLocator：设置覆盖 → 内置 resources/bin → 开发态仓库 tools/sprint0/bin，
//!   以哨兵脚本（run-server-hidden.ps1）存在为准；全不可用 → 禁用原因（spec §4.5）
//! - CommandBuilder：隐藏脚本 = powershell -NoProfile -NonInteractive -File …
//!   -Lang zh|en + CREATE_NO_WINDOW + stdio→日志 + 每脚本超时（ADR-0001 约定）；
//!   Caddy/ddns-go 原生命令（不经 powershell，参数与 setup-autostart.ps1 一致）
//! - Elevator：ShellExecuteW runas 提权可见窗口（AC19：结尾手工步骤可读 → -NoExit）
//! - 退出码 → UI 语义映射（AC20：UAC 拒绝/脚本失败给明确提示，不崩溃）

use crate::consts::{
    CADDYFILE_PATH, CLOUDCLI_PORT, DDNSGO_CONFIG_PATH, DDNSGO_INTERVAL_SECS,
    DDNSGO_LISTEN, STACK_DIR,
};
use crate::lang::Lang;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 隐藏子进程创建标志（ADR-0001：CREATE_NO_WINDOW，而非 -WindowStyle Hidden）
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ── 脚本清单与窗口/超时契约 ────────────────────────────────────────────────

/// sprint0 脚本（plan §5.2）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    /// CloudCLI 拉起（端口已 Listen 时直接 exit 0）
    RunServerHidden,
    /// CloudCLI 停止
    StopServer,
    /// 服务自启任务开/关（含 -Remove）
    SetupAutostart,
    /// CloudCLI 安装/重装（交互式，需管理员）
    InstallServer,
    /// HTTPS 栈装机（需管理员，结尾有手工步骤提示）
    InstallHttps,
    /// HTTPS 环境配置（需管理员）
    EnableHttps,
    /// 客户端配置（交互式，无需管理员）
    InstallClient,
}

/// 窗口形态（spec §4.3）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// 隐藏（CREATE_NO_WINDOW + stdio→日志）
    Hidden,
    /// 提权可见（UAC runas + -NoExit）
    Elevated,
    /// 普通可见交互窗（无 UAC）
    VisibleInteractive,
}

impl Script {
    pub fn file_name(&self) -> &'static str {
        match self {
            Script::RunServerHidden => "run-server-hidden.ps1",
            Script::StopServer => "stop-server.ps1",
            Script::SetupAutostart => "setup-autostart.ps1",
            Script::InstallServer => "install-server.ps1",
            Script::InstallHttps => "install-https.ps1",
            Script::EnableHttps => "enable-https.ps1",
            Script::InstallClient => "install-client.ps1",
        }
    }

    pub fn visibility(&self) -> Visibility {
        match self {
            Script::RunServerHidden | Script::StopServer | Script::SetupAutostart => {
                Visibility::Hidden
            }
            Script::InstallServer | Script::InstallHttps | Script::EnableHttps => {
                Visibility::Elevated
            }
            Script::InstallClient => Visibility::VisibleInteractive,
        }
    }

    /// 每脚本超时（plan §5.2；隐藏脚本由执行器执行，超时即杀并上报）
    pub fn timeout(&self) -> Duration {
        match self {
            Script::RunServerHidden => Duration::from_secs(15),
            Script::StopServer => Duration::from_secs(30),
            Script::SetupAutostart => Duration::from_secs(60),
            Script::InstallServer => Duration::from_secs(900),
            Script::InstallHttps => Duration::from_secs(1800),
            Script::EnableHttps => Duration::from_secs(120),
            Script::InstallClient => Duration::from_secs(600),
        }
    }
}

// ── 脚本目录定位 ───────────────────────────────────────────────────────────

/// 目录来源（优先级自上而下）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptSource {
    /// 用户设置覆盖（settings.scriptsDirOverride）
    Override,
    /// 安装包内置副本（exe 同级 resources/bin，T17 打包契约）
    Bundled,
    /// 开发态仓库路径（debug 构建专属）
    DevRepo,
}

impl ScriptSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScriptSource::Override => "override",
            ScriptSource::Bundled => "bundled",
            ScriptSource::DevRepo => "dev-repo",
        }
    }
}

/// 目录有效性哨兵：三组件编排的最小依赖（spec §4.5）
pub const SENTINEL_SCRIPT_FILE: &str = "run-server-hidden.ps1";

/// 解析结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptsResolution {
    /// 可用：目录 + 来源
    Found { dir: PathBuf, source: ScriptSource },
    /// 全部候选不可用：附原因（功能禁用，spec §4.5）
    Disabled { reason: String },
}

/// 候选目录依次校验（存在 + 含哨兵脚本），第一个可用者胜出
pub fn resolve_scripts_dir(candidates: &[(ScriptSource, PathBuf)]) -> ScriptsResolution {
    for (source, dir) in candidates {
        if dir.join(SENTINEL_SCRIPT_FILE).is_file() {
            return ScriptsResolution::Found { dir: dir.clone(), source: *source };
        }
    }
    // 全不可用：枚举候选与哨兵判据，交 UI 展示禁用原因（spec §4.5）
    let tried = candidates
        .iter()
        .map(|(s, d)| format!("{}({})", s.as_str(), d.display()))
        .collect::<Vec<_>>()
        .join("、");
    ScriptsResolution::Disabled {
        reason: format!(
            "脚本目录全部不可用（已尝试 {tried}）：缺少 {SENTINEL_SCRIPT_FILE}，启动/停止/自启功能禁用（spec §4.5）"
        ),
    }
}

/// 组装真实候选（设置覆盖 → 内置副本 → 开发态仓库）
pub fn real_candidates(override_dir: Option<&str>, exe_dir: Option<&Path>) -> Vec<(ScriptSource, PathBuf)> {
    let mut list: Vec<(ScriptSource, PathBuf)> = Vec::new();
    if let Some(dir) = override_dir.map(str::trim).filter(|s| !s.is_empty()) {
        list.push((ScriptSource::Override, PathBuf::from(dir)));
    }
    if let Some(base) = exe_dir {
        // T17 打包契约：安装版与便携版目录同构（exe + resources\bin）
        list.push((ScriptSource::Bundled, base.join("resources").join("bin")));
    }
    if cfg!(debug_assertions) {
        // 开发态：CARGO_MANIFEST_DIR = apps/workbench/src-tauri → 仓库根 tools/sprint0/bin
        list.push((
            ScriptSource::DevRepo,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tools/sprint0/bin"),
        ));
    }
    list
}

/// 真实定位入口（设置覆盖来自 SettingsState）
pub fn locate(override_dir: Option<&str>, exe_dir: Option<&Path>) -> ScriptsResolution {
    resolve_scripts_dir(&real_candidates(override_dir, exe_dir))
}

// ── 命令规格与构建器 ───────────────────────────────────────────────────────

/// 命令规格（数据态：可断言、可 mock、可入日志）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    /// Windows 进程创建标志（CREATE_NO_WINDOW 等）
    pub creation_flags: u32,
    /// stdout 重定向日志（None = 不重定向）
    pub stdout_log: Option<PathBuf>,
    /// stderr 重定向日志
    pub stderr_log: Option<PathBuf>,
    /// 执行超时（隐藏脚本；超时杀进程上报 TimedOut）
    pub timeout: Duration,
    /// 工作目录（None = 继承当前）
    pub working_dir: Option<PathBuf>,
}

impl CommandSpec {
    /// 日志/断言友好：完整命令行
    pub fn command_line(&self) -> String {
        format!("{} {}", self.program, self.args.join(" "))
    }
}

/// Lang → 脚本 -Lang 实参（auto 已在程序侧解析为具体语言，不透传）
pub fn lang_arg(lang: Lang) -> &'static str {
    match lang {
        Lang::Zh => "zh",
        Lang::En => "en",
    }
}

/// 隐藏脚本命令骨架（ADR-0001：-NoProfile -NonInteractive + CREATE_NO_WINDOW + stdio→日志）
pub fn hidden_script_spec(
    dir: &Path,
    script: Script,
    lang: Lang,
    log_dir: &Path,
    extra_args: &[&str],
) -> CommandSpec {
    let stem = script.file_name().trim_end_matches(".ps1");
    let mut args: Vec<String> = vec![
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-File".into(),
        dir.join(script.file_name()).to_string_lossy().into_owned(),
        "-Lang".into(),
        lang_arg(lang).into(),
    ];
    args.extend(extra_args.iter().map(|s| (*s).to_string()));
    CommandSpec {
        program: "powershell.exe".into(),
        args,
        creation_flags: CREATE_NO_WINDOW,
        stdout_log: Some(log_dir.join(format!("{stem}.log"))),
        stderr_log: Some(log_dir.join(format!("{stem}.err.log"))),
        timeout: script.timeout(),
        working_dir: Some(dir.to_path_buf()),
    }
}

/// run-server-hidden.ps1：CloudCLI 拉起（-Port 显式化）
pub fn run_server_hidden(dir: &Path, lang: Lang, log_dir: &Path) -> CommandSpec {
    let mut spec = hidden_script_spec(dir, Script::RunServerHidden, lang, log_dir, &[]);
    spec.args.push("-Port".into());
    spec.args.push(CLOUDCLI_PORT.to_string());
    spec
}

/// stop-server.ps1：CloudCLI 停止
pub fn stop_server(dir: &Path, lang: Lang, log_dir: &Path) -> CommandSpec {
    let mut spec = hidden_script_spec(dir, Script::StopServer, lang, log_dir, &[]);
    spec.args.push("-Port".into());
    spec.args.push(CLOUDCLI_PORT.to_string());
    spec
}

/// setup-autostart.ps1：服务自启任务开（false）/ 关（true，-Remove）
pub fn setup_autostart(dir: &Path, lang: Lang, log_dir: &Path, remove: bool) -> CommandSpec {
    let mut spec = hidden_script_spec(dir, Script::SetupAutostart, lang, log_dir, &[]);
    spec.args.push("-StackDir".into());
    spec.args.push(STACK_DIR.into());
    if remove {
        spec.args.push("-Remove".into());
    }
    spec
}

/// Caddy 原生拉起（plan §5.2：不经 powershell；参数与 setup-autostart.ps1 任务一致）
pub fn caddy_run(log_dir: &Path) -> CommandSpec {
    CommandSpec {
        program: format!(r"{STACK_DIR}\caddy.exe"),
        args: vec!["run".into(), "--config".into(), CADDYFILE_PATH.into()],
        creation_flags: CREATE_NO_WINDOW,
        stdout_log: Some(log_dir.join("caddy.log")),
        stderr_log: Some(log_dir.join("caddy.err.log")),
        // 派发后由编排层轮询端口就绪（T8），此处超时仅为执行器兜底
        timeout: Duration::from_secs(15),
        working_dir: Some(PathBuf::from(STACK_DIR)),
    }
}

/// ddns-go 原生拉起（-c 配置 -l 监听 -f 间隔；与 setup-autostart.ps1 任务一致）
pub fn ddns_go_run(log_dir: &Path) -> CommandSpec {
    CommandSpec {
        program: format!(r"{STACK_DIR}\ddns-go.exe"),
        args: vec![
            "-c".into(),
            DDNSGO_CONFIG_PATH.into(),
            "-l".into(),
            DDNSGO_LISTEN.into(),
            "-f".into(),
            DDNSGO_INTERVAL_SECS.to_string(),
        ],
        creation_flags: CREATE_NO_WINDOW,
        stdout_log: Some(log_dir.join("ddns-go.log")),
        stderr_log: Some(log_dir.join("ddns-go.err.log")),
        timeout: Duration::from_secs(15),
        working_dir: Some(PathBuf::from(STACK_DIR)),
    }
}

// ── 提权/可见窗口启动（AC19/20）────────────────────────────────────────────

/// 参数串加引号（含空格才加，保持命令行简洁）
fn quote(s: &str) -> String {
    if s.contains(' ') {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

/// 可见窗口脚本参数串（ShellExecuteW lpParameters）。
/// -NoExit：窗口在脚本结束后保留（AC19：install-https 结尾手工步骤完整可读）；
/// 交互式脚本（Read-Host）不加 -NonInteractive。
pub fn visible_script_params(
    dir: &Path,
    script: Script,
    lang: Lang,
    extra_args: &[&str],
) -> String {
    let path = quote(&dir.join(script.file_name()).to_string_lossy());
    let mut parts: Vec<String> = vec![
        "-NoProfile".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-File".into(),
        path,
        "-Lang".into(),
        lang_arg(lang).into(),
        "-NoExit".into(),
    ];
    parts.extend(extra_args.iter().map(|s| (*s).to_string()));
    parts.join(" ")
}

/// UAC 提权启动（ShellExecuteW verb=runas；用户拒绝/失败 → Err 交上层提示，AC20）
pub fn elevate(program: &str, params: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(std::iter::once(0)).collect()
        }
        let verb = wide("runas");
        let file = wide(program);
        let parameters = wide(params);
        let code = unsafe {
            ShellExecuteW(
                None,
                PCWSTR::from_raw(verb.as_ptr()),
                PCWSTR::from_raw(file.as_ptr()),
                PCWSTR::from_raw(parameters.as_ptr()),
                None,
                SW_SHOWNORMAL,
            )
            .0 as isize
        };
        // 惯例：>32 成功；5 = SE_ERR_ACCESSDENIED（UAC 被拒）
        if code > 32 {
            Ok(())
        } else {
            Err(format!(
                "ShellExecuteW 返回 {code}（5=UAC 被拒；AC20：提示用户且不崩溃）"
            ))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (program, params);
        Err("提权启动仅支持 Windows".into())
    }
}

// ── 退出码 → UI 语义（AC20）────────────────────────────────────────────────

/// 脚本/命令执行结果语义（前端提示与状态机共用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptOutcome {
    /// exit 0（含"已在运行，无需重复启动"等幂等成功）
    Success,
    /// 前置条件不满足：cloudcli 不可用 / 端口仍被占 / 栈目录缺文件（各脚本 exit 1）
    Unavailable(&'static str),
    /// 其他非零退出码
    Failed(i32),
    /// 执行超时（进程已被杀）
    TimedOut,
}

/// 退出码 → 语义（与 tools/sprint0 实际退出码对齐）
pub fn interpret_exit(script: Script, code: i32) -> ScriptOutcome {
    match code {
        0 => ScriptOutcome::Success,
        // 各脚本对 exit 1 有明确的前置语义（退出码契约见脚本本体）
        1 => match script {
            // run-server-hidden.ps1：Get-Command cloudcli 失败（未装/不在 PATH）
            Script::RunServerHidden => {
                ScriptOutcome::Unavailable("cloudcli 不可用（未安装或不在 PATH）")
            }
            // stop-server.ps1：taskkill 后端口仍有监听
            Script::StopServer => ScriptOutcome::Unavailable("端口仍被占用（taskkill 后仍有监听）"),
            // setup-autostart.ps1：栈目录缺少 caddy.exe / ddns-go.exe
            Script::SetupAutostart => ScriptOutcome::Unavailable("栈目录缺少 caddy.exe / ddns-go.exe"),
            // 安装/配置类脚本 1 无统一前置语义 → 一般失败（UI 引导看日志）
            _ => ScriptOutcome::Failed(1),
        },
        other => ScriptOutcome::Failed(other),
    }
}

// ── 执行器（trait 注入便于 mock；真实实现 std::process）──────────────────────

/// 执行结果（原始态；语义化经 interpret_exit）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecOutcome {
    Exited(i32),
    TimedOut,
    SpawnFailed(String),
}

/// 执行器接口：T8 编排依赖此 seam 注入 mock。
/// Send + Sync：编排器跨线程持有 Arc<dyn CommandExecutor>
pub trait CommandExecutor: Send + Sync {
    /// 同步执行到退出或超时（stdio 按 spec 落日志）
    fn execute(&self, spec: &CommandSpec) -> ExecOutcome;
    /// 派发不等待（常驻服务进程：caddy run / ddns-go / run-server-hidden 的
    /// Start-Process 语义）
    fn dispatch(&self, spec: &CommandSpec) -> Result<(), String>;
}

/// 真实执行器（std::process；Windows 侧应用 creation_flags 与 stdio 重定向）
pub struct ProcessExecutor;

impl CommandExecutor for ProcessExecutor {
    fn execute(&self, spec: &CommandSpec) -> ExecOutcome {
        let mut child = match std_command(spec, log_stdio(spec.stdout_log.as_deref()), log_stdio(spec.stderr_log.as_deref())).spawn() {
            Ok(c) => c,
            Err(e) => return ExecOutcome::SpawnFailed(e.to_string()),
        };
        // 轮询等待：50ms 粒度，超过 spec.timeout 即杀（T8 编排层转 failed 语义）
        let start = std::time::Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return ExecOutcome::Exited(status.code().unwrap_or(-1));
                }
                Ok(None) => {
                    if start.elapsed() >= spec.timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        log::warn!(
                            "命令超时被杀（{}s）：{}",
                            spec.timeout.as_secs(),
                            spec.command_line()
                        );
                        return ExecOutcome::TimedOut;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return ExecOutcome::SpawnFailed(e.to_string()),
            }
        }
    }

    fn dispatch(&self, spec: &CommandSpec) -> Result<(), String> {
        // 派发即返：std Child 句柄直接丢弃不会终止子进程（Windows 无 kill-on-drop），
        // 常驻服务进程（caddy run / ddns-go）因此存活；日志句柄随子进程生命周期持有
        std_command(spec, log_stdio(spec.stdout_log.as_deref()), log_stdio(spec.stderr_log.as_deref()))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// 日志重定向句柄：建父目录 + 截断创建；失败退化为丢弃（不阻断执行）
fn log_stdio(path: Option<&Path>) -> std::process::Stdio {
    let file = path.and_then(|p| {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::File::create(p).ok()
    });
    match file {
        Some(f) => f.into(),
        None => std::process::Stdio::null(),
    }
}

/// 由规格构造 std::process::Command（stdio 句柄由调用方准备后传入）
fn std_command(
    spec: &CommandSpec,
    stdout: std::process::Stdio,
    stderr: std::process::Stdio,
) -> std::process::Command {
    let mut cmd = std::process::Command::new(&spec.program);
    cmd.args(&spec.args);
    cmd.stdout(stdout);
    cmd.stderr(stderr);
    if let Some(dir) = &spec.working_dir {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(spec.creation_flags);
    }
    cmd
}

// ── 单元测试 ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // ── 定位器 ─────────────────────────────────────────────────────────

    /// 造一个含哨兵脚本的目录
    fn script_dir(tag: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("wb-scripts-test-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(SENTINEL_SCRIPT_FILE), "# stub").unwrap();
        dir
    }

    #[test]
    fn locator_prefers_override_then_bundled() {
        let ov = script_dir("override");
        let bundled = script_dir("bundled");
        let res = resolve_scripts_dir(&[
            (ScriptSource::Override, ov.clone()),
            (ScriptSource::Bundled, bundled.clone()),
        ]);
        assert_eq!(res, ScriptsResolution::Found { dir: ov, source: ScriptSource::Override });

        // 覆盖目录无效（缺哨兵）→ 按优先级回落内置
        let bad_override = std::env::temp_dir().join("wb-scripts-bad-override");
        let _ = std::fs::remove_dir_all(&bad_override);
        std::fs::create_dir_all(&bad_override).unwrap();
        let res2 = resolve_scripts_dir(&[
            (ScriptSource::Override, bad_override),
            (ScriptSource::Bundled, bundled.clone()),
        ]);
        assert_eq!(res2, ScriptsResolution::Found { dir: bundled, source: ScriptSource::Bundled });
    }

    #[test]
    fn locator_missing_sentinel_directory_is_invalid() {
        // 目录存在但缺哨兵脚本 → 不可用（半拷贝/错目录不放行）
        let empty = std::env::temp_dir().join(format!("wb-scripts-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).unwrap();
        let res = resolve_scripts_dir(&[(ScriptSource::Bundled, empty.clone())]);
        match res {
            ScriptsResolution::Disabled { reason } => {
                assert!(reason.contains("run-server-hidden.ps1"), "原因应指向缺失哨兵：{reason}");
            }
            other => panic!("应为 Disabled，实际 {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn locator_all_invalid_reports_every_candidate() {
        let res = resolve_scripts_dir(&[
            (ScriptSource::Override, PathBuf::from(r"D:\no\such\override")),
            (ScriptSource::Bundled, PathBuf::from(r"D:\no\such\bundled")),
        ]);
        match res {
            ScriptsResolution::Disabled { reason } => {
                assert!(reason.contains("override") && reason.contains("bundled"), "原因应枚举候选：{reason}");
            }
            other => panic!("应为 Disabled，实际 {other:?}"),
        }
    }

    #[test]
    fn locator_dev_fallback_finds_repo_tools() {
        // debug 构建：exe 目录给不存在路径 → 开发态候选命中仓库 tools/sprint0/bin
        let res = locate(None, Some(Path::new(r"D:\no\such\exe-dir")));
        match res {
            ScriptsResolution::Found { dir, source } => {
                assert_eq!(source, ScriptSource::DevRepo);
                assert!(
                    dir.to_string_lossy().replace('\\', "/").ends_with("tools/sprint0/bin"),
                    "开发态候选应指向仓库脚本目录：{}",
                    dir.display()
                );
                assert!(dir.join(SENTINEL_SCRIPT_FILE).is_file(), "仓库哨兵脚本应存在");
            }
            other => panic!("debug 构建应回落开发态目录，实际 {other:?}"),
        }
    }

    // ── 脚本契约表 ─────────────────────────────────────────────────────

    #[test]
    fn script_table_visibility_and_timeouts() {
        assert_eq!(Script::RunServerHidden.file_name(), "run-server-hidden.ps1");
        assert_eq!(Script::RunServerHidden.visibility(), Visibility::Hidden);
        assert_eq!(Script::RunServerHidden.timeout(), Duration::from_secs(15));
        assert_eq!(Script::SetupAutostart.timeout(), Duration::from_secs(60));
        assert_eq!(Script::InstallHttps.visibility(), Visibility::Elevated);
        assert_eq!(Script::InstallClient.visibility(), Visibility::VisibleInteractive);
        assert_eq!(Script::StopServer.visibility(), Visibility::Hidden);
        assert_eq!(Script::EnableHttps.visibility(), Visibility::Elevated);
    }

    #[test]
    fn lang_arg_alignment() {
        assert_eq!(lang_arg(Lang::Zh), "zh");
        assert_eq!(lang_arg(Lang::En), "en");
    }

    // ── 隐藏脚本构建器 ─────────────────────────────────────────────────

    #[test]
    fn run_server_hidden_spec_shape() {
        let dir = script_dir("build");
        let logs = dir.join("logs");
        let spec = run_server_hidden(&dir, Lang::Zh, &logs);
        // ADR-0001：powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File …
        assert_eq!(spec.program, "powershell.exe");
        let script_path = dir.join("run-server-hidden.ps1").to_string_lossy().into_owned();
        assert_eq!(
            spec.args,
            vec![
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-ExecutionPolicy".to_string(),
                "Bypass".to_string(),
                "-File".to_string(),
                script_path,
                "-Lang".to_string(),
                "zh".to_string(),
                "-Port".to_string(),
                "3001".to_string(),
            ]
        );
        // CREATE_NO_WINDOW + stdio→日志 + 每脚本超时
        assert_eq!(spec.creation_flags, CREATE_NO_WINDOW);
        assert_eq!(spec.timeout, Duration::from_secs(15));
        let (out_log, err_log) = (&spec.stdout_log, &spec.stderr_log);
        assert!(out_log.as_ref().unwrap().starts_with(&logs), "{out_log:?}");
        assert!(out_log.as_ref().unwrap().to_string_lossy().contains("run-server-hidden"), "{out_log:?}");
        assert!(err_log.is_some(), "stderr 应同样落日志");
        // 命令行整串（日志/断言友好形态）
        assert!(spec.command_line().contains("-Lang zh -Port 3001"), "{}", spec.command_line());
    }

    #[test]
    fn stop_server_and_setup_autostart_specs() {
        let dir = script_dir("build2");
        let logs = dir.join("logs");
        let stop = stop_server(&dir, Lang::En, &logs);
        assert!(stop.args.contains(&"-Port".to_string()) && stop.args.contains(&"3001".to_string()));
        assert!(stop.command_line().contains("-Lang en"), "{}", stop.command_line());
        assert_eq!(stop.timeout, Duration::from_secs(30));

        let enable = setup_autostart(&dir, Lang::Zh, &logs, false);
        assert!(enable.command_line().contains("-StackDir D:\\Software\\cloudcli-https"), "{}", enable.command_line());
        assert!(!enable.command_line().contains("-Remove"), "开启分支不应带 -Remove");
        assert_eq!(enable.timeout, Duration::from_secs(60));

        let disable = setup_autostart(&dir, Lang::Zh, &logs, true);
        assert!(disable.command_line().contains("-Remove"), "关闭分支应带 -Remove");
    }

    #[test]
    fn native_caddy_spec_matches_autostart_contract() {
        // setup-autostart.ps1：caddy.exe run --config '<StackDir>\Caddyfile'
        let logs = PathBuf::from(r"D:\any\logs");
        let spec = caddy_run(&logs);
        assert_eq!(spec.program, r"D:\Software\cloudcli-https\caddy.exe");
        assert_eq!(spec.args, vec!["run".to_string(), "--config".to_string(), CADDYFILE_PATH.to_string()]);
        assert_eq!(spec.creation_flags, CREATE_NO_WINDOW, "服务进程也不许弹窗");
        assert_eq!(spec.stdout_log.as_deref(), Some(logs.join("caddy.log").as_path()), "{:?}", spec.stdout_log);
        assert_eq!(spec.working_dir.as_deref(), Some(Path::new(STACK_DIR)));
    }

    #[test]
    fn native_ddnsgo_spec_matches_autostart_contract() {
        // setup-autostart.ps1：ddns-go.exe -c '<StackDir>\ddns-go.yaml' -l :9876 -f 300
        let logs = PathBuf::from(r"D:\any\logs");
        let spec = ddns_go_run(&logs);
        assert_eq!(spec.program, r"D:\Software\cloudcli-https\ddns-go.exe");
        assert_eq!(
            spec.args,
            vec![
                "-c".to_string(),
                DDNSGO_CONFIG_PATH.to_string(),
                "-l".to_string(),
                DDNSGO_LISTEN.to_string(),
                "-f".to_string(),
                DDNSGO_INTERVAL_SECS.to_string(),
            ]
        );
        assert_eq!(spec.creation_flags, CREATE_NO_WINDOW);
    }

    // ── 可见/提权参数串 ────────────────────────────────────────────────

    #[test]
    fn visible_params_keep_window_open_and_interactive() {
        let dir = script_dir("elev");
        let p = visible_script_params(&dir, Script::InstallHttps, Lang::Zh, &["-Update"]);
        assert!(p.contains("-NoExit"), "AC19：结尾手工步骤须可读 → -NoExit：{p}");
        assert!(!p.contains("-NonInteractive"), "交互式脚本禁用 -NonInteractive：{p}");
        assert!(p.contains(&format!("-File \"{}\"", dir.join("install-https.ps1").to_string_lossy())) || p.contains(&format!("-File {}", dir.join("install-https.ps1").to_string_lossy())), "{p}");
        assert!(p.contains("-Lang zh"));
        assert!(p.contains("-Update"), "额外参数应透传：{p}");
    }

    #[test]
    fn quote_only_when_spaces() {
        assert_eq!(quote(r"D:\plain.ps1"), r"D:\plain.ps1");
        assert_eq!(quote(r"D:\with space\a.ps1"), r#""D:\with space\a.ps1""#);
    }

    // ── 退出码语义 ─────────────────────────────────────────────────────

    #[test]
    fn interpret_exit_maps_sprint0_codes() {
        use ScriptOutcome::*;
        // run-server-hidden：0=成功（含端口已在监听的幂等成功）/1=cloudcli 不可用
        assert_eq!(interpret_exit(Script::RunServerHidden, 0), Success);
        assert_eq!(interpret_exit(Script::RunServerHidden, 1), Unavailable("cloudcli 不可用（未安装或不在 PATH）"));
        assert_eq!(interpret_exit(Script::RunServerHidden, 2), Failed(2));
        // stop-server：1=杀完端口仍被占
        assert_eq!(interpret_exit(Script::StopServer, 1), Unavailable("端口仍被占用（taskkill 后仍有监听）"));
        // setup-autostart：1=栈目录缺 caddy.exe/ddns-go.exe
        assert_eq!(interpret_exit(Script::SetupAutostart, 1), Unavailable("栈目录缺少 caddy.exe / ddns-go.exe"));
        // 其他脚本 1 归一般失败
        assert_eq!(interpret_exit(Script::InstallHttps, 1), Failed(1));
    }

    // ── 执行器 ────────────────────────────────────────────────────────

    /// 记录规格的 mock（T8 编排单测同款 seam）
    struct MockExecutor {
        recorded: Mutex<Vec<CommandSpec>>,
    }

    impl CommandExecutor for MockExecutor {
        fn execute(&self, spec: &CommandSpec) -> ExecOutcome {
            self.recorded.lock().unwrap().push(spec.clone());
            ExecOutcome::Exited(0)
        }
        fn dispatch(&self, spec: &CommandSpec) -> Result<(), String> {
            self.recorded.lock().unwrap().push(spec.clone());
            Ok(())
        }
    }

    #[test]
    fn mock_executor_records_built_specs() {
        // 构建器输出 → 执行器入参 的贯通断言（命令行字符串级）
        let mock = MockExecutor { recorded: Mutex::new(vec![]) };
        let dir = script_dir("mock");
        let logs = dir.join("logs");
        assert_eq!(mock.execute(&run_server_hidden(&dir, Lang::En, &logs)), ExecOutcome::Exited(0));
        assert!(mock.dispatch(&caddy_run(&logs)).is_ok());
        let recorded = mock.recorded.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert!(recorded[0].command_line().contains("-Lang en"));
        assert_eq!(recorded[1].program, r"D:\Software\cloudcli-https\caddy.exe");
        assert_eq!(recorded[1].creation_flags, CREATE_NO_WINDOW);
    }

    /// 真实执行器冒烟：cmd /c echo（stdio 落日志 + 正常退出码）
    #[test]
    #[cfg(windows)]
    fn process_executor_collects_exit_and_log() {
        let dir = script_dir("exec");
        let out_log = dir.join("echo.log");
        let spec = CommandSpec {
            program: "cmd.exe".into(),
            args: vec!["/c".into(), "echo wb-exec-smoke".into()],
            creation_flags: CREATE_NO_WINDOW,
            stdout_log: Some(out_log.clone()),
            stderr_log: Some(dir.join("echo.err.log")),
            timeout: Duration::from_secs(10),
            working_dir: None,
        };
        assert_eq!(ProcessExecutor.execute(&spec), ExecOutcome::Exited(0));
        let logged = std::fs::read_to_string(&out_log).expect("stdout 应落日志");
        assert!(logged.contains("wb-exec-smoke"), "日志内容：{logged}");
    }

    /// 真实执行器超时：长命令被杀并报 TimedOut
    #[test]
    #[cfg(windows)]
    fn process_executor_times_out_and_kills() {
        let dir = script_dir("timeout");
        let spec = CommandSpec {
            program: "cmd.exe".into(),
            args: vec!["/c".into(), "ping -n 30 127.0.0.1 > nul".into()],
            creation_flags: CREATE_NO_WINDOW,
            stdout_log: Some(dir.join("t.log")),
            stderr_log: None,
            timeout: Duration::from_millis(400),
            working_dir: None,
        };
        assert_eq!(ProcessExecutor.execute(&spec), ExecOutcome::TimedOut);
    }

    /// 派发不等待：子命令要跑 ~9s，dispatch 须在 5s 内返回（立即 Ok）
    #[test]
    #[cfg(windows)]
    fn process_executor_dispatch_returns_immediately() {
        let dir = script_dir("dispatch");
        let spec = CommandSpec {
            program: "cmd.exe".into(),
            args: vec!["/c".into(), "ping -n 10 127.0.0.1 > nul".into()],
            creation_flags: CREATE_NO_WINDOW,
            stdout_log: Some(dir.join("d-out.log")),
            stderr_log: None,
            timeout: Duration::from_secs(5),
            working_dir: None,
        };
        let start = std::time::Instant::now();
        assert!(ProcessExecutor.dispatch(&spec).is_ok());
        assert!(start.elapsed() < Duration::from_secs(5), "dispatch 不应等待子进程完成");
    }
}
