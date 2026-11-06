//! Explicit live check of the real Host send paths over the real light transport.
//! Temporary database; writes only to the owner-named disposable thread and to one thread
//! it creates and archives. Run with AIWR_LIGHT_EXE, AIWR_LIGHT_THREAD and AIWR_LIGHT_DIR.
use crate::{
    host_application::{capture_pending_codex_request, clear_resolved_codex_request, MobileCodexResponseInput, RouterCore, Session},
    watch_chat::{self, ChatCommand},
};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;

fn view(core: &RouterCore, thread: &str) -> Value {
    serde_json::to_value(watch_chat::state(core, thread).unwrap()).unwrap()
}

fn send(core: &RouterCore, thread: &str, id: &str, mode: &str, expected: Option<String>, text: &str) -> Value {
    let generation = view(core, thread)["watch"]["generation"].as_i64().unwrap();
    watch_chat::command(
        core,
        ChatCommand::Send {
            thread_id: thread.into(),
            id: id.into(),
            generation,
            source_sequence: None,
            expected_turn_id: expected,
            mode: mode.into(),
            queue_requested: false,
            text: text.into(),
            options: Default::default(),
        },
    )
    .unwrap()
}

fn wait(core: &RouterCore, thread: &str, seconds: u64, done: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        let current = view(core, thread);
        if done(&current) {
            return current;
        }
        assert!(Instant::now() < deadline, "timed out; state={} latest={}", current["watch"]["snapshot"]["state"], current["latestTurn"]);
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn settled(state: &Value) -> bool {
    state["ownedTurnId"].is_null() && state["externalBusy"] != true && state["watch"]["snapshot"]["state"] != "RUNNING"
}

#[test]
#[ignore = "explicit live check; needs real Desktop and a disposable thread"]
fn real_host_send_paths_over_the_light_transport() {
    let exe = std::env::var("AIWR_LIGHT_EXE").unwrap();
    let thread = std::env::var("AIWR_LIGHT_THREAD").unwrap();
    let directory = std::env::var("AIWR_LIGHT_DIR").unwrap();
    let root = tempfile::tempdir().unwrap();
    let core = RouterCore {
        store: Arc::new(crate::RouterStore::open_at(root.path().join("router.db")).unwrap()),
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::default(),
    };
    let session = core.session.clone();
    // On its own thread, as in the Host: this runs on the adapter's only reader, which must
    // stay free to deliver the response a request holding the session lock is waiting for.
    let listener: Arc<dyn Fn(&Value) + Send + Sync> = Arc::new(move |message: &Value| {
        let (session, message) = (session.clone(), message.clone());
        std::thread::spawn(move || {
            capture_pending_codex_request(&session, &message);
            clear_resolved_codex_request(&session, &message);
        });
    });
    let mut command = std::process::Command::new(exe);
    command.arg(super::TRANSPORT_FLAG);
    let mut adapter = crate::codex::adapter::CodexAdapter::start_shared_validation(command, listener).unwrap();
    adapter.initialize().unwrap();
    assert!(adapter.is_shared(), "the light transport uses the Host's existing shared-mode fences");
    core.session.lock().unwrap().adapter = Some(adapter);

    crate::codex_watch::enable(&core, &thread).unwrap();
    // An earlier interruption has to be confirmed before the Host accepts a send.
    let first = wait(&core, &thread, 30, |state| settled(state) && (state["watch"]["snapshot"]["state"] != "INCOMPLETE" || state["latestTurn"]["confirmedTerminal"] == true));
    println!("WATCH state={} generation={}", first["watch"]["snapshot"]["state"], first["watch"]["generation"]);

    // Real durable send, replayed with the same ID.
    let send_id = Uuid::new_v4().to_string();
    let anchor = first["watch"]["snapshot"]["turnId"].as_str().map(str::to_owned);
    let ack = send(&core, &thread, &send_id, "SEND", anchor.clone(), "Reply with only the word LIGHTHOST.");
    assert_eq!(ack["status"], "SENT", "{ack}");
    let replay = send(&core, &thread, &send_id, "SEND", anchor, "Reply with only the word LIGHTHOST.");
    assert_eq!(ack["turnId"], replay["turnId"]);
    assert_eq!(core.store.watch_replies(&thread).unwrap().iter().filter(|reply| reply.id == send_id).count(), 1);
    let done = wait(&core, &thread, 120, settled);
    println!("SEND ack={} final={} text_has_reply={} confirmed={}", ack["status"], done["watch"]["snapshot"]["state"], done["watch"]["snapshot"]["text"].as_str().unwrap_or_default().contains("LIGHTHOST"), done["latestTurn"]["confirmedTerminal"]);
    assert!(done["watch"]["snapshot"]["text"].as_str().unwrap_or_default().contains("LIGHTHOST"));
    assert!(done["publicMessages"].as_array().unwrap().iter().any(|m| m["text"].as_str().is_some_and(|t| t.contains("LIGHTHOST"))));

    // Steer through the real path.
    let long = send(&core, &thread, &Uuid::new_v4().to_string(), "SEND", done["watch"]["snapshot"]["turnId"].as_str().map(str::to_owned), "Write the numbers from 1 to 200, one per line.");
    let long_turn = long["turnId"].as_str().unwrap().to_owned();
    wait(&core, &thread, 20, |state| state["watch"]["snapshot"]["state"] == "RUNNING");
    let steered = send(&core, &thread, &Uuid::new_v4().to_string(), "STEER", Some(long_turn.clone()), "Stop and reply with only the word LIGHTSTEER.");
    assert_eq!(steered["status"], "SENT", "{steered}");
    let done = wait(&core, &thread, 120, settled);
    println!("STEER ack={} same_turn={} final={} text_has_reply={}", steered["status"], steered["turnId"] == json!(long_turn), done["watch"]["snapshot"]["state"], done["watch"]["snapshot"]["text"].as_str().unwrap_or_default().contains("LIGHTSTEER"));

    // Stop through the real path.
    let stoppable = send(&core, &thread, &Uuid::new_v4().to_string(), "SEND", done["watch"]["snapshot"]["turnId"].as_str().map(str::to_owned), "Write the numbers from 1 to 300, one per line.");
    let stop_turn = stoppable["turnId"].as_str().unwrap().to_owned();
    wait(&core, &thread, 20, |state| state["watch"]["snapshot"]["state"] == "RUNNING");
    watch_chat::command(&core, ChatCommand::Stop { thread_id: thread.clone(), turn_id: stop_turn.clone() }).unwrap();
    let stopped = wait(&core, &thread, 60, settled);
    println!("STOP final={} latest_status={} confirmed={}", stopped["watch"]["snapshot"]["state"], stopped["latestTurn"]["status"], stopped["latestTurn"]["confirmedTerminal"]);
    assert_eq!(stopped["latestTurn"]["status"], "interrupted");

    // A native approval reaches the owner-facing card and its answer reaches Desktop.
    let turn = core.session.lock().unwrap().adapter.as_mut().unwrap().request("turn/start", json!({"threadId":thread,"approvalPolicy":"untrusted","input":[{"type":"text","text":"Use the shell tool to run exactly: cmd /c ver. Then reply with only the word LIGHTAPPROVED."}]})).unwrap();
    println!("APPROVAL turn_started={}", turn["turn"]["id"].is_string());
    let asked = wait(&core, &thread, 90, |state| state["requests"].as_array().is_some_and(|rows| !rows.is_empty()));
    let card = asked["requests"][0].clone();
    println!("APPROVAL card kind={} choices={}", card["kind"], card["choices"]);
    let decision = card["choices"].as_array().unwrap().iter().find_map(|choice| choice["value"].as_str().or(choice["id"].as_str()).or(choice.as_str()).filter(|value| value.starts_with("accept"))).unwrap_or("accept").to_owned();
    watch_chat::command(&core, ChatCommand::Respond {
        thread_id: thread.clone(),
        request_id: card["id"].as_str().or(card["requestId"].as_str()).unwrap().to_owned(),
        input: MobileCodexResponseInput { revision: card["revision"].as_u64().unwrap_or(1), decision: Some(decision), answers: None },
    })
    .unwrap();
    let approved = wait(&core, &thread, 120, |state| settled(state) && state["requests"].as_array().is_none_or(Vec::is_empty));
    println!("APPROVAL final={} text_has_reply={}", approved["watch"]["snapshot"]["state"], approved["watch"]["snapshot"]["text"].as_str().unwrap_or_default().contains("LIGHTAPPROVED"));

    // A new conversation has no first prompt, then accepts its first message on the temporary backend.
    let settings = crate::codex::creation::Settings { model: Some("gpt-6-luna".into()), effort: Some("low".into()), service_tier: None };
    let created = core.start_configured_codex_thread(&directory, &settings).unwrap();
    let new_thread = created["threadId"].as_str().or(created.pointer("/thread/id").and_then(Value::as_str)).unwrap_or_else(|| panic!("{created}")).to_owned();
    crate::codex_watch::enable(&core, &new_thread).unwrap();
    let ack = send(&core, &new_thread, &Uuid::new_v4().to_string(), "SEND", None, "Reply with only the word LIGHTNEW.");
    assert_eq!(ack["status"], "SENT", "{ack}");
    let done = wait(&core, &new_thread, 120, settled);
    println!("CREATE first_send={} final={} text_has_reply={}", ack["status"], done["watch"]["snapshot"]["state"], done["watch"]["snapshot"]["text"].as_str().unwrap_or_default().contains("LIGHTNEW"));
    assert!(done["watch"]["snapshot"]["text"].as_str().unwrap_or_default().contains("LIGHTNEW"));

    // Its writer lock is gone soon after the turn, so Desktop can open it. A later send still works.
    let home = super::native_files::codex_home().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while super::native_files::writer_lock_held(&home, &new_thread) {
        assert!(Instant::now() < deadline, "the temporary backend kept holding the thread");
        std::thread::sleep(Duration::from_millis(500));
    }
    println!("RELEASED lock_free=true");
    let again = send(&core, &new_thread, &Uuid::new_v4().to_string(), "SEND", done["watch"]["snapshot"]["turnId"].as_str().map(str::to_owned), "Reply with only the word LIGHTAGAIN.");
    assert_eq!(again["status"], "SENT", "{again}");
    let done = wait(&core, &new_thread, 120, |state| settled(state) && state["watch"]["snapshot"]["text"].as_str().is_some_and(|text| text.contains("LIGHTAGAIN")));
    println!("SEND_AFTER_RELEASE ack={} final={}", again["status"], done["watch"]["snapshot"]["state"]);

    // A created thread goes straight back out of the owner's sidebar.
    let archived = core.session.lock().unwrap().adapter.as_mut().unwrap().request("thread/archive", json!({"threadId":new_thread}));
    println!("ARCHIVE ok={} thread={new_thread}", archived.is_ok());

    // A Goal write fails before it could claim anything.
    let goal = core.session.lock().unwrap().adapter.as_mut().unwrap().request("thread/goal/set", json!({"threadId":thread,"status":"paused"}));
    assert!(goal.is_err_and(|error| error.contains("CODEX_LIGHT_GOAL_WRITE_UNSUPPORTED")));
    core.shutdown_owned_adapter();
    println!("HOST_LIVE_DONE");
}
