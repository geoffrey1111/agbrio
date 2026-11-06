//! The on-demand stdio app-server. It exits when nothing needs it.
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};

/// A held thread cannot be opened in Desktop, so a pin lasts only as long as it must.
/// An unsent new thread only exists in this backend, so it waits for its first turn.
pub(super) const NEW_THREAD_LIMIT: Duration = Duration::from_secs(15 * 60);
/// A resume is normally followed at once by the send it was made for.
pub(super) const RESUMED_LIMIT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PinKind {
    Created,
    Resumed,
}

#[derive(Clone)]
struct Pin {
    kind: PinKind,
    active: bool,
    had_turn: bool,
    since: Instant,
}

pub(super) struct Backend {
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    next_id: AtomicU64,
    pins: Arc<Mutex<HashMap<String, Pin>>>,
    last_used: Mutex<Instant>,
    in_flight: AtomicU64,
    dead: Arc<AtomicBool>,
}

/// A call whose outcome is unknown because the process went away.
pub(super) struct BackendLost;

impl Backend {
    pub fn start(mut command: Command, out: mpsc::UnboundedSender<Value>) -> Result<Arc<Self>, String> {
        command.args(["app-server", "--stdio"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = command.spawn().map_err(|_| "CODEX_LIGHT_BACKEND_START_FAILED")?;
        let stdin = child.stdin.take().ok_or("CODEX_LIGHT_BACKEND_START_FAILED")?;
        let stdout = child.stdout.take().ok_or("CODEX_LIGHT_BACKEND_START_FAILED")?;
        let backend = Arc::new(Self {
            child: Mutex::new(Some(child)),
            stdin: Mutex::new(Some(stdin)),
            pending: Arc::default(),
            next_id: AtomicU64::new(1),
            pins: Arc::default(),
            last_used: Mutex::new(Instant::now()),
            in_flight: AtomicU64::new(0),
            dead: Arc::default(),
        });
        let reader = backend.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
                reader.dispatch(message, &out);
            }
            reader.dead.store(true, Ordering::SeqCst);
            reader.pending.lock().unwrap().clear();
            // Running turns died with the process. Report that, as the Host cannot see it.
            let pins = std::mem::take(&mut *reader.pins.lock().unwrap());
            for (thread, pin) in pins {
                if pin.active {
                    let _ = out.send(json!({"method":"thread/status/changed","params":{"threadId":thread,"status":{"type":"notLoaded"}}}));
                }
            }
        });
        Ok(backend)
    }

    fn dispatch(&self, message: Value, out: &mpsc::UnboundedSender<Value>) {
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str).map(str::to_owned);
        match (id, method) {
            (Some(id), None) => {
                let sender = id.as_u64().and_then(|id| self.pending.lock().unwrap().remove(&id));
                if let Some(sender) = sender {
                    let _ = sender.send(message);
                }
            }
            (Some(id), Some(_)) => {
                // A server request. Tag its id so the Host's answer finds its way back here.
                let mut forwarded = message;
                forwarded["id"] = json!(format!("backend:{id}"));
                let _ = out.send(forwarded);
            }
            (None, Some(method)) => {
                self.track(&method, &message);
                let mut message = message;
                // The Host knows this request by the tagged id it was shown.
                if method == "serverRequest/resolved" {
                    if let Some(raw) = message.pointer("/params/requestId").cloned() {
                        message["params"]["requestId"] = json!(format!("backend:{raw}"));
                    }
                }
                let _ = out.send(message);
            }
            _ => {}
        }
    }

    fn track(&self, method: &str, message: &Value) {
        let thread = message.pointer("/params/threadId").or_else(|| message.pointer("/params/thread/id")).and_then(Value::as_str);
        let Some(thread) = thread else { return };
        let mut pins = self.pins.lock().unwrap();
        match method {
            "turn/started" => {
                if let Some(pin) = pins.get_mut(thread) {
                    pin.active = true;
                    pin.had_turn = true;
                    pin.since = Instant::now();
                }
            }
            "turn/completed" => {
                if let Some(pin) = pins.get_mut(thread) {
                    pin.active = false;
                    pin.since = Instant::now();
                }
            }
            "thread/closed" => {
                pins.remove(thread);
            }
            _ => {}
        }
    }

    pub async fn initialize(&self) -> Result<(), String> {
        let reply = self
            .call("initialize", json!({"clientInfo":{"name":"agbrio_light","title":"Agbrio","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true,"requestAttestation":false}}), Duration::from_secs(30))
            .await
            .map_err(|_| "CODEX_LIGHT_BACKEND_START_FAILED")?;
        if reply.get("error").is_some() {
            return Err("CODEX_LIGHT_BACKEND_START_FAILED".into());
        }
        self.write(&json!({"method":"initialized"}));
        Ok(())
    }

    fn write(&self, message: &Value) -> bool {
        let mut stdin = self.stdin.lock().unwrap();
        let Some(stdin) = stdin.as_mut() else { return false };
        writeln!(stdin, "{message}").is_ok() && stdin.flush().is_ok()
    }

    pub async fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, BackendLost> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, sender);
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        *self.last_used.lock().unwrap() = Instant::now();
        let reply = if self.write(&json!({"id":id,"method":method,"params":params})) {
            tokio::time::timeout(timeout, receiver).await.ok().and_then(Result::ok)
        } else {
            None
        };
        self.pending.lock().unwrap().remove(&id);
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        *self.last_used.lock().unwrap() = Instant::now();
        reply.ok_or(BackendLost)
    }

    pub fn answer_server_request(&self, raw_id: &str, result: &Value) -> bool {
        let id: Value = serde_json::from_str(raw_id).unwrap_or_else(|_| json!(raw_id));
        self.write(&json!({"id":id,"result":result}))
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::SeqCst)
    }

    pub fn pin(&self, thread: &str, kind: PinKind) {
        // The Host resumes a thread it sees running, so an existing pin keeps what it knows:
        // overwriting it would forget the running turn and could release the thread mid-task.
        self.pins.lock().unwrap().entry(thread.to_owned()).or_insert(Pin { kind, active: false, had_turn: false, since: Instant::now() });
    }

    pub fn mark_active(&self, thread: &str) {
        if let Some(pin) = self.pins.lock().unwrap().get_mut(thread) {
            pin.active = true;
            pin.had_turn = true;
            pin.since = Instant::now();
        }
    }

    pub fn mark_idle(&self, thread: &str) {
        if let Some(pin) = self.pins.lock().unwrap().get_mut(thread) {
            pin.active = false;
            pin.since = Instant::now();
        }
    }

    pub fn unpin(&self, thread: &str) {
        self.pins.lock().unwrap().remove(thread);
    }

    pub fn is_pinned(&self, thread: &str) -> bool {
        self.pins.lock().unwrap().contains_key(thread)
    }

    /// Drops every pin nothing needs any more and returns those threads.
    pub fn release_finished(&self, after_turn: Duration) -> Vec<String> {
        let mut pins = self.pins.lock().unwrap();
        let finished: Vec<String> = pins
            .iter()
            .filter(|(_, pin)| {
                let limit = match (pin.had_turn, pin.kind) {
                    (true, _) => after_turn,
                    (false, PinKind::Resumed) => RESUMED_LIMIT,
                    (false, PinKind::Created) => NEW_THREAD_LIMIT,
                };
                !pin.active && pin.since.elapsed() >= limit
            })
            .map(|(thread, _)| thread.clone())
            .collect();
        for thread in &finished {
            pins.remove(thread);
        }
        finished
    }

    pub fn has_pins(&self) -> bool {
        !self.pins.lock().unwrap().is_empty()
    }

    /// True once nothing needs this process.
    pub fn reapable(&self, idle_grace: Duration) -> bool {
        self.in_flight.load(Ordering::SeqCst) == 0 && !self.has_pins() && self.last_used.lock().unwrap().elapsed() >= idle_grace
    }

    /// Closing stdin is how app-server exits cleanly; killing is the fallback.
    pub fn shut_down(&self) {
        self.stdin.lock().unwrap().take();
        let child = self.child.lock().unwrap().take();
        if let Some(mut child) = child {
            std::thread::spawn(move || {
                for _ in 0..50 {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                let _ = child.kill();
                let _ = child.wait();
            });
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.shut_down();
    }
}
