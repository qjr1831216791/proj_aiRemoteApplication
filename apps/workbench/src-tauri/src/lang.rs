//! 语言判定与 Rust 侧原生文案（T2：托盘菜单；T14：切换立即生效链路）。
//!
//! AC25：默认跟随系统**显示语言**——zh → 中文、否则英文；与前端 `src/i18n/`
//! 的 `detectLang()`（navigator.language）同语义。判定核心为纯函数，单测覆盖。
//! 词条同源约定：托盘 [`TrayTexts`] 与前端 `src/i18n/{zh,en}.ts` 的
//! `common.*`/`tray.*` 键位语义一致，两边同步维护；
//! [`DetailTexts`] 供编排器状态卡 detail 双语化（zh 与既有文案逐字一致以稳测试）。

/// 生效语言
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// 中文
    Zh,
    /// 英文
    En,
}

/// PRIMARYLANGID 掩码（winnt.h 同名宏）
const PRIMARYLANGID_MASK: u32 = 0x3FF;
/// winnt.h LANG_CHINESE
const LANG_CHINESE: u32 = 0x04;

/// 纯函数：LANGID → 生效语言（主语言为中文即 Zh，子语言/区域无关）。
/// 等价 `Get-UICulture` 的主语言判定（zh-CN / zh-TW / zh-HK 均 → Zh）。
pub fn resolve_lang_from_langid(langid: u32) -> Lang {
    if (langid & PRIMARYLANGID_MASK) == LANG_CHINESE {
        Lang::Zh
    } else {
        Lang::En
    }
}

/// 系统显示语言探测（AC25 的 auto 分支）。
#[cfg(windows)]
pub fn detect_system_lang() -> Lang {
    use windows::Win32::Globalization::GetUserDefaultUILanguage;
    // 返回 LANGID（u16 语义），PRIMARYLANGID 判定见纯函数
    resolve_lang_from_langid(unsafe { GetUserDefaultUILanguage() } as u32)
}

/// 非 Windows 平台兜底（本项目仅面向 Windows，此分支仅为可编译性）
#[cfg(not(windows))]
pub fn detect_system_lang() -> Lang {
    Lang::En
}

/// 设置语言 → 生效语言（AC25：显式选择优先于系统；auto = 系统显示语言）。
/// 装配层在启动时解析一次（T14 切换语言时重新解析并重建托盘）。
pub fn resolve_setting(setting: crate::settings::LanguageSetting) -> Lang {
    use crate::settings::LanguageSetting;
    match setting {
        LanguageSetting::Auto => detect_system_lang(),
        LanguageSetting::Zh => Lang::Zh,
        LanguageSetting::En => Lang::En,
    }
}

/// 托盘菜单文案（spec §4.2 六项 + tooltip）。
/// zh/en 两套与 `src/i18n/` 词条风格对齐（`common.start` 等键位同源），
/// T2 阶段为独立 const 模块，T14 统一词条源时替换数据来源、接口不变。
pub struct TrayTexts {
    pub tooltip: &'static str,
    pub start: &'static str,
    pub stop: &'static str,
    pub open_workbench: &'static str,
    pub show_main: &'static str,
    pub stop_and_exit: &'static str,
    pub quit: &'static str,
}

/// 按生效语言取托盘文案
pub fn tray_texts(lang: Lang) -> TrayTexts {
    match lang {
        Lang::Zh => TrayTexts {
            tooltip: "AI 远程工作台",
            start: "启动",
            stop: "停止",
            open_workbench: "打开工作台",
            show_main: "显示主界面",
            stop_and_exit: "停止服务并退出",
            quit: "退出",
        },
        Lang::En => TrayTexts {
            tooltip: "AI Remote Workbench",
            start: "Start",
            stop: "Stop",
            open_workbench: "Open Workbench",
            show_main: "Show Main Window",
            // 不用 "&"：避免被菜单层解析为助记符
            stop_and_exit: "Stop Services and Exit",
            quit: "Exit",
        },
    }
}

// ── 生效语言共享态（T14：切换立即生效）──────────────────────────────────────

/// 进程内共享的生效语言：托盘重建、脚本 `-Lang` 派发、状态卡 detail 实时读取。
/// `set_language` 命令为唯一写者（settings 落盘成功后更新）。
pub struct LanguageState {
    current: std::sync::Mutex<Lang>,
}

impl LanguageState {
    pub fn new(lang: Lang) -> Self {
        Self { current: std::sync::Mutex::new(lang) }
    }

    pub fn current(&self) -> Lang {
        *self.current.lock().expect("语言态锁中毒")
    }

    fn set(&self, lang: Lang) {
        *self.current.lock().expect("语言态锁中毒") = lang;
    }
}

/// 生效链路核心（纯逻辑，单测覆盖）：解析生效语言并更新共享态。
/// AC25：显式选择优先于系统（`resolve_setting` 的语义）。
pub fn apply_language(state: &LanguageState, setting: crate::settings::LanguageSetting) -> Lang {
    let effective = resolve_setting(setting);
    state.set(effective);
    effective
}

/// 切换界面语言（T14）：设置落盘 → 共享态更新 → 托盘菜单重建（AC25 立即生效）。
/// 脚本 `-Lang` 经共享态实时对齐（编排器 lang source），无需重启程序。
#[tauri::command]
pub fn set_language(
    app: tauri::AppHandle,
    settings: tauri::State<'_, crate::settings::SettingsState>,
    lang_state: tauri::State<'_, LanguageState>,
    setting: crate::settings::LanguageSetting,
) -> Result<crate::settings::Settings, String> {
    // 先落盘：失败（IO 异常）则一切不变，避免半切换状态
    let patched = settings.patch(&crate::settings::SettingsPatch {
        language: Some(setting),
        ..Default::default()
    })?;
    let effective = apply_language(&lang_state, setting);
    crate::tray::rebuild(&app, effective)
        .map_err(|e| format!("托盘菜单重建失败（语言已切换，重启程序可恢复）：{e}"))?;
    log::info!(
        "语言切换：{setting:?} → 生效 {effective:?}（托盘已重建，脚本 -Lang 实时对齐）"
    );
    Ok(patched)
}

// ── 状态卡动态文案（orchestrator detail 的双语源）────────────────────────────

/// 编排器 detail 文案生成器（zh 与既有文案逐字一致——历史测试断言子串依赖）。
pub struct DetailTexts {
    lang: Lang,
}

/// 按生效语言取 detail 文案
pub fn detail_texts(lang: Lang) -> DetailTexts {
    DetailTexts { lang }
}

impl DetailTexts {
    /// 端口被无关进程占用（AC7 展示口径）
    pub fn port_held(&self, port: u16, process: &str) -> String {
        match self.lang {
            Lang::Zh => format!("端口 {port} 被进程 {process} 占用"),
            Lang::En => format!("Port {port} is held by process {process}"),
        }
    }

    /// 守卫判定被占、未拉起（AC7 + 不重复派发）
    pub fn port_held_not_started(&self, port: u16, process: &str) -> String {
        match self.lang {
            Lang::Zh => format!("端口 {port} 被进程 {process} 占用，未拉起"),
            Lang::En => format!("Port {port} is held by process {process}; not started"),
        }
    }

    /// 脚本目录不可用（spec §4.5 禁用原因；run_tool 命令共用）
    pub fn scripts_dir_unavailable(&self) -> String {
        match self.lang {
            Lang::Zh => "sprint0 脚本目录不可用（spec §4.5）：CloudCLI 启动已禁用，请检查脚本目录设置".into(),
            Lang::En => "sprint0 scripts directory unavailable (spec §4.5): CloudCLI start disabled; check the scripts directory setting".into(),
        }
    }

    /// run-server-hidden.ps1 exit 1：cloudcli 未安装/不可用（AC1）
    pub fn cloudcli_unavailable(&self) -> String {
        match self.lang {
            Lang::Zh => "cloudcli 不可用（未安装或不在 PATH）".into(),
            Lang::En => "cloudcli unavailable (not installed or not on PATH)".into(),
        }
    }

    pub fn start_failed(&self, reason: &str) -> String {
        match self.lang {
            Lang::Zh => format!("启动失败：{reason}"),
            Lang::En => format!("Start failed: {reason}"),
        }
    }

    pub fn script_failed(&self, code: i32) -> String {
        match self.lang {
            Lang::Zh => format!("启动脚本异常退出（code={code}），详情见程序日志目录"),
            Lang::En => format!("Start script exited unexpectedly (code={code}); see the app log directory"),
        }
    }

    pub fn script_timeout(&self) -> String {
        match self.lang {
            Lang::Zh => "启动脚本超时未返回（run-server-hidden.ps1），详情见程序日志".into(),
            Lang::En => "Start script timed out without returning (run-server-hidden.ps1); see the app log".into(),
        }
    }

    pub fn spawn_failed(&self, err: &str) -> String {
        match self.lang {
            Lang::Zh => format!("启动脚本无法执行：{err}"),
            Lang::En => format!("Cannot run the start script: {err}"),
        }
    }

    /// 原生拉起（caddy/ddns-go）失败
    pub fn dispatch_failed(&self, err: &str) -> String {
        match self.lang {
            Lang::Zh => format!("拉起失败：{err}"),
            Lang::En => format!("Failed to launch: {err}"),
        }
    }

    /// 启动就绪超时（AC1：60s 未就绪）
    pub fn start_timeout(&self, secs: u64) -> String {
        match self.lang {
            Lang::Zh => format!("启动超时（{secs}s 未就绪）"),
            Lang::En => format!("Start timed out (not ready in {secs}s)"),
        }
    }

    /// 失败详情附带的日志尾部标题行
    pub fn log_tail_header(&self, path: &str) -> String {
        match self.lang {
            Lang::Zh => format!("—— {path} 尾部 ——"),
            Lang::En => format!("---- tail of {path} ----"),
        }
    }

    /// 日志不可读时的替代提示
    pub fn logs_unreadable(&self, paths: &str) -> String {
        match self.lang {
            Lang::Zh => format!("日志暂不可读：{paths}"),
            Lang::En => format!("Logs not readable: {paths}"),
        }
    }
}

/// ShellExecuteW 失败码 → 用户可读文案（5 = SE_ERR_ACCESSDENIED：UAC 被拒，AC20）
pub fn shell_error_text(code: isize, lang: Lang) -> String {
    match (code, lang) {
        (5, Lang::Zh) => "用户拒绝了 UAC 授权（操作未执行）".into(),
        (5, Lang::En) => "The UAC prompt was declined (nothing was executed)".into(),
        (_, Lang::Zh) => format!("启动外部程序失败（ShellExecuteW 返回 {code}）"),
        (_, Lang::En) => format!("Failed to launch external program (ShellExecuteW returned {code})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::LanguageSetting;

    #[test]
    fn chinese_primary_lang_resolves_zh() {
        assert_eq!(resolve_lang_from_langid(0x0804), Lang::Zh); // zh-CN
        assert_eq!(resolve_lang_from_langid(0x0404), Lang::Zh); // zh-TW
        assert_eq!(resolve_lang_from_langid(0x0C04), Lang::Zh); // zh-HK
        assert_eq!(resolve_lang_from_langid(0x7C04), Lang::Zh); // zh-Hant
    }

    #[test]
    fn non_chinese_resolves_en() {
        assert_eq!(resolve_lang_from_langid(0x0409), Lang::En); // en-US
        assert_eq!(resolve_lang_from_langid(0x0809), Lang::En); // en-GB
        assert_eq!(resolve_lang_from_langid(0x0011), Lang::En); // ja（中性）
        assert_eq!(resolve_lang_from_langid(0x0411), Lang::En); // ja-JP
        assert_eq!(resolve_lang_from_langid(0), Lang::En); // 未知/中性 → 英文
    }

    #[test]
    fn resolve_setting_prefers_explicit_choice() {
        // AC25：显式选择优先于系统语言；auto 落回系统探测（本机中文环境 → Zh）
        use crate::settings::LanguageSetting;
        assert_eq!(resolve_setting(LanguageSetting::Zh), Lang::Zh);
        assert_eq!(resolve_setting(LanguageSetting::En), Lang::En);
        assert_eq!(resolve_setting(LanguageSetting::Auto), detect_system_lang());
    }

    #[test]
    fn tray_texts_complete_for_both_langs() {
        for lang in [Lang::Zh, Lang::En] {
            let t = tray_texts(lang);
            for s in [
                t.tooltip,
                t.start,
                t.stop,
                t.open_workbench,
                t.show_main,
                t.stop_and_exit,
                t.quit,
            ] {
                assert!(!s.trim().is_empty(), "{lang:?} 存在空文案");
            }
        }
    }

    // ── T14：生效链路（显式选择优先于系统，AC25）──────────────────────

    #[test]
    fn apply_language_updates_shared_state_explicit_first() {
        // 显式 zh/en：无视系统显示语言，共享态即时更新
        let state = LanguageState::new(detect_system_lang());
        assert_eq!(apply_language(&state, LanguageSetting::Zh), Lang::Zh);
        assert_eq!(state.current(), Lang::Zh);
        assert_eq!(apply_language(&state, LanguageSetting::En), Lang::En);
        assert_eq!(state.current(), Lang::En);
        // auto：回落系统显示语言（与 resolve_setting 同语义）
        assert_eq!(apply_language(&state, LanguageSetting::Auto), detect_system_lang());
        assert_eq!(state.current(), detect_system_lang());
    }

    // ── T13：状态卡 detail 双语（zh 与既有文案逐字一致）────────────────

    #[test]
    fn detail_texts_zh_matches_legacy_wording() {
        // zh 文案保持历史字样：既有 orchestrator 单测断言这些子串
        let t = detail_texts(Lang::Zh);
        assert_eq!(t.port_held(443, "svchost.exe"), "端口 443 被进程 svchost.exe 占用");
        assert_eq!(
            t.port_held_not_started(3001, "nginx.exe"),
            "端口 3001 被进程 nginx.exe 占用，未拉起"
        );
        assert!(t.scripts_dir_unavailable().contains("脚本目录不可用"));
        assert!(t.cloudcli_unavailable().contains("cloudcli 不可用"));
        assert_eq!(t.start_failed("X"), "启动失败：X");
        assert!(t.script_failed(2).contains("code=2"));
        assert!(t.start_timeout(60).contains("启动超时"));
        assert!(t.logs_unreadable("a、b").contains("日志暂不可读"));
        assert!(t.log_tail_header(r"C:\x.log").contains("尾部"));
        assert!(t.dispatch_failed("err").starts_with("拉起失败"));
    }

    #[test]
    fn detail_texts_en_full_set() {
        let t = detail_texts(Lang::En);
        assert!(t.port_held(443, "svchost.exe").contains("held by process svchost.exe"));
        assert!(t.port_held_not_started(443, "x").contains("not started"));
        assert!(t.scripts_dir_unavailable().contains("unavailable"));
        assert!(t.cloudcli_unavailable().contains("unavailable"));
        assert!(t.start_failed("X").starts_with("Start failed"));
        assert!(t.start_timeout(60).contains("60s"));
        for s in [
            t.script_timeout(),
            t.script_failed(2),
            t.spawn_failed("e"),
            t.dispatch_failed("e"),
            t.log_tail_header("a"),
            t.logs_unreadable("a"),
        ] {
            assert!(!s.contains('端'), "英文文案不得混入中文：{s}");
        }
    }

    #[test]
    fn shell_error_text_maps_uac_decline() {
        // AC20：UAC 拒绝（SE_ERR_ACCESSDENIED=5）给出明确提示
        assert!(shell_error_text(5, Lang::Zh).contains("UAC"));
        assert!(shell_error_text(5, Lang::En).to_lowercase().contains("uac"));
        assert!(shell_error_text(2, Lang::Zh).contains("ShellExecuteW"));
        assert!(shell_error_text(31, Lang::En).contains("31"));
    }
}
