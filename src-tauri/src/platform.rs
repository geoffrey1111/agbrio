//! Desktop platform admission. Unsupported features fail before side effects.
pub(crate) const UNSUPPORTED: &str = "PLATFORM_NOT_YET_SUPPORTED";

pub(crate) fn require_windows() -> Result<(), String> {
    if cfg!(windows) { Ok(()) } else { Err(UNSUPPORTED.into()) }
}

pub(crate) fn node_filename() -> &'static str {
    if cfg!(windows) { "browser-executor-node.exe" } else { "browser-executor-node" }
}

pub(crate) fn development_node_filename() -> &'static str {
    #[cfg(windows)]
    { "browser-executor-node-x86_64-pc-windows-msvc.exe" }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    { "browser-executor-node-aarch64-apple-darwin" }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    { "browser-executor-node-x86_64-apple-darwin" }
    #[cfg(not(any(windows, target_os = "macos")))]
    { "browser-executor-node-unsupported" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_matches_the_implemented_platform() {
        assert_eq!(require_windows().is_ok(), cfg!(windows));
        assert_eq!(node_filename().ends_with(".exe"), cfg!(windows));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    #[test]
    fn unavailable_native_features_fail_before_process_or_settings_work() {
        assert_eq!(crate::pi::installation().err().unwrap(), super::UNSUPPORTED);
        assert_eq!(crate::resource_monitor::read().unwrap_err(), super::UNSUPPORTED);
        assert!(!crate::shared_codex::status().ready);
        assert_eq!(crate::shared_codex::launch().err().unwrap(), super::UNSUPPORTED);
    }
}
