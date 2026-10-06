use super::preparation::PreparationGuard;
use crate::codex::protocol::{map_notification, FeedEvent};
use crate::events::EventSink;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
fn project_public_chat_turn(turn:&str,head:&[Value],tail:&[Value])->Result<Vec<Value>,String>{
    if head.len()>1||tail.len()>20{return Err("CHAT_HISTORY_INVALID".into());}
    let mut messages=Vec::new();let mut seen=HashSet::new();
    for row in head.iter().chain(tail.iter().rev()) {
        if row["turnId"].as_str()!=Some(turn){return Err("CHAT_HISTORY_IDENTITY_MISMATCH".into());}
        let item=&row["item"];
        let id=item["id"].as_str().filter(|id|!id.is_empty()&&id.len()<=256).ok_or("CHAT_HISTORY_INVALID")?;
        if !seen.insert(id){continue;}
        let text=match item["type"].as_str(){
            Some("agentMessage") if matches!(item["phase"].as_str(),Some("commentary"|"final_answer"))=>item["text"].as_str().map(str::to_owned),
            Some("userMessage")=>item["content"].as_array().map(|items|items.iter().filter(|c|c["type"]=="text").filter_map(|c|c["text"].as_str()).collect::<Vec<_>>().join("\n")),
            _=>None,
        };
        if let Some(text)=text.filter(|text|!text.trim().is_empty()&&text.len()<=1_000_000){
            messages.push(json!({"id":id,"turnId":turn,"role":if item["type"]=="userMessage"{"user"}else{"assistant"},"text":text}));
        }
    }
    Ok(messages)
}

#[cfg(test)]
mod public_chat_projection_tests {
    use super::*;
    #[test]
    fn long_turn_keeps_real_prompt_before_tail_and_excludes_private_items(){
        let prompt=json!({"turnId":"exact","item":{"id":"u","type":"userMessage","content":[{"type":"text","text":"original prompt"}]}});
        let answer=json!({"turnId":"exact","item":{"id":"a","type":"agentMessage","phase":"final_answer","text":"result"}});
        let private=json!({"turnId":"exact","item":{"id":"private","type":"reasoning","text":"must never publish"}});
        let rows=project_public_chat_turn("exact",&[prompt.clone()],&[answer.clone(),private]).unwrap();
        assert_eq!(rows.len(),2);assert_eq!(rows[0]["id"],"u");assert_eq!(rows[1]["id"],"a");
        assert_eq!(project_public_chat_turn("exact",&[prompt.clone()],&[answer,prompt]).unwrap().len(),2);
    }
    #[test]
    fn rejects_other_turns_and_does_not_invent_user_messages(){
        assert!(project_public_chat_turn("exact",&[json!({"turnId":"other"})],&[]).is_err());
        let tool=json!({"turnId":"exact","item":{"id":"tool","type":"commandExecution","text":"tool output"}});
        assert!(project_public_chat_turn("exact",&[tool],&[]).unwrap().is_empty());
    }
}
#[derive(Default)]
struct CreationObservation {
    request_id: AtomicU64,
    candidate: Mutex<Option<String>>,
}
impl CreationObservation {
    fn candidate(&self) -> Option<String> {
        self.candidate.lock().ok().and_then(|id| id.clone())
    }
    fn observe(&self, response: &Value) {
        let request = self.request_id.load(Ordering::SeqCst);
        if request == 0 || response.get("id").and_then(Value::as_u64) != Some(request) {
            return;
        }
        if let Some(id) = response
            .pointer("/result/thread/id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
        {
            if let Ok(mut candidate) = self.candidate.lock() {
                if candidate.is_none() {
                    *candidate = Some(id.into());
                }
            }
        }
    }
}
#[derive(Default, Debug)]
pub struct CandidateCleanup {
    pub clear_attempted: bool,
    pub clear_acknowledged: bool,
    pub interrupt_attempted: bool,
    pub interrupt_acknowledged: bool,
}
/// Current native execution is separate from a persisted completed result.
#[derive(Clone,Debug)]
pub struct NativeThreadActivity{pub thread_id:String,pub cwd:String,pub state:&'static str,pub turn_id:Option<String>,pub authoritative:bool}
fn activity_phase(live:bool,status:Option<&str>)->&'static str{
    if !live{return "UNCONFIRMED";}
    match status{Some("inProgress"|"active"|"pending")=>"RUNNING",Some("completed")=>"COMPLETE",Some("interrupted")=>"INTERRUPTED",Some("failed")=>"FAILED",None=>"EMPTY",_=>"UNCONFIRMED"}
}
fn activity_goal_phase(thread_id:&str,response:&Value)->Result<Option<&'static str>,String>{
    let goal=response.get("goal").ok_or("ACTIVITY_GOAL_PROTOCOL_INVALID")?;
    if goal.is_null(){return Ok(None);}
    if goal["threadId"].as_str()!=Some(thread_id){return Err("ACTIVITY_GOAL_IDENTITY_MISMATCH".into());}
    Ok(match goal["status"].as_str(){Some("active")=>Some("RUNNING"),Some("blocked")=>Some("ACTION_REQUIRED"),Some("paused")=>Some("PAUSED"),Some("complete"|"completed")=>None,_=>Some("UNCONFIRMED")})
}
/// A write-acquisition probe must fail promptly.  It precedes every first
/// approved write, so retaining the general turn/result timeout here would
/// leave the mobile Handoff review looking actionable while no Handoff exists.
pub const WRITE_READINESS_TIMEOUT: Duration = Duration::from_secs(10);

/// Causally matched response, constructed only by the owned adapter decoder.
#[derive(Debug)]
pub struct NativeTurnAck {
    pub(crate) adapter_epoch: String,
    pub(crate) request_id: Value,
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
}
impl NativeTurnAck {
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }
}

/// No stdin remains, child exited and observation drained, no turn write was
/// reserved or observed. Only the guarded adapter can produce this proof.
pub struct SealedPrewrite {
    pub(crate) adapter_epoch: String,
}

/// Allows the one exact metadata read that current official app-server needs
/// before it can load an existing Desktop thread and expose that thread's Goal.
/// The caller promotes the read to `verified_metadata_reads` only after its
/// response proves the returned identity.
fn authorize_guarded_metadata_step(
    method: &str,
    params: &Value,
    observed_safe_goals: &mut HashSet<String>,
    initial_metadata_reads: &mut HashSet<String>,
    verified_metadata_reads: &mut HashSet<String>,
) -> Result<(), &'static str> {
    let thread = params
        .get("threadId")
        .and_then(Value::as_str)
        .ok_or("CAPABILITY_UNVERIFIED")?;
    if (method == "thread/read" && params.get("includeTurns") != Some(&Value::Bool(false)))
        || (method == "thread/resume" && params.get("excludeTurns") != Some(&Value::Bool(true)))
    {
        return Err("NORMAL_HISTORY_RPC_FORBIDDEN");
    }
    if method == "thread/read" {
        // Official app-server stores Desktop threads as `notLoaded`; their
        // Goal is unavailable until a resume. Permit exactly one
        // metadata-only read before that resume.
        if !observed_safe_goals.remove(thread) && !initial_metadata_reads.insert(thread.into()) {
            return Err("FRESH_SAFE_GOAL_OBSERVATION_REQUIRED");
        }
    } else if !observed_safe_goals.remove(thread) && !verified_metadata_reads.remove(thread) {
        return Err("FRESH_SAFE_GOAL_OBSERVATION_REQUIRED");
    }
    Ok(())
}

fn exact_metadata_response(response: &Value, expected_thread: &str) -> bool {
    response.pointer("/thread/id").and_then(Value::as_str) == Some(expected_thread)
}

/// The official app-server may answer the first metadata-only `thread/read`
/// for a just-created Desktop thread with this exact JSON-RPC error.  It is
/// neither a history response nor proof of a different thread, but it does
/// permit the already-authorized one-shot metadata-only resume for the same
/// sealed id.  Keep this deliberately narrower than the application-layer
/// fallback: it recognizes the serialized RPC message field, not arbitrary
/// text containing a thread id.
fn exact_not_loaded_metadata_error(error: &str, expected_thread: &str) -> bool {
    let message = format!("thread not loaded: {expected_thread}");
    error.contains(&format!("\"message\":\"{message}\""))
}

#[derive(Default)]
struct TurnRegistry {
    observed:HashMap<String,String>,owned:HashMap<String,String>,
    terminal:HashMap<(String,String),String>,completed:std::collections::VecDeque<(String,String)>,
}
impl TurnRegistry {
    fn acknowledge(&mut self,thread:&str,turn:&str){if self.terminal.contains_key(&(thread.into(),turn.into())){return;}self.owned.insert(thread.into(),turn.into());self.observed.insert(thread.into(),turn.into());}
    fn finish(&mut self,thread:&str,turn:&str,status:&str){
        let key=(thread.to_string(),turn.to_string());if !self.terminal.contains_key(&key){self.completed.push_back(key.clone());}self.terminal.insert(key,status.into());
        while self.completed.len()>1024{if let Some(key)=self.completed.pop_front(){self.terminal.remove(&key);}}
        if self.observed.get(thread).is_some_and(|v|v==turn){self.observed.remove(thread);}
        if self.owned.get(thread).is_some_and(|v|v==turn){self.owned.remove(thread);}
    }
}
pub struct CodexAdapter {
    epoch: String,
    child: Child,
    guard: Option<PreparationGuard>,
    stdout_done: mpsc::Receiver<()>,
    closing: Arc<AtomicBool>,
    drained: bool,
    creation: Arc<CreationObservation>,
    cleanup_interrupt_attempted: bool,
    initial_goal_attempted: bool,
    candidate_start_attempted: bool,
    goal_clear_attempted: bool,
    observed_safe_goals: HashSet<String>,
    initial_metadata_reads: HashSet<String>,
    verified_metadata_reads: HashSet<String>,
    history_recovery: Option<ExactHistoryRecovery>,
    latest_turn_observation: Option<LatestTurnObservation>,
    public_chat_read: Option<(String, u8)>,
    exact_turn_status_read: Option<(String, u8)>,
    stdin: Option<ChildStdin>,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>>,
    closed: Arc<AtomicBool>,
    /// A turn is controllable only while this Router-owned app-server process
    /// has observed its exact thread and turn identity.  A thread-level busy
    /// flag is insufficient for `turn/interrupt`: it could otherwise stop a
    /// later turn on the same thread.
    turns:Arc<Mutex<TurnRegistry>>,
    shared_scope:Option<Arc<Mutex<HashSet<String>>>>,
    #[cfg(debug_assertions)]
    request_counts: HashMap<String, u64>,
}

#[derive(Debug)]
struct ExactHistoryRecovery {
    thread_id: String,
    turn_id: String,
    turn_pages: u8,
    item_pages: u8,
}

/// A separate, narrower capability for the Host's passive existing-thread
/// observer. It may inspect exactly one newest turn and one page of that
/// turn's items; it cannot be reused as a general history reader.
#[derive(Debug)]
struct LatestTurnObservation {
    thread_id: String,
    turn_listed: bool,
    fallback_turn_id: Option<String>,
    fallback_item_listed: bool,
}

impl CodexAdapter {
    /// Presentation-only exact Desktop project membership. No provider writer.
    pub fn read_desktop_catalog(&self)->Result<super::desktop_catalog::DesktopCatalog,String>{
        if self.guard.is_some(){return Err("FORBIDDEN".into());}
        super::desktop_catalog::current()
    }
    /// Exceptional control only for this child's causally acknowledged candidate.
    /// Neither ACK here is proof that execution stopped. The safety latch stays set.
    pub fn cleanup_owned_candidate(&mut self) -> CandidateCleanup {
        let mut result = CandidateCleanup::default();
        if self.guard.is_none() {
            return result;
        }
        let Some(candidate) = self.creation.candidate() else {
            return result;
        };
        if self.initial_goal_attempted && !self.goal_clear_attempted {
            result.clear_attempted = true;
            self.goal_clear_attempted = true;
            result.clear_acknowledged = self
                .request_inner(
                    "thread/goal/clear",
                    goal_clear_params(&candidate),
                    Duration::from_secs(10),
                    true,
                    None,
                )
                .ok()
                .and_then(|r| r.get("cleared").and_then(Value::as_bool))
                .unwrap_or(false);
        }
        let observed = self
            .guard
            .as_ref()
            .and_then(PreparationGuard::observed_turn);
        if let Some((thread, turn)) = observed {
            if thread == candidate && !self.cleanup_interrupt_attempted {
                self.cleanup_interrupt_attempted = true;
                result.interrupt_attempted = true;
                result.interrupt_acknowledged = self
                    .request_inner(
                        "turn/interrupt",
                        turn_interrupt_params(&thread, &turn),
                        Duration::from_secs(10),
                        true,
                        None,
                    )
                    .is_ok();
            }
        }
        result
    }
    pub fn start(
        sink: Arc<dyn EventSink>,
        event_listener: Arc<dyn Fn(&Value) + Send + Sync>,
        command: Command,
    ) -> Result<Self, String> {
        Self::start_inner(
            sink,
            event_listener,
            command,
            None,
            uuid::Uuid::new_v4().to_string(),
        )
    }

    /// Owns only the stdio/WebSocket proxy child, never the shared backend.
    pub fn start_shared_transport(sink:Arc<dyn EventSink>,listener:Arc<dyn Fn(&Value)+Send+Sync>,command:Command)->Result<Self,String>{
        Self::start_transport_inner(sink,listener,command,None,uuid::Uuid::new_v4().to_string(),&[],true)
    }
    pub fn is_shared(&self)->bool{self.shared_scope.is_some()}
    pub fn has_owned_turns(&self)->bool{self.turns.lock().map(|r|!r.owned.is_empty()).unwrap_or(true)}
    pub fn start_guarded(
        sink: Arc<dyn EventSink>,
        event_listener: Arc<dyn Fn(&Value) + Send + Sync>,
        command: Command,
        guard: PreparationGuard,
    ) -> Result<Self, String> {
        guard.check()?;
        Self::start_inner(
            sink,
            event_listener,
            command,
            Some(guard),
            uuid::Uuid::new_v4().to_string(),
        )
    }

    pub fn epoch(&self) -> &str {
        &self.epoch
    }
    pub fn start_guarded_at_epoch(
        sink: Arc<dyn EventSink>,
        event_listener: Arc<dyn Fn(&Value) + Send + Sync>,
        command: Command,
        guard: PreparationGuard,
        epoch: String,
    ) -> Result<Self, String> {
        uuid::Uuid::parse_str(&epoch).map_err(|_| "INVALID_ADAPTER_EPOCH")?;
        guard.check()?;
        Self::start_inner(sink, event_listener, command, Some(guard), epoch)
    }
    pub fn start_guarded_with_args_at_epoch(
        sink: Arc<dyn EventSink>,
        event_listener: Arc<dyn Fn(&Value) + Send + Sync>,
        command: Command,
        guard: PreparationGuard,
        epoch: String,
        app_server_args: &[String],
    ) -> Result<Self, String> {
        uuid::Uuid::parse_str(&epoch).map_err(|_| "INVALID_ADAPTER_EPOCH")?;
        guard.check()?;
        Self::start_inner_with_args(
            sink,
            event_listener,
            command,
            Some(guard),
            epoch,
            app_server_args,
        )
    }
    fn start_inner(
        sink: Arc<dyn EventSink>,
        event_listener: Arc<dyn Fn(&Value) + Send + Sync>,
        command: Command,
        guard: Option<PreparationGuard>,
        epoch: String,
    ) -> Result<Self, String> {
        Self::start_inner_with_args(sink, event_listener, command, guard, epoch, &[])
    }
    fn start_inner_with_args(
        sink: Arc<dyn EventSink>,
        event_listener: Arc<dyn Fn(&Value) + Send + Sync>,
        command: Command,
        guard: Option<PreparationGuard>,
        epoch: String,
        app_server_args: &[String],
    ) -> Result<Self, String> {
        Self::start_transport_inner(sink,event_listener,command,guard,epoch,app_server_args,false)
    }
    fn start_transport_inner(sink:Arc<dyn EventSink>,event_listener:Arc<dyn Fn(&Value)+Send+Sync>,mut command:Command,guard:Option<PreparationGuard>,epoch:String,app_server_args:&[String],shared:bool)->Result<Self,String>{
        if !shared {command.args(["app-server","--stdio"]).args(app_server_args);}
        let mut child = command
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().map_err(|error| format!("Could not start Codex app-server. Ensure the current Codex executable is available: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or("Codex app-server did not expose stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Codex app-server did not expose stdout")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("Codex app-server did not expose stderr")?;
        let pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let turns=Arc::new(Mutex::new(TurnRegistry::default()));
        let shared_scope=shared.then(||Arc::new(Mutex::new(HashSet::new())));
        let creation = Arc::new(CreationObservation::default());
        let closing = Arc::new(AtomicBool::new(false));
        // App-server diagnostics travel on stderr.  Retain only a short tail
        // so a closed stdout can explain the actual local startup failure
        // without turning stderr into a history or telemetry channel.
        let stderr_detail = Arc::new(Mutex::new(None));
        let (done_sender, stdout_done) = mpsc::channel();
        spawn_stdout_reader(
            stdout,
            pending.clone(),
            closed.clone(),
            turns.clone(),
            sink.clone(),
            event_listener,
            guard.clone(),
            done_sender,
            closing.clone(),
            creation.clone(),
            stderr_detail.clone(),
            shared_scope.clone(),
        );
        spawn_stderr_reader(stderr, stderr_detail);
        Ok(Self {
            epoch,
            child,
            guard,
            stdout_done,
            closing,
            drained: false,
            creation,
            cleanup_interrupt_attempted: false,
            initial_goal_attempted: false,
            candidate_start_attempted: false,
            goal_clear_attempted: false,
            observed_safe_goals: HashSet::new(),
            initial_metadata_reads: HashSet::new(),
            verified_metadata_reads: HashSet::new(),
            history_recovery: None,
            latest_turn_observation: None,
            public_chat_read: None,
            exact_turn_status_read: None,
            stdin: Some(stdin),
            next_id: AtomicU64::new(1),
            pending,
            closed,
            turns,shared_scope,
            #[cfg(debug_assertions)]
            request_counts: HashMap::new(),
        })
    }

    pub fn initialize(&mut self) -> Result<(), String> {
        self.request("initialize", json!({
            "clientInfo": { "name": "ai_work_router", "title": "AI Work Router", "version": "0.1.0" },
            "capabilities": {
                "experimentalApi": true,
                "requestAttestation": false
            }
        }))?;
        self.notify("initialized", Value::Null)
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn is_turn_active(&self,thread:&str)->bool{self.turns.lock().map(|r|r.observed.contains_key(thread)).unwrap_or(true)}
    pub fn is_exact_turn_active(&self,thread:&str,turn:&str)->bool{self.turns.lock().map(|r|r.owned.get(thread).is_some_and(|v|v==turn)&&r.observed.get(thread).is_some_and(|v|v==turn)).unwrap_or(false)}
    pub fn active_turn_id(&self,thread:&str)->Option<String>{self.turns.lock().ok().and_then(|r|r.owned.get(thread).filter(|id|r.observed.get(thread)==Some(*id)).cloned())}
    pub fn mark_turn_started(&self,thread:&str,turn:&str){if let Ok(mut r)=self.turns.lock(){r.acknowledge(thread,turn);}}
    pub fn terminal_turn_status(&self,thread:&str,turn:&str)->Option<String>{self.turns.lock().ok().and_then(|r|r.terminal.get(&(thread.into(),turn.into())).cloned())}

    /// Reconcile an acknowledged turn after observer reconnect. At most four
    /// metadata pages (16 turns), no items/tool output, resume, write or retry.
    pub fn read_exact_turn_status(&mut self, thread:&str, turn:&str)->Result<Option<String>,String>{
        if self.guard.is_some() || [thread,turn].iter().any(|s|s.is_empty()||s.len()>256){return Err("EXACT_TURN_STATUS_IDENTITY_INVALID".into());}
        let meta=self.request_with_timeout("thread/read",json!({"threadId":thread,"includeTurns":false}),Duration::from_secs(3))?;
        if meta.pointer("/thread/id").and_then(Value::as_str)!=Some(thread){return Err("EXACT_TURN_STATUS_THREAD_MISMATCH".into());}
        if let Some(status)=self.terminal_turn_status(thread,turn){return Ok(Some(status));}
        self.exact_turn_status_read=Some((thread.into(),4));
        let result=(||{
            let mut cursor=Value::Null;
            for _ in 0..4 {
                let page=self.request_with_timeout("thread/turns/list",json!({"threadId":thread,"cursor":cursor,"limit":4,"sortDirection":"desc","itemsView":"notLoaded"}),Duration::from_secs(3))?;
                let rows=page["data"].as_array().filter(|r|r.len()<=4).ok_or("EXACT_TURN_STATUS_PROTOCOL_INVALID")?;
                if let Some(row)=rows.iter().find(|row|row["id"].as_str()==Some(turn)){
                    return Ok(row["status"].as_str().map(str::to_owned));
                }
                let Some(next)=page["nextCursor"].as_str().filter(|s|!s.is_empty()&&s.len()<=512)else{return Ok(None)};
                if cursor.as_str()==Some(next){return Err("EXACT_TURN_STATUS_CURSOR_REPEATED".into());}
                cursor=Value::String(next.into());
            }
            Ok(None)
        })();
        self.exact_turn_status_read=None;
        result
    }

    /// Exact bounded metadata/status observation. Never resumes or sends.
    pub fn read_thread_activity(&mut self,thread_id:&str)->Result<NativeThreadActivity,String>{
        let budget=Duration::from_secs(3);
        let metadata=self.request_with_timeout("thread/read",json!({"threadId":thread_id,"includeTurns":false}),budget)?;
        let thread=&metadata["thread"];if thread["id"].as_str()!=Some(thread_id){return Err("ACTIVITY_IDENTITY_MISMATCH".into());}
        let cwd=thread["cwd"].as_str().filter(|p|std::path::Path::new(p).is_absolute()).ok_or("ACTIVITY_PROJECT_UNAVAILABLE")?.to_string();
        let kind=thread.pointer("/status/type").and_then(Value::as_str);
        if kind==Some("active"){return Ok(NativeThreadActivity{thread_id:thread_id.into(),cwd,state:"RUNNING",turn_id:self.turns.lock().ok().and_then(|r|r.observed.get(thread_id).cloned()),authoritative:true});}
        self.begin_latest_turn_observation(thread_id)?;
        let turns=match self.request_with_timeout("thread/turns/list",json!({"threadId":thread_id,"cursor":null,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"}),budget){Ok(v)=>v,Err(e) if e.strip_prefix("Codex JSON-RPC error: ").and_then(|v|serde_json::from_str::<Value>(v).ok()).is_some_and(|v|v["code"]==-32600&&v["message"].as_str()==Some(format!("thread {thread_id} is not materialized yet; thread/turns/list is unavailable before first user message").as_str()))=>json!({"data":[]}),Err(e)=>return Err(e)};
        let rows=turns["data"].as_array().filter(|v|v.len()<=1).ok_or("ACTIVITY_PROTOCOL_INVALID")?;
        let turn_id=rows.first().map(|t|t["id"].as_str().filter(|id|!id.is_empty()&&id.len()<=256).map(str::to_owned).ok_or("ACTIVITY_PROTOCOL_INVALID")).transpose()?;
        let live=kind==Some("idle")&&(self.is_shared()||turn_id.as_deref().is_some_and(|t|self.terminal_turn_status(thread_id,t).is_some()));
        if live{
            let response=self.request_with_timeout("thread/goal/get",goal_get_params(thread_id),budget)?;
            if let Some(state)=activity_goal_phase(thread_id,&response)?{return Ok(NativeThreadActivity{thread_id:thread_id.into(),cwd,state,turn_id,authoritative:state!="UNCONFIRMED"});}
        }
        let state=activity_phase(live,rows.first().and_then(|t|t["status"].as_str()));
        Ok(NativeThreadActivity{thread_id:thread_id.into(),cwd,state,turn_id,authoritative:live})
    }

    pub fn get_goal(&mut self, thread_id: &str) -> Result<Value, String> {
        self.request("thread/goal/get", goal_get_params(thread_id))
    }

    /// Deliberately sends only the new Goal status.  In particular it does
    /// not repeat `objective`, token budget, or usage fields, so app-server
    /// preserves the authoritative Goal objective and accounting.
    pub fn set_goal_status(&mut self, thread_id: &str, status: &str) -> Result<Value, String> {
        self.request("thread/goal/set", goal_set_status_params(thread_id, status))
    }

    pub fn clear_goal(&mut self, thread_id: &str) -> Result<Value, String> {
        self.request("thread/goal/clear", goal_clear_params(thread_id))
    }

    pub fn interrupt_turn(&mut self, thread_id: &str, turn_id: &str) -> Result<Value, String> {
        self.request("turn/interrupt", turn_interrupt_params(thread_id, turn_id))
    }

    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.request_with_timeout(method, params, REQUEST_TIMEOUT)
    }

    /// Enables only the bounded historical result read used by recovery. The
    /// permit is process-local and cannot be used by ordinary request paths.
    pub fn begin_exact_history_recovery(
        &mut self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<(), String> {
        if self.guard.is_none()
            || thread_id.is_empty()
            || turn_id.is_empty()
            || thread_id.len() > 256
            || turn_id.len() > 256
            || self.history_recovery.is_some()
        {
            return Err("HISTORY_RECOVERY_UNSUPPORTED".into());
        }
        self.history_recovery = Some(ExactHistoryRecovery {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            turn_pages: 0,
            item_pages: 0,
        });
        Ok(())
    }

    /// Opens one process-local permit for one passive latest-turn observation.
    /// Returned identities remain caller-validated; this only makes broad
    /// transcript requests structurally unavailable to normal Router paths.
    pub fn begin_latest_turn_observation(&mut self, thread_id: &str) -> Result<(), String> {
        if thread_id.is_empty() || thread_id.len() > 256 || self.history_recovery.is_some() {
            return Err("LATEST_TURN_OBSERVATION_UNSUPPORTED".into());
        }
        self.latest_turn_observation = Some(LatestTurnObservation {
            thread_id: thread_id.into(),
            turn_listed: false,
            fallback_turn_id: None,
            fallback_item_listed: false,
        });
        Ok(())
    }

    /// Allows the one bounded newest-first item projection only after the
    /// caller has observed the exact latest turn metadata. A nonterminal page
    /// may supply public progress to the Host; it never proves completion. Keeping
    /// turn metadata `notLoaded` avoids receiving an unbounded transcript
    /// inside the latest-turn response.
    pub fn begin_latest_turn_item_fallback(
        &mut self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<(), String> {
        let valid = |value: &str| !value.is_empty() && value.len() <= 256;
        let permit = self
            .latest_turn_observation
            .as_mut()
            .ok_or("NORMAL_HISTORY_RPC_FORBIDDEN")?;
        if !valid(thread_id)
            || !valid(turn_id)
            || permit.thread_id != thread_id
            || !permit.turn_listed
            || permit.fallback_turn_id.is_some()
        {
            return Err("NORMAL_HISTORY_RPC_FORBIDDEN".into());
        }
        permit.fallback_turn_id = Some(turn_id.into());
        Ok(())
    }

    pub fn request_with_timeout(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        if let Some(scope)=&self.shared_scope{if let Some(id)=params.get("threadId").and_then(Value::as_str).filter(|s|!s.is_empty()&&s.len()<=256){scope.lock().map_err(|_|"SHARED_SCOPE_UNAVAILABLE")?.insert(id.into());}}
        self.request_inner(method, params, timeout, false, None)
    }

    /// Explicit conversation-reader action, bounded to four turns (tail20 +
    /// first item). Never used
    /// by passive observers; expose only public messages in the Host projection.
    pub fn read_public_chat_page(&mut self, thread: &str, cursor: Option<&str>) -> Result<Value, String> {
        if self.guard.is_some() || thread.is_empty() || thread.len()>256 || cursor.is_some_and(|c|c.len()>2048) {
            return Err("PUBLIC_CHAT_READ_UNAVAILABLE".into());
        }
        self.public_chat_read=Some((thread.into(),9));
        let result=(|| {
            let turns=self.request_with_timeout("thread/turns/list",json!({"threadId":thread,"cursor":cursor,"limit":4,"sortDirection":"desc","itemsView":"notLoaded"}),Duration::from_secs(12))?;
            let rows=turns["data"].as_array().filter(|r|r.len()<=4).ok_or("CHAT_HISTORY_INVALID")?;
            let mut messages=Vec::new();
            for turn in rows {
                let id=turn["id"].as_str().filter(|s|!s.is_empty()&&s.len()<=256).ok_or("CHAT_HISTORY_INVALID")?;
                let items=self.request_with_timeout("thread/items/list",json!({"threadId":thread,"turnId":id,"cursor":null,"limit":20,"sortDirection":"desc"}),Duration::from_secs(12))?;
                let rows=items["data"].as_array().filter(|r|r.len()<=20).ok_or("CHAT_HISTORY_INVALID")?;
                // A long tool-heavy turn may omit its initiating user message
                // from the tail. Read its real first item, never invent a prompt.
                let first=self.request_with_timeout("thread/items/list",json!({"threadId":thread,"turnId":id,"cursor":null,"limit":1,"sortDirection":"asc"}),Duration::from_secs(12))?;
                let first_rows=first["data"].as_array().filter(|r|r.len()<=1).ok_or("CHAT_HISTORY_INVALID")?;
                messages.extend(project_public_chat_turn(id,first_rows,rows)?);
            }
            Ok(json!({"messages":messages,"nextCursor":turns["nextCursor"]}))
        })();
        self.public_chat_read=None;
        result
    }

    /// Read one public message by native identity for a displayed media reference.
    /// No resume, turn write, private reasoning, or title-based routing.
    pub fn read_public_chat_item(&mut self, thread:&str, turn:&str, item:&str) -> Result<Option<String>,String> {
        if self.guard.is_some() || [thread,turn,item].iter().any(|s|s.is_empty()||s.len()>256) {
            return Err("PUBLIC_CHAT_READ_UNAVAILABLE".into());
        }
        self.public_chat_read=Some((thread.into(),1));
        let result=(|| {
            let value=self.request_with_timeout("thread/items/list",json!({"threadId":thread,"turnId":turn,"cursor":null,"limit":20,"sortDirection":"desc"}),Duration::from_secs(12))?;
            let rows=value["data"].as_array().filter(|r|r.len()<=20).ok_or("CHAT_HISTORY_INVALID")?;
            for row in rows {
                if row["turnId"].as_str()!=Some(turn){return Err("CHAT_HISTORY_IDENTITY_MISMATCH".into());}
                let message=&row["item"];
                if message["id"].as_str()!=Some(item){continue;}
                let text=match message["type"].as_str(){
                    Some("agentMessage") if matches!(message["phase"].as_str(),Some("commentary"|"final_answer"))=>message["text"].as_str().map(str::to_owned),
                    Some("userMessage")=>message["content"].as_array().map(|r|r.iter().filter(|c|c["type"]=="text").filter_map(|c|c["text"].as_str()).collect::<Vec<_>>().join("\n")),
                    _=>None,
                };
                return Ok(text.filter(|s|!s.trim().is_empty()&&s.len()<=1_000_000));
            }
            Ok(None)
        })();
        self.public_chat_read=None;
        result
    }

    /// The only guarded turn write entry. The gate must synchronously persist
    /// the exact request ID/approved target before returning. Any later failure
    /// is ambiguous; this method never retries.
    pub fn dispatch_turn(
        &mut self,
        params: Value,
        before_write: &mut dyn FnMut(u64, &Value) -> Result<(), String>,
    ) -> Result<NativeTurnAck, String> {
        if self.guard.is_none() {
            return Err("GUARDED_ADAPTER_REQUIRED".into());
        }
        let thread_id = params
            .get("threadId")
            .and_then(Value::as_str)
            .ok_or("CAPABILITY_UNVERIFIED")?
            .to_owned();
        let mut registered = None;
        let result = self.request_inner(
            "turn/start",
            params,
            Duration::from_secs(30),
            false,
            Some(&mut |id, p| {
                before_write(id, p)?;
                registered = Some(id);
                Ok(())
            }),
        )?;
        let turn_id = result
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .ok_or("CAPABILITY_UNVERIFIED")?
            .to_owned();
        Ok(NativeTurnAck {
            adapter_epoch: self.epoch.clone(),
            request_id: json!(registered.ok_or("DISPATCH_NOT_REGISTERED")?),
            thread_id,
            turn_id,
        })
    }

    fn request_inner(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
        cleanup: bool,
        before_turn_write: Option<&mut dyn FnMut(u64, &Value) -> Result<(), String>>,
    ) -> Result<Value, String> {
        #[cfg(debug_assertions)]
        {
            *self.request_counts.entry(method.to_string()).or_default() += 1;
        }
        if matches!(method, "thread/turns/list" | "thread/items/list")
            && (self.guard.is_some() || self.latest_turn_observation.is_some() || self.exact_turn_status_read.is_some())
        {
            if let Some((thread,remaining))=self.exact_turn_status_read.as_mut() {
                if method!="thread/turns/list" || params["threadId"].as_str()!=Some(thread.as_str()) || *remaining==0 || params["limit"].as_u64()!=Some(4) || params["sortDirection"].as_str()!=Some("desc") || params["itemsView"].as_str()!=Some("notLoaded") || !params.get("cursor").is_some_and(|v|v.is_null()||v.as_str().is_some_and(|s|s.len()<=512)){return Err("EXACT_TURN_STATUS_RPC_FORBIDDEN".into());}
                *remaining-=1;
            } else if let Some((thread,remaining))=self.public_chat_read.as_mut() {
                let head_item=method=="thread/items/list"&&params["sortDirection"].as_str()==Some("asc")&&params["limit"].as_u64()==Some(1)&&params.get("cursor").is_some_and(Value::is_null);
                if params.get("threadId").and_then(Value::as_str)!=Some(thread.as_str())||*remaining==0||(!head_item&&(params["sortDirection"].as_str()!=Some("desc")||params["limit"].as_u64()!=Some(if method=="thread/turns/list"{4}else{20}))){return Err("NORMAL_HISTORY_RPC_FORBIDDEN".into());}
                *remaining-=1;
            } else if let Some(permit) = self.latest_turn_observation.as_mut() {
                let same_thread = params.get("threadId").and_then(Value::as_str)
                    == Some(permit.thread_id.as_str());
                let cursor_is_null = params.get("cursor").is_some_and(Value::is_null);
                if method == "thread/turns/list" {
                    if !same_thread
                        || !cursor_is_null
                        || params.get("limit").and_then(Value::as_u64) != Some(1)
                        || params.get("sortDirection").and_then(Value::as_str) != Some("desc")
                        || params.get("itemsView").and_then(Value::as_str) != Some("notLoaded")
                        || permit.turn_listed
                    {
                        return Err("NORMAL_HISTORY_RPC_FORBIDDEN".into());
                    }
                    permit.turn_listed = true;
                } else {
                    let valid_turn = params
                        .get("turnId")
                        .and_then(Value::as_str)
                        .is_some_and(|id| !id.is_empty() && id.len() <= 256);
                    if !same_thread
                        || !cursor_is_null
                        || params.get("limit").and_then(Value::as_u64) != Some(20)
                        || params.get("sortDirection").and_then(Value::as_str) != Some("desc")
                        || !valid_turn
                        || permit.fallback_turn_id.as_deref()
                            != params.get("turnId").and_then(Value::as_str)
                        || permit.fallback_item_listed
                    {
                        return Err("NORMAL_HISTORY_RPC_FORBIDDEN".into());
                    }
                    permit.fallback_item_listed = true;
                }
            } else {
                let permit = self
                    .history_recovery
                    .as_mut()
                    .ok_or("NORMAL_HISTORY_RPC_FORBIDDEN")?;
                let same_thread = params.get("threadId").and_then(Value::as_str)
                    == Some(permit.thread_id.as_str());
                let limit = params
                    .get("limit")
                    .and_then(Value::as_u64)
                    .is_some_and(|v| (1..=20).contains(&v));
                let cursor_ok = params
                    .get("cursor")
                    .is_some_and(|v| v.is_null() || v.as_str().is_some_and(|s| s.len() <= 512));
                if !same_thread
                    || !limit
                    || !cursor_ok
                    || params.get("sortDirection").and_then(Value::as_str) != Some("asc")
                {
                    return Err("NORMAL_HISTORY_RPC_FORBIDDEN".into());
                }
                if method == "thread/turns/list" {
                    if params.get("itemsView").and_then(Value::as_str) != Some("notLoaded")
                        || permit.turn_pages >= 3
                    {
                        return Err("NORMAL_HISTORY_RPC_FORBIDDEN".into());
                    }
                    permit.turn_pages += 1;
                } else {
                    if params.get("turnId").and_then(Value::as_str) != Some(permit.turn_id.as_str())
                        || permit.item_pages >= 4
                    {
                        return Err("NORMAL_HISTORY_RPC_FORBIDDEN".into());
                    }
                    permit.item_pages += 1;
                }
            }
        }
        if let Some(guard) = self.guard.as_ref().filter(|_| !cleanup) {
            guard.check()?;
            if method == "turn/start" && before_turn_write.is_none() {
                return Err("DURABLE_DISPATCH_GATE_REQUIRED".into());
            }
            if !matches!(
                method,
                "initialize"
                    // Read-only Project catalog methods are used only to present
                    // an existing native conversation for explicit review.
                    | "project/list"
                    | "project/read"
                    | "thread/start"
                    | "thread/list"
                    | "thread/goal/get"
                    | "thread/goal/set"
                    | "thread/goal/clear"
                    | "thread/read"
                    | "thread/resume"
                    | "config/read"
                    | "turn/start"
                    | "turn/interrupt"
                    | "account/read"
                    | "thread/turns/list"
                    | "thread/items/list"
            ) {
                return Err("NATIVE_METHOD_NOT_ALLOWED".into());
            }
            if method == "thread/goal/get" {
                if let Some(thread) = params.get("threadId").and_then(Value::as_str) {
                    self.observed_safe_goals.remove(thread);
                }
            }
            if method == "thread/start" && self.candidate_start_attempted {
                return Err("CREATION_UNKNOWN_NO_RETRY".into());
            }
            if matches!(method, "thread/read" | "thread/resume") {
                authorize_guarded_metadata_step(
                    method,
                    &params,
                    &mut self.observed_safe_goals,
                    &mut self.initial_metadata_reads,
                    &mut self.verified_metadata_reads,
                )?;
            }
            if method == "thread/goal/clear" {
                if self.creation.candidate().as_deref()
                    != params.get("threadId").and_then(Value::as_str)
                    || self.creation.candidate().is_none()
                    || self.goal_clear_attempted
                {
                    return Err("OWN_NEW_CANDIDATE_REQUIRED".into());
                }
            }
            if method == "thread/goal/set" {
                let candidate = self
                    .creation
                    .candidate()
                    .ok_or("OWN_NEW_CANDIDATE_REQUIRED")?;
                super::preparation::validate_initial_paused(
                    &params,
                    &candidate,
                    "Router initialization only. No engineering task; clear before binding.",
                )?;
                if self.initial_goal_attempted {
                    return Err("INITIAL_GOAL_ALREADY_ATTEMPTED".into());
                }
            }
        }
        if self.closed.load(Ordering::SeqCst) {
            return Err(
                "Codex backend disconnected. Use Reconnect before sending another request."
                    .to_string(),
            );
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = mpsc::channel();
        {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| "Codex response registry is unavailable")?;
            if pending.len() >= 32 {
                return Err("NATIVE_PENDING_LIMIT".into());
            }
            pending.insert(id, sender);
        }
        let _cleanup = PendingCleanup(self.pending.clone(), id);
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let stdin = self
            .stdin
            .as_mut()
            .ok_or("Codex app-server stdin is closed")?;
        if let Some(guard) = self.guard.as_ref().filter(|_| !cleanup) {
            guard.check()?;
        }
        if message.to_string().len() > 8 * 1024 * 1024 {
            return Err("NATIVE_FRAME_LIMIT".into());
        }
        if self.guard.is_some() && method == "thread/goal/set" {
            self.initial_goal_attempted = true;
        }
        if self.guard.is_some() && method == "thread/start" {
            self.candidate_start_attempted = true;
            self.creation.request_id.store(id, Ordering::SeqCst);
        }
        if self.guard.is_some() && method == "thread/goal/clear" {
            self.goal_clear_attempted = true;
        }
        if let Some(guard) = self.guard.as_ref().filter(|_| !cleanup) {
            if method == "turn/start" {
                guard.reserve_turn_request(
                    message
                        .pointer("/params/threadId")
                        .and_then(Value::as_str)
                        .ok_or("CAPABILITY_UNVERIFIED")?,
                )?;
            }
        }
        if let Some(gate) = before_turn_write {
            gate(id, &message["params"])?;
            if let Some(guard) = &self.guard {
                guard.check()?;
            }
        }
        writeln!(stdin, "{message}")
            .map_err(|error| format!("Could not send {method} to Codex: {error}"))?;
        stdin
            .flush()
            .map_err(|error| format!("Could not flush {method} to Codex: {error}"))?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(guard) = self.guard.as_ref().filter(|_| !cleanup) {
                guard.check()?;
            }
            if Instant::now() >= deadline {
                return Err(format!("Timed out waiting for Codex {method}"));
            }
            match receiver.recv_timeout(
                Duration::from_millis(25).min(deadline.saturating_duration_since(Instant::now())),
            ) {
                Ok(value) => {
                    if let Some(guard) = self.guard.as_ref().filter(|_| !cleanup) {
                        guard.check()?;
                    }
                    if self.guard.is_some() && method == "thread/goal/get" {
                        if let Ok(response) = &value {
                            if let Err(error) = super::preparation::verify_existing_goal(
                                response,
                                message
                                    .pointer("/params/threadId")
                                    .and_then(Value::as_str)
                                    .ok_or("CAPABILITY_UNVERIFIED")?,
                            ) {
                                if let Some(guard) = self.guard.as_ref().filter(|_| !cleanup) {
                                    guard.fail("UNSAFE_EXISTING_GOAL");
                                }
                                return Err(error);
                            }
                            if let Some(thread) =
                                message.pointer("/params/threadId").and_then(Value::as_str)
                            {
                                self.observed_safe_goals.insert(thread.into());
                            }
                        }
                    }
                    if self.guard.is_some() && method == "thread/read" {
                        let thread = message
                            .pointer("/params/threadId")
                            .and_then(Value::as_str)
                            .ok_or("CAPABILITY_UNVERIFIED")?;
                        if self.initial_metadata_reads.contains(thread) {
                            let exact_read = value
                                .as_ref()
                                .is_ok_and(|response| exact_metadata_response(response, thread));
                            let exact_not_loaded = value
                                .as_ref()
                                .is_err_and(|error| exact_not_loaded_metadata_error(error, thread));
                            if !exact_read && !exact_not_loaded {
                                if let Some(guard) = self.guard.as_ref().filter(|_| !cleanup) {
                                    guard.fail("CAPABILITY_UNVERIFIED");
                                }
                                return Err("CAPABILITY_UNVERIFIED".into());
                            }
                            // A same-id not-loaded response is the sole error
                            // that may unlock the exact metadata-only resume.
                            // Its returned error remains visible to the
                            // application, which still verifies resume and
                            // Goal safety before any write is possible.
                            self.verified_metadata_reads.insert(thread.into());
                        }
                    }
                    return value;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("NATIVE_RESPONSE_CLOSED".into())
                }
            }
        }
    }

    #[cfg(debug_assertions)]
    pub fn request_counts(&self) -> HashMap<String, u64> {
        self.request_counts.clone()
    }

    pub fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        if let Some(guard) = &self.guard {
            guard.check()?;
            if method != "initialized" || !params.is_null() {
                return Err("NATIVE_NOTIFICATION_NOT_ALLOWED".into());
            }
        }
        let message = if params.is_null() {
            json!({ "jsonrpc": "2.0", "method": method })
        } else {
            json!({ "jsonrpc": "2.0", "method": method, "params": params })
        };
        let stdin = self
            .stdin
            .as_mut()
            .ok_or("Codex app-server stdin is closed")?;
        writeln!(stdin, "{message}")
            .map_err(|error| format!("Could not send Codex notification {method}: {error}"))?;
        stdin
            .flush()
            .map_err(|error| format!("Could not flush Codex notification {method}: {error}"))
    }

    /// Responds to a server-initiated JSON-RPC request. `raw_id` is retained as
    /// JSON rather than converted to a number or string: the app-server schema
    /// permits either representation and they are distinct request identities.
    pub fn respond_to_server_request(
        &mut self,
        raw_id: &Value,
        result: Value,
    ) -> Result<(), String> {
        if self.guard.is_some() {
            return Err("FORBIDDEN".into());
        }
        self.write_server_response(raw_id, result)
    }

    pub fn respond_verified(&mut self, reply: super::requests::NativeReply) -> Result<(), String> {
        let guard = self.guard.as_ref().ok_or("FORBIDDEN")?;
        guard.check()?;
        if reply.epoch != self.epoch {
            return Err("STALE_ADAPTER_EPOCH".into());
        }
        self.write_server_response(&reply.id, reply.result)
    }

    fn write_server_response(&mut self, raw_id: &Value, result: Value) -> Result<(), String> {
        let message = server_request_response_message(raw_id, result);
        let stdin = self
            .stdin
            .as_mut()
            .ok_or("Codex app-server stdin is closed")?;
        writeln!(stdin, "{message}")
            .map_err(|error| format!("Could not respond to Codex server request: {error}"))?;
        stdin
            .flush()
            .map_err(|error| format!("Could not flush Codex server request response: {error}"))
    }

    pub fn seal_prewrite_after_shutdown(
        &mut self,
        adapter_epoch: &str,
    ) -> Result<SealedPrewrite, String> {
        uuid::Uuid::parse_str(adapter_epoch).map_err(|_| "INVALID_ADAPTER_EPOCH")?;
        if self.epoch != adapter_epoch {
            return Err("STALE_ADAPTER_EPOCH".into());
        }
        self.shutdown_guarded()?;
        self.guard
            .as_ref()
            .ok_or("GUARDED_ADAPTER_REQUIRED")?
            .prove_no_turn_write_after_exit()?;
        if self.stdin.is_some() || !self.drained || !self.closed.load(Ordering::SeqCst) {
            return Err("PREWRITE_PROOF_UNAVAILABLE".into());
        }
        Ok(SealedPrewrite {
            adapter_epoch: adapter_epoch.into(),
        })
    }

    pub fn exited_and_drained(&self) -> bool {
        self.closed.load(Ordering::SeqCst) && self.drained
    }

    /// Preview exit reports pending and retains durable locks; never force-kills.
    pub fn shutdown_guarded(&mut self) -> Result<(), String> {
        self.closing.store(true, Ordering::SeqCst);
        self.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self.child.try_wait().map_err(|_| "EXIT_PENDING")? {
                Some(status) => {
                    if !self.drained {
                        self.stdout_done
                            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                            .map_err(|_| "OBSERVATION_DRAIN_FAILED")?;
                        self.drained = true;
                    }
                    self.closed.store(true, Ordering::SeqCst);
                    if !status.success() {
                        return Err("NATIVE_EXIT_FAILED".into());
                    }
                    if let Some(guard) = &self.guard {
                        guard.check()?;
                    }
                    return Ok(());
                }
                None if Instant::now() >= deadline => return Err("EXIT_PENDING".into()),
                None => thread::sleep(Duration::from_millis(25)),
            }
        }
    }

    pub fn shutdown(&mut self) {
        if self.guard.is_some() {
            if !self.closed.load(Ordering::SeqCst) {
                let _ = self.shutdown_guarded();
            }
            return;
        }
        self.stdin.take();
        // Router owns this app-server process. A tray Quit must not wait
        // forever for an unresponsive child, but it must also not leave an
        // owned process running after the Tauri host exits.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
            }
        }
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl Drop for CodexAdapter {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn spawn_stdout_reader(
    stdout: impl std::io::Read + Send + 'static,
    pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>>,
    closed: Arc<AtomicBool>,
    turns:Arc<Mutex<TurnRegistry>>,
    sink: Arc<dyn EventSink>,
    event_listener: Arc<dyn Fn(&Value) + Send + Sync>,
    guard: Option<PreparationGuard>,
    done: mpsc::Sender<()>,
    closing: Arc<AtomicBool>,
    creation: Arc<CreationObservation>,
    stderr_detail: Arc<Mutex<Option<String>>>,
    shared_scope:Option<Arc<Mutex<HashSet<String>>>>,
) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let line = match bounded_frame(&mut reader) {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(reason) => {
                    if let Some(g) = &guard {
                        g.fail(reason);
                    }
                    break;
                }
            };
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                if let Some(g) = &guard {
                    g.fail("FRAME_CORRUPT");
                    break;
                }
                sink.feed(FeedEvent {
                    id: "stdout-non-json".to_string(),
                    kind: crate::codex::protocol::FeedKind::Warning,
                    method: "stdout/non-json".to_string(),
                    item_id: None,
                    thread_id: None,
                    turn_id: None,
                    text: None,
                    detail: Some("Codex app-server emitted non-JSON stdout data.".to_string()),
                    raw: None,
                });
                continue;
            };
            if let Some(g) = &guard {
                g.observe(&message);
            }
            let permitted=shared_scope.as_ref().map(|scope|message.pointer("/params/threadId").or_else(||message.pointer("/params/thread/id")).and_then(Value::as_str).is_some_and(|id|scope.lock().is_ok_and(|s|s.contains(id)))).unwrap_or(true);
            match classify_stdout_message(&message) {
                StdoutMessageKind::ServerRequest => {
                    // A JSON-RPC request from app-server has both `id` and
                    // `method`. It is not a response to any Router request,
                    // even when its numeric id happens to overlap ours. The
                    // listener receives the original JSON so callers can keep
                    // the exact string-or-integer request ID for the response.
                    // Do not map this through the raw presentation feed.
                    if permitted {event_listener(&message);}
                }
                StdoutMessageKind::ClientResponse => {
                    creation.observe(&message);
                    if let Some(id) = message.get("id").and_then(Value::as_u64) {
                        if let Some(sender) =
                            pending.lock().ok().and_then(|mut map| map.remove(&id))
                        {
                            let result = message
                                .get("error")
                                .map(|error| Err(format!("Codex JSON-RPC error: {error}")))
                                .unwrap_or_else(|| {
                                    Ok(message.get("result").cloned().unwrap_or(Value::Null))
                                });
                            let _ = sender.send(result);
                        }
                    }
                }
                StdoutMessageKind::Notification => {
                    if !permitted{continue;}
                    match message.get("method").and_then(Value::as_str) {
                        Some("turn/started")=>{if let(Some(thread),Some(turn))=(message.pointer("/params/threadId").and_then(Value::as_str),notification_turn_id(&message)){if let Ok(mut r)=turns.lock(){r.observed.insert(thread.into(),turn.into());if shared_scope.is_none(){r.owned.insert(thread.into(),turn.into());}}}}
                        Some("turn/completed")=>{if let(Some(thread),Some(turn),Some(status))=(message.pointer("/params/threadId").and_then(Value::as_str),notification_turn_id(&message),message.pointer("/params/turn/status").and_then(Value::as_str).filter(|s|matches!(*s,"completed"|"interrupted"|"failed"))){if let Ok(mut r)=turns.lock(){r.finish(thread,turn,status);}}}
                        _ => {}
                    }
                    event_listener(&message);
                    if sink.wants_feed() {
                        sink.feed(map_notification(&message));
                    }
                }
                StdoutMessageKind::Malformed => {
                    if let Some(g) = &guard {
                        g.fail("FRAME_CORRUPT");
                    }
                }
            }
        }
        if !closing.load(Ordering::SeqCst) {
            if let Some(g) = &guard {
                g.fail("OBSERVATION_CLOSED");
            }
        }
        closed.store(true, Ordering::SeqCst);
        if let Ok(mut map) = pending.lock() {
            let diagnostic = stderr_detail
                .lock()
                .ok()
                .and_then(|detail| detail.clone())
                .filter(|detail| !detail.trim().is_empty())
                .map(|detail| format!("Codex app-server stdout closed unexpectedly: {detail}"))
                .unwrap_or_else(|| "Codex app-server stdout closed unexpectedly".to_string());
            for (_, sender) in map.drain() {
                let _ = sender.send(Err(diagnostic.clone()));
            }
        }
        sink.disconnected();
        let _ = done.send(());
    });
}

fn notification_turn_id(message: &Value) -> Option<&str> {
    message
        .pointer("/params/turnId")
        .and_then(Value::as_str)
        .or_else(|| message.pointer("/params/turn/id").and_then(Value::as_str))
}

pub(crate) fn goal_get_params(thread_id: &str) -> Value {
    json!({ "threadId": thread_id })
}

pub(crate) fn goal_set_status_params(thread_id: &str, status: &str) -> Value {
    json!({ "threadId": thread_id, "status": status })
}

pub(crate) fn goal_clear_params(thread_id: &str) -> Value {
    json!({ "threadId": thread_id })
}

pub(crate) fn turn_interrupt_params(thread_id: &str, turn_id: &str) -> Value {
    json!({ "threadId": thread_id, "turnId": turn_id })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StdoutMessageKind {
    /// A server-initiated JSON-RPC request: it has both `id` and `method`.
    ServerRequest,
    /// A response to a Router-originated request: it has `id`, no `method`,
    /// and a JSON-RPC `result` or `error` payload.
    ClientResponse,
    /// A JSON-RPC notification: it has `method` and no `id`.
    Notification,
    Malformed,
}

fn classify_stdout_message(message: &Value) -> StdoutMessageKind {
    let has_id = message.get("id").is_some();
    let has_method = message.get("method").is_some();
    match (has_id, has_method) {
        (true, true) => StdoutMessageKind::ServerRequest,
        (true, false) if message.get("result").is_some() || message.get("error").is_some() => {
            StdoutMessageKind::ClientResponse
        }
        (false, true) => StdoutMessageKind::Notification,
        _ => StdoutMessageKind::Malformed,
    }
}

fn server_request_response_message(raw_id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": raw_id, "result": result })
}

fn spawn_stderr_reader(
    mut stderr: impl std::io::Read + Send + 'static,
    stderr_detail: Arc<Mutex<Option<String>>>,
) {
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        while let Ok(count) = stderr.read(&mut buffer) {
            if count == 0 {
                break;
            }
            let chunk = String::from_utf8_lossy(&buffer[..count]);
            let chunk = chunk.trim();
            if chunk.is_empty() {
                continue;
            }
            if let Ok(mut detail) = stderr_detail.lock() {
                let mut tail = detail.take().unwrap_or_default();
                if !tail.is_empty() {
                    tail.push('\n');
                }
                tail.push_str(chunk);
                if tail.len() > 2048 {
                    let start = tail
                        .char_indices()
                        .nth(tail.chars().count().saturating_sub(2048))
                        .map(|(index, _)| index)
                        .unwrap_or(0);
                    tail = tail[start..].to_string();
                }
                *detail = Some(tail);
            }
        }
    });
}

struct PendingCleanup(
    Arc<Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>>,
    u64,
);
impl Drop for PendingCleanup {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.0.lock() {
            pending.remove(&self.1);
        }
    }
}

fn bounded_frame(reader: &mut impl BufRead) -> Result<Option<String>, &'static str> {
    let mut bytes = Vec::new();
    let count = Read::by_ref(reader)
        .take(8 * 1024 * 1024 + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "OBSERVATION_IO_FAILED")?;
    if count == 0 {
        return Ok(None);
    }
    if count > 8 * 1024 * 1024 {
        return Err("NATIVE_FRAME_LIMIT");
    }
    if bytes.last() != Some(&b'\n') {
        return Err("FRAME_CORRUPT");
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "FRAME_CORRUPT")
}

#[cfg(test)]
mod tests {
    use super::{
        authorize_guarded_metadata_step, bounded_frame, exact_metadata_response,
        exact_not_loaded_metadata_error, spawn_stdout_reader, PendingCleanup,
    };
    use super::{
        classify_stdout_message, goal_clear_params, goal_get_params, goal_set_status_params,
        notification_turn_id, server_request_response_message, turn_interrupt_params,
        StdoutMessageKind,
    };
    use crate::{codex::preparation::PreparationGuard, events::NullEventSink};
    use serde_json::json;
    use std::{
        collections::HashMap,
        io::Cursor,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc, Mutex,
        },
        time::Duration,
    };

    #[test]
    fn current_goal_prevents_old_terminal_turn_from_becoming_completed_task() {
        for (status,phase) in [("active","RUNNING"),("blocked","ACTION_REQUIRED"),("paused","PAUSED")] {
            assert_eq!(super::activity_goal_phase("exact",&json!({"goal":{"threadId":"exact","status":status}})).unwrap(),Some(phase));
        }
        assert_eq!(super::activity_goal_phase("exact",&json!({"goal":null})).unwrap(),None);
        assert_eq!(super::activity_goal_phase("exact",&json!({"goal":{"threadId":"exact","status":"complete"}})).unwrap(),None);
        assert!(super::activity_goal_phase("exact",&json!({"goal":{"threadId":"other","status":"complete"}})).is_err());
        assert!(super::activity_goal_phase("exact",&json!({})).is_err());
    }
    #[test]
    fn persisted_completion_or_interruption_is_never_current_execution_authority(){
        for status in [Some("completed"),Some("interrupted"),Some("inProgress"),None]{assert_eq!(super::activity_phase(false,status),"UNCONFIRMED");}
        assert_eq!(super::activity_phase(true,Some("inProgress")),"RUNNING");assert_eq!(super::activity_phase(true,Some("completed")),"COMPLETE");assert_eq!(super::activity_phase(true,Some("interrupted")),"INTERRUPTED");
    }
    #[test]
    fn shared_reader_filters_foreign_requests_and_never_claims_observed_turns(){
        let turns=Arc::new(Mutex::new(super::TurnRegistry::default()));let events=Arc::new(Mutex::new(Vec::new()));let output=events.clone();let scope=Arc::new(Mutex::new(std::collections::HashSet::from(["own".to_string()])));
        let lines=[json!({"method":"turn/started","params":{"threadId":"foreign","turn":{"id":"foreign-turn"}}}),json!({"id":"foreign-request","method":"item/tool/requestUserInput","params":{"threadId":"foreign"}}),json!({"method":"turn/started","params":{"threadId":"own","turn":{"id":"desktop-turn"}}}),json!({"id":"own-request","method":"item/tool/requestUserInput","params":{"threadId":"own"}})];
        let input=lines.iter().map(|v|format!("{v}\n")).collect::<String>().into_bytes();let(tx,rx)=mpsc::channel();
        spawn_stdout_reader(Cursor::new(input),Arc::new(Mutex::new(HashMap::new())),Arc::new(AtomicBool::new(false)),turns.clone(),Arc::new(NullEventSink),Arc::new(move|v|output.lock().unwrap().push(v.clone())),None,tx,Arc::new(AtomicBool::new(true)),Arc::new(super::CreationObservation::default()),Arc::new(Mutex::new(None)),Some(scope));
        rx.recv_timeout(Duration::from_secs(2)).unwrap();let r=turns.lock().unwrap();assert_eq!(r.observed.get("own").unwrap(),"desktop-turn");assert!(r.observed.get("foreign").is_none());assert!(r.owned.is_empty());assert_eq!(events.lock().unwrap().len(),2);assert_eq!(events.lock().unwrap()[1]["id"],"own-request");
    }
    #[test]
    fn shared_observation_does_not_own_desktop_turn_and_late_ack_never_revives_terminal(){
        let mut r=super::TurnRegistry::default();r.observed.insert("chat".into(),"desktop-turn".into());assert!(r.owned.is_empty());r.finish("chat","desktop-turn","completed");r.acknowledge("chat","desktop-turn");assert!(r.owned.is_empty());
        r.acknowledge("chat","router-turn");assert_eq!(r.owned["chat"],"router-turn");r.finish("chat","router-turn","completed");r.observed.insert("chat".into(),"new-desktop-turn".into());r.acknowledge("chat","router-turn");assert!(r.owned.is_empty());assert_eq!(r.observed["chat"],"new-desktop-turn");
    }
    #[test]
    fn bounded_reader_rejects_large_truncated_and_non_utf8_frames() {
        assert!(bounded_frame(&mut Cursor::new(vec![b'x'; 8 * 1024 * 1024 + 1])).is_err());
        assert!(bounded_frame(&mut Cursor::new(b"{}".to_vec())).is_err());
        assert!(bounded_frame(&mut Cursor::new(vec![255, b'\n'])).is_err());
        assert_eq!(
            bounded_frame(&mut Cursor::new(b"{}\n".to_vec())).unwrap(),
            Some("{}\n".into())
        );
    }
    #[test]
    fn late_event_after_ack_is_observed_before_exit_drain_completes() {
        let guard = PreparationGuard::default();
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let (reply_tx, reply_rx) = mpsc::channel();
        pending.lock().unwrap().insert(1, reply_tx);
        let (done_tx, done_rx) = mpsc::channel();
        let closed = Arc::new(AtomicBool::new(false));
        let input=b"{\"id\":1,\"result\":{}}\n{\"method\":\"turn/started\",\"params\":{\"threadId\":\"own\",\"turn\":{\"id\":\"unexpected\"}}}\n".to_vec();
        spawn_stdout_reader(
            Cursor::new(input),
            pending.clone(),
            closed.clone(),
            Arc::new(Mutex::new(super::TurnRegistry::default())),
            Arc::new(NullEventSink),
            Arc::new(|_| {}),
            Some(guard.clone()),
            done_tx,
            Arc::new(AtomicBool::new(true)),
            Arc::new(super::CreationObservation::default()),
            Arc::new(Mutex::new(None)),None,
        );
        assert!(reply_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_ok());
        done_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(guard.check().is_err());
        assert_eq!(guard.turn_starts(), 1);
        assert!(closed.load(Ordering::SeqCst));
        assert!(pending.lock().unwrap().is_empty());
    }
    #[test]
    fn corrupt_stream_unexpected_eof_and_request_cleanup_fail_closed() {
        for input in [b"bad\n".to_vec(), Vec::new()] {
            let guard = PreparationGuard::default();
            let (done_tx, done_rx) = mpsc::channel();
            spawn_stdout_reader(
                Cursor::new(input),
                Arc::new(Mutex::new(HashMap::new())),
                Arc::new(AtomicBool::new(false)),
                Arc::new(Mutex::new(super::TurnRegistry::default())),
                Arc::new(NullEventSink),
                Arc::new(|_| {}),
                Some(guard.clone()),
                done_tx,
                Arc::new(AtomicBool::new(false)),
                Arc::new(super::CreationObservation::default()),
                Arc::new(Mutex::new(None)),None,
            );
            done_rx.recv_timeout(Duration::from_secs(1)).unwrap();
            assert!(guard.check().is_err());
        }
        let map = Arc::new(Mutex::new(HashMap::new()));
        let (tx, _rx) = mpsc::channel();
        map.lock().unwrap().insert(7, tx);
        drop(PendingCleanup(map.clone(), 7));
        assert!(map.lock().unwrap().is_empty());
    }

    #[test]
    fn server_request_with_numeric_id_is_not_a_client_response() {
        let request = json!({
            "id": 7,
            "method": "item/commandExecution/requestApproval",
            "params": { "threadId": "thread-a", "turnId": "turn-a", "itemId": "item-a" }
        });
        assert_eq!(
            classify_stdout_message(&request),
            StdoutMessageKind::ServerRequest
        );
    }

    #[test]
    fn server_request_with_string_id_is_not_coerced_to_a_client_response() {
        let request = json!({
            "id": "7",
            "method": "item/tool/requestUserInput",
            "params": { "threadId": "thread-a", "turnId": "turn-a", "itemId": "item-a" }
        });
        assert_eq!(
            classify_stdout_message(&request),
            StdoutMessageKind::ServerRequest
        );
    }

    #[test]
    fn only_id_without_method_and_with_result_or_error_is_a_client_response() {
        assert_eq!(
            classify_stdout_message(&json!({ "id": 7, "result": {} })),
            StdoutMessageKind::ClientResponse
        );
        assert_eq!(
            classify_stdout_message(&json!({ "id": 7, "error": { "code": -1 } })),
            StdoutMessageKind::ClientResponse
        );
        assert_eq!(
            classify_stdout_message(&json!({ "id": 7 })),
            StdoutMessageKind::Malformed
        );
    }

    #[test]
    fn method_without_id_is_a_notification() {
        assert_eq!(
            classify_stdout_message(&json!({ "method": "serverRequest/resolved", "params": {} })),
            StdoutMessageKind::Notification
        );
    }

    #[test]
    fn server_request_response_preserves_the_request_id_json_type() {
        let numeric_id = json!(7);
        let string_id = json!("7");
        assert_eq!(
            server_request_response_message(&numeric_id, json!({ "decision": "accept" }))["id"],
            numeric_id
        );
        assert_eq!(
            server_request_response_message(&string_id, json!({ "decision": "accept" }))["id"],
            string_id
        );
    }

    #[test]
    fn goal_requests_use_only_the_exact_thread_identity_and_status_delta() {
        assert_eq!(
            goal_get_params("thread-a"),
            json!({ "threadId": "thread-a" })
        );
        assert_eq!(
            goal_set_status_params("thread-a", "paused"),
            json!({ "threadId": "thread-a", "status": "paused" })
        );
        assert_eq!(
            goal_clear_params("thread-a"),
            json!({ "threadId": "thread-a" })
        );
        assert!(goal_set_status_params("thread-a", "active")
            .get("objective")
            .is_none());
        assert!(goal_set_status_params("thread-a", "active")
            .get("tokenBudget")
            .is_none());
    }

    #[test]
    fn unloaded_metadata_read_can_enable_only_one_exact_resume() {
        let mut safe = std::collections::HashSet::new();
        let mut initial = std::collections::HashSet::new();
        let mut verified = std::collections::HashSet::new();
        let read = json!({"threadId":"exact","includeTurns":false});
        let resume = json!({"threadId":"exact","excludeTurns":true});

        authorize_guarded_metadata_step(
            "thread/read",
            &read,
            &mut safe,
            &mut initial,
            &mut verified,
        )
        .unwrap();
        assert!(initial.contains("exact"));
        assert!(exact_metadata_response(
            &json!({"thread":{"id":"exact"}}),
            "exact"
        ));
        assert!(!exact_metadata_response(
            &json!({"thread":{"id":"other"}}),
            "exact"
        ));
        assert!(exact_not_loaded_metadata_error(
            "Codex JSON-RPC error: {\"code\":-32600,\"message\":\"thread not loaded: exact\"}",
            "exact"
        ));
        assert!(!exact_not_loaded_metadata_error(
            "Codex JSON-RPC error: {\"code\":-32600,\"message\":\"thread not loaded: other\"}",
            "exact"
        ));
        assert!(!exact_not_loaded_metadata_error(
            "thread not loaded: exact",
            "exact"
        ));
        verified.insert("exact".into());
        authorize_guarded_metadata_step(
            "thread/resume",
            &resume,
            &mut safe,
            &mut initial,
            &mut verified,
        )
        .unwrap();
        assert!(verified.is_empty());
        assert!(authorize_guarded_metadata_step(
            "thread/resume",
            &resume,
            &mut safe,
            &mut initial,
            &mut verified,
        )
        .is_err());
        assert!(authorize_guarded_metadata_step(
            "thread/read",
            &json!({"threadId":"exact","includeTurns":true}),
            &mut safe,
            &mut initial,
            &mut verified,
        )
        .is_err());
    }

    #[test]
    fn turn_interrupt_requires_the_exact_thread_and_turn_pair() {
        assert_eq!(
            turn_interrupt_params("thread-a", "turn-a"),
            json!({ "threadId": "thread-a", "turnId": "turn-a" })
        );
        assert_ne!(
            turn_interrupt_params("thread-a", "turn-a"),
            turn_interrupt_params("thread-a", "turn-b")
        );
    }

    #[test]
    fn turn_lifecycle_reads_both_official_turn_id_shapes() {
        assert_eq!(
            notification_turn_id(&json!({ "params": { "turnId": "turn-a" } })),
            Some("turn-a")
        );
        assert_eq!(
            notification_turn_id(&json!({ "params": { "turn": { "id": "turn-b" } } })),
            Some("turn-b")
        );
    }
}
