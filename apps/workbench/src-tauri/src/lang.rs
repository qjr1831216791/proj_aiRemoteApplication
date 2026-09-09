//! 语言判定与 Rust 侧原生文案（T2：托盘菜单）。
//!
//! AC25：默认跟随系统**显示语言**——zh → 中文、否则英文；与前端 `src/i18n/`
//! 的 `detectLang()`（navigator.language）同语义。判定核心为纯函数，单测覆盖；
//! T14 统一前后端词条源后，托盘文案改由统一词典驱动。

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
