//! Routes the Host's app-server JSON-RPC to the on-demand backend or Desktop's pipe.
use super::{
    backend::{Backend, BackendLost, PinKind},
    ipc::{Connection, PeerPolicy, Reply, StateEvent, OFFICIAL_PIPE},
    native_files::{self, Boundary, BoundaryKind, Signature},
    owner::Sessions,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, Write},
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

const READ_TIMEOUT: Duration = Duration::from_secs(60);
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);
const OWNER_CACHE: Duration = Duration::from_secs(2);
const RECONNECT_THROTTLE: Duration = Duration::from_secs(5);
const CACHE_LIMIT: usize = 512;
const CACHE_VALUE_LIMIT: usize = 256 * 1024;
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(500);
const REFRESH_ACTIVE: Duration = Duration::from_secs(5);
const REFRESH_IDLE: Duration = Duration::from_secs(15);
const REFRESH_TOO_LARGE: Duration = Duration::from_secs(30);
const REFRESH_SETTLING: Duration = Duration::from_secs(1);
const SETTLE_ATTEMPTS: u8 = 5;
const ROLLOUT_EMPTY_RETRIES: u8 = 10;
const ROLLOUT_EMPTY_DELAY: Duration = Duration::from_millis(200);
const UNSUBSCRIBED_FOLLOW_LIMIT: Duration = Duration::from_secs(120);
/// Stays inside the Host's own limit for a resume before a write.
const LOAD_RETRIES: u8 = 28;
const LOAD_RETRY_DELAY: Duration = Duration::from_millis(250);

pub(super) struct Config {
    pub pipe: String,
    pub policy: PeerPolicy,
    pub codex_home: PathBuf,
    pub backend_command: Box<dyn Fn() -> Command + Send + Sync>,
    pub idle_grace: Duration,
    /// How long a thread stays held after its turn, in case the next message follows at once.
    pub release_after_turn: Duration,
}

impl Config {
    fn production() -> Result<Self, String> {
        Ok(Self {
            pipe: OFFICIAL_PIPE.to_owned(),
            policy: PeerPolicy { verify: true, versions: None },
            codex_home: native_files::codex_home().ok_or("CODEX_LIGHT_HOME_UNAVAILABLE")?,
            backend_command: Box::new(|| match native_files::desktop_codex_executable() {
                Some(executable) => {
                    let mut command = Command::new(executable);
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000);
                    command
                }
                None => router_core::codex::platform::codex_app_server_command(),
            }),
            idle_grace: Duration::from_secs(30),
            release_after_turn: Duration::from_secs(5),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum TurnEvent {
    None,
    Started(String),
    Ended { turn: String, unseen: bool, stale: bool },
}

/// Works out turn lifecycle events from snapshots. A turn can start and end between two
/// snapshots, so an end never depends on having seen it run: the Host deliberately has no
/// fallback for a missed interruption and would show the turn as running forever.
#[derive(Default)]
pub(super) struct TurnTracker {
    open: Option<String>,
    started_sent: Option<String>,
    baseline: Option<Option<String>>,
    settle_attempts: u8,
}

impl TurnTracker {
    fn terminal_id(boundary: Option<&Boundary>) -> Option<String> {
        boundary.filter(|b| b.kind != BoundaryKind::Started).and_then(|b| b.turn_id.clone())
    }

    /// True while waiting for the end record of a turn Desktop already reports as idle.
    pub fn settling(&self) -> bool {
        self.settle_attempts > 0
    }

    pub fn take_open(&mut self) -> Option<String> {
        self.settle_attempts = 0;
        self.open.take()
    }

    pub fn observe(&mut self, active: bool, announced: Option<String>, boundary: Option<&Boundary>) -> TurnEvent {
        let first = self.baseline.is_none();
        let terminal = Self::terminal_id(boundary);
        if first {
            self.baseline = Some(terminal.clone());
        }
        if let Some(turn) = announced {
            self.open = Some(turn);
        }
        if active {
            self.settle_attempts = 0;
            let running = boundary.filter(|b| b.kind == BoundaryKind::Started).and_then(|b| b.turn_id.clone());
            let Some(current) = running.or_else(|| self.open.clone()) else { return TurnEvent::None };
            // A newer turn is running while this still holds an older one: end that first.
            if let Some(previous) = self.open.clone().filter(|open| *open != current) {
                let unseen = self.started_sent.as_ref() != Some(&previous);
                self.open = Some(current);
                self.baseline = Some(Some(previous.clone()));
                return TurnEvent::Ended { turn: previous, unseen, stale: false };
            }
            self.open = Some(current.clone());
            if self.started_sent.as_ref() == Some(&current) {
                return TurnEvent::None;
            }
            self.started_sent = Some(current.clone());
            return TurnEvent::Started(current);
        }
        if let Some(open) = self.open.clone() {
            // Desktop can report idle just before the end record reaches the file.
            let record_pending = boundary.is_some_and(|b| b.kind == BoundaryKind::Started && b.turn_id.as_deref() == Some(open.as_str()));
            if record_pending && self.settle_attempts < SETTLE_ATTEMPTS {
                self.settle_attempts += 1;
                return TurnEvent::None;
            }
            self.settle_attempts = 0;
            self.open = None;
            self.baseline = Some(terminal.or(Some(open.clone())));
            let unseen = self.started_sent.as_ref() != Some(&open);
            return TurnEvent::Ended { turn: open, unseen, stale: record_pending };
        }
        // A turn that began and finished between snapshots, for example from Desktop itself.
        if !first && terminal.is_some() && self.baseline.as_ref() != Some(&terminal) {
            self.baseline = Some(terminal.clone());
            return TurnEvent::Ended { turn: terminal.unwrap(), unseen: true, stale: false };
        }
        // An idle Desktop with an open record left over from before this follow.
        if first {
            if let Some(turn) = boundary.filter(|b| b.kind == BoundaryKind::Started).and_then(|b| b.turn_id.clone()) {
                return TurnEvent::Ended { turn, unseen: false, stale: true };
            }
        }
        TurnEvent::None
    }
}

/// How a thread can be reached right now.
enum Route {
    Backend,
    Desktop(Arc<Connection>, String),
    HeldElsewhere,
    Free,
}

struct CacheEntry {
    signature: Option<Signature>,
    stored: Instant,
    ttl: Option<Duration>,
    reply: Value,
}

struct Follow {
    events: mpsc::UnboundedSender<StateEvent>,
    host_subscribed: bool,
    status: Option<Value>,
    announced_turn: Option<String>,
}

pub(super) struct Hub {
    config: Config,
    out: mpsc::UnboundedSender<Value>,
    connection: tokio::sync::Mutex<Option<Arc<Connection>>>,
    last_connect: Mutex<Option<Instant>>,
    backend: tokio::sync::Mutex<Option<Arc<Backend>>>,
    cache: Mutex<HashMap<String, CacheEntry>>,
    rollouts: Mutex<HashMap<String, PathBuf>>,
    owners: Mutex<HashMap<String, (Instant, Option<String>)>>,
    follows: Mutex<HashMap<String, Follow>>,
    stale_open_turns: Mutex<HashMap<String, String>>,
    confirmed_interruptions: Mutex<HashSet<(String, String)>>,
    sessions: Arc<Sessions>,
    me: Weak<Hub>,
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({"id":id,"error":{"code":code,"message":message}})
}

fn is_rollout_still_empty(reply: &Value) -> bool {
    reply.pointer("/error/message").and_then(Value::as_str).is_some_and(|message| message.starts_with("failed to read thread:") && message.trim_end().ends_with("is empty"))
}

/// A read with no live identity, so a TTL is its only freshness.
fn catalog_ttl(method: &str) -> Option<Duration> {
    match method {
        "model/list" | "config/read" | "account/read" => Some(Duration::from_secs(300)),
        "account/rateLimits/read" => Some(Duration::from_secs(120)),
        "thread/list" => Some(Duration::from_secs(5)),
        _ => None,
    }
}

/// Reads whose answer follows the thread's rollout file.
fn is_thread_read(method: &str) -> bool {
    matches!(method, "thread/read" | "thread/turns/list" | "thread/items/list" | "thread/goal/get")
}

/// Host text items gain the field Desktop's turn handler reads.
fn with_text_elements(input: &Value) -> Value {
    let Some(items) = input.as_array() else { return input.clone() };
    Value::Array(
        items
            .iter()
            .map(|item| {
                let mut item = item.clone();
                if item["type"] == "text" && item.get("text_elements").is_none() {
                    item["text_elements"] = json!([]);
                }
                item
            })
            .collect(),
    )
}

/// Desktop's steer handler dereferences this shape even for a remote follower.
fn steer_params(thread: &str, params: &Value, cwd: &str, collaboration_mode: &Value) -> Value {
    let message_id = params["clientUserMessageId"].as_str().map(str::to_owned).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let input = with_text_elements(&params["input"]);
    let text = input.as_array().and_then(|items| items.iter().find_map(|item| item["text"].as_str())).unwrap_or_default().to_owned();
    json!({
        "conversationId":thread,"input":input,"clientUserMessageId":message_id,
        "restoreMessage":{"id":message_id,"text":text,"cwd":cwd,"context":{
            "workspaceRoots":[cwd],"collaborationMode":collaboration_mode,"prompt":text,"addedFiles":[],
            "fileAttachments":[],"imageAttachments":[],"ideContext":null,"commentAttachments":[],
            "pullRequestChecks":[],"reviewRequest":null
        }}
    })
}

/// Maps the Host's answer to the follower method for a pending native request.
fn approval_method(request_method: &str) -> Option<(&'static str, bool)> {
    match request_method {
        "item/commandExecution/requestApproval" => Some(("thread-follower-command-approval-decision", true)),
        "item/fileChange/requestApproval" => Some(("thread-follower-file-approval-decision", true)),
        "item/permissions/requestApproval" => Some(("thread-follower-permissions-request-approval-response", false)),
        "item/tool/requestUserInput" => Some(("thread-follower-submit-user-input", false)),
        "mcpServer/elicitation/request" => Some(("thread-follower-submit-mcp-server-elicitation-response", false)),
        _ => None,
    }
}

pub(super) fn host_request_id(thread: &str, method: &str, request_id: &Value) -> Value {
    json!(format!("ipc:{thread}:{method}:{request_id}"))
}

pub(super) fn parse_host_request_id(id: &str) -> Option<(String, String, Value)> {
    let rest = id.strip_prefix("ipc:")?;
    let (thread, rest) = rest.split_once(':')?;
    let (method, raw) = rest.split_once(':')?;
    Some((thread.to_owned(), method.to_owned(), serde_json::from_str(raw).ok()?))
}

impl Hub {
    pub fn new(config: Config, out: mpsc::UnboundedSender<Value>) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            sessions: Arc::default(),
            config,
            out,
            connection: Default::default(),
            last_connect: Mutex::new(None),
            backend: Default::default(),
            cache: Mutex::default(),
            rollouts: Mutex::default(),
            owners: Mutex::default(),
            follows: Mutex::default(),
            stale_open_turns: Mutex::default(),
            confirmed_interruptions: Mutex::default(),
        })
    }

    fn send(&self, message: Value) {
        let _ = self.out.send(message);
    }

    pub(super) fn owners(&self) -> &Sessions {
        &self.sessions
    }

    pub(super) fn lock_held(&self, thread: &str) -> bool {
        native_files::writer_lock_held(&self.config.codex_home, thread)
    }

    async fn pipe_connection(&self, force: bool) -> Option<Arc<Connection>> {
        let mut slot = self.connection.lock().await;
        if let Some(existing) = slot.as_ref().filter(|connection| !connection.is_closed()) {
            return Some(existing.clone());
        }
        *slot = None;
        {
            let mut last = self.last_connect.lock().unwrap();
            if !force && last.is_some_and(|at| at.elapsed() < RECONNECT_THROTTLE) {
                return None;
            }
            *last = Some(Instant::now());
        }
        let opened = Connection::open(&self.config.pipe, &self.config.policy, "agbrio-light").await.ok()?;
        *slot = Some(opened.clone());
        Some(opened)
    }

    async fn running_backend(&self) -> Result<Arc<Backend>, String> {
        let mut slot = self.backend.lock().await;
        if let Some(existing) = slot.as_ref().filter(|backend| !backend.is_dead()) {
            return Ok(existing.clone());
        }
        // Everything the backend says also updates what Desktop is shown.
        let (tap, mut tapped) = mpsc::unbounded_channel::<Value>();
        let (sessions, out) = (self.sessions.clone(), self.out.clone());
        tokio::spawn(async move {
            while let Some(message) = tapped.recv().await {
                sessions.observe(&message);
                let _ = out.send(message);
            }
        });
        let backend = Backend::start((self.config.backend_command)(), tap)?;
        backend.initialize().await?;
        *slot = Some(backend.clone());
        Ok(backend)
    }

    pub(super) async fn pinned_backend(&self, thread: &str) -> Option<Arc<Backend>> {
        self.backend.lock().await.as_ref().filter(|backend| !backend.is_dead() && backend.is_pinned(thread)).cloned()
    }

    async fn owner_of(&self, thread: &str, fresh: bool) -> Option<(Arc<Connection>, String)> {
        let connection = self.pipe_connection(fresh).await?;
        if !fresh {
            let cached = self.owners.lock().unwrap().get(thread).filter(|(at, _)| at.elapsed() < OWNER_CACHE).cloned();
            if let Some((_, owner)) = cached {
                return owner.map(|owner| (connection, owner));
            }
        }
        let owner = connection.discover_owner(thread).await;
        self.owners.lock().unwrap().insert(thread.to_owned(), (Instant::now(), owner.clone()));
        owner.map(|owner| (connection, owner))
    }

    async fn route(&self, thread: &str, fresh: bool) -> Route {
        if self.pinned_backend(thread).await.is_some() {
            return Route::Backend;
        }
        // Released but not yet unloaded: this process's own owner connection still answers
        // discovery, which must not be mistaken for Desktop. Loading it again cancels the release.
        if self.sessions.contains(thread) {
            return Route::Free;
        }
        if let Some((connection, owner)) = self.owner_of(thread, fresh).await {
            return Route::Desktop(connection, owner);
        }
        if native_files::writer_lock_held(&self.config.codex_home, thread) {
            return Route::HeldElsewhere;
        }
        Route::Free
    }

    fn rollout_signature(&self, thread: &str) -> Option<Signature> {
        let path = self.rollouts.lock().unwrap().get(thread).cloned()?;
        native_files::signature(&path)
    }

    fn cached(&self, key: &str, signature: Option<Signature>) -> Option<Value> {
        let cache = self.cache.lock().unwrap();
        let entry = cache.get(key)?;
        let fresh = match (entry.ttl, signature) {
            (Some(ttl), _) => entry.stored.elapsed() < ttl,
            (None, Some(current)) => entry.signature == Some(current),
            (None, None) => false,
        };
        fresh.then(|| entry.reply.clone())
    }

    fn remember(&self, key: String, signature: Option<Signature>, ttl: Option<Duration>, reply: &Value) {
        if reply.to_string().len() > CACHE_VALUE_LIMIT {
            return;
        }
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, CacheEntry { signature, stored: Instant::now(), ttl, reply: reply.clone() });
    }

    fn note_rollout(&self, thread: &str, reply: &Value) {
        if let Some(path) = reply.pointer("/result/thread/path").and_then(Value::as_str) {
            self.rollouts.lock().unwrap().insert(thread.to_owned(), PathBuf::from(path));
        }
    }

    async fn backend_read(&self, method: &str, params: &Value) -> Result<Value, String> {
        let backend = self.running_backend().await?;
        let mut attempts = 0;
        loop {
            let reply = backend.call(method, params.clone(), READ_TIMEOUT).await.map_err(|BackendLost| "CODEX_LIGHT_BACKEND_LOST".to_owned())?;
            // A first turn creates its rollout an instant before writing to it. Reads only,
            // so repeating one here cannot repeat anything the owner asked for.
            if attempts >= ROLLOUT_EMPTY_RETRIES || !is_rollout_still_empty(&reply) {
                return Ok(reply);
            }
            attempts += 1;
            tokio::time::sleep(ROLLOUT_EMPTY_DELAY).await;
        }
    }

    /// The live status of a thread this process did not load. The rollout tail says
    /// whether a turn looks open; a follow's snapshot overrides it when one exists.
    async fn overlay_status(&self, thread: &str) -> Option<Value> {
        if self.pinned_backend(thread).await.is_some() {
            return None;
        }
        if let Some(status) = self.follows.lock().unwrap().get(thread).and_then(|follow| follow.status.clone()) {
            return Some(status);
        }
        self.owner_of(thread, false).await?;
        let boundary = self.rollouts.lock().unwrap().get(thread).and_then(|path| native_files::latest_boundary(path));
        let stale = self.stale_open_turns.lock().unwrap().get(thread).cloned();
        Some(match boundary {
            Some(Boundary { kind: BoundaryKind::Started, turn_id }) if turn_id.is_none() || turn_id != stale => json!({"type":"active","activeFlags":[]}),
            _ => json!({"type":"idle"}),
        })
    }

    async fn thread_read(&self, id: &Value, method: &str, params: &Value) -> Value {
        let thread = params["threadId"].as_str().unwrap_or_default().to_owned();
        let key = format!("{method}:{params}");
        let pinned = self.pinned_backend(&thread).await.is_some();
        let signature = self.rollout_signature(&thread);
        let cached = if pinned { None } else { self.cached(&key, signature) };
        let mut reply = match cached {
            Some(reply) => reply,
            None => match self.backend_read(method, params).await {
                Ok(reply) => {
                    self.note_rollout(&thread, &reply);
                    if !pinned && reply.get("result").is_some() {
                        self.remember(key, self.rollout_signature(&thread), None, &reply);
                    }
                    reply
                }
                Err(error) => return rpc_error(id, -32000, &error),
            },
        };
        reply["id"] = id.clone();
        if method == "thread/read" && reply.get("result").is_some() {
            if let Some(status) = self.overlay_status(&thread).await {
                reply["result"]["thread"]["status"] = status;
            }
        }
        if method == "thread/turns/list" {
            self.overlay_newest_turn(&thread, &mut reply).await;
            self.confirm_recorded_interruption(&thread, &reply).await;
        }
        reply
    }

    /// A reader process cannot see Desktop's running turn as running.
    async fn overlay_newest_turn(&self, thread: &str, reply: &mut Value) {
        let Some(status) = self.overlay_status(thread).await else { return };
        if status["type"] != "active" {
            return;
        }
        let open_turn = self.rollouts.lock().unwrap().get(thread).and_then(|path| native_files::latest_boundary(path)).and_then(|boundary| boundary.turn_id);
        let Some(rows) = reply.pointer_mut("/result/data").and_then(Value::as_array_mut) else { return };
        for row in rows {
            if row["id"].as_str() == open_turn.as_deref() && row["status"] != "completed" && row["status"] != "failed" {
                row["status"] = json!("inProgress");
            }
        }
    }

    /// The Host refuses to send after an interruption it did not witness, so report one
    /// once when it is proven. The proof is the turn's own abort record, or a free writer
    /// lock under a turn that never recorded an end: a process killed outright, or a power
    /// cut, writes nothing, and without a writer no turn can be running. Only interruptions:
    /// the Host already reconciles completed turns, and those carry side effects.
    async fn confirm_recorded_interruption(&self, thread: &str, reply: &Value) {
        let Some(row) = reply.pointer("/result/data/0") else { return };
        let Some(turn) = row["id"].as_str().filter(|_| row["status"] == "interrupted") else { return };
        if self.pinned_backend(thread).await.is_some() {
            return;
        }
        let boundary = self.rollouts.lock().unwrap().get(thread).and_then(|path| native_files::latest_boundary(path));
        let proven = boundary.is_some_and(|b| {
            b.turn_id.as_deref() == Some(turn) && (b.kind == BoundaryKind::Aborted || b.kind == BoundaryKind::Started && !self.lock_held(thread))
        });
        if !proven || !self.confirmed_interruptions.lock().unwrap().insert((thread.to_owned(), turn.to_owned())) {
            return;
        }
        self.send(json!({"method":"turn/completed","params":{"threadId":thread,"turn":{"id":turn,"status":"interrupted"}}}));
    }

    async fn catalog_read(&self, id: &Value, method: &str, params: &Value) -> Value {
        let key = format!("{method}:{params}");
        let ttl = catalog_ttl(method);
        let mut reply = match self.cached(&key, None) {
            Some(reply) => reply,
            None => match self.backend_read(method, params).await {
                Ok(reply) => {
                    if reply.get("result").is_some() {
                        self.remember(key, None, ttl, &reply);
                    }
                    reply
                }
                Err(error) => return rpc_error(id, -32000, &error),
            },
        };
        reply["id"] = id.clone();
        reply
    }

    /// Forwards a write. A lost process leaves the request unanswered, so the Host's
    /// own timeout records UNKNOWN and the write is never repeated.
    async fn backend_write(&self, id: &Value, method: &str, params: &Value) -> Option<Value> {
        let backend = match self.running_backend().await {
            Ok(backend) => backend,
            Err(error) => return Some(rpc_error(id, -32000, &error)),
        };
        // Marked before sending: a quick turn's completion can arrive ahead of this reply,
        // and marking afterwards would leave its thread held forever.
        let starting = (method == "turn/start").then(|| params["threadId"].as_str()).flatten();
        if let Some(thread) = starting {
            backend.mark_active(thread);
        }
        let reply = backend.call(method, params.clone(), WRITE_TIMEOUT).await.ok()?;
        if let (Some(thread), true) = (starting, reply.get("error").is_some()) {
            backend.mark_idle(thread);
        }
        let thread = reply.pointer("/result/thread/id").and_then(Value::as_str).or_else(|| params["threadId"].as_str()).map(str::to_owned);
        if reply.get("result").is_some() {
            if let Some(thread) = &thread {
                match method {
                    "thread/start" | "thread/resume" => {
                        let kind = if method == "thread/start" { PinKind::Created } else { PinKind::Resumed };
                        backend.pin(thread, kind);
                        self.note_rollout(thread, &reply);
                        if let Some(hub) = self.me.upgrade() {
                            hub.claim(thread, &reply["result"], kind, backend.clone(), self.config.policy.clone(), self.config.pipe.clone());
                        }
                    }
                    "thread/unsubscribe" | "thread/archive" => {
                        backend.unpin(thread);
                        if let Some(hub) = self.me.upgrade() {
                            hub.wait_then_close_owner(thread.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
        if let (Some(turn), Some(thread)) = (reply.pointer("/result/turn/id").and_then(Value::as_str).filter(|_| method == "turn/start"), &thread) {
            self.sessions.note_turn_request(thread, turn, params);
        }
        let mut reply = reply;
        reply["id"] = id.clone();
        Some(reply)
    }

    /// A thread a backend is still unloading refuses a resume for a moment, and one a backend
    /// that is exiting holds its lock for a moment. A resume starts nothing, so waiting those
    /// out repeats nothing the owner asked for. Anything else is returned at once.
    async fn load_on_backend(&self, id: &Value, thread: &str, params: &Value) -> Option<Value> {
        for _ in 0..LOAD_RETRIES {
            let reply = self.backend_write(id, "thread/resume", params).await?;
            let closing = reply.pointer("/error/message").and_then(Value::as_str).is_some_and(|message| message.contains("is closing"));
            if reply.get("error").is_none() || !(closing || self.sessions.contains(thread) && self.lock_held(thread)) {
                return Some(reply);
            }
            tokio::time::sleep(LOAD_RETRY_DELAY).await;
        }
        self.backend_write(id, "thread/resume", params).await
    }

    pub(super) async fn follower_load(&self, thread: &str) -> Result<(), String> {
        let reply = self.load_on_backend(&json!("follower"), thread, &json!({"threadId":thread,"excludeTurns":true})).await.ok_or("request-timeout")?;
        reply.get("error").map_or(Ok(()), |error| Err(error["message"].as_str().unwrap_or("request-failed").to_owned()))
    }

    /// A write Desktop asked for while following a thread. An uncertain one is reported as
    /// failed to Desktop and never repeated.
    pub(super) async fn follower_write(&self, method: &str, params: &Value) -> Result<Value, String> {
        let reply = self.backend_write(&json!("follower"), method, params).await.ok_or("request-timeout")?;
        match reply.get("error") {
            Some(error) => Err(error["message"].as_str().unwrap_or("request-failed").to_owned()),
            None => Ok(reply["result"].clone()),
        }
    }

    fn held_error(id: &Value, thread: &str) -> Value {
        rpc_error(id, -32000, &format!("CODEX_LIGHT_THREAD_HELD_ELSEWHERE: thread {thread} has an active writer that this Host cannot reach"))
    }

    fn version_error(id: &Value) -> Value {
        rpc_error(id, -32000, "CODEX_LIGHT_DESKTOP_VERSION_UNSUPPORTED: this Codex Desktop build has not been validated for sending")
    }

    /// The status a follow remembered predates this turn, and the wake makes it look now.
    fn announce_turn(&self, thread: &str, turn: Option<&str>) {
        let mut follows = self.follows.lock().unwrap();
        if let (Some(follow), Some(turn)) = (follows.get_mut(thread), turn) {
            follow.announced_turn = Some(turn.to_owned());
            follow.status = None;
            let _ = follow.events.send(StateEvent::Changed);
        }
    }

    fn start_follow(self: &Arc<Self>, thread: &str, host_subscribed: bool) {
        let (events, receiver) = mpsc::unbounded_channel();
        {
            let mut follows = self.follows.lock().unwrap();
            if let Some(existing) = follows.get_mut(thread) {
                existing.host_subscribed |= host_subscribed;
                return;
            }
            follows.insert(thread.to_owned(), Follow { events, host_subscribed, status: None, announced_turn: None });
        }
        let hub = self.clone();
        let thread = thread.to_owned();
        tokio::spawn(async move { hub.follow_loop(thread, receiver).await });
    }

    /// Keeps the Host informed about one Desktop thread. Re-following returns a full
    /// snapshot, so this does not depend on the private patch format.
    async fn follow_loop(self: Arc<Self>, thread: String, mut events: mpsc::UnboundedReceiver<StateEvent>) {
        let mut known_requests: HashMap<String, Value> = HashMap::new();
        let mut turns = TurnTracker::default();
        let mut attached: Option<(Arc<Connection>, String)> = None;
        let mut delay = Duration::ZERO;
        let mut idle_since: Option<Instant> = None;
        loop {
            let still_wanted = {
                let follows = self.follows.lock().unwrap();
                follows.get(&thread).map(|follow| follow.host_subscribed || idle_since.is_none_or(|since| since.elapsed() < UNSUBSCRIBED_FOLLOW_LIMIT))
            };
            if still_wanted != Some(true) {
                break;
            }
            if attached.as_ref().is_none_or(|(connection, _)| connection.is_closed()) {
                attached = self.owner_of(&thread, true).await;
                match &attached {
                    Some((connection, _)) => {
                        let sender = self.follows.lock().unwrap().get(&thread).map(|follow| follow.events.clone());
                        if let Some(sender) = sender {
                            connection.subscribe(&thread, sender);
                        }
                    }
                    None => {
                        self.owner_lost(&thread, &mut known_requests, &mut turns).await;
                        tokio::time::sleep(RECONNECT_THROTTLE).await;
                        continue;
                    }
                }
                delay = Duration::ZERO;
            }
            let (connection, owner) = attached.clone().unwrap();
            if delay.is_zero() {
                connection.set_following(&owner, &thread, true);
            }
            let wait = if delay.is_zero() { REFRESH_ACTIVE } else { delay };
            match tokio::time::timeout(wait, events.recv()).await {
                Ok(Some(StateEvent::Snapshot(state))) => {
                    let active = state.pointer("/threadRuntimeStatus/type").and_then(Value::as_str) == Some("active");
                    idle_since = if active || !state["requests"].as_array().is_none_or(Vec::is_empty) { None } else { idle_since.or(Some(Instant::now())) };
                    self.apply_snapshot(&thread, &state, &mut known_requests, &mut turns).await;
                    delay = if turns.settling() { REFRESH_SETTLING } else if active { REFRESH_ACTIVE } else { REFRESH_IDLE };
                }
                Ok(Some(StateEvent::Changed)) => {
                    // Coalesce a burst of streaming updates into one refresh.
                    tokio::time::sleep(REFRESH_DEBOUNCE).await;
                    while matches!(events.try_recv(), Ok(StateEvent::Changed)) {}
                    delay = Duration::ZERO;
                }
                Ok(Some(StateEvent::TooLarge)) => delay = REFRESH_TOO_LARGE,
                Ok(Some(StateEvent::Disconnected)) | Ok(None) => attached = None,
                Err(_) => delay = Duration::ZERO,
            }
        }
        if let Some((connection, owner)) = attached {
            connection.set_following(&owner, &thread, false);
            connection.unsubscribe(&thread);
        }
        self.follows.lock().unwrap().remove(&thread);
    }

    async fn apply_snapshot(&self, thread: &str, state: &Value, known: &mut HashMap<String, Value>, turns: &mut TurnTracker) {
        if let Some(path) = state["rolloutPath"].as_str() {
            self.rollouts.lock().unwrap().insert(thread.to_owned(), PathBuf::from(path));
        }
        let status_kind = state.pointer("/threadRuntimeStatus/type").and_then(Value::as_str).unwrap_or("idle");
        let status = match status_kind {
            "active" => json!({"type":"active","activeFlags":state.pointer("/threadRuntimeStatus/activeFlags").cloned().unwrap_or(json!([]))}),
            other => json!({"type":other}),
        };
        let changed = {
            let mut follows = self.follows.lock().unwrap();
            let Some(follow) = follows.get_mut(thread) else { return };
            let changed = follow.status.as_ref() != Some(&status);
            follow.status = Some(status.clone());
            changed
        };
        let boundary = self.rollouts.lock().unwrap().get(thread).and_then(|path| native_files::latest_boundary(path));
        let announced = self.follows.lock().unwrap().get_mut(thread).and_then(|follow| follow.announced_turn.take());
        match turns.observe(status_kind == "active", announced, boundary.as_ref()) {
            TurnEvent::None => {}
            TurnEvent::Started(turn) => self.send(json!({"method":"turn/started","params":{"threadId":thread,"turn":{"id":turn,"status":"inProgress"}}})),
            TurnEvent::Ended { turn, unseen, stale } => {
                if unseen {
                    self.send(json!({"method":"turn/started","params":{"threadId":thread,"turn":{"id":turn,"status":"inProgress"}}}));
                }
                self.report_turn_end(thread, &turn, boundary.as_ref()).await;
                if stale {
                    // A crash leaves an open boundary behind; do not read it as running again.
                    self.stale_open_turns.lock().unwrap().insert(thread.to_owned(), turn);
                }
            }
        }
        if changed {
            self.send(json!({"method":"thread/status/changed","params":{"threadId":thread,"status":status}}));
        }
        let mut seen = HashSet::new();
        for request in state["requests"].as_array().into_iter().flatten() {
            let Some(method) = request["method"].as_str() else { continue };
            if approval_method(method).is_none() {
                continue;
            }
            let wire_id = host_request_id(thread, method, &request["id"]);
            let key = wire_id.as_str().unwrap_or_default().to_owned();
            seen.insert(key.clone());
            if !known.contains_key(&key) {
                known.insert(key, request.clone());
                let mut params = request["params"].clone();
                if params.get("threadId").is_none() {
                    params["threadId"] = json!(thread);
                }
                self.send(json!({"id":wire_id,"method":method,"params":params}));
            }
        }
        known.retain(|key, _| {
            let present = seen.contains(key);
            if !present {
                self.send(json!({"method":"serverRequest/resolved","params":{"threadId":thread,"requestId":key}}));
            }
            present
        });
    }

    async fn report_turn_end(&self, thread: &str, turn: &str, boundary: Option<&Boundary>) {
        let from_rollout = match boundary {
            Some(Boundary { kind: BoundaryKind::Completed, turn_id }) if turn_id.as_deref() == Some(turn) => "completed",
            _ => "interrupted",
        };
        // Only the native reader can tell "failed" and carries its error.
        let native = self
            .backend_read("thread/turns/list", &json!({"threadId":thread,"cursor":null,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"}))
            .await
            .ok()
            .and_then(|reply| reply.pointer("/result/data/0").cloned())
            .filter(|row| row["id"].as_str() == Some(turn));
        let mut payload = json!({"id":turn,"status":from_rollout});
        if let Some(row) = native.filter(|row| matches!(row["status"].as_str(), Some("completed" | "failed" | "interrupted"))) {
            payload["status"] = row["status"].clone();
            if !row["error"].is_null() {
                payload["error"] = row["error"].clone();
            }
        }
        self.send(json!({"method":"turn/completed","params":{"threadId":thread,"turn":payload}}));
    }

    async fn owner_lost(&self, thread: &str, known: &mut HashMap<String, Value>, turns: &mut TurnTracker) {
        for (key, _) in known.drain() {
            self.send(json!({"method":"serverRequest/resolved","params":{"threadId":thread,"requestId":key}}));
        }
        if let Some(turn) = turns.take_open() {
            let boundary = self.rollouts.lock().unwrap().get(thread).and_then(|path| native_files::latest_boundary(path));
            self.report_turn_end(thread, &turn, boundary.as_ref()).await;
        }
        let changed = {
            let mut follows = self.follows.lock().unwrap();
            follows.get_mut(thread).is_some_and(|follow| follow.status.take().is_some())
        };
        if changed {
            self.send(json!({"method":"thread/status/changed","params":{"threadId":thread,"status":{"type":"notLoaded"}}}));
        }
    }

    /// `None` means no reply: the outcome is uncertain and the Host must record UNKNOWN.
    pub async fn handle_request(self: Arc<Self>, id: Value, method: String, params: Value) -> Option<Value> {
        let thread = params["threadId"].as_str().unwrap_or_default().to_owned();
        match method.as_str() {
            "initialize" => Some(json!({"id":id,"result":{
                "userAgent":concat!("agbrio-light/", env!("CARGO_PKG_VERSION")),
                "codexHome":self.config.codex_home,"platformFamily":"windows","platformOs":"windows"
            }})),
            m if is_thread_read(m) => Some(self.thread_read(&id, m, &params).await),
            m if catalog_ttl(m).is_some() => Some(self.catalog_read(&id, m, &params).await),
            "thread/goal/set" | "thread/goal/clear" => Some(rpc_error(&id, -32601, "CODEX_LIGHT_GOAL_WRITE_UNSUPPORTED")),
            "thread/backgroundTerminals/list" => match self.pinned_backend(&thread).await {
                Some(_) => self.backend_write(&id, &method, &params).await,
                None => Some(json!({"id":id,"result":{"data":[],"nextCursor":null}})),
            },
            "thread/start" => self.backend_write(&id, &method, &params).await,
            "thread/resume" => self.resume(&id, &thread, &params).await,
            "thread/unsubscribe" => self.unsubscribe(&id, &thread, &params).await,
            "thread/archive" | "thread/name/set" => match self.route(&thread, true).await {
                Route::Backend | Route::Free => self.backend_write(&id, &method, &params).await,
                Route::Desktop(..) | Route::HeldElsewhere => Some(Self::held_error(&id, &thread)),
            },
            "turn/start" => self.turn_start(&id, &thread, &params).await,
            "turn/steer" => self.turn_steer(&id, &thread, &params).await,
            "turn/interrupt" => self.turn_interrupt(&id, &thread, &params).await,
            _ => Some(rpc_error(&id, -32601, "CODEX_LIGHT_METHOD_UNSUPPORTED")),
        }
    }

    async fn resume(self: &Arc<Self>, id: &Value, thread: &str, params: &Value) -> Option<Value> {
        match self.route(thread, true).await {
            // Loaded here already. A brand-new thread answers this at once with its own error.
            Route::Backend => self.backend_write(id, "thread/resume", params).await,
            Route::Free => self.load_on_backend(id, thread, params).await,
            Route::HeldElsewhere => Some(Self::held_error(id, thread)),
            // Already loaded by Desktop. Attach as a follower and report its identity.
            Route::Desktop(..) => {
                self.start_follow(thread, true);
                Some(self.thread_read(id, "thread/read", &json!({"threadId":thread,"includeTurns":false})).await)
            }
        }
    }

    async fn unsubscribe(&self, id: &Value, thread: &str, params: &Value) -> Option<Value> {
        if self.pinned_backend(thread).await.is_some() {
            return self.backend_write(id, "thread/unsubscribe", params).await;
        }
        let mut follows = self.follows.lock().unwrap();
        let status = match follows.get_mut(thread) {
            Some(follow) => {
                // The loop ends at its next pass once idle, then releases the follow.
                follow.host_subscribed = false;
                "unsubscribed"
            }
            None => "notSubscribed",
        };
        Some(json!({"id":id,"result":{"status":status}}))
    }

    async fn turn_start(self: &Arc<Self>, id: &Value, thread: &str, params: &Value) -> Option<Value> {
        match self.route(thread, true).await {
            Route::Backend => self.backend_write(id, "turn/start", params).await,
            // The Host still counts a thread as loaded after the backend let it go, and app-server
            // refuses a turn on a thread it has not loaded. A resume starts nothing, so a failed
            // one is a plain refusal: the turn was never sent.
            Route::Free => {
                let resumed = self.load_on_backend(id, thread, &json!({"threadId":thread,"excludeTurns":true})).await?;
                if resumed.get("error").is_some() {
                    return Some(resumed);
                }
                self.backend_write(id, "turn/start", params).await
            }
            Route::HeldElsewhere => Some(Self::held_error(id, thread)),
            Route::Desktop(connection, owner) => {
                if !connection.writes_allowed {
                    return Some(Self::version_error(id));
                }
                let mut request = params.clone();
                request["input"] = with_text_elements(&params["input"]);
                if request.get("clientUserMessageId").is_none() {
                    request["clientUserMessageId"] = json!(uuid::Uuid::new_v4().to_string());
                }
                self.start_follow(thread, false);
                let body = json!({"conversationId":thread,"turnStart":{"request":request,"context":{"inheritThreadSettings":true}}});
                match connection.owner_request(&owner, "thread-follower-start-turn", body, WRITE_TIMEOUT).await {
                    Reply::Success(result) => {
                        self.announce_turn(thread, result.pointer("/result/turn/id").and_then(Value::as_str));
                        Some(json!({"id":id,"result":result["result"]}))
                    }
                    Reply::Rejected(error) => Some(rpc_error(id, -32000, &format!("CODEX_LIGHT_DESKTOP_REJECTED: {error}"))),
                    Reply::Uncertain => None,
                }
            }
        }
    }

    async fn turn_steer(self: &Arc<Self>, id: &Value, thread: &str, params: &Value) -> Option<Value> {
        match self.route(thread, true).await {
            Route::Backend | Route::Free => self.backend_write(id, "turn/steer", params).await,
            Route::HeldElsewhere => Some(Self::held_error(id, thread)),
            Route::Desktop(connection, owner) => {
                if !connection.writes_allowed {
                    return Some(Self::version_error(id));
                }
                // Desktop's handler does not check this, so a stale steer would land on
                // whichever turn is running. Native app-server rejects it; so does this.
                let boundary = self.rollouts.lock().unwrap().get(thread).and_then(|path| native_files::latest_boundary(path));
                let current = boundary.filter(|b| b.kind == BoundaryKind::Started).and_then(|b| b.turn_id);
                if let Some(expected) = params["expectedTurnId"].as_str() {
                    if current.as_deref() != Some(expected) {
                        return Some(rpc_error(id, -32600, "CODEX_LIGHT_STEER_TURN_MISMATCH: expected turn is not the running turn"));
                    }
                }
                let metadata = self.thread_read(id, "thread/read", &json!({"threadId":thread,"includeTurns":false})).await;
                let Some(cwd) = metadata.pointer("/result/thread/cwd").and_then(Value::as_str) else {
                    return Some(rpc_error(id, -32000, "CODEX_LIGHT_THREAD_METADATA_UNAVAILABLE"));
                };
                self.start_follow(thread, false);
                let body = steer_params(thread, params, cwd, &Value::Null);
                match connection.owner_request(&owner, "thread-follower-steer-turn", body, WRITE_TIMEOUT).await {
                    Reply::Success(result) => Some(json!({"id":id,"result":result["result"]})),
                    Reply::Rejected(error) => Some(rpc_error(id, -32000, &format!("CODEX_LIGHT_DESKTOP_REJECTED: {error}"))),
                    Reply::Uncertain => None,
                }
            }
        }
    }

    async fn turn_interrupt(&self, id: &Value, thread: &str, params: &Value) -> Option<Value> {
        match self.route(thread, true).await {
            Route::Backend | Route::Free => self.backend_write(id, "turn/interrupt", params).await,
            Route::HeldElsewhere => Some(Self::held_error(id, thread)),
            Route::Desktop(connection, owner) => {
                if !connection.writes_allowed {
                    return Some(Self::version_error(id));
                }
                let turn = params["turnId"].as_str().unwrap_or_default();
                let body = json!({"conversationId":thread,"mode":"user-stop","expectedTurnId":turn});
                match connection.owner_request(&owner, "thread-follower-interrupt-turn", body, WRITE_TIMEOUT).await {
                    // Desktop answers ok for a stale turn and touches nothing: only the
                    // interrupted ID says that this exact turn was stopped.
                    Reply::Success(result) if result["interruptedTurnId"].as_str() == Some(turn) => Some(json!({"id":id,"result":{}})),
                    Reply::Success(_) => Some(rpc_error(id, -32600, "CODEX_LIGHT_INTERRUPT_TURN_NOT_RUNNING")),
                    Reply::Rejected(error) => Some(rpc_error(id, -32000, &format!("CODEX_LIGHT_DESKTOP_REJECTED: {error}"))),
                    Reply::Uncertain => None,
                }
            }
        }
    }

    /// The Host's answer to a native request it was shown.
    pub async fn handle_response(&self, id: &Value, result: &Value) {
        let Some(id) = id.as_str() else { return };
        if let Some(raw) = id.strip_prefix("backend:") {
            if let Some(backend) = self.backend.lock().await.as_ref() {
                backend.answer_server_request(raw, result);
            }
            return;
        }
        let Some((thread, method, request_id)) = parse_host_request_id(id) else { return };
        let Some((follower_method, uses_decision)) = approval_method(&method) else { return };
        let Some((connection, owner)) = self.owner_of(&thread, true).await else { return };
        if !connection.writes_allowed {
            return;
        }
        let mut body = json!({"conversationId":thread,"requestId":request_id});
        if uses_decision {
            body["decision"] = result["decision"].clone();
        } else {
            body["response"] = result.clone();
        }
        // An unconfirmed answer stays on the card through the next snapshot; never resent here.
        let _ = connection.owner_request(&owner, follower_method, body, WRITE_TIMEOUT).await;
    }

    /// Lets the backend exit once nothing needs it.
    pub async fn reap(&self) {
        let mut slot = self.backend.lock().await;
        let Some(backend) = slot.clone() else { return };
        let released = backend.release_finished(self.config.release_after_turn);
        let hub = self.me.upgrade();
        // Exiting frees a held thread at once, while an unsubscribed one waits out
        // app-server's own unload delay. Desktop cannot open the thread until then.
        let free_now = !released.is_empty() && !backend.has_pins();
        if backend.is_dead() {
            // The process and its turns are gone, so nothing is left to wait for.
            slot.take();
            backend.shut_down();
            self.sessions.close_all();
            return;
        }
        if free_now || backend.reapable(self.config.idle_grace) {
            slot.take();
            backend.shut_down();
            if let Some(hub) = &hub {
                for thread in released {
                    hub.wait_then_close_owner(thread);
                }
            }
            return;
        }
        drop(slot);
        for thread in released {
            let _ = backend.call("thread/unsubscribe", json!({"threadId":thread}), Duration::from_secs(10)).await;
            if let Some(hub) = &hub {
                hub.wait_then_close_owner(thread);
            }
        }
    }

    pub async fn shut_down(&self) {
        self.sessions.close_all();
        if let Some(backend) = self.backend.lock().await.take() {
            backend.shut_down();
        }
    }
}

/// Runs against any line-oriented input and output. Returns when input ends.
pub(super) async fn serve(config: Config, mut input: mpsc::UnboundedReceiver<String>, out: mpsc::UnboundedSender<Value>) {
    let hub = Hub::new(config, out.clone());
    let reaper = {
        let hub = hub.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                hub.reap().await;
            }
        })
    };
    while let Some(line) = input.recv().await {
        let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
        match (message.get("id").cloned(), message.get("method").and_then(Value::as_str).map(str::to_owned)) {
            (Some(id), Some(method)) => {
                let hub = hub.clone();
                let out = out.clone();
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                tokio::spawn(async move {
                    if let Some(reply) = hub.handle_request(id, method, params).await {
                        let _ = out.send(reply);
                    }
                });
            }
            (Some(id), None) => {
                let hub = hub.clone();
                let result = message.get("result").cloned().unwrap_or(Value::Null);
                tokio::spawn(async move { hub.handle_response(&id, &result).await });
            }
            _ => {}
        }
    }
    reaper.abort();
    hub.shut_down().await;
}

pub(super) fn run_stdio() {
    let config = match Config::production() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() else {
        eprintln!("CODEX_LIGHT_RUNTIME_UNAVAILABLE");
        return;
    };
    let (line_sender, lines) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if line_sender.send(line).is_err() {
                break;
            }
        }
    });
    let (out, mut outgoing) = mpsc::unbounded_channel::<Value>();
    let writer = std::thread::spawn(move || {
        let mut stdout = std::io::stdout().lock();
        while let Some(message) = outgoing.blocking_recv() {
            if writeln!(stdout, "{message}").is_err() || stdout.flush().is_err() {
                break;
            }
        }
    });
    runtime.block_on(serve(config, lines, out));
    runtime.shutdown_timeout(Duration::from_secs(1));
    drop(writer);
}
