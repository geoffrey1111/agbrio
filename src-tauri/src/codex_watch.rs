//! Read-only monitor for explicitly selected native conversations.
use crate::host_application::*;
use router_core::store::codex_watch::{CodexWatch, WatchSnapshot};
use serde_json::{json, Value};
fn id(v: &Value) -> Result<String, String> {
    v.as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .map(str::to_owned)
        .ok_or("WATCH_PROTOCOL_INVALID".into())
}
pub(crate) fn inspect(adapter: &mut crate::codex::adapter::CodexAdapter, thread_id: &str) -> Result<(String,String,WatchSnapshot),String> {
    let (label,cwd,snapshot,_,_)=inspect_live(adapter,thread_id)?;Ok((label,cwd,snapshot))
}
pub(crate) fn inspect_live(
    adapter: &mut crate::codex::adapter::CodexAdapter,
    thread_id: &str,
) -> Result<(String, String, WatchSnapshot,Vec<Value>,Option<String>), String> {
    let m = adapter.request_with_timeout(
        "thread/read",
        json!({"threadId":thread_id,"includeTurns":false}),
        CODEX_OBSERVER_REQUEST_TIMEOUT,
    )?;
    let thread = m.get("thread").ok_or("WATCH_PROTOCOL_INVALID")?;
    if thread.get("id").and_then(Value::as_str) != Some(thread_id) {
        return Err("WATCH_IDENTITY_MISMATCH".into());
    }
    let cwd = thread
        .get("cwd")
        .and_then(Value::as_str)
        .filter(|p| std::path::Path::new(p).is_absolute())
        .ok_or("WATCH_PROJECT_UNAVAILABLE")?
        .to_owned();
    // Subscribe only to an already loaded native conversation. Loading a cold
    // thread (potentially with a persisted Goal) is a separate write-readiness
    // action; a passive watch must not cause that lifecycle transition.
    if matches!(thread.pointer("/status/type").and_then(Value::as_str),Some("active"|"idle")){
        adapter.subscribe_shared_thread(thread_id,&cwd)?;
    }
    let label = thread
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| thread.get("preview").and_then(Value::as_str))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("未命名 Codex 对话")
        .chars()
        .take(120)
        .collect::<String>();
    adapter.begin_latest_turn_observation(thread_id)?;
    let turns=match adapter.request_with_timeout("thread/turns/list",json!({"threadId":thread_id,"cursor":null,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"}),CODEX_OBSERVER_REQUEST_TIMEOUT){Ok(v)=>v,Err(e) if is_unmaterialized_empty_codex_thread(thread_id,&e)=>json!({"data":[]}),Err(e)=>return Err(e)};
    let rows = turns
        .get("data")
        .and_then(Value::as_array)
        .filter(|r| r.len() <= 1)
        .ok_or("WATCH_PROTOCOL_INVALID")?;
    let Some(turn) = rows.first() else {
        return Ok((
            label,
            cwd,
            WatchSnapshot {
                state: "IDLE".into(),
                turn_id: None,
                item_id: None,
                text: String::new(),
            },Vec::new(),None,
        ));
    };
    let turn_id = id(&turn["id"])?;
    let external_active=thread.pointer("/status/type").and_then(Value::as_str)==Some("active");
    let state = if external_active {"RUNNING"} else {match turn["status"].as_str() {
        Some("completed") => "RESULT_PENDING",
        Some("inProgress" | "active" | "pending") => "RUNNING",
        Some("failed") => "FAILED",
        // The external app-server's persisted projection also uses interrupted
        // for a live turn owned by another process. It cannot prove an abort.
        Some("interrupted") => "INCOMPLETE",
        _ => "UNKNOWN",
    }};
    let mut snapshot = WatchSnapshot {
        state: state.into(),
        turn_id: Some(turn_id.clone()),
        item_id: None,
        text: String::new(),
    };
    if turn.get("itemsView").and_then(Value::as_str) != Some("notLoaded") {
        return Ok((label, cwd, snapshot,Vec::new(),if state=="RUNNING"{Some("EXECUTING".into())}else{None}));
    }
    adapter.begin_latest_turn_item_fallback(thread_id, &turn_id)?;
    let items=adapter.request_with_timeout("thread/items/list",json!({"threadId":thread_id,"turnId":turn_id,"cursor":null,"limit":20,"sortDirection":"desc"}),CODEX_OBSERVER_REQUEST_TIMEOUT)?;
    let rows = items
        .get("data")
        .and_then(Value::as_array)
        .filter(|r| r.len() <= 20)
        .ok_or("WATCH_PROTOCOL_INVALID")?;
    if let Some((item_id, text)) = select_message(rows, &turn_id, state == "RESULT_PENDING")? {
        snapshot.item_id = Some(item_id);
        snapshot.text = text;
        if state == "RESULT_PENDING" {
            snapshot.state = "RESULT_READY".into();
        }
    }
    let messages=public_messages(rows,&turn_id)?;
    let activity=if state=="RUNNING"{Some(if rows.first().is_some_and(|row|row["item"]["type"]=="reasoning"){"THINKING"}else{"EXECUTING"}.into())}else{None};
    Ok((label,cwd,snapshot,messages,activity))
}
fn public_messages(rows:&[Value],turn:&str)->Result<Vec<Value>,String>{
    let mut messages=Vec::new();
    for row in rows.iter().rev(){
        if row["turnId"].as_str()!=Some(turn){return Err("WATCH_TURN_MISMATCH".into());}
        let item=&row["item"];
        if item["type"]!="agentMessage"||!matches!(item["phase"].as_str(),Some("commentary"|"final_answer")){continue;}
        let text=item["text"].as_str().filter(|t|!t.trim().is_empty()&&t.len()<=1_000_000).ok_or("WATCH_MESSAGE_INVALID")?;
        messages.push(json!({"id":id(&item["id"])?,"turnId":turn,"role":"assistant","text":text}));
    }Ok(messages)
}
// User-visible assistant commentary/final only. No reasoning, commands or user prompts.
fn select_message(
    rows: &[Value],
    turn: &str,
    final_only: bool,
) -> Result<Option<(String, String)>, String> {
    for row in rows {
        if row.get("turnId").and_then(Value::as_str) != Some(turn) {
            return Err("WATCH_TURN_MISMATCH".into());
        }
        let item = &row["item"];
        let phase = item["phase"].as_str();
        if item["type"] != "agentMessage"
            || !matches!(phase, Some("commentary" | "final_answer"))
            || (final_only && phase != Some("final_answer"))
        {
            continue;
        }
        let text = item["text"]
            .as_str()
            .filter(|t| !t.trim().is_empty() && t.len() <= 1_000_000)
            .ok_or("WATCH_MESSAGE_INVALID")?;
        return Ok(Some((id(&item["id"])?, text.into())));
    }
    Ok(None)
}
pub(crate) fn candidates(core:&RouterCore)->Result<ExistingCodexThreadCatalog,String>{
 let bound:std::collections::HashSet<_>=core.store.bridge_codex_thread_ids()?.into_iter().collect();let mut catalog=crate::role_bridge::catalog(core)?;catalog.threads.retain(|t|!bound.contains(&t.id));Ok(catalog)
}
pub(crate) fn enable(core: &RouterCore, thread_id: &str) -> Result<(), String> {
    let mut s = core
        .session
        .lock()
        .map_err(|_| "Router session unavailable")?;
    let a = s
        .adapter
        .as_mut()
        .filter(|a| !a.is_closed())
        .ok_or("Codex backend disconnected")?;
    let (label, cwd, baseline) = inspect(a, thread_id)?;
    core.store
        .enable_codex_watch(thread_id, &label, &cwd, &baseline)
}
pub(crate) fn poll(
    store: &crate::persistence::RouterStore,
    session: &std::sync::Arc<std::sync::Mutex<Session>>,
    watch: &CodexWatch,
    epoch: u64,
) -> Result<bool, String> {
    let outcome = (|| {
        let mut s = session.lock().map_err(|_| "Router session unavailable")?;
        if s.codex_observer_epoch != epoch {
            return Ok(false);
        }
        if s.codex_adapter_borrowed && s.adapter.is_none() { return Ok(false); }
        let a = s
            .adapter
            .as_mut()
            .filter(|a| !a.is_closed())
            .ok_or("Codex backend disconnected")?;
        let (_, cwd, mut snapshot) = inspect(a, &watch.thread_id)?;
        let owned=a.active_turn_id(&watch.thread_id);
        if let Some(request)=s.pending_codex_requests.values().find(|r|r.thread_id==watch.thread_id&&!r.responded&&owned.as_deref()==Some(&r.turn_id)){
            if let Some(public)=mobile_codex_request_projection(request){snapshot.state="ACTION_REQUIRED".into();snapshot.turn_id=Some(request.turn_id.clone());snapshot.item_id=Some(request.action_id.clone());snapshot.text=if public.kind=="USER_INPUT"{"Codex 需要你的回答。请打开原对话查看当前请求。".into()}else{"Codex 需要你确认执行请求。请打开原对话查看详情。".into()};}
        }
        if cwd != watch.cwd {
            return Err("WATCH_PROJECT_CHANGED".into());
        }
        store.record_codex_watch(&watch.thread_id, watch.generation, &snapshot)
    })();
    if outcome.is_err() {
        let _ = store.codex_watch_unavailable(&watch.thread_id, watch.generation);
    }
    outcome
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excludes_reasoning_and_requires_exact_turn_and_final_phase() {
        let rows = vec![
            json!({"turnId":"t","item":{"id":"reason","type":"reasoning","text":"hidden"}}),
            json!({"turnId":"t","item":{"id":"comment","type":"agentMessage","phase":"commentary","text":"progress"}}),
            json!({"turnId":"t","item":{"id":"final","type":"agentMessage","phase":"final_answer","text":"result"}}),
        ];
        assert_eq!(
            select_message(&rows, "t", false).unwrap().unwrap().1,
            "progress"
        );
        assert_eq!(
            select_message(&rows, "t", true).unwrap().unwrap().1,
            "result"
        );
        assert!(select_message(&rows, "wrong", true).is_err());
    }
    #[test]
    fn public_live_reports_are_chronological_and_private_items_never_leave_projection(){
        let rows=vec![json!({"turnId":"t","item":{"id":"b","type":"agentMessage","phase":"commentary","text":"second"}}),json!({"turnId":"t","item":{"id":"private","type":"reasoning","text":"hidden"}}),json!({"turnId":"t","item":{"id":"a","type":"agentMessage","phase":"commentary","text":"first"}})];
        let messages=public_messages(&rows,"t").unwrap();assert_eq!(messages.len(),2);assert_eq!(messages[0]["text"],"first");assert_eq!(messages[1]["text"],"second");assert!(public_messages(&rows,"other").is_err());
    }
    #[test]
    #[ignore="explicit read-only existing shared transport, no provider turn or lifecycle mutation"]
    fn real_shared_live_read_only(){
        use std::sync::Arc;
        let target=std::env::var("AIWR_READ_ONLY_WATCH_THREAD").expect("exact authorized watched thread");
        let command=crate::shared_codex::transport_command().unwrap().expect("existing healthy shared gateway only");
        let mut adapter=crate::codex::adapter::CodexAdapter::start_shared_validation(command,Arc::new(|_|{})).unwrap();adapter.initialize().unwrap();
        let (_,_,snapshot,messages,activity)=inspect_live(&mut adapter,&target).unwrap();
        let page=adapter.read_public_chat_page(&target,None).unwrap();
        assert!(messages.iter().all(|m|m["turnId"].as_str()==snapshot.turn_id.as_deref()&&m["role"]=="assistant"));
        let native=page["messages"].as_array().unwrap();
        let root=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/plugin-release-review");std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("read-only-live-proof.json"),serde_json::to_vec_pretty(&json!({"exactThread":true,"state":snapshot.state,"activity":activity,"publicReports":messages.len(),"nativeMessages":native.len(),"userPrompts":native.iter().filter(|m|m["role"]=="user").count(),"requests":adapter.request_counts(),"providerWrites":0})).unwrap()).unwrap();
        adapter.shutdown();
    }
    #[test]
    #[ignore = "one minimum-cost disposable turn on the retained role QA conversation; no resend"]
    fn real_external_codex_writer_is_observed_read_only() {
        assert_eq!(std::env::var("AIWR_REAL_WATCH_GATE").as_deref(), Ok("1"));
        use std::{
            fs,
            sync::{Arc, Mutex},
            time::{Duration, Instant},
        };
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("runtime/codex-notifications");
        fs::create_dir_all(&root).unwrap();
        let proof_dir = root.join(format!("real-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&proof_dir).unwrap();
        fs::write(
            root.join("current-real-case.txt"),
            proof_dir.to_string_lossy().as_bytes(),
        )
        .unwrap();
        let target = "00000000-0000-7000-8000-1f7fcc242e87";
        let store = Arc::new(crate::RouterStore::open_at(proof_dir.join("router.db")).unwrap());
        let mut reader =
            crate::codex::adapter::CodexAdapter::start_validation(Arc::new(|_| {})).unwrap();
        reader.initialize().unwrap();
        let core = RouterCore {
            store: store.clone(),
            chatgpt: Arc::default(),
            session: Arc::new(Mutex::new(Session::default())),
            completed_chatgpt_responses: Arc::default(),
        };
        core.session.lock().unwrap().adapter = Some(reader);
        enable(&core, target).unwrap();
        assert!(store.codex_watch_feed(0).unwrap().events.is_empty());
        let mut writer =
            crate::codex::adapter::CodexAdapter::start_validation(Arc::new(|_| {})).unwrap();
        writer.initialize().unwrap();
        let resumed=writer.request("thread/resume",json!({"threadId":target,"excludeTurns":true,"model":"gpt-6-luna","config":{"model_reasoning_effort":"low"}})).unwrap();
        assert_eq!(resumed["thread"]["id"], target);
        assert_eq!(resumed["model"], "gpt-6-luna");
        assert_eq!(resumed["reasoningEffort"], "low");
        let goal = writer
            .request("thread/goal/get", json!({"threadId":target}))
            .unwrap();
        assert!(!matches!(
            goal.pointer("/goal/status").and_then(Value::as_str),
            Some("active" | "blocked")
        ));
        fs::write(
            proof_dir.join("send-intent.json"),
            serde_json::to_vec(
                &json!({"target":target,"model":"gpt-6-luna","effort":"low","atMostOne":true}),
            )
            .unwrap(),
        )
        .unwrap();
        let turn=start_turn(&mut writer,target,"Disposable Router notification transport validation. Do not change files. Give one brief visible progress update, then output exactly WATCH_REAL_ACK.").unwrap().turn_id;
        fs::write(
            proof_dir.join("accepted.json"),
            serde_json::to_vec(&json!({"threadId":target,"turnId":turn})).unwrap(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(180);
        let mut observed = Vec::new();
        loop {
            assert!(
                Instant::now() < deadline,
                "Monitor failed to observe the accepted exact turn; never resend"
            );
            let w = store.codex_watches().unwrap().remove(0);
            poll(&store, &core.session, &w, 0).unwrap();
            let w = store.codex_watches().unwrap().remove(0);
            observed.push(w.snapshot.state.clone());
            if w.snapshot.turn_id.as_deref() == Some(turn.as_str())
                && w.snapshot.state == "RESULT_READY"
            {
                assert_eq!(w.snapshot.text.trim(), "WATCH_REAL_ACK");
                break;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        let w = store.codex_watches().unwrap().remove(0);
        assert!(!poll(&store, &core.session, &w, 0).unwrap());
        let feed = store.codex_watch_feed(0).unwrap();
        assert!(!feed.events.is_empty());
        let final_event = feed.events.last().unwrap();
        assert_eq!(
            store
                .codex_watch_event(final_event.sequence)
                .unwrap()
                .snapshot
                .text
                .trim(),
            "WATCH_REAL_ACK"
        );
        store.pause_codex_watch(target).unwrap();
        let reopened = crate::RouterStore::open_at(proof_dir.join("router.db")).unwrap();
        assert_eq!(
            reopened.codex_watch_feed(0).unwrap().events.len(),
            feed.events.len()
        );
        assert!(reopened.snapshot().unwrap().endpoint_lineage.is_empty());
        fs::write(proof_dir.join("public-proof.json"),serde_json::to_vec_pretty(&json!({"status":"AUTOMATED_VALIDATION_PASS","threadId":target,"turnId":turn,"model":"gpt-6-luna","effort":"low","externalWriterProcesses":1,"readerProcesses":1,"readerResumes":0,"readerTurnWrites":0,"newTurns":1,"observedStates":observed,"eventCount":feed.events.len(),"fullResult":"WATCH_REAL_ACK","restartAndDedupe":true,"bridgeBindingsCreated":0,"userUat":false})).unwrap()).unwrap();
    }
}
