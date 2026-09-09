//! 跨会话单实例判定（spec §4.5）。
//!
//! 调研结论（2026-09-09，tauri-plugin-single-instance v2 源码 platform_impl/windows.rs）：
//! 插件在 Windows 创建的互斥体名为 `{identifier}-sim`，**无 `Global\` 前缀**，
//! 落在本会话 `Local\` 命名空间 → 只保证**会话内**唯一（重复实例经隐藏窗口
//! WM_COPYDATA 通知首实例回调，随后自行 `process::exit(0)`，时点早于应用
//! setup 与窗口创建）。
//!
//! 跨会话唯一性由本模块补足：
//! - 全机首个实例成功创建 `Global\ai-remote-workbench` 并持有至进程退出
//!   （随进程终止由内核回收），同时创建会话标记 `Local\ai-remote-workbench`；
//! - 后来的实例探测到 Global 已存在时，以会话标记区分：
//!   - 标记存在 → 本会话已有主实例，交由插件完成「激活主窗 + 自行退出」；
//!   - 标记不存在 → 主实例在其他会话，无法激活其窗口（多会话原则），
//!     弹提示后退出。

use std::time::Duration;

use crate::lang::Lang;

const GLOBAL_MUTEX: &str = "Global\\ai-remote-workbench";
const SESSION_MARKER: &str = "Local\\ai-remote-workbench";
/// 主实例创建 Global 后建立会话标记之间有极小窗口，重复实例探测时稍作重试
/// （仅影响同时双击等极端竞态的归类，不影响正确性）
const MARKER_RETRY: usize = 3;
const MARKER_RETRY_DELAY: Duration = Duration::from_millis(100);

/// 本进程的单实例身份（`probe()` 结果）
pub enum GlobalPrimary {
    /// 全机首个实例：持有 Global 互斥体与会话标记（guard 需存活至进程退出）
    Acquired(PrimaryGuard),
    /// 同会话已有主实例：插件会激活它并让本进程退出
    SameSession,
    /// 其他会话持有 Global 互斥体：提示后退出（多会话原则）
    OtherSession,
    /// Win32 调用异常（极罕见）：放弃跨会话约束继续运行，会话内唯一仍由插件保证
    Unavailable,
}

/// 主实例持有的内核句柄（Global 互斥体 + 会话标记）。
/// 注意：guard 被 Drop 即释放互斥体，主进程须以 `mem::forget` 持有至进程退出。
pub struct PrimaryGuard {
    _global: OwnedMutex,
    _marker: OwnedMutex,
}

/// 启动时判定本进程的单实例身份
pub fn probe() -> GlobalPrimary {
    #[cfg(windows)]
    {
        match OwnedMutex::create_owned(GLOBAL_MUTEX) {
            Ok(Some(global)) => match OwnedMutex::create_owned(SESSION_MARKER) {
                Ok(Some(marker)) => GlobalPrimary::Acquired(PrimaryGuard {
                    _global: global,
                    _marker: marker,
                }),
                // 能新建 Global 却撞上会话标记：仅理论可达（竞态），按降级处理
                Ok(None) => {
                    log::warn!("Global 互斥体新建成功但会话标记已存在（竞态），放弃跨会话约束");
                    GlobalPrimary::Unavailable
                }
                Err(e) => {
                    log::warn!("创建会话标记失败：{e}——放弃跨会话约束，仅会话内单实例生效");
                    GlobalPrimary::Unavailable
                }
            },
            Ok(None) => {
                // 已有实例持有 Global：区分同会话（可激活）与跨会话（提示后退出）
                for attempt in 1..=MARKER_RETRY {
                    if OwnedMutex::exists(SESSION_MARKER) {
                        return GlobalPrimary::SameSession;
                    }
                    if attempt < MARKER_RETRY {
                        std::thread::sleep(MARKER_RETRY_DELAY);
                    }
                }
                GlobalPrimary::OtherSession
            }
            Err(e) => {
                log::warn!("创建 Global 互斥体失败：{e}——放弃跨会话约束，仅会话内单实例生效");
                GlobalPrimary::Unavailable
            }
        }
    }
    #[cfg(not(windows))]
    {
        GlobalPrimary::Unavailable
    }
}

/// 跨会话重复启动提示（spec §4.5：给明确提示而非静默执行/静默退出）
pub fn show_other_session_hint(lang: Lang) {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
        };

        let (text, caption) = match lang {
            Lang::Zh => (
                "AI 远程工作台已在其他用户会话中运行。\n\n每个会话独立管理自己的组件，\
                 本实例不再启动。请切换到已运行的会话使用。",
                "AI 远程工作台",
            ),
            Lang::En => (
                "AI Remote Workbench is already running in another user session.\n\n\
                 Each session manages its own services; this instance will not start. \
                 Please switch to the running session.",
                "AI Remote Workbench",
            ),
        };
        let text_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let caption_w: Vec<u16> = caption.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            MessageBoxW(
                None,
                PCWSTR::from_raw(text_w.as_ptr()),
                PCWSTR::from_raw(caption_w.as_ptr()),
                MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND | MB_TOPMOST,
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = lang;
    }
}

// ── Win32 薄封装 ──────────────────────────────────────────────────────────

#[cfg(windows)]
mod ffi {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
    use windows::Win32::System::Threading::{
        CreateMutexW, OpenMutexW, ReleaseMutex, MUTEX_ALL_ACCESS,
    };

    fn wide(name: &str) -> Vec<u16> {
        name.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 命名互斥体 guard（Drop 释放所有权并关闭句柄）
    pub(super) struct OwnedMutex(HANDLE);

    impl OwnedMutex {
        /// 创建命名互斥体并取得所有权。
        /// `Ok(None)` = 已存在（ERROR_ALREADY_EXISTS，探测句柄已关闭）；
        /// `Err` = API 失败（返回 NULL）。
        pub(super) fn create_owned(name: &str) -> Result<Option<Self>, String> {
            let name_w = wide(name);
            // bInitialOwner=true：主实例随 guard 存活持有所有权；
            // 句柄随进程终止由内核回收，主实例意外死亡不会留下"僵尸互斥体"
            let handle = unsafe {
                CreateMutexW(None, true, PCWSTR::from_raw(name_w.as_ptr()))
                    .map_err(|e| format!("CreateMutexW({name}) 失败：{e}"))?
            };
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                Ok(None)
            } else {
                Ok(Some(Self(handle)))
            }
        }

        /// 只探测存在性：OpenMutexW 不创建对象，不存在即失败——无探测残留。
        pub(super) fn exists(name: &str) -> bool {
            let name_w = wide(name);
            match unsafe {
                OpenMutexW(MUTEX_ALL_ACCESS, false, PCWSTR::from_raw(name_w.as_ptr()))
            } {
                Ok(handle) => {
                    unsafe {
                        let _ = CloseHandle(handle);
                    }
                    true
                }
                Err(_) => false,
            }
        }
    }

    impl Drop for OwnedMutex {
        fn drop(&mut self) {
            unsafe {
                let _ = ReleaseMutex(self.0);
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
use ffi::OwnedMutex;
