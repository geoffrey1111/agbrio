//! Codex connection without a resident shared backend.
//!
//! The Host keeps speaking app-server JSON-RPC over a child's stdio, exactly as it
//! does with the shared transport, so its receipts, fences and UNKNOWN handling are
//! unchanged. This child only routes:
//! - reads go to an on-demand stdio app-server, cached against the rollout file so an
//!   idle machine runs no extra Codex process;
//! - writes to a thread Desktop holds go through Desktop's private IPC pipe;
//! - writes to a free thread go to the on-demand app-server, which the writer lock
//!   keeps from ever sharing a thread with Desktop.
//!
//! A write whose outcome is uncertain gets no reply, so the Host's own timeout records
//! UNKNOWN. Nothing here retries a write. The IPC is private to OpenAI and unstable:
//! see docs/CODEX_LIGHT_CONNECTION.md.
#[cfg(windows)]
mod backend;
#[cfg(windows)]
mod hub;
#[cfg(windows)]
mod ipc;
#[cfg(windows)]
mod native_files;
#[cfg(all(windows, test))]
mod host_live;
#[cfg(all(windows, test))]
mod tests;

pub const TRANSPORT_FLAG: &str = "--codex-light-transport";

/// Runs the transport when this process was launched as one. Returns true if it did.
pub fn run_if_requested() -> bool {
    if !std::env::args().any(|argument| argument == TRANSPORT_FLAG) {
        return false;
    }
    #[cfg(windows)]
    hub::run_stdio();
    #[cfg(not(windows))]
    eprintln!("CODEX_LIGHT_UNSUPPORTED_PLATFORM");
    true
}

/// Off by default until the product surface and owner acceptance land. Shared stays
/// first, because a Host must never run both connections.
pub(crate) fn enabled() -> bool {
    #[cfg(windows)]
    {
        if std::env::var_os("AGBRIO_CODEX_LIGHT").is_some_and(|value| value == "1") {
            return true;
        }
        std::env::var_os("LOCALAPPDATA")
            .map(|root| std::path::PathBuf::from(root).join("AIWorkRouter/codex-light.json"))
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .is_some_and(|value| value["enabled"] == true)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Owner desktop choice only; no phone or MCP path reaches this.
pub(crate) fn set_enabled(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let root = std::env::var_os("LOCALAPPDATA").ok_or("CODEX_LIGHT_SETTINGS_UNAVAILABLE")?;
        write_preference(&std::path::PathBuf::from(root).join("AIWorkRouter"), enabled)
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("CODEX_LIGHT_UNSUPPORTED_PLATFORM".into())
    }
}

#[cfg(windows)]
fn write_preference(directory: &std::path::Path, enabled: bool) -> Result<(), String> {
    use std::io::Write;
    std::fs::create_dir_all(directory).map_err(|_| "CODEX_LIGHT_SETTINGS_UNAVAILABLE")?;
    let path = directory.join("codex-light.json");
    if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink() || meta.is_dir()) {
        return Err("CODEX_LIGHT_SETTINGS_INVALID".into());
    }
    let temporary = directory.join(format!(".codex-light-{}.tmp", uuid::Uuid::new_v4()));
    let written = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        file.write_all(serde_json::json!({"enabled":enabled}).to_string().as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, &path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err("CODEX_LIGHT_SETTINGS_UNAVAILABLE".into());
    }
    Ok(())
}

/// Whether Desktop's pipe exists. Listing the namespace opens no connection.
pub(crate) fn desktop_reachable() -> bool {
    #[cfg(windows)]
    {
        std::fs::read_dir(r"\\.\pipe\").is_ok_and(|mut names| names.any(|name| name.is_ok_and(|name| name.file_name() == "codex-ipc")))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub(crate) fn transport_command() -> Result<std::process::Command, String> {
    let executable = std::env::current_exe().map_err(|_| "CODEX_LIGHT_EXECUTABLE_UNAVAILABLE")?;
    let mut command = std::process::Command::new(executable);
    command.arg(TRANSPORT_FLAG);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    Ok(command)
}

#[cfg(all(windows, test))]
mod preference_tests {
    #[test]
    fn the_preference_round_trips_and_refuses_a_directory_in_its_place() {
        let root = tempfile::tempdir().unwrap();
        super::write_preference(root.path(), true).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(root.path().join("codex-light.json")).unwrap()).unwrap();
        assert_eq!(saved["enabled"], true);
        super::write_preference(root.path(), false).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(root.path().join("codex-light.json")).unwrap()).unwrap();
        assert_eq!(saved["enabled"], false);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1, "no temporary file is left behind");

        let other = tempfile::tempdir().unwrap();
        std::fs::create_dir(other.path().join("codex-light.json")).unwrap();
        assert!(super::write_preference(other.path(), true).is_err());
    }
}
