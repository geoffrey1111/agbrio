//! Host-owned Pi workers. Each task has one serialized writer and a continuously
//! consumed event channel; request IDs, native identity and receipts are durable.
use super::{
    permissions, projection,
    session::{plain, Session},
    settings,
    transport::Event,
};
use router_core::store::{
    pi::{CommandReceipt, Task},
    RouterStore,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

type Reply = mpsc::SyncSender<Result<Value, String>>;
enum Request {
    Send {
        request: String,
        generation: i64,
        mode: String,
        text: String,
        images: Vec<Value>,
        reply: Reply,
    },
    History {
        reply: Reply,
    },
    Stop,
}
struct Actor {
    sender: mpsc::SyncSender<Request>,
    worker: thread::JoinHandle<()>,
    directory: PathBuf,
    writable: bool,
    audit:Value,
    coordinator:String,
}
#[derive(Default)]
pub(crate) struct PiService {
    actors: Mutex<HashMap<String, Actor>>,
    launch:Mutex<()>,
}
impl PiService {
    pub(crate) fn audit(&self,key:&str)->Option<Value>{self.actors.try_lock().ok().and_then(|a|a.get(key).map(|v|v.audit.clone()))}
    /// Only a reviewed create/restore operation may enter here. Never implicitly
    /// recreate an empty lost session or resume an uncertain send.
    pub(crate) fn start(
        &self,
        store: Arc<RouterStore>,
        key: &str,
        restore: bool,
    ) -> Result<Task, String> {
        let _launch=self.launch.lock().map_err(|_|"PI_SERVICE_UNAVAILABLE")?;
        self.start_locked(store,key,restore)
    }
    fn start_locked(&self,store:Arc<RouterStore>,key:&str,restore:bool)->Result<Task,String>{
        let mut actors = self.actors.lock().map_err(|_| "PI_SERVICE_UNAVAILABLE")?;
        let task = store.pi_task(key)?;
        if actors.get(&task.id).is_some_and(|a| a.worker.is_finished()) {
            if let Some(actor) = actors.remove(&task.id) {
                let _ = actor.worker.join();
            }
        }
        if actors.contains_key(&task.id) {
            let scope = store.pi_scope(&task.scope_id)?;
            if !scope.enabled || scope.generation != task.scope_generation {
                return Err("PI_SCOPE_CHANGED".into());
            }
            return Ok(task);
        }
        if !restore && task.state != "CREATING" {
            return Ok(task);
        }
        let scope = store.pi_scope(&task.scope_id)?;
        if !scope.enabled || scope.generation != task.scope_generation {
            return Err("PI_SCOPE_CHANGED".into());
        }
        let access=store.pi_task_access(&task.id)?.unwrap_or(router_core::store::pi_policy::PiTaskAccess{roots:vec![task.directory.clone()],writable:scope.writable,terminal:false,administrator:false,inherit_context:false});
        if access.terminal&&store.pi_workspace_policy(&scope.id)?.is_none(){return Err("PI_TERMINAL_WORKSPACE_POLICY_REQUIRED".into());}
        if access.administrator&&!super::elevation::identity(None)?.administrator{super::cohort::require_ready()?;}
        let directory=PathBuf::from(permissions::authorize_root(&task.directory)?);
        if !access.roots.iter().any(|root|permissions::canonical_directory(root).is_ok_and(|r|directory.starts_with(r))){return Err("PI_DIRECTORY_OUT_OF_SCOPE".into());}
        let coordinated=store.pi_workspace_policy(&scope.id)?.is_some();
        let coordinator=json!({"provider":task.parent.provider,"id":task.parent.external_id,"root":task.parent.cwd.replace('/',"\\").to_lowercase()}).to_string();
        if access.writable
            && actors.values().any(|a| {
                a.writable
                    && (!coordinated||a.coordinator!=coordinator)
                    && (a.directory.starts_with(&directory) || directory.starts_with(&a.directory))
            })
        {
            return Err("PI_DIRECTORY_WRITER_ALREADY_ACTIVE".into());
        }
        if uuid::Uuid::parse_str(&task.id).is_err() {
            return Err("PI_TASK_ID_INVALID".into());
        }
        let owned = store.pi_storage_root()?.join(&task.id);
        std::fs::create_dir_all(&owned).map_err(|_| "PI_SESSION_DIRECTORY_UNAVAILABLE")?;
        let owned = permissions::canonical_directory(&plain(&owned))?;
        if owned.starts_with(&directory) || directory.starts_with(&owned) {
            return Err("PI_STORAGE_OVERLAPS_PROJECT".into());
        }
        drop(actors);
        let requested = if restore {
            task.selected
                .clone()
                .or(store.pi_requested_selection(&task.id)?)
        } else {
            store.pi_requested_selection(&task.id)?
        };
        let resume = if restore {
            if !task.file_verified
                && !task
                    .native_session_file
                    .as_ref()
                    .is_some_and(|f| Path::new(f).is_file())
            {
                return Err("PI_ORIGINAL_SESSION_NOT_DURABLE".into());
            }
            Some((
                task.native_session_id
                    .as_deref()
                    .ok_or("PI_SESSION_ID_UNCONFIRMED")?,
                Path::new(
                    task.native_session_file
                        .as_deref()
                        .ok_or("PI_SESSION_FILE_UNCONFIRMED")?,
                ),
            ))
        } else {
            None
        };
        let mut checkpoint =
            |native: &str, file: &Path, observed: Option<&router_core::store::pi::Selection>| {
                store.pi_checkpoint_native_identity(&task.id, native, &plain(file), observed)
            };
        store.pi_bind_task_access(&task.id,task.scope_generation,&access)?;
        if store.pi_workspace_policy(&scope.id)?.is_some()&&!access.administrator&&super::elevation::identity(None)?.administrator{return Err("PI_STANDARD_MODE_REQUIRES_STANDARD_AGBRIO".into());}
        let resources=super::resources::prepare(&task,&access,&owned)?;
        let session = match Session::open_scoped(
            &super::installation()?,
            &directory,
            &owned,
            access.writable,
            access.terminal,
            requested.as_ref(),
            resume,
            Some(&mut checkpoint),
            Some((&access,&resources)),
        ) {
            Ok(v) => v,
            Err(error) => {
                if error=="PI_ADMIN_BACKEND_PREPARE_ON_DESKTOP"{return Err(error);}
                store.pi_set_state(&task.id, "UNKNOWN", None, Some(&error))?;
                return Err(error);
            }
        };
        let current_scope=store.pi_scope(&task.scope_id)?;
        if !current_scope.enabled||current_scope.generation!=task.scope_generation{let _=session.close();return Err("PI_SCOPE_CHANGED".into());}
        let durable = session.verify_durable(&directory)?;
        let task = if restore {
            store.pi_advance_runtime_generation(&task.id, task.generation)?
        } else {
            task
        };
        let epoch = uuid::Uuid::new_v4().to_string();
        let task = store.pi_checkpoint_session(
            &task.id,
            &session.native_id,
            &plain(&session.native_file),
            durable,
            &session.selected,
            if restore {
                &task.selection_source
            } else if requested.is_some() {
                "EXPLICIT"
            } else {
                "DEFAULT"
            },
            &epoch,
        )?;
        if restore {
            public_history(&store, &task.id, &session)?;
            store.pi_recover_previous_runtime(&task.id, task.generation)?;
        }
        let audit=json!({"access":access,"resources":resources,"nativeSkills":session.skill_catalog,"capabilities":session.capabilities,"process":session.rpc.process_identity()?,"contextApplied":false,"resourceEpoch":session.resource_epoch()});
        store.pi_put_execution_audit(&task.id,&audit)?;
        let (tx, rx) = mpsc::sync_channel(8);
        let key = task.id.clone();
        let worker_directory = directory.clone();
        let worker = thread::spawn(move || run_worker(store, key, worker_directory, session, rx));
        self.actors.lock().map_err(|_|"PI_SERVICE_UNAVAILABLE")?.insert(
            task.id.clone(),
            Actor {
                sender: tx,
                worker,
                directory,
                writable: access.writable,
                audit,
                coordinator,
            },
        );
        Ok(task)
    }
    pub(crate) fn reconfigure_and_restore(&self,store:Arc<RouterStore>,key:&str,expected:i64,scope:&str,generation:i64,access:&router_core::store::pi_policy::PiTaskAccess)->Result<Task,String>{
        let _launch=self.launch.lock().map_err(|_|"PI_SERVICE_UNAVAILABLE")?;
        {let mut actors=self.actors.lock().map_err(|_|"PI_SERVICE_UNAVAILABLE")?;if let Some(actor)=actors.get(key){if !actor.worker.is_finished(){return Err("PI_CLOSE_BEFORE_ACCESS_CHANGE".into());}}if let Some(actor)=actors.remove(key){let _=actor.worker.join();}}
        let original=store.pi_task(key)?;
        let file=original.native_session_file.as_ref().ok_or("PI_ORIGINAL_SESSION_NOT_DURABLE")?;
        let native=original.native_session_id.as_deref().ok_or("PI_SESSION_ID_UNCONFIRMED")?;
        super::session::verify_session_header(Path::new(file),native,Path::new(&original.directory))?;
        if access.administrator{super::cohort::require_ready()?;}
        store.pi_reconfigure_task_access(key,expected,scope,generation,access)?;
        self.start_locked(store,key,true)
    }
    fn dispatch(&self, key: &str, request: impl FnOnce(Reply) -> Request) -> Result<Value, String> {
        let sender = self
            .actors
            .lock()
            .map_err(|_| "PI_SERVICE_UNAVAILABLE")?
            .get(key)
            .ok_or("PI_SESSION_NOT_LOADED")?
            .sender
            .clone();
        let (tx, rx) = mpsc::sync_channel(1);
        sender
            .try_send(request(tx))
            .map_err(|_| "PI_LOCAL_REQUEST_NOT_QUEUED")?;
        rx.recv_timeout(Duration::from_secs(75))
            .map_err(|_| "PI_OPERATION_RESPONSE_UNCONFIRMED".to_string())?
    }
    pub(super) fn refresh_failed_context(&self,store:Arc<RouterStore>,key:&str)->Result<(),String>{
        let _launch=self.launch.lock().map_err(|_|"PI_SERVICE_UNAVAILABLE")?;
        let t=store.pi_task(key)?;
        let idle_terminal=matches!(t.state.as_str(),"FAILED"|"RESULT_READY"|"PARTIAL"|"CANCELLED"|"INTERRUPTED"|"IDLE");
        if !self.loaded(key){let known_prelaunch_failure=t.state=="UNKNOWN"&&t.error_code.as_deref()==Some("PI_ADMIN_BACKEND_PREPARE_ON_DESKTOP");if !idle_terminal&&!known_prelaunch_failure{return Err("PI_PRIOR_RUN_UNCONFIRMED_RESTORE_REQUIRED_NOT_SENT".into());}self.start_locked(store,key,true)?;return Ok(());}
        if !idle_terminal{return Ok(());}
        let changed=self.audit(key).and_then(|a|serde_json::from_value::<super::resources::Resources>(a["resources"].clone()).ok()).is_some_and(|r|!r.unchanged());
        if !changed{return Ok(());}
        let live=self.history(key)?;if live["nativeBusy"]!=false{return Err("PI_CONTEXT_CHANGED_WAIT_UNTIL_IDLE".into());}
        if live["pendingMessageCount"].as_u64()!=Some(0){return Err("PI_CONTEXT_CHANGED_PENDING_INPUTS_NOT_RESTARTED".into());}
        let current=store.pi_task(key)?;if !matches!(current.state.as_str(),"FAILED"|"RESULT_READY"|"PARTIAL"|"CANCELLED"|"INTERRUPTED"|"IDLE")||current.runtime_epoch!=t.runtime_epoch||current.generation!=t.generation{return Err("PI_CONTEXT_CHANGED_WAIT_UNTIL_IDLE".into());}
        {let mut actors=self.actors.lock().map_err(|_|"PI_SERVICE_UNAVAILABLE")?;if let Some(actor)=actors.remove(key){let _=actor.sender.send(Request::Stop);let _=actor.worker.join();}}
        self.start_locked(store,key,true)?;Ok(())
    }
    pub(crate) fn send(
        &self,
        store: Arc<RouterStore>,
        key: &str,
        request: String,
        generation: i64,
        mode: String,
        text: String,
        images: Vec<Value>,
    ) -> Result<Value, String> {
        if store.pi_command_receipt(key,&request).is_err(){let was_loaded=self.loaded(key);self.refresh_failed_context(store.clone(),key)?;if store.pi_task(key)?.generation!=generation{return Err(if was_loaded{"PI_CONTEXT_REFRESHED_READ_CHAT_AND_PREPARE_NEW_ACTION_NOT_SENT"}else{"PI_SESSION_RELOADED_READ_CHAT_AND_PREPARE_NEW_ACTION_NOT_SENT"}.into());}}
        let (receipt, claimed) = store.pi_claim_command(
            key,
            &request,
            generation,
            &mode,
            &journal_input(&text, &images),
        )?;
        if !claimed {
            return Ok(json!({"receipt":receipt,"replayed":false}));
        }
        let identity = request.clone();
        let result = self.dispatch(key, |reply| Request::Send {
            request,
            generation,
            mode,
            text,
            images,
            reply,
        });
        if result.as_ref().err().is_some_and(|e| {
            matches!(
                e.as_str(),
                "PI_LOCAL_REQUEST_NOT_QUEUED" | "PI_SESSION_NOT_LOADED"
            )
        }) {
            store.pi_abandon_unattempted(key, &identity)?;
        }
        result
    }

    pub(crate) fn close_task(
        &self,
        store: &RouterStore,
        key: &str,
        request: &str,
        generation: i64,
    ) -> Result<Value, String> {
        let (receipt, claimed) = store.pi_claim_command(
            key,
            request,
            generation,
            "CANCEL",
            &json!({"operation":"CLOSE_PI_TASK"}),
        )?;
        if !claimed {
            return Ok(json!({"receipt":receipt,"automaticRetry":false}));
        }
        let mut actors = self.actors.lock().map_err(|_| "PI_SERVICE_UNAVAILABLE")?;
        if !store.pi_begin_native_command(
            key,
            request,
            &receipt.payload_hash,
            generation,
            "CANCEL",
        )? {
            return Err("PI_CLOSE_ALREADY_CLAIMED".into());
        }
        let existed = if let Some(actor) = actors.remove(key) {
            let _ = actor.sender.send(Request::Stop);
            let _ = actor.worker.join();
            true
        } else {
            false
        };
        store.pi_cancel_unattempted_inputs(key)?;
        let receipt =
            store.pi_finish_command(key, request, &receipt.payload_hash, "APPLIED", None, None)?;
        let task = store.pi_task(key)?;
        Ok(
            json!({"receipt":receipt,"closed":true,"alreadyClosed":!existed,"state":task.state,"cancelled":task.state=="CANCELLED","automaticRetry":false}),
        )
    }
    pub(crate) fn history(&self, key: &str) -> Result<Value, String> {
        self.dispatch(key, |reply| Request::History { reply })
    }
    pub(crate) fn loaded(&self, key: &str) -> bool {
        self.actors
            .lock()
            .ok()
            .and_then(|actors| actors.get(key).map(|actor| !actor.worker.is_finished()))
            .unwrap_or(false)
    }
    pub(crate) fn shutdown(&self) {
        let _launch=match self.launch.lock(){Ok(v)=>v,Err(_)=>return};
        let actors = match self.actors.lock() {
            Ok(mut actors) => std::mem::take(&mut *actors),
            Err(_) => return,
        };
        for (_, actor) in actors {
            let _ = actor.sender.send(Request::Stop);
            let _ = actor.worker.join();
        }
    }
    pub(crate) fn scope_in_use(&self, store: &RouterStore, scope: &str) -> Result<bool, String> {
        let actors = self.actors.lock().map_err(|_| "PI_SERVICE_UNAVAILABLE")?;
        for (key, actor) in actors.iter() {
            if !actor.worker.is_finished() && store.pi_task(key)?.scope_id == scope {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(crate) fn stop_scope(&self, store: &RouterStore, scope: &str) -> Result<(), String> {
        // Keep the retiring cohort in the admission fence until its exact
        // worker/job is gone. Removing it before join permits a second writer.
        let mut actors = self.actors.lock().map_err(|_| "PI_SERVICE_UNAVAILABLE")?;
        let keys = actors
            .keys()
            .filter_map(|key| {
                store
                    .pi_task(key)
                    .ok()
                    .filter(|t| t.scope_id == scope)
                    .map(|_| key.clone())
            })
            .collect::<Vec<_>>();
        for key in keys {
            if let Some(actor) = actors.remove(&key) {
                let _ = actor.sender.send(Request::Stop);
                let _ = actor.worker.join();
            }
        }
        Ok(())
    }
}
impl Drop for PiService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn public_history(store: &RouterStore, key: &str, session: &Session) -> Result<Value, String> {
    let history = session.history()?;
    let entries = history["entries"].as_array().ok_or("PI_HISTORY_INVALID")?;
    let task = store.pi_task(key)?;
    let index = entries
        .iter()
        .filter_map(|e| e["id"].as_str().map(|id| (id, e)))
        .collect::<HashMap<_, _>>();
    let messages: Vec<Value> = entries
        .iter()
        .filter_map(|entry| {
            let mut public = projection::public_entry(entry)?;
            if public["role"] == "user" {
                let mut parent = entry["parentId"].as_str();
                let mut seen = std::collections::HashSet::new();
                while let Some(id) = parent {
                    if !seen.insert(id) {
                        break;
                    }
                    let Some(prior) = index.get(id) else { break };
                    if prior["type"] == "message" {
                        break;
                    }
                    if prior["customType"] == "agbrio.consumed" {
                        public["clientId"] = prior["data"]["requestId"].clone();
                        break;
                    }
                    parent = prior["parentId"].as_str();
                }
            }
            Some(public)
        })
        .collect();
    let active_native = projection::active_branch(entries, history["leafId"].as_str())?
        .iter()
        .filter_map(|e| e["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    store.pi_store_public_history(
        key,
        task.generation,
        &messages,
        &active_native,
        history["leafId"].as_str(),
    )?;
    for entry in entries {
        if entry["type"] != "custom" || entry["customType"] != "agbrio.consumed" {
            continue;
        }
        let Some(request) = entry["data"]["requestId"].as_str() else {
            continue;
        };
        let Some(hash) = entry["data"]["payloadHash"].as_str() else {
            continue;
        };
        let Some(native) = entry["id"].as_str() else {
            continue;
        };
        if let Ok(receipt) = store.pi_command_receipt(key, request) {
            if receipt.payload_hash == hash
                && matches!(
                    receipt.status.as_str(),
                    "ACCEPTED" | "UNKNOWN" | "SENDING" | "ACKNOWLEDGED"
                )
            {
                store.pi_finish_command(key, request, hash, "CONSUMED", Some(native), None)?;
            }
        }
    }
    if let Some(resource)=entries.iter().rev().find(|e|e["customType"]=="agbrio.resources"){
        if let Some(mut audit)=store.pi_execution_audit(key)?.filter(|a|a["resourceEpoch"]==resource["data"]["runtimeId"]){audit["contextApplied"]=json!(true);audit["resourceReceiptId"]=resource["id"].clone();audit["loadedResources"]=resource["data"].clone();store.pi_put_execution_audit(key,&audit)?;}
    }
    if let Some(mut audit)=store.pi_execution_audit(key)?{audit["skillReads"]=json!(entries.iter().filter(|e|e["customType"]=="agbrio.skill_read"&&e["data"]["runtimeId"]==audit["resourceEpoch"]).map(|e|e["data"].clone()).collect::<Vec<_>>());audit["modelRequestAttempts"]=json!(entries.iter().filter(|e|e["customType"]=="agbrio.model_request"&&e["data"]["runtimeId"]==audit["resourceEpoch"]).count());store.pi_put_execution_audit(key,&audit)?;}
    apply_terminal_receipts(store, key, entries, &active_native)?;
    let active = projection::active_branch(entries, history["leafId"].as_str())?
        .iter()
        .filter_map(|e| {
            projection::public_entry(e).and_then(|v| v["id"].as_str().map(str::to_owned))
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"messages":messages,"activeEntryIds":active,"leafId":history["leafId"],"nativeBusy":history["isStreaming"]==true||history["isCompacting"]==true,"pendingMessageCount":history["pendingMessageCount"]}),
    )
}
fn apply_terminal_receipts(
    store: &RouterStore,
    key: &str,
    entries: &[Value],
    active: &[String],
) -> Result<(), String> {
    for receipt in entries
        .iter()
        .filter(|e| e["type"] == "custom" && e["customType"] == "agbrio.settled")
    {
        let data = &receipt["data"];
        let leaf = data["leafId"].as_str();
        let branch = projection::active_branch(entries, leaf)?;
        let requests = data["requests"]
            .as_array()
            .ok_or("PI_TERMINAL_RECEIPT_INVALID")?;
        if requests.is_empty() {
            continue;
        }
        let state = projection::settled_state(
            entries,
            leaf,
            data["aborted"]
                .as_bool()
                .ok_or("PI_TERMINAL_RECEIPT_INVALID")?,
        )?;
        let latest_user = branch
            .iter()
            .rposition(|e| e["type"] == "message" && e["message"]["role"] == "user");
        let current = latest_user.map(|at| &branch[at + 1..]).unwrap_or(&[]);
        let last = current
            .iter()
            .rev()
            .find(|e| e["type"] == "message" && e["message"]["role"] == "assistant")
            .and_then(|e| projection::public_entry(e));
        let result = json!({"lastAssistant":last,"leafId":leaf,"nativeTerminalReceiptId":receipt["id"],"piRunId":data["runId"],"completedAt":receipt["timestamp"].as_str().and_then(crate::claude_quota::parse_utc_ms),"hadToolErrors":current.iter().any(|e|e["message"]["role"]=="toolResult"&&e["message"]["isError"]==true),"consumption":"NOT_CONFIRMED"});
        let applied=store.pi_record_terminal(
            key,
            receipt["id"]
                .as_str()
                .ok_or("PI_TERMINAL_RECEIPT_INVALID")?,
            requests,
            state,
            &result,
            leaf.is_some_and(|id| active.iter().any(|a| a == id)),
        )?;
        if leaf.is_some_and(|id|active.iter().any(|a|a==id)){let _=store.notify_pi_terminal(key,receipt["id"].as_str().unwrap_or_default(),state,result["completedAt"].as_i64());}let _=applied;
    }
    Ok(())
}
fn journal_input(text: &str, images: &[Value]) -> Value {
    let attachments = images
        .iter()
        .filter_map(|v| v["agbrioAttachmentId"].as_str())
        .collect::<Vec<_>>();
    let images:Vec<Value>=images.iter().map(|v|json!({"mimeType":v["mimeType"],"encodedSha256":format!("{:x}",Sha256::digest(v["data"].as_str().unwrap_or_default().as_bytes()))})).collect();
    json!({"text":text,"images":images,"attachments":attachments})
}
fn finish_send(
    store: &RouterStore,
    key: &str,
    receipt: &CommandReceipt,
    result: Result<Value, String>,
) -> Result<Value, String> {
    match result {
        Ok(accepted) => {
            store.pi_note_effective_mode(
                key,
                &receipt.request_id,
                accepted["effectiveMode"]
                    .as_str()
                    .ok_or("PI_NATIVE_MODE_INVALID")?,
            )?;
            let entry = accepted["nativeEntryId"]
                .as_str()
                .ok_or("PI_INPUT_RECEIPT_UNCONFIRMED")?;
            let receipt = store.pi_finish_command(
                key,
                &receipt.request_id,
                &receipt.payload_hash,
                "ACCEPTED",
                Some(entry),
                None,
            )?;
            store.pi_set_state(key, "RUNNING", None, None)?;
            Ok(json!({"receipt":receipt,"accepted":true,"completed":false}))
        }
        Err(error) => {
            if matches!(
                error.as_str(),
                "PI_PROMPT_REJECTED"
                    | "PI_CONTEXT_CHANGED_BEFORE_SEND"
                    | "PI_INPUT_NOT_ACCEPTED"
                    | "PI_INPUT_INVALID"
                    | "PI_IMAGE_INVALID"
                    | "PI_ATTACHMENT_LIMIT"
                    | "PI_SEND_MODE_INVALID"
                    | "PI_RUNNING_USE_STEER"
            ) {
                store.pi_finish_command(
                    key,
                    &receipt.request_id,
                    &receipt.payload_hash,
                    "NOT_SENT",
                    None,
                    Some(&error),
                )?;
                return Err(error);
            }
            let receipt = store.pi_finish_command(
                key,
                &receipt.request_id,
                &receipt.payload_hash,
                "UNKNOWN",
                None,
                Some(&error),
            )?;
            store.pi_set_state(key, "UNKNOWN", None, Some(&error))?;
            Ok(json!({"receipt":receipt,"accepted":null,"completed":false}))
        }
    }
}
fn run_worker(
    store: Arc<RouterStore>,
    key: String,
    directory: PathBuf,
    session: Session,
    requests: mpsc::Receiver<Request>,
) {
    loop {
        if session.overflow.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = store.pi_set_state(&key, "UNKNOWN", None, Some("PI_EVENT_STREAM_INCOMPLETE"));
            break;
        }
        while let Ok(event) = session.events.try_recv() {
            match event {
                Event::Exited => {
                    if !store.pi_task(&key).is_ok_and(|t|t.state=="FAILED"&&t.error_code.as_deref().is_some_and(|e|e.starts_with("PI_CONTEXT_")||e.starts_with("PI_SKILL_")||e=="PI_RUN_NOT_DELEGATED")){let _ = store.pi_set_state(&key, "UNKNOWN", None, Some("PI_PROCESS_EXITED"));}
                    return;
                }
                Event::Record(value) => {
                    let result: Result<(), String> = (|| {
                        match value["type"].as_str() {
                            Some("agbrio_guard_error")=>{store.pi_set_state(&key,"FAILED",None,Some(value["errorCode"].as_str().unwrap_or("PI_CONTEXT_UNAVAILABLE")))?;return Ok(());},
                            Some("agent_start") => {}
                            Some("message_end" | "entry_appended") => {
                                public_history(&store, &key, &session)?;
                            }
                            Some("agent_settled") => {
                                public_history(&store, &key, &session)?;
                                let task = store.pi_task(&key)?;
                                store.pi_checkpoint_session(
                                    &key,
                                    &session.native_id,
                                    &plain(&session.native_file),
                                    session.verify_durable(&directory)?,
                                    &session.selected,
                                    &task.selection_source,
                                    task.runtime_epoch
                                        .as_deref()
                                        .ok_or("PI_RUNTIME_EPOCH_MISSING")?,
                                )?;
                            }
                            Some("extension_ui_request") => {
                                if matches!(value["method"].as_str(),Some("confirm"|"select"|"input"|"editor")){
                                    let current=store.pi_task(&key)?;
                                    store.pi_record_owner_question(&key,current.runtime_epoch.as_deref().ok_or("PI_RUNTIME_EPOCH_MISSING")?,&value)?;
                                    let _=store.observe_pi_current_task(&key);
                                }
                                // No unreviewed extension dialog can execute a
                                // command while owner-only UI is unavailable.
                                if let Some(id) = value["id"].as_str() {
                                    if matches!(
                                        value["method"].as_str(),
                                        Some("confirm" | "select" | "input" | "editor")
                                    ) {
                                        session.rpc.owner_ui_response(id, false)?;
                                    }
                                }
                            }
                            _ => {}
                        }
                        Ok(())
                    })();
                    if let Err(error) = result {
                        let _ = store.pi_set_state(&key, "UNKNOWN", None, Some(&error));
                    }
                    if matches!(
                        value["type"].as_str(),
                        Some("agent_start" | "agent_settled" | "message_end")
                    ) {
                        if let Ok(task) = store.pi_task(&key) {
                            if let Ok(mut view) = super::presentation::watch(&task) {
                                if let Ok(registered) =
                                    store.registered_codex_watch(&view.thread_id)
                                {
                                    view.snapshot.text =
                                        view.snapshot.text.chars().take(6000).collect();
                                    let _ = store.record_codex_watch(
                                        &view.thread_id,
                                        registered.generation,
                                        &view.snapshot,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        match requests.recv_timeout(Duration::from_millis(30)) {
            Ok(Request::Send {
                request,
                generation,
                mode,
                text,
                images,
                reply,
            }) => {
                let result = (|| {
                    session.directory_intact()?;
                    let policy = mode.clone();
                    let mode = if mode == "AUTO" {
                        let state = settings::call(&session.rpc, json!({"type":"get_state"}))?;
                        if state["isStreaming"] == true || state["isCompacting"] == true {
                            "STEER"
                        } else {
                            "SEND"
                        }
                    } else {
                        mode.as_str()
                    };
                    let receipt = store.pi_command_receipt(&key, &request)?;
                    if !store.pi_begin_native_command(
                        &key,
                        &request,
                        &receipt.payload_hash,
                        generation,
                        mode,
                    )? {
                        return Ok(
                            json!({"receipt":store.pi_command_receipt(&key,&request)?,"replayed":false}),
                        );
                    }
                    let result = finish_send(
                        &store,
                        &key,
                        &receipt,
                        session.submit(&request, &receipt.payload_hash, &text, &images, &policy),
                    )?;
                    public_history(&store, &key, &session)?;
                    Ok(result)
                })();
                if result.is_err() {
                    let _ = store.pi_abandon_unattempted(&key, &request);
                }
                let _ = reply.send(result);
            }
            Ok(Request::History { reply }) => {
                let _ = reply.send(public_history(&store, &key, &session));
            }
            Ok(Request::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = store.pi_cancel_unattempted_inputs(&key);
                let observed = session.history();
                let busy = observed
                    .as_ref()
                    .is_ok_and(|v| v["isStreaming"] == true || v["isCompacting"] == true);
                let mut uncertain = observed.is_err();
                if busy {
                    if session.cancel().is_err() {
                        uncertain = true;
                    } else {
                        let _ = public_history(&store, &key, &session);
                        let _ = store.pi_cancel_native_queue_receipts(&key);
                    }
                }
                let _ = public_history(&store, &key, &session);
                if let Ok(task) = store.pi_task(&key) {
                    if let Ok(durable) = session.verify_durable(&directory) {
                        let _ = store.pi_checkpoint_session(
                            &key,
                            &session.native_id,
                            &plain(&session.native_file),
                            durable,
                            &session.selected,
                            &task.selection_source,
                            task.runtime_epoch.as_deref().unwrap_or_default(),
                        );
                        if !durable {
                            let _ = store.pi_set_state(
                                &key,
                                "EMPTY_LOST",
                                None,
                                Some("PI_EMPTY_SESSION_NOT_DURABLE"),
                            );
                        }
                    }
                    if uncertain {
                        let _ = store.pi_set_state(
                            &key,
                            "UNKNOWN",
                            task.result.as_ref(),
                            Some("PI_CLOSE_NATIVE_CANCEL_UNCONFIRMED"),
                        );
                    }
                }
                let _ = session.close();
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}
