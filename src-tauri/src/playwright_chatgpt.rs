//! Persistent, exact-identity connection to the bundled Playwright sidecar.
//! One Router process owns one Chrome Playwright context for its lifetime.

use serde::Deserialize;
use serde_json::json;
use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        mpsc::{Receiver, RecvTimeoutError},
        Mutex, OnceLock,
    },
    time::Duration,
};

#[derive(Debug, Deserialize)]
struct SidecarEnvelope {
    ok: bool,
    error: Option<String>,
    result: Option<SidecarResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SidecarResult {
    state: Option<String>,
    message_id: Option<String>,
    text: Option<String>,
    accepted_user_turn_identity: Option<String>,
    previous_terminal_message_id: Option<String>,
    downloads: Option<Vec<SidecarDownload>>,
    filename: Option<String>,
    path: Option<String>,
    size: Option<u64>,
    sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SidecarDownload {
    filename: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TerminalReply {
    pub message_id: String,
    pub text: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AcceptedUserTurn {
    pub message_id: String,
    pub previous_terminal_message_id: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TerminalDownload {
    pub filename: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct MaterializedDownload {
    pub filename: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ApprovedAttachment<'a> {
    path: &'a str,
    sha256: &'a str,
}

fn router_profile_path(local: &Path) -> PathBuf {
    local
        // This is a Router-owned persistent profile. It is deliberately
        // distinct from ordinary Chrome and Codex's global Playwright profile;
        // neither credentials nor profile state are copied between them.
        .join("AIWorkRouter")
        .join("playwright-chatgpt-profile")
}

fn router_profile() -> Result<PathBuf, String> {
    let local = env::var_os("LOCALAPPDATA").ok_or("CHATGPT_ROUTER_PROFILE_REQUIRED")?;
    let profile = router_profile_path(Path::new(&local));
    // Create only the Router-owned empty profile root. Actual authentication is
    // always a human action in the explicitly opened browser surface.
    fs::create_dir_all(&profile).map_err(|_| "CHATGPT_ROUTER_PROFILE_REQUIRED")?;
    // A second Router carrier must not compete for this dedicated profile.
    // Do not solve contention by closing, reopening, or retrying a browser.
    if profile.join("lockfile").is_file() {
        return Err("CHATGPT_ROUTER_PROFILE_IN_USE".into());
    }
    Ok(profile)
}

enum SidecarLaunch {
    BundledBinary(PathBuf),
    DevelopmentScript(PathBuf),
}

/// Chrome DevTools MCP is an ESM package, so the owner-authorized normal
/// Chrome observer runs under the pristine bundled Node runtime rather than
/// the CommonJS-only SEA carrier. This is a separate read-only helper; it
/// never launches, closes, or reconfigures Chrome.
enum NormalChromeObserverLaunch {
    Bundled { runtime: PathBuf, script: PathBuf },
    DevelopmentScript(PathBuf),
}

fn bundled_sidecar_binary_candidate(executable: &std::path::Path) -> Option<PathBuf> {
    executable
        .parent()
        .map(|directory| directory.join("chatgpt-playwright-sidecar.exe"))
}

fn bundled_normal_chrome_node_candidate(executable: &std::path::Path) -> Option<PathBuf> {
    executable
        .parent()
        .map(|directory| directory.join("normal-chrome-observer-node.exe"))
}

fn bundled_normal_chrome_script_candidate(executable: &std::path::Path) -> Option<PathBuf> {
    executable.parent().map(|directory| {
        directory
            .join("normal-chrome")
            .join("normal-chrome-observer.mjs")
    })
}

fn sidecar_launch() -> Result<SidecarLaunch, String> {
    if let Some(path) = env::var_os("AI_WORK_ROUTER_PLAYWRIGHT_SIDECAR") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(
                if path.extension().is_some_and(|extension| extension == "exe") {
                    SidecarLaunch::BundledBinary(path)
                } else {
                    SidecarLaunch::DevelopmentScript(path)
                },
            );
        }
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(path) =
            bundled_sidecar_binary_candidate(&executable).filter(|path| path.is_file())
        {
            return Ok(SidecarLaunch::BundledBinary(path));
        }
    }
    let cwd = env::current_dir().map_err(|_| "CHATGPT_SIDECAR_MISSING")?;
    let root = if cwd.file_name().is_some_and(|name| name == "src-tauri") {
        cwd.parent().unwrap_or(&cwd).to_path_buf()
    } else {
        cwd
    };
    let path = root.join("scripts").join("chatgpt-playwright-sidecar.mjs");
    if path.is_file() {
        Ok(SidecarLaunch::DevelopmentScript(path))
    } else {
        Err("CHATGPT_SIDECAR_MISSING".into())
    }
}

fn normal_chrome_observer_launch() -> Result<NormalChromeObserverLaunch, String> {
    if let Ok(executable) = env::current_exe() {
        let runtime = bundled_normal_chrome_node_candidate(&executable);
        let script = bundled_normal_chrome_script_candidate(&executable);
        if let (Some(runtime), Some(script)) = (runtime, script) {
            if runtime.is_file() && script.is_file() {
                return Ok(NormalChromeObserverLaunch::Bundled { runtime, script });
            }
        }
    }
    let cwd = env::current_dir().map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_MISSING")?;
    let root = if cwd.file_name().is_some_and(|name| name == "src-tauri") {
        cwd.parent().unwrap_or(&cwd).to_path_buf()
    } else {
        cwd
    };
    let script = root.join("scripts").join("normal-chrome-observer.mjs");
    if script.is_file() {
        Ok(NormalChromeObserverLaunch::DevelopmentScript(script))
    } else {
        Err("CHATGPT_NORMAL_BROWSER_OBSERVER_MISSING".into())
    }
}

fn required_nonempty(value: Option<String>) -> Result<String, String> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or("CHATGPT_SIDECAR_PROTOCOL_INVALID".into())
}

struct SidecarSession {
    child: Child,
    stdin: ChildStdin,
    stdout: Receiver<std::io::Result<String>>,
}

const NORMAL_BROWSER_OBSERVER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(55);
const ROUTER_SIDECAR_RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

fn sidecar_stdout_lines(stdout: ChildStdout) -> Receiver<std::io::Result<String>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            let result = reader.read_line(&mut line).map(|_| line);
            let finished = matches!(&result, Ok(line) if line.is_empty()) || result.is_err();
            if sender.send(result).is_err() || finished {
                break;
            }
        }
    });
    receiver
}

fn read_sidecar_line(
    sidecar: &mut SidecarSession,
    missing_error: &'static str,
    timeout_error: &'static str,
    timeout: Duration,
) -> Result<String, String> {
    match sidecar.stdout.recv_timeout(timeout) {
        Ok(Ok(line)) if !line.is_empty() => Ok(line),
        Ok(Ok(_)) | Ok(Err(_)) | Err(RecvTimeoutError::Disconnected) => Err(missing_error.into()),
        Err(RecvTimeoutError::Timeout) => {
            // A blocked helper must never leave the owner-facing check in an
            // infinite loading state. Kill only the Router child; this never
            // changes the ordinary Chrome process or its pages.
            let _ = sidecar.child.kill();
            Err(timeout_error.into())
        }
    }
}

fn sidecar_session() -> &'static Mutex<Option<SidecarSession>> {
    static SESSION: OnceLock<Mutex<Option<SidecarSession>>> = OnceLock::new();
    SESSION.get_or_init(|| Mutex::new(None))
}

/// Separate from the Router-owned carrier session.  The normal-browser
/// observer is read-only and must never inherit a Router-owned context or
/// cause the ordinary Chrome process to be launched, closed, or restarted.
fn normal_browser_observer_session() -> &'static Mutex<Option<SidecarSession>> {
    static SESSION: OnceLock<Mutex<Option<SidecarSession>>> = OnceLock::new();
    SESSION.get_or_init(|| Mutex::new(None))
}

/// The Router-owned carrier and the owner-approved normal-Chrome observer are
/// different security surfaces.  A challenge in the retired carrier must
/// hard-stop that carrier, but it cannot be used as evidence that the owner's
/// already-open Chrome page is challenged too.  The normal observer detects a
/// challenge on its own exact page and fails closed there.
fn router_owned_carrier_security_required() -> &'static Mutex<bool> {
    static REQUIRED: OnceLock<Mutex<bool>> = OnceLock::new();
    REQUIRED.get_or_init(|| Mutex::new(false))
}

fn router_owned_carrier_security_gate() -> Result<(), String> {
    if *router_owned_carrier_security_required()
        .lock()
        .map_err(|_| "CHATGPT_PLAYWRIGHT_UNAVAILABLE")?
    {
        return Err("CHATGPT_ACCOUNT_SECURITY_REQUIRED".into());
    }
    Ok(())
}

fn is_account_security_error(error: &str) -> bool {
    error == "CHATGPT_ACCOUNT_SECURITY_REQUIRED"
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChatGptBrowserChannel {
    RouterOwnedCarrier,
    NormalChromeObserver,
}

fn account_security_trips_router_owned_carrier(
    channel: ChatGptBrowserChannel,
    error: &str,
) -> bool {
    channel == ChatGptBrowserChannel::RouterOwnedCarrier && is_account_security_error(error)
}

#[cfg(test)]
fn is_profile_in_use_error(error: &str) -> bool {
    error == "CHATGPT_ROUTER_PROFILE_IN_USE"
}

fn router_owned_carrier_sidecar_error(error: Option<String>) -> String {
    let error = error.unwrap_or_else(|| "CHATGPT_SIDECAR_FAILED".into());
    if account_security_trips_router_owned_carrier(
        ChatGptBrowserChannel::RouterOwnedCarrier,
        &error,
    ) {
        if let Ok(mut required) = router_owned_carrier_security_required().lock() {
            *required = true;
        }
    }
    error
}

/// A normal-Chrome security challenge is already confined by the helper to the
/// unique exact page.  Do not promote it into the retired carrier's circuit
/// breaker: that would make a stale dedicated-profile challenge incorrectly
/// prevent an owner from using their ordinary, independently healthy Chrome.
fn normal_browser_sidecar_error(error: Option<String>) -> String {
    let error = error.unwrap_or_else(|| "CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE".into());
    debug_assert!(!account_security_trips_router_owned_carrier(
        ChatGptBrowserChannel::NormalChromeObserver,
        &error,
    ));
    error
}

/// Router helper processes communicate exclusively through stdio.  On Windows
/// they must never create a visible console window in the owner's desktop
/// session, particularly when an explicit normal-Chrome read-only check starts
/// the observer.
fn keep_router_helper_background(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
}

fn start_sidecar_session() -> Result<SidecarSession, String> {
    let profile = router_profile()?;
    let launch = sidecar_launch()?;
    let mut command = match launch {
        SidecarLaunch::BundledBinary(path) => Command::new(path),
        SidecarLaunch::DevelopmentScript(path) => {
            let mut command = Command::new("node");
            command.arg(path);
            command
        }
    };
    keep_router_helper_background(&mut command);
    let mut child = command
        .arg("--profile")
        .arg(profile)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Provider detail is intentionally never accumulated in a long-lived
        // stderr pipe; typed JSON envelopes are the only application protocol.
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "CHATGPT_PLAYWRIGHT_UNAVAILABLE")?;
    Ok(SidecarSession {
        stdin: child.stdin.take().ok_or("CHATGPT_PLAYWRIGHT_UNAVAILABLE")?,
        stdout: sidecar_stdout_lines(
            child
                .stdout
                .take()
                .ok_or("CHATGPT_PLAYWRIGHT_UNAVAILABLE")?,
        ),
        child,
    })
}

fn start_normal_browser_observer_session() -> Result<SidecarSession, String> {
    let launch = normal_chrome_observer_launch()?;
    let mut command = match launch {
        NormalChromeObserverLaunch::Bundled { runtime, script } => {
            let mut command = Command::new(runtime);
            command.arg(script);
            command
        }
        NormalChromeObserverLaunch::DevelopmentScript(path) => {
            let mut command = Command::new("node");
            command.arg(path);
            command
        }
    };
    keep_router_helper_background(&mut command);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Typed JSON envelopes remain the only Router protocol.  Stderr is
        // intentionally not retained because it could contain provider page
        // details that are outside this observer's narrow contract.
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?;
    Ok(SidecarSession {
        stdin: child
            .stdin
            .take()
            .ok_or("CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?,
        stdout: sidecar_stdout_lines(
            child
                .stdout
                .take()
                .ok_or("CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?,
        ),
        child,
    })
}

fn request_after_open(
    conversation_id: &str,
    request: serde_json::Value,
) -> Result<SidecarResult, String> {
    router_owned_carrier_security_gate()?;
    let mut session = sidecar_session()
        .lock()
        .map_err(|_| "CHATGPT_PLAYWRIGHT_UNAVAILABLE")?;
    if session.is_none() {
        *session = Some(start_sidecar_session()?);
    }
    let sidecar = session.as_mut().ok_or("CHATGPT_PLAYWRIGHT_UNAVAILABLE")?;
    if sidecar
        .child
        .try_wait()
        .map_err(|_| "CHATGPT_PLAYWRIGHT_UNAVAILABLE")?
        .is_some()
    {
        *session = Some(start_sidecar_session()?);
    }
    let sidecar = session.as_mut().ok_or("CHATGPT_PLAYWRIGHT_UNAVAILABLE")?;
    let input = format!(
        "{}\n{}\n",
        json!({"id":"open","op":"open_exact","conversationId":conversation_id}),
        request,
    );
    sidecar
        .stdin
        .write_all(input.as_bytes())
        .map_err(|_| "CHATGPT_PLAYWRIGHT_UNAVAILABLE")?;
    sidecar
        .stdin
        .flush()
        .map_err(|_| "CHATGPT_PLAYWRIGHT_UNAVAILABLE")?;
    let open_line = read_sidecar_line(
        sidecar,
        "CHATGPT_SIDECAR_PROTOCOL_INVALID:OPEN_MISSING",
        "CHATGPT_PLAYWRIGHT_UNAVAILABLE",
        ROUTER_SIDECAR_RESPONSE_TIMEOUT,
    )?;
    let open: SidecarEnvelope = serde_json::from_str(&open_line)
        .map_err(|_| "CHATGPT_SIDECAR_PROTOCOL_INVALID:OPEN_MALFORMED")?;
    if !open.ok {
        return Err(router_owned_carrier_sidecar_error(open.error));
    }
    let response_line = read_sidecar_line(
        sidecar,
        "CHATGPT_SIDECAR_PROTOCOL_INVALID:REQUEST_MISSING",
        "CHATGPT_PLAYWRIGHT_UNAVAILABLE",
        ROUTER_SIDECAR_RESPONSE_TIMEOUT,
    )?;
    let response: SidecarEnvelope = serde_json::from_str(&response_line)
        .map_err(|_| "CHATGPT_SIDECAR_PROTOCOL_INVALID:REQUEST_MALFORMED")?;
    if !response.ok {
        return Err(router_owned_carrier_sidecar_error(response.error));
    }
    response
        .result
        .ok_or_else(|| "CHATGPT_SIDECAR_PROTOCOL_INVALID:RESULT_MISSING".into())
}

/// Read one terminal assistant reply from a unique, already-open normal
/// Chrome page matching the supplied stable conversation ID.  This sidecar
/// protocol has no navigation, setup, download, input, submit, or retry
/// operation.  If Chrome's user-confirmed debug connection is unavailable,
/// fail closed rather than launching a Router profile as a fallback.
fn normal_browser_request_after_exact_lookup(
    conversation_id: &str,
    lookup_operation: &str,
    request: serde_json::Value,
    allow_explicit_start: bool,
) -> Result<SidecarResult, String> {
    let mut session = normal_browser_observer_session()
        .lock()
        .map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?;
    if session.is_none() {
        if !allow_explicit_start {
            return Err("CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE".into());
        }
        *session = Some(start_normal_browser_observer_session()?);
    }
    let prior_session_exited = session
        .as_mut()
        .ok_or("CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?
        .child
        .try_wait()
        .map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?
        .is_some();
    if prior_session_exited {
        // Chrome may have dismissed or denied a previous visible consent. A
        // passive loop never restarts the helper; a fresh explicit owner check
        // gets exactly one new connection request instead of requiring an
        // unrelated second button press.
        *session = None;
        if !allow_explicit_start {
            return Err("CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE".into());
        }
        *session = Some(start_normal_browser_observer_session()?);
    }
    let sidecar = session
        .as_mut()
        .ok_or("CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?;
    let input = format!(
        "{}\n{}\n",
        json!({"id":"open","op":lookup_operation,"conversationId":conversation_id}),
        request,
    );
    sidecar
        .stdin
        .write_all(input.as_bytes())
        .map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?;
    sidecar
        .stdin
        .flush()
        .map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?;
    let open_line = read_sidecar_line(
        sidecar,
        "CHATGPT_NORMAL_BROWSER_OBSERVER_PROTOCOL_INVALID:OPEN_MISSING",
        "CHATGPT_NORMAL_BROWSER_OBSERVER_TIMEOUT",
        NORMAL_BROWSER_OBSERVER_RESPONSE_TIMEOUT,
    )?;
    let open: SidecarEnvelope = serde_json::from_str(&open_line)
        .map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_PROTOCOL_INVALID:OPEN_MALFORMED")?;
    if !open.ok {
        return Err(normal_browser_sidecar_error(open.error));
    }
    let response_line = read_sidecar_line(
        sidecar,
        "CHATGPT_NORMAL_BROWSER_OBSERVER_PROTOCOL_INVALID:REQUEST_MISSING",
        "CHATGPT_NORMAL_BROWSER_OBSERVER_TIMEOUT",
        NORMAL_BROWSER_OBSERVER_RESPONSE_TIMEOUT,
    )?;
    let response: SidecarEnvelope = serde_json::from_str(&response_line)
        .map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_PROTOCOL_INVALID:REQUEST_MALFORMED")?;
    if !response.ok {
        return Err(normal_browser_sidecar_error(response.error));
    }
    response
        .result
        .ok_or_else(|| "CHATGPT_NORMAL_BROWSER_OBSERVER_PROTOCOL_INVALID:RESULT_MISSING".into())
}

/// Establishes the narrow normal-Chrome observer connection without listing
/// targets or evaluating any page. Chrome owns the visible consent prompt;
/// this helper merely records whether that one-time connection completed.
fn normal_browser_connection_request() -> Result<SidecarResult, String> {
    let mut session = normal_browser_observer_session()
        .lock()
        .map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?;
    if session.is_none() {
        *session = Some(start_normal_browser_observer_session()?);
    }
    let prior_session_exited = session
        .as_mut()
        .ok_or("CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?
        .child
        .try_wait()
        .map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?
        .is_some();
    if prior_session_exited {
        *session = Some(start_normal_browser_observer_session()?);
    }
    let sidecar = session
        .as_mut()
        .ok_or("CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?;
    let input = format!("{}\n", json!({"id":"connect","op":"connect"}));
    sidecar
        .stdin
        .write_all(input.as_bytes())
        .map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?;
    sidecar
        .stdin
        .flush()
        .map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE")?;
    let response_line = read_sidecar_line(
        sidecar,
        "CHATGPT_NORMAL_BROWSER_OBSERVER_PROTOCOL_INVALID:CONNECT_MISSING",
        "CHATGPT_NORMAL_BROWSER_OBSERVER_TIMEOUT",
        NORMAL_BROWSER_OBSERVER_RESPONSE_TIMEOUT,
    )?;
    let response: SidecarEnvelope = serde_json::from_str(&response_line)
        .map_err(|_| "CHATGPT_NORMAL_BROWSER_OBSERVER_PROTOCOL_INVALID:CONNECT_MALFORMED")?;
    if !response.ok {
        return Err(normal_browser_sidecar_error(response.error));
    }
    response.result.ok_or_else(|| {
        "CHATGPT_NORMAL_BROWSER_OBSERVER_PROTOCOL_INVALID:CONNECT_RESULT_MISSING".into()
    })
}

/// Opens only the fixed ChatGPT sign-in/home page in the persistent
/// Router-owned Chrome profile.  This is explicit setup affordance, not a
/// conversation route, provider request, or credential automation.
pub fn open_setup() -> Result<(), String> {
    router_owned_carrier_security_gate()?;
    let mut session = sidecar_session()
        .lock()
        .map_err(|_| "CHATGPT_PLAYWRIGHT_UNAVAILABLE")?;
    if session.is_none() {
        *session = Some(start_sidecar_session()?);
    }
    let sidecar = session.as_mut().ok_or("CHATGPT_PLAYWRIGHT_UNAVAILABLE")?;
    if sidecar
        .child
        .try_wait()
        .map_err(|_| "CHATGPT_PLAYWRIGHT_UNAVAILABLE")?
        .is_some()
    {
        *session = Some(start_sidecar_session()?);
    }
    let sidecar = session.as_mut().ok_or("CHATGPT_PLAYWRIGHT_UNAVAILABLE")?;
    let input = format!("{}\n", json!({"id":"setup","op":"open_setup"}));
    sidecar
        .stdin
        .write_all(input.as_bytes())
        .map_err(|_| "CHATGPT_PLAYWRIGHT_UNAVAILABLE")?;
    sidecar
        .stdin
        .flush()
        .map_err(|_| "CHATGPT_PLAYWRIGHT_UNAVAILABLE")?;
    let response_line = read_sidecar_line(
        sidecar,
        "CHATGPT_SIDECAR_PROTOCOL_INVALID:SETUP_MISSING",
        "CHATGPT_PLAYWRIGHT_UNAVAILABLE",
        ROUTER_SIDECAR_RESPONSE_TIMEOUT,
    )?;
    let response: SidecarEnvelope = serde_json::from_str(&response_line)
        .map_err(|_| "CHATGPT_SIDECAR_PROTOCOL_INVALID:SETUP_MALFORMED")?;
    if !response.ok {
        return Err(router_owned_carrier_sidecar_error(response.error));
    }
    let result = response
        .result
        .ok_or("CHATGPT_SIDECAR_PROTOCOL_INVALID:SETUP_RESULT_MISSING")?;
    if result.state.as_deref() != Some("SETUP_READY") {
        return Err("CHATGPT_SIDECAR_PROTOCOL_INVALID:SETUP_RESULT_INVALID".into());
    }
    Ok(())
}

pub fn read_latest_terminal(conversation_id: &str) -> Result<Option<TerminalReply>, String> {
    let result = request_after_open(
        conversation_id,
        json!({"id":"latest","op":"latest_terminal"}),
    )?;
    if result.state.as_deref() != Some("TERMINAL") {
        return Ok(None);
    }
    let message_id = required_nonempty(result.message_id)?;
    let text = required_nonempty(result.text)?;
    Ok(Some(TerminalReply { message_id, text }))
}

/// The owner-authorized normal-browser read path.  It returns only the
/// terminal assistant identity/text for one exact already-open endpoint and
/// never falls back to the Router-owned carrier or a provider write.
pub fn read_existing_normal_browser_latest_terminal(
    conversation_id: &str,
) -> Result<Option<TerminalReply>, String> {
    let result = normal_browser_request_after_exact_lookup(
        conversation_id,
        "open_exact",
        json!({"id":"latest","op":"latest_terminal"}),
        true,
    )?;
    if result.state.as_deref() != Some("TERMINAL") {
        return Ok(None);
    }
    Ok(Some(TerminalReply {
        message_id: required_nonempty(result.message_id)?,
        text: required_nonempty(result.text)?,
    }))
}

/// One explicit desktop setup action. It only establishes Chrome's native
/// read-only DevTools connection and never lists tabs, opens an endpoint, or
/// reads ChatGPT content. Subsequent business checks can use that connection
/// passively.
pub fn connect_existing_normal_browser_read_only() -> Result<(), String> {
    let result = normal_browser_connection_request()?;
    if !normal_browser_connection_completed(&result) {
        return Err(
            "CHATGPT_NORMAL_BROWSER_OBSERVER_PROTOCOL_INVALID:CONNECT_RESULT_INVALID".into(),
        );
    }
    Ok(())
}

fn normal_browser_connection_completed(result: &SidecarResult) -> bool {
    result.state.as_deref() == Some("NORMAL_BROWSER_READ_ONLY_CONNECTED")
}

/// The host-resident observer may reuse an already established normal-Chrome
/// connection, but it is never allowed to start or restart one. A stopped
/// connection requires a fresh explicit owner action in Desktop/Mobile.
pub fn read_existing_normal_browser_latest_terminal_passive(
    conversation_id: &str,
) -> Result<Option<TerminalReply>, String> {
    let result = normal_browser_request_after_exact_lookup(
        conversation_id,
        "open_exact",
        json!({"id":"latest","op":"latest_terminal"}),
        false,
    )?;
    if result.state.as_deref() != Some("TERMINAL") {
        return Ok(None);
    }
    Ok(Some(TerminalReply {
        message_id: required_nonempty(result.message_id)?,
        text: required_nonempty(result.text)?,
    }))
}

/// Checks exact already-open-tab identity using only an existing owner-approved
/// normal-Chrome connection.  It does not start/restart a connection, activate
/// a tab, or read a message; `false` means this conversation was not open,
/// while an unavailable/ambiguous/security state remains an error.
pub fn existing_normal_browser_exact_tab_open_passive(
    conversation_id: &str,
) -> Result<bool, String> {
    match normal_browser_request_after_exact_lookup(
        conversation_id,
        "find_exact",
        json!({"id":"presence","op":"exact_tab_presence"}),
        false,
    ) {
        Ok(result) => Ok(result.state.as_deref() == Some("EXACT_TAB_PRESENT")),
        Err(error) if error == "CHATGPT_NORMAL_BROWSER_EXACT_TAB_NOT_OPEN" => Ok(false),
        Err(error) => Err(error),
    }
}

/// Read-only exact binding proof. Opening the exact `/c/<id>` route is verified
/// by the sidecar before this bounded terminal-state read. A conversation with
/// no terminal assistant turn is still a valid binding candidate.
pub fn verify_exact_conversation(conversation_id: &str) -> Result<(), String> {
    request_after_open(
        conversation_id,
        json!({"id":"verify","op":"latest_terminal"}),
    )?;
    Ok(())
}

/// Lists only downloadable resource-card filenames rendered on one exact
/// terminal message. The sidecar returns no provider URL, page text, cookie,
/// or DOM detail.
pub fn list_terminal_downloads(
    conversation_id: &str,
    message_id: &str,
) -> Result<Vec<TerminalDownload>, String> {
    let result = request_after_open(
        conversation_id,
        json!({"id":"downloads","op":"list_terminal_downloads","messageId":message_id}),
    )?;
    Ok(result
        .downloads
        .unwrap_or_default()
        .into_iter()
        .filter(|item| !item.filename.trim().is_empty())
        .map(|item| TerminalDownload {
            filename: item.filename,
        })
        .collect())
}

/// Performs exactly one provider-visible download click for one selected file
/// card on one exact terminal message. The caller supplies a Router-owned,
/// not-yet-existing destination and verifies the returned hash before use.
pub fn materialize_terminal_download(
    conversation_id: &str,
    message_id: &str,
    filename: &str,
    destination: &std::path::Path,
) -> Result<MaterializedDownload, String> {
    if filename.trim().is_empty() || destination.exists() {
        return Err("CHATGPT_ARTIFACT_INPUT_INVALID".into());
    }
    let destination = destination
        .to_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or("CHATGPT_ARTIFACT_INPUT_INVALID")?;
    let result = request_after_open(
        conversation_id,
        json!({
            "id":"materialize",
            "op":"materialize_terminal_download",
            "messageId":message_id,
            "filename":filename,
            "destination":destination,
        }),
    )?;
    let path = required_nonempty(result.path)?;
    let returned_filename = required_nonempty(result.filename)?;
    let sha256 = required_nonempty(result.sha256)?;
    let size = result
        .size
        .filter(|value| *value > 0)
        .ok_or("CHATGPT_ARTIFACT_CAPTURE_FAILED")?;
    if returned_filename != filename || path != destination {
        return Err("CHATGPT_ARTIFACT_CAPTURE_FAILED".into());
    }
    Ok(MaterializedDownload {
        filename: returned_filename,
        path,
        size,
        sha256,
    })
}

/// Writes one already-approved text payload to one exact conversation and
/// returns the provider-native accepted user-turn identity. It never retries.
pub fn submit_approved_text(conversation_id: &str, text: &str) -> Result<AcceptedUserTurn, String> {
    if text.trim().is_empty() {
        return Err("CHATGPT_APPROVED_TEXT_INVALID".into());
    }
    let result = request_after_open(
        conversation_id,
        json!({"id":"submit","op":"submit_approved_text","text":text}),
    )?;
    Ok(AcceptedUserTurn {
        message_id: required_nonempty(result.accepted_user_turn_identity)?,
        previous_terminal_message_id: result
            .previous_terminal_message_id
            .filter(|value| !value.trim().is_empty()),
    })
}

/// Writes one approved payload with already rehashed attachment files to one
/// exact conversation. The sidecar verifies the supplied send-time hashes again
/// immediately before Playwright assigns the exact file input; it never opens a
/// picker, clears a human draft, or retries ambiguous acceptance.
pub fn submit_approved_handoff(
    conversation_id: &str,
    text: &str,
    attachments: &[(String, String)],
) -> Result<AcceptedUserTurn, String> {
    if text.trim().is_empty() || attachments.is_empty() {
        return Err("CHATGPT_APPROVED_HANDOFF_INVALID".into());
    }
    let attachments = attachments
        .iter()
        .map(|(path, sha256)| ApprovedAttachment {
            path: path.as_str(),
            sha256: sha256.as_str(),
        })
        .collect::<Vec<_>>();
    let result = request_after_open(
        conversation_id,
        json!({"id":"submit","op":"submit_approved_handoff","text":text,"attachments":attachments}),
    )?;
    Ok(AcceptedUserTurn {
        message_id: required_nonempty(result.accepted_user_turn_identity)?,
        previous_terminal_message_id: result
            .previous_terminal_message_id
            .filter(|value| !value.trim().is_empty()),
    })
}

/// Bounded same-conversation terminal read after an accepted user turn. The
/// sidecar observes its existing page and never refreshes or resubmits.
pub fn wait_for_new_terminal(
    conversation_id: &str,
    previous_message_id: Option<&str>,
) -> Result<TerminalReply, String> {
    let result = request_after_open(
        conversation_id,
        json!({"id":"terminal","op":"wait_for_new_terminal","previousMessageId":previous_message_id}),
    )?;
    if result.state.as_deref() != Some("TERMINAL") {
        return Err("CHATGPT_TERMINAL_NOT_OBSERVED".into());
    }
    Ok(TerminalReply {
        message_id: required_nonempty(result.message_id)?,
        text: required_nonempty(result.text)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_reply_requires_identity_and_text() {
        let value: SidecarEnvelope = serde_json::from_value(
            json!({"ok":true,"result":{"state":"TERMINAL","messageId":"m","text":"body"}}),
        )
        .unwrap();
        assert!(value.ok);
        assert_eq!(value.result.unwrap().message_id.as_deref(), Some("m"));
    }

    #[test]
    fn accepted_user_turn_requires_provider_native_identity() {
        let value: SidecarEnvelope = serde_json::from_value(json!({
            "ok": true,
            "result": {
                "acceptedUserTurnIdentity": "user-message-1",
                "previousTerminalMessageId": "assistant-message-0"
            }
        }))
        .unwrap();
        let result = value.result.unwrap();
        assert_eq!(
            required_nonempty(result.accepted_user_turn_identity).unwrap(),
            "user-message-1"
        );
        assert_eq!(
            result.previous_terminal_message_id.as_deref(),
            Some("assistant-message-0")
        );
    }

    #[test]
    fn bundled_sidecar_binary_candidate_stays_next_to_the_desktop_executable() {
        let executable = PathBuf::from(r"D:\\preview\\AI Work Router.exe");
        assert_eq!(
            bundled_sidecar_binary_candidate(&executable).unwrap(),
            PathBuf::from(r"D:\\preview\\chatgpt-playwright-sidecar.exe")
        );
    }

    #[test]
    fn bundled_normal_chrome_observer_stays_with_its_explicit_runtime_and_script() {
        let executable = PathBuf::from(r"D:\\preview\\AI Work Router.exe");
        assert_eq!(
            bundled_normal_chrome_node_candidate(&executable).unwrap(),
            PathBuf::from(r"D:\\preview\\normal-chrome-observer-node.exe")
        );
        assert_eq!(
            bundled_normal_chrome_script_candidate(&executable).unwrap(),
            PathBuf::from(r"D:\\preview\\normal-chrome\\normal-chrome-observer.mjs")
        );
    }

    #[test]
    fn account_security_error_is_a_stable_typed_failure() {
        assert!(is_account_security_error(
            "CHATGPT_ACCOUNT_SECURITY_REQUIRED"
        ));
        assert!(!is_account_security_error(
            "CHATGPT_AUTH_OR_COMPOSER_REQUIRED"
        ));
    }

    #[test]
    fn only_a_router_owned_carrier_challenge_trips_its_circuit_breaker() {
        assert!(account_security_trips_router_owned_carrier(
            ChatGptBrowserChannel::RouterOwnedCarrier,
            "CHATGPT_ACCOUNT_SECURITY_REQUIRED",
        ));
        assert!(!account_security_trips_router_owned_carrier(
            ChatGptBrowserChannel::NormalChromeObserver,
            "CHATGPT_ACCOUNT_SECURITY_REQUIRED",
        ));
    }

    #[test]
    fn profile_in_use_error_is_a_stable_typed_failure() {
        assert!(is_profile_in_use_error("CHATGPT_ROUTER_PROFILE_IN_USE"));
        assert!(!is_profile_in_use_error("CHATGPT_ROUTER_PROFILE_REQUIRED"));
    }

    #[test]
    fn profile_path_is_router_owned_and_never_the_codex_playwright_profile() {
        assert_eq!(
            router_profile_path(Path::new(r"C:\Users\owner\AppData\Local")),
            PathBuf::from(r"C:\Users\owner\AppData\Local")
                .join("AIWorkRouter")
                .join("playwright-chatgpt-profile")
        );
    }

    #[test]
    fn normal_browser_connection_requires_the_narrow_connect_acknowledgement() {
        let connected: SidecarEnvelope = serde_json::from_value(json!({
            "ok": true,
            "result": { "state": "NORMAL_BROWSER_READ_ONLY_CONNECTED" }
        }))
        .unwrap();
        let wrong: SidecarEnvelope = serde_json::from_value(json!({
            "ok": true,
            "result": { "state": "TERMINAL" }
        }))
        .unwrap();
        assert!(normal_browser_connection_completed(&connected.result.unwrap()));
        assert!(!normal_browser_connection_completed(&wrong.result.unwrap()));
    }
}
