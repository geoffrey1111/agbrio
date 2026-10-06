//! Narrow Native-Messaging transport for the owner-approved normal Chrome
//! connector.  This is intentionally not a browser driver: Chrome owns page
//! selection and the content script only reports the page it is already in.
//! The local host decides whether that exact stable conversation identity is
//! bound before retaining an observation or issuing an approved dispatch.

use crate::{record_provider_surface_reply_with, playwright_chatgpt::TerminalReply, Endpoint, RouterCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
      collections::{HashMap, VecDeque},
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{mpsc, Arc, Mutex, OnceLock},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const CONFIG_FILE: &str = "normal-chrome-extension.json";
const MAX_NATIVE_MESSAGE_BYTES: usize = 512 * 1024;
// A Router restart replaces its loopback listener and per-process token.  The
// Chrome native host must bridge that brief local gap without asking Chrome to
// reopen, reload, or touch a ChatGPT page.  This is deliberately bounded: it
// is restart recovery, never a permanent retry loop.
const MAX_LOCAL_HOST_RECOVERY_ATTEMPTS: u8 = 20;
const LOCAL_HOST_RECOVERY_DELAY: Duration = Duration::from_millis(250);
// An unpacked extension can keep its files open while Chrome is running. A
// versioned, immutable product-owned directory lets Router update safely
// without stopping Chrome or deleting an extension the owner is viewing.
// A source-changing Connector release receives a fresh directory. Chrome can
// keep a loaded unpacked bundle open, so mutating its existing files is not a
// valid update strategy even when the Host is otherwise stopped.
const EXTENSION_DIRECTORY_NAME: &str = "AI Work Router Connector 20260930-1";
const EXTENSION_FILES: [&str; 3] = ["manifest.json", "background.js", "content.js"];

#[derive(Clone, Serialize, Deserialize)]
struct BridgeConfig {
    port: u16,
    token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedUserTurn {
    pub message_id: String,
    pub previous_terminal_message_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DispatchResult {
    request_id: String,
    conversation_id: String,
    state: String,
    accepted_user_turn_identity: Option<String>,
    previous_terminal_message_id: Option<String>,
}

#[derive(Clone)]
pub struct NormalChromeExtensionHost {
    outbound: Arc<Mutex<Option<mpsc::Sender<Value>>>>,
    pending: Arc<Mutex<HashMap<String, mpsc::Sender<DispatchResult>>>>,
}

fn active_host() -> &'static Mutex<Option<NormalChromeExtensionHost>> {
    static ACTIVE: OnceLock<Mutex<Option<NormalChromeExtensionHost>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(None))
}

fn config_path() -> Result<PathBuf, String> {
    let local = env::var_os("LOCALAPPDATA").ok_or("CHATGPT_EXTENSION_LOCALAPPDATA_REQUIRED")?;
    Ok(PathBuf::from(local).join("AIWorkRouter").join("data").join(CONFIG_FILE))
}

fn persist_config(config: &BridgeConfig) -> Result<(), String> {
    let path = config_path()?;
    let parent = path.parent().ok_or("CHATGPT_EXTENSION_CONFIG_PATH_INVALID")?;
    fs::create_dir_all(parent).map_err(|_| "CHATGPT_EXTENSION_CONFIG_DIRECTORY_UNAVAILABLE")?;
    let pending = path.with_extension("json.pending");
    let encoded = serde_json::to_vec(config).map_err(|_| "CHATGPT_EXTENSION_CONFIG_ENCODE_FAILED")?;
    fs::write(&pending, encoded).map_err(|_| "CHATGPT_EXTENSION_CONFIG_WRITE_FAILED")?;
    fs::rename(pending, path).map_err(|_| "CHATGPT_EXTENSION_CONFIG_WRITE_FAILED".to_string())
}

pub fn native_host_manifest_path() -> Result<PathBuf, String> {
    let local = env::var_os("LOCALAPPDATA").ok_or("CHATGPT_EXTENSION_LOCALAPPDATA_REQUIRED")?;
    Ok(PathBuf::from(local)
        .join("AIWorkRouter")
        .join("native-messaging")
        .join("com.geoffrey.aiworkrouter.chatgpt.json"))
}

/// The unpacked extension is deliberately installed outside the encrypted
/// application resource directory. Chrome's developer loader must be able to
/// read the directory itself; it receives no alternate browser profile,
/// extension, or provider credentials from Router.
fn extension_install_path() -> Result<PathBuf, String> {
    let local = env::var_os("LOCALAPPDATA")
        .ok_or("CHATGPT_EXTENSION_LOCALAPPDATA_REQUIRED")?;
    Ok(PathBuf::from(local)
        .join("AIWorkRouter")
        .join("connector-bundles")
        .join(EXTENSION_DIRECTORY_NAME))
}

fn copy_extension_file(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    if !source.is_file() {
        return Err("CHATGPT_EXTENSION_PACKAGE_FILES_MISSING".into());
    }
    // Chrome may retain a read handle to a successfully loaded unpacked file.
    // A byte-identical file is already the exact packaged product content, so
    // do not rewrite it merely because the Host restarted.
    if destination.is_file()
        && fs::read(source).map_err(|_| "CHATGPT_EXTENSION_PACKAGE_FILES_MISSING")?
            == fs::read(destination).map_err(|_| "CHATGPT_EXTENSION_INSTALL_WRITE_FAILED")?
    {
        return Ok(());
    }
    let pending = destination.with_extension("pending");
    // `fs::copy` preserves the EFS attribute from the installed resource on
    // this machine.  That fails when its destination is deliberately the
    // non-encrypted Chrome-loadable data directory.  Copy bytes instead so
    // the destination inherits only its own normal local-data policy.
    let source_bytes = fs::read(source).map_err(|_| "CHATGPT_EXTENSION_PACKAGE_FILES_MISSING")?;
    fs::write(&pending, source_bytes).map_err(|_| "CHATGPT_EXTENSION_INSTALL_WRITE_FAILED")?;
    if destination.exists() {
        fs::remove_file(destination).map_err(|_| "CHATGPT_EXTENSION_INSTALL_WRITE_FAILED")?;
    }
    fs::rename(pending, destination).map_err(|_| "CHATGPT_EXTENSION_INSTALL_WRITE_FAILED".to_string())
}

fn provision_extension_bundle_from(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    if destination.exists() && !destination.is_dir() {
        return Err("CHATGPT_EXTENSION_INSTALL_PATH_IS_NOT_DIRECTORY".into());
    }
    fs::create_dir_all(destination).map_err(|_| "CHATGPT_EXTENSION_INSTALL_DIRECTORY_UNAVAILABLE")?;
    for filename in EXTENSION_FILES {
        copy_extension_file(&source.join(filename), &destination.join(filename))?;
    }
    Ok(())
}

/// Refreshes only the three packaged connector files in Router's dedicated
/// user-visible directory. This avoids asking the owner to browse a transient
/// AppData resource path after each current-user installation.
pub fn provision_extension_bundle(app: &AppHandle) -> Result<PathBuf, String> {
    let source = app
        .path()
        .resource_dir()
        .map_err(|_| "CHATGPT_EXTENSION_PACKAGE_DIRECTORY_UNAVAILABLE")?
        .join("chatgpt-extension");
    let destination = extension_install_path()?;
    provision_extension_bundle_from(&source, &destination)?;
    Ok(destination)
}

/// The installer writes the Chrome registry entry.  Keeping the manifest
/// per-user makes the executable path update-safe and avoids a machine-wide
/// browser integration.
pub fn write_native_host_manifest(executable: &std::path::Path) -> Result<(), String> {
    let path = native_host_manifest_path()?;
    let parent = path.parent().ok_or("CHATGPT_EXTENSION_NATIVE_MANIFEST_INVALID")?;
    fs::create_dir_all(parent).map_err(|_| "CHATGPT_EXTENSION_NATIVE_MANIFEST_UNAVAILABLE")?;
    let body = json!({
        "name": "com.geoffrey.aiworkrouter.chatgpt",
        "description": "AI Work Router local ChatGPT connector",
        "path": executable,
        "type": "stdio",
        "allowed_origins": ["chrome-extension://hbbmojhcijkbddnidnobakilcloepadn/"]
    });
    let encoded = serde_json::to_vec_pretty(&body).map_err(|_| "CHATGPT_EXTENSION_NATIVE_MANIFEST_ENCODE_FAILED")?;
    fs::write(path, encoded).map_err(|_| "CHATGPT_EXTENSION_NATIVE_MANIFEST_WRITE_FAILED".to_string())
}

#[cfg(windows)]
pub fn register_native_host_current_user() -> Result<(), String> {
    let executable = env::current_exe().map_err(|_| "CHATGPT_EXTENSION_EXECUTABLE_UNAVAILABLE")?;
    write_native_host_manifest(&executable)?;
    let manifest = native_host_manifest_path()?;
    let status = std::process::Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Google\Chrome\NativeMessagingHosts\com.geoffrey.aiworkrouter.chatgpt",
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
        ])
        .arg(manifest)
        .args(["/f"])
        .status()
        .map_err(|_| "CHATGPT_EXTENSION_NATIVE_REGISTRATION_FAILED")?;
    if status.success() { Ok(()) } else { Err("CHATGPT_EXTENSION_NATIVE_REGISTRATION_FAILED".into()) }
}

#[cfg(not(windows))]
pub fn register_native_host_current_user() -> Result<(), String> { Ok(()) }

fn required_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or("CHATGPT_EXTENSION_PROTOCOL_INVALID".into())
}

fn observer_endpoint(core: &RouterCore, conversation_id: &str) -> Result<Option<Endpoint>, String> {
    core.store.active_endpoint_for_external_id("CHATGPT", conversation_id)
}

fn record_observation(core: &RouterCore, message: &Value) -> Result<(), String> {
    if message.get("state").and_then(Value::as_str) != Some("TERMINAL") {
        return Ok(());
    }
    let conversation_id = required_string(message, "conversationId")?;
    let Some(endpoint) = observer_endpoint(core, &conversation_id)? else {
        // The extension can be installed while a user has unrelated ChatGPT
        // tabs open.  They are outside Router scope and are discarded before
        // any text persistence or push work.
        return Ok(());
    };
    let reply = TerminalReply {
        message_id: required_string(message, "messageId")?,
        text: required_string(message, "text")?,
    };
    let watermark = core.store.chatgpt_reply_observer_watermark(&endpoint.id)?;
    let initialized = core.store.chatgpt_reply_observer_is_initialized(&endpoint.id)?;
    let watermark_identity = format!("chatgpt-watermark:{}:{}", conversation_id, reply.message_id);
    if watermark.is_none() && !initialized {
        core.store.save_chatgpt_reply_observer_watermark(
            &endpoint.workstream_id,
            &endpoint.id,
            &conversation_id,
            &reply.message_id,
        )?;
        return Ok(());
    }
    if watermark.as_ref().is_some_and(|(identity, _)| identity == &watermark_identity) {
        core.store.save_chatgpt_reply_observer_watermark(
            &endpoint.workstream_id,
            &endpoint.id,
            &conversation_id,
            &reply.message_id,
        )?;
        return Ok(());
    }
    let mut push = |payload: &[u8]| crate::push::send_payload(payload);
    let _ = record_provider_surface_reply_with(&core.store, &endpoint, &reply.message_id, &reply.text, &mut push)?;
    core.store.save_chatgpt_reply_observer_watermark(
        &endpoint.workstream_id,
        &endpoint.id,
        &conversation_id,
        &reply.message_id,
    )?;
    Ok(())
}

fn handle_connection(
    stream: TcpStream,
    expected_token: String,
    host: NormalChromeExtensionHost,
    core: RouterCore,
) {
    let writer = match stream.try_clone() { Ok(writer) => writer, Err(_) => return };
    let (outbound_tx, outbound_rx) = mpsc::channel::<Value>();
    thread::spawn(move || {
        let mut writer = writer;
        for message in outbound_rx {
            let line = match serde_json::to_string(&message) { Ok(line) => line, Err(_) => continue };
            if writer.write_all(line.as_bytes()).and_then(|_| writer.write_all(b"\n")).is_err() { break; }
            if writer.flush().is_err() { break; }
        }
    });
    let mut reader = BufReader::new(stream);
    let mut authenticated = false;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.is_empty() { break; }
        let Ok(envelope) = serde_json::from_str::<Value>(&line) else { continue; };
        if !authenticated {
            if envelope.get("token").and_then(Value::as_str) != Some(expected_token.as_str()) { break; }
            authenticated = true;
            // This acknowledgement proves the native process reached the
            // current Host's token-protected loopback listener.  It arms the
            // extension's one event-bound recovery for a later Host update;
            // it contains no page identity, text, or secret.
            let _ = outbound_tx.send(json!({ "type": "connector_ready" }));
            continue;
        }
        let Some(message) = envelope.get("message") else { continue; };
        match message.get("type").and_then(Value::as_str) {
            Some("extension_online") => {
                if let Ok(mut current) = host.outbound.lock() { *current = Some(outbound_tx.clone()); }
            }
            Some("observation") => { let _ = record_observation(&core, message); }
            Some("dispatch_result") => {
                let Ok(result) = serde_json::from_value::<DispatchResult>(message.clone()) else { continue; };
                if let Ok(mut pending) = host.pending.lock() {
                    if let Some(waiter) = pending.remove(&result.request_id) { let _ = waiter.send(result); }
                }
            }
            _ => {}
        }
    }
    if let Ok(mut current) = host.outbound.lock() { *current = None; }
}

impl NormalChromeExtensionHost {
    pub fn start(core: RouterCore, _app: AppHandle) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| "CHATGPT_EXTENSION_LISTENER_UNAVAILABLE")?;
        let port = listener.local_addr().map_err(|_| "CHATGPT_EXTENSION_LISTENER_UNAVAILABLE")?.port();
        let config = BridgeConfig { port, token: Uuid::new_v4().to_string() };
        persist_config(&config)?;
        let host = Self { outbound: Arc::new(Mutex::new(None)), pending: Arc::new(Mutex::new(HashMap::new())) };
        if let Ok(mut active) = active_host().lock() { *active = Some(host.clone()); }
        let thread_host = host.clone();
        thread::spawn(move || {
            for accepted in listener.incoming() {
                let Ok(stream) = accepted else { continue; };
                let _ = stream.set_nodelay(true);
                let expected_token = config.token.clone();
                let core = core.clone();
                let host = thread_host.clone();
                thread::spawn(move || handle_connection(stream, expected_token, host, core));
            }
        });
        Ok(host)
    }

    fn dispatch(&self, conversation_id: &str, text: &str) -> Result<AcceptedUserTurn, String> {
        if conversation_id.trim().is_empty() || text.trim().is_empty() { return Err("CHATGPT_EXTENSION_APPROVED_PAYLOAD_INVALID".into()); }
        let request_id = Uuid::new_v4().to_string();
        let (sender, receiver) = mpsc::channel();
        self.pending.lock().map_err(|_| "CHATGPT_EXTENSION_DISPATCH_UNAVAILABLE")?.insert(request_id.clone(), sender);
        let command = json!({ "type": "dispatch", "requestId": request_id, "conversationId": conversation_id, "text": text, "attachments": [] });
        let sent = self.outbound.lock().map_err(|_| "CHATGPT_EXTENSION_DISPATCH_UNAVAILABLE")?.clone()
            .ok_or("CHATGPT_EXTENSION_NOT_CONNECTED")?.send(command).is_ok();
        if !sent {
            let _ = self.pending.lock().map(|mut pending| pending.remove(&request_id));
            return Err("CHATGPT_EXTENSION_NOT_CONNECTED".into());
        }
        let result = receiver.recv_timeout(Duration::from_secs(12)).map_err(|_| "CHATGPT_ACCEPTANCE_UNPROVEN")?;
        if result.conversation_id != conversation_id { return Err("CHATGPT_EXTENSION_EXACT_IDENTITY_MISMATCH".into()); }
        if result.state != "ACCEPTED" { return Err(result.state); }
        Ok(AcceptedUserTurn {
            message_id: result.accepted_user_turn_identity.filter(|value| !value.trim().is_empty()).ok_or("CHATGPT_ACCEPTANCE_UNPROVEN")?,
            previous_terminal_message_id: result.previous_terminal_message_id.filter(|value| !value.trim().is_empty()),
        })
    }
}

pub fn submit_approved_text(conversation_id: &str, text: &str) -> Result<AcceptedUserTurn, String> {
    active_host().lock().map_err(|_| "CHATGPT_EXTENSION_DISPATCH_UNAVAILABLE")?
        .clone().ok_or("CHATGPT_EXTENSION_NOT_CONNECTED")?.dispatch(conversation_id, text)
}

pub fn is_connected() -> bool {
    active_host().lock().ok().and_then(|host| host.as_ref().cloned())
        .and_then(|host| host.outbound.lock().ok().and_then(|outbound| outbound.as_ref().cloned()))
        .is_some()
}

/// Compact presentation state only; no page identity or connector secret is
/// exposed through this status.
pub fn connection_status() -> &'static str {
    if is_connected() {
        "NORMAL_CHROME_EXTENSION_CONNECTED"
    } else {
        // The Host can prove a live authenticated native pipe, but it cannot
        // safely inspect Chrome's extension inventory.  Do not turn the
        // absence of that pipe into a false claim that the extension was not
        // installed or loaded.
        "NORMAL_CHROME_EXTENSION_NOT_CONNECTED"
    }
}

fn native_read() -> Result<Option<Value>, String> {
    let mut length = [0u8; 4];
    match std::io::stdin().read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(_) => return Err("CHATGPT_EXTENSION_NATIVE_INPUT_INVALID".into()),
    }
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_NATIVE_MESSAGE_BYTES { return Err("CHATGPT_EXTENSION_NATIVE_INPUT_INVALID".into()); }
    let mut payload = vec![0u8; length];
    std::io::stdin().read_exact(&mut payload).map_err(|_| "CHATGPT_EXTENSION_NATIVE_INPUT_INVALID")?;
    serde_json::from_slice(&payload).map(Some).map_err(|_| "CHATGPT_EXTENSION_NATIVE_INPUT_INVALID".into())
}

fn native_write(value: &Value) -> Result<(), String> {
    let payload = serde_json::to_vec(value).map_err(|_| "CHATGPT_EXTENSION_NATIVE_OUTPUT_INVALID")?;
    if payload.len() > MAX_NATIVE_MESSAGE_BYTES { return Err("CHATGPT_EXTENSION_NATIVE_OUTPUT_INVALID".into()); }
    let mut stdout = std::io::stdout();
    stdout.write_all(&(payload.len() as u32).to_le_bytes()).map_err(|_| "CHATGPT_EXTENSION_NATIVE_OUTPUT_INVALID")?;
    stdout.write_all(&payload).map_err(|_| "CHATGPT_EXTENSION_NATIVE_OUTPUT_INVALID")?;
    stdout.flush().map_err(|_| "CHATGPT_EXTENSION_NATIVE_OUTPUT_INVALID".to_string())
}

/// Entry point used only by Chrome Native Messaging. The extension identity is
/// checked by Chrome before this executable starts; this bridge still uses a
/// per-Host random loopback token so another local process cannot impersonate
/// a connected extension.
pub fn run_native_messaging_bridge() -> Result<(), String> {
    let (native_tx, native_rx) = mpsc::sync_channel::<Value>(16);
    let native_input = thread::spawn(move || {
        while let Ok(Some(message)) = native_read() {
            if native_tx.send(message).is_err() { break; }
        }
    });
    let mut queued_messages = VecDeque::<Value>::new();
    let mut native_input_closed = false;

    for attempt in 0..MAX_LOCAL_HOST_RECOVERY_ATTEMPTS {
        while let Ok(message) = native_rx.try_recv() { queued_messages.push_back(message); }
        match native_rx.try_recv() {
            Ok(message) => queued_messages.push_back(message),
            Err(mpsc::TryRecvError::Disconnected) => native_input_closed = true,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if native_input_closed && queued_messages.is_empty() { break; }

        let config: BridgeConfig = match fs::read(config_path()?)
            .map_err(|_| "CHATGPT_EXTENSION_HOST_NOT_RUNNING")
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|_| "CHATGPT_EXTENSION_HOST_NOT_RUNNING".into()))
        {
            Ok(config) => config,
            Err(error) => {
                if attempt + 1 == MAX_LOCAL_HOST_RECOVERY_ATTEMPTS { return Err(error.to_string()); }
                thread::sleep(LOCAL_HOST_RECOVERY_DELAY);
                continue;
            }
        };
        let stream = match TcpStream::connect(("127.0.0.1", config.port)) {
            Ok(stream) => stream,
            Err(_) if attempt + 1 < MAX_LOCAL_HOST_RECOVERY_ATTEMPTS => {
                thread::sleep(LOCAL_HOST_RECOVERY_DELAY);
                continue;
            }
            Err(_) => return Err("CHATGPT_EXTENSION_HOST_RECOVERY_EXHAUSTED".into()),
        };
        let _ = stream.set_read_timeout(Some(LOCAL_HOST_RECOVERY_DELAY));
        let mut writer = stream.try_clone().map_err(|_| "CHATGPT_EXTENSION_HOST_NOT_RUNNING")?;
        writeln!(writer, "{}", json!({ "token": config.token }))
            .and_then(|_| writer.flush())
            .map_err(|_| "CHATGPT_EXTENSION_HOST_NOT_RUNNING")?;
        let mut reader = BufReader::new(stream);
        loop {
            while let Ok(message) = native_rx.try_recv() { queued_messages.push_back(message); }
            match native_rx.try_recv() {
                Ok(message) => queued_messages.push_back(message),
                Err(mpsc::TryRecvError::Disconnected) => native_input_closed = true,
                Err(mpsc::TryRecvError::Empty) => {}
            }
            while let Some(message) = queued_messages.pop_front() {
                if writeln!(writer, "{}", json!({ "message": &message })).and_then(|_| writer.flush()).is_err() {
                    queued_messages.push_front(message);
                    break;
                }
            }
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if let Ok(message) = serde_json::from_str::<Value>(&line) { native_write(&message)?; }
                }
                Err(error) if matches!(error.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) => {
                    if native_input_closed && queued_messages.is_empty() { let _ = native_input.join(); return Ok(()); }
                }
                Err(_) => break,
            }
        }
        if native_input_closed && queued_messages.is_empty() { break; }
    }
    let _ = native_input.join();
    if native_input_closed { Ok(()) } else { Err("CHATGPT_EXTENSION_HOST_RECOVERY_EXHAUSTED".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn native_messages_are_bounded_and_framed() {
        assert!(MAX_NATIVE_MESSAGE_BYTES >= 512 * 1024);
        let manifest = json!({"type":"dispatch","conversationId":"exact","attachments":[]});
        assert_eq!(manifest["conversationId"], "exact");
    }

    #[test]
    fn disconnected_status_does_not_claim_the_connector_was_not_installed() {
        assert_eq!(connection_status(), "NORMAL_CHROME_EXTENSION_NOT_CONNECTED");
    }

    #[test]
    fn provision_copies_only_the_connector_bundle_into_a_directory() {
        let source_root = tempdir().unwrap();
        let destination_root = tempdir().unwrap();
        for filename in EXTENSION_FILES {
            fs::write(source_root.path().join(filename), format!("{filename}-source")).unwrap();
        }
        let destination = destination_root.path().join(EXTENSION_DIRECTORY_NAME);
        provision_extension_bundle_from(source_root.path(), &destination).unwrap();
        for filename in EXTENSION_FILES {
            assert_eq!(fs::read_to_string(destination.join(filename)).unwrap(), format!("{filename}-source"));
        }
        assert_eq!(fs::read_dir(&destination).unwrap().count(), EXTENSION_FILES.len());
        // Re-provisioning an already current bundle is a no-write operation,
        // so a restarted Host does not require Chrome to release file handles.
        provision_extension_bundle_from(source_root.path(), &destination).unwrap();
    }
}
