//! Exact fictional native peer: no owner chats, model prompts or global state.
use crate::host_application::{RouterCore, Session};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
const THREAD: &str = "fixture-empty-completed";
fn fixture(mode: &str, shared: bool) -> (tempfile::TempDir, RouterCore) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime/item15-fixtures");
    std::fs::create_dir_all(&root).unwrap();
    let d = tempfile::tempdir_in(root).unwrap();
    let store = Arc::new(crate::RouterStore::open_at(d.path().join("router.db")).unwrap());
    store
        .enable_codex_watch(
            THREAD,
            "fixture",
            d.path().to_str().unwrap(),
            &router_core::store::codex_watch::WatchSnapshot {
                state: "IDLE".into(),
                turn_id: None,
                item_id: None,
                text: String::new(),
            },
        )
        .unwrap();
    let script = r#"const fs=require('node:fs'),rl=require('node:readline').createInterface({input:process.stdin}),root=process.argv[1],mode=process.argv[2];rl.on('line',line=>{const r=JSON.parse(line);if(r.id===undefined)return;fs.appendFileSync(root+'/rpc.jsonl',JSON.stringify(r)+'\n');let result={};const thread={id:'fixture-empty-completed',cwd:root,status:{type:mode==='ACTIVE'?'active':mode==='COLD'?'notLoaded':mode==='UNKNOWN'?'unknown':'idle'}};if(r.method==='thread/read'||r.method==='thread/resume')result={thread};else if(r.method==='thread/turns/list')result={data:[{id:'completed-empty',status:mode==='INTERRUPTED'?'interrupted':'completed',itemsView:'notLoaded'}],nextCursor:null};else if(r.method==='thread/items/list')result={data:[],nextCursor:null};else if(r.method==='thread/goal/get')result={goal:null};else if(r.method==='turn/start')result={turn:{id:'new-fictional-turn'}};else if(r.method!=='initialize'){process.stdout.write(JSON.stringify({id:r.id,error:{code:-32600,message:'Unexpected '+r.method}})+'\n');return;}process.stdout.write(JSON.stringify({id:r.id,result})+'\n');});"#;
    let mut command = std::process::Command::new("node");
    command.args(["-e", script]).arg(d.path()).arg(mode);
    let listener: Arc<dyn Fn(&Value) + Send + Sync> = Arc::new(|_| {});
    let mut adapter = if shared {
        crate::codex::adapter::CodexAdapter::start_shared_validation(command, listener).unwrap()
    } else {
        crate::codex::adapter::CodexAdapter::start_command_validation(command, listener).unwrap()
    };
    adapter.initialize().unwrap();
    let core = RouterCore {
        store,

        session: Arc::new(Mutex::new(Session {
            adapter: Some(adapter),
            ..Default::default()
        })),
    };
    (d, core)
}
#[test]
fn shared_idle_completed_without_assistant_accepts_next_reviewed_send_once() {
    let (d, core) = fixture("IDLE", true);
    let view = serde_json::to_value(crate::watch_chat::state(&core, THREAD).unwrap()).unwrap();
    assert_eq!(view["latestTurn"]["confirmedTerminal"], true);
    assert_eq!(view["latestTurn"]["confirmationSource"], "SHARED_IDLE_READ");
    assert_eq!(view["watch"]["snapshot"]["state"], "RESULT_PENDING");
    assert!(view["publicMessages"].as_array().unwrap().is_empty());
    let command = || crate::watch_chat::ChatCommand::Send {
        thread_id: THREAD.into(),
        id: "00000000-0000-4000-8000-000000000015".into(),
        generation: 1,
        source_sequence: None,
        expected_turn_id: Some("completed-empty".into()),
        mode: "SEND".into(),
        queue_requested: false,
        text: "fictional isolated transport check".into(),
        options: Default::default(),
    };
    let result = crate::watch_chat::command(&core, command()).unwrap();
    assert_eq!(result["status"], "SENT");
    assert_eq!(result["turnId"], "new-fictional-turn");
    let duplicate = crate::watch_chat::command(&core, command()).unwrap();
    assert_eq!(duplicate["id"], result["id"]);
    let requests = std::fs::read_to_string(d.path().join("rpc.jsonl")).unwrap();
    assert_eq!(
        requests
            .lines()
            .filter(|r| serde_json::from_str::<Value>(r).unwrap()["method"] == "turn/start")
            .count(),
        1
    );
    core.shutdown_owned_adapter();
}
#[test]
fn persisted_cold_active_unknown_and_interrupted_never_become_confirmed() {
    for (mode, shared) in [
        ("COLD", true),
        ("ACTIVE", true),
        ("UNKNOWN", true),
        ("INTERRUPTED", true),
        ("IDLE", false),
    ] {
        let (d, core) = fixture(mode, shared);
        let view = serde_json::to_value(crate::watch_chat::state(&core, THREAD).unwrap()).unwrap();
        assert_eq!(
            view["latestTurn"]["confirmedTerminal"], false,
            "{mode}/{shared}"
        );
        let requests = std::fs::read_to_string(d.path().join("rpc.jsonl")).unwrap();
        assert!(!requests
            .lines()
            .any(|r| serde_json::from_str::<Value>(r).unwrap()["method"] == "turn/start"));
        core.shutdown_owned_adapter();
    }
}
#[test]
fn terminal_reconcile_preserves_newer_owned_turn_and_exact_identity() {
    let (d, core) = fixture("IDLE", true);
    let mut session = core.session.lock().unwrap();
    let adapter = session.adapter.as_mut().unwrap();
    adapter
        .subscribe_shared_thread(THREAD, d.path().to_str().unwrap())
        .unwrap();
    let metadata = json!({"id":THREAD,"cwd":d.path().to_str().unwrap(),"status":{"type":"idle"}});
    let turn = json!({"id":"completed-empty","status":"completed"});
    assert!(!adapter.reconcile_shared_idle_completed(
        "other",
        d.path().to_str().unwrap(),
        &metadata,
        &turn
    ));
    assert!(!adapter.reconcile_shared_idle_completed(THREAD, "wrong-cwd", &metadata, &turn));
    adapter.mark_turn_started(THREAD, "newer");
    assert!(!adapter.reconcile_shared_idle_completed(
        THREAD,
        d.path().to_str().unwrap(),
        &metadata,
        &turn
    ));
    assert_eq!(adapter.active_turn_id(THREAD).as_deref(), Some("newer"));
    assert!(adapter
        .terminal_turn_status(THREAD, "completed-empty")
        .is_none());
    drop(session);
    core.shutdown_owned_adapter();
}
