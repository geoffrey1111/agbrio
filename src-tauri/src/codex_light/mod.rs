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
#[cfg(windows)]
mod legacy_cleanup;
#[cfg(windows)]
pub(crate) mod retirement;
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

/// The only connection on Windows. Other platforms keep their independent app-server,
/// which is also the path the Host's unit tests exercise on every platform.
pub(crate) fn enabled() -> bool {
    cfg!(all(windows, not(test)))
}

/// What first-run and settings read. `enabled` says this platform uses the light
/// connection, never that Desktop is open.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub enabled: bool,
    pub ready: bool,
    pub desktop_connected: bool,
    pub state: String,
    pub detail: Option<String>,
    /// Set only while an old shared backend waits for the owner's confirmation.
    pub shared_active_turns: Option<u64>,
    pub shared_desktop_connected: Option<bool>,
}

pub(crate) fn status() -> Status {
    let waiting = |state: &str, detail: Option<String>| Status { enabled: enabled(), ready: false, desktop_connected: false, state: state.into(), detail, shared_active_turns: None, shared_desktop_connected: None };
    #[cfg(windows)]
    {
        match retirement::phase() {
            Some(retirement::Phase::Waiting) => {
                let usage = retirement::usage();
                Status { shared_active_turns: usage.map(|usage| usage.0), shared_desktop_connected: usage.map(|usage| usage.1), ..waiting("SHARED_RETIRING", None) }
            }
            Some(retirement::Phase::Stopping) => waiting("SHARED_STOPPING", None),
            Some(retirement::Phase::Blocked(code)) => waiting("SHARED_RETIREMENT_BLOCKED", Some(code)),
            None => {
                let reachable = desktop_reachable();
                Status { enabled: true, ready: true, desktop_connected: reachable, state: if reachable { "LIGHT_CONNECTED" } else { "LIGHT_WAITING" }.into(), detail: None, shared_active_turns: None, shared_desktop_connected: None }
            }
        }
    }
    #[cfg(not(windows))]
    {
        waiting("UNSUPPORTED", None)
    }
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

/// Ignored live gates cannot start the test binary as a transport, so they name the
/// installed executable that can.
#[cfg(test)]
pub(crate) fn live_gate_transport_command() -> std::process::Command {
    let executable = std::env::var_os("AGBRIO_LIVE_TRANSPORT_EXE").expect("AGBRIO_LIVE_TRANSPORT_EXE names an Agbrio executable");
    let mut command = std::process::Command::new(executable);
    command.arg(TRANSPORT_FLAG);
    command
}
