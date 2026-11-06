//! One-time retirement of a shared backend left behind by an older Agbrio.
//!
//! The shared code is gone, but an upgraded machine can still have that resident backend
//! running from the pinned copy in `codex-shared/runtime`, which carries its own node.exe.
//! This uses only that copy's own scripts. Its guarded disable restores Desktop's environment
//! and removes its certificate, and refuses while work is active. An idle backend retires by
//! itself. One in use waits for the owner's confirmation on this desktop, which stops the
//! processes the copy verifies as its own, by identity and never by name, and then runs the
//! same disable. An elevated backend asks for Windows consent, and a refusal is not retried
//! until the next launch. Once nothing is pending, retired files are cleared.
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
const UAC_CANCELLED: i32 = 1223;
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const RUNTIME_INCOMPLETE: &str = "SHARED_RETIREMENT_RUNTIME_INCOMPLETE";
/// Everything the disable path of the pinned copy loads.
const DISABLE_FILES: [&str; 8] = ["node.exe", "control.mjs", "privilege.mjs", "state-file.mjs", "environment.ps1", "owned-processes.ps1", "privilege.ps1", "peer-service.ps1"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Still in use, or not attempted yet. Retried automatically.
    Waiting,
    /// The owner confirmed and the backend is being stopped.
    Stopping,
    /// Refused for a reason that time will not fix. Tried again on the next launch.
    Blocked(String),
}

static RUNNING: AtomicBool = AtomicBool::new(false);
static OUTCOME: Mutex<Option<Phase>> = Mutex::new(None);
static CLEARED: AtomicBool = AtomicBool::new(false);

fn data_root() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("AIWorkRouter"))
}

fn directory() -> Option<PathBuf> {
    Some(data_root()?.join("codex-shared"))
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
    match pending_in(&directory) {
        Ok(false) => clear_once(),
        Ok(true) => {
            if !matches!(phase(), Some(Phase::Blocked(_))) && !RUNNING.swap(true, Ordering::SeqCst) {
                std::thread::spawn(move || finish(attempt(&directory)));
            }
        }
        Err(_) => {}
    }
}

fn clear_once() {
    if let Some(root) = data_root() {
        if !CLEARED.swap(true, Ordering::SeqCst) {
            std::thread::spawn(move || super::legacy_cleanup::run(&root));
        }
    }
}

fn finish(outcome: Option<Phase>) {
    if let Ok(mut saved) = OUTCOME.lock() {
        *saved = outcome;
    }
    RUNNING.store(false, Ordering::SeqCst);
}

/// What the confirmation describes. None when the backend is not answering.
pub(crate) fn usage() -> Option<(u64, bool)> {
    let directory = directory()?;
    let (health, instance) = read_health(&directory)?;
    (health["instance"].as_str() == Some(instance.as_str())).then(|| (health["activeTurns"].as_u64().unwrap_or(0), health["desktopConnected"] == true))
}

/// The owner's desktop confirmation. A refused or failed stop stays until the next launch.
pub(crate) fn confirm_stop() -> Result<(), String> {
    let directory = directory().ok_or("SHARED_SETTINGS_UNAVAILABLE")?;
    if !pending_in(&directory)? {
        return Ok(());
    }
    if let Some(Phase::Blocked(code)) = phase() {
        return Err(code);
    }
    // An automatic attempt ends within seconds, and it only ever reports the backend in use.
    for _ in 0..100 {
        if !RUNNING.swap(true, Ordering::SeqCst) {
            if let Ok(mut saved) = OUTCOME.lock() {
                *saved = Some(Phase::Stopping);
            }
            std::thread::spawn(move || finish(attempt_confirmed(&directory)));
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("SHARED_RETIREMENT_BUSY".into())
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
    mark_ready_file_utf8(directory);
    disable(directory, &runtime).err().map(|code| classify(&code))
}

fn attempt_confirmed(directory: &Path) -> Option<Phase> {
    let runtime = directory.join("runtime");
    if !runtime_complete(&runtime) {
        return Some(Phase::Blocked(RUNTIME_INCOMPLETE.into()));
    }
    let Ok(_owner) = OwnerMutex::acquire(directory) else { return Some(Phase::Waiting) };
    mark_ready_file_utf8(directory);
    stop_owned_processes(directory, &runtime).and_then(|()| disable(directory, &runtime)).err().map(|code| classify(&code))
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

fn read_health(directory: &Path) -> Option<(Value, String)> {
    let settings = fs::read(directory.join("settings.json")).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())?;
    let (port, instance) = (settings["port"].as_u64()?, settings["instance"].as_str()?.to_owned());
    let client = reqwest::blocking::Client::builder().no_proxy().timeout(Duration::from_secs(2)).build().ok()?;
    let health = client.get(format!("http://127.0.0.1:{port}/health")).send().and_then(|reply| reply.json::<Value>()).ok()?;
    Some((health, instance))
}

fn still_in_use(directory: &Path) -> bool {
    read_health(directory).is_some_and(|(health, instance)| in_use(&health, &instance))
}

/// The disable refuses while a turn runs, so the owner's confirmation ends the processes first
/// through the same identity checks the disable uses. Desktop itself is never among them.
fn stop_owned_processes(directory: &Path, runtime: &Path) -> Result<(), String> {
    let script = runtime.join("environment.ps1");
    // Without a snapshot the script captures and stops in one process. Passing its capture
    // through here would break on a profile path outside the console's code page.
    let arguments = ["-Action".to_owned(), "Stop".to_owned(), "-Directory".to_owned(), directory.to_string_lossy().into_owned()];
    if !powershell(&script, &arguments)?.status.success() {
        // An elevated backend cannot be stopped from this token. Windows asks the owner.
        if !may_be_elevated(directory) {
            return Err("SHARED_STOP_FAILED".into());
        }
        match elevated_powershell(&script, &arguments)? {
            0 => {}
            UAC_CANCELLED => return Err("SHARED_UAC_CANCELLED".into()),
            _ => return Err("SHARED_STOP_FAILED".into()),
        }
    }
    // With nothing left running, the disable would otherwise follow this old preference and
    // ask a second time, for work that only touches this user's own settings.
    fs::write(directory.join("launch-preferences.json"), r#"{"launchAsAdministrator":false}"#).map_err(|_| "SHARED_SETTINGS_UNAVAILABLE".into())
}

fn may_be_elevated(directory: &Path) -> bool {
    fs::read(directory.join("settings.json")).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).is_none_or(|settings| settings["launchElevated"] != false)
}

/// The copy's scripts read this file with the shell's default encoding. Windows PowerShell
/// then misreads a profile path outside the code page unless the file says it is UTF-8.
/// Only those scripts read it, so the marker costs nothing elsewhere.
fn mark_ready_file_utf8(directory: &Path) {
    let path = directory.join("ready.json");
    if let Ok(bytes) = fs::read(&path) {
        if !bytes.starts_with(&UTF8_BOM) {
            let _ = fs::write(&path, [&UTF8_BOM[..], &bytes].concat());
        }
    }
}

/// The same choice the pinned copy makes: it was written against this order.
fn preferred_powershell() -> PathBuf {
    let core = std::env::var_os("ProgramW6432").or_else(|| std::env::var_os("ProgramFiles")).map(|root| PathBuf::from(root).join(r"PowerShell\7\pwsh.exe"));
    core.filter(|path| path.is_file()).unwrap_or_else(|| std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
}

fn powershell(script: &Path, arguments: &[String]) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(preferred_powershell());
    command.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(script).args(arguments).creation_flags(0x08000000); // CREATE_NO_WINDOW
    command.output().map_err(|_| "SHARED_LIFECYCLE_FAILED".into())
}

/// Single quotes are PowerShell's literal strings, doubled when they occur in a path.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn elevated_arguments(script: &Path, arguments: &[String]) -> String {
    let mut inner = vec!["-NoProfile".to_owned(), "-NonInteractive".to_owned(), "-ExecutionPolicy".to_owned(), "Bypass".to_owned(), "-File".to_owned(), format!("\"{}\"", script.display())];
    inner.extend(arguments.iter().map(|value| if value.starts_with('-') { value.clone() } else { format!("\"{value}\"") }));
    let list = inner.iter().map(|value| quoted(value)).collect::<Vec<_>>().join(",");
    format!("try{{$p=Start-Process -FilePath {} -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList {list};exit $p.ExitCode}}catch{{exit {UAC_CANCELLED}}}", quoted(&preferred_powershell().to_string_lossy()))
}

fn elevated_powershell(script: &Path, arguments: &[String]) -> Result<i32, String> {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(preferred_powershell());
    command.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &elevated_arguments(script, arguments)]).creation_flags(0x08000000); // CREATE_NO_WINDOW
    let status = command.status().map_err(|_| "SHARED_LIFECYCLE_FAILED")?;
    status.code().ok_or_else(|| "SHARED_LIFECYCLE_FAILED".into())
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

    /// The whole confirmed path with the copy's real scripts: a process it owns and that reports
    /// a running turn is stopped, then the disable completes. Point it as above.
    #[test]
    #[ignore = "explicit fixture holding a copy of a pinned runtime with a live owned process"]
    fn a_confirmed_stop_ends_a_busy_backend_and_retires_it() {
        let directory = PathBuf::from(std::env::var_os("AGBRIO_RETIREMENT_FIXTURE").unwrap());
        assert_eq!(pending_in(&directory), Ok(true));
        assert!(still_in_use(&directory), "the fixture must report a running turn");
        assert_eq!(attempt(&directory), Some(Phase::Waiting), "an unconfirmed attempt leaves it alone");
        assert_eq!(attempt_confirmed(&directory), None);
        assert_eq!(pending_in(&directory), Ok(false));
        assert!(!still_in_use(&directory));
    }

    #[test]
    fn the_ready_file_is_marked_once_and_only_an_unelevated_backend_skips_consent() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("ready.json"), "{}").unwrap();
        mark_ready_file_utf8(root.path());
        mark_ready_file_utf8(root.path());
        assert_eq!(fs::read(root.path().join("ready.json")).unwrap(), [&UTF8_BOM[..], b"{}"].concat());
        assert!(may_be_elevated(root.path()), "unknown is treated as possibly elevated");
        fs::write(root.path().join("settings.json"), r#"{"launchElevated":true}"#).unwrap();
        assert!(may_be_elevated(root.path()));
        fs::write(root.path().join("settings.json"), r#"{"launchElevated":false}"#).unwrap();
        assert!(!may_be_elevated(root.path()));
    }

    #[test]
    fn elevation_passes_each_path_as_one_literal_argument() {
        let line = elevated_arguments(Path::new(r"C:\O'Neil\a b\environment.ps1"), &["-Action".into(), "Stop".into(), "-Directory".into(), r"C:\O'Neil\a b".into()]);
        assert!(line.contains(r#"'"C:\O''Neil\a b\environment.ps1"'"#), "{line}");
        assert!(line.contains(r#"'"C:\O''Neil\a b"'"#), "{line}");
        assert!(line.contains("-Verb RunAs"));
        assert!(line.contains(&format!("exit {UAC_CANCELLED}")));
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
