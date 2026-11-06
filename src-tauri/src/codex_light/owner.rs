//! Lets Desktop show and use a thread that is running on the on-demand backend.
//!
//! A thread's writer lock keeps Desktop from loading it while the backend holds it. Desktop
//! has a way to share a thread between its own windows: one window is the owner, and the
//! others follow its state over the pipe and send it their requests. This answers as that
//! owner for exactly the threads loaded on this process's backend, so Desktop opens them as
//! a follower instead of reporting them as open in another app.
//!
//! Each thread has its own pipe connection. Closing it is what tells Desktop the owner has
//! gone, and Desktop then loads the thread itself, so it closes only once the lock is free.
use super::{
    backend::{Backend, PinKind},
    hub::Hub,
    ipc::{Connection, OwnerEvent, PeerPolicy},
    owner_state::OwnerState,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const HISTORY_TURNS: u64 = 20;
const SEND_INTERVAL: Duration = Duration::from_millis(400);
/// A large state takes real work to send and to render, so it is sent less often.
const BYTES_PER_EXTRA_SECOND: usize = 2 * 1024 * 1024;
const RECONNECT_DELAY: Duration = Duration::from_secs(3);
/// A new thread's rollout exists an instant after its first turn begins.
const ANNOUNCE_DELAYS: [Duration; 3] = [Duration::from_millis(1500), Duration::from_secs(4), Duration::from_secs(15)];
const RELEASE_LOCK_WAIT: Duration = Duration::from_secs(30);
const HOST: &str = "local";

pub(super) struct Session {
    thread: String,
    state: Mutex<OwnerState>,
    is_new: bool,
    dirty: AtomicBool,
    released: AtomicBool,
    revision: AtomicU64,
    followers: Mutex<HashSet<String>>,
    connection: Mutex<Option<Arc<Connection>>>,
}

#[derive(Default)]
pub(super) struct Sessions(Mutex<HashMap<String, Arc<Session>>>);

impl Session {
    fn publish(&self, connection: &Connection, targets: Vec<String>) -> usize {
        if targets.is_empty() {
            return 0;
        }
        let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        let state = self.state.lock().unwrap().snapshot();
        connection.broadcast(
            "thread-stream-state-changed",
            Some(targets),
            json!({"conversationId":self.thread,"hostId":HOST,"change":{"type":"snapshot","revision":revision,"conversationState":state}}),
        )
    }

    fn close(&self) {
        self.released.store(true, Ordering::SeqCst);
        if let Some(connection) = self.connection.lock().unwrap().take() {
            connection.close();
        }
    }
}

impl Sessions {
    fn get(&self, thread: &str) -> Option<Arc<Session>> {
        self.0.lock().unwrap().get(thread).cloned()
    }

    /// False when this thread already has a session.
    pub fn add(&self, thread: &str, state: OwnerState, kind: PinKind) -> Option<Arc<Session>> {
        let mut sessions = self.0.lock().unwrap();
        if sessions.contains_key(thread) {
            return None;
        }
        let session = Arc::new(Session {
            thread: thread.to_owned(),
            state: Mutex::new(state),
            is_new: kind == PinKind::Created,
            dirty: AtomicBool::new(false),
            released: AtomicBool::new(false),
            revision: AtomicU64::new(0),
            followers: Mutex::default(),
            connection: Mutex::default(),
        });
        sessions.insert(thread.to_owned(), session.clone());
        Some(session)
    }

    /// Everything the backend says passes through here on its way to the Host.
    pub fn observe(&self, message: &Value) {
        let params = &message["params"];
        let thread = params["threadId"].as_str().or_else(|| params.pointer("/thread/id").and_then(Value::as_str));
        let Some(session) = thread.and_then(|thread| self.get(thread)) else { return };
        let Some(method) = message["method"].as_str() else { return };
        let mut state = session.state.lock().unwrap();
        let changed = match message.get("id") {
            // The backend tags a request's id so the Host's answer finds its way back.
            Some(id) => {
                let raw = id.as_str().and_then(|id| id.strip_prefix("backend:")).and_then(|raw| serde_json::from_str(raw).ok()).unwrap_or_else(|| id.clone());
                state.add_request(raw, method, params.clone());
                true
            }
            None => state.apply(method, params),
        };
        drop(state);
        if changed {
            session.dirty.store(true, Ordering::SeqCst);
        }
    }

    pub fn note_turn_request(&self, thread: &str, turn: &str, request: &Value) {
        if let Some(session) = self.get(thread) {
            session.state.lock().unwrap().note_turn_request(turn, request);
            session.dirty.store(true, Ordering::SeqCst);
        }
    }

    pub fn close(&self, thread: &str) {
        if let Some(session) = self.0.lock().unwrap().remove(thread) {
            session.close();
        }
    }

    pub fn close_all(&self) {
        for (_, session) in self.0.lock().unwrap().drain() {
            session.close();
        }
    }
}

impl Hub {
    /// Answers for a thread the backend just loaded. Fails closed to the plain behaviour:
    /// on an unvalidated Desktop nothing is claimed.
    pub(super) fn claim(self: Arc<Self>, thread: &str, result: &Value, kind: PinKind, backend: Arc<Backend>, policy: PeerPolicy, pipe: String) {
        let Some(state) = OwnerState::from_reply(result) else { return };
        let Some(session) = self.owners().add(thread, state, kind) else { return };
        tokio::spawn(async move {
            if kind == PinKind::Resumed {
                let reply = backend.call("thread/turns/list", json!({"threadId":session.thread,"cursor":null,"limit":HISTORY_TURNS,"sortDirection":"desc","itemsView":"full"}), Duration::from_secs(60)).await;
                if let Some(rows) = reply.ok().as_ref().and_then(|reply| reply.pointer("/result/data")).and_then(Value::as_array) {
                    session.state.lock().unwrap().load_turns(rows, true);
                }
            }
            self.owner_loop(session, policy, pipe).await;
        });
    }

    async fn owner_loop(self: Arc<Self>, session: Arc<Session>, policy: PeerPolicy, pipe: String) {
        let opened_at = Instant::now();
        let mut announced = 0;
        let mut first_turn_at: Option<Instant> = None;
        while !session.released.load(Ordering::SeqCst) {
            let (events_in, mut events) = tokio::sync::mpsc::unbounded_channel();
            let opened = Connection::open_owner(&pipe, &policy, &session.thread, events_in).await;
            let connection = match opened {
                Ok(connection) if connection.owner_allowed => connection,
                Ok(connection) => {
                    connection.close();
                    return;
                }
                // Desktop is not running. It may open at any time.
                Err(_) => {
                    tokio::time::sleep(RECONNECT_DELAY).await;
                    continue;
                }
            };
            *session.connection.lock().unwrap() = Some(connection.clone());
            if session.released.load(Ordering::SeqCst) {
                connection.close();
                return;
            }
            session.followers.lock().unwrap().clear();
            connection.broadcast("thread-stream-following-status-requested", None, json!({"conversationId":session.thread,"hostId":HOST}));
            let mut next_send = Instant::now();
            loop {
                let wait = tokio::time::sleep(Duration::from_millis(200));
                tokio::select! {
                    event = events.recv() => match event {
                        Some(OwnerEvent::Request(message)) => {
                            let hub = self.clone();
                            let session = session.clone();
                            let connection = connection.clone();
                            tokio::spawn(async move { hub.follower_request(&session, &connection, message).await });
                        }
                        Some(OwnerEvent::Following { client, following }) => {
                            let mut followers = session.followers.lock().unwrap();
                            if following {
                                followers.insert(client.clone());
                                drop(followers);
                                session.publish(&connection, vec![client]);
                            } else {
                                followers.remove(&client);
                            }
                        }
                        Some(OwnerEvent::ClientGone(client)) => {
                            session.followers.lock().unwrap().remove(&client);
                        }
                        Some(OwnerEvent::Disconnected) | None => break,
                    },
                    () = wait => {}
                }
                if session.released.load(Ordering::SeqCst) {
                    return;
                }
                if session.dirty.load(Ordering::SeqCst) && Instant::now() >= next_send {
                    session.dirty.store(false, Ordering::SeqCst);
                    let targets: Vec<String> = session.followers.lock().unwrap().iter().cloned().collect();
                    let bytes = session.publish(&connection, targets);
                    next_send = Instant::now() + SEND_INTERVAL + Duration::from_secs_f64(bytes as f64 / BYTES_PER_EXTRA_SECOND as f64);
                }
                // Desktop reads the thread itself and lists it, so no restart is needed to see it.
                if session.is_new && announced < ANNOUNCE_DELAYS.len() {
                    if first_turn_at.is_none() && !session.state.lock().unwrap().is_empty() {
                        first_turn_at = Some(Instant::now());
                    }
                    if first_turn_at.is_some_and(|at| at.elapsed() >= ANNOUNCE_DELAYS[announced]) {
                        connection.broadcast("thread-unarchived", None, json!({"hostId":HOST,"conversationId":session.thread}));
                        announced += 1;
                    }
                }
            }
            *session.connection.lock().unwrap() = None;
            if opened_at.elapsed() < RECONNECT_DELAY {
                tokio::time::sleep(RECONNECT_DELAY).await;
            }
        }
    }

    async fn follower_request(&self, session: &Session, connection: &Connection, message: Value) {
        let method = message["method"].as_str().unwrap_or_default().to_owned();
        let request_id = message["requestId"].clone();
        match self.follower_answer(session, &method, &message["params"]).await {
            Ok(result) => connection.respond(&request_id, &method, result),
            Err(error) => connection.respond_error(&request_id, &error),
        }
    }

    /// `Err` tells Desktop the request failed. Nothing here is ever retried.
    async fn follower_answer(&self, session: &Session, method: &str, params: &Value) -> Result<Value, String> {
        let thread = session.thread.as_str();
        let backend = || async { self.pinned_backend(thread).await.ok_or_else(|| "no-client-found".to_owned()) };
        let answer = |backend: Arc<Backend>, result: Value| {
            if backend.answer_server_request(&params["requestId"].to_string(), &result) {
                Ok(json!({"ok":true}))
            } else {
                Err("no-client-found".to_owned())
            }
        };
        match method {
            "thread-owner-discovery" => Ok(json!({"supportsUntrustedAppInput":true})),
            "thread-follower-start-turn" => {
                backend().await?;
                let mut request = params.pointer("/turnStart/request").cloned().unwrap_or_else(|| params["turnStart"].clone());
                request["threadId"] = json!(thread);
                Ok(json!({"result":self.follower_write("turn/start", &request).await?}))
            }
            "thread-follower-steer-turn" => {
                backend().await?;
                let turn = session.state.lock().unwrap().running_turn().ok_or("no-active-turn")?;
                let request = json!({"threadId":thread,"input":params["input"],"expectedTurnId":turn});
                Ok(json!({"result":self.follower_write("turn/steer", &request).await?}))
            }
            "thread-follower-interrupt-turn" => {
                backend().await?;
                let running = session.state.lock().unwrap().running_turn();
                let Some(turn) = params["expectedTurnId"].as_str().map(str::to_owned).or(running.clone()) else { return Ok(json!({"interruptedTurnId":null,"ok":true})) };
                if running.as_deref() != Some(turn.as_str()) {
                    return Ok(json!({"interruptedTurnId":null,"ok":true}));
                }
                self.follower_write("turn/interrupt", &json!({"threadId":thread,"turnId":turn})).await?;
                Ok(json!({"interruptedTurnId":turn,"ok":true}))
            }
            "thread-follower-command-approval-decision" | "thread-follower-file-approval-decision" => answer(backend().await?, json!({"decision":params["decision"]})),
            "thread-follower-permissions-request-approval-response" | "thread-follower-submit-user-input" | "thread-follower-submit-mcp-server-elicitation-response" => answer(backend().await?, params["response"].clone()),
            "thread-follower-read-model-settings" => Ok(json!({"settings":session.state.lock().unwrap().model_settings()})),
            "thread-follower-update-thread-settings" => Ok(json!({"applied":false})),
            "thread-follower-load-complete-history" => {
                let targets: Vec<String> = session.followers.lock().unwrap().iter().cloned().collect();
                session.publish(connection_of(session)?.as_ref(), targets);
                Ok(json!({"revision":session.revision.load(Ordering::SeqCst)}))
            }
            "thread-follower-compact-thread" => {
                backend().await?;
                self.follower_write("thread/compact/start", &json!({"threadId":thread})).await?;
                Ok(json!({"ok":true}))
            }
            "thread-follower-remove-queued-message" => Ok(json!({"removed":false})),
            "thread-follower-clear-queued-messages" => Ok(json!({"messages":[]})),
            _ => Err("unsupported-while-running-on-agbrio".to_owned()),
        }
    }

    pub(super) fn wait_then_close_owner(self: &Arc<Self>, thread: String) {
        let hub = self.clone();
        tokio::spawn(async move {
            let started = Instant::now();
            // Closing tells Desktop to load the thread, which fails while the lock is held.
            while hub.lock_held(&thread) && started.elapsed() < RELEASE_LOCK_WAIT {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            hub.owners().close(&thread);
        });
    }
}

fn connection_of(session: &Session) -> Result<Arc<Connection>, String> {
    session.connection.lock().unwrap().clone().ok_or_else(|| "no-client-found".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(id: &str) -> OwnerState {
        OwnerState::from_reply(&json!({"thread":{"id":id},"model":"m"})).unwrap()
    }

    #[test]
    fn notifications_reach_only_their_own_session() {
        let sessions = Sessions::default();
        let a = sessions.add("a", state("a"), PinKind::Created).unwrap();
        let b = sessions.add("b", state("b"), PinKind::Resumed).unwrap();
        assert!(sessions.add("a", state("a"), PinKind::Created).is_none(), "one session per thread");
        sessions.observe(&json!({"method":"turn/started","params":{"threadId":"a","turn":{"id":"u"}}}));
        assert!(a.dirty.load(Ordering::SeqCst));
        assert!(!b.dirty.load(Ordering::SeqCst));
        assert_eq!(a.state.lock().unwrap().running_turn().as_deref(), Some("u"));
        sessions.observe(&json!({"method":"turn/started","params":{"threadId":"unknown","turn":{"id":"u"}}}));
    }

    #[test]
    fn a_backend_request_keeps_the_id_the_backend_gave_it() {
        let sessions = Sessions::default();
        let a = sessions.add("a", state("a"), PinKind::Created).unwrap();
        sessions.observe(&json!({"id":"backend:7","method":"item/commandExecution/requestApproval","params":{"threadId":"a"}}));
        sessions.observe(&json!({"id":"backend:\"x\"","method":"item/fileChange/requestApproval","params":{"threadId":"a"}}));
        let snapshot = a.state.lock().unwrap().snapshot();
        assert_eq!(snapshot["requests"][0]["id"], 7);
        assert_eq!(snapshot["requests"][1]["id"], "x");
        sessions.observe(&json!({"method":"serverRequest/resolved","params":{"threadId":"a","requestId":"backend:\"x\""}}));
        sessions.observe(&json!({"method":"serverRequest/resolved","params":{"threadId":"a","requestId":"backend:7"}}));
        assert!(a.state.lock().unwrap().snapshot()["requests"].as_array().unwrap().is_empty());
    }

    #[test]
    fn closing_marks_the_session_released_and_forgets_it() {
        let sessions = Sessions::default();
        let a = sessions.add("a", state("a"), PinKind::Created).unwrap();
        sessions.close("a");
        assert!(a.released.load(Ordering::SeqCst));
        assert!(sessions.get("a").is_none());
        let b = sessions.add("b", state("b"), PinKind::Created).unwrap();
        sessions.close_all();
        assert!(b.released.load(Ordering::SeqCst));
    }
}
