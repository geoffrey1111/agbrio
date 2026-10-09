use super::*;
use crate::codex::adapter::CodexAdapter;
use crate::host_application::{exact_codex_completed_event_result, start_turn};
use serde_json::{json, Value};

/// Executes the real Host/store/adapter against a bounded fictional RPC peer.
/// Every physical method is recorded; no production provider or model is used.
fn active_guidance_fixture(mode:&str,run:Option<(&str,Option<&str>)>)->(tempfile::TempDir,RouterCore,String,String){
    let dir=tempfile::tempdir().unwrap();
    let store=Arc::new(crate::RouterStore::open_at(dir.path().join("router.db")).unwrap());
    let p=store.create_project("fictional guidance".into(),None).unwrap();let w=store.create_workstream(&p.id,"fixture".into()).unwrap();
    let side=|id:&str|RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:"fixture".into(),cwd:Some(dir.path().to_string_lossy().into())};
    let binding=store.bind_role_bridge(&w.id,0,side("source-fixture"),side("target-fixture")).unwrap();
    let source=store.record_reply_observation(&w.id,&binding.decision.unwrap().endpoint.id,Some("codex:source-turn:source-message"),"exact supplemental instruction",None).unwrap().unwrap();
    let h=store.prepare_role_handoff(&w.id,"DECISION",&source.id,"exact supplemental instruction").unwrap();store.approve_role_handoff_checked(&h.id,&h.payload_hash).unwrap();
    if let Some((status,turn))=run{store.create_provider_run(&w.id,&binding.execution.unwrap().endpoint.id,"CODEX",None,turn,status).unwrap();}
    let script=r#"const fs=require('node:fs'),rl=require('node:readline').createInterface({input:process.stdin});const root=process.argv[1],initialMode=process.argv[2];rl.on('line',line=>{const mode=fs.existsSync(root+'/metadata-fail.flag')?'METADATA_ERROR':initialMode;const r=JSON.parse(line);if(r.id===undefined)return;fs.appendFileSync(root+'/rpc.jsonl',JSON.stringify(r)+'\n');let v={};if(r.method==='thread/read'&&mode==='METADATA_ERROR'){process.stdout.write(JSON.stringify({id:r.id,error:{code:-32602,message:'INVALID_ARGUMENT'}})+'\n');return;}if(r.method==='thread/read')v={thread:{id:mode==='WRONG_THREAD'?'other':r.params.threadId,cwd:root,status:{type:mode==='IDLE'?'idle':'active'}}};else if(r.method==='thread/goal/get')v={goal:{threadId:r.params.threadId,status:'active',objective:'fictional',tokenBudget:null}};else if(r.method==='thread/turns/list')v={data:mode==='NO_TURN'?[]:[{id:'active-turn',status:mode==='TERMINAL'?'completed':'inProgress'}],nextCursor:null};else if(r.method==='turn/steer'){if(r.params.threadId!=='target-fixture'||r.params.expectedTurnId!=='active-turn'||r.params.input[0].text!=='exact supplemental instruction')throw Error('Wrong physical input');v={turnId:mode==='WRONG_ACK'?'other-turn':'active-turn'};}else if(r.method==='turn/start')v={turn:{id:'new-turn'}};else if(r.method!=='initialize'){process.stdout.write(JSON.stringify({id:r.id,error:{code:-32600,message:'Unexpected operation '+r.method}})+'\n');return;}process.stdout.write(JSON.stringify({id:r.id,result:v})+'\n');});"#;
    let mut cmd=std::process::Command::new("node");cmd.args(["-e",script]).arg(dir.path()).arg(mode);
    let mut adapter=CodexAdapter::start_shared_validation(cmd,Arc::new(|_|{})).unwrap();adapter.initialize().unwrap();
    let core=RouterCore{store,chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session{adapter:Some(adapter),..Default::default()})),completed_chatgpt_responses:Arc::default()};
    (dir,core,w.id,h.id)
}
fn guidance_methods(dir:&tempfile::TempDir)->Vec<Value>{fs::read_to_string(dir.path().join("rpc.jsonl")).unwrap().lines().map(|line|serde_json::from_str(line).unwrap()).collect()}
#[test]
fn assistant_confirm_failure_has_a_persisted_stage_and_receipt_without_send(){
    for mode in ["METADATA_ERROR","DISCONNECTED"] {
        let(dir,core,w,_)=active_guidance_fixture("IDLE",None);
        let g=core.store.connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput{label:"fixture assistant".into(),expires_at:std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64+600000}).unwrap();
        let b=core.store.role_bridge(&w).unwrap();let obs=core.store.reply_observations_for_workstream(&w).unwrap().remove(0);
        let h=crate::assistant_mcp::call(&core,&g.id,"agbrio_prepare_handoff",json!({"workstreamId":w,"bindingRevision":b.binding_revision,"requestId":"diagnostic-one","observationId":obs.id,"role":"DECISION","text":"exact supplemental instruction","attachmentIds":[]})).unwrap();
        if mode=="DISCONNECTED"{core.session.lock().unwrap().adapter.take();}else{fs::write(dir.path().join("metadata-fail.flag"),"fail only after prepare").unwrap();}
        let failed=crate::assistant_mcp::rpc(&core,&g.id,json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"agbrio_confirm_and_send","arguments":{"handoffId":h["id"],"expectedHash":h["payloadHash"],"ruleId":null,"decisionId":null,"assessment":"Within the exact owner-authorized fictional task."}}}));
        let expected=if mode=="DISCONNECTED"{"BRIDGE_PRECLAIM_ADAPTER"}else{"BRIDGE_PRECLAIM_TARGET_METADATA"};
        assert_eq!(failed["result"]["isError"],true);assert_eq!(failed["result"]["structuredContent"]["error_code"],expected);
        let receipt=crate::assistant_mcp::call(&core,&g.id,"agbrio_receipt",json!({"handoffId":h["id"],"expectedHash":h["payloadHash"]})).unwrap();
        assert_eq!(receipt["handoff"]["status"],"APPROVED");assert_eq!(receipt["handoff"]["errorCode"],expected);assert_eq!(receipt["lastPreclaimFailure"]["physicalSendStarted"],false);assert!(receipt["lastPreclaimFailure"]["cause"].is_string());
        let stranger=core.store.connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput{label:"other fixture client".into(),expires_at:std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64+600000}).unwrap();
        assert!(crate::assistant_mcp::call(&core,&stranger.id,"agbrio_confirm_and_send",json!({"handoffId":h["id"],"expectedHash":h["payloadHash"],"ruleId":null,"decisionId":null,"assessment":"Not the draft owner."})).is_err());
        assert_eq!(core.store.role_handoff(h["id"].as_str().unwrap()).unwrap().error_code.as_deref(),Some(expected));
        assert!(!guidance_methods(&dir).iter().any(|r|matches!(r["method"].as_str(),Some("turn/start"|"turn/steer"))));
    }
}
#[test]
fn assistant_reviewed_handoff_roundtrip_sends_exactly_once(){
    let(dir,core,w,_)=active_guidance_fixture("IDLE",None);
    let g=core.store.connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput{label:"fixture assistant".into(),expires_at:std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64+600000}).unwrap();
    let b=core.store.role_bridge(&w).unwrap();let obs=core.store.reply_observations_for_workstream(&w).unwrap().remove(0);
    let h=crate::assistant_mcp::call(&core,&g.id,"agbrio_prepare_handoff",json!({"workstreamId":w,"bindingRevision":b.binding_revision,"requestId":"one-roundtrip","observationId":obs.id,"role":"DECISION","text":"exact supplemental instruction","attachmentIds":[]})).unwrap();
    let args=json!({"handoffId":h["id"],"expectedHash":h["payloadHash"],"ruleId":null,"decisionId":null,"assessment":"Reviewed fictional task and exact recipient."});
    assert_eq!(crate::assistant_mcp::call(&core,&g.id,"agbrio_confirm_and_send",args.clone()).unwrap()["status"],"SENT");
    assert!(crate::assistant_mcp::call(&core,&g.id,"agbrio_confirm_and_send",args).is_err());
    let receipt=crate::assistant_mcp::call(&core,&g.id,"agbrio_receipt",json!({"handoffId":h["id"],"expectedHash":h["payloadHash"]})).unwrap();assert_eq!(receipt["handoff"]["status"],"SENT");assert!(receipt["handoff"]["errorCode"].is_null());
    assert_eq!(guidance_methods(&dir).iter().filter(|r|r["method"]=="turn/start").count(),1);
}
#[test]
fn active_goal_bridge_guidance_steers_once_without_resume_pause_or_new_execution_owner(){
    let(dir,core,w,h)=active_guidance_fixture("ACTIVE",None);
    assert_eq!(send(&core,&h).unwrap().handoffs[0].status,"SENT");assert!(send(&core,&h).is_err());
    let requests=guidance_methods(&dir);let writes:Vec<_>=requests.iter().filter(|r|r["method"]=="turn/steer").collect();assert_eq!(writes.len(),1);assert_eq!(writes[0]["params"]["clientUserMessageId"],h);
    assert!(!requests.iter().any(|r|matches!(r["method"].as_str(),Some("turn/start"|"thread/resume"|"turn/interrupt"|"thread/goal/set"|"thread/goal/clear"))));
    assert!(core.store.provider_runs_for_workstream(&w).unwrap().is_empty());assert!(core.session.lock().unwrap().adapter.as_ref().unwrap().active_turn_id("target-fixture").is_none());
}
#[test]
fn guidance_can_share_one_exact_known_running_provider_run_without_creating_another(){
    let(dir,core,w,h)=active_guidance_fixture("ACTIVE",Some(("RUNNING",Some("active-turn"))));
    assert!(send(&core,&h).is_ok());let runs=core.store.provider_runs_for_workstream(&w).unwrap();assert_eq!(runs.len(),1);assert_eq!(runs[0].external_run_id.as_deref(),Some("active-turn"));assert_eq!(runs[0].status,"RUNNING");
    assert_eq!(guidance_methods(&dir).iter().filter(|r|r["method"]=="turn/steer").count(),1);
}
#[test]
fn guidance_keeps_unknown_starting_missing_and_different_turn_writers_blocked(){
    for(status,turn)in[("STARTING",Some("active-turn")),("RUNNING",None),("RUNNING",Some("different-turn"))]{let(dir,core,_w,h)=active_guidance_fixture("ACTIVE",Some((status,turn)));assert!(send(&core,&h).is_err());assert_eq!(core.store.role_handoff(&h).unwrap().status,"APPROVED");assert!(!guidance_methods(&dir).iter().any(|r|r["method"]=="turn/steer"));}
}
#[test]
fn guidance_requires_an_exact_current_nonterminal_turn_before_claim(){
    for mode in ["NO_TURN","TERMINAL","WRONG_THREAD"]{let(dir,core,_w,h)=active_guidance_fixture(mode,None);assert!(send(&core,&h).is_err());assert_eq!(core.store.role_handoff(&h).unwrap().status,"APPROVED");assert!(!guidance_methods(&dir).iter().any(|r|r["method"]=="turn/steer"||r["method"]=="turn/start"));}
}
#[test]
fn guidance_wrong_ack_is_uncertain_and_never_replayed_or_falls_back_to_start(){
    let(dir,core,w,h)=active_guidance_fixture("WRONG_ACK",None);assert!(send(&core,&h).is_err());assert!(send(&core,&h).is_err());assert_eq!(core.store.role_handoff(&h).unwrap().status,"SENDING");assert!(core.store.provider_runs_for_workstream(&w).unwrap().is_empty());let requests=guidance_methods(&dir);assert_eq!(requests.iter().filter(|r|r["method"]=="turn/steer").count(),1);assert!(!requests.iter().any(|r|r["method"]=="turn/start"));
}
#[test]
fn guidance_idle_loaded_shared_goal_uses_original_start_without_pausing_goal(){
    let(dir,core,w,h)=active_guidance_fixture("IDLE",None);assert!(send(&core,&h).is_ok());let requests=guidance_methods(&dir);assert_eq!(requests.iter().filter(|r|r["method"]=="turn/start").count(),1);assert!(!requests.iter().any(|r|r["method"]=="turn/steer"||r["method"]=="thread/goal/set"));assert_eq!(core.store.provider_runs_for_workstream(&w).unwrap().len(),1);
}

#[test]
#[ignore="Explicit isolated official native WS + localhost model; never production threads"]
fn actual_native_active_goal_bridge_guidance(){
    assert_eq!(std::env::var("AIWR_LOCAL_MODEL_ONLY").as_deref(),Ok("1"));
    let root=std::path::PathBuf::from(std::env::var("AIWR_GUIDANCE_ROOT").unwrap());let thread=std::env::var("AIWR_GUIDANCE_THREAD").unwrap();
    let store=Arc::new(crate::RouterStore::open_at(root.join("native-guidance.db")).unwrap());let p=store.create_project("fictional native guidance".into(),None).unwrap();let w=store.create_workstream(&p.id,"fixture".into()).unwrap();
    let side=|id:&str|RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:"fictional".into(),cwd:Some(root.to_string_lossy().into())};let binding=store.bind_role_bridge(&w.id,0,side("unused-source-fixture"),side(&thread)).unwrap();
    let obs=store.record_reply_observation(&w.id,&binding.decision.unwrap().endpoint.id,Some("fictional-native-source"),"FICTIONAL_SUPPLEMENTAL_GUIDANCE",None).unwrap().unwrap();let h=store.prepare_role_handoff(&w.id,"DECISION",&obs.id,"FICTIONAL_SUPPLEMENTAL_GUIDANCE").unwrap();
    let mut cmd=std::process::Command::new(std::env::var("AIWR_GUIDANCE_NODE").unwrap());cmd.arg(root.join("transport.mjs")).arg(std::env::var("AIWR_GUIDANCE_URL").unwrap());let mut adapter=CodexAdapter::start_shared_validation(cmd,Arc::new(|_|{})).unwrap();adapter.initialize().unwrap();
    let before=adapter.get_goal(&thread).unwrap();assert_eq!(before["goal"]["status"],"active");
    let native=metadata(&mut adapter,&thread).unwrap();assert_eq!(native["thread"]["status"]["type"],"active");let expected=exact_active_turn(&mut adapter,&thread).unwrap();
    let core=RouterCore{store:store.clone(),chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session{adapter:Some(adapter),..Default::default()})),completed_chatgpt_responses:Arc::default()};
    let grant=store.connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput{label:"isolated native MCP QA".into(),expires_at:std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64+600000}).unwrap();store.register_assistant_prepare(&grant.id,&h.id,&obs.id,"native-mcp-request","native-only-test").unwrap();let result=crate::assistant_mcp::call(&core,&grant.id,"agbrio_confirm_and_send",json!({"handoffId":h.id,"expectedHash":h.payload_hash,"ruleId":null,"decisionId":null,"assessment":"Exact fictional instruction under isolated owner-authorized validation."})).unwrap();assert_eq!(result["status"],"SENT");assert!(send(&core,&h.id).is_err());assert!(store.provider_runs_for_workstream(&w.id).unwrap().is_empty());
    let mut session=core.session.lock().unwrap();let adapter=session.adapter.as_mut().unwrap();assert!(adapter.active_turn_id(&thread).is_none());let after=adapter.get_goal(&thread).unwrap();assert_eq!(after["goal"]["status"],"active");assert_eq!(before["goal"]["objective"],after["goal"]["objective"]);
    fs::write(root.join("host-native-proof.json"),serde_json::to_vec_pretty(&json!({"result":"AUTOMATED_VALIDATION_PASS","nativeActiveGoal":true,"sameActiveTurn":expected,"handoffId":h.id,"receipt":"SENT","duplicateRejected":true,"throughAssistantMcp":true,"goalStatusRetained":true,"goalObjectiveRetained":true,"externalExecutionStillExternal":true,"newProviderRuns":0,"ownerThreadsTouched":false})).unwrap()).unwrap();
}

#[test]
fn target_goal_is_exact_and_active_or_blocked_never_authorizes_delivery(){
    assert!(require_idle_goal("thread-a",&json!({"goal":null})).is_ok());
    for status in ["active","blocked"]{assert!(require_idle_goal("thread-a",&json!({"goal":{"threadId":"thread-a","objective":"original","status":status,"tokenBudget":null}})).is_err());}
    for status in ["paused","complete"]{assert!(require_idle_goal("thread-a",&json!({"goal":{"threadId":"thread-a","objective":"original","status":status,"tokenBudget":null}})).is_ok());}
    assert!(require_idle_goal("thread-a",&json!({"goal":{"threadId":"thread-b","objective":"other","status":"paused"}})).is_err());
}
use std::{
    collections::HashMap,
    fs,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[test]
#[ignore = "one explicit three-turn minimum-cost real cross-project role/file/return gate"]
fn real_cross_project_codex_bridge() {
    assert_eq!(
        std::env::var("AIWR_REAL_ROLE_BRIDGE_GATE").as_deref(),
        Ok("1")
    );
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("runtime/integrated-provider-router");
    let existing = std::env::var("AIWR_ROLE_EXISTING_CASE")
        .ok()
        .map(std::path::PathBuf::from);
    let evidence = existing
        .clone()
        .unwrap_or_else(|| root.join(format!("role-real-{}", uuid::Uuid::new_v4())));
    if existing.is_none() {
        fs::create_dir(&evidence).unwrap();
    } else {
        assert_eq!(evidence.parent(), Some(root.as_path()));
    }
    let old_case = existing.as_ref().map(|_| {
        serde_json::from_slice::<Value>(&fs::read(evidence.join("case.json")).unwrap()).unwrap()
    });
    fs::write(
        root.join("current-role-real-case.txt"),
        evidence.to_string_lossy().as_bytes(),
    )
    .unwrap();
    let decision_root = evidence.join("decision-project");
    let execution_root = evidence.join("execution-project");
    if existing.is_none() {
        fs::create_dir(&decision_root).unwrap();
        fs::create_dir(&execution_root).unwrap();
    }
    let store = Arc::new(crate::RouterStore::open_at(evidence.join("router.db")).unwrap());
    let events: Arc<Mutex<HashMap<String, (String, String, String)>>> = Arc::default();
    let captured = events.clone();
    let listener = Arc::new(move |event: &Value| {
        if event["method"] != "turn/completed" {
            return;
        }
        let Some(turn) = event.pointer("/params/turn/id").and_then(Value::as_str) else {
            return;
        };
        let Some(thread) = event
            .pointer("/params/threadId")
            .or_else(|| event.pointer("/params/turn/threadId"))
            .and_then(Value::as_str)
        else {
            return;
        };
        if let Ok((item, text)) = exact_codex_completed_event_result(thread, turn, event) {
            captured
                .lock()
                .unwrap()
                .insert(turn.into(), (thread.into(), item, text));
        }
    });
    let mut adapter = CodexAdapter::start_validation(listener).unwrap();
    adapter.initialize().unwrap();
    let models = adapter
        .request("model/list", json!({"limit":100,"includeHidden":false}))
        .unwrap();
    assert!(models["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["model"] == "gpt-6-luna"
            && m["supportedReasoningEfforts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["reasoningEffort"] == "low")));
    let mut ids = Vec::new();
    if let Some(old) = &old_case {
        let id = old["decisionThread"].as_str().unwrap();
        let resumed=adapter.request("thread/resume",json!({"threadId":id,"excludeTurns":true,"model":"gpt-6-luna","config":{"model_reasoning_effort":"low"}})).unwrap();
        assert_eq!(resumed["model"], "gpt-6-luna");
        assert_eq!(resumed["reasoningEffort"], "low");
        ids.push(id.into());
    }
    for cwd in if old_case.is_some() {
        vec![&execution_root]
    } else {
        vec![&decision_root, &execution_root]
    } {
        let started=adapter.request("thread/start",json!({"cwd":cwd,"ephemeral":false,"model":"gpt-6-luna","config":{"model_reasoning_effort":"low"},"approvalPolicy":"never","sandbox":"workspace-write"})).unwrap();
        assert_eq!(started["model"], "gpt-6-luna");
        assert_eq!(started["reasoningEffort"], "low");
        ids.push(started["thread"]["id"].as_str().unwrap().to_owned());
    }
    let core = RouterCore {
        store: store.clone(),
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::default(),
    };
    {
        let mut session = core.session.lock().unwrap();
        session.ready_threads.extend(ids.iter().cloned());
        session.adapter = Some(adapter);
    }
    let w = if let Some(old) = &old_case {
        store
            .snapshot_for_workstream(old["workstreamId"].as_str().unwrap())
            .unwrap()
            .workstreams
            .into_iter()
            .find(|w| w.id == old["workstreamId"].as_str().unwrap())
            .unwrap()
    } else {
        let project = store
            .create_project("Role compatibility real validation".into(), None)
            .unwrap();
        store
            .create_workstream(&project.id, "Cross-project Codex Bridge".into())
            .unwrap()
    };
    let input = |id: &str, name: &str| RoleBindingInput {
        provider: "CODEX".into(),
        external_id: id.into(),
        label: name.into(),
        cwd: None,
    };
    let b = bind(
        &core,
        &w.id,
        store.role_bridge(&w.id).unwrap().binding_revision,
        input(&ids[0], "Disposable Decision"),
        input(&ids[1], "Disposable Execution"),
    )
    .unwrap();
    assert_ne!(
        b.decision.as_ref().unwrap().cwd,
        b.execution.as_ref().unwrap().cwd
    );
    fs::write(evidence.join(if old_case.is_some(){"case-recovery.json"}else{"case.json"}),serde_json::to_vec_pretty(&json!({"workstreamId":w.id,"decisionThread":ids[0],"executionThread":ids[1],"decisionRoot":decision_root,"executionRoot":execution_root,"noRetry":true})).unwrap()).unwrap();
    let wait = |turn: &str| {
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            if let Some(result) = events.lock().unwrap().get(turn).cloned() {
                return result;
            }
            assert!(
                Instant::now() < deadline,
                "Original turn timeout; never resend"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    let source_prompt = format!(
        r#"Create only {} using PowerShell [IO.File]::WriteAllBytes and [Text.Encoding]::UTF8.GetBytes on the string "AIWR_PUBLIC_CODEX_ATTACHMENT_V1`nSelected bytes only.`n". Use LF bytes, no BOM; the result must be exactly53 bytes. Reply only two lines: ARTIFACT: followed by that absolute file path, then SHA256: followed by its actual lowercase SHA-256. No network or other file changes."#,
        decision_root.join("role-source.txt").display()
    );
    let (source_turn, source_result) = if old_case.is_some() {
        let state = read(&core, &w.id, "DECISION").unwrap();
        let source = state
            .replies
            .iter()
            .find(|r| r.endpoint_id == b.decision.as_ref().unwrap().endpoint.id)
            .unwrap();
        (
            source
                .assistant_identity
                .as_deref()
                .unwrap()
                .split(':')
                .nth(1)
                .unwrap()
                .to_string(),
            (ids[0].clone(), String::new(), source.text.clone()),
        )
    } else {
        fs::write(
            evidence.join("source-intent.json"),
            "{\"physicalTurnIntent\":1,\"purpose\":\"disposable native source preparation\"}",
        )
        .unwrap();
        let turn = {
            let mut s = core.session.lock().unwrap();
            start_turn(s.adapter.as_mut().unwrap(), &ids[0], &source_prompt)
                .unwrap()
                .turn_id
        };
        let result = wait(&turn);
        (turn, result)
    };
    assert_eq!(
        fs::read(decision_root.join("role-source.txt")).unwrap(),
        b"AIWR_PUBLIC_CODEX_ATTACHMENT_V1\nSelected bytes only.\n"
    );
    let state = read(&core, &w.id, "DECISION").unwrap();
    let source = state
        .replies
        .iter()
        .find(|r| {
            r.endpoint_id == b.decision.as_ref().unwrap().endpoint.id && r.text == source_result.2
        })
        .unwrap();
    let options = attachments(&core, &w.id, "DECISION", &source.id).unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].filename, "role-source.txt");
    let text="Inspect only role-source.txt under .aiwr/incoming in your own current project. Require exactly one matching file, read its bytes and return exactly ROLE_FILE_SHA256: followed by its actual lowercase SHA-256. Do not execute the attachment, access the network or change files.";
    let ready = prepare(
        &core,
        &w.id,
        "DECISION",
        &source.id,
        text,
        &[options[0].id.clone()],
    )
    .unwrap();
    assert_eq!(ready.status, "READY");
    assert!(store
        .provider_runs_for_workstream(&w.id)
        .unwrap()
        .is_empty());
    assert!(send(&core, &ready.id).is_err());
    let approved = approve(&core, &ready.id, &ready.payload_hash).unwrap();
    assert!(approved.approved_text.starts_with(text));
    assert!(store
        .provider_runs_for_workstream(&w.id)
        .unwrap()
        .is_empty());
    fs::write(
        evidence.join(if old_case.is_some() {
            "execution-recovery-intent.json"
        } else {
            "execution-intent.json"
        }),
        serde_json::to_vec(&json!({"handoffId":ready.id,"physicalTurnIntent":1})).unwrap(),
    )
    .unwrap();
    send(&core, &ready.id).unwrap();
    let run = store.provider_runs_for_workstream(&w.id).unwrap().remove(0);
    let target_turn = run.external_run_id.as_deref().unwrap();
    let target_result = wait(target_turn);
    assert_eq!(target_result.0, ids[1]);
    assert_eq!(
        target_result.2.split(':').nth(1).unwrap().trim(),
        "fd6ae9737f8d16c1003222b3d7170826053fe8cb59719e1a2439eb07fc589fed"
    );
    store
        .accept_completed_provider_result(
            &run.id,
            target_turn,
            &target_result.1,
            target_result.2.clone(),
        )
        .unwrap();
    assert_eq!(
        fs::read(
            execution_root
                .join(".aiwr/incoming")
                .join(&ready.id)
                .join("role-source.txt")
        )
        .unwrap(),
        fs::read(decision_root.join("role-source.txt")).unwrap()
    );
    assert!(send(&core, &ready.id).is_err());
    let state = read(&core, &w.id, "EXECUTION").unwrap();
    let reply = state
        .replies
        .iter()
        .find(|r| r.endpoint_id == b.execution.as_ref().unwrap().endpoint.id)
        .unwrap();
    let back = prepare(
        &core,
        &w.id,
        "EXECUTION",
        &reply.id,
        "Reply with exactly ROLE_RETURN_ACK. Do not access files or the network.",
        &[],
    )
    .unwrap();
    approve(&core, &back.id, &back.payload_hash).unwrap();
    fs::write(
        evidence.join("return-intent.json"),
        serde_json::to_vec(&json!({"handoffId":back.id,"physicalTurnIntent":1})).unwrap(),
    )
    .unwrap();
    send(&core, &back.id).unwrap();
    let back_run = store
        .provider_runs_for_workstream(&w.id)
        .unwrap()
        .into_iter()
        .find(|r| r.origin_handoff_id.as_deref() == Some(back.id.as_str()))
        .unwrap();
    let back_turn = back_run.external_run_id.as_deref().unwrap();
    let back_result = wait(back_turn);
    assert_eq!(back_result.0, ids[0]);
    assert_eq!(back_result.2.trim(), "ROLE_RETURN_ACK");
    store
        .accept_completed_provider_result(
            &back_run.id,
            back_turn,
            &back_result.1,
            back_result.2.clone(),
        )
        .unwrap();
    core.shutdown_owned_adapter();
    drop(core);
    drop(store);
    let reopened = crate::RouterStore::open_at(evidence.join("router.db")).unwrap();
    let restored = reopened.role_bridge(&w.id).unwrap();
    assert_eq!(restored.decision.unwrap().endpoint.external_id, ids[0]);
    assert_eq!(restored.execution.unwrap().endpoint.external_id, ids[1]);
    assert!(reopened.claim_role_handoff(&ready.id).is_err());
    assert!(reopened.claim_role_handoff(&back.id).is_err());
    assert_eq!(
        reopened.provider_runs_for_workstream(&w.id).unwrap().len(),
        2
    );
    let proof = json!({"status":"AUTOMATED_VALIDATION_PASS","scope":"Real Core role binding/read/selected file/approval/send/return between two different native Codex projects; SQLite restart/no resend","decisionThread":ids[0],"executionThread":ids[1],"sourcePreparationTurn":source_turn,"executionTurn":target_turn,"returnTurn":back_turn,"transferTurns":2,"sourcePreparationTurns":1,"previousCaseReusedWithoutSourceResend":old_case.is_some(),"model":"gpt-6-luna","effort":"low","selectedBytes":53,"sha256":"fd6ae9737f8d16c1003222b3d7170826053fe8cb59719e1a2439eb07fc589fed","workspaceDirectoriesDifferent":true,"noPreapprovalDispatch":true,"restartPersistence":true,"replayRejected":true,"installedEntry":false,"userUat":false});
    fs::write(
        evidence.join("public-proof.json"),
        serde_json::to_vec_pretty(&proof).unwrap(),
    )
    .unwrap();
    println!("REAL_ROLE_BRIDGE_CORE_PASS");
}

#[test]
#[ignore = "read only the retained original role transfer; never resend or create threads"]
fn read_retained_role_transfer() {
    assert_eq!(
        std::env::var("AIWR_REAL_ROLE_BRIDGE_GATE").as_deref(),
        Ok("1")
    );
    let evidence = std::path::PathBuf::from(std::env::var("AIWR_ROLE_EXISTING_CASE").unwrap());
    let case: Value =
        serde_json::from_slice(&fs::read(evidence.join("case-recovery.json")).unwrap()).unwrap();
    let store = Arc::new(crate::RouterStore::open_at(evidence.join("router.db")).unwrap());
    let mut adapter = CodexAdapter::start_validation(Arc::new(|_| {})).unwrap();
    adapter.initialize().unwrap();
    let core = RouterCore {
        store: store.clone(),
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::default(),
    };
    core.session.lock().unwrap().adapter = Some(adapter);
    let run = store
        .provider_runs_for_workstream(case["workstreamId"].as_str().unwrap())
        .unwrap()
        .into_iter()
        .last()
        .unwrap();
    {
        let mut session = core.session.lock().unwrap();
        let items=session.adapter.as_mut().unwrap().request("thread/items/list",json!({"threadId":case["executionThread"],"turnId":run.external_run_id,"limit":20,"sortDirection":"desc"})).unwrap();
        fs::write(
            evidence.join("retained-target-items.json"),
            serde_json::to_vec_pretty(&items).unwrap(),
        )
        .unwrap();
    }
    let catalog=service_list_existing_codex_threads(&core).unwrap();let selected=catalog.threads.into_iter().filter(|r|r.id==case["decisionThread"].as_str().unwrap()||r.id==case["executionThread"].as_str().unwrap()).collect::<Vec<_>>();fs::write(evidence.join("qa-native-catalog.json"),serde_json::to_vec_pretty(&selected).unwrap()).unwrap();
    let state = read(&core, case["workstreamId"].as_str().unwrap(), "EXECUTION").unwrap();
    let endpoint = state.bindings.execution.unwrap().endpoint;
    let reply = state
        .replies
        .iter()
        .find(|r| r.endpoint_id == endpoint.id)
        .unwrap();
    println!("RETAINED_PUBLIC_TARGET_REPLY={}", reply.text);
    fs::write(
        evidence.join("retained-target-reply.json"),
        serde_json::to_vec_pretty(reply).unwrap(),
    )
    .unwrap();
    core.shutdown_owned_adapter();
}

#[test]
#[ignore = "one new explicitly edited correction and one return on the retained exact role Bridge"]
fn finalize_retained_cross_project_bridge() {
    assert_eq!(
        std::env::var("AIWR_REAL_ROLE_BRIDGE_GATE").as_deref(),
        Ok("1")
    );
    let evidence = std::path::PathBuf::from(std::env::var("AIWR_ROLE_EXISTING_CASE").unwrap());
    assert!(
        !evidence.join("corrected-execution-intent.json").exists(),
        "Prior correction intent exists; read its receipt, never rerun this writer gate"
    );
    let case: Value =
        serde_json::from_slice(&fs::read(evidence.join("case-recovery.json")).unwrap()).unwrap();
    let w = case["workstreamId"].as_str().unwrap();
    let decision = case["decisionThread"].as_str().unwrap();
    let execution = case["executionThread"].as_str().unwrap();
    let store = Arc::new(crate::RouterStore::open_at(evidence.join("router.db")).unwrap());
    let events: Arc<Mutex<HashMap<String, (String, String, String)>>> = Arc::default();
    let captured = events.clone();
    let listener = Arc::new(move |event: &Value| {
        if event["method"] != "turn/completed" {
            return;
        }
        let Some(turn) = event.pointer("/params/turn/id").and_then(Value::as_str) else {
            return;
        };
        let Some(thread) = event
            .pointer("/params/threadId")
            .or_else(|| event.pointer("/params/turn/threadId"))
            .and_then(Value::as_str)
        else {
            return;
        };
        if let Ok((item, text)) = exact_codex_completed_event_result(thread, turn, event) {
            captured
                .lock()
                .unwrap()
                .insert(turn.into(), (thread.into(), item, text));
        }
    });
    let mut adapter = CodexAdapter::start_validation(listener).unwrap();
    adapter.initialize().unwrap();
    for id in [decision, execution] {
        let resumed=adapter.request("thread/resume",json!({"threadId":id,"excludeTurns":true,"model":"gpt-6-luna","config":{"model_reasoning_effort":"low"}})).unwrap();
        assert_eq!(resumed["model"], "gpt-6-luna");
        assert_eq!(resumed["reasoningEffort"], "low");
    }
    let core = RouterCore {
        store: store.clone(),
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::default(),
    };
    {
        let mut s = core.session.lock().unwrap();
        s.adapter = Some(adapter);
        s.ready_threads
            .extend([decision.to_string(), execution.to_string()]);
    }
    // Reconcile the known completed failed assertion's ORIGINAL accepted native turn.
    // Its response is honest failure evidence, never a fabricated success or retry.
    let original = store.provider_runs_for_workstream(w).unwrap().remove(0);
    let retained = {
        let mut s = core.session.lock().unwrap();
        crate::host_application::read_latest_codex_reply(s.adapter.as_mut().unwrap(), execution)
            .unwrap()
            .reply
            .unwrap()
    };
    assert_eq!(
        Some(retained.completed_turn_id.as_str()),
        original.external_run_id.as_deref()
    );
    assert!(retained.text.contains("none was found"));
    store
        .accept_completed_provider_result(
            &original.id,
            &retained.completed_turn_id,
            &retained.agent_item_id,
            retained.text.clone(),
        )
        .unwrap();
    let current = read(&core, w, "DECISION").unwrap();
    let src = current.bindings.decision.as_ref().unwrap();
    let obs = current
        .replies
        .iter()
        .find(|r| r.endpoint_id == src.endpoint.id)
        .unwrap();
    let files = attachments(&core, w, "DECISION", &obs.id).unwrap();
    assert_eq!(files.len(), 1);
    let ready=prepare(&core,w,"DECISION",&obs.id,"Read only the one selected role-source.txt at the exact attachment path below. Use PowerShell Get-ChildItem -LiteralPath on that file's parent directory with -Force -File, filter Name -ceq role-source.txt and require exactly one match. Hash the actual bytes. Reply only ROLE_FILE_SHA256: followed by the actual lowercase SHA-256. Do not execute it, access the network or change a file.",&[files[0].id.clone()]).unwrap();
    assert!(ready
        .approved_text
        .contains(&format!(".aiwr/incoming/{}/role-source.txt", ready.id)));
    let edited = store
        .edit_role_handoff(
            &ready.id,
            &ready.payload_hash,
            &format!(
                "{}\nUse UTF-8 if text decoding is needed.",
                ready.approved_text
            ),
        )
        .unwrap();
    approve(&core, &edited.id, &edited.payload_hash).unwrap();
    assert_eq!(store.provider_runs_for_workstream(w).unwrap().len(), 1);
    fs::write(evidence.join("corrected-execution-intent.json"),serde_json::to_vec_pretty(&json!({"handoffId":edited.id,"approvedText":edited.approved_text,"physicalSendIntent":1,"reason":"New reviewed exact-path instruction after the original turn terminal reported no file; never replay original SENT"})).unwrap()).unwrap();
    send(&core, &edited.id).unwrap();
    let wait = |turn: &str| {
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            if let Some(r) = events.lock().unwrap().get(turn).cloned() {
                return r;
            }
            assert!(
                Instant::now() < deadline,
                "Original accepted turn timed out; no resend"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    let run = store
        .provider_runs_for_workstream(w)
        .unwrap()
        .into_iter()
        .find(|r| r.origin_handoff_id.as_deref() == Some(edited.id.as_str()))
        .unwrap();
    let turn = run.external_run_id.as_deref().unwrap();
    let result = wait(turn);
    assert_eq!(result.0, execution);
    let value = result
        .2
        .trim()
        .strip_prefix("ROLE_FILE_SHA256:")
        .unwrap_or(result.2.trim())
        .trim();
    assert_eq!(
        value,
        "fd6ae9737f8d16c1003222b3d7170826053fe8cb59719e1a2439eb07fc589fed"
    );
    store
        .accept_completed_provider_result(&run.id, turn, &result.1, result.2.clone())
        .unwrap();
    assert!(send(&core, &edited.id).is_err());
    let executed = read(&core, w, "EXECUTION").unwrap();
    let reply = executed
        .replies
        .iter()
        .find(|r| {
            r.endpoint_id == executed.bindings.execution.as_ref().unwrap().endpoint.id
                && r.text == result.2
        })
        .unwrap();
    let back = prepare(
        &core,
        w,
        "EXECUTION",
        &reply.id,
        "Reply exactly ROLE_RETURN_ACK. Do not access files or network.",
        &[],
    )
    .unwrap();
    approve(&core, &back.id, &back.payload_hash).unwrap();
    fs::write(
        evidence.join("corrected-return-intent.json"),
        serde_json::to_vec(&json!({"handoffId":back.id,"physicalSendIntent":1})).unwrap(),
    )
    .unwrap();
    send(&core, &back.id).unwrap();
    let backrun = store
        .provider_runs_for_workstream(w)
        .unwrap()
        .into_iter()
        .find(|r| r.origin_handoff_id.as_deref() == Some(back.id.as_str()))
        .unwrap();
    let backturn = backrun.external_run_id.as_deref().unwrap();
    let returned = wait(backturn);
    assert_eq!(returned.0, decision);
    assert_eq!(returned.2.trim(), "ROLE_RETURN_ACK");
    store
        .accept_completed_provider_result(&backrun.id, backturn, &returned.1, returned.2.clone())
        .unwrap();
    read(&core, w, "DECISION").unwrap();
    let sha = crate::host_application::sha256_path(
        std::path::Path::new(case["executionRoot"].as_str().unwrap())
            .join(".aiwr/incoming")
            .join(&edited.id)
            .join("role-source.txt")
            .as_path(),
    )
    .unwrap();
    assert_eq!(sha, value);
    core.shutdown_owned_adapter();
    drop(core);
    drop(store);
    let reopened = crate::RouterStore::open_at(evidence.join("router.db")).unwrap();
    assert!(reopened.claim_role_handoff(&edited.id).is_err());
    assert!(reopened.claim_role_handoff(&back.id).is_err());
    let b = reopened.role_bridge(w).unwrap();
    assert_eq!(b.decision.unwrap().endpoint.external_id, decision);
    assert_eq!(b.execution.unwrap().endpoint.external_id, execution);
    let proof = json!({"status":"AUTOMATED_VALIDATION_PASS","scope":"Real shared Core cross-project Codex role binding/read/selected exact-path file/edit/approve/separate send/native result/return/SQLite restart/no resend","sourcePreparationTurns":1,"successfulTransferTurns":2,"retainedEarlierCompletedFileSearchFailureTurns":1,"originalPrewriteEmptyThreadResumeFailureHadProviderRuns":0,"decisionThread":decision,"executionThread":execution,"executionTurn":turn,"returnTurn":backturn,"executionHandoff":edited.id,"returnHandoff":back.id,"approvedText":edited.approved_text,"model":"gpt-6-luna","effort":"low","selectedBytes":53,"sha256":sha,"workspaceDirectoriesDifferent":true,"restartPersistence":true,"replayRejected":true,"sourceResends":0,"readOnlyReconciliationOfOriginal":true,"installedEntry":false,"userUat":false});
    fs::write(
        evidence.join("public-proof.json"),
        serde_json::to_vec_pretty(&proof).unwrap(),
    )
    .unwrap();
    println!("REAL_RETAINED_ROLE_BRIDGE_PASS");
}

#[test]
#[ignore = "one explicitly reviewed exact-command validation plus return; retains earlier model failures"]
fn complete_retained_bridge_with_exact_command() {
    assert_eq!(
        std::env::var("AIWR_REAL_ROLE_BRIDGE_GATE").as_deref(),
        Ok("1")
    );
    let evidence = std::path::PathBuf::from(std::env::var("AIWR_ROLE_EXISTING_CASE").unwrap());
    assert!(
        !evidence.join("exact-command-intent.json").exists(),
        "Never replay an existing writer intent"
    );
    let case: Value =
        serde_json::from_slice(&fs::read(evidence.join("case-recovery.json")).unwrap()).unwrap();
    let w = case["workstreamId"].as_str().unwrap();
    let decision = case["decisionThread"].as_str().unwrap();
    let execution = case["executionThread"].as_str().unwrap();
    let store = Arc::new(crate::RouterStore::open_at(evidence.join("router.db")).unwrap());
    let events: Arc<Mutex<HashMap<String, (String, String, String)>>> = Arc::default();
    let captured = events.clone();
    let listener = Arc::new(move |event: &Value| {
        if event["method"] != "turn/completed" {
            return;
        }
        let Some(turn) = event.pointer("/params/turn/id").and_then(Value::as_str) else {
            return;
        };
        let Some(thread) = event
            .pointer("/params/threadId")
            .or_else(|| event.pointer("/params/turn/threadId"))
            .and_then(Value::as_str)
        else {
            return;
        };
        if let Ok((item, text)) = exact_codex_completed_event_result(thread, turn, event) {
            captured
                .lock()
                .unwrap()
                .insert(turn.into(), (thread.into(), item, text));
        }
    });
    let mut adapter = CodexAdapter::start_validation(listener).unwrap();
    adapter.initialize().unwrap();
    for id in [decision, execution] {
        let resumed=adapter.request("thread/resume",json!({"threadId":id,"excludeTurns":true,"model":"gpt-6-luna","config":{"model_reasoning_effort":"low"}})).unwrap();
        assert_eq!(resumed["model"], "gpt-6-luna");
        assert_eq!(resumed["reasoningEffort"], "low");
    }
    let core = RouterCore {
        store: store.clone(),
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::default(),
    };
    {
        let mut s = core.session.lock().unwrap();
        s.adapter = Some(adapter);
        s.ready_threads.extend([decision.into(), execution.into()]);
    }
    let previous = store
        .provider_runs_for_workstream(w)
        .unwrap()
        .into_iter()
        .last()
        .unwrap();
    let latest = {
        let mut s = core.session.lock().unwrap();
        crate::host_application::read_latest_codex_reply(s.adapter.as_mut().unwrap(), execution)
            .unwrap()
            .reply
            .unwrap()
    };
    assert_eq!(
        previous.external_run_id.as_deref(),
        Some(latest.completed_turn_id.as_str())
    );
    if previous.status != "COMPLETED" {
        store
            .accept_completed_provider_result(
                &previous.id,
                &latest.completed_turn_id,
                &latest.agent_item_id,
                latest.text.clone(),
            )
            .unwrap();
    }
    let source = read(&core, w, "DECISION").unwrap();
    let obs = source
        .replies
        .iter()
        .find(|r| r.endpoint_id == source.bindings.decision.as_ref().unwrap().endpoint.id)
        .unwrap();
    let files = attachments(&core, w, "DECISION", &obs.id).unwrap();
    let ready = prepare(
        &core,
        w,
        "DECISION",
        &obs.id,
        "This is a disposable exact-command file-transport check, not a reasoning task.",
        &[files[0].id.clone()],
    )
    .unwrap();
    let path = std::path::Path::new(case["executionRoot"].as_str().unwrap())
        .join(".aiwr/incoming")
        .join(&ready.id)
        .join("role-source.txt");
    let directory = path.parent().unwrap();
    let command=format!("$ErrorActionPreference='Stop'; $qaFiles=@(Get-ChildItem -LiteralPath '{}' -Force -File | Where-Object {{$_.Name -ceq 'role-source.txt'}}); if($qaFiles.Count -ne 1){{throw 'Exact file required'}}; $qaHash=(Get-FileHash -LiteralPath $qaFiles[0].FullName -Algorithm SHA256).Hash.ToLowerInvariant(); Write-Output ('ROLE_FILE_SHA256:'+$qaHash)",directory.display());
    let edited=store.edit_role_handoff(&ready.id,&ready.payload_hash,&format!("{}\n\nExecute exactly this PowerShell command. Do not use Split-Path or substitute another hashing method. Reply with the command's actual output only; on failure return the error, never invent a hash. No network, attachment execution, or file changes.\n{}",ready.approved_text,command)).unwrap();
    approve(&core, &edited.id, &edited.payload_hash).unwrap();
    fs::write(evidence.join("exact-command-intent.json"),serde_json::to_vec_pretty(&json!({"handoffId":edited.id,"approvedText":edited.approved_text,"physicalSendIntent":1,"newReviewedInstruction":true})).unwrap()).unwrap();
    send(&core, &edited.id).unwrap();
    let wait = |turn: &str| {
        let end = Instant::now() + Duration::from_secs(180);
        loop {
            if let Some(r) = events.lock().unwrap().get(turn).cloned() {
                return r;
            }
            assert!(Instant::now() < end, "Never resend");
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    let run = store
        .provider_runs_for_workstream(w)
        .unwrap()
        .into_iter()
        .find(|r| r.origin_handoff_id.as_deref() == Some(edited.id.as_str()))
        .unwrap();
    let turn = run.external_run_id.clone().unwrap();
    let r = wait(&turn);
    assert_eq!(r.0, execution);
    assert_eq!(
        r.2.trim(),
        "ROLE_FILE_SHA256:fd6ae9737f8d16c1003222b3d7170826053fe8cb59719e1a2439eb07fc589fed"
    );
    store
        .accept_completed_provider_result(&run.id, &turn, &r.1, r.2.clone())
        .unwrap();
    assert!(send(&core, &edited.id).is_err());
    let returned = read(&core, w, "EXECUTION").unwrap();
    let reply = returned
        .replies
        .iter()
        .find(|reply| reply.text == r.2)
        .unwrap();
    let back = prepare(
        &core,
        w,
        "EXECUTION",
        &reply.id,
        "Reply exactly ROLE_RETURN_ACK. Do not use tools, files or network.",
        &[],
    )
    .unwrap();
    approve(&core, &back.id, &back.payload_hash).unwrap();
    fs::write(
        evidence.join("exact-return-intent.json"),
        serde_json::to_vec(&json!({"handoffId":back.id,"physicalSendIntent":1})).unwrap(),
    )
    .unwrap();
    send(&core, &back.id).unwrap();
    let br = store
        .provider_runs_for_workstream(w)
        .unwrap()
        .into_iter()
        .find(|r| r.origin_handoff_id.as_deref() == Some(back.id.as_str()))
        .unwrap();
    let bt = br.external_run_id.clone().unwrap();
    let result = wait(&bt);
    assert_eq!(result.0, decision);
    assert_eq!(result.2.trim(), "ROLE_RETURN_ACK");
    store
        .accept_completed_provider_result(&br.id, &bt, &result.1, result.2.clone())
        .unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        b"AIWR_PUBLIC_CODEX_ATTACHMENT_V1\nSelected bytes only.\n"
    );
    core.shutdown_owned_adapter();
    drop(core);
    drop(store);
    let reopened = crate::RouterStore::open_at(evidence.join("router.db")).unwrap();
    assert!(reopened.claim_role_handoff(&edited.id).is_err());
    assert!(reopened.claim_role_handoff(&back.id).is_err());
    let b = reopened.role_bridge(w).unwrap();
    assert_eq!(b.decision.unwrap().endpoint.external_id, decision);
    assert_eq!(b.execution.unwrap().endpoint.external_id, execution);
    let proof = json!({"status":"AUTOMATED_VALIDATION_PASS","scope":"Real shared Core cross-project Codex selected file/edited immutable approval/native read/hash result/return/persistent role binding/no resend","decisionThread":decision,"executionThread":execution,"executionHandoff":edited.id,"returnHandoff":back.id,"executionTurn":turn,"returnTurn":bt,"sourcePreparationTurns":1,"successfulTransferTurns":2,"retainedEarlierCompletedFileSearchAndShellFailures":2,"sourceResends":0,"selectedBytes":53,"sha256":"fd6ae9737f8d16c1003222b3d7170826053fe8cb59719e1a2439eb07fc589fed","workspaceDirectoriesDifferent":true,"model":"gpt-6-luna","effort":"low","approvalBeforeSeparateSend":true,"attachmentPathsVisibleBeforeApproval":true,"restartPersistence":true,"replayRejected":true,"earlierFailuresReconciledReadOnly":true,"installedEntry":false,"userUat":false});
    fs::write(
        evidence.join("public-proof.json"),
        serde_json::to_vec_pretty(&proof).unwrap(),
    )
    .unwrap();
    println!("REAL_ROLE_BRIDGE_COMPLETE_PASS");
}

#[test]
fn interrupted_read_returns_retained_state_without_new_observation_or_handoff(){
 let d=tempfile::tempdir().unwrap();let store=Arc::new(crate::RouterStore::open_at(d.path().join("router.db")).unwrap());let p=store.create_project("isolated status".into(),None).unwrap();let w=store.create_workstream(&p.id,"interrupted reader".into()).unwrap();let input=|id:&str|RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:id.into(),cwd:Some(d.path().to_string_lossy().into())};let b=store.bind_role_bridge(&w.id,store.role_bridge(&w.id).unwrap().binding_revision,input("source-fixture"),input("target-fixture")).unwrap();let endpoint=&b.decision.unwrap().endpoint.id;store.record_reply_observation(&w.id,endpoint,Some("codex:old-complete:old-item"),"prior complete reply",None).unwrap();let core=RouterCore{store:store.clone(),chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::default()};
 let result=read_without_new_reply(&core,&w.id,"DECISION",endpoint,"LATEST_TURN_INTERRUPTED").unwrap();assert_eq!(result.replies.len(),1);assert_eq!(result.replies[0].text,"prior complete reply");assert!(result.handoffs.is_empty());let outcome=result.read_outcome.unwrap();assert_eq!(outcome.state,"LATEST_TURN_INTERRUPTED");assert!(outcome.retained_reply);assert_eq!(outcome.endpoint_id,*endpoint);assert!(read_without_new_reply(&core,&w.id,"DECISION",endpoint,"CODEX_OBSERVATION_PROTOCOL_INVALID").is_err());assert_eq!(store.reply_observations_for_workstream(&w.id).unwrap().len(),1);
}

#[test]
fn blocks_are_readonly_and_bound_to_exact_observation_and_role_without_provider_calls() {
 let d=tempfile::tempdir().unwrap();let store=Arc::new(crate::RouterStore::open_at(d.path().join("router.db")).unwrap());let p=store.create_project("isolated blocks".into(),None).unwrap();let w=store.create_workstream(&p.id,"source segmentation".into()).unwrap();let input=|id:&str|RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:id.into(),cwd:Some(d.path().to_string_lossy().into())};let b=store.bind_role_bridge(&w.id,store.role_bridge(&w.id).unwrap().binding_revision,input("source-fixture"),input("target-fixture")).unwrap();let endpoint=&b.decision.unwrap().endpoint.id;
 store.record_reply_observation(&w.id,endpoint,Some("codex:old-complete:old-item"),"结论。\n\n可直接发给原对话：\n\n```text\n请继续。\n```",None).unwrap();
 let core=RouterCore{store:store.clone(),chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::default()};let obs=store.reply_observations_for_workstream(&w.id).unwrap().remove(0);let selected=blocks(&core,&w.id,"DECISION",&obs.id).unwrap();assert_eq!(selected.len(),2);assert_eq!(selected[1].text,"请继续。");assert!(selected[1].recommended);assert!(blocks(&core,&w.id,"EXECUTION",&obs.id).is_err());assert!(blocks(&core,&w.id,"DECISION","unrelated-observation").is_err());assert!(state(&core,&w.id).unwrap().handoffs.is_empty());assert_eq!(store.reply_observations_for_workstream(&w.id).unwrap().len(),1);
}

#[test]
fn create_bridge_reuses_existing_group_and_never_binds_or_sends(){
 let d=tempfile::tempdir().unwrap();let store=Arc::new(crate::RouterStore::open_at(d.path().join("router.db")).unwrap());let core=RouterCore{store:store.clone(),chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::default()};assert!(create(&core,"").is_err());let first=create(&core,"First").unwrap();let second=create(&core,"Second").unwrap();let snapshot=store.snapshot().unwrap();assert_eq!(snapshot.projects.len(),1);assert_eq!(snapshot.workstreams.len(),2);assert!(!state(&core,&first).unwrap().bindings.explicit_roles);assert!(!state(&core,&second).unwrap().bindings.explicit_roles);assert!(store.provider_runs_for_workstream(&first).unwrap().is_empty());assert!(state(&core,&first).unwrap().handoffs.is_empty());
}

#[test]
fn historical_role_handoff_remains_visible_after_rebinding_without_becoming_sendable(){
 let d=tempfile::tempdir().unwrap();let store=Arc::new(crate::RouterStore::open_at(d.path().join("router.db")).unwrap());let p=store.create_project("history".into(),None).unwrap();let w=store.create_workstream(&p.id,"bridge".into()).unwrap();let input=|id:&str|RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:id.into(),cwd:Some(d.path().to_string_lossy().into())};let b=store.bind_role_bridge(&w.id,store.role_bridge(&w.id).unwrap().binding_revision,input("old-source"),input("old-target")).unwrap();let obs=store.record_reply_observation(&w.id,&b.decision.unwrap().endpoint.id,Some("old-native"),"original",None).unwrap().unwrap();let h=store.prepare_role_handoff(&w.id,"DECISION",&obs.id,"selected original").unwrap();store.bind_role_bridge(&w.id,b.binding_revision,input("new-source"),input("new-target")).unwrap();assert!(store.approve_role_handoff_checked(&h.id,&h.payload_hash).is_err());let core=RouterCore{store,chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::default()};let view=state(&core,&w.id).unwrap();assert_eq!(view.handoffs.len(),1);assert_eq!(view.handoff_sources[0].role,"DECISION");assert_eq!(view.handoff_sources[0].binding_revision,b.binding_revision);
}
