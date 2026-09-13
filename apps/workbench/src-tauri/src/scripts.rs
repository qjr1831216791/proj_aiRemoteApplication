//! 脚本定位与命令构建（AC19/20 逻辑部分，plan §5.2 脚本契约）。
//!
//! - ScriptLocator：设置覆盖 → 内置 resources/bin → 开发态仓库 tools/sprint0/bin，
//!   以哨兵脚本（run-server-hidden.ps1）存在为准；全不可用 → 禁用原因（spec §4.5）
//! - CommandBuilder：隐藏脚本 = powershell -NoProfile -NonInteractive -File …
//!   -Lang zh|en + CREATE_NO_WINDOW + stdio→日志 + 每脚本超时（ADR-0001 约定）；
//!   Caddy 原生命令（不经 powershell，参数与 setup-autostart.ps1 一致）；
//!   ddns-go/frp 相关脚本随直连/穿透通道退役（spec 008）
//! - Elevator：ShellExecuteW runas 提权可见窗口（AC19：结尾手工步骤可读 → -NoExit）
//! - 退出码 → UI 语义映射（AC20：UAC 拒绝/脚本失败给明确提示，不崩溃）

use crate::consts::{CADDYFILE_PATH, CLOUDCLI_PORT};
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
    /// 服务自启任务开/关（含 -Remove）
    SetupAutostart,
    /// CloudCLI 安装/重装（交互式，需管理员）
    InstallServer,
    /// HTTPS 栈装机（需管理员，结尾有手工步骤提示）
    InstallHttps,
    /// HTTPS 环境配置（需管理员）
    EnableHttps,
    /// 腾讯云 CAM 密钥写入 .env（交互式，无需管理员；spec 006——
    /// spec 008 后为栈 .env 凭证唯一写入通道）
    SetTencentKey,
    /// EasyTier 组网服务管理（install/uninstall/start/stop/restart/status；
    /// 需管理员，变更动作自检提权——spec 007，status 只读免提权）
    MeshService,
    /// EasyTier 组网密钥写入 network-secret（交互式，无需管理员；spec 007）
    SetMeshSecret,
    /// HTTPS 访问账号 add/set/remove（交互式，无需管理员；spec 011 T6——
    /// 密码在脚本窗口 Read-Host 输入，工作台只传非敏感的 Action/Username/StackDir）
    SetHttpsAccount,
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
            Script::SetupAutostart => "setup-autostart.ps1",
            Script::InstallServer => "install-server.ps1",
            Script::InstallHttps => "install-https.ps1",
            Script::EnableHttps => "enable-https.ps1",
            Script::SetTencentKey => "set-tencent-key.ps1",
            Script::MeshService => "mesh-service.ps1",
            Script::SetMeshSecret => "set-mesh-secret.ps1",
            Script::SetHttpsAccount => "set-https-account.ps1",
        }
    }

    pub fn visibility(&self) -> Visibility {
        match self {
            Script::RunServerHidden | Script::SetupAutostart => {
                Visibility::Hidden
            }
            Script::InstallServer | Script::InstallHttps | Script::EnableHttps
            | Script::MeshService => {
                Visibility::Elevated
            }
            Script::SetTencentKey | Script::SetMeshSecret
            | Script::SetHttpsAccount => {
                Visibility::VisibleInteractive
            }
        }
    }

    /// 每脚本超时（plan §5.2；隐藏脚本由执行器执行，超时即杀并上报）
    pub fn timeout(&self) -> Duration {
        match self {
            Script::RunServerHidden => Duration::from_secs(15),
            Script::SetupAutostart => Duration::from_secs(60),
            Script::InstallServer => Duration::from_secs(900),
            Script::InstallHttps => Duration::from_secs(1800),
            Script::EnableHttps => Duration::from_secs(120),
            // 交互输入等待无上限，给足余量；隐藏执行器不消费此值（可见窗 detached）
            Script::SetTencentKey => Duration::from_secs(300),
            // 服务动作最快（stop/start 秒级；install 含落位与 Start-Service 预算 60s）
            Script::MeshService => Duration::from_secs(60),
            // 交互输入等待无上限，给足余量；隐藏执行器不消费此值（可见窗 detached）
            Script::SetMeshSecret => Duration::from_secs(300),
            // 交互输入等待无上限（密码两次输入 + validate/reload），可见窗 detached
            Script::SetHttpsAccount => Duration::from_secs(300),
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

/// 目录有效性哨兵：两组件编排的最小依赖（spec §4.5）
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

/// 进程环境注入（可能含敏感凭证：Debug 只显示变量名，值永不出现在日志/断言输出，
/// 宪法 §3；PartialEq 比较真实值供单测断言）
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SecretEnv(pub Vec<(String, String)>);

impl std::fmt::Debug for SecretEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.0.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_tuple("SecretEnv").field(&names).finish()
    }
}

/// 命令规格（数据态：可断言、可 mock、可入日志；env 字段经 SecretEnv 屏蔽值）
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
    /// 进程环境注入（空 = 不注入；caddy 拉起时注入 Caddyfile {env.*} 凭证，ADR-0003）
    pub env: SecretEnv,
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
        env: SecretEnv::default(),
    }
}

/// run-server-hidden.ps1：CloudCLI 拉起（-Port 显式化）
pub fn run_server_hidden(dir: &Path, lang: Lang, log_dir: &Path) -> CommandSpec {
    let mut spec = hidden_script_spec(dir, Script::RunServerHidden, lang, log_dir, &[]);
    spec.args.push("-Port".into());
    spec.args.push(CLOUDCLI_PORT.to_string());
    spec
}

/// setup-autostart.ps1：服务自启任务开（false）/ 关（true，-Remove）
pub fn setup_autostart(dir: &Path, lang: Lang, log_dir: &Path, remove: bool, stack_dir: &str) -> CommandSpec {
    let mut spec = hidden_script_spec(dir, Script::SetupAutostart, lang, log_dir, &[]);
    spec.args.push("-StackDir".into());
    spec.args.push(stack_dir.into());
    if remove {
        spec.args.push("-Remove".into());
    }
    spec
}

/// Caddy 原生拉起（plan §5.2：不经 powershell；参数与 setup-autostart.ps1 任务一致）。
/// 插件式 Caddyfile 的 {env.*} 凭证在 spawn 时注入进程环境（ADR-0003）——
/// 凭证经 dns_api::read_credential 自栈 .env 读取（D3 单源），仅进子进程环境、
/// 不入 CommandSpec 日志输出（SecretEnv 屏蔽）。
pub fn caddy_run(log_dir: &Path, stack_dir: &str) -> CommandSpec {
    let env = crate::dns_api::read_credential(stack_dir)
        .map(|cred| {
            SecretEnv(vec![
                ("TENCENT_SECRET_ID".into(), cred.id),
                ("TENCENT_SECRET_KEY".into(), cred.key),
            ])
        })
        .unwrap_or_default();
    CommandSpec {
        program: format!(r"{stack_dir}\caddy.exe"),
        args: vec!["run".into(), "--config".into(), CADDYFILE_PATH.into()],
        creation_flags: CREATE_NO_WINDOW,
        stdout_log: Some(log_dir.join("caddy.log")),
        stderr_log: Some(log_dir.join("caddy.err.log")),
        // 派发后由编排层轮询端口就绪（T8），此处超时仅为执行器兜底
        timeout: Duration::from_secs(15),
        working_dir: Some(PathBuf::from(stack_dir)),
        env,
    }
}

// ── 提权/可见窗口启动（AC19/20）────────────────────────────────────────────

/// 脚本结束后的收尾提示（配合 -NoExit 让窗口停留，输出可读）
fn finished_hint(lang: Lang) -> &'static str {
    match lang {
        Lang::Zh => "—— 脚本已结束，请核对上方输出后关闭本窗口 ——",
        Lang::En => "---- script finished; review the output above, then close this window ----",
    }
}

/// 可见窗口脚本参数串（ShellExecuteW lpParameters）。
/// 必须经 `-Command "& '<脚本>' <参数>"` 包装：`-File` 模式下脚本体内的 `exit`
/// 会直接终止 powershell.exe（-NoExit 拦不住，实测四个脚本的失败分支窗口瞬关）；
/// `&` 调用下 exit 仅退出脚本作用域，-NoExit 保留窗口，成功/失败输出均完整可读
/// （AC19：install-https 结尾手工步骤完整可读）。交互式脚本（Read-Host）
/// 不加 -NonInteractive。
pub fn visible_script_params(
    dir: &Path,
    script: Script,
    lang: Lang,
    extra_args: &[&str],
) -> String {
    // PowerShell 单引号字面量：含空格天然安全，内部单引号按规则翻倍
    let script_path = dir.join(script.file_name());
    let path = crate::lan_guard::ps_quote(&script_path.to_string_lossy());
    let mut inner = format!("& {path} -Lang {}", lang_arg(lang));
    for arg in extra_args {
        // spec 011 T1 sink 包裹：值参数（非 `-` 开头 token）一律单引号字面量
        // 包裹——注入兜底；参数名 token 保持裸，引号会让 PowerShell 把
        // `'-Update'` 绑成位置实参（评审实证其会进 $StackDir）
        inner.push(' ');
        if arg.starts_with('-') {
            inner.push_str(arg);
        } else {
            inner.push_str(&crate::lan_guard::ps_quote(arg));
        }
    }
    inner.push_str("; Write-Host ''; Write-Host '");
    inner.push_str(finished_hint(lang));
    inner.push('\'');
    format!("-NoProfile -ExecutionPolicy Bypass -NoExit -Command \"{inner}\"")
}

/// ShellExecuteW 原语（T15：提权/可见窗/URL 打开共用）。
/// 返回原始结果码（惯例 >32 成功；5 = SE_ERR_ACCESSDENIED 即 UAC 被拒），
/// 错误文案由调用方经 `lang::shell_error_text` 本地化（AC20）。
pub fn shell_execute(verb: Option<&str>, program: &str, params: &str) -> Result<(), isize> {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(std::iter::once(0)).collect()
        }
        let verb = verb.map(wide);
        let file = wide(program);
        let parameters = wide(params);
        let code = unsafe {
            ShellExecuteW(
                None,
                verb.as_deref().map(|v| PCWSTR::from_raw(v.as_ptr())).unwrap_or_default(),
                PCWSTR::from_raw(file.as_ptr()),
                PCWSTR::from_raw(parameters.as_ptr()),
                None,
                SW_SHOWNORMAL,
            )
            .0 as isize
        };
        if code > 32 {
            Ok(())
        } else {
            Err(code)
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (verb, program, params);
        Err(0)
    }
}

/// UAC 提权启动（ShellExecuteW verb=runas，可见窗；用户拒绝 → Err 交上层提示，AC20）
pub fn elevate(program: &str, params: &str) -> Result<(), String> {
    shell_execute(Some("runas"), program, params)
        .map_err(|code| format!("ShellExecuteW 返回 {code}（5=UAC 被拒；AC20：提示用户且不崩溃）"))
}

/// 普通可见窗口启动（无 UAC）：交互式脚本（install-client.ps1）走此通道
pub fn open_visible(program: &str, params: &str) -> Result<(), isize> {
    shell_execute(Some("open"), program, params)
}

/// 用默认浏览器打开 URL（open_external 命令与托盘共用）
pub fn open_url(url: &str) -> Result<(), isize> {
    shell_execute(Some("open"), url, "")
}

/// 用资源管理器打开目录（open_logs_dir 命令）
pub fn open_dir(path: &Path) -> Result<(), isize> {
    shell_execute(Some("open"), &path.to_string_lossy(), "")
}

// ── 输入校验双闸（spec 011 T1：派发层 + 设置源头，AC1/AC2）──────────────────

/// domain 白名单校验（spec 011 AC1/AC2）：DNS 主机名形态——总长 ≤253；按 `.`
/// 分段，每段 1~63 字符、字符仅 ASCII 字母/数字/连字符、不以连字符起止；
/// 拒绝任何非 ASCII 与空段。错误文案中文、指明非法类别。
pub fn validate_domain(domain: &str) -> Result<(), String> {
    if domain.is_empty() {
        return Err("domain 非法：不能为空（不设置域名请传 null，勿传空串）".into());
    }
    if domain.len() > 253 {
        return Err(format!("domain 非法：总长 {} 超过 253 字符上限", domain.len()));
    }
    if !domain.is_ascii() {
        return Err("domain 非法：含非 ASCII 字符（仅允许英文字母、数字、连字符与点）".into());
    }
    for label in domain.split('.') {
        if label.is_empty() {
            return Err("domain 非法：存在空段（连续的点或首尾的点）".into());
        }
        if label.len() > 63 {
            return Err(format!("domain 非法：段「{label}」超过 63 字符上限"));
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(format!(
                "domain 非法：段「{label}」含非法字符（仅允许英文字母、数字、连字符）"
            ));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(format!("domain 非法：段「{label}」以连字符开头或结尾"));
        }
    }
    Ok(())
}

/// stack_dir 拒绝的 PowerShell 元字符（spec 011 AC1）：值参数虽有 sink 包裹
/// 兜底，源头仍拒绝——纵深而非单点。空格不在列（合法路径成分，由包裹兜安全）。
const STACK_DIR_FORBIDDEN: [char; 11] = ['\'', '"', ';', '`', '$', '(', ')', '|', '&', '<', '>'];

/// stack_dir 白名单校验（spec 011 AC1/AC2）：必须是绝对盘符路径（`X:\...`
/// 形态，分隔符亦容忍 `/`）；拒绝 PowerShell 元字符、非 ASCII 与控制字符；
/// 空格允许（由 sink 单引号包裹兜底安全）。
pub fn validate_stack_dir(dir: &str) -> Result<(), String> {
    let bytes = dir.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || (bytes[2] != b'\\' && bytes[2] != b'/')
    {
        return Err(format!(
            "stack_dir 非法：必须是绝对盘符路径（如 D:\\apps\\stack），当前「{dir}」"
        ));
    }
    if !dir.is_ascii() {
        return Err("stack_dir 非法：含非 ASCII 字符（中文等目录名请改用英文）".into());
    }
    if let Some(c) = dir.chars().find(|c| STACK_DIR_FORBIDDEN.contains(c)) {
        return Err(format!("stack_dir 非法：含 PowerShell 元字符「{c}」"));
    }
    if dir.chars().any(|c| c.is_control()) {
        return Err("stack_dir 非法：含控制字符".into());
    }
    Ok(())
}

// ── 访问账号参数校验（spec 011 T6/AC11：派发闸，构造命令行之前）──────────────

/// auth_action 白名单（spec 011 AC11）：仅 add / set / remove 三动作。
/// 错误文案中文、指明字段与合法取值。
pub fn validate_auth_action(action: &str) -> Result<(), String> {
    if matches!(action, "add" | "set" | "remove") {
        Ok(())
    } else {
        Err(format!(
            "auth_action 非法：「{action}」——仅允许 add（新增）/ set（改密）/ remove（移除）"
        ))
    }
}

/// auth_user 用户名白名单（spec 011 AC11 / plan §4）：`^[a-zA-Z0-9_-]{1,32}$`
/// ——与脚本侧同规格（注入防线双闸；空值走 tool_plan 的缺失分支，此处只裁形态）。
pub fn validate_auth_user(name: &str) -> Result<(), String> {
    let len_ok = (1..=32).contains(&name.chars().count());
    let chars_ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if len_ok && chars_ok {
        Ok(())
    } else {
        Err(format!(
            "username 非法：「{name}」——仅允许英文字母/数字/下划线/连字符，长度 1~32"
        ))
    }
}

// ── 低频工具派发（run_tool，plan §5.1 / AC19-20）────────────────────────────

/// 工具类别（前端序列化：snake_case 字符串）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// 安装/重装服务端（install-server.ps1，UAC）
    InstallServer,
    /// HTTPS 栈装机（install-https.ps1，UAC）
    InstallHttps,
    /// HTTPS 环境配置（enable-https.ps1，UAC）
    EnableHttps,
    /// 腾讯云 CAM 密钥写入 .env（set-tencent-key.ps1，可见交互窗；spec 006）
    SetTencentKey,
    /// EasyTier 组网密钥写入 network-secret（set-mesh-secret.ps1，可见交互窗；
    /// spec 007 AC8——密钥经交互脚本注入，不进工作台内存）
    SetMeshSecret,
    /// HTTPS 访问账号 add/set/remove（set-https-account.ps1，可见交互窗；
    /// spec 011 AC11——密码在脚本窗口输入，工作台只传非敏感参数）
    SetHttpsAccount,
}

impl From<ToolKind> for Script {
    fn from(kind: ToolKind) -> Self {
        match kind {
            ToolKind::InstallServer => Script::InstallServer,
            ToolKind::InstallHttps => Script::InstallHttps,
            ToolKind::EnableHttps => Script::EnableHttps,
            ToolKind::SetTencentKey => Script::SetTencentKey,
            ToolKind::SetMeshSecret => Script::SetMeshSecret,
            ToolKind::SetHttpsAccount => Script::SetHttpsAccount,
        }
    }
}

/// 安装类可选项（plan §5.2 + spec 006 域名透传）
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolOpts {
    /// `-Update`：升级 CloudCLI 到最新版
    pub update: bool,
    /// `-UseMirror`：npm 换国内镜像源
    pub mirror: bool,
    /// `-Domain`：安装/配置类脚本的域名透传（装机向导自定义域名；None = 脚本默认）
    pub domain: Option<String>,
    /// `-Action`：访问账号动作（spec 011 T6——仅 add/set/remove，派发前白名单校验）
    pub auth_action: Option<String>,
    /// `-Username`：访问账号用户名（spec 011 T6——`^[a-zA-Z0-9_-]{1,32}$`，
    /// 派发前校验；密码绝不在此结构中——脚本窗口交互输入）
    pub auth_user: Option<String>,
}

/// 派发计划（纯函数构造，可单测；run_tool 命令消费）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDispatch {
    pub script: Script,
    /// ShellExecuteW lpParameters（可见窗 + -NoExit + -Lang）
    pub params: String,
    /// true = runas 提权；false = 普通可见窗（交互式）
    pub elevated: bool,
}

/// 构造派发计划：可见窗参数 + 提权判定（按脚本契约表 Visibility 推导）。
/// spec 011 AC1 双闸之一：构造任何命令行之前先过 domain/stack_dir 白名单——
/// 非法 → Err（中文、指明字段与非法类别），不产出 ToolDispatch、不启动进程。
pub fn tool_plan(
    kind: ToolKind,
    opts: ToolOpts,
    dir: &Path,
    lang: Lang,
    stack_dir: &str,
) -> Result<ToolDispatch, String> {
    // spec 011 AC1 双闸之一：构造任何命令行之前先过白名单——前端被攻破时
    // 校验依然生效（不信任前端）；非法 → Err，不产出 ToolDispatch
    if let Some(domain) = opts.domain.as_deref() {
        validate_domain(domain)?;
    }
    validate_stack_dir(stack_dir)?;
    // spec 011 AC11 派发闸：访问账号动作/用户名白名单——前端被攻破时校验依然生效，
    // 非法 → Err（中文、指明字段），不产出 ToolDispatch、不启动任何进程
    if matches!(kind, ToolKind::SetHttpsAccount) {
        let action = opts.auth_action.as_deref().ok_or_else(|| {
            "auth_action 缺失：访问账号动作必须提供（add / set / remove）".to_string()
        })?;
        validate_auth_action(action)?;
        let user = opts.auth_user.as_deref().ok_or_else(|| {
            "username 缺失：访问账号用户名必须提供（字母/数字/下划线/连字符，1~32 字符）"
                .to_string()
        })?;
        validate_auth_user(user)?;
    }
    let script = Script::from(kind);
    let mut extra: Vec<&str> = Vec::new();
    if matches!(kind, ToolKind::InstallServer) {
        if opts.update {
            extra.push("-Update");
        }
        if opts.mirror {
            extra.push("-UseMirror");
        }
    }
    // 感知栈目录的脚本跟随用户配置（spec 004：装机/密钥写入正确位置）
    if matches!(
        kind,
        ToolKind::InstallHttps
            | ToolKind::SetTencentKey
            | ToolKind::SetMeshSecret
            | ToolKind::SetHttpsAccount
    ) {
        extra.push("-StackDir");
        extra.push(stack_dir);
    }
    // 访问账号：非敏感的动作与用户名（值经 sink 单引号包裹；密码不存在于任何参数）
    if matches!(kind, ToolKind::SetHttpsAccount) {
        extra.push("-Action");
        extra.push(opts.auth_action.as_deref().expect("上方白名单校验已确保存在"));
        extra.push("-Username");
        extra.push(opts.auth_user.as_deref().expect("上方白名单校验已确保存在"));
    }
    // 域名透传（spec 006：装机向导自定义域名 → Caddyfile；ddns-go 已退役）
    if let Some(domain) = opts.domain.as_deref() {
        if matches!(kind, ToolKind::InstallHttps) {
            extra.push("-Domain");
            extra.push(domain);
        }
    }
    Ok(ToolDispatch {
        script,
        params: visible_script_params(dir, script, lang, &extra),
        elevated: script.visibility() == Visibility::Elevated,
    })
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
            // setup-autostart.ps1：栈目录缺少 caddy.exe（ddns-go 已随直连通道退役，spec 008）
            Script::SetupAutostart => ScriptOutcome::Unavailable("栈目录缺少 caddy.exe"),
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
    /// 派发不等待（常驻服务进程：caddy run / run-server-hidden 的 Start-Process 语义）
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
        // 常驻服务进程（caddy run）因此存活；日志句柄随子进程生命周期持有
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
    for (key, value) in &spec.env.0 {
        cmd.env(key, value);
    }
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
    use crate::consts::DEFAULT_STACK_DIR;
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
        assert_eq!(Script::SetTencentKey.visibility(), Visibility::VisibleInteractive);
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
    fn setup_autostart_specs() {
        let dir = script_dir("build2");
        let logs = dir.join("logs");

        let enable = setup_autostart(&dir, Lang::Zh, &logs, false, DEFAULT_STACK_DIR);
        assert!(enable.command_line().contains("-StackDir D:\\Software\\cloudcli-https"), "{}", enable.command_line());
        assert!(!enable.command_line().contains("-Remove"), "开启分支不应带 -Remove");
        assert_eq!(enable.timeout, Duration::from_secs(60));

        let disable = setup_autostart(&dir, Lang::Zh, &logs, true, DEFAULT_STACK_DIR);
        assert!(disable.command_line().contains("-Remove"), "关闭分支应带 -Remove");
    }

    #[test]
    fn native_caddy_spec_matches_autostart_contract() {
        // setup-autostart.ps1：caddy.exe run --config '<StackDir>\Caddyfile'
        let logs = PathBuf::from(r"D:\any\logs");
        let spec = caddy_run(&logs, DEFAULT_STACK_DIR);
        assert_eq!(spec.program, r"D:\Software\cloudcli-https\caddy.exe");
        assert_eq!(spec.args, vec!["run".to_string(), "--config".to_string(), CADDYFILE_PATH.to_string()]);
        assert_eq!(spec.creation_flags, CREATE_NO_WINDOW, "服务进程也不许弹窗");
        assert_eq!(spec.stdout_log.as_deref(), Some(logs.join("caddy.log").as_path()), "{:?}", spec.stdout_log);
        assert_eq!(spec.working_dir.as_deref(), Some(Path::new(DEFAULT_STACK_DIR)));
    }

    // ── 可见/提权参数串 ────────────────────────────────────────────────

    #[test]
    fn visible_params_survive_script_exit_and_keep_window() {
        let dir = script_dir("elev");
        let p = visible_script_params(&dir, Script::InstallHttps, Lang::Zh, &["-Update"]);
        assert!(p.contains("-NoExit"), "AC19：结尾手工步骤须可读 → -NoExit：{p}");
        assert!(
            p.contains("-Command"),
            "必须经 -Command 包装（-File 下脚本内 exit 会连窗关闭，实测缺陷）：{p}"
        );
        assert!(
            p.contains(&format!("& '{}'", dir.join("install-https.ps1").to_string_lossy())),
            "脚本路径以单引号字面量传给 & 调用：{p}"
        );
        assert!(!p.contains("-NonInteractive"), "交互式脚本禁用 -NonInteractive：{p}");
        assert!(p.contains("-Lang zh"));
        assert!(p.contains("-Update"), "额外参数应透传：{p}");
        assert!(
            p.contains("核对上方输出后关闭本窗口"),
            "结尾应有收尾提示（配合 -NoExit 停留可读）：{p}"
        );
    }

    #[test]
    fn visible_params_quote_path_with_spaces_and_quotes() {
        // 含空格路径：单引号字面量天然安全；路径内单引号按 PowerShell 规则翻倍
        let dir = Path::new(r"D:\with space");
        let p = visible_script_params(dir, Script::SetTencentKey, Lang::En, &[]);
        assert!(p.contains(r"'D:\with space\set-tencent-key.ps1'"), "{p}");

        let dir_q = Path::new(r"D:\odd'name");
        let p2 = visible_script_params(dir_q, Script::SetTencentKey, Lang::En, &[]);
        assert!(p2.contains(r"'D:\odd''name\set-tencent-key.ps1'"), "{p2}");
    }

    // ── 低频工具派发（run_tool 契约，T15）──────────────────────────────

    #[test]
    fn tool_plan_install_server_default_and_options() {
        let dir = script_dir("tool");
        // 默认安装/重装：UAC 提权 + -Lang + -NoExit，无 -Update/-UseMirror
        let plan = tool_plan(ToolKind::InstallServer, ToolOpts::default(), &dir, Lang::Zh, DEFAULT_STACK_DIR)
            .expect("默认栈目录应可派发");
        assert_eq!(plan.script, Script::InstallServer);
        assert!(plan.elevated, "install-server 需 UAC");
        assert!(plan.params.contains("install-server.ps1"), "{}", plan.params);
        assert!(plan.params.contains("-Lang zh"));
        assert!(!plan.params.contains("-Update") && !plan.params.contains("-UseMirror"), "{}", plan.params);

        // 升级 + 镜像源：两个开关透传
        let opts = ToolOpts { update: true, mirror: true, domain: None, ..ToolOpts::default() };
        let plan = tool_plan(ToolKind::InstallServer, opts, &dir, Lang::En, DEFAULT_STACK_DIR)
            .expect("默认栈目录应可派发");
        assert!(plan.params.contains("-Update"), "{}", plan.params);
        assert!(plan.params.contains("-UseMirror"), "{}", plan.params);
        assert!(plan.params.contains("-Lang en"), "{}", plan.params);
    }

    #[test]
    fn tool_plan_https_visibility() {
        let dir = script_dir("tool2");
        // 提权类：install-https / enable-https → runas 可见窗（AC19：结尾手工步骤可读）
        let https = tool_plan(ToolKind::InstallHttps, ToolOpts::default(), &dir, Lang::Zh, DEFAULT_STACK_DIR)
            .expect("默认栈目录应可派发");
        assert!(https.elevated);
        assert!(https.params.contains("install-https.ps1") && https.params.contains("-NoExit"));

        let enable = tool_plan(ToolKind::EnableHttps, ToolOpts::default(), &dir, Lang::Zh, DEFAULT_STACK_DIR)
            .expect("默认栈目录应可派发");
        assert!(enable.elevated);
        assert!(enable.params.contains("enable-https.ps1"));
    }

    #[test]
    fn tool_plan_set_tencent_key_passes_stack_dir() {
        // spec 008：set-tencent-key 为栈 .env 凭证唯一写入通道，须带 -StackDir
        let dir = script_dir("tool3");
        let plan = tool_plan(ToolKind::SetTencentKey, ToolOpts::default(), &dir, Lang::Zh, DEFAULT_STACK_DIR)
            .expect("默认栈目录应可派发");
        assert_eq!(plan.script, Script::SetTencentKey);
        assert!(!plan.elevated, "密钥写入为用户级交互操作，无 UAC");
        assert!(plan.params.contains("set-tencent-key.ps1"), "{}", plan.params);
        assert!(plan.params.contains("-StackDir"), "感知栈目录：{}", plan.params);
        assert!(plan.params.contains("-Lang zh"), "-Lang 对齐程序语言：{}", plan.params);
        assert!(!plan.params.contains("-NonInteractive"), "交互式脚本禁用 -NonInteractive：{}", plan.params);
    }

    // ── spec 011 T1：输入校验双闸 + sink 包裹（AC1/AC2）─────────────────

    /// AC1：domain 注入元字符逐个拒绝 + 形态类拒绝（空段/超长/连字符起止/非 ASCII）
    #[test]
    fn validate_domain_rejects_injection_and_bad_shapes() {
        for c in [';', '\'', '"', '`', '$', '(', ')', '|', '&', '<', '>', ' ', '中'] {
            let bad = format!("ai.jackqi{c}cn");
            let err =
                validate_domain(&bad).expect_err(&format!("domain 元字符「{c}」应拒绝：{bad}"));
            assert!(err.contains("domain"), "「{c}」文案应指明字段：{err}");
        }
        // 空串 / 空段（连续、首、尾点）/ 单段超 63 / 总长超 253 / 连字符起止
        assert!(validate_domain("").is_err(), "空串拒绝");
        assert!(validate_domain("ai..cn").is_err(), "连续点空段拒绝");
        assert!(validate_domain(".ai.cn").is_err(), "首点空段拒绝");
        assert!(validate_domain("ai.cn.").is_err(), "尾点空段拒绝");
        assert!(
            validate_domain(&format!("{}.example.com", "a".repeat(64))).is_err(),
            "段超 63 拒绝"
        );
        assert!(
            validate_domain(&format!("{}.com", "a".repeat(251))).is_err(),
            "总长超 253 拒绝"
        );
        assert!(validate_domain("-ai.cn").is_err(), "段以连字符开头拒绝");
        assert!(validate_domain("ai-.cn").is_err(), "段以连字符结尾拒绝");
        assert!(validate_domain("中文.cn").is_err(), "非 ASCII 拒绝");
    }

    /// AC2：合法 DNS 主机名放行（多级域名、单标签、段内连字符、边界长度、大小写）
    #[test]
    fn validate_domain_accepts_legal_hostnames() {
        validate_domain("ai.jackqi.cn").expect("多级域名应放行");
        validate_domain("localhost").expect("单标签应放行");
        validate_domain("a-b.example.com").expect("段内连字符应放行");
        validate_domain(&format!("{}.example.com", "a".repeat(63)))
            .expect("63 字符段边界应放行");
        // 253 总长边界：63+1+63+1+62+1+62 = 253
        let boundary = format!(
            "{}.{}.{}.{}",
            "a".repeat(63),
            "b".repeat(63),
            "c".repeat(62),
            "d".repeat(62)
        );
        assert_eq!(boundary.len(), 253);
        validate_domain(&boundary).expect("总长 253 边界应放行");
        validate_domain("AI.Example.COM").expect("大小写应放行");
    }

    /// AC1：stack_dir 注入元字符逐个拒绝 + 非盘符形态拒绝
    #[test]
    fn validate_stack_dir_rejects_injection_and_bad_shapes() {
        for c in [';', '\'', '"', '`', '$', '(', ')', '|', '&', '<', '>'] {
            let bad = format!("D:\\my{c}stack");
            let err = validate_stack_dir(&bad)
                .expect_err(&format!("stack_dir 元字符「{c}」应拒绝：{bad}"));
            assert!(err.contains("stack_dir"), "「{c}」文案应指明字段：{err}");
        }
        assert!(validate_stack_dir("D:\\我的栈").is_err(), "非 ASCII 拒绝");
        for bad in ["", "stack\\dir", "\\\\server\\share", "D:", "D:relative", "/unix/abs"] {
            assert!(validate_stack_dir(bad).is_err(), "非盘符形态应拒绝：{bad}");
        }
    }

    /// AC2：合法绝对盘符路径放行（含空格、正斜杠、小写盘符）
    #[test]
    fn validate_stack_dir_accepts_drive_paths_with_spaces() {
        validate_stack_dir(r"D:\Software\cloudcli-https").expect("默认栈目录应放行");
        validate_stack_dir(r"D:\my stack\cloudcli-https").expect("含空格应放行");
        validate_stack_dir("E:/fwd/slash").expect("正斜杠分隔应放行");
        validate_stack_dir(r"d:\lower\case").expect("小写盘符应放行");
    }

    /// AC2 sink 包裹形态：值参数单引号字面量包裹（含空格不破形）；
    /// 参数名 token（-Update）保持裸——引号会让 PowerShell 绑成位置实参
    /// （评审实证 '-Update' 会进 $StackDir）
    #[test]
    fn visible_params_wrap_values_but_keep_switch_tokens_bare() {
        let dir = script_dir("wrap");
        let p = visible_script_params(
            &dir,
            Script::InstallHttps,
            Lang::Zh,
            &["-StackDir", r"D:\my stack\https", "-Update"],
        );
        assert!(
            p.contains(r"-StackDir 'D:\my stack\https'"),
            "值参数应单引号包裹：{p}"
        );
        assert!(
            p.contains(" -Update") && !p.contains("'-Update'"),
            "参数名保持裸 token：{p}"
        );
    }

    /// sink 兜底（纵深）：值内单引号按 PowerShell 规则翻倍——即使双闸漏放，
    /// 包裹形态也不破形
    #[test]
    fn visible_params_double_embedded_single_quotes_in_values() {
        let dir = script_dir("esc");
        let p = visible_script_params(
            &dir,
            Script::SetHttpsAccount,
            Lang::En,
            &["-StackDir", r"D:\od'd"],
        );
        assert!(p.contains(r"-StackDir 'D:\od''d'"), "内部单引号应翻倍：{p}");
    }

    /// AC1 派发闸：domain/stack_dir 非法 → Err（类型上即不产出命令行构造）；
    /// 文案中文且指明字段
    #[test]
    fn tool_plan_rejects_injection_before_building_params() {
        let dir = script_dir("gate");
        for c in [';', '\'', '"', '`', '$', '(', ')', '|', '&', '<', '>', '中'] {
            let opts = ToolOpts {
                domain: Some(format!("ai.jackqi{c}cn")),
                ..ToolOpts::default()
            };
            let err = tool_plan(ToolKind::InstallHttps, opts, &dir, Lang::Zh, DEFAULT_STACK_DIR)
                .expect_err(&format!("domain 元字符「{c}」应拒绝派发"));
            assert!(err.contains("domain"), "「{c}」文案应指明字段：{err}");
        }
        for c in [';', '\'', '"', '`', '$', '(', ')', '|', '&', '<', '>'] {
            let bad = format!("D:\\evil{c}stack");
            let err = tool_plan(ToolKind::SetTencentKey, ToolOpts::default(), &dir, Lang::Zh, &bad)
                .expect_err(&format!("stack_dir 元字符「{c}」应拒绝派发"));
            assert!(err.contains("stack_dir"), "「{c}」文案应指明字段：{err}");
        }
        // 非盘符形态同样拒绝（相对路径 / UNC / 裸盘符）
        for bad in ["no\\drive", "\\\\srv\\share", "E:"] {
            assert!(
                tool_plan(ToolKind::SetMeshSecret, ToolOpts::default(), &dir, Lang::Zh, bad).is_err(),
                "非盘符栈目录应拒绝：{bad}"
            );
        }
    }

    /// AC2：合法输入正常透传——多级域名与含空格栈目录以单引号包裹形态进命令行
    #[test]
    fn tool_plan_accepts_legal_inputs_with_wrapped_values() {
        let dir = script_dir("ok");
        let opts = ToolOpts {
            domain: Some("ai.jackqi.cn".into()),
            ..ToolOpts::default()
        };
        let plan = tool_plan(
            ToolKind::InstallHttps,
            opts,
            &dir,
            Lang::Zh,
            r"D:\my stack\cloudcli-https",
        )
        .expect("合法输入应放行");
        assert!(plan.params.contains("-Domain 'ai.jackqi.cn'"), "{}", plan.params);
        assert!(
            plan.params.contains(r"-StackDir 'D:\my stack\cloudcli-https'"),
            "{}",
            plan.params
        );
        assert!(plan.elevated, "install_https 仍为提权可见窗");
    }

    /// spec 008：已退役工具种类不得再被前端唤起（serde 反序列化拒绝）
    #[test]
    fn retired_tool_kinds_fail_deserialization() {
        for gone in ["reset_ddns_password", "set_frp_key", "config_ddns_go", "clear_frp_key"] {
            assert!(
                serde_json::from_str::<ToolKind>(&format!("\"{gone}\"")).is_err(),
                "退役种类 {gone} 应被拒绝"
            );
        }
        assert_eq!(
            serde_json::from_str::<ToolKind>("\"set_tencent_key\"").unwrap(),
            ToolKind::SetTencentKey
        );
    }

    // ── spec 011 T6：访问账号派发闸 + 参数包裹（AC11）───────────────────

    /// action 白名单：三动作放行，其余（含大小写变体/注入串）拒绝且文案指明字段
    #[test]
    fn validate_auth_action_whitelist() {
        for good in ["add", "set", "remove"] {
            validate_auth_action(good).expect("合法动作应放行");
        }
        for bad in ["", "delete", "ADD", "add ", "add;rm", "add\nset", "']&&"] {
            let err =
                validate_auth_action(bad).expect_err(&format!("非法动作「{bad}」应拒绝"));
            assert!(err.contains("auth_action"), "「{bad}」文案应指明字段：{err}");
        }
    }

    /// username 白名单：合法形态放行（含下划线/连字符/数字、32 边界）；
    /// 注入元字符、超长、空、非 ASCII、控制字符拒绝
    #[test]
    fn validate_auth_user_whitelist() {
        for good in ["jack", "a", "member-02", "phone_home", &"u".repeat(32)] {
            validate_auth_user(good).expect("合法用户名应放行");
        }
        for bad in [
            "", " ", "jack;rm", "jack'rule", "jack\"x", "ja ck", "jack`n", "a$b",
            "$(whoami)", "jack|ls", "jack&dir", "jack>out", "jack<in", "中文", "jack\n",
            &"u".repeat(33),
        ] {
            let err =
                validate_auth_user(bad).expect_err(&format!("非法用户名「{bad}」应拒绝"));
            assert!(err.contains("username"), "「{bad}」文案应指明字段：{err}");
        }
    }

    /// 派发形态：-StackDir/-Action/-Username 值参数单引号包裹、参数名裸 token；
    /// 可见交互窗（无 UAC、无 -NonInteractive——密码要靠 Read-Host 输入）
    #[test]
    fn tool_plan_set_https_account_dispatch_shape() {
        let dir = script_dir("auth");
        let opts = ToolOpts {
            auth_action: Some("add".into()),
            auth_user: Some("member-02".into()),
            ..ToolOpts::default()
        };
        let plan = tool_plan(ToolKind::SetHttpsAccount, opts, &dir, Lang::Zh, DEFAULT_STACK_DIR)
            .expect("合法账号参数应可派发");
        assert_eq!(plan.script, Script::SetHttpsAccount);
        assert!(!plan.elevated, "账号管理为用户级交互操作，无 UAC");
        assert!(plan.params.contains("set-https-account.ps1"), "{}", plan.params);
        assert!(plan.params.contains("-NoExit"), "交互脚本窗口结束后保留：{}", plan.params);
        assert!(!plan.params.contains("-NonInteractive"), "交互式脚本禁用 -NonInteractive：{}", plan.params);
        assert!(
            plan.params.contains(r"-StackDir 'D:\Software\cloudcli-https'"),
            "栈目录应以单引号包裹透传：{}",
            plan.params
        );
        assert!(
            plan.params.contains("-Action 'add'") && plan.params.contains("-Username 'member-02'"),
            "动作与用户名应以单引号包裹透传：{}",
            plan.params
        );
        // 值内单引号翻倍兜底（纵深）：即使白名单漏放也不破形
        let opts_q = ToolOpts {
            auth_action: Some("add".into()),
            auth_user: Some("ja'ck".into()),
            ..ToolOpts::default()
        };
        let err = tool_plan(ToolKind::SetHttpsAccount, opts_q, &dir, Lang::Zh, DEFAULT_STACK_DIR)
            .expect_err("含单引号用户名应被白名单拒绝");
        assert!(err.contains("username"), "文案应指明字段：{err}");
    }

    /// 派发闸：缺失/非法 action、缺失/非法 username → Err（不产出命令行），
    /// 文案中文且指明字段；合法 remove 同样放行
    #[test]
    fn tool_plan_set_https_account_rejects_bad_params() {
        let dir = script_dir("authgate");
        // 缺失
        let err = tool_plan(
            ToolKind::SetHttpsAccount,
            ToolOpts {
                auth_action: None,
                auth_user: Some("jack".into()),
                ..ToolOpts::default()
            },
            &dir,
            Lang::Zh,
            DEFAULT_STACK_DIR,
        )
        .expect_err("缺失 action 应拒绝派发");
        assert!(err.contains("auth_action"), "{err}");
        let err = tool_plan(
            ToolKind::SetHttpsAccount,
            ToolOpts {
                auth_action: Some("add".into()),
                auth_user: None,
                ..ToolOpts::default()
            },
            &dir,
            Lang::Zh,
            DEFAULT_STACK_DIR,
        )
        .expect_err("缺失 username 应拒绝派发");
        assert!(err.contains("username"), "{err}");
        // 非法（注入形态——派发闸在包裹兜底之前就该拦下）
        for (action, user) in [("delete", "jack"), ("add;rm -rf", "jack"), ("add", "jack;calc"), ("add", " jack")] {
            let err = tool_plan(
                ToolKind::SetHttpsAccount,
                ToolOpts {
                    auth_action: Some(action.into()),
                    auth_user: Some(user.into()),
                    ..ToolOpts::default()
                },
                &dir,
                Lang::Zh,
                DEFAULT_STACK_DIR,
            )
            .expect_err(&format!("非法参数（{action}/{user}）应拒绝派发"));
            assert!(
                err.contains("auth_action") || err.contains("username"),
                "文案应指明字段：{err}"
            );
        }
        // remove 合法放行（无密码交互的动作）
        let plan = tool_plan(
            ToolKind::SetHttpsAccount,
            ToolOpts {
                auth_action: Some("remove".into()),
                auth_user: Some("jack".into()),
                ..ToolOpts::default()
            },
            &dir,
            Lang::En,
            DEFAULT_STACK_DIR,
        )
        .expect("remove 应放行");
        assert!(plan.params.contains("-Action 'remove'"), "{}", plan.params);
    }

    /// 前端序列化契约：set_https_account 可反序列化（snake_case）
    #[test]
    fn set_https_account_tool_kind_deserializes() {
        assert_eq!(
            serde_json::from_str::<ToolKind>("\"set_https_account\"").unwrap(),
            ToolKind::SetHttpsAccount
        );
        assert_eq!(Script::from(ToolKind::SetHttpsAccount).file_name(), "set-https-account.ps1");
        assert_eq!(
            Script::from(ToolKind::SetHttpsAccount).visibility(),
            Visibility::VisibleInteractive
        );
    }

    // ── 退出码语义 ─────────────────────────────────────────────────────

    #[test]
    fn interpret_exit_maps_sprint0_codes() {
        use ScriptOutcome::*;
        // run-server-hidden：0=成功（含端口已在监听的幂等成功）/1=cloudcli 不可用
        assert_eq!(interpret_exit(Script::RunServerHidden, 0), Success);
        assert_eq!(interpret_exit(Script::RunServerHidden, 1), Unavailable("cloudcli 不可用（未安装或不在 PATH）"));
        assert_eq!(interpret_exit(Script::RunServerHidden, 2), Failed(2));
        // setup-autostart：1=栈目录缺 caddy.exe（ddns-go 已退役，spec 008）
        assert_eq!(interpret_exit(Script::SetupAutostart, 1), Unavailable("栈目录缺少 caddy.exe"));
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
        assert!(mock.dispatch(&caddy_run(&logs, DEFAULT_STACK_DIR)).is_ok());
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
            env: SecretEnv::default(),
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
            env: SecretEnv::default(),
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
            env: SecretEnv::default(),
        };
        let start = std::time::Instant::now();
        assert!(ProcessExecutor.dispatch(&spec).is_ok());
        assert!(start.elapsed() < Duration::from_secs(5), "dispatch 不应等待子进程完成");
    }

    /// 独享临时栈目录（含 .env 写入辅助），返回路径
    fn temp_stack_dir(tag: &str, env_content: Option<&str>) -> std::path::PathBuf {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "wb-scripts-caddyenv-{}-{tag}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("创建临时栈目录失败");
        if let Some(content) = env_content {
            std::fs::write(dir.join(".env"), content).expect("写 .env 失败");
        }
        dir
    }

    /// ADR-0003：caddy_run 从栈 .env 读凭证注入 {env.*} 所需进程环境
    #[test]
    fn caddy_run_injects_tencent_creds_from_env_file() {
        let stack = temp_stack_dir(
            "creds",
            Some("SAMPLE_KEY=ignored\nTENCENT_SECRET_ID=AKIDtest1234\nTENCENT_SECRET_KEY=secretkey\n"),
        );
        let spec = caddy_run(&stack.join("logs"), stack.to_str().unwrap());
        assert_eq!(
            spec.env.0,
            vec![
                ("TENCENT_SECRET_ID".to_string(), "AKIDtest1234".to_string()),
                ("TENCENT_SECRET_KEY".to_string(), "secretkey".to_string()),
            ],
            "应注入且仅注入 Caddyfile {{env.*}} 消费的两个变量"
        );
        // 宪法 §3：Debug/日志路径不得出现凭证值（SecretEnv 屏蔽）
        let dbg = format!("{spec:?}");
        assert!(!dbg.contains("AKIDtest1234"), "Debug 泄漏 SecretId：{dbg}");
        assert!(!dbg.contains("secretkey"), "Debug 泄漏 SecretKey：{dbg}");
        assert!(dbg.contains("SecretEnv"), "应显示屏蔽形态：{dbg}");
        let _ = std::fs::remove_dir_all(&stack);
    }

    /// 无 .env / 无凭证：不注入（CommandSpec 其余断言不受影响）
    #[test]
    fn caddy_run_without_credentials_has_empty_env() {
        let stack = temp_stack_dir("nocreds", None);
        let spec = caddy_run(&stack.join("logs"), stack.to_str().unwrap());
        assert!(spec.env.0.is_empty(), "无凭证时 env 应为空");
        assert_eq!(
            spec.program,
            format!(r"{}\caddy.exe", stack.display()),
            "program 不受 env 逻辑影响"
        );
        let _ = std::fs::remove_dir_all(&stack);
    }
}
