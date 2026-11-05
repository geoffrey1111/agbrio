//! Desktop platform admission. Unsupported features fail before side effects.
// D4 transaction leaf only: no desktop command or installer admission yet.
#[cfg(any(test,target_os="macos"))]
pub(crate) mod macos_update_transaction;
#[cfg(any(test,target_os="macos"))]
mod email_keychain;
#[cfg(target_os="macos")]
pub(crate) mod macos_keychain;
pub(crate) const UNSUPPORTED: &str = "PLATFORM_NOT_YET_SUPPORTED";
pub(crate) fn pi_offered() -> bool { !cfg!(target_os="macos") }
pub(crate) fn require_pi_offered() -> Result<(),String> {
    if pi_offered(){Ok(())}else{Err("PI_NOT_OFFERED_ON_MAC".into())}
}

#[cfg(target_os="macos")]
pub(crate) mod macos_notifications;
#[cfg(target_os="macos")]
pub(crate) mod macos_startup;
#[cfg(target_os="macos")]
pub(crate) mod macos_resources;
#[cfg(target_os="macos")]
pub(crate) mod macos_web_recovery;

pub(crate) fn require_native_desktop()->Result<(),String>{
    if cfg!(any(windows,target_os="macos")){Ok(())}else{Err(UNSUPPORTED.into())}
}

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
    fn platform_resource_merge_excludes_pi_only_from_the_mac_bundle() {
        use tauri::utils::{config::parse::read_from,platform::Target};
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let (mac,_)=read_from(Target::MacOS,root).unwrap();
        let (windows,_)=read_from(Target::Windows,root).unwrap();
        assert!(mac["bundle"]["resources"].get("../tools/pi-adapter").is_none());
        assert_eq!(windows["bundle"]["resources"]["../tools/pi-adapter"],"pi-adapter");
        for key in ["resources/browser-executor-lite","../tools/claude-code-plugin","../tools/agbrio-control"] {
            assert_eq!(mac["bundle"]["resources"][key],windows["bundle"]["resources"][key]);
        }
        let _:tauri::utils::config::Config=serde_json::from_value(mac).unwrap();
        let _:tauri::utils::config::Config=serde_json::from_value(windows).unwrap();
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
    fn owner_omitted_pi_is_neither_discovered_nor_accepted() {
        assert!(!super::pi_offered());
        let caps=crate::app_control::capabilities("INSTANCE");
        assert!(caps["providers"].get("PI").is_none());
        let tools=crate::assistant_mcp::tools();
        let encoded=serde_json::to_string(&tools).unwrap();
        assert!(!encoded.contains("agbrio_list_pi_models"));
        assert!(!encoded.contains("Pi "));
        assert!(crate::assistant_operations::decode("CREATE_CHAT",serde_json::json!({"provider":"PI","directory":"not-even-a-native-path"})).is_err());
    }
    #[test]
    fn unavailable_native_features_fail_before_process_or_settings_work() {
        assert_eq!(crate::pi::installation().err().unwrap(), super::UNSUPPORTED);
        assert!(!crate::shared_codex::status().ready);
        assert_eq!(crate::shared_codex::launch().err().unwrap(), super::UNSUPPORTED);
    }
}
