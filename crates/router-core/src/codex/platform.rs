//! Narrow local installation/process boundary; no Tauri or protocol authority.
use std::{ffi::OsString, path::PathBuf, process::Command};

/// The resident app-server is a Router child, not an owner-facing shell.
/// In particular, npm's Windows `codex.cmd` wrapper otherwise opens a visible
/// console window even though all protocol I/O is piped by `CoreAdapter`.
/// Keeping the process windowless has no effect on its stdin/stdout contract.
pub fn codex_app_server_command() -> Command {
    let mut command = if let Some(override_path) =
        std::env::var_os("AI_WORK_ROUTER_CODEX_EXECUTABLE").filter(|value| !value.is_empty())
    {
        Command::new(override_path)
    } else if let Some(native_executable) = current_user_npm_codex_native_executable() {
        // The JavaScript entrypoint launches this same bundled native CLI, but
        // that extra process produces a visible conhost on Windows.  Start the
        // official platform binary directly so CREATE_NO_WINDOW applies to the
        // actual app-server process, not only to its wrapper.
        Command::new(native_executable)
    } else if let Some(entrypoint) = current_user_npm_codex_entrypoint() {
        // Do not launch npm's `codex.cmd`: its batch-file trampoline creates a
        // second `cmd.exe`/`conhost.exe` even when the direct child is created
        // with CREATE_NO_WINDOW.  Codex's documented npm wrapper invokes this
        // exact Node entrypoint, so call it directly and preserve the same
        // app-server stdio protocol without an owner-facing console surface.
        let mut command = Command::new("node");
        command.arg(entrypoint);
        command
    } else {
        Command::new(codex_executable())
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command
}
/// Uses an explicit process override first, then the current user's npm-managed
/// Codex CLI when available, and finally the PATH-visible CLI. The Desktop app
/// can intentionally retain an older bundled executable after the user has
/// updated the standalone CLI; preferring the npm-managed install keeps Router
/// on the current app-server surface needed for bounded reply observation.
/// An explicit process-scoped override remains available for a verified
/// compatible CLI generation.
fn codex_executable() -> OsString {
    std::env::var_os("AI_WORK_ROUTER_CODEX_EXECUTABLE")
        .filter(|value| !value.is_empty())
        .or_else(current_user_npm_codex_executable)
        .unwrap_or_else(|| OsString::from("codex"))
}

fn current_user_npm_codex_executable() -> Option<OsString> {
    #[cfg(windows)]
    {
        let path = PathBuf::from(std::env::var_os("APPDATA")?)
            .join("npm")
            .join("codex.cmd");
        path.is_file().then(|| path.into_os_string())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn current_user_npm_codex_entrypoint() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let path = PathBuf::from(std::env::var_os("APPDATA")?)
            .join("npm")
            .join("node_modules")
            .join("@openai")
            .join("codex")
            .join("bin")
            .join("codex.js");
        path.is_file().then_some(path)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn current_user_npm_codex_native_executable() -> Option<PathBuf> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        let path = PathBuf::from(std::env::var_os("APPDATA")?)
            .join("npm")
            .join("node_modules")
            .join("@openai")
            .join("codex")
            .join("node_modules")
            .join("@openai")
            .join("codex-win32-x64")
            .join("vendor")
            .join("x86_64-pc-windows-msvc")
            .join("bin")
            .join("codex.exe");
        path.is_file().then_some(path)
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        None
    }
}
