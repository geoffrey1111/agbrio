//! Offline tests: a fake Desktop pipe and a scripted backend. No real Codex process or model.
use super::{
    hub::{host_request_id, parse_host_request_id, Config, Hub, TurnEvent, TurnTracker},
    ipc::{parse_version_table, versions_match, PeerPolicy, VALIDATED_VERSIONS},
    native_files::{Boundary, BoundaryKind},
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::ServerOptions,
    sync::mpsc,
};

const THREAD: &str = "11111111-1111-4111-8111-111111111111";

/// Answers like Codex for the few methods the routing needs, and logs every call.
const BACKEND_SCRIPT: &str = r#"
const fs=require('fs');const log=process.argv[2];const rollout=process.argv[3];let reads=0;const loaded=new Set(['new-thread']);
require('readline').createInterface({input:process.stdin}).on('line',line=>{
 const m=JSON.parse(line);if(m.id===9&&m.result){fs.appendFileSync(log,'answer:'+m.result.decision+'\n');return process.stdout.write(JSON.stringify({method:'serverRequest/resolved',params:{threadId:'asks',requestId:9}})+'\n');}if(m.id===undefined)return;fs.appendFileSync(log,m.method+'\n');
 const reply=result=>process.stdout.write(JSON.stringify({id:m.id,result})+'\n');
 if(m.method==='initialize')return reply({});
 if(m.method==='turn/start'&&m.params.threadId==='lost')return process.exit(1);
 if(m.method==='thread/read'&&m.params.threadId==='materializing'&&reads++<2)return process.stdout.write(JSON.stringify({id:m.id,error:{code:-32603,message:'failed to read thread: thread-store internal error: failed to read session metadata x: rollout at x is empty'}})+'\n');
 if(m.method==='thread/read'&&m.params.threadId==='never-written')return process.stdout.write(JSON.stringify({id:m.id,error:{code:-32603,message:'failed to read thread: rollout at x is empty'}})+'\n');
 if(m.method==='thread/read'&&m.params.threadId==='other-error')return process.stdout.write(JSON.stringify({id:m.id,error:{code:-32600,message:'no rollout found'}})+'\n');
 if(m.method==='thread/read')return reply({thread:{id:m.params.threadId,cwd:'C:/project',path:rollout,status:{type:'notLoaded'},read:++reads}});
 if(m.method==='thread/turns/list')return reply({data:[{id:'running-turn',status:'interrupted'}],nextCursor:null});
 if(m.method==='thread/start')return reply({thread:{id:'new-thread',cwd:'C:/project',path:rollout}});
 if(m.method==='thread/resume'&&m.params.threadId==='gone')return process.stdout.write(JSON.stringify({id:m.id,error:{code:-32600,message:'no rollout found'}})+'\n');
 if(m.method==='thread/resume'){loaded.add(m.params.threadId);return reply({thread:{id:m.params.threadId,cwd:'C:/project',path:rollout}});}
 if(m.method==='turn/start'&&!loaded.has(m.params.threadId))return process.stdout.write(JSON.stringify({id:m.id,error:{code:-32600,message:'thread not found: '+m.params.threadId}})+'\n');
 if(m.method==='turn/start'&&m.params.threadId==='asks'){reply({turn:{id:'backend-turn',status:'inProgress'}});return process.stdout.write(JSON.stringify({id:9,method:'item/commandExecution/requestApproval',params:{threadId:'asks',turnId:'backend-turn',itemId:'item'}})+'\n');}
 if(m.method==='turn/start'&&m.params.threadId==='finishes'){reply({turn:{id:'backend-turn',status:'inProgress'}});return process.stdout.write(JSON.stringify({method:'turn/completed',params:{threadId:'finishes',turn:{id:'backend-turn',status:'completed'}}})+'\n');}
 if(m.method==='turn/start')return reply({turn:{id:'backend-turn',status:'inProgress'}});
 return reply({ok:true});
});
"#;

struct Harness {
    hub: Arc<Hub>,
    outgoing: mpsc::UnboundedReceiver<Value>,
    pipe_log: Arc<Mutex<Vec<Value>>>,
    backend_log: PathBuf,
    rollout: PathBuf,
    home: tempfile::TempDir,
}

#[derive(Clone, Default)]
struct DesktopScript {
    owns: bool,
    state: Option<Value>,
    interrupted_turn: Option<String>,
    close_on_write: bool,
}

async fn fake_desktop(pipe: String, script: DesktopScript, log: Arc<Mutex<Vec<Value>>>) {
    let mut server = ServerOptions::new().first_pipe_instance(true).create(&pipe).unwrap();
    tokio::spawn(async move {
        loop {
            if server.connect().await.is_err() {
                return;
            }
            let current = server;
            server = ServerOptions::new().create(&pipe).unwrap();
            let log = log.clone();
            let script = script.clone();
            tokio::spawn(async move {
                let (mut read, mut write) = tokio::io::split(current);
                loop {
                    let Ok(length) = read.read_u32_le().await else { return };
                    let mut body = vec![0_u8; length as usize];
                    if read.read_exact(&mut body).await.is_err() {
                        return;
                    }
                    let message: Value = serde_json::from_slice(&body).unwrap();
                    log.lock().unwrap().push(message.clone());
                    let method = message["method"].as_str().unwrap_or_default().to_owned();
                    let request_id = message["requestId"].clone();
                    let mut frames = Vec::new();
                    match method.as_str() {
                        "initialize" => frames.push(json!({"type":"response","requestId":request_id,"resultType":"success","method":method,"result":{"clientId":"light-1"}})),
                        "thread-owner-discovery" if script.owns => frames.push(json!({"type":"response","requestId":request_id,"resultType":"success","method":method,"handledByClientId":"owner-1","result":{}})),
                        "thread-owner-discovery" => frames.push(json!({"type":"response","requestId":request_id,"resultType":"error","error":"no-client-found"})),
                        "thread-stream-following-changed" => {
                            if message.pointer("/params/following") == Some(&json!(true)) {
                                if let Some(state) = &script.state {
                                    frames.push(json!({"type":"broadcast","method":"thread-stream-state-changed","sourceClientId":"owner-1","version":11,"params":{"conversationId":THREAD,"hostId":"local","change":{"type":"snapshot","revision":1,"conversationState":state}}}));
                                }
                            }
                        }
                        m if m.starts_with("thread-follower-") => {
                            if script.close_on_write {
                                return;
                            }
                            let result = match m {
                                "thread-follower-interrupt-turn" => json!({"interruptedTurnId":script.interrupted_turn,"ok":true}),
                                _ => json!({"result":{"turn":{"id":"desktop-turn","status":"inProgress"}}}),
                            };
                            frames.push(json!({"type":"response","requestId":request_id,"resultType":"success","method":method,"handledByClientId":"owner-1","result":result}));
                        }
                        _ => {}
                    }
                    for frame in frames {
                        let body = frame.to_string().into_bytes();
                        if write.write_all(&(body.len() as u32).to_le_bytes()).await.is_err() || write.write_all(&body).await.is_err() {
                            return;
                        }
                    }
                }
            });
        }
    });
}

fn validated_versions() -> HashMap<String, u64> {
    VALIDATED_VERSIONS.iter().map(|(method, version)| ((*method).to_owned(), *version)).collect()
}

fn write_rollout(path: &Path, lines: &[Value]) {
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    std::fs::write(path, text).unwrap();
}

async fn harness(script: DesktopScript, versions: HashMap<String, u64>) -> Harness {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("thread-writer-locks")).unwrap();
    let rollout = home.path().join("rollout.jsonl");
    write_rollout(&rollout, &[json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"old-turn"}})]);
    let backend_log = home.path().join("backend.log");
    std::fs::write(&backend_log, "").unwrap();
    let script_path = home.path().join("backend.cjs");
    std::fs::write(&script_path, BACKEND_SCRIPT).unwrap();
    let pipe = format!(r"\\.\pipe\agbrio-light-test-{}", uuid::Uuid::new_v4());
    let pipe_log: Arc<Mutex<Vec<Value>>> = Arc::default();
    fake_desktop(pipe.clone(), script, pipe_log.clone()).await;
    let (out, outgoing) = mpsc::unbounded_channel();
    let (log, roll, source) = (backend_log.clone(), rollout.clone(), script_path);
    let config = Config {
        pipe,
        policy: PeerPolicy { verify: false, versions: Some(versions) },
        codex_home: home.path().to_path_buf(),
        backend_command: Box::new(move || {
            // The real launcher adds "app-server --stdio"; node ignores them after the script's own args.
            let mut command = Command::new("node");
            command.arg(&source).arg(&log).arg(&roll);
            command
        }),
        idle_grace: Duration::ZERO,
        release_after_turn: Duration::ZERO,
    };
    Harness { hub: Hub::new(config, out), outgoing, pipe_log, backend_log, rollout, home }
}

impl Harness {
    async fn call(&self, method: &str, params: Value) -> Option<Value> {
        self.hub.clone().handle_request(json!(7), method.to_owned(), params).await
    }
    fn backend_calls(&self, method: &str) -> usize {
        std::fs::read_to_string(&self.backend_log).unwrap().lines().filter(|line| *line == method).count()
    }
    fn pipe_calls(&self, method: &str) -> Vec<Value> {
        self.pipe_log.lock().unwrap().iter().filter(|message| message["method"] == method).cloned().collect()
    }
    fn hold_lock(&self, thread: &str) -> std::fs::File {
        let file = std::fs::File::create(self.home.path().join("thread-writer-locks").join(format!("{thread}.lock"))).unwrap();
        file.try_lock().unwrap();
        file
    }
    async fn next_where(&mut self, predicate: impl Fn(&Value) -> bool) -> Value {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let message = self.outgoing.recv().await.expect("transport closed");
                if predicate(&message) {
                    return message;
                }
            }
        })
        .await
        .expect("expected message never arrived")
    }
}

const READ: fn() -> Value = || json!({"threadId":THREAD,"includeTurns":false});

#[tokio::test]
async fn reads_stop_needing_a_backend_until_the_rollout_changes() {
    let t = harness(DesktopScript::default(), validated_versions()).await;
    let first = t.call("thread/read", READ()).await.unwrap();
    assert_eq!(first["id"], 7);
    assert_eq!(first["result"]["thread"]["cwd"], "C:/project");
    for _ in 0..3 {
        let again = t.call("thread/read", READ()).await.unwrap();
        assert_eq!(again["result"]["thread"]["read"], 1);
    }
    assert_eq!(t.backend_calls("thread/read"), 1);

    std::fs::write(&t.rollout, "{\"changed\":true}\n{\"more\":true}\n").unwrap();
    assert_eq!(t.call("thread/read", READ()).await.unwrap()["result"]["thread"]["read"], 2);
    assert_eq!(t.backend_calls("thread/read"), 2);
}

#[tokio::test]
async fn an_idle_backend_exits_and_comes_back_on_demand() {
    let t = harness(DesktopScript::default(), validated_versions()).await;
    t.call("model/list", json!({"limit":100})).await.unwrap();
    t.call("model/list", json!({"limit":100})).await.unwrap();
    assert_eq!(t.backend_calls("model/list"), 1, "catalog reads are served from the TTL cache");
    t.hub.reap().await;
    t.call("account/read", json!({})).await.unwrap();
    assert_eq!(t.backend_calls("initialize"), 2, "a reaped backend restarts only when needed");
}

#[tokio::test]
async fn a_new_thread_keeps_its_backend_until_the_pin_is_released() {
    let t = harness(DesktopScript::default(), validated_versions()).await;
    let created = t.call("thread/start", json!({"cwd":"C:/project"})).await.unwrap();
    assert_eq!(created["result"]["thread"]["id"], "new-thread");
    t.hub.reap().await;
    t.call("account/read", json!({})).await.unwrap();
    assert_eq!(t.backend_calls("initialize"), 1, "an empty thread only exists in its backend, so it must survive reaping");
    t.call("thread/unsubscribe", json!({"threadId":"new-thread"})).await.unwrap();
    t.hub.reap().await;
    t.call("config/read", json!({})).await.unwrap();
    assert_eq!(t.backend_calls("initialize"), 2);
}

#[tokio::test]
async fn desktop_status_comes_from_the_rollout_tail_only_while_owned() {
    let t = harness(DesktopScript { owns: true, ..Default::default() }, validated_versions()).await;
    assert_eq!(t.call("thread/read", READ()).await.unwrap()["result"]["thread"]["status"]["type"], "idle");
    write_rollout(&t.rollout, &[json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"running-turn"}})]);
    t.call("thread/read", READ()).await.unwrap();
    assert_eq!(t.call("thread/read", READ()).await.unwrap()["result"]["thread"]["status"]["type"], "active");
    let turns = t.call("thread/turns/list", json!({"threadId":THREAD,"limit":1,"sortDirection":"desc","cursor":null,"itemsView":"notLoaded"})).await.unwrap();
    assert_eq!(turns["result"]["data"][0]["status"], "inProgress", "a reader sees Desktop's running turn as interrupted");
}

#[tokio::test]
async fn an_open_boundary_without_an_owner_is_not_running() {
    let t = harness(DesktopScript::default(), validated_versions()).await;
    write_rollout(&t.rollout, &[json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"crashed"}})]);
    t.call("thread/read", READ()).await.unwrap();
    assert_eq!(t.call("thread/read", READ()).await.unwrap()["result"]["thread"]["status"]["type"], "notLoaded");
}

#[tokio::test]
async fn sends_to_a_desktop_thread_go_through_the_owner_and_never_the_backend() {
    let t = harness(DesktopScript { owns: true, ..Default::default() }, validated_versions()).await;
    let reply = t.call("turn/start", json!({"threadId":THREAD,"input":[{"type":"text","text":"hello"}]})).await.unwrap();
    assert_eq!(reply["result"]["turn"]["id"], "desktop-turn");
    assert_eq!(t.backend_calls("turn/start"), 0);
    let sent = t.pipe_calls("thread-follower-start-turn");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["targetClientId"], "owner-1");
    assert_eq!(sent[0]["version"], 2);
    let request = &sent[0]["params"]["turnStart"]["request"];
    assert_eq!(request["input"][0]["text_elements"], json!([]));
    assert!(request["clientUserMessageId"].is_string());
    assert_eq!(sent[0]["params"]["turnStart"]["context"]["inheritThreadSettings"], true);
}

#[tokio::test]
async fn a_thread_held_by_another_writer_is_refused_without_touching_anything() {
    let t = harness(DesktopScript::default(), validated_versions()).await;
    let _held = t.hold_lock(THREAD);
    for (method, params) in [("turn/start", json!({"threadId":THREAD,"input":[]})), ("thread/resume", json!({"threadId":THREAD})), ("thread/archive", json!({"threadId":THREAD}))] {
        let reply = t.call(method, params).await.unwrap();
        assert!(reply["error"]["message"].as_str().unwrap().starts_with("CODEX_LIGHT_THREAD_HELD_ELSEWHERE"), "{method}");
        assert_eq!(t.backend_calls(method), 0);
    }
}

#[tokio::test]
async fn a_free_thread_uses_the_backend() {
    let t = harness(DesktopScript::default(), validated_versions()).await;
    assert_eq!(t.call("thread/resume", json!({"threadId":THREAD})).await.unwrap()["result"]["thread"]["id"], THREAD);
    assert_eq!(t.call("turn/start", json!({"threadId":THREAD,"input":[]})).await.unwrap()["result"]["turn"]["id"], "backend-turn");
    assert_eq!(t.backend_calls("turn/start"), 1);
}

/// The Host still counts a thread as loaded after the backend let it go when its turn ended.
#[tokio::test]
async fn a_free_thread_the_host_did_not_resume_still_takes_the_message() {
    let t = harness(DesktopScript::default(), validated_versions()).await;
    assert_eq!(t.call("turn/start", json!({"threadId":THREAD,"input":[]})).await.unwrap()["result"]["turn"]["id"], "backend-turn");
    assert_eq!(t.backend_calls("thread/resume"), 1);
    assert_eq!(t.backend_calls("turn/start"), 1);
    assert_eq!(t.call("turn/start", json!({"threadId":THREAD,"input":[]})).await.unwrap()["result"]["turn"]["id"], "backend-turn");
    assert_eq!(t.backend_calls("thread/resume"), 1, "already loaded the second time");
}

#[tokio::test]
async fn a_failed_resume_is_a_refusal_and_sends_no_turn() {
    let t = harness(DesktopScript::default(), validated_versions()).await;
    let reply = t.call("turn/start", json!({"threadId":"gone","input":[]})).await.unwrap();
    assert_eq!(reply["error"]["message"], "no rollout found");
    assert_eq!(t.backend_calls("turn/start"), 0);
}

#[tokio::test]
async fn an_unvalidated_desktop_build_cannot_be_written_to() {
    let mut versions = validated_versions();
    versions.insert("thread-follower-start-turn".into(), 3);
    let t = harness(DesktopScript { owns: true, ..Default::default() }, versions).await;
    let reply = t.call("turn/start", json!({"threadId":THREAD,"input":[]})).await.unwrap();
    assert!(reply["error"]["message"].as_str().unwrap().starts_with("CODEX_LIGHT_DESKTOP_VERSION_UNSUPPORTED"));
    assert!(t.pipe_calls("thread-follower-start-turn").is_empty());
}

#[tokio::test]
async fn an_uncertain_desktop_write_gets_no_reply() {
    let t = harness(DesktopScript { owns: true, close_on_write: true, ..Default::default() }, validated_versions()).await;
    assert!(t.call("turn/start", json!({"threadId":THREAD,"input":[]})).await.is_none(), "the Host's timeout must record UNKNOWN");
    assert_eq!(t.pipe_calls("thread-follower-start-turn").len(), 1, "never resent");
}

#[tokio::test]
async fn a_backend_lost_during_a_write_gets_no_reply() {
    let t = harness(DesktopScript::default(), validated_versions()).await;
    assert!(t.call("turn/start", json!({"threadId":"lost","input":[]})).await.is_none());
    assert_eq!(t.backend_calls("turn/start"), 1, "never resent");
}

#[tokio::test]
async fn interrupt_succeeds_only_when_that_exact_turn_stopped() {
    let stale = harness(DesktopScript { owns: true, interrupted_turn: None, ..Default::default() }, validated_versions()).await;
    let reply = stale.call("turn/interrupt", json!({"threadId":THREAD,"turnId":"old"})).await.unwrap();
    assert_eq!(reply["error"]["message"], "CODEX_LIGHT_INTERRUPT_TURN_NOT_RUNNING");

    let exact = harness(DesktopScript { owns: true, interrupted_turn: Some("live".into()), ..Default::default() }, validated_versions()).await;
    assert!(exact.call("turn/interrupt", json!({"threadId":THREAD,"turnId":"live"})).await.unwrap().get("result").is_some());
    let sent = exact.pipe_calls("thread-follower-interrupt-turn");
    assert_eq!((sent[0]["version"].as_u64(), sent[0]["params"]["expectedTurnId"].as_str()), (Some(4), Some("live")));
}

#[tokio::test]
async fn a_stale_steer_is_refused_and_a_current_one_carries_restore_message() {
    let t = harness(DesktopScript { owns: true, ..Default::default() }, validated_versions()).await;
    t.call("thread/read", READ()).await.unwrap();
    write_rollout(&t.rollout, &[json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"live"}})]);
    let stale = t.call("turn/steer", json!({"threadId":THREAD,"expectedTurnId":"old","input":[{"type":"text","text":"x"}]})).await.unwrap();
    assert!(stale["error"]["message"].as_str().unwrap().starts_with("CODEX_LIGHT_STEER_TURN_MISMATCH"));
    assert!(t.pipe_calls("thread-follower-steer-turn").is_empty());

    let reply = t.call("turn/steer", json!({"threadId":THREAD,"expectedTurnId":"live","input":[{"type":"text","text":"go"}]})).await.unwrap();
    assert!(reply.get("result").is_some(), "{reply}");
    let params = &t.pipe_calls("thread-follower-steer-turn")[0]["params"];
    assert_eq!(params["restoreMessage"]["cwd"], "C:/project");
    assert_eq!(params["restoreMessage"]["context"]["workspaceRoots"], json!(["C:/project"]));
    assert_eq!(params["restoreMessage"]["id"], params["clientUserMessageId"]);
}

#[tokio::test]
async fn goal_writes_and_unknown_methods_are_refused() {
    let t = harness(DesktopScript { owns: true, ..Default::default() }, validated_versions()).await;
    for method in ["thread/goal/set", "thread/goal/clear"] {
        assert_eq!(t.call(method, json!({"threadId":THREAD})).await.unwrap()["error"]["message"], "CODEX_LIGHT_GOAL_WRITE_UNSUPPORTED");
    }
    assert_eq!(t.call("command/exec", json!({})).await.unwrap()["error"]["message"], "CODEX_LIGHT_METHOD_UNSUPPORTED");
    assert_eq!(t.backend_calls("command/exec"), 0);
}

#[tokio::test]
async fn a_native_request_reaches_the_host_and_its_answer_reaches_the_owner() {
    let state = json!({
        "threadRuntimeStatus":{"type":"active","activeFlags":["waitingOnApproval"]},
        "requests":[{"id":42,"method":"item/commandExecution/requestApproval","params":{"threadId":THREAD,"turnId":"live"}}]
    });
    let mut t = harness(DesktopScript { owns: true, state: Some(state), ..Default::default() }, validated_versions()).await;
    let resumed = t.call("thread/resume", json!({"threadId":THREAD,"excludeTurns":true})).await.unwrap();
    assert_eq!(resumed["result"]["thread"]["id"], THREAD);
    assert_eq!(t.backend_calls("thread/resume"), 0, "Desktop already holds it; a second backend must not load it");

    let request = t.next_where(|m| m["method"] == "item/commandExecution/requestApproval").await;
    assert_eq!(request["params"]["threadId"], THREAD);
    t.hub.handle_response(&request["id"], &json!({"decision":"accept"})).await;
    let answered = t.pipe_calls("thread-follower-command-approval-decision");
    assert_eq!(answered.len(), 1);
    assert_eq!(answered[0]["params"]["requestId"], 42);
    assert_eq!(answered[0]["params"]["decision"], "accept");
}

#[test]
fn host_request_ids_round_trip_string_and_number_ids() {
    for raw in [json!(42), json!("abc:def")] {
        let wire = host_request_id(THREAD, "item/tool/requestUserInput", &raw);
        assert_eq!(parse_host_request_id(wire.as_str().unwrap()), Some((THREAD.to_owned(), "item/tool/requestUserInput".to_owned(), raw)));
    }
    assert_eq!(parse_host_request_id("backend:7"), None);
}

#[test]
fn the_version_table_is_read_from_bundle_text_and_compared_strictly() {
    let body: String = VALIDATED_VERSIONS.iter().map(|(method, version)| format!("\"{method}\":{version}")).collect::<Vec<_>>().join(",");
    let bundle = format!("var x=1;const t={{\"a\":1,{body},\"z\":2}};var y=2;");
    let table = parse_version_table(&bundle).unwrap();
    assert!(versions_match(&table));
    let mut changed = table.clone();
    changed.insert("thread-follower-steer-turn".into(), 2);
    assert!(!versions_match(&changed));
    changed = table;
    changed.remove("thread-owner-discovery");
    assert!(!versions_match(&changed));
    assert_eq!(parse_version_table("no table here"), None);
}

fn boundary(kind: BoundaryKind, turn: &str) -> Option<Boundary> {
    Some(Boundary { kind, turn_id: Some(turn.to_owned()) })
}

#[test]
fn a_turn_stopped_before_any_snapshot_saw_it_running_still_ends() {
    let mut t = TurnTracker::default();
    assert_eq!(t.observe(false, None, boundary(BoundaryKind::Completed, "old").as_ref()), TurnEvent::None);
    // Sent and interrupted between snapshots: the only trace is the send reply.
    let event = t.observe(false, Some("sent".into()), boundary(BoundaryKind::Aborted, "sent").as_ref());
    assert_eq!(event, TurnEvent::Ended { turn: "sent".into(), unseen: true, stale: false });
    assert_eq!(t.observe(false, None, boundary(BoundaryKind::Aborted, "sent").as_ref()), TurnEvent::None, "reported once");
}

#[test]
fn a_sent_turn_announces_its_start_when_it_is_seen_running() {
    let mut t = TurnTracker::default();
    t.observe(false, None, boundary(BoundaryKind::Completed, "old").as_ref());
    let running = boundary(BoundaryKind::Started, "sent");
    assert_eq!(t.observe(true, Some("sent".into()), running.as_ref()), TurnEvent::Started("sent".into()));
    assert_eq!(t.observe(true, None, running.as_ref()), TurnEvent::None);
    assert_eq!(t.observe(false, None, boundary(BoundaryKind::Completed, "sent").as_ref()), TurnEvent::Ended { turn: "sent".into(), unseen: false, stale: false });
}

#[test]
fn a_late_end_record_waits_then_counts_as_stale() {
    let mut t = TurnTracker::default();
    t.observe(false, None, boundary(BoundaryKind::Completed, "old").as_ref());
    let open = boundary(BoundaryKind::Started, "sent");
    assert_eq!(t.observe(false, Some("sent".into()), open.as_ref()), TurnEvent::None);
    assert!(t.settling());
    for _ in 0..4 {
        assert_eq!(t.observe(false, None, open.as_ref()), TurnEvent::None);
    }
    assert_eq!(t.observe(false, None, open.as_ref()), TurnEvent::Ended { turn: "sent".into(), unseen: true, stale: true });
    assert!(!t.settling());
}

#[test]
fn the_end_record_arriving_during_settling_is_a_normal_end() {
    let mut t = TurnTracker::default();
    t.observe(false, None, boundary(BoundaryKind::Completed, "old").as_ref());
    assert_eq!(t.observe(false, Some("sent".into()), boundary(BoundaryKind::Started, "sent").as_ref()), TurnEvent::None);
    assert_eq!(t.observe(false, None, boundary(BoundaryKind::Completed, "sent").as_ref()), TurnEvent::Ended { turn: "sent".into(), unseen: true, stale: false });
}

#[test]
fn a_desktop_turn_between_snapshots_is_reported_with_its_start() {
    let mut t = TurnTracker::default();
    t.observe(false, None, boundary(BoundaryKind::Completed, "old").as_ref());
    assert_eq!(t.observe(false, None, boundary(BoundaryKind::Completed, "quick").as_ref()), TurnEvent::Ended { turn: "quick".into(), unseen: true, stale: false });
}

#[test]
fn history_before_the_follow_is_quiet_and_a_leftover_open_record_is_stale() {
    let mut t = TurnTracker::default();
    assert_eq!(t.observe(false, None, boundary(BoundaryKind::Completed, "old").as_ref()), TurnEvent::None);
    let mut crashed = TurnTracker::default();
    assert_eq!(crashed.observe(false, None, boundary(BoundaryKind::Started, "dead").as_ref()), TurnEvent::Ended { turn: "dead".into(), unseen: false, stale: true });
}

#[test]
fn a_newer_running_turn_closes_the_older_one_first() {
    let mut t = TurnTracker::default();
    assert_eq!(t.observe(true, None, boundary(BoundaryKind::Started, "first").as_ref()), TurnEvent::Started("first".into()));
    assert_eq!(t.observe(true, None, boundary(BoundaryKind::Started, "second").as_ref()), TurnEvent::Ended { turn: "first".into(), unseen: false, stale: false });
    assert_eq!(t.observe(true, None, boundary(BoundaryKind::Started, "second").as_ref()), TurnEvent::Started("second".into()));
}

#[tokio::test]
async fn a_recorded_interruption_is_reported_once_and_only_with_its_own_abort_record() {
    let mut t = harness(DesktopScript::default(), validated_versions()).await;
    let list = || json!({"threadId":THREAD,"limit":1,"sortDirection":"desc","cursor":null,"itemsView":"notLoaded"});
    t.call("thread/read", READ()).await.unwrap();
    t.call("thread/turns/list", list()).await.unwrap();
    assert!(t.outgoing.try_recv().is_err(), "the newest record does not belong to this turn");

    write_rollout(&t.rollout, &[json!({"type":"event_msg","payload":{"type":"turn_aborted","turn_id":"running-turn"}})]);
    t.call("thread/read", READ()).await.unwrap();
    t.call("thread/turns/list", list()).await.unwrap();
    let event = t.next_where(|m| m["method"] == "turn/completed").await;
    assert_eq!(event["params"]["turn"], json!({"id":"running-turn","status":"interrupted"}));
    t.call("thread/turns/list", list()).await.unwrap();
    assert!(t.outgoing.try_recv().is_err(), "reported once");
}

/// What a process killed outright leaves behind: a started turn and no end record.
#[tokio::test]
async fn a_turn_with_no_end_record_counts_as_interrupted_only_once_no_writer_holds_the_thread() {
    let mut t = harness(DesktopScript::default(), validated_versions()).await;
    let list = || json!({"threadId":THREAD,"limit":1,"sortDirection":"desc","cursor":null,"itemsView":"notLoaded"});
    write_rollout(&t.rollout, &[json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"running-turn"}})]);
    t.call("thread/read", READ()).await.unwrap();
    let held = t.hold_lock(THREAD);
    t.call("thread/turns/list", list()).await.unwrap();
    assert!(t.outgoing.try_recv().is_err(), "a writer may still be running it");
    drop(held);
    t.call("thread/turns/list", list()).await.unwrap();
    let event = t.next_where(|m| m["method"] == "turn/completed").await;
    assert_eq!(event["params"]["turn"], json!({"id":"running-turn","status":"interrupted"}));
    t.call("thread/turns/list", list()).await.unwrap();
    assert!(t.outgoing.try_recv().is_err(), "reported once");
}

#[tokio::test]
async fn a_rollout_that_is_still_empty_is_read_again_and_other_errors_are_not() {
    let t = harness(DesktopScript::default(), validated_versions()).await;
    let ready = t.call("thread/read", json!({"threadId":"materializing","includeTurns":false})).await.unwrap();
    assert_eq!(ready["result"]["thread"]["id"], "materializing");
    assert_eq!(t.backend_calls("thread/read"), 3);

    let other = t.call("thread/read", json!({"threadId":"other-error","includeTurns":false})).await.unwrap();
    assert_eq!(other["error"]["message"], "no rollout found");
    assert_eq!(t.backend_calls("thread/read"), 4, "a real error is returned at once");

    let never = t.call("thread/read", json!({"threadId":"never-written","includeTurns":false})).await.unwrap();
    assert!(never["error"]["message"].as_str().unwrap().ends_with("is empty"), "the wait is bounded and the native error is kept");
    assert_eq!(t.backend_calls("thread/read"), 15);
}

#[tokio::test]
async fn a_finished_turn_frees_its_thread_for_desktop_at_once() {
    let mut t = harness(DesktopScript::default(), validated_versions()).await;
    t.call("thread/resume", json!({"threadId":"finishes"})).await.unwrap();
    t.hub.reap().await;
    t.call("config/read", json!({})).await.unwrap();
    assert_eq!(t.backend_calls("initialize"), 1, "a resume waits for the send it was made for");

    t.call("turn/start", json!({"threadId":"finishes","input":[]})).await.unwrap();
    t.next_where(|m| m["method"] == "turn/completed").await;
    t.hub.reap().await;
    t.call("account/read", json!({})).await.unwrap();
    assert_eq!(t.backend_calls("initialize"), 2, "the backend exited, which is what frees the writer lock");
}

#[tokio::test]
async fn a_running_turn_keeps_its_thread() {
    let t = harness(DesktopScript::default(), validated_versions()).await;
    t.call("thread/resume", json!({"threadId":THREAD})).await.unwrap();
    t.call("turn/start", json!({"threadId":THREAD,"input":[]})).await.unwrap();
    t.hub.reap().await;
    t.call("account/read", json!({})).await.unwrap();
    assert_eq!(t.backend_calls("initialize"), 1);
}

#[tokio::test]
async fn a_backend_request_is_answered_and_resolved_under_the_id_the_host_was_shown() {
    let mut t = harness(DesktopScript::default(), validated_versions()).await;
    t.call("thread/resume", json!({"threadId":"asks"})).await.unwrap();
    t.call("turn/start", json!({"threadId":"asks","input":[]})).await.unwrap();
    let request = t.next_where(|m| m["method"] == "item/commandExecution/requestApproval").await;
    assert_eq!(request["id"], "backend:9");
    t.hub.handle_response(&request["id"], &json!({"decision":"accept"})).await;
    let resolved = t.next_where(|m| m["method"] == "serverRequest/resolved").await;
    assert_eq!(resolved["params"]["requestId"], "backend:9", "otherwise the Host's card never clears");
    assert_eq!(t.backend_calls("answer:accept"), 1);
}

#[tokio::test]
async fn resuming_a_running_thread_does_not_forget_its_turn() {
    let t = harness(DesktopScript::default(), validated_versions()).await;
    t.call("thread/resume", json!({"threadId":THREAD})).await.unwrap();
    t.call("turn/start", json!({"threadId":THREAD,"input":[]})).await.unwrap();
    t.call("thread/resume", json!({"threadId":THREAD})).await.unwrap();
    t.hub.reap().await;
    t.call("account/read", json!({})).await.unwrap();
    assert_eq!(t.backend_calls("initialize"), 1, "the turn is still running, so the backend must stay");
}
