//! Client for Desktop's private owner/follower pipe. An ordinary connection claims nothing;
//! an owner connection claims the one thread it was opened for.
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::ClientOptions,
    sync::{mpsc, oneshot},
};

pub(super) const OFFICIAL_PIPE: &str = r"\\.\pipe\codex-ipc";
const SNAPSHOT_FRAME_LIMIT: usize = 64 * 1024 * 1024;
const PROTOCOL_FRAME_LIMIT: usize = 256 * 1024 * 1024;
const LOCAL_HOST: &str = "local";
const INITIALIZING_CLIENT: &str = "initializing-client";

/// Every version this client was validated against, on Desktop 26.1103.3517.0. A wrong
/// version is answered as `no-client-found`, which looks exactly like "no owner", so the
/// table has to be compared up front. Any difference closes IPC writes.
pub(super) const VALIDATED_VERSIONS: [(&str, u64); 9] = [
    ("thread-owner-discovery", 1),
    ("thread-stream-following-changed", 1),
    ("thread-stream-state-changed", 11),
    ("thread-follower-start-turn", 2),
    ("thread-follower-steer-turn", 1),
    ("thread-follower-interrupt-turn", 4),
    ("thread-follower-command-approval-decision", 1),
    ("thread-follower-file-approval-decision", 1),
    ("thread-follower-submit-user-input", 1),
];

/// The rest of the table an owner relies on. Desktop checks a broadcast's version and drops a
/// mismatch, so any difference means no thread is claimed.
const OWNER_VERSIONS: [(&str, u64); 12] = [
    ("thread-stream-following-status-requested", 1),
    ("thread-unarchived", 1),
    ("thread-follower-load-complete-history", 1),
    ("thread-follower-compact-thread", 1),
    ("thread-follower-update-thread-settings", 2),
    ("thread-follower-read-model-settings", 1),
    ("thread-follower-permissions-request-approval-response", 1),
    ("thread-follower-submit-mcp-server-elicitation-response", 1),
    ("thread-follower-set-queued-follow-ups-state", 1),
    ("thread-follower-remove-queued-message", 1),
    ("thread-follower-clear-queued-messages", 1),
    ("thread-follower-edit-last-user-turn", 2),
];

/// What Desktop sends the owner of a thread.
pub(super) enum OwnerEvent {
    Request(Value),
    Following { client: String, following: bool },
    ClientGone(String),
    Disconnected,
}

struct Claim {
    thread: String,
    events: mpsc::UnboundedSender<OwnerEvent>,
}

impl Claim {
    fn handles(&self, request: &Value) -> bool {
        let method = request["method"].as_str().unwrap_or_default();
        request.pointer("/params/conversationId").and_then(Value::as_str) == Some(self.thread.as_str()) && (method == "thread-owner-discovery" || method.starts_with("thread-follower-"))
    }
}

pub(super) enum StateEvent {
    Snapshot(Value),
    Changed,
    Resync,
    TooLarge,
    Disconnected,
}

/// Outcome of one owner-directed request. `Uncertain` means it may have taken effect.
pub(super) enum Reply {
    Success(Value),
    Rejected(String),
    Uncertain,
}

struct Subscriber {events:mpsc::UnboundedSender<StateEvent>,owner:String,mirror:super::follower_state::Mirror}
type Subscribers = Arc<Mutex<HashMap<String, Subscriber>>>;

struct StateWrite {done:Option<oneshot::Sender<()>>,pending:Arc<AtomicBool>,released:bool}
impl StateWrite {fn release(&mut self){if !self.released{self.released=true;self.pending.store(false,Ordering::SeqCst);}}}
impl Drop for StateWrite {fn drop(&mut self){self.release();}}
struct Outgoing {bytes:Vec<u8>,written:Option<StateWrite>}
impl From<Vec<u8>> for Outgoing {fn from(bytes:Vec<u8>)->Self{Self{bytes,written:None}}}

pub(super) struct Connection {
    writer: mpsc::UnboundedSender<Outgoing>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>,
    subscribers: Subscribers,
    client_id: String,
    closed: Arc<AtomicBool>,
    tasks: Mutex<Vec<tokio::task::AbortHandle>>,
    pub writes_allowed: bool,
    pub owner_allowed: bool,
    state_pending:Arc<AtomicBool>,
}

#[derive(Clone)]
pub(super) struct PeerPolicy {
    pub verify: bool,
    pub versions: Option<HashMap<String, u64>>,
}

fn is_official_image(image: &Path) -> bool {
    let text = image.to_string_lossy().to_ascii_lowercase();
    // WindowsApps content is only writable through package deployment.
    image.file_name().is_some_and(|name| name.eq_ignore_ascii_case("chatgpt.exe"))
        && text.contains(r"\windowsapps\openai.codex_")
}

fn peer_image(handle: *mut std::ffi::c_void) -> Option<PathBuf> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetNamedPipeServerProcessId(pipe: *mut std::ffi::c_void, pid: *mut u32) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn QueryFullProcessImageNameW(process: *mut std::ffi::c_void, flags: u32, name: *mut u16, size: *mut u32) -> i32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    let mut pid = 0_u32;
    if unsafe { GetNamedPipeServerProcessId(handle, &mut pid) } == 0 {
        return None;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut buffer = vec![0_u16; 4096];
    let mut size = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size) };
    unsafe { CloseHandle(process) };
    (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..size as usize])))
}

/// The method table inside the package serving the pipe.
fn installed_versions(image: &Path) -> Option<HashMap<String, u64>> {
    use std::io::{Read, Seek, SeekFrom};
    let asar = image.parent()?.join("resources/app.asar");
    let mut file = std::fs::File::open(asar).ok()?;
    let mut head = [0_u8; 16];
    file.read_exact(&mut head).ok()?;
    let header_size = u32::from_le_bytes(head[4..8].try_into().ok()?) as u64;
    let json_length = u32::from_le_bytes(head[12..16].try_into().ok()?) as usize;
    let mut header = vec![0_u8; json_length];
    file.read_exact(&mut header).ok()?;
    let header: Value = serde_json::from_slice(&header).ok()?;
    let entry = header.pointer("/files/.vite/files/build/files/early-bootstrap.js")?;
    let size = entry["size"].as_u64()?;
    let offset: u64 = entry["offset"].as_str()?.parse().ok()?;
    file.seek(SeekFrom::Start(8 + header_size + offset)).ok()?;
    let mut body = vec![0_u8; usize::try_from(size).ok()?];
    file.read_exact(&mut body).ok()?;
    parse_version_table(&String::from_utf8_lossy(&body))
}

pub(super) fn parse_version_table(text: &str) -> Option<HashMap<String, u64>> {
    let anchor = text.find("\"thread-owner-discovery\":")?;
    let start = text[..anchor].rfind('{')?;
    let end = anchor + text[anchor..].find('}')? + 1;
    serde_json::from_str(&text[start..end]).ok()
}

pub(super) fn versions_match(installed: &HashMap<String, u64>) -> bool {
    VALIDATED_VERSIONS.iter().all(|(method, version)| installed.get(*method) == Some(version))
}

pub(super) fn owner_versions_match(installed: &HashMap<String, u64>) -> bool {
    versions_match(installed) && OWNER_VERSIONS.iter().all(|(method, version)| installed.get(*method) == Some(version))
}

fn version_of(method: &str) -> u64 {
    VALIDATED_VERSIONS.iter().chain(OWNER_VERSIONS.iter()).find(|(name, _)| *name == method).map_or(0, |(_, version)| *version)
}

fn encode(message: &Value) -> Vec<u8> {
    let body = message.to_string().into_bytes();
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(&body);
    frame
}

impl Connection {
    /// Fails closed: an unverifiable server gets no frame at all.
    pub async fn open(pipe: &str, policy: &PeerPolicy, client_type: &str) -> Result<Arc<Self>, String> {
        Self::open_as(pipe, policy, client_type, None).await
    }

    /// A connection that answers as the owner of one thread until it is closed.
    pub async fn open_owner(pipe: &str, policy: &PeerPolicy, thread: &str, events: mpsc::UnboundedSender<OwnerEvent>) -> Result<Arc<Self>, String> {
        Self::open_as(pipe, policy, "agbrio-light", Some(Claim { thread: thread.to_owned(), events })).await
    }

    async fn open_as(pipe: &str, policy: &PeerPolicy, client_type: &str, claim: Option<Claim>) -> Result<Arc<Self>, String> {
        let client = ClientOptions::new().open(pipe).map_err(|error| format!("IPC_CONNECT_FAILED:{}", error.raw_os_error().unwrap_or(0)))?;
        let mut versions = policy.versions.clone();
        if policy.verify {
            let image = peer_image(client.as_raw_handle()).ok_or("IPC_PEER_UNVERIFIED")?;
            if !is_official_image(&image) {
                return Err("IPC_PEER_UNTRUSTED".into());
            }
            versions = installed_versions(&image);
        }
        let writes_allowed = versions.as_ref().is_some_and(versions_match);
        let owner_allowed = versions.as_ref().is_some_and(owner_versions_match);

        let (mut read, mut write) = tokio::io::split(client);
        let (writer, outgoing) = mpsc::unbounded_channel::<Outgoing>();
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>> = Arc::default();
        let subscribers: Subscribers = Arc::default();
        let closed = Arc::new(AtomicBool::new(false));

        let writing = tokio::spawn(async move {write_loop(&mut write,outgoing).await;});
        let reading = {
            let pending = pending.clone();
            let subscribers = subscribers.clone();
            let closed = closed.clone();
            let writer = writer.clone();
            tokio::spawn(async move {
                let _ = read_loop(&mut read, &writer, &pending, &subscribers, claim.as_ref()).await;
                closed.store(true, Ordering::SeqCst);
                pending.lock().unwrap().clear();
                for subscriber in subscribers.lock().unwrap().values() {
                    let _ = subscriber.events.send(StateEvent::Disconnected);
                }
                if let Some(claim) = &claim {
                    let _ = claim.events.send(OwnerEvent::Disconnected);
                }
            })
        };

        let tasks = Mutex::new(vec![writing.abort_handle(), reading.abort_handle()]);
        let mut connection = Self { writer, pending, subscribers, client_id: INITIALIZING_CLIENT.into(), closed, tasks, writes_allowed, owner_allowed,state_pending:Arc::new(AtomicBool::new(false)) };
        let reply = connection.exchange(json!({"type":"request","method":"initialize","version":0,"params":{"clientType":client_type}}), Duration::from_secs(5)).await;
        let client_id = reply.and_then(|m| m.pointer("/result/clientId").and_then(Value::as_str).map(str::to_owned)).ok_or("IPC_INITIALIZE_FAILED")?;
        connection.client_id = client_id;
        Ok(Arc::new(connection))
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Ends the connection, which Desktop sees as this client disconnecting.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        for task in self.tasks.lock().unwrap().drain(..) {
            task.abort();
        }
    }

    /// Sends to the named clients, or to every client. Returns the frame's size.
    pub fn broadcast(&self, method: &str, targets: Option<Vec<String>>, params: Value) -> usize {
        let mut message = json!({"type":"broadcast","method":method,"sourceClientId":self.client_id,"version":version_of(method),"params":params});
        if let Some(targets) = targets {
            message["targetClientIds"] = json!(targets);
        }
        let frame = encode(&message);
        let size = frame.len();
        let _ = self.writer.send(frame.into());
        size
    }

    /// A state publisher has at most one frame in flight. A stalled Desktop
    /// cannot retain an unbounded queue of obsolete snapshots/patches here.
    pub fn state_write_busy(&self)->bool{self.state_pending.load(Ordering::SeqCst)}
    pub async fn broadcast_state(&self,targets:Vec<String>,params:Value)->usize {
        if self.state_pending.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).is_err(){return 0;}
        let mut state=StateWrite{done:None,pending:self.state_pending.clone(),released:false};
        let frame=encode(&json!({"type":"broadcast","method":"thread-stream-state-changed","sourceClientId":self.client_id,"version":version_of("thread-stream-state-changed"),"targetClientIds":targets,"params":params}));
        if frame.len()>super::owner_stream::STATE_LIMIT+64*1024 {return 0;}
        let size=frame.len();let(done,written)=oneshot::channel();state.done=Some(done);
        if self.writer.send(Outgoing{bytes:frame,written:Some(state)}).is_err(){return 0;}
        // A timeout leaves the single frame in flight. Keep the connection and
        // provider requests alive; later updates wait and rebaseline afterwards.
        if !matches!(tokio::time::timeout(Duration::from_secs(3),written).await,Ok(Ok(()))) {return 0;}
        size
    }

    pub fn respond(&self, request_id: &Value, method: &str, result: Value) {
        let _ = self.writer.send(encode(&json!({"type":"response","requestId":request_id,"resultType":"success","method":method,"handledByClientId":self.client_id,"result":result})).into());
    }

    pub fn respond_error(&self, request_id: &Value, error: &str) {
        let _ = self.writer.send(encode(&json!({"type":"response","requestId":request_id,"resultType":"error","error":error})).into());
    }

    async fn exchange(&self, mut message: Value, timeout: Duration) -> Option<Value> {
        let request_id = uuid::Uuid::new_v4().to_string();
        message["requestId"] = json!(request_id);
        message["sourceClientId"] = json!(self.client_id);
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().unwrap().insert(request_id.clone(), sender);
        if self.writer.send(encode(&message).into()).is_err() {
            self.pending.lock().unwrap().remove(&request_id);
            return None;
        }
        let answer = tokio::time::timeout(timeout, receiver).await;
        self.pending.lock().unwrap().remove(&request_id);
        answer.ok()?.ok()
    }

    /// The owner's client ID, or None when nobody answers. `None` alone never proves a
    /// thread is free: an unanswered or hung owner looks the same, so callers check the lock.
    pub async fn discover_owner(&self, thread: &str) -> Option<String> {
        let reply = self
            .exchange(
                json!({"type":"request","method":"thread-owner-discovery","version":version_of("thread-owner-discovery"),"params":{"hostId":LOCAL_HOST,"conversationId":thread}}),
                Duration::from_secs(12),
            )
            .await?;
        (reply["resultType"] == "success").then(|| reply["handledByClientId"].as_str().map(str::to_owned)).flatten()
    }

    pub async fn owner_request(&self, owner: &str, method: &str, params: Value, timeout: Duration) -> Reply {
        let reply = self.exchange(json!({"type":"request","method":method,"version":version_of(method),"targetClientId":owner,"params":params}), timeout).await;
        let Some(reply) = reply else { return Reply::Uncertain };
        if reply["resultType"] == "success" {
            return Reply::Success(reply["result"].clone());
        }
        let error = reply["error"].as_str().unwrap_or("IPC_ERROR").to_owned();
        // These can fire after the owner already acted.
        if matches!(error.as_str(), "request-timeout" | "client-disconnected" | "server-closed") {
            return Reply::Uncertain;
        }
        Reply::Rejected(error)
    }

    pub fn set_following(&self, owner: &str, thread: &str, following: bool) -> bool {
        self.writer
            .send(encode(&json!({
                "type":"broadcast","method":"thread-stream-following-changed","sourceClientId":self.client_id,
                "targetClientIds":[owner],"version":version_of("thread-stream-following-changed"),
                "params":{"conversationId":thread,"hostId":LOCAL_HOST,"following":following}
            })).into())
            .is_ok()
    }

    pub fn subscribe(&self, thread: &str, owner:&str, events: mpsc::UnboundedSender<StateEvent>) {
        self.subscribers.lock().unwrap().insert(thread.to_owned(),Subscriber{events,owner:owner.into(),mirror:Default::default()});
    }

    pub fn unsubscribe(&self, thread: &str) {
        self.subscribers.lock().unwrap().remove(thread);
    }
}

async fn write_loop(write:&mut (impl AsyncWriteExt+Unpin),mut outgoing:mpsc::UnboundedReceiver<Outgoing>){
    while let Some(frame)=outgoing.recv().await{
        if write.write_all(&frame.bytes).await.is_err(){break;}
        if let Some(mut state)=frame.written{state.release();if let Some(done)=state.done.take(){let _=done.send(());}}
    }
}

async fn read_loop(
    read: &mut (impl AsyncReadExt + Unpin),
    writer: &mpsc::UnboundedSender<Outgoing>,
    pending: &Mutex<HashMap<String, oneshot::Sender<Value>>>,
    subscribers: &Subscribers,
    claim: Option<&Claim>,
) -> std::io::Result<()> {
    loop {
        let length = read.read_u32_le().await? as usize;
        if length == 0 || length > PROTOCOL_FRAME_LIMIT {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        if length > SNAPSHOT_FRAME_LIMIT {
            // Skip without buffering; a huge state must not take this process with it.
            tokio::io::copy(&mut read.take(length as u64), &mut tokio::io::sink()).await?;
            for subscriber in subscribers.lock().unwrap().values() {
                let _ = subscriber.events.send(StateEvent::TooLarge);
            }
            continue;
        }
        let mut body = vec![0_u8; length];
        read.read_exact(&mut body).await?;
        if route_state_frame(&body,subscribers){continue;}
        let Ok(message) = serde_json::from_slice::<Value>(&body) else { continue };
        drop(body);
        if message["type"]=="broadcast"&&message["method"]=="ipc-connection-reset"{
            return Err(std::io::ErrorKind::ConnectionReset.into());
        }
        match message["type"].as_str() {
            Some("response") => {
                let sender = message["requestId"].as_str().and_then(|id| pending.lock().unwrap().remove(id));
                if let Some(sender) = sender {
                    let _ = sender.send(message);
                }
            }
            Some("client-discovery-request") => {
                // Silence would stall the router for ten seconds.
                let can_handle = claim.is_some_and(|claim| claim.handles(&message["request"]));
                let _ = writer.send(encode(&json!({"type":"client-discovery-response","requestId":message["requestId"],"response":{"canHandle":can_handle}})).into());
            }
            Some("request") => {
                if let Some(claim) = claim.filter(|claim| claim.handles(&message)) {
                    let _ = claim.events.send(OwnerEvent::Request(message));
                }
            }
            Some("broadcast") => {
                if let Some(claim) = claim {
                    dispatch_to_owner(&message, claim);
                }
                dispatch_broadcast(message, subscribers);
            }
            _ => {}
        }
    }
}

fn dispatch_to_owner(message: &Value, claim: &Claim) {
    match message["method"].as_str() {
        Some("thread-stream-following-changed") if message.pointer("/params/conversationId").and_then(Value::as_str) == Some(claim.thread.as_str()) => {
            if let (Some(client), Some(following)) = (message["sourceClientId"].as_str(), message.pointer("/params/following").and_then(Value::as_bool)) {
                let _ = claim.events.send(OwnerEvent::Following { client: client.to_owned(), following });
            }
        }
        Some("client-status-changed") if message.pointer("/params/status").and_then(Value::as_str) != Some("connected") => {
            if let Some(client) = message.pointer("/params/clientId").and_then(Value::as_str) {
                let _ = claim.events.send(OwnerEvent::ClientGone(client.to_owned()));
            }
        }
        _ => {}
    }
}

/// Deserialize only observed fields. In particular no turn/image/tool history
/// is materialized or retained merely to observe status and native decisions.
fn route_state_frame(body:&[u8],subscribers:&Subscribers)->bool{
    use super::follower_state::{Envelope,Packet,Update};
    let Ok(frame)=serde_json::from_slice::<Envelope<'_>>(body)else{return false};
    if frame.kind!="broadcast"||frame.method!=Some("thread-stream-state-changed"){return false;}
    let Some(raw)=frame.params else{return true};let Ok(packet)=serde_json::from_str::<Packet<'_>>(raw.get())else{return true};
    if packet.host.is_some_and(|host|host!=LOCAL_HOST){return true;}
    let mut subscriptions=subscribers.lock().unwrap();let Some(subscriber)=subscriptions.get_mut(packet.thread)else{return true};
    if frame.source!=Some(subscriber.owner.as_str()){return true;}
    let event=match subscriber.mirror.ingest(packet.change){Update::State(state)=>StateEvent::Snapshot(state),Update::Resync=>StateEvent::Resync,Update::Quiet=>return true};
    let _=subscriber.events.send(event);true
}

fn dispatch_broadcast(message: Value, subscribers: &Subscribers) {
    if message["method"]=="client-status-changed"&&message.pointer("/params/status").and_then(Value::as_str)!=Some("connected"){
        if let Some(client)=message.pointer("/params/clientId").and_then(Value::as_str){
            for sub in subscribers.lock().unwrap().values().filter(|sub|sub.owner==client){let _=sub.events.send(StateEvent::Disconnected);}
        }
        return;
    }
    if message["method"]!="thread-stream-following-status-requested"{return;}
    let Some(thread)=message.pointer("/params/conversationId").and_then(Value::as_str)else{return};
    let subscriptions=subscribers.lock().unwrap();let Some(subscriber)=subscriptions.get(thread)else{return};
    if message["sourceClientId"].as_str()!=Some(subscriber.owner.as_str()){return;}
    let _=subscriber.events.send(StateEvent::Resync);
}

#[cfg(test)]mod backpressure_tests{
 use super::*;
 async fn read_frame(server:&mut tokio::io::DuplexStream)->Value{let len=server.read_u32_le().await.unwrap();let mut bytes=vec![0;len as usize];server.read_exact(&mut bytes).await.unwrap();serde_json::from_slice(&bytes).unwrap()}
 #[tokio::test]async fn stalled_state_write_keeps_one_frame_and_does_not_close_the_control_connection(){
  // A bounded duplex gives deterministic backpressure; Windows pipe buffering
  // may accept a whole frame before its application reads it.
  let(mut client,mut server)=tokio::io::duplex(1024);let(writer,outgoing)=mpsc::unbounded_channel();
  let writing=tokio::spawn(async move{write_loop(&mut client,outgoing).await;});
  let c=Arc::new(Connection{writer,pending:Arc::default(),subscribers:Arc::default(),client_id:"fixture".into(),closed:Arc::new(AtomicBool::new(false)),tasks:Mutex::new(vec![writing.abort_handle()]),writes_allowed:true,owner_allowed:true,state_pending:Arc::new(AtomicBool::new(false))});
  let(release,wait)=oneshot::channel();let(count,received)=oneshot::channel();
  let peer=tokio::spawn(async move{wait.await.unwrap();let a=read_frame(&mut server).await;assert_eq!(a["method"],"thread-stream-state-changed");let _=count.send(());let b=read_frame(&mut server).await;assert_eq!(b["params"]["value"],"after");});
  let copy=c.clone();let sending=tokio::spawn(async move{copy.broadcast_state(vec!["reader".into()],json!({"value":"x".repeat(4*1024*1024)})).await});
  tokio::time::timeout(Duration::from_secs(5),async{while !c.state_write_busy(){assert!(!sending.is_finished());tokio::time::sleep(Duration::from_millis(1)).await;}}).await.unwrap();
  assert_eq!(c.broadcast_state(vec!["reader".into()],json!({"value":"must-not-queue"})).await,0);
  assert_eq!(sending.await.unwrap(),0);assert!(c.state_write_busy());assert!(!c.is_closed(),"a slow view must not cancel provider/control requests");
  release.send(()).unwrap();received.await.unwrap();tokio::time::timeout(Duration::from_secs(2),async{while c.state_write_busy(){tokio::task::yield_now().await;}}).await.unwrap();
  assert!(c.broadcast_state(vec!["reader".into()],json!({"value":"after"})).await>0);peer.await.unwrap();c.close();
 }
 #[test]fn releasing_an_old_write_cannot_clear_a_new_writes_slot(){
  let pending=Arc::new(AtomicBool::new(true));let mut old=StateWrite{done:None,pending:pending.clone(),released:false};old.release();pending.store(true,Ordering::SeqCst);drop(old);assert!(pending.load(Ordering::SeqCst));
 }
}

#[cfg(test)]mod projection_tests{
 use super::*;
 #[test]fn only_the_discovered_owner_can_update_the_metadata_projection(){
  let subscribers:Subscribers=Arc::default();let(tx,mut rx)=mpsc::unbounded_channel();subscribers.lock().unwrap().insert("t".into(),Subscriber{events:tx,owner:"owner".into(),mirror:Default::default()});
  let mut frame=json!({"type":"broadcast","method":"thread-stream-state-changed","sourceClientId":"foreign","params":{"conversationId":"t","hostId":"local","change":{"type":"snapshot","revision":1,"conversationState":{"threadRuntimeStatus":{"type":"active"},"requests":[],"history":"x".repeat(1024*1024)}}}});
  assert!(route_state_frame(frame.to_string().as_bytes(),&subscribers));assert!(rx.try_recv().is_err());
  frame["sourceClientId"]=json!("owner");assert!(route_state_frame(frame.to_string().as_bytes(),&subscribers));let StateEvent::Snapshot(state)=rx.try_recv().unwrap()else{panic!()};assert!(state.to_string().len()<200);assert!(state.get("history").is_none());
  dispatch_broadcast(json!({"method":"client-status-changed","params":{"status":"disconnected","clientId":"owner"}}),&subscribers);assert!(matches!(rx.try_recv(),Ok(StateEvent::Disconnected)));
 }
}
