//! 设置持久化模块（AC21/23/24 逻辑部分，plan §4 schema / §5.1 命令）。
//!
//! - 路径：`%APPDATA%\ai-remote-workbench\settings.json`（spec §4.1）
//! - 加载：文件缺失 → 默认值（AC23）；损坏（非法 JSON/结构）→ 改名
//!   `.bad-<时间戳>` 留档 + 回退默认 + 上层发 `settings://repaired`（AC24）
//! - 保存：补丁合并（只改提交的字段）+ 原子写（临时文件 rename，防半写）
//!
//! 核心函数均以路径为入参（注入式），便于单测；Tauri 命令为薄封装。

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 设置文件所在目录名（%APPDATA% 下）
pub const APP_DIR_NAME: &str = "ai-remote-workbench";
/// 设置文件名
pub const SETTINGS_FILE: &str = "settings.json";
/// 设置文件 schema 版本
pub const SCHEMA_VERSION: u32 = 1;
/// 设置文件损坏后留档的事件名（命令/装配层发出，前端 T14 监听）
pub const EVENT_REPAIRED: &str = "settings://repaired";

/// 语言设置（AC25）：auto = 跟随系统显示语言
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguageSetting {
    Auto,
    Zh,
    En,
}

/// 退出动作（AC13~15）：keep = 保留服务退出，stop = 停止服务后退出
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExitAction {
    Keep,
    Stop,
}

/// 访问通道（spec 008 收敛）：mesh = EasyTier 私有组网，唯一通道
/// （直连/穿透已彻底移除；局域网 IP 直访非"通道"，是 3001 的固有形态）。
/// 旧值迁移（spec 008 D1/AC2）：加载旧版 settings.json 时
/// `"direct"`/`"tunnel"` 一律映射为 Mesh，其余字段不受影响；
/// 未知值报错走既有损坏修复流程（AC24 口径）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessChannel {
    #[default]
    Mesh,
}

impl<'de> Deserialize<'de> for AccessChannel {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(de)?;
        match raw.as_str() {
            "mesh" | "direct" | "tunnel" => Ok(AccessChannel::Mesh),
            other => Err(serde::de::Error::unknown_variant(other, &["mesh"])),
        }
    }
}

/// 组网配置的非敏感部分（spec 007 plan §4.1）。network_secret 属敏感凭证，
/// 经脚本写栈目录 `<stack>/easytier/network-secret`，永不进入本结构/设置
/// 文件/命令行/日志（AC8）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MeshConfig {
    /// 网络名（EasyTier network_name；成员以此 + network_secret 相认）
    pub network_name: String,
    /// 宿主机虚拟 IP（config.toml 顶层 ipv4，dhcp=false 静态持有）
    pub virtual_ip: String,
    /// 虚拟网段 CIDR（保存/切换时与物理网卡网段冲突检测的输入）
    pub virtual_cidr: String,
    /// 对端节点列表（默认社区节点，可编辑多条——公共节点无 SLA，冗余对冲）
    pub peers: Vec<String>,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            network_name: crate::consts::DEFAULT_MESH_NETWORK_NAME.to_string(),
            virtual_ip: crate::consts::DEFAULT_MESH_VIRTUAL_IP.to_string(),
            virtual_cidr: crate::consts::DEFAULT_MESH_VIRTUAL_CIDR.to_string(),
            peers: crate::consts::DEFAULT_MESH_PEERS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// 局域网边界守卫设置（spec 010 plan §4.1）：例外开关标记。
/// 标记是状态机输入，规则实况永远以 lan-guard status 探测为准（plan R8 判定
/// 口径）；`exception_since_ms` 由后端在派发成功后写入，前端不直写时间戳
/// （只提交开关意图）。旧版 settings.json 缺字段 → serde default → false/0。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LanGuardSettings {
    /// 例外开关是否开启（3001 放行标记；12h 回落由 lan_guard watcher 承载）
    pub exception_enabled: bool,
    /// 例外开启时点（epoch ms）；0 = 未开启（边界 `now - since == 12h` 即到期）
    pub exception_since_ms: u64,
}

impl Default for LanGuardSettings {
    fn default() -> Self {
        Self { exception_enabled: false, exception_since_ms: 0 }
    }
}

/// 全量设置（plan §4 schema；camelCase 序列化，未知字段忽略、缺失字段回默认，
/// 兼容旧版/新版文件）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// schema 版本（当前恒为 1，保存时强制写当前版本）
    pub version: u32,
    /// 界面语言：auto/zh/en
    pub language: LanguageSetting,
    /// 启动程序时自动拉起三组件（AC8）
    pub autostart_services: bool,
    /// 程序自身随系统登录自启（AC9）
    pub autostart_app: bool,
    /// 登录联动启动：程序被自启拉起时补齐服务（AC11/12）
    pub link_start_services: bool,
    /// 托盘「退出」的行为：保留服务 / 停止服务（AC13~15）
    pub exit_action: ExitAction,
    /// 启动后自动打开工作台页面
    pub open_page_on_start: bool,
    /// 脚本目录覆盖：None = 未设置（用内置/开发态路径）
    pub scripts_dir_override: Option<String>,
    /// 访问通道：唯一值 mesh（spec 008 收敛；旧文件缺字段或值为
    /// direct/tunnel 时同样得到 Mesh——迁移语义见 enum 注释）
    #[serde(default)]
    pub access_channel: AccessChannel,
    /// 组网配置（spec 007：非敏感部分，AC11 设置区编辑；secret 不在此——AC8）
    pub mesh: MeshConfig,
    /// 域名心跳检测（spec 005 AC7）：关闭则不发探测，既有标记冻结
    pub domain_heartbeat: bool,
    /// HTTPS 栈部署目录（spec 004：用户可配置；输入安装根自动追加
    /// `cloudcli-https` 子目录并规整；**重启工作台后生效**）
    pub stack_dir: String,
    /// 局域网边界守卫（spec 010：例外开关标记，plan §4.1）
    #[serde(default)]
    pub lan_guard: LanGuardSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            language: LanguageSetting::Auto,
            autostart_services: true,
            autostart_app: false,
            link_start_services: true,
            exit_action: ExitAction::Keep,
            open_page_on_start: false,
            scripts_dir_override: None,
            access_channel: AccessChannel::default(),
            mesh: MeshConfig::default(),
            domain_heartbeat: true,
            stack_dir: crate::consts::DEFAULT_STACK_DIR.to_string(),
            lan_guard: LanGuardSettings::default(),
        }
    }
}

/// 补丁（save_settings 命令入参，plan §5.1：补丁写）。
/// 语义：`None` = 不修改该字段；`Some(None)`（仅 scriptsDirOverride）= 显式置空。
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SettingsPatch {
    pub language: Option<LanguageSetting>,
    pub autostart_services: Option<bool>,
    pub autostart_app: Option<bool>,
    pub link_start_services: Option<bool>,
    pub exit_action: Option<ExitAction>,
    pub open_page_on_start: Option<bool>,
    /// 三态：None 不改 / Some(None) 置空 / Some(Some(dir)) 设置。
    /// 原生 serde 会把「显式 null」与「字段缺省」都塌缩为 None，
    /// 故用 deserialize_with 区分：缺省走 default（None），null → Some(None)。
    #[serde(default, deserialize_with = "deserialize_scripts_dir")]
    pub scripts_dir_override: Option<Option<String>>,
    /// 组网配置写入（spec 007 AC11 设置区编辑保存）
    pub mesh: Option<MeshConfig>,
    /// 域名心跳开关（spec 005 AC7）
    pub domain_heartbeat: Option<bool>,
    /// 栈目录（spec 004：用户输入安装根，保存时自动规整）
    pub stack_dir: Option<String>,
    /// 局域网边界守卫整块写入（spec 010：开关标记 + since 同块提交，mesh 同款）
    pub lan_guard: Option<LanGuardSettings>,
}

/// scriptsDirOverride 三态反序列化（仅字段出现时被调用）：
/// JSON null → Some(None)，字符串 → Some(Some(s))
fn deserialize_scripts_dir<'de, D>(de: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <Option<String>>::deserialize(de).map(Some)
}

/// 加载结果（核心层返回枚举供单测；修复事件由命令/装配层按 backup_path 发出）
#[derive(Debug)]
pub enum LoadOutcome {
    /// 文件不存在 → 默认值（AC23，不落盘）
    Missing(Settings),
    /// 正常解析
    Loaded(Settings),
    /// 文件损坏 → 已改名 `.bad-<时间戳>` 留档 + 回退默认（AC24）
    Repaired { settings: Settings, backup_path: PathBuf },
}

/// 实际设置文件路径（%APPDATA%\ai-remote-workbench\settings.json）
pub fn settings_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join(APP_DIR_NAME).join(SETTINGS_FILE)
}

/// 从指定路径加载设置（核心逻辑，路径注入便于单测）
pub fn load_from(path: &Path) -> LoadOutcome {
    let raw = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // AC23：文件（或目录）不存在 → 默认值，不落盘
            return LoadOutcome::Missing(Settings::default());
        }
        Err(e) => {
            // 其他读取异常（权限等）按损坏处理：留档 + 回退默认，不中断启动
            log::warn!("设置文件读取失败（{e}），按损坏处理（AC24）");
            return repair(path);
        }
    };
    match serde_json::from_str::<Settings>(&raw) {
        Ok(s) => LoadOutcome::Loaded(s),
        Err(e) => {
            // AC24：非法 JSON/结构 → 改名 .bad-<时间戳> 留档 + 回退默认
            log::warn!("设置文件解析失败（{e}）：留档并回退默认值（AC24）");
            repair(path)
        }
    }
}

/// 损坏恢复：改名 `.bad-<毫秒时间戳>` 留档（失败不阻断，仅日志），返回默认值
fn repair(path: &Path) -> LoadOutcome {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut backup: std::ffi::OsString =
        path.as_os_str().to_owned(); // settings.json → settings.json.bad-<ts>
    backup.push(format!(".bad-{ts}"));
    let backup_path = path.with_file_name(backup);
    if let Err(e) = fs::rename(path, &backup_path) {
        // 改名失败（占用/权限等）：留档尽力而为，恢复语义不变
        log::error!("损坏设置文件留档改名失败：{e}（{}）", backup_path.display());
    }
    LoadOutcome::Repaired { settings: Settings::default(), backup_path }
}

/// 补丁合并（纯函数）：未提交的字段保持原值，version 恒为当前 schema 版本
pub fn apply_patch(base: &Settings, patch: &SettingsPatch) -> Settings {
    let mut merged = base.clone();
    merged.version = SCHEMA_VERSION;
    if let Some(v) = patch.language {
        merged.language = v;
    }
    if let Some(v) = patch.autostart_services {
        merged.autostart_services = v;
    }
    if let Some(v) = patch.autostart_app {
        merged.autostart_app = v;
    }
    if let Some(v) = patch.link_start_services {
        merged.link_start_services = v;
    }
    if let Some(v) = patch.exit_action {
        merged.exit_action = v;
    }
    if let Some(v) = patch.open_page_on_start {
        merged.open_page_on_start = v;
    }
    if let Some(v) = patch.scripts_dir_override.clone() {
        merged.scripts_dir_override = v;
    }
    if let Some(v) = patch.mesh.clone() {
        merged.mesh = v;
    }
    if let Some(v) = patch.domain_heartbeat {
        merged.domain_heartbeat = v;
    }
    if let Some(v) = patch.stack_dir.as_deref() {
        merged.stack_dir = normalize_stack_dir(v);
    }
    if let Some(v) = patch.lan_guard {
        merged.lan_guard = v;
    }
    merged
}

/// 栈目录规整（纯函数）：去首尾空白与尾随分隔符；未以 `cloudcli-https`
/// 子目录结尾则自动追加（需求方：用户输入安装根，子目录名固定）；
/// 空输入回落默认值。
pub fn normalize_stack_dir(input: &str) -> String {
    let t = input.trim().trim_end_matches(['\\', '/']);
    if t.is_empty() {
        return crate::consts::DEFAULT_STACK_DIR.to_string();
    }
    let lower = t.to_ascii_lowercase();
    if lower.ends_with("\\cloudcli-https") || lower.ends_with("/cloudcli-https") {
        t.to_string()
    } else {
        format!("{t}\\cloudcli-https")
    }
}

/// 原子保存：写同目录临时文件后 rename 覆盖（半写防护）
pub fn save_to(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?; // 首次保存：目录可能尚不存在
    }
    let mut json = serde_json::to_string_pretty(settings)?;
    json.push('\n');
    let mut tmp: std::ffi::OsString = path.as_os_str().to_owned();
    tmp.push(format!(".tmp-{}", std::process::id()));
    let tmp_path = path.with_file_name(tmp);
    fs::write(&tmp_path, json)?;
    // std::fs::rename 在 Windows 走 MoveFileEx(REPLACE_EXISTING)，可原子覆盖
    fs::rename(&tmp_path, path)
}

// ── 应用态与 Tauri 命令（薄封装）────────────────────────────────────────────

/// 内存态设置 + 落盘路径（app.manage 注册；命令读写均经此，避免多源）
pub struct SettingsState {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsState {
    /// 启动时加载（AC23/24 语义在核心函数内）；返回 (状态, 损坏留档路径)
    pub fn load_at(path: PathBuf) -> (Self, Option<PathBuf>) {
        match load_from(&path) {
            LoadOutcome::Missing(s) | LoadOutcome::Loaded(s) => {
                (Self { path, current: Mutex::new(s) }, None)
            }
            LoadOutcome::Repaired { settings, backup_path } => {
                (Self { path, current: Mutex::new(settings) }, Some(backup_path))
            }
        }
    }

    /// 当前生效设置（克隆返回，命令层序列化给前端）
    pub fn current(&self) -> Settings {
        self.current.lock().expect("设置锁中毒").clone()
    }

    /// 补丁保存：合并 → 原子写盘 → 更新内存
    pub fn patch(&self, patch: &SettingsPatch) -> Result<Settings, String> {
        let mut cur = self.current.lock().expect("设置锁中毒");
        let merged = apply_patch(&cur, patch);
        save_to(&self.path, &merged).map_err(|e| format!("保存设置失败：{e}"))?;
        *cur = merged.clone();
        Ok(merged)
    }
}

/// 全量读（plan §5.1）
#[tauri::command]
pub fn get_settings(state: tauri::State<'_, SettingsState>) -> Settings {
    state.current()
}

/// 补丁写（plan §5.1）：返回合并后的全量设置
#[tauri::command]
pub fn save_settings(
    state: tauri::State<'_, SettingsState>,
    patch: SettingsPatch,
) -> Result<Settings, String> {
    state.patch(&patch)
}

// ── 单元测试 ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// 每个测试独享的临时目录（同名目录先清空），返回其中的 settings.json 路径
    fn temp_settings_path(tag: &str) -> PathBuf {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "wb-settings-test-{}-{tag}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("创建临时目录失败");
        dir.join(SETTINGS_FILE)
    }

    /// 测试后清理（失败不影响断言结果）
    fn cleanup(path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn defaults_match_plan_schema() {
        // plan §4：version=1 / language=auto / autostartServices=true /
        // autostartApp=false / linkStartServices=true / exitAction=keep /
        // openPageOnStart=false / scriptsDirOverride=null
        // spec 007 §4.1：MeshConfig 全默认；spec 008：单通道 mesh
        let d = Settings::default();
        assert_eq!(d.version, 1);
        assert_eq!(d.language, LanguageSetting::Auto);
        assert!(d.autostart_services);
        assert!(!d.autostart_app);
        assert!(d.link_start_services);
        assert_eq!(d.exit_action, ExitAction::Keep);
        assert!(!d.open_page_on_start);
        assert_eq!(d.scripts_dir_override, None);
        assert_eq!(d.access_channel, AccessChannel::Mesh, "唯一通道 mesh（008 收敛）");
        assert_eq!(d.mesh, MeshConfig::default());
        assert_eq!(d.mesh.network_name, crate::consts::DEFAULT_MESH_NETWORK_NAME);
        assert_eq!(d.mesh.virtual_ip, crate::consts::DEFAULT_MESH_VIRTUAL_IP);
        assert_eq!(
            d.mesh.peers,
            vec!["tcp://sh.vomiku.com:7910".to_string()],
            "默认对端=社区节点（T2 实测选定）"
        );
        assert!(d.domain_heartbeat, "心跳默认开（spec 005 AC1）");
        assert_eq!(d.stack_dir, crate::consts::DEFAULT_STACK_DIR);
        // spec 010：例外开关标记默认关（AC5 默认收口态）
        assert_eq!(d.lan_guard, LanGuardSettings { exception_enabled: false, exception_since_ms: 0 });
    }

    /// spec 010 plan §4.1：例外标记 serde default（旧文件缺字段 → false/0，其余
    /// 字段不受影响）+ roundtrip + camelCase 键名
    #[test]
    fn lan_guard_settings_default_roundtrip_and_compat() {
        // 旧版文件无 lanGuard 字段：回默认 false/0，其余字段保留（向后兼容）
        let path = temp_settings_path("languard-legacy");
        fs::write(
            &path,
            r#"{"version":1,"language":"zh","mesh":{"networkName":"kept","virtualIp":"10.9.9.1","virtualCidr":"10.9.9.0/24","peers":["tcp://k:1"]}}"#,
        )
        .expect("写入失败");
        match load_from(&path) {
            LoadOutcome::Loaded(s) => {
                assert_eq!(s.lan_guard, LanGuardSettings::default(), "缺字段 → false/0");
                assert_eq!(s.mesh.network_name, "kept", "其余字段不受影响");
            }
            other => panic!("应为 Loaded，实际 {other:?}"),
        }
        cleanup(&path);

        // roundtrip：开启态（enabled + since）落盘重载保持
        let path = temp_settings_path("languard-roundtrip");
        let mut s = Settings::default();
        s.lan_guard = LanGuardSettings { exception_enabled: true, exception_since_ms: 1_728_000_000_000 };
        save_to(&path, &s).expect("保存失败");
        match load_from(&path) {
            LoadOutcome::Loaded(loaded) => assert_eq!(loaded.lan_guard, s.lan_guard, "roundtrip 保持"),
            other => panic!("应为 Loaded，实际 {other:?}"),
        }
        // camelCase 键名（前端 Settings.lanGuard 对齐）
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            json.contains(r#""lanGuard":{"exceptionEnabled":true,"exceptionSinceMs":1728000000000}"#),
            "{json}"
        );
        cleanup(&path);
    }

    /// spec 010：SettingsPatch.lan_guard 整块写入（mesh 同款）；空补丁不动既有值
    #[test]
    fn apply_patch_lan_guard_block_write() {
        let base = Settings::default();
        // 空补丁：原样
        let untouched = apply_patch(&base, &SettingsPatch::default());
        assert_eq!(untouched.lan_guard, LanGuardSettings::default());

        // 整块写入：on 态
        let on = LanGuardSettings { exception_enabled: true, exception_since_ms: 42 };
        let merged = apply_patch(
            &base,
            &SettingsPatch { lan_guard: Some(on), ..Default::default() },
        );
        assert_eq!(merged.lan_guard, on, "例外标记整块写入");
        assert_eq!(merged.mesh, base.mesh, "未提交字段不变");

        // 整块写回：off 态（清标记同形态，since 一并归 0）
        let off = LanGuardSettings { exception_enabled: false, exception_since_ms: 0 };
        let merged = apply_patch(
            &merged,
            &SettingsPatch { lan_guard: Some(off), ..Default::default() },
        );
        assert_eq!(merged.lan_guard, off);

        // JSON 侧：camelCase 对象可解析
        let patch: SettingsPatch = serde_json::from_str(
            r#"{"lanGuard":{"exceptionEnabled":true,"exceptionSinceMs":123}}"#,
        )
        .expect("解析失败");
        assert_eq!(
            patch.lan_guard,
            Some(LanGuardSettings { exception_enabled: true, exception_since_ms: 123 })
        );
    }

    #[test]
    fn stack_dir_normalization() {
        // 需求方 2026-09-10：输入安装根，自动追加 cloudcli-https 子目录
        assert_eq!(normalize_stack_dir("D:\\Software\\"), "D:\\Software\\cloudcli-https");
        assert_eq!(normalize_stack_dir(" D:\\Software "), "D:\\Software\\cloudcli-https");
        // 完整路径原样（大小写/尾斜杠容忍）
        assert_eq!(
            normalize_stack_dir("d:\\software\\CloudCLI-HTTPS\\"),
            "d:\\software\\CloudCLI-HTTPS"
        );
        assert_eq!(normalize_stack_dir("E:\\MyStack"), "E:\\MyStack\\cloudcli-https");
        // 空输入回落默认
        assert_eq!(normalize_stack_dir(""), crate::consts::DEFAULT_STACK_DIR);
        assert_eq!(normalize_stack_dir("   "), crate::consts::DEFAULT_STACK_DIR);
    }

    #[test]
    fn serde_camel_case_and_enum_strings() {
        // 字段名 camelCase、枚举小写字符串（与前端 i18n 约定一致）
        let json = serde_json::to_string(&Settings::default()).expect("序列化失败");
        for key in [
            "\"version\":1",
            "\"language\":\"auto\"",
            "\"autostartServices\":true",
            "\"autostartApp\":false",
            "\"linkStartServices\":true",
            "\"exitAction\":\"keep\"",
            "\"openPageOnStart\":false",
            "\"scriptsDirOverride\":null",
            "\"accessChannel\":\"mesh\"",
            "\"mesh\":{",
            "\"networkName\":\"ai-remote\"",
            "\"virtualIp\":\"10.126.126.1\"",
            "\"virtualCidr\":\"10.126.126.0/24\"",
            "\"peers\":[\"tcp://sh.vomiku.com:7910\"]",
        ] {
            assert!(json.contains(key), "序列化结果缺 {key}：{json}");
        }
        // spec 008：旧通道字段已彻底移除，序列化不得再现
        for gone in ["\"tunnel\"", "\"tunnelEnabled\"", "\"tunnelDisabled\"", "\"directDisabled\""] {
            assert!(!json.contains(gone), "旧通道字段 {gone} 不应再出现：{json}");
        }
        // 回读一致
        let back: Settings = serde_json::from_str(&json).expect("反序列化失败");
        assert_eq!(back, Settings::default());
        // zh / stop 字符串
        let mut s = Settings::default();
        s.language = LanguageSetting::Zh;
        s.exit_action = ExitAction::Stop;
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"language\":\"zh\"") && j.contains("\"exitAction\":\"stop\""), "{j}");
    }

    #[test]
    fn load_missing_file_returns_defaults() {
        // AC23：文件/目录不存在 → 默认值，不报错不落盘
        let path = temp_settings_path("missing");
        match load_from(&path) {
            LoadOutcome::Missing(s) => assert_eq!(s, Settings::default()),
            other => panic!("应为 Missing，实际 {other:?}"),
        }
        assert!(!path.exists(), "缺失分支不应落盘");
        cleanup(&path);
    }

    #[test]
    fn load_existing_file_round_trip() {
        // AC21：保存后重启加载，值保持
        let path = temp_settings_path("roundtrip");
        let mut s = Settings::default();
        s.language = LanguageSetting::En;
        s.exit_action = ExitAction::Stop;
        s.autostart_app = true;
        s.scripts_dir_override = Some("D:\\my-scripts".into());
        s.mesh = MeshConfig {
            network_name: "office".into(),
            virtual_ip: "10.200.0.1".into(),
            virtual_cidr: "10.200.0.0/24".into(),
            peers: vec!["tcp://p.example.com:11010".into()],
        };
        save_to(&path, &s).expect("保存失败");
        match load_from(&path) {
            LoadOutcome::Loaded(loaded) => assert_eq!(loaded, s),
            other => panic!("应为 Loaded，实际 {other:?}"),
        }
        cleanup(&path);
    }

    #[test]
    fn load_partial_and_unknown_fields_fall_back_to_defaults() {
        // 前向/后向兼容：缺字段回默认、未知字段忽略
        let path = temp_settings_path("partial");
        fs::write(&path, "{\"language\":\"zh\",\"futureField\":true}").expect("写入失败");
        match load_from(&path) {
            LoadOutcome::Loaded(s) => {
                assert_eq!(s.language, LanguageSetting::Zh);
                assert_eq!(s.exit_action, ExitAction::Keep, "缺失字段应回默认");
                assert!(s.autostart_services);
            }
            other => panic!("应为 Loaded，实际 {other:?}"),
        }
        cleanup(&path);
    }

    #[test]
    fn corrupt_file_repaired_with_bad_backup() {
        // AC24：非法 JSON → 回退默认 + 原文件改名 .bad-<时间戳> 留档
        let path = temp_settings_path("corrupt");
        let garbage = "{ 这不是合法 JSON";
        fs::write(&path, garbage).expect("写入失败");
        let outcome = load_from(&path);
        let backup = match &outcome {
            LoadOutcome::Repaired { settings, backup_path } => {
                assert_eq!(*settings, Settings::default(), "损坏后应回默认值");
                backup_path.clone()
            }
            other => panic!("应为 Repaired，实际 {other:?}"),
        };
        assert!(!path.exists(), "原文件应已被改名");
        assert!(backup.is_file(), "留档文件应存在：{}", backup.display());
        assert_eq!(fs::read_to_string(&backup).unwrap(), garbage, "留档内容应与原文件一致");
        let name = backup.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("settings.json.bad-") && name.len() > "settings.json.bad-".len(),
            "留档名应为 settings.json.bad-<时间戳>，实际 {name}"
        );
        // 二次加载：留档不再参与，等价于文件缺失 → 默认
        match load_from(&path) {
            LoadOutcome::Missing(s) => assert_eq!(s, Settings::default()),
            other => panic!("修复后二次加载应为 Missing，实际 {other:?}"),
        }
        cleanup(&path);
    }

    #[test]
    fn apply_patch_only_touched_fields() {
        let mut base = Settings::default();
        base.language = LanguageSetting::Zh;
        base.autostart_app = true;
        base.scripts_dir_override = Some("D:\\old".into());

        // 空补丁：原样返回（仅 version 规整）；base 默认通道随 007 改为 mesh
        let untouched = apply_patch(&base, &SettingsPatch::default());
        assert_eq!(untouched, base);
        assert_eq!(untouched.access_channel, AccessChannel::Mesh);

        // 部分补丁：只改提交字段
        let patch = SettingsPatch {
            exit_action: Some(ExitAction::Stop),
            scripts_dir_override: Some(Some("D:\\new".into())),
            ..Default::default()
        };
        let merged = apply_patch(&base, &patch);
        assert_eq!(merged.exit_action, ExitAction::Stop);
        assert_eq!(merged.scripts_dir_override.as_deref(), Some("D:\\new"));
        assert_eq!(merged.language, LanguageSetting::Zh, "未提交字段不变");
        assert!(merged.autostart_app, "未提交字段不变");
    }

    #[test]
    fn apply_patch_scripts_dir_tri_state() {
        // None 不改 / Some(None) 显式置空 / Some(Some) 设置
        let mut base = Settings::default();
        base.scripts_dir_override = Some("D:\\old".into());

        let keep = apply_patch(&base, &SettingsPatch::default());
        assert_eq!(keep.scripts_dir_override.as_deref(), Some("D:\\old"));

        let cleared = apply_patch(
            &base,
            &SettingsPatch { scripts_dir_override: Some(None), ..Default::default() },
        );
        assert_eq!(cleared.scripts_dir_override, None, "Some(None) 应显式置空");

        // JSON 侧语义验证：null → 置空；缺字段 → 不改
        let patch: SettingsPatch =
            serde_json::from_str("{\"scriptsDirOverride\":null}").expect("解析失败");
        assert_eq!(patch.scripts_dir_override, Some(None));
        let patch: SettingsPatch = serde_json::from_str("{}").expect("解析失败");
        assert_eq!(patch.scripts_dir_override, None);
    }

    #[test]
    fn apply_patch_stack_dir_normalizes_input() {
        // spec 004：栈目录补丁合并 + 规整；空补丁不动其余字段
        let base = Settings::default();
        let untouched = apply_patch(&base, &SettingsPatch::default());
        assert_eq!(untouched, base);

        let patch = SettingsPatch {
            stack_dir: Some("D:\\Software\\".into()),
            ..Default::default()
        };
        let merged = apply_patch(&base, &patch);
        // 栈目录输入安装根 → 规整追加子目录（需求方 2026-09-10）
        assert_eq!(merged.stack_dir, "D:\\Software\\cloudcli-https");
        assert_eq!(merged.language, base.language, "未提交字段不变");
    }

    #[test]
    fn old_settings_file_without_channel_fields_loads_as_mesh() {
        // 向后兼容（spec 008 D1）：004 之前的 settings.json（无 accessChannel 字段）
        // → serde default → Mesh，其余字段原样
        let path = temp_settings_path("legacy");
        fs::write(
            &path,
            r#"{"version":1,"language":"zh","autostartServices":true,"autostartApp":false,"linkStartServices":true,"exitAction":"keep","openPageOnStart":false,"scriptsDirOverride":null}"#,
        )
        .expect("写入失败");
        match load_from(&path) {
            LoadOutcome::Loaded(s) => {
                assert_eq!(s.access_channel, AccessChannel::Mesh, "缺字段回默认=mesh");
                assert_eq!(s.language, LanguageSetting::Zh, "其余字段保留");
                assert_eq!(s.mesh, MeshConfig::default(), "无 mesh 字段回默认");
            }
            other => panic!("应为 Loaded，实际 {other:?}"),
        }
        cleanup(&path);
    }

    /// spec 008 AC2 迁移矩阵：旧值加载映射 + 字段保留 + 异常值走损坏修复
    fn write_legacy(raw: &str, tag: &str) -> PathBuf {
        let path = temp_settings_path(tag);
        fs::write(&path, raw).expect("写入失败");
        path
    }

    #[test]
    fn migration_direct_and_tunnel_values_map_to_mesh() {
        // 五态之一/二：direct、tunnel → Mesh，其余字段不受影响
        for old in ["direct", "tunnel"] {
            let path = write_legacy(
                &format!(
                    r#"{{"version":1,"accessChannel":"{old}","language":"en","tunnel":{{"tunnelId":"29080263","nodeDomain":"frp-can.com"}},"tunnelEnabled":true,"tunnelDisabled":false,"directDisabled":false,"mesh":{{"networkName":"kept","virtualIp":"10.9.9.1","virtualCidr":"10.9.9.0/24","peers":["tcp://keep:1"]}},"stackDir":"D:\\Software\\cloudcli-https"}}"#
                ),
                &format!("mig-{old}"),
            );
            match load_from(&path) {
                LoadOutcome::Loaded(s) => {
                    assert_eq!(s.access_channel, AccessChannel::Mesh, "{old} 应迁移为 mesh");
                    assert_eq!(s.language, LanguageSetting::En, "{old}:其余字段保留");
                    assert_eq!(s.stack_dir, "D:\\Software\\cloudcli-https");
                    // 旧 tunnel 字段被 serde 忽略未知字段语义丢弃（无对应结构可断言，
                    // 迁移不触发损坏修复即为其消失的证据）
                    assert_eq!(s.mesh.network_name, "kept", "{old}:组网配置保留");
                }
                other => panic!("{old} 应为 Loaded，实际 {other:?}"),
            }
            // 迁移后保存：被删字段从磁盘消失（AC2「下次保存时自然消失」）
            match load_from(&path) {
                LoadOutcome::Loaded(s) => {
                    save_to(&path, &s).expect("保存失败");
                    let on_disk = fs::read_to_string(&path).unwrap();
                    assert!(!on_disk.contains("tunnelId"), "保存后穿透字段应消失");
                    // pretty JSON 冒号后带空格，解析断言与格式无关
                    let saved: serde_json::Value =
                        serde_json::from_str(&on_disk).expect("保存文件应为合法 JSON");
                    assert_eq!(saved["accessChannel"], "mesh");
                }
                _ => unreachable!(),
            }
            cleanup(&path);
        }
    }

    #[test]
    fn migration_mesh_value_loads_unchanged() {
        // 五态之三：mesh 原样
        let path = write_legacy(
            r#"{"version":1,"accessChannel":"mesh","stackDir":"E:\\Stack\\cloudcli-https"}"#,
            "mig-mesh",
        );
        match load_from(&path) {
            LoadOutcome::Loaded(s) => {
                assert_eq!(s.access_channel, AccessChannel::Mesh);
                assert_eq!(s.stack_dir, "E:\\Stack\\cloudcli-https");
            }
            other => panic!("应为 Loaded，实际 {other:?}"),
        }
        cleanup(&path);
    }

    #[test]
    fn migration_unknown_channel_value_triggers_repair() {
        // 五态之四：异常值（如 "frp"）→ 反序列化报错 → 既有损坏修复（AC24 口径）
        let path = write_legacy(
            r#"{"version":1,"accessChannel":"frp"}"#,
            "mig-bad",
        );
        match load_from(&path) {
            LoadOutcome::Repaired { settings, backup_path } => {
                assert_eq!(settings, Settings::default(), "回默认值");
                assert!(backup_path.is_file(), "留档存在");
            }
            other => panic!("应为 Repaired，实际 {other:?}"),
        }
        cleanup(&path);
    }

    #[test]
    fn apply_patch_mesh_config() {
        // spec 007 AC11：组网配置编辑的补丁合并
        let base = Settings::default();
        let custom = MeshConfig {
            network_name: "office".into(),
            virtual_ip: "10.200.0.1".into(),
            virtual_cidr: "10.200.0.0/24".into(),
            peers: vec!["tcp://a.example.com:11010".into(), "udp://b.example.com:11011".into()],
        };
        let patch = SettingsPatch {
            mesh: Some(custom.clone()),
            ..Default::default()
        };
        let merged = apply_patch(&base, &patch);
        assert_eq!(merged.mesh, custom, "组网配置整块写入");

        // 空补丁不动既有值
        let again = apply_patch(&merged, &SettingsPatch::default());
        assert_eq!(again.mesh, custom);

        // JSON 侧：mesh 对象 camelCase 可解析
        let patch: SettingsPatch = serde_json::from_str(
            r#"{"mesh":{"networkName":"n","virtualIp":"10.0.0.9","virtualCidr":"10.0.0.0/24","peers":["tcp://x:1"]}}"#,
        )
        .expect("解析失败");
        assert_eq!(patch.mesh.as_ref().unwrap().virtual_ip, "10.0.0.9");
    }

    #[test]
    fn save_is_atomic_and_leaves_no_temp_files() {
        // 原子写：完成后目录里只有 settings.json（无 .tmp 残留）
        let path = temp_settings_path("atomic");
        save_to(&path, &Settings::default()).expect("保存失败");
        save_to(&path, &Settings::default()).expect("覆盖保存失败");
        let dir = path.parent().unwrap();
        let names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![SETTINGS_FILE.to_string()], "目录应只剩 {SETTINGS_FILE}：{names:?}");
        cleanup(&path);
    }

    #[test]
    fn save_creates_missing_parent_dir() {
        // 首次保存：目录不存在也能落盘
        let dir = std::env::temp_dir().join(format!("wb-settings-test-{}-mkdir", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join(SETTINGS_FILE);
        save_to(&path, &Settings::default()).expect("目录缺失时保存失败");
        assert!(path.is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn state_patch_persists_and_updates_memory() {
        // SettingsState：补丁保存后落盘、内存一致；进程内二次补丁基于新值
        let path = temp_settings_path("state");
        let (state, repair) = SettingsState::load_at(path.clone());
        assert_eq!(repair, None);
        assert_eq!(state.current(), Settings::default());

        let patch = SettingsPatch {
            language: Some(LanguageSetting::En),
            ..Default::default()
        };
        let merged = state.patch(&patch).expect("补丁保存失败");
        assert_eq!(merged.language, LanguageSetting::En);
        assert_eq!(state.current().language, LanguageSetting::En, "内存应同步");

        // 落盘可验证（模拟重启）
        let (state2, _) = SettingsState::load_at(path.clone());
        assert_eq!(state2.current().language, LanguageSetting::En, "重启后保持（AC21）");

        let patch2 = SettingsPatch { autostart_app: Some(true), ..Default::default() };
        let merged2 = state2.patch(&patch2).expect("二次补丁失败");
        assert_eq!(merged2.language, LanguageSetting::En, "二次补丁不应丢已有值");
        assert!(merged2.autostart_app);
        cleanup(&path);
    }

    #[test]
    fn state_load_reports_corrupt_repair() {
        let path = temp_settings_path("state-corrupt");
        fs::write(&path, "]]不是 JSON[[[").expect("写入失败");
        let (state, repair) = SettingsState::load_at(path.clone());
        assert!(repair.is_some(), "损坏时应报告留档路径");
        assert_eq!(state.current(), Settings::default(), "状态应为默认值");
        cleanup(&path);
    }

    #[test]
    fn settings_path_under_appdata() {
        // 实际路径：...%APPDATA%\ai-remote-workbench\settings.json（spec §4.1）
        let p = settings_path();
        assert_eq!(p.file_name().unwrap(), SETTINGS_FILE);
        let comps: Vec<_> = p.components().collect();
        let dir_names: Vec<String> = comps
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        assert!(
            dir_names.iter().any(|n| n == APP_DIR_NAME),
            "路径应含 {APP_DIR_NAME}：{dir_names:?}"
        );
    }
}
