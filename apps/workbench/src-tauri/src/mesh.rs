//! 组网通道（spec 007 mesh）：EasyTier（secure-mode）私有组网。
//!
//! 本文件随 T1 建立骨架：随包二进制完整性校验（供应链防线，plan §7-R6）。
//! 后续任务按 [plan.md](../../../specs/007-mesh-access/plan.md) §8 扩展：
//! T4 配置渲染与 secure-mode 强制校验、T7 服务管理、T8 状态探询。

use crate::consts::{
    EASYTIER_CLI_EXE_NAME, EASYTIER_CLI_SHA256, EASYTIER_CORE_EXE_NAME, EASYTIER_CORE_SHA256,
    WINTUN_DLL_NAME, WINTUN_DLL_SHA256,
};
use crate::dns_api::sha256_hex;
use std::path::Path;

/// 校验目录内三个随包文件（easytier-core.exe / easytier-cli.exe / wintun.dll）
/// 的 SHA256 与版本锁定值一致。
///
/// 调用方：栈目录落位复制前（T7，防篡改源）、装机向导组网分支（T13，办后校验）。
/// 失败返回首个不符项的可读原因（文件缺失或哈希不符，含文件名）——不含密钥
/// 类敏感信息，可直接透出 UI（spec AC4 口径）。
pub fn verify_easytier_binaries(dir: &Path) -> Result<(), String> {
    let checks: [(&str, &str); 3] = [
        (EASYTIER_CORE_EXE_NAME, EASYTIER_CORE_SHA256),
        (EASYTIER_CLI_EXE_NAME, EASYTIER_CLI_SHA256),
        (WINTUN_DLL_NAME, WINTUN_DLL_SHA256),
    ];
    for (name, expected) in checks {
        let path = dir.join(name);
        let bytes = std::fs::read(&path).map_err(|e| format!("{name} 读取失败：{e}"))?;
        let actual = sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!(
                "{name} SHA256 校验不符（期望 {expected}，实际 {actual}）——疑似被篡改或版本不符"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 随包目录实测：resources/bin 三文件哈希应全部通过（真文件，发布构建同源）
    #[test]
    fn bundled_binaries_pass_verification() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("bin");
        assert!(dir.is_dir(), "resources/bin 不存在：{dir:?}");
        verify_easytier_binaries(&dir).expect("随包 easytier 文件校验应通过");
    }

    /// 文件缺失 → 返回含文件名的可读错误（不 panic）
    #[test]
    fn missing_file_reports_error() {
        let dir = std::env::temp_dir().join("et-verify-missing-test");
        std::fs::create_dir_all(&dir).unwrap();
        let err = verify_easytier_binaries(&dir)
            .expect_err("空目录校验应失败");
        assert!(
            err.contains(EASYTIER_CORE_EXE_NAME),
            "错误信息应含文件名：{err}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 哈希不符（内容被篡改）→ 拒绝并给出期望/实际值
    #[test]
    fn tampered_file_fails_hash() {
        let dir = std::env::temp_dir().join("et-verify-tamper-test");
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            EASYTIER_CORE_EXE_NAME,
            EASYTIER_CLI_EXE_NAME,
            WINTUN_DLL_NAME,
        ] {
            std::fs::write(dir.join(name), b"tampered").unwrap();
        }
        let err = verify_easytier_binaries(&dir)
            .expect_err("篡改内容校验应失败");
        assert!(err.contains("SHA256 校验不符"), "应报哈希不符：{err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
