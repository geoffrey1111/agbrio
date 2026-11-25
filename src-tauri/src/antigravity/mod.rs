//! Native Antigravity ACP client. One lazy process is shared by all sessions.
//! Native credentials, tools and permissions stay inside agy. No model proxy.
mod rpc;
use crate::{
    host_application::RouterCore,
    watch_chat::{ChatCommand, ReplyOptions},
};
use router_core::store::{
    codex_watch::WatchSnapshot, managed_conversations::ManagedConversation,
    watch_reply::WatchReply, RouterStore,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};
pub(crate) const PREFIX: &str = "antigravity:";
#[derive(Default)]
pub(crate) struct Service {
    client: Mutex<Option<Arc<Client>>>,
}
struct Active {
    reply: String,
    control: String,
    agent_message: String,
    user_message: String,
    user_text: String,
}
struct Live {
    key: String,
    cwd: String,
    active: Option<Active>,
    _configuration: Value,
    replay: Option<Vec<(String, String)>>,
    replay_error: bool,
}
struct Permission {
    native: Value,
    session: String,
    generation: i64,
    options: Vec<Value>,
    attempted: bool,
}
struct Client {
    rpc: Arc<rpc::Rpc>,
    epoch: String,
    sessions: Mutex<HashMap<String, Live>>,
    permissions: Mutex<HashMap<String, Permission>>,
    gate: Mutex<()>,
    capabilities: Value,
}
fn executable() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("AGBRIO_AGY_EXECUTABLE") {
        let path = PathBuf::from(path);
        if !path.is_absolute() || !path.is_file() {
            return Err("ANTIGRAVITY_CLI_EXPLICIT_PATH_INVALID".into());
        }
        return std::fs::canonicalize(path)
            .map_err(|_| "ANTIGRAVITY_CLI_EXPLICIT_PATH_INVALID".into());
    }
    let path = None::<PathBuf>
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("agy/bin/agy.exe"))
        })
        .ok_or("ANTIGRAVITY_CLI_NOT_INSTALLED")?;
    // The standard agy installer may be owner-directed to another drive. Resolve
    // its executable via PATH only; never run a shell or download a replacement.
    if path.is_absolute() && path.is_file() {
        return std::fs::canonicalize(path).map_err(|_| "ANTIGRAVITY_CLI_NOT_INSTALLED".into());
    }
    let mut paths = std::env::var_os("PATH").into_iter().collect::<Vec<_>>();
    #[cfg(windows)]
    {
        if let Ok(environment) =
            winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey("Environment")
        {
            if let Ok(path) = environment.get_value::<String, _>("Path") {
                paths.push(path.into());
            }
        }
    }
    for paths in paths {
        for p in std::env::split_paths(&paths) {
            let candidate = p.join("agy.exe");
            if p.is_absolute() && candidate.is_file() {
                return std::fs::canonicalize(candidate)
                    .map_err(|_| "ANTIGRAVITY_CLI_NOT_INSTALLED".into());
            }
        }
    }
    Err("ANTIGRAVITY_CLI_NOT_INSTALLED".into())
}
pub(crate) fn models() -> Result<Value, String> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Mutex<Option<(Instant, Value)>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "ANTIGRAVITY_MODEL_CATALOG_UNAVAILABLE")?;
    if let Some((at, v)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(60) {
            return Ok(v.clone());
        }
    }
    let mut cmd = Command::new(executable()?);
    cmd.arg("models")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().map_err(|_| "ANTIGRAVITY_CLI_UNAVAILABLE")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("ANTIGRAVITY_MODEL_CATALOG_UNAVAILABLE")?;
    let reader = std::thread::spawn(move || {
        let mut raw = Vec::new();
        let _ = stdout.take(65537).read_to_end(&mut raw);
        raw
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let raw = reader
        .join()
        .map_err(|_| "ANTIGRAVITY_MODEL_CATALOG_UNAVAILABLE")?;
    if !success || raw.len() > 65536 {
        return Err("ANTIGRAVITY_MODEL_CATALOG_UNAVAILABLE".into());
    }
    let mut rows = Vec::new();
    for line in String::from_utf8(raw)
        .map_err(|_| "ANTIGRAVITY_MODEL_CATALOG_INVALID")?
        .lines()
    {
        let Some((id, label)) = line.split_once('\t') else {
            continue;
        };
        if id.is_empty()
            || id.len() > 200
            || !id.bytes().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b':' | b'[' | b']')
            })
        {
            return Err("ANTIGRAVITY_MODEL_CATALOG_INVALID".into());
        }
        let effort = id
            .rsplit('-')
            .next()
            .filter(|v| matches!(*v, "low" | "medium" | "high" | "xhigh" | "max"));
        rows.push(json!({"model":id,"displayName":label,"supportedReasoningEfforts":effort.map(|e|vec![json!({"reasoningEffort":e})]).unwrap_or_default()}));
    }
    if rows.is_empty() || rows.len() > 200 {
        return Err("ANTIGRAVITY_MODEL_CATALOG_UNAVAILABLE".into());
    }
    let v = json!({"models":rows,"selection":"NATIVE_MODEL_VARIANT","startsModelTurn":false});
    *cache = Some((Instant::now(), v.clone()));
    Ok(v)
}
pub(crate) fn preflight(model: Option<&str>, effort: Option<&str>) -> Result<(), String> {
    let catalog = models()?;
    if model.is_none() && effort.is_some() {
        return Err("ANTIGRAVITY_MODEL_REQUIRED".into());
    }
    if let Some(model) = model {
        let row = catalog["models"]
            .as_array()
            .and_then(|a| a.iter().find(|v| v["model"] == model))
            .ok_or("ANTIGRAVITY_MODEL_UNAVAILABLE")?;
        if let Some(effort) = effort {
            if !row["supportedReasoningEfforts"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v["reasoningEffort"] == effort))
            {
                return Err("ANTIGRAVITY_EFFORT_UNAVAILABLE".into());
            }
        }
    }
    Ok(())
}
fn service(core: &RouterCore) -> Result<Arc<Service>, String> {
    Ok(core
        .session
        .lock()
        .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?
        .antigravity
        .clone())
}
fn client(core: &RouterCore) -> Result<Arc<Client>, String> {
    let service = service(core)?;
    let mut slot = service
        .client
        .lock()
        .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?;
    if let Some(c) = slot.as_ref().filter(|c| !c.rpc.closed()) {
        return Ok(c.clone());
    }
    let target: Arc<Mutex<Weak<Client>>> = Arc::new(Mutex::new(Weak::new()));
    let weak = target.clone();
    let store = Arc::downgrade(&core.store);
    let mut cmd = Command::new(executable()?);
    cmd.args(["acp", "--stdio", "--disable-telemetry"]);
    let rpc = rpc::Rpc::start(
        cmd,
        Arc::new(move |event| {
            let c = weak.lock().ok().and_then(|v| v.upgrade());
            if let (Some(c), Some(store)) = (c, store.upgrade()) {
                c.event(&store, event);
            }
        }),
    )?;
    let init=rpc.request("initialize",json!({"protocolVersion":1,"clientCapabilities":{},"clientInfo":{"name":"Agbrio","version":crate::app_control::version()}}))?;
    if init["protocolVersion"] != 1 {
        return Err("ANTIGRAVITY_PROTOCOL_UNSUPPORTED".into());
    }
    let c = Arc::new(Client {
        rpc,
        epoch: uuid::Uuid::new_v4().to_string(),
        sessions: Mutex::new(HashMap::new()),
        permissions: Mutex::new(HashMap::new()),
        gate: Mutex::new(()),
        capabilities: init["agentCapabilities"].clone(),
    });
    *target
        .lock()
        .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")? = Arc::downgrade(&c);
    *slot = Some(c.clone());
    Ok(c)
}
fn configuration(v: &Value) -> Value {
    json!({"models":v["models"],"configOptions":v["configOptions"],"modes":v["modes"]})
}
impl Client {
    fn register(
        &self,
        store: &RouterStore,
        sid: &str,
        cwd: &str,
        config: Value,
    ) -> Result<String, String> {
        if sid.is_empty() || sid.len() > 256 || sid.chars().any(char::is_control) {
            return Err("ANTIGRAVITY_NATIVE_IDENTITY_INVALID".into());
        }
        let key = format!("{PREFIX}{sid}");
        store.enable_codex_watch(
            &key,
            "新 Antigravity 对话",
            cwd,
            &WatchSnapshot {
                state: "IDLE".into(),
                turn_id: None,
                item_id: None,
                text: String::new(),
            },
        )?;
        store.register_managed_conversation(&ManagedConversation {
            thread_id: key.clone(),
            native_id: sid.into(),
            provider: "ANTIGRAVITY".into(),
            cwd: cwd.into(),
            epoch: self.epoch.clone(),
            configuration: config.clone(),
        })?;
        self.sessions
            .lock()
            .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?
            .insert(
                sid.into(),
                Live {
                    key: key.clone(),
                    cwd: cwd.into(),
                    active: None,
                    _configuration: config,
                    replay: None,
                    replay_error: false,
                },
            );
        Ok(key)
    }
    fn ensure(&self, core: &RouterCore, key: &str) -> Result<String, String> {
        let meta = core.store.managed_conversation(key)?;
        if meta.provider != "ANTIGRAVITY" || format!("{PREFIX}{}", meta.native_id) != key {
            return Err("ANTIGRAVITY_TARGET_CHANGED".into());
        }
        let _gate = self
            .gate
            .lock()
            .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?;
        if self
            .sessions
            .lock()
            .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?
            .contains_key(&meta.native_id)
        {
            return Ok(meta.native_id);
        }
        // Loading the exact native conversation replays its history, never a prompt.
        // Reject a native ownership/load failure; there is no takeover or clone path.
        self.register(&core.store, &meta.native_id, &meta.cwd, meta.configuration)?;
        self.sessions
            .lock()
            .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?
            .get_mut(&meta.native_id)
            .ok_or("ANTIGRAVITY_TARGET_CHANGED")?
            .replay = Some(Vec::new());
        let loaded = self.rpc.request(
            "session/load",
            json!({"sessionId":meta.native_id,"cwd":meta.cwd,"mcpServers":[]}),
        );
        if loaded.is_err() {
            self.sessions
                .lock()
                .ok()
                .map(|mut s| s.remove(&meta.native_id));
            return Err("ANTIGRAVITY_NATIVE_LOAD_UNAVAILABLE".into());
        }
        let (replay, invalid) = self
            .sessions
            .lock()
            .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?
            .get_mut(&meta.native_id)
            .map(|s| (s.replay.take(), s.replay_error))
            .ok_or("ANTIGRAVITY_NATIVE_HISTORY_UNCONFIRMED")?;
        if invalid {
            self.sessions
                .lock()
                .ok()
                .map(|mut s| s.remove(&meta.native_id));
            return Err("ANTIGRAVITY_NATIVE_HISTORY_LIMIT".into());
        }
        let replay = replay.ok_or("ANTIGRAVITY_NATIVE_HISTORY_UNCONFIRMED")?;
        if let Err(error) = core
            .store
            .replace_managed_history(&key, &self.epoch, &replay)
        {
            self.sessions
                .lock()
                .ok()
                .map(|mut s| s.remove(&meta.native_id));
            return Err(error);
        }
        Ok(meta.native_id)
    }
    fn snapshot(&self, store: &RouterStore, sid: &str, state: &str) -> Result<(), String> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?;
        let live = sessions.get(sid).ok_or("ANTIGRAVITY_TARGET_CHANGED")?;
        let w = store.codex_chat_context(&live.key)?;
        let messages = store.managed_public_history(&live.key, None)?;
        let last = messages["messages"]
            .as_array()
            .and_then(|a| a.iter().rev().find(|m| m["role"] == "assistant"));
        store.record_codex_watch(
            &live.key,
            w.generation,
            &WatchSnapshot {
                state: state.into(),
                turn_id: live.active.as_ref().map(|a| a.control.clone()),
                item_id: last.and_then(|m| m["id"].as_str()).map(str::to_owned),
                text: last
                    .and_then(|m| m["text"].as_str())
                    .unwrap_or_default()
                    .chars()
                    .take(6000)
                    .collect(),
            },
        )?;
        Ok(())
    }
    fn event(&self, store: &RouterStore, event: Value) {
        if event["method"] == "agbrio/disconnected" {
            if let Ok(s) = self.sessions.lock() {
                for live in s.values() {
                    if let Some(active) = &live.active {
                        let _ = store.finish_watch_reply(
                            &active.reply,
                            "UNKNOWN",
                            None,
                            Some("ANTIGRAVITY_NATIVE_DISCONNECTED_UNCONFIRMED"),
                        );
                    }
                    if let Ok(w) = store.codex_chat_context(&live.key) {
                        let _ = store.codex_watch_unavailable_reason(
                            &live.key,
                            w.generation,
                            "ANTIGRAVITY_NATIVE_DISCONNECTED_UNCONFIRMED",
                        );
                    }
                }
            }
            return;
        }
        if event["method"] == "session/request_permission" {
            let Some(sid) = event["params"]["sessionId"].as_str() else {
                let _ = self.rpc.decline(&event["id"]);
                return;
            };
            let target = self
                .sessions
                .lock()
                .ok()
                .and_then(|s| s.get(sid).map(|v| v.key.clone()));
            let Some(key) = target else {
                let _ = self
                    .rpc
                    .answer(&event["id"], json!({"outcome":{"outcome":"cancelled"}}));
                return;
            };
            if let Ok(w) = store.codex_chat_context(&key) {
                let id = uuid::Uuid::new_v4().to_string();
                let options = event["params"]["options"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if let Ok(mut p) = self.permissions.lock() {
                    p.insert(
                        id,
                        Permission {
                            native: event["id"].clone(),
                            session: sid.into(),
                            generation: w.generation,
                            options,
                            attempted: false,
                        },
                    );
                }
                let _ = self.snapshot(store, sid, "ACTION_REQUIRED");
            }
            return;
        }
        if event["method"] != "session/update" {
            if event.get("id").is_some() {
                let _ = self.rpc.decline(&event["id"]);
            }
            return;
        }
        let Some(sid) = event["params"]["sessionId"].as_str() else {
            return;
        };
        let update = &event["params"]["update"];
        let target = self.sessions.lock().ok().and_then(|s| {
            s.get(sid).map(|v| {
                (
                    v.key.clone(),
                    v.cwd.clone(),
                    v.active.as_ref().map(|a| {
                        (
                            a.agent_message.clone(),
                            a.user_message.clone(),
                            a.control.clone(),
                        )
                    }),
                )
            })
        });
        let Some((key, cwd, active)) = target else {
            return;
        };
        match update["sessionUpdate"].as_str() {
            Some("session_info_update") => {
                if let Some(title) = update["title"].as_str().filter(|v| {
                    !v.trim().is_empty()
                        && v.chars().count() <= 120
                        && !v.chars().any(char::is_control)
                }) {
                    if let Ok(w) = store.codex_chat_context(&key) {
                        let _ = store.refresh_codex_watch_label(&key, w.generation, &cwd, title);
                    }
                }
            }
            Some(kind @ ("agent_message_chunk" | "user_message_chunk")) => {
                let role = if kind == "agent_message_chunk" {
                    "assistant"
                } else {
                    "user"
                };
                if let Some(text) = update["content"]["text"].as_str() {
                    if role == "assistant" {
                        if let Some((id, submitted)) = self.sessions.lock().ok().and_then(|s| {
                            s.get(sid).and_then(|l| {
                                l.active
                                    .as_ref()
                                    .map(|a| (a.user_message.clone(), a.user_text.clone()))
                            })
                        }) {
                            if store.managed_public_history(&key, None).ok().and_then(|v| {
                                v["messages"]
                                    .as_array()
                                    .map(|a| a.iter().any(|v| v["id"] == id))
                            }) != Some(true)
                            {
                                let _ = store.managed_append_text(
                                    &key,
                                    &self.epoch,
                                    &id,
                                    "user",
                                    &submitted,
                                );
                            }
                        }
                    }
                    if active.is_none() {
                        if let Ok(mut sessions) = self.sessions.lock() {
                            if let Some(live) = sessions.get_mut(sid) {
                                if live.replay_error {
                                    return;
                                }
                                let Some(replay) = live.replay.as_mut() else {
                                    return;
                                };
                                let bytes: usize = replay.iter().map(|(_, t)| t.len()).sum();
                                if bytes + text.len() > 8 * 1024 * 1024
                                    || replay.len() > 10000
                                    || replay.last().is_some_and(|(r, t)| {
                                        r == role && t.len() + text.len() > 500000
                                    })
                                {
                                    live.replay_error = true;
                                    return;
                                }
                                if let Some((r, t)) = replay.last_mut().filter(|(r, _)| r == role) {
                                    let _ = r;
                                    t.push_str(text);
                                } else {
                                    replay.push((role.into(), text.into()));
                                }
                            }
                        }
                        return;
                    }
                    let id = active
                        .as_ref()
                        .map(|(agent, user, _)| {
                            if role == "assistant" {
                                agent.clone()
                            } else {
                                user.clone()
                            }
                        })
                        .unwrap_or_else(|| format!("acp-history:{}", uuid::Uuid::new_v4()));
                    if store
                        .managed_append_text(&key, &self.epoch, &id, role, text)
                        .is_err()
                    {
                        if let Ok(w) = store.codex_chat_context(&key) {
                            let _ = store.codex_watch_unavailable_reason(
                                &key,
                                w.generation,
                                "ANTIGRAVITY_PUBLIC_MESSAGE_UNCONFIRMED",
                            );
                        }
                        return;
                    }
                }
                if active.is_some() {
                    let _ = self.snapshot(store, sid, "RUNNING");
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn catalog(
    core: &RouterCore,
) -> Result<crate::host_application::ExistingCodexThreadCatalog, String> {
    let c = client(core)?;
    if c.capabilities
        .pointer("/sessionCapabilities/list")
        .is_none()
    {
        return Err("ANTIGRAVITY_NATIVE_LIST_UNSUPPORTED".into());
    }
    let mut threads = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..10 {
        let v = c.rpc.request("session/list", json!({"cursor":cursor}))?;
        let rows = v["sessions"]
            .as_array()
            .filter(|a| a.len() <= 1000)
            .ok_or("ANTIGRAVITY_NATIVE_CATALOG_INVALID")?;
        for row in rows {
            let sid = row["sessionId"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() < 240 && !s.chars().any(char::is_control))
                .ok_or("ANTIGRAVITY_NATIVE_IDENTITY_INVALID")?;
            let cwd = row["cwd"]
                .as_str()
                .filter(|d| std::path::Path::new(d).is_absolute())
                .ok_or("ANTIGRAVITY_NATIVE_ROOT_INVALID")?;
            if !seen.insert(sid.to_string()) {
                continue;
            }
            let label = row["title"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("未命名 Antigravity 对话");
            threads.push(crate::host_application::ExistingCodexThreadCandidate {
                id: format!("{PREFIX}{sid}"),
                label: label.chars().take(120).collect(),
                preview: Some("Antigravity".into()),
                updated_at: None,
                project_provenance: Some(cwd.into()),
                recency_at: None,
                project_id: None,
                project_label: None,
                project_status: "PROJECTLESS".into(),
            });
        }
        cursor = v["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            return Ok(crate::host_application::ExistingCodexThreadCatalog {
                threads,
                complete: true,
            });
        }
    }
    Ok(crate::host_application::ExistingCodexThreadCatalog {
        threads,
        complete: false,
    })
}
pub(crate) fn enable(core: &RouterCore, key: &str) -> Result<(), String> {
    if core.store.codex_chat_context(key).is_ok() {
        client(core)?.ensure(core, key)?;
        return Ok(());
    }
    if core.store.managed_conversation(key).is_ok() {
        let meta = core.store.managed_conversation(key)?;
        let c = client(core)?;
        c.register(&core.store, &meta.native_id, &meta.cwd, meta.configuration)?;
        c.sessions
            .lock()
            .ok()
            .map(|mut s| s.remove(&meta.native_id));
        c.ensure(core, key)?;
        return Ok(());
    }
    let row = catalog(core)?
        .threads
        .into_iter()
        .find(|r| r.id == key)
        .ok_or("ANTIGRAVITY_NATIVE_SESSION_NOT_FOUND")?;
    let c = client(core)?;
    let sid = key
        .strip_prefix(PREFIX)
        .ok_or("ANTIGRAVITY_NATIVE_IDENTITY_INVALID")?;
    let cwd = row
        .project_provenance
        .ok_or("ANTIGRAVITY_NATIVE_ROOT_INVALID")?;
    c.register(&core.store, sid, &cwd, json!({}))?;
    c.sessions.lock().ok().map(|mut s| s.remove(sid));
    c.ensure(core, key)?;
    let w = core.store.codex_chat_context(key)?;
    core.store
        .refresh_codex_watch_label(key, w.generation, &cwd, &row.label)?;
    Ok(())
}
pub(crate) fn create_empty(
    core: &RouterCore,
    request: &str,
    directory: &str,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<Value, String> {
    create_empty_for(core, request, directory, model, effort, false)
}
pub(crate) fn create_empty_for(
    core: &RouterCore,
    request: &str,
    directory: &str,
    model: Option<&str>,
    effort: Option<&str>,
    assistant: bool,
) -> Result<Value, String> {
    preflight(model, effort)?;
    let c = client(core)?;
    let created = c
        .rpc
        .request("session/new", json!({"cwd":directory,"mcpServers":[]}))?;
    let sid = created["sessionId"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .ok_or("ANTIGRAVITY_NATIVE_IDENTITY_UNCONFIRMED")?;
    if assistant {
        core.store.checkpoint_assistant_created_chat(request,&json!({"provider":"ANTIGRAVITY","threadId":format!("{PREFIX}{sid}"),"directory":directory,"firstMessageSent":false,"registration":"PENDING","automaticRetry":false}))?;
    } else {
        core.store
            .checkpoint_owner_conversation_created(request, sid)?;
    }
    let key = c.register(&core.store, sid, directory, configuration(&created))?;
    if let Some(model) = model {
        let selected = c.rpc.request(
            "session/set_model",
            json!({"sessionId":sid,"modelId":model}),
        )?;
        if selected
            .pointer("/models/currentModelId")
            .and_then(Value::as_str)
            .is_some_and(|m| m != model)
        {
            return Err("ANTIGRAVITY_NATIVE_MODEL_UNCONFIRMED".into());
        }
        // The requested native variant encodes its advertised reasoning level. A
        // native rejection never falls back to a different model or first prompt.
    }
    let watch = core.store.codex_chat_context(&key)?;
    Ok(
        json!({"threadId":key,"watch":watch,"nativeSessionId":sid,"firstMessageSent":false,"model":model,"effort":effort,"automaticRetry":false}),
    )
}
pub(crate) fn state(core: &RouterCore, key: &str) -> Result<Value, String> {
    let w = core.store.codex_chat_context(key)?;
    let meta = core.store.managed_conversation(key)?;
    let c = client(core)?;
    let loaded = c
        .sessions
        .lock()
        .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?
        .contains_key(&meta.native_id);
    let active = c.sessions.lock().ok().and_then(|s| {
        s.get(&meta.native_id)
            .and_then(|v| v.active.as_ref().map(|a| a.control.clone()))
    });
    let permissions = c
        .permissions
        .lock()
        .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?;
    let requests=permissions.iter().filter(|(_,p)|p.session==meta.native_id&&p.generation==w.generation).map(|(id,p)|json!({"requestId":id,"revision":1,"kind":"APPROVAL","method":"session/request_permission","reason":if p.attempted{"权限答复送达未确认，请核对原生会话；不会重复发送"}else{"Antigravity 请求执行权限"},"deliveryStatus":if p.attempted{"UNKNOWN"}else{"READY"},"choices":p.options.iter().filter(|_|!p.attempted).filter(|o|matches!(o["kind"].as_str(),Some("allow_once"|"reject_once"))).map(|o|json!({"id":o["optionId"],"label":o["name"]})).collect::<Vec<_>>(),"questions":[],"isBlocking":true,"responseSent":false})).collect::<Vec<_>>();
    let history = core.store.managed_public_history(key, None)?;
    Ok(
        json!({"provider":"ANTIGRAVITY","watch":w,"host":std::env::var("COMPUTERNAME").unwrap_or_default(),"inputReady":loaded,"ownedTurnId":active,"controllableTurnId":null,"externalBusy":active.is_some(),"replies":core.store.watch_replies(key)?,"requests":requests,"goal":null,"publicMessages":history["messages"],"historyRevision":format!("acp:{}",meta.epoch),"activity":active.as_ref().map(|_|"EXECUTING"),"checkedAt":if loaded{now()}else{w.checked_at},"sendCapabilities":{"queue":true,"steer":false,"permissionBypass":false,"receiptBasis":"NATIVE_PROMPT_RESULT","nativeTurnIdExposed":false}}),
    )
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub(crate) fn validate_send(
    core: &RouterCore,
    key: &str,
    generation: i64,
    mode: &str,
    options: &ReplyOptions,
) -> Result<(), String> {
    let w = core.store.codex_chat_context(key)?;
    if w.generation != generation || !w.enabled {
        return Err("REPLY_TARGET_CHANGED_REFRESH".into());
    }
    if !matches!(mode, "SEND" | "AUTO" | "QUEUE")
        || options.model.is_some()
        || options.effort.is_some()
    {
        return Err("ANTIGRAVITY_SEND_OPTIONS_UNSUPPORTED".into());
    }
    let view = state(core, key)?;
    if (mode != "QUEUE" && view["externalBusy"] == true)
        || !view["requests"].as_array().is_none_or(Vec::is_empty)
    {
        return Err("ANTIGRAVITY_NATIVE_BUSY_OR_PERMISSION_PENDING".into());
    }
    Ok(())
}
pub(crate) fn send(
    core: &RouterCore,
    key: &str,
    id: &str,
    generation: i64,
    mode: &str,
    text: String,
    options: ReplyOptions,
    assistant: bool,
) -> Result<Value, String> {
    let w = core.store.codex_chat_context(key)?;
    let input = WatchReply {
        id: id.into(),
        thread_id: key.into(),
        cwd: w.cwd,
        generation,
        source_sequence: None,
        expected_turn_id: None,
        mode: if mode == "AUTO" {
            "SEND".into()
        } else {
            mode.into()
        },
        text,
        options: serde_json::to_value(options).map_err(|_| "REPLY_OPTIONS_INVALID")?,
        payload_hash: String::new(),
        status: String::new(),
        turn_id: None,
        error_code: None,
        created_at: 0,
        updated_at: 0,
    };
    if core.store.watch_reply(id)?.is_some() {
        return Ok(json!(core.store.prepare_watch_reply(&input)?));
    }
    let options: ReplyOptions =
        serde_json::from_value(input.options.clone()).map_err(|_| "REPLY_OPTIONS_INVALID")?;
    validate_send(core, key, generation, mode, &options)?;
    let c = client(core)?;
    let sid = c.ensure(core, key)?;
    let _cache = crate::attachment_cache::guard()?;
    let _ = crate::watch_chat::claude_attachment_text(
        &core.store.codex_chat_context(key)?,
        &input.text,
        &options,
    )?;
    let _gate = c
        .gate
        .lock()
        .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?;
    if input.mode != "QUEUE"
        && c.sessions
            .lock()
            .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?
            .get(&sid)
            .is_none_or(|s| s.active.is_some())
    {
        return Err("ANTIGRAVITY_NATIVE_BUSY_OR_PERMISSION_PENDING".into());
    }
    if assistant {
        core.store.prepare_managed_assistant_reply(&input)?;
    } else {
        core.store.prepare_managed_owner_reply(&input)?;
    }
    drop(_gate);
    drop(_cache);
    pump(core.store.clone(), c, sid);
    Ok(json!(core.store.watch_reply(&input.id)?))
}
pub(crate) fn dispatch_queue(core: &RouterCore) {
    let Ok(rows) = core.store.queued_watch_replies() else {
        return;
    };
    for r in rows.into_iter().filter(|r| r.thread_id.starts_with(PREFIX)) {
        if let Ok(c) = client(core) {
            if let Ok(sid) = c.ensure(core, &r.thread_id) {
                pump(core.store.clone(), c, sid);
            }
        }
    }
}
fn pump(store: Arc<RouterStore>, c: Arc<Client>, sid: String) {
    let Ok(gate) = c.gate.lock() else { return };
    let key = match c.sessions.lock().ok().and_then(|s| {
        s.get(&sid)
            .filter(|s| s.active.is_none())
            .map(|s| s.key.clone())
    }) {
        Some(v) => v,
        None => return,
    };
    let Some(r) = store.managed_next_queued(&key).ok().flatten() else {
        return;
    };
    let prepared = (|| -> Result<(String, WatchReply), String> {
        let w = store.codex_chat_context(&key)?;
        if w.generation != r.generation {
            return Err("REPLY_TARGET_CHANGED_REFRESH".into());
        }
        if c.permissions
            .lock()
            .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?
            .values()
            .any(|p| p.session == sid)
        {
            return Err("ANTIGRAVITY_NATIVE_BUSY_OR_PERMISSION_PENDING".into());
        }
        store.authorize_managed_reply(&r.id, &key)?;
        let options = serde_json::from_value::<ReplyOptions>(r.options.clone())
            .map_err(|_| "REPLY_OPTIONS_INVALID")?;
        let _cache = crate::attachment_cache::guard()?;
        let text = crate::watch_chat::claude_attachment_text(&w, &r.text, &options)?;
        let claimed = store.claim_watch_reply(&r.id)?;
        Ok((text, claimed))
    })();
    let (text, r) = match prepared {
        Ok(v) => v,
        Err(code) => {
            if !code.contains("PENDING") && !code.contains("PREVIOUS") && !code.contains("UNKNOWN")
            {
                let _ = store.finish_watch_reply(&r.id, "FAILED", None, Some(&code));
            }
            return;
        }
    };
    let id = r.id;
    let control = uuid::Uuid::new_v4().to_string();
    let user_message = format!("acp-user:{id}");
    if let Ok(mut s) = c.sessions.lock() {
        if let Some(s) = s.get_mut(&sid) {
            s.active = Some(Active {
                reply: id.clone(),
                control: control.clone(),
                agent_message: format!("acp-agent:{id}"),
                user_message: user_message.clone(),
                user_text: text.clone(),
            });
        }
    }
    let call = c.rpc.begin(
        "session/prompt",
        json!({"sessionId":sid,"prompt":[{"type":"text","text":text}]}),
    );
    let (_, rx) = match call {
        Ok(v) => v,
        Err(_) => {
            let _ = store.finish_watch_reply(
                &id,
                "UNKNOWN",
                None,
                Some("ANTIGRAVITY_NATIVE_WRITE_UNCONFIRMED"),
            );
            return;
        }
    };
    let _ = c.snapshot(&store, &sid, "RUNNING");
    drop(gate);
    std::thread::spawn(move || {
        let result = rx.recv();
        let outcome = match result {
            Ok(Ok(v)) if v["stopReason"].is_string() => {
                let _ = store.finish_watch_reply(&id, "SENT", Some(&control), None);
                if store.managed_public_history(&key, None).ok().and_then(|v| {
                    v["messages"]
                        .as_array()
                        .map(|a| a.iter().any(|v| v["id"] == user_message))
                }) != Some(true)
                {
                    let _ = store.managed_append_text(&key, &c.epoch, &user_message, "user", &text);
                }
                if v["stopReason"] == "cancelled" {
                    "INTERRUPTED"
                } else {
                    "RESULT_READY"
                }
            }
            _ => {
                let _ = store.finish_watch_reply(
                    &id,
                    "UNKNOWN",
                    None,
                    Some("ANTIGRAVITY_NATIVE_RESULT_UNCONFIRMED"),
                );
                "UNKNOWN"
            }
        };
        if let Ok(mut s) = c.sessions.lock() {
            if let Some(s) = s.get_mut(&sid) {
                s.active = None;
            }
        }
        let _ = c.snapshot(&store, &sid, outcome);
        if outcome != "UNKNOWN" {
            pump(store, c, sid);
        }
    });
}
pub(crate) fn validate_response(
    core: &RouterCore,
    key: &str,
    request: &str,
    input: &crate::host_application::MobileCodexResponseInput,
) -> Result<(), String> {
    let w = core.store.codex_chat_context(key)?;
    let sid = core.store.managed_conversation(key)?.native_id;
    let c = client(core)?;
    let permissions = c
        .permissions
        .lock()
        .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?;
    let p = permissions
        .get(request)
        .ok_or("ANTIGRAVITY_PERMISSION_EXPIRED")?;
    if p.attempted
        || input.revision != 1
        || input.answers.is_some()
        || p.session != sid
        || p.generation != w.generation
        || !input.decision.as_ref().is_some_and(|d| {
            p.options.iter().any(|o| {
                o["optionId"] == *d
                    && matches!(o["kind"].as_str(), Some("allow_once" | "reject_once"))
            })
        })
    {
        return Err("ANTIGRAVITY_PERMISSION_CHANGED".into());
    }
    Ok(())
}
pub(crate) fn command(core: &RouterCore, c: ChatCommand) -> Result<Value, String> {
    let key = c.thread_id().to_string();
    match c {
        ChatCommand::History { cursor, .. } => {
            let before = cursor
                .map(|v| {
                    v.strip_prefix("managed:")
                        .and_then(|v| v.parse::<i64>().ok())
                        .ok_or("ANTIGRAVITY_HISTORY_CURSOR_INVALID")
                })
                .transpose()?;
            core.store.codex_chat_context(&key)?;
            core.store.managed_public_history(&key, before)
        }
        ChatCommand::Send {
            id,
            generation,
            mode,
            text,
            options,
            source_sequence,
            expected_turn_id,
            queue_requested,
            ..
        } => {
            if source_sequence.is_some() || expected_turn_id.is_some() {
                return Err("ANTIGRAVITY_SEND_CONTEXT_CHANGED".into());
            }
            if (mode == "QUEUE") != queue_requested {
                return Err("REPLY_EXPLICIT_QUEUE_REQUIRED".into());
            }
            send(core, &key, &id, generation, &mode, text, options, false)
        }
        ChatCommand::Upload { name, data, .. } => {
            crate::watch_chat::upload(core, &key, &name, &data)
        }
        ChatCommand::Receipt { id, .. } => {
            let r = core.store.watch_reply(&id)?;
            if r.as_ref().is_some_and(|r| r.thread_id != key) {
                return Err("REPLY_TARGET_CHANGED_REFRESH".into());
            }
            Ok(json!(r))
        }
        ChatCommand::Restore { generation, .. } => {
            let w = core.store.codex_chat_context(&key)?;
            if w.generation != generation {
                return Err("REPLY_TARGET_CHANGED_REFRESH".into());
            }
            let c = client(core)?;
            c.ensure(core, &key)?;
            state(core, &key)
        }
        ChatCommand::Cancel { id, .. } => {
            core.store.codex_chat_context(&key)?;
            core.store.cancel_watch_reply(&key, &id)?;
            Ok(json!({"ok":true}))
        }
        ChatCommand::DiscardReply { id, .. } => {
            core.store.codex_chat_context(&key)?;
            core.store.discard_watch_reply(&key, &id)?;
            Ok(json!({"removed":true}))
        }
        ChatCommand::Abandon { id, .. } => {
            core.store.codex_chat_context(&key)?;
            Ok(json!({"cancelled":core.store.abandon_watch_reply(&key,&id)?}))
        }
        ChatCommand::AcknowledgeUnknown { id, confirmed, .. } => {
            core.store.codex_chat_context(&key)?;
            core.store
                .acknowledge_watch_reply_unknown(&key, &id, confirmed)?;
            Ok(json!({"ok":true,"deliveryConfirmed":false}))
        }
        ChatCommand::Options { .. } => Ok(
            json!({"models":[],"permissions":"NATIVE_REQUESTS","modelSelection":"CREATION_ONLY"}),
        ),
        ChatCommand::Stop { turn_id, .. } => {
            let c = client(core)?;
            let sid = core.store.managed_conversation(&key)?.native_id;
            let valid = c
                .sessions
                .lock()
                .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?
                .get(&sid)
                .is_some_and(|s| s.active.as_ref().is_some_and(|a| a.control == turn_id));
            if !valid {
                return Err("ANTIGRAVITY_ACTIVE_REQUEST_CHANGED".into());
            }
            c.rpc.notify("session/cancel", json!({"sessionId":sid}))?;
            Ok(json!({"requested":true,"completionConfirmed":false}))
        }
        ChatCommand::Respond {
            request_id, input, ..
        } => {
            let c = client(core)?;
            let w = core.store.codex_chat_context(&key)?;
            let sid = core.store.managed_conversation(&key)?.native_id;
            if input.revision != 1 || input.answers.is_some() {
                return Err("ANTIGRAVITY_PERMISSION_CHANGED".into());
            }
            let decision = input.decision.ok_or("ANTIGRAVITY_PERMISSION_CHANGED")?;
            let mut permissions = c
                .permissions
                .lock()
                .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?;
            let p = permissions
                .get(&request_id)
                .ok_or("ANTIGRAVITY_PERMISSION_EXPIRED")?;
            if p.attempted
                || p.session != sid
                || p.generation != w.generation
                || !p.options.iter().any(|o| {
                    o["optionId"] == decision
                        && matches!(o["kind"].as_str(), Some("allow_once" | "reject_once"))
                })
            {
                return Err("ANTIGRAVITY_PERMISSION_CHANGED".into());
            }
            let p = permissions
                .get_mut(&request_id)
                .ok_or("ANTIGRAVITY_PERMISSION_EXPIRED")?;
            p.attempted = true;
            let native = p.native.clone();
            drop(permissions);
            c.rpc.answer(
                &native,
                json!({"outcome":{"outcome":"selected","optionId":decision}}),
            )?;
            c.permissions
                .lock()
                .map_err(|_| "ANTIGRAVITY_SERVICE_UNAVAILABLE")?
                .remove(&request_id);
            let _ = c.snapshot(&core.store, &sid, "RUNNING");
            Ok(json!({"accepted":true,"nativeCompletionConfirmed":false}))
        }
        _ => Err("ANTIGRAVITY_OPERATION_UNSUPPORTED".into()),
    }
}

#[cfg(test)]
mod tests;
