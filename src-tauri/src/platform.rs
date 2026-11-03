//! Desktop platform admission. Unsupported features fail before side effects.
pub(crate) const UNSUPPORTED: &str = "PLATFORM_NOT_YET_SUPPORTED";

/// Caller owns the URL allowlist. This leaf only invokes the native opener;
/// no shell, navigation automation or account authorization is performed.
pub(crate) fn open_web_url(target:&str)->Result<(),String>{
    let parsed=url::Url::parse(target).map_err(|_|"PLATFORM_URL_INVALID")?;
    if parsed.scheme()!="https"||parsed.host_str().is_none()||!parsed.username().is_empty()||parsed.password().is_some(){return Err("PLATFORM_URL_INVALID".into());}
    #[cfg(windows)]{
        use std::os::windows::process::CommandExt;
        std::process::Command::new("rundll32.exe").arg("url.dll,FileProtocolHandler").arg(target).creation_flags(0x08000000).spawn().map_err(|_|"PLATFORM_URL_OPEN_FAILED")?;
        Ok(())
    }
    #[cfg(target_os="macos")]{
        std::process::Command::new("/usr/bin/open").arg(target).spawn().map_err(|_|"PLATFORM_URL_OPEN_FAILED")?;
        Ok(())
    }
    #[cfg(not(any(windows,target_os="macos")))]{Err(UNSUPPORTED.into())}
}

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
    #[test]
    fn native_opener_refuses_non_https_and_embedded_credentials_before_spawn(){
        for target in ["file:///private/data","javascript:alert(1)","http://example.invalid","https://user:secret@example.invalid"]{
            assert_eq!(open_web_url(target).unwrap_err(),"PLATFORM_URL_INVALID");
        }
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
