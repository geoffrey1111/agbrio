use super::*;

use codex::adapter::CodexAdapter;
use persistence::{Endpoint, NewHandoff, Provider, RouterStore, Workstream};
use serde_json::{json, Value};
use sha2::Digest;

#[test]
fn unmaterialized_empty_codex_thread_requires_exact_structured_error_and_proven_identity() {
    let error = |id: &str, code: i64| format!("Codex JSON-RPC error: {}", json!({"code":code,"message":format!("thread {id} is not materialized yet; thread/turns/list is unavailable before first user message")}));
    assert!(is_unmaterialized_empty_codex_thread("native-new", &error("native-new", -32600)));
    assert!(!is_unmaterialized_empty_codex_thread("native-new", &error("other-thread", -32600)));
    assert!(!is_unmaterialized_empty_codex_thread("native-new", &error("native-new", -32603)));
    assert!(!is_unmaterialized_empty_codex_thread("native-new", "thread native-new is not materialized yet"));
    assert!(!is_unmaterialized_empty_codex_thread("native-new", "Codex JSON-RPC error: {\"code\":-32600,\"message\":\"thread not found\"}"));
}
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

#[test]
fn active_exact_codex_binding_gets_an_empty_baseline_but_materializing_history_does_not() {
    assert!(should_initialize_empty_codex_baseline(
        "NO_NEW_TERMINAL_REPLY"
    ));
    assert!(should_initialize_empty_codex_baseline("LATEST_TURN_ACTIVE"));
    assert!(!should_initialize_empty_codex_baseline(
        "OBSERVATION_MATERIALIZATION_PENDING"
    ));
    assert!(!should_initialize_empty_codex_baseline(
        "LATEST_TURN_INTERRUPTED"
    ));
}

#[test]
fn exact_not_loaded_metadata_error_requires_the_bound_thread_identity() {
    assert!(exact_not_loaded_metadata_error(
        "Codex JSON-RPC error: {\"code\":-32600,\"message\":\"thread not loaded: exact-thread\"}",
        "exact-thread"
    ));
    assert!(!exact_not_loaded_metadata_error(
        "Codex JSON-RPC error: {\"code\":-32600,\"message\":\"thread not loaded: other-thread\"}",
        "exact-thread"
    ));
}

#[test]
fn existing_thread_candidate_keeps_exact_identity_separate_from_optional_project_provenance() {
    let candidate = existing_codex_thread_candidate(&json!({
        "id": "thread-exact-a", "name": "Duplicate title", "preview": "first",
        "updatedAt": 1720000000, "projectId": "native-project-a"
    }))
    .unwrap();
    assert_eq!(candidate.id, "thread-exact-a");
    assert_eq!(candidate.label, "Duplicate title");
    assert_eq!(candidate.updated_at.as_deref(), Some("1720000000"));
    assert_eq!(
        candidate.project_provenance.as_deref(),
        Some("已核验原生 Codex 项目归属")
    );

    let without_project = existing_codex_thread_candidate(&json!({
        "id": "thread-exact-b", "name": "Duplicate title"
    }))
    .unwrap();
    assert_eq!(without_project.id, "thread-exact-b");
    assert!(without_project.project_provenance.is_none());
}

#[test]
fn existing_thread_candidate_fails_closed_without_a_stable_identity() {
    assert!(existing_codex_thread_candidate(&json!({ "name": "unroutable" })).is_err());
}

#[test]
fn desktop_catalog_join_uses_exact_thread_id_not_native_project_name_cwd_or_updated_time(){
 use router_core::codex::desktop_catalog::{DesktopCatalog,Project,Membership};
 let mut d=DesktopCatalog::default();d.projects.insert("correct".into(),Project{name:"同名项目".into(),roots:vec!["D:/source".into()]});d.memberships.insert("exact".into(),Membership::Project("correct".into()));d.memberships.insert("free".into(),Membership::Projectless);
 let mut rows=vec![existing_codex_thread_candidate(&json!({"id":"exact","name":"重复名称","projectId":"wrong","cwd":"D:/wrong","recencyAt":100,"updatedAt":900})).unwrap(),existing_codex_thread_candidate(&json!({"id":"different","name":"重复名称","projectId":"correct","cwd":"D:/source"})).unwrap(),existing_codex_thread_candidate(&json!({"id":"free"})).unwrap()];
 apply_desktop_thread_projects(&mut rows,Some(&d));assert_eq!(rows[0].project_id.as_deref(),Some("correct"));assert_eq!(rows[0].recency_at,Some(100));assert_eq!(rows[1].project_status,"UNCONFIRMED");assert_eq!(rows[2].project_status,"PROJECTLESS");apply_desktop_thread_projects(&mut rows,None);assert!(rows.iter().all(|r|r.project_status=="UNCONFIRMED"));
}

#[test]
#[ignore="read-only full local thread catalogue and typed Desktop grouping; no provider turn or binding"]
fn real_grouped_codex_catalog_read_only(){
 assert_eq!(std::env::var("AIWR_REAL_CATALOG_GATE").as_deref(),Ok("1"));
 let root=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/codex-catalog");std::fs::create_dir_all(&root).unwrap();
 let path=root.join(format!("read-{}.db",uuid::Uuid::new_v4()));let store=std::sync::Arc::new(RouterStore::open_at(path).unwrap());
 let mut adapter=crate::codex::adapter::CodexAdapter::start_validation(std::sync::Arc::new(|_|{})).unwrap();adapter.initialize().unwrap();let desktop=adapter.read_desktop_catalog().unwrap();
 let core=RouterCore{store,session:std::sync::Arc::new(std::sync::Mutex::new(Session::default())),};core.session.lock().unwrap().adapter=Some(adapter);
 let catalogue=service_list_existing_codex_threads(&core).unwrap();assert!(catalogue.complete);assert!(!catalogue.threads.is_empty());let now=rollover_now().unwrap();
 let windows=[1,3,7,30].into_iter().map(|day|catalogue.threads.iter().filter(|t|t.recency_at.is_some_and(|at|at>=now-day*86400)).count()).collect::<Vec<_>>();
 let mut grouped=0;let mut free=0;let mut unknown=0;
 for t in &catalogue.threads {match t.project_status.as_str(){"PROJECT"=>{let id=t.project_id.as_ref().unwrap();assert_eq!(desktop.memberships.get(&t.id),Some(&router_core::codex::desktop_catalog::Membership::Project(id.clone())));assert_eq!(t.project_label.as_ref().unwrap(),&desktop.projects[id].name);grouped+=1;},"PROJECTLESS"=>free+=1,_=>unknown+=1}}
 assert!(core.store.snapshot().unwrap().endpoint_lineage.is_empty());let counts=core.session.lock().unwrap().adapter.as_ref().unwrap().request_counts();assert_eq!(counts.get("turn/start"),None);assert_eq!(counts.get("thread/resume"),None);
 let proof=serde_json::json!({"status":"AUTOMATED_VALIDATION_PASS","complete":true,"threads":catalogue.threads.len(),"projectAssigned":grouped,"explicitProjectless":free,"membershipUnconfirmed":unknown,"windowCounts1_3_7_30":windows,"allCount":catalogue.threads.len(),"nativePages":counts.get("thread/list"),"nativeProjectNames":true,"source":"exact Desktop metadata projection plus official thread/list recencyAt","bindingWrites":0,"turnWrites":0,"resumes":0,"userUat":false});
 std::fs::write(root.join("real-catalog-proof.json"),serde_json::to_vec_pretty(&proof).unwrap()).unwrap();
 // Retain only the typed API projection in ignored evidence, not the whole Desktop state.
 std::fs::write(root.join("ui-catalog.json"),serde_json::to_vec(&catalogue).unwrap()).unwrap();
}

#[test]
fn completed_codex_event_requires_exact_terminal_thread_turn_and_result() {
    let event = json!({
        "method": "turn/completed",
        "params": {
            "threadId": "thread-exact",
            "turn": {
                "id": "turn-exact",
                "status": "completed",
                "items": [
                    { "type": "agentMessage", "id": "item-exact", "text": "result" }
                ]
            }
        }
    });
    assert_eq!(
        exact_codex_completed_event_result("thread-exact", "turn-exact", &event).unwrap(),
        ("item-exact".into(), "result".into())
    );
    assert!(exact_codex_completed_event_result("other-thread", "turn-exact", &event).is_err());
    assert!(exact_codex_completed_event_result("thread-exact", "other-turn", &event).is_err());
}

#[test]
fn codex_request_state_filter_preserves_requests_and_skips_large_stream_events() {
    for method in [
        "item/commandExecution/requestApproval",
        "item/fileChange/requestApproval",
        "item/permissions/requestApproval",
        "item/tool/requestUserInput",
        "serverRequest/resolved",
    ] {
        for id in [json!(17), json!("request-exact-17")] {
            let message = json!({"method": method, "id": id, "params": {"threadId":"thread-exact", "turnId":"turn-exact"}});
            assert!(codex_request_state_event(&message), "{method}");
            assert_eq!(message["id"], id);
        }
    }
    for method in [
        "item/agentMessage/delta",
        "item/commandExecution/outputDelta",
        "turn/started",
        "turn/completed",
        "thread/tokenUsage/updated",
    ] {
        let message = json!({"method": method, "params": {"delta": "x".repeat(128 * 1024)}});
        assert!(!codex_request_state_event(&message), "{method}");
    }
    assert!(!codex_request_state_event(&json!({"id": 17, "result": {}})));
    assert!(!codex_request_state_event(&json!({"method": null})));
}


#[test]
fn mobile_generic_pairing_rejects_chatgpt_and_preserves_codex_pairing() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
    let project = store.create_project("Mobile pairing".into(), None).unwrap();
    let workstream = store
        .create_workstream(&project.id, "Workstream".into())
        .unwrap();
    let core = RouterCore {
        
        session: Arc::new(Mutex::new(Session::default())),
        store: Arc::clone(&store),
    };
    let rejection = core
        .pair_mobile_workstream_endpoints(
            &workstream.id,
            EndpointPairingInput {
                expected_binding_revision: workstream.binding_revision,
                chatgpt: Some(EndpointPairingSideInput {
                    expected_active_endpoint_id: None,
                    external_id: "12345678-abcd".into(),
                    label: "must not persist".into(),
                }),
                codex: None,
            },
        )
        .unwrap_err();
    assert_eq!(
        rejection,
        "CHATGPT_REMOVED"
    );
    assert!(store
        .active_endpoint_for_workstream(&workstream.id, Provider::Chatgpt)
        .unwrap()
        .is_none());

    let result = core
        .pair_mobile_workstream_endpoints(
            &workstream.id,
            EndpointPairingInput {
                expected_binding_revision: workstream.binding_revision,
                chatgpt: None,
                codex: Some(EndpointPairingSideInput {
                    expected_active_endpoint_id: None,
                    external_id: "thread-mobile-exact".into(),
                    label: "verified Codex thread".into(),
                }),
            },
        )
        .unwrap();
    assert_eq!(
        result
            .codex_endpoint
            .as_ref()
            .map(|endpoint| endpoint.external_id.as_str()),
        Some("thread-mobile-exact")
    );
}



#[test]
fn provider_run_status_is_exact_and_never_substitutes_a_different_workstream_run() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
    let project = store
        .create_project("Provider status".into(), None)
        .unwrap();
    let workstream_a = store.create_workstream(&project.id, "A".into()).unwrap();
    let workstream_b = store.create_workstream(&project.id, "B".into()).unwrap();
    let endpoint_a = store
        .bind_endpoint(
            &workstream_a.id,
            Provider::Codex,
            "thread-a".into(),
            "Codex A".into(),
            false,
        )
        .unwrap();
    let endpoint_b = store
        .bind_endpoint(
            &workstream_b.id,
            Provider::Codex,
            "thread-b".into(),
            "Codex B".into(),
            false,
        )
        .unwrap();
    let run_a = store
        .create_provider_run(
            &workstream_a.id,
            &endpoint_a.id,
            "CODEX",
            None,
            Some("turn-a"),
            "RUNNING",
        )
        .unwrap();
    let run_b = store
        .create_provider_run(
            &workstream_b.id,
            &endpoint_b.id,
            "CODEX",
            None,
            Some("turn-b"),
            "RUNNING",
        )
        .unwrap();
    store
        .reconcile_provider_run(&run_a.id, "FAILED", "TEST_EXACT_FAILURE")
        .unwrap();
    store
        .reconcile_provider_run(&run_b.id, "CANCELLED", "TEST_OTHER_WORKSTREAM")
        .unwrap();
    let core = RouterCore {
        
        session: Arc::new(Mutex::new(Session::default())),
        store,
    };

    let status = core
        .mobile_provider_run_status(&workstream_a.id, &run_a.id)
        .unwrap();
    assert_eq!(status.run_id, run_a.id);
    assert_eq!(status.status, "FAILED");
    assert_eq!(status.terminal_code.as_deref(), Some("TEST_EXACT_FAILURE"));
    assert!(!status.has_reviewable_result);
    assert!(core
        .mobile_provider_run_status(&workstream_a.id, &run_b.id)
        .is_err());
}





fn observer_context() -> (tempfile::TempDir, RouterStore, Workstream, Endpoint) {
    let directory = tempfile::tempdir().unwrap();
    let store = RouterStore::open_at(directory.path().join("router.db")).unwrap();
    let project = store.create_project("Observer test".into(), None).unwrap();
    let workstream = store.create_workstream(&project.id, "WS".into()).unwrap();
    let endpoint = store
        .bind_endpoint(
            &workstream.id,
            Provider::Chatgpt,
            "conversation".into(),
            "ChatGPT".into(),
            false,
        )
        .unwrap();
    (directory, store, workstream, endpoint)
}

#[test]
fn deleted_bridge_completion_never_invokes_a_push_transport_and_restore_can_notify_future_results(){
 let(_d,store,w,endpoint)=observer_context();store.trash_workstream(&w.id).unwrap();let mut calls=0;let mut transport=|_:&[u8]|{calls+=1;Ok(crate::push::PushDeliveryOutcome::NoSubscription)};
 record_provider_surface_reply_with(&store,&endpoint,"while-deleted","audited result",&mut transport).unwrap();assert_eq!(calls,0);
 store.restore_workstream(&w.id).unwrap();let mut transport=|_:&[u8]|{calls+=1;Ok(crate::push::PushDeliveryOutcome::NoSubscription)};record_provider_surface_reply_with(&store,&endpoint,"after-restore","future result",&mut transport).unwrap();assert_eq!(calls,1);
}

#[test]
fn direct_carrier_terminal_identity_creates_one_observation_and_one_push_attempt() {
    let (_directory, store, workstream, endpoint) = observer_context();
    let mut push_attempts = 0;
    let mut no_subscription = |_payload: &[u8]| {
        push_attempts += 1;
        Ok(crate::push::PushDeliveryOutcome::NoSubscription)
    };

    assert!(record_provider_surface_reply_with(
        &store,
        &endpoint,
        "assistant-message-direct-1",
        "DIRECT_CARRIER_TERMINAL_REPLY",
        &mut no_subscription,
    )
    .unwrap());
    assert!(!record_provider_surface_reply_with(
        &store,
        &endpoint,
        "assistant-message-direct-1",
        "DIRECT_CARRIER_TERMINAL_REPLY",
        &mut no_subscription,
    )
    .unwrap());
    assert_eq!(push_attempts, 1);

    let observations = store
        .reply_observations_for_workstream(&workstream.id)
        .unwrap();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].endpoint_id, endpoint.id);
    assert_eq!(observations[0].text, "DIRECT_CARRIER_TERMINAL_REPLY");
    assert_eq!(observations[0].push_state, "NO_SUBSCRIPTION");
}

#[test]
fn codex_business_reply_push_uses_the_exact_observation_deep_link_without_reply_text() {
    let (_directory, store, workstream, _chatgpt_endpoint) = observer_context();
    let codex_endpoint = store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread-game-exact".into(),
            "当前游戏开发 Codex".into(),
            false,
        )
        .unwrap();
    let mut payloads = Vec::new();
    let mut sent = |payload: &[u8]| {
        payloads.push(payload.to_vec());
        Ok(crate::push::PushDeliveryOutcome::Sent {
            invalid_subscription_fingerprints: vec![],
        })
    };

    assert!(record_provider_surface_reply_with(
        &store,
        &codex_endpoint,
        "codex:turn-game:item-final",
        "PRIVATE_CODEX_REPLY_MUST_NOT_ENTER_PUSH",
        &mut sent,
    )
    .unwrap());
    assert_eq!(payloads.len(), 1);
    let payload: Value = serde_json::from_slice(&payloads[0]).unwrap();
    assert_eq!(payload["type"], "codex_reply");
    assert_eq!(payload["workstreamId"], workstream.id);
    assert_eq!(payload["workstreamName"], workstream.name);
    assert_eq!(payload["conversationName"], codex_endpoint.label);
    assert!(payload["observationId"]
        .as_str()
        .is_some_and(|id| !id.is_empty()));
    assert!(
        !String::from_utf8_lossy(&payloads[0]).contains("PRIVATE_CODEX_REPLY_MUST_NOT_ENTER_PUSH")
    );

    let observation = store
        .reply_observations_for_workstream(&workstream.id)
        .unwrap()
        .into_iter()
        .find(|item| item.endpoint_id == codex_endpoint.id)
        .unwrap();
    assert_eq!(observation.push_state, "SENT");
    assert_eq!(payload["observationId"], observation.id);
}

#[test]
fn completed_codex_event_creates_one_exact_observation_and_pushes_without_reply_text() {
    let (_directory, store, workstream, _chatgpt_endpoint) = observer_context();
    let codex_endpoint = store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread-event-observed".into(),
            "Event-observed Codex".into(),
            false,
        )
        .unwrap();
    let mut payloads = Vec::new();
    let mut sent = |payload: &[u8]| {
        payloads.push(payload.to_vec());
        Ok(crate::push::PushDeliveryOutcome::Sent {
            invalid_subscription_fingerprints: vec![],
        })
    };

    capture_completed_codex_result_with_push(
        &store,
        "thread-event-observed",
        "turn-event-observed",
        "item-event-observed".into(),
        "PRIVATE_EVENT_REPLY_MUST_NOT_ENTER_PUSH".into(),
        &mut sent,
    )
    .unwrap();
    capture_completed_codex_result_with_push(
        &store,
        "thread-event-observed",
        "turn-event-observed",
        "item-event-observed".into(),
        "PRIVATE_EVENT_REPLY_MUST_NOT_ENTER_PUSH".into(),
        &mut sent,
    )
    .unwrap();

    assert_eq!(payloads.len(), 1);
    let payload: Value = serde_json::from_slice(&payloads[0]).unwrap();
    assert_eq!(payload["type"], "codex_reply");
    assert_eq!(payload["workstreamId"], workstream.id);
    assert!(
        !String::from_utf8_lossy(&payloads[0]).contains("PRIVATE_EVENT_REPLY_MUST_NOT_ENTER_PUSH")
    );
    let observations = store
        .reply_observations_for_workstream(&workstream.id)
        .unwrap();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].endpoint_id, codex_endpoint.id);
    assert_eq!(observations[0].push_state, "SENT");
    assert_eq!(
        store
            .codex_reply_observer_watermark(&codex_endpoint.id)
            .unwrap()
            .unwrap()
            .0,
        "codex-watermark:thread-event-observed:turn-event-observed:item-event-observed"
    );
}

#[test]
fn background_codex_observer_skips_uninitialized_historical_endpoints() {
    let (_directory, store, workstream, _chatgpt_endpoint) = observer_context();
    let historical = store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread-historical".into(),
            "Historical Codex".into(),
            false,
        )
        .unwrap();
    let current_project = store
        .create_project("Current observer project".into(), None)
        .unwrap();
    let current_workstream = store
        .create_workstream(&current_project.id, "Current observer workstream".into())
        .unwrap();
    let current = store
        .bind_endpoint(
            &current_workstream.id,
            Provider::Codex,
            "thread-current-observer".into(),
            "Current Codex".into(),
            false,
        )
        .unwrap();
    store
        .save_codex_reply_observer_empty_baseline(
            &current_workstream.id,
            &current.id,
            "thread-current-observer",
        )
        .unwrap();

    let observed = active_initialized_codex_observer_workstreams(&store).unwrap();
    assert_eq!(observed, vec![current_workstream.id]);
    assert!(!store
        .codex_reply_observer_is_initialized(&historical.id)
        .unwrap());
}




/// Opt-in real contract proof for the passive Codex observer.  It keeps
/// the owner's bound thread out of scope: a fresh persistent thread is
/// bound only to a temporary SQLite store, receives one short turn, and
/// is read from a second official app-server client.  The pre-turn empty
/// baseline models the Host's durable first-observer checkpoint.
#[test]
#[ignore = "requires explicit authorized disposable Codex observation E2E"]
fn real_codex_existing_thread_observer_e2e() {
    assert_eq!(
        std::env::var("AIWR_REAL_CODEX_OBSERVER_E2E").as_deref(),
        Ok("1"),
        "set AIWR_REAL_CODEX_OBSERVER_E2E=1 to authorize the one disposable Codex turn"
    );
    let scratch = std::env::temp_dir().join("aiwr-observer-e2e");
    std::fs::create_dir_all(&scratch).unwrap();
    let directory = tempfile::tempdir_in(scratch).expect("temporary observer workspace");
    let store = Arc::new(
        RouterStore::open_at(directory.path().join("router-observer-e2e.db"))
            .expect("temporary observer store"),
    );
    let project = store
        .create_project("AIWR disposable Codex observer E2E".into(), None)
        .expect("temporary project");
    let workstream = store
        .create_workstream(&project.id, "Passive observer".into())
        .expect("temporary workstream");
    let no_events: Arc<dyn Fn(&Value) + Send + Sync> = Arc::new(|_| {});
    let mut producer =
        CodexAdapter::start_validation(Arc::clone(&no_events)).expect("producer app-server starts");
    producer.initialize().expect("producer initializes");
    let catalog = producer
        .request("model/list", json!({"limit":100,"includeHidden":false}))
        .unwrap();
    let validation_model = catalog["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["model"] == "gpt-6-luna")
        .expect("lowest sufficient current Luna model is reported");
    assert!(validation_model["supportedReasoningEfforts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|effort| effort["reasoningEffort"] == "low"));
    let target = start_verified_codex_thread(&mut producer, directory.path())
        .expect("fresh persistent thread starts");
    let endpoint = store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            target.id.clone(),
            "disposable observer target".into(),
            false,
        )
        .expect("temporary exact Codex endpoint");
    // No completed turn exists yet. Persisting this empty baseline is what
    // makes the sole following reply new rather than historic.
    store
        .save_codex_reply_observer_empty_baseline(&workstream.id, &endpoint.id, &target.id)
        .expect("empty baseline persists");
    let marker = format!("AIWR_CODEX_OBSERVER_{}", Uuid::new_v4().simple());
    let started = producer
            .request(
                "turn/start",
                json!({
                    "threadId": target.id,
                    "model": "gpt-6-luna", "effort": "low",
                    "input": [{"type": "text", "text": format!("Reply with exactly {marker}. Do not use tools or modify files.")}],
                }),
            )
            .expect("one disposable turn starts");
    let turn_id = started
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .expect("turn id")
        .to_string();
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    let mut last_transient_read_error = None;
    loop {
        let latest = match producer
                .request(
                    "thread/turns/list",
                    json!({"threadId": target.id, "cursor": Value::Null, "limit": 1, "sortDirection": "desc", "itemsView": "notLoaded"}),
                ) {
                Ok(latest) => latest,
                // `thread/start` can return before the CLI's new persisted
                // rollout has received its metadata. This is a temporary
                // server-side availability race on our disposable test
                // thread, not a reason to issue a second write or to touch a
                // user-owned thread.
                Err(error)
                    if error.contains("failed to read session metadata")
                        && error.contains("rollout")
                        && error.contains("is empty")
                        && std::time::Instant::now() < deadline =>
                {
                    last_transient_read_error = Some(error);
                    std::thread::sleep(Duration::from_millis(250));
                    continue;
                }
                Err(error)
                    if (error.contains("thread/turns/list is not supported yet")
                        || error.contains("list_turns is not supported yet"))
                        && std::time::Instant::now() < deadline =>
                {
                    last_transient_read_error = Some(error);
                    std::thread::sleep(Duration::from_millis(250));
                    continue;
                }
                Err(error) => panic!("producer polls only its disposable latest turn: {error}"),
            };
        if latest.pointer("/data/0/id").and_then(Value::as_str) == Some(turn_id.as_str())
            && latest.pointer("/data/0/status").and_then(Value::as_str) == Some("completed")
        {
            break;
        }
        assert!(
                std::time::Instant::now() < deadline,
                "disposable Codex turn timed out; latest turn snapshot: {latest}; latest transient read error: {last_transient_read_error:?}"
            );
        std::thread::sleep(Duration::from_millis(250));
    }

    let mut observer =
        CodexAdapter::start_validation(no_events).expect("second observer app-server starts");
    observer.initialize().expect("second observer initializes");
    let session = Arc::new(Mutex::new(Session::default()));
    session.lock().unwrap().adapter = Some(observer);
    let mut no_subscription =
        |_payload: &[u8]| Ok(crate::push::PushDeliveryOutcome::NoSubscription);
    // The official server can publish `turn/completed` before its bounded
    // item projection is queryable from a second fresh client. This is an
    // observer-read availability wait, never a Provider retry or write.
    let observation_deadline = std::time::Instant::now() + Duration::from_secs(30);
    let first = loop {
        match check_new_codex_replies_with(&store, &session, &workstream.id, &mut no_subscription) {
            Ok(result) => break result,
            Err(error)
                if error.contains("thread/items/list is not supported yet")
                    && std::time::Instant::now() < observation_deadline =>
            {
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(error) => panic!("second client creates one external observation: {error}"),
        }
    };
    assert_eq!(first.state, "NEW_REPLY_OBSERVED");
    assert!(first.observation_created);
    let replies = store
        .reply_observations_for_workstream(&workstream.id)
        .unwrap();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].text.trim(), marker);
    assert_eq!(
        store
            .provider_runs_for_workstream(&workstream.id)
            .unwrap()
            .len(),
        0
    );

    // Simulate a fresh observer session. The durable exact watermark, not
    // reply text, prevents a second alert or ProviderRun.
    let replacement = CodexAdapter::start_validation(Arc::new(|_| {}))
        .expect("restart observer app-server starts");
    let mut replacement = replacement;
    replacement
        .initialize()
        .expect("restart observer initializes");
    session.lock().unwrap().adapter = Some(replacement);
    let second =
        check_new_codex_replies_with(&store, &session, &workstream.id, &mut no_subscription)
            .expect("restart observer checks the same exact reply");
    assert_eq!(second.state, "NO_NEW_TERMINAL_REPLY");
    assert!(!second.observation_created);
    assert_eq!(
        store
            .reply_observations_for_workstream(&workstream.id)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store
            .provider_runs_for_workstream(&workstream.id)
            .unwrap()
            .len(),
        0
    );
    let counts = session
        .lock()
        .unwrap()
        .adapter
        .as_ref()
        .unwrap()
        .request_counts();
    assert_eq!(counts.get("thread/resume"), None);
    assert_eq!(counts.get("turn/start"), None);
    assert_eq!(counts.get("thread/goal/get"), None);
    assert_eq!(counts.get("thread/goal/set"), None);
    assert_eq!(counts.get("thread/goal/clear"), None);
}

#[test]
fn current_reply_projection_excludes_superseded_endpoint_observations_but_keeps_history() {
    let (_directory, store, workstream, endpoint_e1) = observer_context();
    let store = Arc::new(store);
    let reply_e1 = store
        .record_reply_observation(
            &workstream.id,
            &endpoint_e1.id,
            Some("message-e1"),
            "E1 historical observation",
            None,
        )
        .unwrap()
        .unwrap();
    let endpoint_e2 = store
        .bind_endpoint(
            &workstream.id,
            Provider::Chatgpt,
            "conversation-e2".into(),
            "Replacement".into(),
            true,
        )
        .unwrap();
    let core = RouterCore {
        
        session: Arc::new(Mutex::new(Session::default())),
        store: store.clone(),
    };

    // E1 remains durable lineage/history but cannot project as E2 current.
    let persisted = store
        .reply_observations_for_workstream(&workstream.id)
        .unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].id, reply_e1.id);
    assert!(core
        .mobile_reply_observations(&workstream.id)
        .unwrap()
        .is_empty());

    let reply_e2 = store
        .record_reply_observation(
            &workstream.id,
            &endpoint_e2.id,
            Some("message-e2"),
            "E2 current observation",
            None,
        )
        .unwrap()
        .unwrap();
    let current = core.mobile_reply_observations(&workstream.id).unwrap();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].id, reply_e2.id);
    assert_eq!(current[0].text, "E2 current observation");
    let persisted = store
        .reply_observations_for_workstream(&workstream.id)
        .unwrap();
    assert!(persisted.iter().any(|item| item.id == reply_e1.id));
    assert!(persisted.iter().any(|item| item.id == reply_e2.id));
}

#[test]
fn mobile_reply_projection_keeps_current_exact_codex_observations() {
    let (_directory, store, workstream, endpoint_chatgpt) = observer_context();
    let store = Arc::new(store);
    let endpoint_codex = store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread-current".into(),
            "Current Codex".into(),
            true,
        )
        .unwrap();
    let chatgpt_reply = store
        .record_reply_observation(
            &workstream.id,
            &endpoint_chatgpt.id,
            Some("message-current"),
            "Current ChatGPT observation",
            None,
        )
        .unwrap()
        .unwrap();
    let codex_reply = store
        .record_reply_observation(
            &workstream.id,
            &endpoint_codex.id,
            Some("turn-current:item-current"),
            "Current Codex observation",
            None,
        )
        .unwrap()
        .unwrap();
    let core = RouterCore {
        
        session: Arc::new(Mutex::new(Session::default())),
        store,
    };

    let current = core.mobile_reply_observations(&workstream.id).unwrap();
    let current_ids = current
        .into_iter()
        .map(|item| item.id)
        .collect::<HashSet<_>>();
    assert_eq!(
        current_ids,
        HashSet::from([chatgpt_reply.id, codex_reply.id])
    );
}














#[test]
fn reverse_attachment_staging_commits_only_a_complete_verified_set() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    let capture = directory.path().join("capture");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&capture).unwrap();
    let source = capture.join("evidence.txt");
    fs::write(&source, b"verified attachment\n").unwrap();
    let hash = sha256_path(&source).unwrap();
    let mut attachments = vec![crate::relay_model::HandoffAttachment {
        id: "attachment-1".into(),
        path: source.display().to_string(),
        filename: "evidence.txt".into(),
        actual_sha256: Some(hash.clone()),
        integrity_status: Some("VERIFIED".into()),
    }];

    stage_reverse_attachment_files(&workspace, "handoff-1", &mut attachments).unwrap();

    let staged = workspace
        .join(".aiwr")
        .join("incoming")
        .join("handoff-1")
        .join("evidence.txt");
    assert!(staged.is_file());
    assert_eq!(sha256_path(&staged).unwrap(), hash);
    assert_eq!(attachments[0].path, staged.display().to_string());
}

#[test]
fn reverse_attachment_staging_rejects_an_invalid_set_before_any_target_is_visible() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    let capture = directory.path().join("capture");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&capture).unwrap();
    let source = capture.join("evidence.txt");
    fs::write(&source, b"verified attachment\n").unwrap();
    let hash = sha256_path(&source).unwrap();
    let attachment = || crate::relay_model::HandoffAttachment {
        id: Uuid::new_v4().to_string(),
        path: source.display().to_string(),
        filename: "evidence.txt".into(),
        actual_sha256: Some(hash.clone()),
        integrity_status: Some("VERIFIED".into()),
    };
    let mut attachments = vec![attachment(), attachment()];

    let error =
        stage_reverse_attachment_files(&workspace, "handoff-2", &mut attachments).unwrap_err();

    assert_eq!(error, "Invalid or duplicate reverse attachment filename");
    assert!(!workspace
        .join(".aiwr")
        .join("incoming")
        .join("handoff-2")
        .exists());
}




#[test]
fn persisted_attachment_keeps_review_integrity_distinct_from_actual_sha256() {
    let directory = tempfile::tempdir().unwrap();
    let store = RouterStore::open_at(directory.path().join("router.db")).unwrap();
    let project = store
        .create_project("Attachment audit".into(), None)
        .unwrap();
    let workstream = store
        .create_workstream(&project.id, "WS-ATTACHMENT".into())
        .unwrap();
    let source = store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread-attachment".into(),
            "Codex".into(),
            false,
        )
        .unwrap();
    let destination = store
        .bind_endpoint(
            &workstream.id,
            Provider::Chatgpt,
            "conversation-attachment".into(),
            "ChatGPT".into(),
            false,
        )
        .unwrap();
    let attachment =
        |filename: &str, integrity_status: &str| crate::relay_model::HandoffAttachment {
            id: filename.into(),
            path: format!("D:\\review\\{filename}"),
            filename: filename.into(),
            actual_sha256: Some(format!("actual-{integrity_status}")),
            integrity_status: Some(integrity_status.into()),
        };
    let handoff = store
        .create_ready_handoff(NewHandoff {
            workstream_id: workstream.id,
            source_endpoint_id: source.id,
            destination_endpoint_id: destination.id,
            direction: "CODEX_TO_CHATGPT".into(),
            source_response_identity: None,
            original_text: "original".into(),
            approved_text: "approved".into(),
            attachments: [
                attachment("mismatch.txt", "MISMATCH"),
                attachment("verified.txt", "VERIFIED"),
                attachment("undeclared.txt", "NOT_PROVIDED"),
            ]
            .iter()
            .map(persisted_attachment)
            .collect(),
        })
        .unwrap();
    let persisted = store.handoff_by_id(&handoff.id).unwrap();
    for (filename, integrity_status) in [
        ("mismatch.txt", "MISMATCH"),
        ("verified.txt", "VERIFIED"),
        ("undeclared.txt", "NOT_PROVIDED"),
    ] {
        let record = persisted
            .attachments
            .iter()
            .find(|record| record.filename == filename)
            .unwrap();
        assert_eq!(record.integrity_status.as_deref(), Some(integrity_status));
        assert_eq!(
            record.sha256.as_deref(),
            Some(format!("actual-{integrity_status}").as_str())
        );
    }
}

#[test]
fn write_readiness_is_scoped_to_the_current_app_server_session() {
    let thread_id = "thread-a";
    let mut ready_threads = HashSet::new();
    assert!(thread_needs_write_acquisition(&ready_threads, thread_id));

    ready_threads.insert(thread_id.to_string());
    assert!(!thread_needs_write_acquisition(&ready_threads, thread_id));

    ready_threads.clear();
    assert!(thread_needs_write_acquisition(&ready_threads, thread_id));
}

#[test]
fn write_readiness_errors_are_human_actionable() {
    assert_eq!(
        thread_write_readiness_error("thread not found: thread-a".into()),
        "Persisted Codex binding is no longer readable. Replace the binding manually."
    );
    assert_eq!(
            thread_write_readiness_error("writer conflict for thread-a".into()),
            "Codex thread is currently owned by another application. Close/release it there, then retry."
        );
    assert_eq!(
        thread_write_readiness_error("unexpected transport failure".into()),
        "unexpected transport failure"
    );
}

#[test]
fn read_only_history_requires_the_exact_persisted_thread_identity() {
    let matching = json!({
        "thread": {
            "id": "thread-a",
            "turns": [{ "id": "turn-a", "items": [{
                "id": "message-a", "type": "agentMessage", "text": "history"
            }]}]
        }
    });
    let history = exact_read_history("thread-a", &matching).unwrap();
    assert_eq!(history.thread.id, "thread-a");
    assert_eq!(history.history.len(), 1);
    assert!(exact_read_history("thread-b", &matching).is_err());
    assert!(exact_read_history("thread-a", &json!({})).is_err());
}

#[test]
fn rollover_verification_requires_exact_completed_turn_and_result_identity() {
    let read = json!({
        "thread": {
            "id": "successor-thread",
            "turns": [{
                "id": "initialization-turn",
                "status": "completed",
                "items": [{ "id": "result-item", "type": "agentMessage", "text": "V0_008_SUCCESSOR_READY" }]
            }]
        }
    });
    assert_eq!(
        verified_rollover_result_identity("successor-thread", "initialization-turn", &read)
            .unwrap(),
        "result-item"
    );
    assert!(
        verified_rollover_result_identity("other-thread", "initialization-turn", &read).is_err()
    );
    assert!(verified_rollover_result_identity("successor-thread", "other-turn", &read).is_err());

    let unfinished = json!({
        "thread": { "id": "successor-thread", "turns": [{ "id": "initialization-turn", "status": "inProgress", "items": [] }] }
    });
    assert!(verified_rollover_result_identity(
        "successor-thread",
        "initialization-turn",
        &unfinished
    )
    .is_err());

    let missing_result = json!({
        "thread": { "id": "successor-thread", "turns": [{ "id": "initialization-turn", "status": "completed", "items": [{ "id": "user-item", "type": "userMessage" }] }] }
    });
    assert!(verified_rollover_result_identity(
        "successor-thread",
        "initialization-turn",
        &missing_result
    )
    .is_err());
}


#[test]
fn structured_codex_response_uses_only_the_exact_offered_one_time_decision() {
    let request = PendingCodexRequest {
        action_id: "opaque".into(),
        raw_request_id: json!(7),
        method: "item/commandExecution/requestApproval".into(),
        thread_id: "thread".into(),
        turn_id: "turn".into(),
        item_id: Some("item".into()),
        params: json!({"availableDecisions":["accept","acceptForSession","decline"],"reason":"output a local test marker"}),
        revision: 1,
        responded: false,
    };
    assert_eq!(
        server_request_response_result(
            &request,
            MobileCodexResponseInput {
                revision: 1,
                decision: Some("accept".into()),
                answers: None,
            }
        )
        .unwrap(),
        json!({"decision":"accept"})
    );
    assert!(server_request_response_result(
        &request,
        MobileCodexResponseInput {
            revision: 1,
            decision: Some("acceptForSession".into()),
            answers: None,
        }
    )
    .is_err());
    let legacy = PendingCodexRequest {
        params: json!({}),
        ..request.clone()
    };
    let projection = mobile_codex_request_projection(&legacy).unwrap();
    assert_eq!(projection.revision, 1);
    assert!(projection.reason.is_none());
    assert_eq!(
        projection
            .choices
            .iter()
            .map(|choice| choice.id.as_str())
            .collect::<Vec<_>>(),
        vec!["accept", "decline", "cancel"]
    );
    assert_eq!(
        server_request_response_result(
            &legacy,
            MobileCodexResponseInput {
                revision: 1,
                decision: Some("accept".into()),
                answers: None,
            }
        )
        .unwrap(),
        json!({"decision":"accept"})
    );
    assert!(server_request_response_result(
        &legacy,
        MobileCodexResponseInput {
            revision: 1,
            decision: Some("acceptForSession".into()),
            answers: None,
        }
    )
    .is_err());
    let reason_projection = mobile_codex_request_projection(&request).unwrap();
    assert_eq!(
        reason_projection.reason.as_deref(),
        Some("output a local test marker")
    );
}

#[test]
fn question_projection_answers_skip_and_resolution_preserve_native_identity(){
 let request=PendingCodexRequest{action_id:"opaque".into(),raw_request_id:json!("native-string-id"),method:"item/tool/requestUserInput".into(),thread_id:"exact".into(),turn_id:"turn".into(),item_id:Some("item".into()),params:json!({"questions":[{"id":"scope","header":"Scope","question":"Choose the scope?","isOther":true,"isSecret":false,"options":[{"label":"Small","description":"Bounded"}]}]}),revision:4,responded:false};
 let p=mobile_codex_request_projection(&request).unwrap();assert_eq!(p.questions[0].options[0].description,"Bounded");assert!(p.questions[0].is_other);assert_eq!(p.questions[0].placeholder.as_deref(),Some("Choose the scope?"));
 assert_eq!(server_request_response_result(&request,MobileCodexResponseInput{revision:4,decision:None,answers:Some(HashMap::from([("scope".into(),"Small".into())]))}).unwrap(),json!({"answers":{"scope":{"answers":["Small"]}}}));
 assert_eq!(server_request_response_result(&request,MobileCodexResponseInput{revision:4,decision:Some("skip".into()),answers:None}).unwrap(),json!({"answers":{}}));
 let session=Arc::new(Mutex::new(Session::default()));let frame=json!({"id":request.raw_request_id,"method":request.method,"params":{"threadId":"exact","turnId":"turn","itemId":"item","questions":[{"id":"scope","question":"Scope?"}]}});
 capture_pending_codex_request(&session,&frame);capture_pending_codex_request(&session,&frame);assert_eq!(session.lock().unwrap().pending_codex_requests.len(),1);
 clear_resolved_codex_request(&session,&json!({"method":"serverRequest/resolved","params":{"threadId":"exact","requestId":"native-string-id"}}));capture_pending_codex_request(&session,&frame);assert!(session.lock().unwrap().pending_codex_requests.is_empty());
}



#[test]
fn resolved_request_clears_only_matching_typed_id_and_thread() {
    let session = Arc::new(Mutex::new(Session::default()));
    capture_pending_codex_request(
        &session,
        &json!({
            "id": 7, "method":"item/fileChange/requestApproval",
            "params":{"threadId":"thread-a","turnId":"turn-a","itemId":"item-a"}
        }),
    );
    clear_resolved_codex_request(
        &session,
        &json!({
            "method":"serverRequest/resolved", "params":{"threadId":"thread-a","requestId":"7"}
        }),
    );
    assert_eq!(session.lock().unwrap().pending_codex_requests.len(), 1);
    clear_resolved_codex_request(
        &session,
        &json!({
            "method":"serverRequest/resolved", "params":{"threadId":"thread-b","requestId":7}
        }),
    );
    assert_eq!(session.lock().unwrap().pending_codex_requests.len(), 1);
    clear_resolved_codex_request(
        &session,
        &json!({
            "method":"serverRequest/resolved", "params":{"threadId":"thread-a","requestId":7}
        }),
    );
    assert!(session.lock().unwrap().pending_codex_requests.is_empty());
}

#[test]
fn codex_goal_projection_requires_the_exact_thread_and_preserves_reported_usage() {
    let response = json!({
        "goal": {
            "threadId": "thread-a",
            "objective": "Finish the approved work",
            "status": "active",
            "tokenBudget": null,
            "tokensUsed": 17,
            "timeUsedSeconds": 23,
            "createdAt": 100,
            "updatedAt": 101
        }
    });
    let goal = codex_goal_from_response("thread-a", &response)
        .unwrap()
        .unwrap();
    assert_eq!(goal.objective, "Finish the approved work");
    assert_eq!(goal.status, "active");
    assert_eq!(goal.token_budget, None);
    assert_eq!(goal.tokens_used, Some(17));
    assert_eq!(goal.time_used_seconds, Some(23));
    assert!(codex_goal_from_response("thread-b", &response).is_err());
    assert!(
        codex_goal_from_response("thread-a", &json!({ "goal": null }))
            .unwrap()
            .is_none()
    );
}

#[test]
fn goal_state_transitions_and_turn_interrupt_are_separate_controls() {
    assert!(ensure_goal_transition("active", "paused").is_ok());
    assert!(ensure_goal_transition("paused", "active").is_ok());
    assert!(ensure_goal_transition("paused", "paused").is_err());
    assert!(ensure_goal_transition("complete", "active").is_err());
    assert!(require_goal_confirmation(false).is_err());
    assert!(require_goal_confirmation(true).is_ok());
}



#[test]
fn borrowed_codex_transport_blocks_background_and_explicit_second_connector(){
 let dir=tempfile::tempdir().unwrap();let core=RouterCore{store:Arc::new(RouterStore::open_at(dir.path().join("router.db")).unwrap()),session:Arc::new(Mutex::new(Session{codex_adapter_borrowed:true,..Default::default()})),};
 let sink=Arc::new(router_core::events::NullEventSink);connect_codex_for_resident_host(sink.clone(),&core);let result=connect_codex_service(&core,sink).unwrap();assert!(!result.connected);assert!(!core.session.lock().unwrap().connecting);assert!(core.session.lock().unwrap().codex_adapter_borrowed);
}


#[test]
fn mobile_global_index_preserves_trash_and_readable_endpoint_context(){
 let d=tempfile::tempdir().unwrap();let store=Arc::new(RouterStore::open_at(d.path().join("router.db")).unwrap());
 let first=store.create_project("first group".into(),None).unwrap();let second=store.create_project("second group".into(),None).unwrap();
 let active=store.create_workstream(&first.id,"active".into()).unwrap();let trashed=store.create_workstream(&second.id,"trash".into()).unwrap();
 store.bind_endpoint(&active.id,Provider::Codex,"exact-thread-id".into(),"human dialogue label".into(),false).unwrap();store.trash_workstream(&trashed.id).unwrap();
 let core=RouterCore{store,session:Arc::new(Mutex::new(Session::default())),};
 let items=core.mobile_workstreams().unwrap();let visible=items.iter().find(|i|i.id==active.id).unwrap();let deleted=items.iter().find(|i|i.id==trashed.id).unwrap();
 assert_eq!(visible.source_label,"Codex · human dialogue label");assert_eq!(visible.project_name,"first group");assert!(deleted.trashed_at.is_some());assert_eq!(deleted.project_name,"second group");assert_ne!(visible.id,deleted.id);
}

#[test]fn disabled_push_category_records_skipped_without_replaying_existing_results(){
 let(_d,store,w,endpoint)=observer_context();let mut calls=0;let mut send=|_:&[u8]|{calls+=1;Ok(crate::push::PushDeliveryOutcome::Suppressed)};
 assert!(record_provider_surface_reply_with(&store,&endpoint,"fixture-category-off","preserved result",&mut send).unwrap());
 assert!(!record_provider_surface_reply_with(&store,&endpoint,"fixture-category-off","preserved result",&mut send).unwrap());
 assert_eq!(calls,1);let items=store.reply_observations_for_workstream(&w.id).unwrap();assert_eq!(items.len(),1);assert_eq!(items[0].push_state,"SKIPPED");assert_eq!(items[0].text,"preserved result");
}
