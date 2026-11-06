//! One-time retirement of a shared backend left behind by an older Agbrio.
//!
//! The shared code is gone, but an upgraded machine can still have that resident backend
//! running from the pinned copy in `codex-shared/runtime`, which carries its own node.exe.
//! This uses only that copy's guarded disable: it refuses while work is active, restores
//! Desktop's environment and stops only the processes it started. Nothing is killed by
//! name and nothing is deleted. An elevated backend makes the disable ask for Windows
//! consent, so a refusal is not retried until the next launch.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};

const PENDING: &str = "SHARED_RETIREMENT_PENDING";
const RUNTIME_INCOMPLETE: &str = "SHARED_RETIREMENT_RUNTIME_INCOMPLETE";
/// Everything the disable path of the pinned copy loads.
const DISABLE_FILES: [&str; 8] = ["node.exe", "control.mjs", "privilege.mjs", "state-file.mjs", "environment.ps1", "owned-processes.ps1", "privilege.ps1", "peer-service.ps1"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Still in use, or not attempted yet. Retried automatically.
    Waiting,
    /// Refused for a reason that time will not fix. Tried again on the next launch.
    Blocked(String),
}

static RUNNING: AtomicBool = AtomicBool::new(false);
static OUTCOME: Mutex<Option<Phase>> = Mutex::new(None);

fn directory() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("AIWorkRouter/codex-shared"))
}

fn needs_retiring(settings: &Value) -> bool {
    settings["enabled"] == true || settings["phase"].as_str().is_some_and(|phase| !phase.is_empty() && phase != "DISABLED")
}

fn pending_in(directory: &Path) -> Result<bool, String> {
    match fs::read(directory.join("settings.json")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("SHARED_SETTINGS_UNAVAILABLE".into()),
        Ok(bytes) => serde_json::from_slice(&bytes).map(|settings| needs_retiring(&settings)).map_err(|_| "SHARED_SETTINGS_INVALID".into()),
    }
}

/// The Host must not connect while a shared backend may still own threads.
pub(crate) fn require_clear() -> Result<(), String> {
    let Some(directory) = directory() else { return Ok(()) };
    if pending_in(&directory)? {
        return Err(PENDING.into());
    }
    Ok(())
}

pub(crate) fn phase() -> Option<Phase> {
    match require_clear() {
        Ok(()) => None,
        Err(code) if code != PENDING => Some(Phase::Blocked(code)),
        Err(_) => Some(OUTCOME.lock().ok().and_then(|outcome| outcome.clone()).unwrap_or(Phase::Waiting)),
    }
}

/// Called from the Host's reconnection loop. Costs one file read when nothing is pending.
pub(crate) fn advance() {
    let Some(directory) = directory() else { return };
    if !matches!(pending_in(&directory), Ok(true)) {
        return;
    }
    if matches!(phase(), Some(Phase::Blocked(_))) || RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let outcome = attempt(&directory);
        if let Ok(mut saved) = OUTCOME.lock() {
            *saved = outcome;
        }
        RUNNING.store(false, Ordering::SeqCst);
    });
}

fn attempt(directory: &Path) -> Option<Phase> {
    let runtime = directory.join("runtime");
    if !runtime_complete(&runtime) {
        return Some(Phase::Blocked(RUNTIME_INCOMPLETE.into()));
    }
    if still_in_use(directory) {
        return Some(Phase::Waiting);
    }
    let Ok(_owner) = OwnerMutex::acquire(directory) else { return Some(Phase::Waiting) };
    disable(directory, &runtime).err().map(|code| classify(&code))
}

fn runtime_complete(runtime: &Path) -> bool {
    DISABLE_FILES.iter().all(|name| runtime.join(name).is_file())
}

fn classify(code: &str) -> Phase {
    if code == "SHARED_DESKTOP_BUSY" {
        Phase::Waiting
    } else {
        Phase::Blocked(code.to_owned())
    }
}

/// A connected window would lose its backend, so it waits for the owner to quit Codex,
/// which also saves starting Windows consent for a disable that would refuse.
fn in_use(health: &Value, instance: &str) -> bool {
    health["instance"].as_str() == Some(instance) && (health["activeTurns"].as_u64().is_some_and(|turns| turns > 0) || health["desktopConnected"] == true)
}

fn still_in_use(directory: &Path) -> bool {
    let Some(settings) = fs::read(directory.join("settings.json")).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()) else { return false };
    let (Some(port), Some(instance)) = (settings["port"].as_u64(), settings["instance"].as_str()) else { return false };
    let Ok(client) = reqwest::blocking::Client::builder().no_proxy().timeout(Duration::from_secs(2)).build() else { return false };
    let health = client.get(format!("http://127.0.0.1:{port}/health")).send().and_then(|reply| reply.json::<Value>());
    health.is_ok_and(|health| in_use(&health, instance))
}

fn disable(directory: &Path, runtime: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let input = directory.join("control-request.json");
    let request = json!({"action":"disable","directory":directory,"scripts":runtime});
    fs::write(&input, request.to_string()).map_err(|_| "SHARED_SETTINGS_UNAVAILABLE")?;
    let mut command = Command::new(runtime.join("node.exe"));
    command.arg(runtime.join("control.mjs")).arg(&input).creation_flags(0x08000000); // CREATE_NO_WINDOW
    let output = command.output().map_err(|_| "SHARED_LIFECYCLE_FAILED")?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if error.starts_with("SHARED_") && error.len() < 128 { error } else { "SHARED_LIFECYCLE_FAILED".into() });
    }
    if pending_in(directory)? {
        return Err("SHARED_ROLLBACK_INCOMPLETE".into());
    }
    Ok(())
}

/// The pinned copy expects its callers to hold this mutex across every control call.
struct OwnerMutex(*mut std::ffi::c_void);

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateMutexW(attributes: *mut std::ffi::c_void, initial_owner: i32, name: *const u16) -> *mut std::ffi::c_void;
    fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
    fn ReleaseMutex(handle: *mut std::ffi::c_void) -> i32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
}

impl OwnerMutex {
    fn acquire(directory: &Path) -> Result<Self, ()> {
        let key = directory.to_string_lossy().replace('/', "\\");
        let name: Vec<u16> = format!("Local\\Agbrio.SharedCodex.{:x}", Sha256::digest(key.as_bytes())).encode_utf16().chain(Some(0)).collect();
        let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(());
        }
        let wait = unsafe { WaitForSingleObject(handle, 0) };
        if wait != 0 && wait != 0x80 {
            unsafe { CloseHandle(handle) };
            return Err(());
        }
        Ok(Self(handle))
    }
}

impl Drop for OwnerMutex {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the real pinned disable. Point it at a copy, with LOCALAPPDATA set to the copy's
    /// root: the disable refuses any directory other than LOCALAPPDATA's own.
    #[test]
    #[ignore = "explicit fixture holding a copy of a pinned runtime; never the real data folder"]
    fn a_pinned_copy_retires_itself() {
        let directory = PathBuf::from(std::env::var_os("AGBRIO_RETIREMENT_FIXTURE").unwrap());
        assert_eq!(pending_in(&directory), Ok(true));
        assert_eq!(attempt(&directory), None);
        assert_eq!(pending_in(&directory), Ok(false));
    }

    #[test]
    fn only_a_backend_that_may_still_exist_is_retired() {
        assert!(needs_retiring(&json!({"enabled":true,"phase":"ACTIVE"})));
        assert!(needs_retiring(&json!({"enabled":false,"phase":"PREPARED"})), "a prepared backend is resident too");
        assert!(needs_retiring(&json!({"enabled":false,"phase":"RECOVERY_REQUIRED"})), "an unfinished disable finishes");
        assert!(needs_retiring(&json!({"enabled":true,"format":1})), "a legacy backend has no phase");
        assert!(!needs_retiring(&json!({"enabled":false,"phase":"DISABLED"})));
        assert!(!needs_retiring(&json!({"enabled":false})));
    }

    #[test]
    fn missing_settings_are_clear_and_unreadable_settings_block() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(pending_in(root.path()), Ok(false));
        fs::write(root.path().join("settings.json"), "not json").unwrap();
        assert_eq!(pending_in(root.path()), Err("SHARED_SETTINGS_INVALID".into()));
        fs::write(root.path().join("settings.json"), r#"{"enabled":true,"phase":"ACTIVE"}"#).unwrap();
        assert_eq!(pending_in(root.path()), Ok(true));
    }

    #[test]
    fn an_incomplete_pinned_copy_is_reported_instead_of_run() {
        let root = tempfile::tempdir().unwrap();
        let runtime = root.path().join("runtime");
        fs::create_dir(&runtime).unwrap();
        for name in DISABLE_FILES {
            assert!(!runtime_complete(&runtime));
            fs::write(runtime.join(name), "").unwrap();
        }
        assert!(runtime_complete(&runtime));
        fs::remove_file(runtime.join("node.exe")).unwrap();
        assert_eq!(attempt(root.path()), Some(Phase::Blocked(RUNTIME_INCOMPLETE.into())));
    }

    #[test]
    fn active_work_waits_and_anything_else_stops_until_the_next_launch() {
        assert_eq!(classify("SHARED_DESKTOP_BUSY"), Phase::Waiting);
        assert_eq!(classify("SHARED_UAC_CANCELLED"), Phase::Blocked("SHARED_UAC_CANCELLED".into()));
        assert_eq!(classify("SHARED_CONNECTION_UNCERTAIN"), Phase::Blocked("SHARED_CONNECTION_UNCERTAIN".into()));
    }

    #[test]
    fn a_running_turn_or_open_window_keeps_the_backend_and_a_foreign_listener_does_not() {
        assert!(in_use(&json!({"instance":"a","activeTurns":1,"desktopConnected":false}), "a"));
        assert!(in_use(&json!({"instance":"a","activeTurns":0,"desktopConnected":true}), "a"));
        assert!(!in_use(&json!({"instance":"a","activeTurns":0,"desktopConnected":false}), "a"));
        assert!(!in_use(&json!({"instance":"other","activeTurns":3,"desktopConnected":true}), "a"));
    }
}
