use super::*;

// A fictional ACP process: never executes agy or submits an inference request.
fn fixture() -> (tempfile::TempDir, RouterCore, Arc<Client>, String) {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("agent.cjs");
    std::fs::write(&script, r#"
const send=v=>process.stdout.write(JSON.stringify({jsonrpc:'2.0',...v})+'\n');
const update=(sessionId,role,text)=>send({method:'session/update',params:{sessionId,update:{sessionUpdate:role+'_message_chunk',content:{type:'text',text}}}});
let permission;
require('readline').createInterface({input:process.stdin}).on('line',line=>{
 const r=JSON.parse(line),p=r.params||{};
 if(r.method==='initialize')return send({id:r.id,result:{protocolVersion:1}});
 if(r.method==='session/load'){
  update(p.sessionId,'user','old ');update(p.sessionId,'user','question');
  update(p.sessionId,'agent','old ');update(p.sessionId,'agent','answer');
  return send({id:r.id,result:{}});
 }
 if(r.method==='session/prompt'){
  const text=p.prompt[0].text;
  if(text==='disconnect')return process.exit(0);
  if(text==='permission'){
   permission=r;
   return send({id:'native-permission',method:'session/request_permission',params:{sessionId:p.sessionId,options:[{optionId:'once',name:'Allow once',kind:'allow_once'},{optionId:'always',name:'Always',kind:'allow_always'},{optionId:'reject',name:'Reject once',kind:'reject_once'}]}});
  }
  update(p.sessionId,'user',text);
  return setTimeout(()=>{update(p.sessionId,'agent','reply:'+text);send({id:r.id,result:{stopReason:'end_turn'}})},text==='slow'?2000:250);
 }
 if(r.id==='native-permission'&&r.result){
  update(permission.params.sessionId,'agent','permission handled');
  send({id:permission.id,result:{stopReason:'end_turn'}});permission=null;
 }
});
"#).unwrap();
    let store = Arc::new(RouterStore::open_at(dir.path().join("router.sqlite")).unwrap());
    let core = RouterCore {
        store: store.clone(),
        session: Arc::new(Mutex::new(Default::default())),
    };
    let target: Arc<Mutex<Weak<Client>>> = Arc::new(Mutex::new(Weak::new()));
    let weak = target.clone();
    let db = Arc::downgrade(&store);
    let mut command = Command::new("node");
    command.arg(script);
    let rpc = rpc::Rpc::start(
        command,
        Arc::new(move |event| {
            if let (Some(c), Some(store)) = (weak.lock().unwrap().upgrade(), db.upgrade()) {
                c.event(&store, event);
            }
        }),
    )
    .unwrap();
    rpc.request("initialize", json!({"protocolVersion":1}))
        .unwrap();
    let c = Arc::new(Client {
        permission_mode: "ASK".into(),
        rpc,
        epoch: "fixture-epoch".into(),
        sessions: Mutex::new(HashMap::new()),
        permissions: Mutex::new(HashMap::new()),
        gate: Mutex::new(()),
        capabilities: json!({}),
    });
    *target.lock().unwrap() = Arc::downgrade(&c);
    *service(&core).unwrap().client.lock().unwrap() = Some(c.clone());
    let key = c
        .register(
            &store,
            "fixture-native",
            &dir.path().to_string_lossy(),
            json!({}),
        )
        .unwrap();
    (dir, core, c, key)
}
fn wait(core: &RouterCore, id: &str, expected: &str) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if core
            .store
            .watch_reply(id)
            .unwrap()
            .is_some_and(|r| r.status == expected)
        {
            return;
        }
        assert!(Instant::now() < until, "receipt did not reach {expected}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn original_send_and_explicit_queue_are_once_only_and_chronological() {
    let (_dir, core, _c, key) = fixture();
    let generation = core.store.codex_chat_context(&key).unwrap().generation;
    let first = uuid::Uuid::new_v4().to_string();
    let second = uuid::Uuid::new_v4().to_string();
    let result = send(
        &core,
        &key,
        &first,
        generation,
        "SEND",
        "one".into(),
        Default::default(),
        false,
    )
    .unwrap();
    assert_eq!(result["status"], "SENDING");
    assert_eq!(
        send(
            &core,
            &key,
            &first,
            generation,
            "SEND",
            "one".into(),
            Default::default(),
            false
        )
        .unwrap()["status"],
        "SENDING"
    );
    assert!(send(
        &core,
        &key,
        &first,
        generation,
        "SEND",
        "changed".into(),
        Default::default(),
        false
    )
    .is_err());
    assert_eq!(
        send(
            &core,
            &key,
            &second,
            generation,
            "QUEUE",
            "two".into(),
            Default::default(),
            false
        )
        .unwrap()["status"],
        "QUEUED"
    );
    wait(&core, &first, "SENT");
    wait(&core, &second, "SENT");
    let h = core.store.managed_public_history(&key, None).unwrap();
    let texts: Vec<_> = h["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, vec!["one", "reply:one", "two", "reply:two"]);
}
#[test]
fn disconnect_is_unknown_and_never_dispatches_next_queue() {
    let (_dir, core, _c, key) = fixture();
    let generation = core.store.codex_chat_context(&key).unwrap().generation;
    let first = uuid::Uuid::new_v4().to_string();
    send(
        &core,
        &key,
        &first,
        generation,
        "SEND",
        "disconnect".into(),
        Default::default(),
        false,
    )
    .unwrap();
    wait(&core, &first, "UNKNOWN");
    // Reading the original attempt works even when the native process is gone.
    assert_eq!(
        send(
            &core,
            &key,
            &first,
            generation,
            "SEND",
            "disconnect".into(),
            Default::default(),
            false
        )
        .unwrap()["status"],
        "UNKNOWN"
    );
    assert!(core
        .store
        .prepare_managed_owner_reply(&WatchReply {
            id: uuid::Uuid::new_v4().to_string(),
            thread_id: key,
            cwd: _dir.path().to_string_lossy().into_owned(),
            generation,
            source_sequence: None,
            expected_turn_id: None,
            mode: "QUEUE".into(),
            text: "never send".into(),
            options: json!({}),
            payload_hash: String::new(),
            status: String::new(),
            turn_id: None,
            error_code: None,
            created_at: 0,
            updated_at: 0
        })
        .is_err());
}
#[test]
fn permission_uses_exact_native_request_and_rejects_persistent_grants() {
    let (_dir, core, c, key) = fixture();
    let generation = core.store.codex_chat_context(&key).unwrap().generation;
    let id = uuid::Uuid::new_v4().to_string();
    send(
        &core,
        &key,
        &id,
        generation,
        "SEND",
        "permission".into(),
        Default::default(),
        false,
    )
    .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while c.permissions.lock().unwrap().is_empty() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    let view = state(&core, &key).unwrap();
    assert_eq!(view["requests"][0]["choices"].as_array().unwrap().len(), 2);
    let request = view["requests"][0]["requestId"].as_str().unwrap();
    let answer = |decision: &str| crate::host_application::MobileCodexResponseInput {
        revision: 1,
        decision: Some(decision.into()),
        answers: None,
    };
    assert!(validate_response(&core, &key, request, &answer("always")).is_err());
    assert!(validate_response(&core, &key, request, &answer("once")).is_ok());
    command(
        &core,
        ChatCommand::Respond {
            thread_id: key.clone(),
            request_id: request.into(),
            input: answer("once"),
        },
    )
    .unwrap();
    assert!(validate_response(&core, &key, request, &answer("once")).is_err());
    wait(&core, &id, "SENT");
}
#[test]
fn load_replay_is_grouped_and_replaces_previous_observations() {
    let (_dir, core, c, key) = fixture();
    for _ in 0..2 {
        c.sessions.lock().unwrap().remove("fixture-native");
        c.ensure(&core, &key).unwrap();
        let h = core.store.managed_public_history(&key, None).unwrap();
        let rows = h["messages"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["text"], "old question");
        assert_eq!(rows[1]["text"], "old answer");
    }
}

#[test]
fn reviewed_mcp_send_keeps_provenance_and_revocation_blocks_queued_native_offer() {
    let (_dir, core, _c, key) = fixture();
    let generation = core.store.codex_chat_context(&key).unwrap().generation;
    let grant = core
        .store
        .connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput {
            label: "Fixture manager".into(),
            expires_at: now() + 600000,
        })
        .unwrap();
    let call = |name: &str, args| crate::assistant_mcp::call(&core, &grant.id, name, args);
    let view = call("agbrio_read_chat", json!({"threadId":key})).unwrap();
    assert_eq!(view["state"]["provider"], "ANTIGRAVITY");
    let approved = |a: &Value| json!({"actionId":a["id"],"expectedHash":a["payloadHash"],"ruleId":null,"useOwnerAnswer":false,"assessment":"Owner-delegated fictional message"});
    let first=call("agbrio_prepare_action",json!({"requestId":"fixture-first","operation":"SEND_CHAT","input":{"threadId":key,"generation":generation,"mode":"AUTO","text":"slow"}})).unwrap();
    assert_eq!(
        call("agbrio_execute_action", approved(&first)).unwrap()["status"],
        "APPLIED"
    );
    assert_eq!(
        call("agbrio_execute_action", approved(&first)).unwrap()["status"],
        "APPLIED"
    );
    let next=call("agbrio_prepare_action",json!({"requestId":"fixture-next","operation":"SEND_CHAT","input":{"threadId":key,"generation":generation,"mode":"QUEUE","queueRequested":true,"text":"revoked-never-offered"}})).unwrap();
    assert_eq!(
        call("agbrio_execute_action", approved(&next)).unwrap()["result"]["status"],
        "QUEUED"
    );
    core.store.revoke_assistant_grant(&grant.id).unwrap();
    wait(&core, first["id"].as_str().unwrap(), "SENT");
    wait(&core, next["id"].as_str().unwrap(), "FAILED");
    let h = core.store.managed_public_history(&key, None).unwrap();
    assert_eq!(h["messages"].as_array().unwrap().len(), 2);
    assert_eq!(h["messages"][0]["text"], "slow");
}

#[test]fn owner_default_is_persisted_without_changing_the_running_native_process(){let(_d,core,c,_key)=fixture();assert_eq!(permissions(&core).unwrap()["defaultMode"],"ASK");core.store.set_antigravity_permission_mode("FULL_ACCESS").unwrap();let v=permissions(&core).unwrap();assert_eq!(v["defaultMode"],"FULL_ACCESS");assert_eq!(v["runningMode"],"ASK");assert_eq!(v["pendingNextLaunch"],true);assert!(!c.rpc.closed());assert!(core.store.set_antigravity_permission_mode("INVALID").is_err());}

#[test]fn full_access_uses_the_native_flag_and_regular_mode_never_silently_adds_it(){for(mode,want)in[("ASK",false),("FULL_ACCESS",true)]{let mut cmd=Command::new("fictional-native-agent");configure_permissions(&mut cmd,mode).unwrap();assert_eq!(cmd.get_args().any(|v|v=="--dangerously-skip-permissions"),want)}assert!(configure_permissions(&mut Command::new("fixture"),"unknown").is_err());}

#[test]
fn native_configuration_never_guesses_model_from_title_or_prompt() {
    assert!(runtime_configuration(&json!({"title":"gemini-fiction", "text":"high"})).is_none());
    let r=runtime_configuration(&json!({"models":{"currentModelId":"native-model","availableModels":[{"modelId":"native-model","name":"Native Model"}]},"configOptions":[{"id":"thought_level","currentValue":"high"}]})).unwrap();
    assert_eq!(r["model"], "native-model");
    assert_eq!(r["modelLabel"], "Native Model");
    assert_eq!(r["effort"], "high");
    assert_eq!(r["source"], "NATIVE");
}
#[test]
fn pending_observation_and_snapshot_identity_survive_silent_tools_without_replaying() {
    let (_dir,core,c,key)=fixture();
    let generation=core.store.codex_chat_context(&key).unwrap().generation;
    let id=uuid::Uuid::new_v4().to_string();
    send(&core,&key,&id,generation,"SEND","slow".into(),Default::default(),false).unwrap();
    // A stale observer timestamp is repaired by checking the live pending call.
    c.refresh_pending_observations(&core.store);
    let w=core.store.codex_chat_context(&key).unwrap();
    assert_eq!(w.snapshot.state,"RUNNING");
    assert_eq!(core.store.watch_reply(&id).unwrap().unwrap().status,"SENDING");
    std::thread::sleep(Duration::from_millis(2400));
    let h=core.store.managed_public_history(&key,None).unwrap();
    assert_eq!(h["messages"].as_array().unwrap().len(),2);
    assert_eq!(h["messages"][0]["clientId"],id);
    let w=core.store.codex_chat_context(&key).unwrap();
    assert_eq!(w.snapshot.state,"RESULT_READY");
    assert_eq!(json!(w.snapshot.turn_id),h["messages"][1]["turnId"]);
    assert_eq!(json!(w.snapshot.item_id),h["messages"][1]["id"]);
    c.refresh_pending_observations(&core.store);
    c.snapshot(&core.store,"fixture-native","RUNNING").unwrap();
    assert_eq!(core.store.codex_chat_context(&key).unwrap().snapshot.state,"RESULT_READY");
}

#[test]
fn creation_label_is_visible_before_first_prompt_and_native_title_can_follow() {
    let (_dir,core,c,key)=fixture();
    apply_creation_label(&core.store,&key,Some("Experiment 1 - Neon")).unwrap();
    assert_eq!(core.store.codex_chat_context(&key).unwrap().label,"Experiment 1 - Neon");
    assert!(core.store.watch_replies(&key).unwrap().is_empty());
    assert!(apply_creation_label(&core.store,&key,Some("bad\ntitle")).is_err());
    c.event(&core.store,json!({"method":"session/update","params":{"sessionId":"fixture-native","update":{"sessionUpdate":"session_info_update","title":"Native title"}}}));
    assert_eq!(core.store.codex_chat_context(&key).unwrap().label,"Native title");
}
#[test]
fn progress_exposes_only_timestamps_and_tool_counts_never_reasoning_or_arguments() {
    let mut p=Progress::default();
    p.observe(&json!({"sessionUpdate":"agent_thought_chunk","content":{"text":"PRIVATE_REASONING"}}));
    p.observe(&json!({"sessionUpdate":"tool_call","toolCallId":"private-id","status":"in_progress","rawInput":{"secret":"DO_NOT_EXPOSE"}}));
    let v=p.view(true,true);
    assert_eq!(v["activeTools"],1);assert_eq!(v["promptPending"],true);
    assert!(v["lastPublicTextAt"].is_null());assert!(v["lastNativeEventAt"].is_i64());
    for forbidden in ["PRIVATE_REASONING","DO_NOT_EXPOSE","private-id"] {assert!(!v.to_string().contains(forbidden));}
    p.observe(&json!({"sessionUpdate":"tool_call_update","toolCallId":"private-id","status":"completed"}));
    assert_eq!(p.view(true,true)["activeTools"],0);
    assert_eq!(p.view(true,true)["latestToolStatus"],"completed");
}
#[test]
fn long_silent_task_retains_pending_receipt_and_progress_checks_never_resend() {
    let (_dir,core,c,key)=fixture();let id=uuid::Uuid::new_v4().to_string();
    let generation=core.store.codex_chat_context(&key).unwrap().generation;
    send(&core,&key,&id,generation,"SEND","slow".into(),Default::default(),false).unwrap();
    {let mut sessions=c.sessions.lock().unwrap();let l=sessions.get_mut("fixture-native").unwrap();l.progress.last_native_event_at=Some(now()-60*60*1000);}
    let old=progress(&core,&key).unwrap()["lastNativeEventAt"].clone();
    for _ in 0..3 {c.refresh_pending_observations(&core.store);}
    assert_eq!(core.store.codex_chat_context(&key).unwrap().snapshot.state,"RUNNING");
    let v=progress(&core,&key).unwrap();assert_eq!(v["lastNativeEventAt"],old);assert_eq!(v["promptPending"],true);
    assert_eq!(core.store.watch_reply(&id).unwrap().unwrap().status,"SENDING");
    assert_eq!(core.store.watch_replies(&key).unwrap().len(),1);
    wait(&core,&id,"SENT");
    assert_eq!(progress(&core,&key).unwrap()["promptPending"],false);
}
