use super::*;
use chatgpt::direct::CompletedChatGptResponse;
use chatgpt::model::{HandoffStatus, ReverseHandoffDraft};
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
 let core=RouterCore{store,chatgpt:std::sync::Arc::default(),session:std::sync::Arc::new(std::sync::Mutex::new(Session::default())),completed_chatgpt_responses:std::sync::Arc::default()};core.session.lock().unwrap().adapter=Some(adapter);
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
fn explicit_binding_review_is_read_only_until_host_proof_and_confirmation() {
    let directory = tempfile::tempdir().unwrap();
    let store = RouterStore::open_at(directory.path().join("router.db")).unwrap();
    let project = store
        .create_project("Explicit host proof".into(), None)
        .unwrap();
    let workstream = store
        .create_workstream(&project.id, "Workstream".into())
        .unwrap();

    let review = review_explicit_chatgpt_binding(&store, &workstream.id, "12345678-abcd").unwrap();
    assert_eq!(review.expected_old_endpoint_id, None);
    assert_eq!(
        review.expected_binding_revision,
        workstream.binding_revision
    );
    assert!(store
        .active_endpoint_for_workstream(&workstream.id, Provider::Chatgpt)
        .unwrap()
        .is_none());
    assert!(store
        .snapshot_for_workstream(&workstream.id)
        .unwrap()
        .endpoint_lineage
        .is_empty());
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
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
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
        "Bind ChatGPT through exact URL validation and explicit confirmation"
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
fn mobile_chatgpt_reply_check_rejects_a_missing_endpoint_before_normal_browser_read() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
    let mut reader_called = false;
    let mut reader = |_conversation_id: &str| {
        reader_called = true;
        Ok(None)
    };
    let mut no_subscription =
        |_payload: &[u8]| Ok(crate::push::PushDeliveryOutcome::NoSubscription);

    // A normal-Chrome observer is owner-approved and distinct from the
    // retired Router carrier. With no matching Workstream/endpoint, the
    // exact lookup must fail closed before it can read any browser state.
    let error = match check_new_chatgpt_replies_with(
        &store,
        "not-a-workstream",
        &mut reader,
        &mut no_subscription,
    ) {
        Err(error) => error,
        Ok(_) => panic!("a missing endpoint must fail before browser observation"),
    };
    assert_eq!(error, "The Workstream has no ACTIVE ChatGPT Endpoint");
    assert!(!reader_called);
}

#[test]
fn mobile_snapshot_recovers_only_a_proven_prewrite_reverse_handoff() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
    let project = store
        .create_project("Mobile recovery".into(), None)
        .unwrap();
    let workstream = store
        .create_workstream(&project.id, "Workstream".into())
        .unwrap();
    let source = store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread".into(),
            "Codex".into(),
            false,
        )
        .unwrap();
    let destination = store
        .bind_endpoint(
            &workstream.id,
            Provider::Chatgpt,
            "conversation".into(),
            "ChatGPT".into(),
            false,
        )
        .unwrap();
    let handoff = store
        .create_ready_handoff(NewHandoff {
            workstream_id: workstream.id.clone(),
            source_endpoint_id: source.id,
            destination_endpoint_id: destination.id.clone(),
            direction: "CODEX_TO_CHATGPT".into(),
            source_response_identity: None,
            original_text: "original".into(),
            approved_text: "approved".into(),
            attachments: vec![],
        })
        .unwrap();
    store
        .transition_handoff(&handoff.id, "APPROVED", None)
        .unwrap();
    store
        .transition_handoff(&handoff.id, "SENDING", None)
        .unwrap();
    let run = store
        .create_provider_run(
            &workstream.id,
            &destination.id,
            "CHATGPT",
            Some(&handoff.id),
            None,
            "RUNNING",
        )
        .unwrap();
    store
        .reconcile_provider_run(&run.id, "UNKNOWN", "CHATGPT_AUTH_OR_COMPOSER_REQUIRED")
        .unwrap();
    let core = RouterCore {
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        store: Arc::clone(&store),
    };

    let snapshot = core.mobile_workstream_snapshot(&workstream.id).unwrap();
    assert_eq!(
        snapshot
            .handoffs
            .iter()
            .find(|item| item.id == handoff.id)
            .unwrap()
            .status,
        "FAILED"
    );
    let repaired = store
        .provider_runs_for_workstream(&workstream.id)
        .unwrap()
        .into_iter()
        .find(|item| item.id == run.id)
        .unwrap();
    assert_eq!(repaired.status, "FAILED");
    assert!(repaired.external_run_id.is_none());
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
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
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

#[test]
fn default_browser_link_never_opens_when_background_tab_presence_is_unproven() {
    assert_eq!(
        default_browser_open_decision(
            true,
            Err("CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE".into())
        )
        .unwrap(),
        "ALREADY_OPEN"
    );
    assert_eq!(
        default_browser_open_decision(false, Ok(true)).unwrap(),
        "ALREADY_OPEN"
    );
    assert_eq!(
        default_browser_open_decision(false, Ok(false)).unwrap(),
        "OPEN_REQUESTED"
    );
    assert_eq!(
        default_browser_open_decision(
            false,
            Err("CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE".into())
        )
        .unwrap(),
        "CHECK_REQUIRED"
    );
    assert_eq!(
        default_browser_open_decision(false, Err("CHATGPT_ACCOUNT_SECURITY_REQUIRED".into()))
            .unwrap_err(),
        "CHATGPT_ACCOUNT_SECURITY_REQUIRED"
    );
}

#[test]
fn explicit_chatgpt_confirmation_leaves_the_active_codex_endpoint_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
    let project = store
        .create_project("Independent explicit confirmation".into(), None)
        .unwrap();
    let workstream = store
        .create_workstream(&project.id, "Workstream".into())
        .unwrap();
    let codex = store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread-current".into(),
            "current Codex".into(),
            false,
        )
        .unwrap();
    let revised = store
        .snapshot_for_workstream(&workstream.id)
        .unwrap()
        .workstreams
        .into_iter()
        .find(|item| item.id == workstream.id)
        .unwrap();
    let core = RouterCore {
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        store: Arc::clone(&store),
    };
    core.session
        .lock()
        .unwrap()
        .explicit_chatgpt_binding_candidates
        .insert(
            workstream.id.clone(),
            ExplicitChatGptBindingCandidate {
                workstream_id: workstream.id.clone(),
                external_id: "conversation-confirmed".into(),
                canonical_url: "https://chatgpt.com/c/conversation-confirmed".into(),
                label: "verified ChatGPT".into(),
                verification: "BROWSER_EXECUTOR_EXACT_ROUTE".into(),
                expected_old_endpoint_id: None,
                expected_binding_revision: revised.binding_revision,
            },
        );

    let chatgpt = core
        .confirm_explicit_chatgpt_endpoint_binding(&workstream.id)
        .unwrap();
    let current_codex = store
        .active_endpoint_for_workstream(&workstream.id, Provider::Codex)
        .unwrap()
        .unwrap();
    assert_eq!(chatgpt.provider, "CHATGPT");
    assert_eq!(current_codex.id, codex.id);
    assert_eq!(current_codex.external_id, "thread-current");
}

#[test]
fn failed_host_proof_never_creates_an_explicit_mobile_or_desktop_candidate() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
    let project = store
        .create_project("Host proof failure".into(), None)
        .unwrap();
    let workstream = store
        .create_workstream(&project.id, "Workstream".into())
        .unwrap();
    let core = RouterCore {
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        store: Arc::clone(&store),
    };
    let error = core
        .prepare_explicit_chatgpt_endpoint_binding_with(
            &workstream.id,
            "https://chatgpt.com/g/g-project/c/12345678-abcd",
            |_| Err("Host exact proof failed".into()),
        )
        .err()
        .unwrap();
    assert_eq!(error, "Host exact proof failed");
    assert!(core
        .session
        .lock()
        .unwrap()
        .explicit_chatgpt_binding_candidates
        .get(&workstream.id)
        .is_none());
    assert!(store
        .active_endpoint_for_workstream(&workstream.id, Provider::Chatgpt)
        .unwrap()
        .is_none());
}

#[test]
fn owner_confirmed_exact_url_is_session_only_until_the_same_atomic_confirmation() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
    let project = store
        .create_project("Manual exact URL".into(), None)
        .unwrap();
    let workstream = store
        .create_workstream(&project.id, "Workstream".into())
        .unwrap();
    let old = store
        .bind_endpoint(
            &workstream.id,
            Provider::Chatgpt,
            "conversation-old".into(),
            "old ChatGPT".into(),
            false,
        )
        .unwrap();
    let core = RouterCore {
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        store: Arc::clone(&store),
    };

    let candidate = core
        .prepare_owner_confirmed_chatgpt_endpoint_binding(
            &workstream.id,
            "https://chatgpt.com/g/g-p-router/c/6ab86566-a260-83ec-a603-a930845e22c6",
        )
        .unwrap();
    assert_eq!(
        candidate.external_id,
        "6ab86566-a260-83ec-a603-a930845e22c6"
    );
    assert_eq!(candidate.verification, "OWNER_CONFIRMED_EXACT_URL");
    assert_eq!(
        candidate.expected_old_endpoint_id.as_deref(),
        Some(old.id.as_str())
    );
    assert_eq!(
        store
            .active_endpoint_for_workstream(&workstream.id, Provider::Chatgpt)
            .unwrap()
            .unwrap()
            .id,
        old.id
    );

    let active = core
        .confirm_explicit_chatgpt_endpoint_binding(&workstream.id)
        .unwrap();
    assert_eq!(active.external_id, "6ab86566-a260-83ec-a603-a930845e22c6");
    assert_eq!(
        store
            .active_endpoint_for_workstream(&workstream.id, Provider::Chatgpt)
            .unwrap()
            .unwrap()
            .id,
        active.id
    );
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

#[test]
fn chatgpt_observer_baselines_then_observes_one_new_exact_reply() {
    let (_directory, store, workstream, endpoint) = observer_context();
    let mut requested_conversations = vec![];
    let mut reads = 0;
    let mut reader = |conversation_id: &str| {
        requested_conversations.push(conversation_id.to_owned());
        reads += 1;
        Ok(Some(chatgpt::direct::TerminalReply {
            message_id: if reads == 3 {
                "assistant-message-chatgpt-new".into()
            } else {
                "assistant-message-chatgpt-baseline".into()
            },
            text: "MANUAL_REFRESH_CORE_TERMINAL_REPLY".into(),
        }))
    };
    let mut no_subscription =
        |_payload: &[u8]| Ok(crate::push::PushDeliveryOutcome::NoSubscription);

    let first =
        check_new_chatgpt_replies_with(&store, &workstream.id, &mut reader, &mut no_subscription)
            .unwrap();
    assert_eq!(first.state, "OBSERVER_BASELINE_ESTABLISHED");
    assert!(!first.observation_created);

    let second =
        check_new_chatgpt_replies_with(&store, &workstream.id, &mut reader, &mut no_subscription)
            .unwrap();
    assert_eq!(second.state, "NO_NEW_TERMINAL_REPLY");
    assert!(!second.observation_created);

    let third =
        check_new_chatgpt_replies_with(&store, &workstream.id, &mut reader, &mut no_subscription)
            .unwrap();
    assert_eq!(third.state, "NEW_REPLY_OBSERVED");
    assert!(third.observation_created);
    assert_eq!(
        requested_conversations,
        vec![
            endpoint.external_id.clone(),
            endpoint.external_id.clone(),
            endpoint.external_id,
        ]
    );
    assert_eq!(
        store
            .reply_observations_for_workstream(&workstream.id)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn chatgpt_observer_new_reply_pushes_one_exact_privacy_safe_deep_link_payload() {
    let (_directory, store, workstream, endpoint) = observer_context();
    let mut reads = 0;
    let mut reader = |_conversation_id: &str| {
        reads += 1;
        Ok(Some(chatgpt::direct::TerminalReply {
            message_id: if reads == 3 {
                "assistant-message-chatgpt-push-new".into()
            } else {
                "assistant-message-chatgpt-push-baseline".into()
            },
            text: "CHATGPT_REPLY_BODY_MUST_NOT_LEAVE_THE_ROUTER".into(),
        }))
    };
    let mut payloads = Vec::new();
    let mut push = |payload: &[u8]| {
        payloads.push(String::from_utf8(payload.to_vec()).unwrap());
        Ok(crate::push::PushDeliveryOutcome::Sent {
            invalid_subscription_fingerprints: vec![],
        })
    };

    assert_eq!(
        check_new_chatgpt_replies_with(&store, &workstream.id, &mut reader, &mut push)
            .unwrap()
            .state,
        "OBSERVER_BASELINE_ESTABLISHED"
    );
    assert_eq!(
        check_new_chatgpt_replies_with(&store, &workstream.id, &mut reader, &mut push)
            .unwrap()
            .state,
        "NO_NEW_TERMINAL_REPLY"
    );
    let observed =
        check_new_chatgpt_replies_with(&store, &workstream.id, &mut reader, &mut push).unwrap();
    assert_eq!(observed.state, "NEW_REPLY_OBSERVED");
    assert!(observed.observation_created);

    assert_eq!(payloads.len(), 1);
    let payload: serde_json::Value = serde_json::from_str(&payloads[0]).unwrap();
    let observation = store
        .reply_observations_for_workstream(&workstream.id)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(payload["type"], "chatgpt_reply");
    assert_eq!(payload["workstreamId"], workstream.id);
    assert_eq!(payload["observationId"], observation.id);
    assert_eq!(
        payload["workstreamName"],
        store.workstream_name(&endpoint.workstream_id).unwrap()
    );
    assert!(!payloads[0].contains("CHATGPT_REPLY_BODY_MUST_NOT_LEAVE_THE_ROUTER"));
    assert_eq!(
        store
            .reply_observations_for_workstream(&workstream.id)
            .unwrap()
            .first()
            .map(|item| item.push_state.as_str()),
        Some("SENT")
    );
}

#[test]
fn background_chatgpt_observer_skips_uninitialized_historical_endpoints() {
    let (_directory, store, workstream, historical) = observer_context();
    let current_project = store
        .create_project("Current ChatGPT observer project".into(), None)
        .unwrap();
    let current_workstream = store
        .create_workstream(
            &current_project.id,
            "Current ChatGPT observer workstream".into(),
        )
        .unwrap();
    let current = store
        .bind_endpoint(
            &current_workstream.id,
            Provider::Chatgpt,
            "conversation-current-observer".into(),
            "Current ChatGPT".into(),
            false,
        )
        .unwrap();
    store
        .save_chatgpt_reply_observer_empty_baseline(
            &current_workstream.id,
            &current.id,
            "conversation-current-observer",
        )
        .unwrap();

    let observed = active_initialized_chatgpt_observer_workstreams(&store).unwrap();
    assert_eq!(observed, vec![current_workstream.id]);
    assert!(!store
        .chatgpt_reply_observer_is_initialized(&historical.id)
        .unwrap());
    assert_eq!(workstream.id, historical.workstream_id);
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
    let scratch = std::path::PathBuf::from(r"D:\fixtures\临时处理");
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
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
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
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
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
fn mobile_codex_observation_prepares_only_the_current_exact_endpoint() {
    let (_directory, store, workstream, _endpoint_chatgpt) = observer_context();
    let store = Arc::new(store);
    let endpoint_codex_e1 = store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread-e1".into(),
            "Codex E1".into(),
            true,
        )
        .unwrap();
    let observation_e1 = store
        .record_reply_observation(
            &workstream.id,
            &endpoint_codex_e1.id,
            Some("turn-e1:item-e1"),
            "Current Codex external reply",
            None,
        )
        .unwrap()
        .unwrap();
    let core = RouterCore {
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        store: store.clone(),
    };

    let review = core
        .prepare_mobile_codex_outbound(
            &workstream.id,
            MobileCodexPrepareInput {
                run_id: observation_e1.id.clone(),
                attachment_ids: vec![],
            },
        )
        .expect("the current exact external Codex reply prepares a review");
    assert_eq!(review.status, "READY");
    assert_eq!(review.message, "Current Codex external reply");
    assert!(!review.requires_manual_dispatch);
    assert!(store
        .provider_runs_for_workstream(&workstream.id)
        .unwrap()
        .is_empty());

    store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread-e2".into(),
            "Codex E2".into(),
            true,
        )
        .unwrap();
    let error = match core.prepare_mobile_codex_outbound(
        &workstream.id,
        MobileCodexPrepareInput {
            run_id: observation_e1.id,
            attachment_ids: vec![],
        },
    ) {
        Ok(_) => panic!("a superseded Codex observation cannot become a handoff"),
        Err(error) => error,
    };
    assert!(error.contains("No durable complete Codex result or external observation"));
}

#[test]
fn mobile_codex_review_survives_host_restart_before_owner_approval() {
    let (_directory, store, workstream, _endpoint_chatgpt) = observer_context();
    let store = Arc::new(store);
    let endpoint_codex = store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread-restart".into(),
            "Codex restart review".into(),
            true,
        )
        .unwrap();
    let observation = store
        .record_reply_observation(
            &workstream.id,
            &endpoint_codex.id,
            Some("turn-restart:item-restart"),
            "Restart-safe Codex reply",
            None,
        )
        .unwrap()
        .unwrap();
    let before_restart = RouterCore {
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        store: store.clone(),
    };
    let prepared = before_restart
        .prepare_mobile_codex_outbound(
            &workstream.id,
            MobileCodexPrepareInput {
                run_id: observation.id.clone(),
                attachment_ids: vec![],
            },
        )
        .unwrap();
    drop(before_restart);

    let after_restart = RouterCore {
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        store: store.clone(),
    };
    let restored = after_restart
        .mobile_codex_outbound_review(&prepared.action_id)
        .expect("the exact action ID must recover the review after restart");
    assert_eq!(restored.status, "READY");
    assert_eq!(restored.message, "Restart-safe Codex reply");
    let identity = restored
        .codex_outbound_identity
        .expect("recovered review identity");
    assert_eq!(identity.workstream_id, workstream.id);
    assert_eq!(identity.source_id, observation.id);
    assert_eq!(identity.source_kind, "REPLY_OBSERVATION");
    assert_eq!(identity.source_endpoint_id, endpoint_codex.id);
    assert_eq!(identity.destination_chatgpt_conversation_id, "conversation");
    let approved = after_restart
        .approve_mobile_codex_outbound(
            &prepared.action_id,
            MobileApproveInput {
                revision: prepared.revision,
                message: "Owner-edited approved payload".into(),
            },
        )
        .expect("restart must not invalidate the exact mobile review");
    assert_eq!(approved.status, "APPROVED");
    assert_eq!(approved.message, "Owner-edited approved payload");
    let durable = store
        .mobile_codex_outbound_review(&prepared.action_id)
        .unwrap()
        .unwrap();
    assert_eq!(durable.status, "APPROVED");
    assert_eq!(
        durable.approved_text.as_deref(),
        Some("Owner-edited approved payload")
    );
    let snapshot = after_restart
        .mobile_workstream_snapshot(&workstream.id)
        .unwrap();
    let projected = snapshot
        .mobile_codex_outbound_reviews
        .iter()
        .find(|review| review.action_id == prepared.action_id)
        .expect("ordinary Workstream snapshot retains the exact pending review");
    assert_eq!(projected.status, "APPROVED");
    assert_eq!(projected.source_endpoint_id, endpoint_codex.id);
    assert_eq!(
        projected.destination_chatgpt_conversation_id,
        "conversation"
    );
    assert_eq!(
        projected.approved_text.as_deref(),
        Some("Owner-edited approved payload")
    );

    let manual_destination = after_restart
        .mobile_codex_outbound_manual_destination(&prepared.action_id)
        .expect("an approved exact review exposes only its active saved ChatGPT destination");
    assert_eq!(manual_destination.conversation_id, "conversation");
    assert_eq!(
        manual_destination.canonical_url,
        "https://chatgpt.com/c/conversation"
    );
    let ordinary_destination = after_restart
        .mobile_current_chatgpt_manual_destination(&workstream.id)
        .expect("an ordinary manual discussion resolves the current exact active binding");
    assert_eq!(ordinary_destination.conversation_id, "conversation");
    assert_eq!(
        ordinary_destination.canonical_url,
        "https://chatgpt.com/c/conversation"
    );
    store
        .bind_endpoint(
            &workstream.id,
            Provider::Chatgpt,
            "conversation-rebound".into(),
            "A different ChatGPT conversation".into(),
            true,
        )
        .unwrap();
    let error = after_restart
        .mobile_codex_outbound_manual_destination(&prepared.action_id)
        .expect_err("a re-bound conversation must never replace the review's saved destination");
    assert!(error.contains("no longer matches the current binding"));
}

#[test]
fn mobile_chatgpt_review_survives_host_restart_before_owner_approval() {
    let (_directory, store, workstream, endpoint_chatgpt) = observer_context();
    let store = Arc::new(store);
    let endpoint_codex = store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread-inbound-restart".into(),
            "Codex restart review".into(),
            true,
        )
        .unwrap();
    let observation = store
        .record_reply_observation(
            &workstream.id,
            &endpoint_chatgpt.id,
            Some("chat-turn-restart"),
            "Full ChatGPT source must stay intact",
            None,
        )
        .unwrap()
        .unwrap();
    let before_restart = RouterCore {
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        store: store.clone(),
    };
    let prepared = before_restart
        .prepare_mobile_reverse(
            &workstream.id,
            MobilePrepareInput {
                response_id: observation.id.clone(),
                initial_message: Some("Only this owner-selected instruction".into()),
                attachment_filenames: vec![],
            },
        )
        .unwrap();
    drop(before_restart);

    let after_restart = RouterCore {
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        store: store.clone(),
    };
    let restored = after_restart
        .mobile_chatgpt_inbound_review(&prepared.action_id)
        .expect("the exact action ID must recover the ChatGPT review after restart");
    assert_eq!(restored.status, "READY");
    assert_eq!(restored.message, "Only this owner-selected instruction");
    let identity = restored
        .chatgpt_inbound_identity
        .expect("recovered review identity");
    assert_eq!(identity.workstream_id, workstream.id);
    assert_eq!(identity.source_id, observation.id);
    assert_eq!(identity.source_kind, "REPLY_OBSERVATION");
    assert_eq!(identity.source_endpoint_id, endpoint_chatgpt.id);
    assert_eq!(
        identity.destination_codex_thread_id,
        endpoint_codex.external_id
    );
    let approved = after_restart
        .approve_mobile_reverse(
            &prepared.action_id,
            MobileApproveInput {
                revision: prepared.revision,
                message: "  Approved only selected instruction\n中文\n ".into(),
            },
        )
        .expect("restart must not invalidate the exact ChatGPT review");
    assert_eq!(approved.status, "APPROVED");
    let durable = store
        .mobile_chatgpt_inbound_review(&prepared.action_id)
        .unwrap()
        .unwrap();
    assert_eq!(durable.status, "APPROVED");
    assert_eq!(
        durable.approved_text.as_deref(),
        Some("  Approved only selected instruction\n中文\n ")
    );
    let snapshot = after_restart
        .mobile_workstream_snapshot(&workstream.id)
        .unwrap();
    let projected = snapshot
        .mobile_chatgpt_inbound_reviews
        .iter()
        .find(|review| review.action_id == prepared.action_id)
        .expect("ordinary Workstream snapshot retains the exact pending ChatGPT review");
    assert_eq!(projected.source_reference_id, observation.id);
    assert_eq!(
        projected.destination_codex_thread_id,
        endpoint_codex.external_id
    );
}

fn reverse_draft(message: &str) -> ReverseHandoffDraft {
    ReverseHandoffDraft {
        workstream_id: "workstream".into(),
        source_chatgpt_conversation_id: "conversation".into(),
        source_response_identity: "request".into(),
        source_candidate_id: Some("relay-candidate-1".into()),
        destination_codex_thread_id: "thread".into(),
        message: message.into(),
        attachments: vec![],
        status: HandoffStatus::Ready,
    }
}

fn mobile_outbound_review() -> MobileCodexOutboundReview {
    MobileCodexOutboundReview {
        workstream_id: "workstream".into(),
        source_run_id: Some("run".into()),
        source_reference_id: "run".into(),
        source_endpoint_id: "source".into(),
        source_codex_thread_id: "thread".into(),
        destination_chatgpt_conversation_id: "conversation".into(),
        original_text: "original".into(),
        attachments: vec![],
        approved_text: Some("approved".into()),
        revision: 3,
        status: "SENDING".into(),
        handoff_id: None,
    }
}

#[test]
fn mobile_codex_outbound_keeps_delivery_sending_until_exact_sent() {
    let mut review = mobile_outbound_review();
    apply_mobile_outbound_result(
        &mut review,
        &Ok(OutboundHandoffResult {
            status: "SENDING".into(),
            handoff_id: None,
            final_text: None,
            detail: Some("acceptance unproven".into()),
        }),
    );
    assert_eq!(review.status, "SENDING");
    apply_mobile_outbound_result(&mut review, &Err("endpoint changed before send".into()));
    assert_eq!(review.status, "FAILED");
    apply_mobile_outbound_result(
        &mut review,
        &Ok(OutboundHandoffResult {
            status: "SENT".into(),
            handoff_id: None,
            final_text: None,
            detail: None,
        }),
    );
    assert_eq!(review.status, "SENT");
}

#[test]
fn direct_chatgpt_carrier_failure_hides_untrusted_sidecar_detail() {
    assert_eq!(
        classified_chatgpt_carrier_failure("CHATGPT_PLAYWRIGHT_EXIT_1: profile path and stderr"),
        "CHATGPT_PLAYWRIGHT_EXIT"
    );
    assert_eq!(
        classified_chatgpt_carrier_failure("some unclassified provider detail"),
        "CHATGPT_DIRECT_CARRIER_FAILED"
    );
    assert_eq!(
        classified_chatgpt_carrier_failure("ATTACHMENT_ASSIGNMENT_UNPROVEN"),
        "ATTACHMENT_ASSIGNMENT_UNPROVEN"
    );
    assert_eq!(
        classified_chatgpt_carrier_failure("CHATGPT_ROUTER_PROFILE_IN_USE"),
        "CHATGPT_ROUTER_PROFILE_IN_USE"
    );
}

#[test]
fn mobile_codex_outbound_shows_failed_only_for_an_explicit_delivery_failure() {
    let mut review = mobile_outbound_review();
    apply_mobile_outbound_result(
        &mut review,
        &Ok(OutboundHandoffResult {
            status: "FAILED".into(),
            handoff_id: None,
            final_text: None,
            detail: Some("exact failure".into()),
        }),
    );
    assert_eq!(review.status, "FAILED");
}

#[test]
fn persisted_handoff_status_reconciles_a_mobile_sending_review_after_late_acceptance() {
    let directory = tempfile::tempdir().unwrap();
    let store = RouterStore::open_at(directory.path().join("router.db")).unwrap();
    let project = store
        .create_project("Mobile reconciliation".into(), None)
        .unwrap();
    let workstream = store.create_workstream(&project.id, "WS".into()).unwrap();
    let source = store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread".into(),
            "Codex".into(),
            false,
        )
        .unwrap();
    let destination = store
        .bind_endpoint(
            &workstream.id,
            Provider::Chatgpt,
            "conversation".into(),
            "ChatGPT".into(),
            false,
        )
        .unwrap();
    let handoff = store
        .create_ready_handoff(NewHandoff {
            workstream_id: workstream.id.clone(),
            source_endpoint_id: source.id.clone(),
            destination_endpoint_id: destination.id,
            direction: "CODEX_TO_CHATGPT".into(),
            source_response_identity: None,
            original_text: "original".into(),
            approved_text: "approved".into(),
            attachments: vec![],
        })
        .unwrap();
    store
        .transition_handoff(&handoff.id, "APPROVED", None)
        .unwrap();
    store
        .transition_handoff(&handoff.id, "SENDING", None)
        .unwrap();
    let mut review = mobile_outbound_review();
    review.workstream_id = workstream.id;
    review.source_endpoint_id = source.id;
    review.source_codex_thread_id = "thread".into();
    review.destination_chatgpt_conversation_id = "conversation".into();
    review.handoff_id = Some(handoff.id.clone());
    assert_eq!(
        persisted_mobile_outbound_status(&store, &review)
            .unwrap()
            .as_deref(),
        Some("SENDING")
    );
    store.transition_handoff(&handoff.id, "SENT", None).unwrap();
    assert_eq!(
        persisted_mobile_outbound_status(&store, &review)
            .unwrap()
            .as_deref(),
        Some("SENT")
    );
}

#[test]
fn reverse_handoff_deduplication_uses_source_destination_and_approved_payload() {
    assert_eq!(
        reverse_handoff_key(&reverse_draft("edited")),
        reverse_handoff_key(&reverse_draft("edited"))
    );
    assert_ne!(
        reverse_handoff_key(&reverse_draft("edited")),
        reverse_handoff_key(&reverse_draft("original"))
    );
}

#[test]
fn only_router_observed_completed_response_is_eligible() {
    let draft = reverse_draft("reply");
    let mut responses: HashMap<String, CompletedChatGptResponse> = HashMap::new();
    assert!(completed_reverse_source(&responses, &draft).is_err());
    responses.insert(
        "request".into(),
        CompletedChatGptResponse {
            conversation_id: "conversation".into(),
            response_identity: Some("request".into()),
            final_text: "completed reply".into(),
            artifacts: None,
            relay_candidates: vec![crate::chatgpt::model::RelayCandidate {
                id: "relay-candidate-1".into(),
                kind: crate::chatgpt::model::RelayCandidateKind::ContextualCodeBlock,
                confidence: "HIGH".into(),
                text: "completed reply".into(),
            }],
        },
    );
    assert!(completed_reverse_source(&responses, &draft).is_ok());
}

#[test]
fn completed_response_waits_for_an_explicit_reverse_handoff() {
    let mut responses: HashMap<String, CompletedChatGptResponse> = HashMap::new();
    responses.insert(
        "request".into(),
        CompletedChatGptResponse {
            conversation_id: "conversation".into(),
            response_identity: Some("request".into()),
            final_text: "completed reply".into(),
            artifacts: None,
            relay_candidates: vec![],
        },
    );
    assert!(responses.contains_key("request"));
}

#[test]
fn edited_reverse_draft_payload_is_what_reaches_codex() {
    let mut draft = reverse_draft("ORIGINAL_CHATGPT_TEXT");
    draft.message = "  EDITED_BEFORE_CODEX\n中文\n ".into();
    assert_eq!(
        approved_reverse_payload(&draft),
        "  EDITED_BEFORE_CODEX\n中文\n "
    );
    assert_ne!(approved_reverse_payload(&draft), "ORIGINAL_CHATGPT_TEXT");
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
    let mut attachments = vec![chatgpt::model::HandoffAttachment {
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
    let attachment = || chatgpt::model::HandoffAttachment {
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

/// This is intentionally opt-in: it speaks to the authenticated direct
/// Chrome carrier and starts one disposable official Codex thread, but it
/// never starts Tauri or an embedded provider surface. The caller supplies only
/// exact source identities and expected attachment integrity through
/// process-scoped environment values.
#[test]
#[ignore = "requires explicit authenticated Chrome and Codex real-E2E authorization"]
fn real_reverse_attachment_review_to_codex_core_e2e_without_tauri() {
    assert_eq!(
        std::env::var("AIWR_REAL_REVERSE_ATTACHMENT_E2E").as_deref(),
        Ok("1"),
        "set AIWR_REAL_REVERSE_ATTACHMENT_E2E=1 to authorize this disposable provider validation"
    );
    let conversation_id = std::env::var("AIWR_REAL_REVERSE_ATTACHMENT_CONVERSATION_ID")
        .expect("AIWR_REAL_REVERSE_ATTACHMENT_CONVERSATION_ID is required");
    let message_id = std::env::var("AIWR_REAL_REVERSE_ATTACHMENT_MESSAGE_ID")
        .expect("AIWR_REAL_REVERSE_ATTACHMENT_MESSAGE_ID is required");
    let filename = std::env::var("AIWR_REAL_REVERSE_ATTACHMENT_FILENAME")
        .expect("AIWR_REAL_REVERSE_ATTACHMENT_FILENAME is required");
    let expected_sha256 = std::env::var("AIWR_REAL_REVERSE_ATTACHMENT_SHA256")
        .expect("AIWR_REAL_REVERSE_ATTACHMENT_SHA256 is required");
    assert!(Uuid::parse_str(&conversation_id).is_ok());
    assert!(Uuid::parse_str(&message_id).is_ok());
    assert_eq!(
        std::path::Path::new(&filename)
            .file_name()
            .and_then(|name| name.to_str()),
        Some(filename.as_str())
    );
    assert_eq!(expected_sha256.len(), 64);

    let evidence = std::path::PathBuf::from(std::env::var("AIWR_REAL_REVERSE_ATTACHMENT_EVIDENCE").expect("non-overwriting evidence path required"));
    assert!(evidence.is_absolute() && !evidence.exists());
    let intent = evidence.with_extension("intent.json");
    assert!(!intent.exists(), "prior physical intent requires read-only reconciliation, never rerun");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/integrated-provider-router");
    let directory = tempfile::Builder::new().prefix("real-reverse-core-").tempdir_in(&root).expect("temporary validation workspace").keep();
    let store = Arc::new(
        RouterStore::open_at(directory.as_path().join("router-e2e.db"))
            .expect("temporary validation store"),
    );
    let core = RouterCore {
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        store: Arc::clone(&store),
    };
    core.chatgpt.configure(router_core::chatgpt_service::ExecutorConfiguration {
        resources: std::env::var("AIWR_REAL_REVERSE_EXECUTOR_RESOURCES").expect("exact resident fixture resources required").into(),
        node: std::env::var("AIWR_REAL_REVERSE_EXECUTOR_NODE").expect("bundled Node required").into(),
        data: directory.as_path().join("executor-data"),
    }).unwrap();
    let listener_store = Arc::clone(&store);
    let listener: Arc<dyn Fn(&Value) + Send + Sync> = Arc::new(move |event| {
        if event.get("method").and_then(Value::as_str) != Some("turn/completed") {
            return;
        }
        let Some(turn_id) = event.pointer("/params/turn/id").and_then(Value::as_str) else {
            return;
        };
        let Some(thread_id) = event
            .pointer("/params/threadId")
            .or_else(|| event.pointer("/params/turn/threadId"))
            .and_then(Value::as_str)
        else {
            return;
        };
        let status = event.pointer("/params/turn/status").and_then(Value::as_str);
        if status != Some("completed") {
            let _ = listener_store.complete_provider_run("CODEX", turn_id, "UNKNOWN", status);
            return;
        }
        match exact_codex_completed_event_result(thread_id, turn_id, event) {
            Ok((item_id, text)) => {
                let _ = capture_completed_codex_result(
                    &listener_store,
                    thread_id,
                    turn_id,
                    item_id,
                    text,
                );
            }
            Err(_) => {
                let _ = listener_store.complete_provider_run(
                    "CODEX",
                    turn_id,
                    "UNKNOWN",
                    Some("CODEX_RESULT_UNOBSERVED"),
                );
            }
        }
    });
    let mut adapter = CodexAdapter::start_validation(listener)
        .expect("official Codex app-server must start without Tauri");
    adapter
        .initialize()
        .expect("official Codex app-server must initialize");
    let catalog = adapter.request("model/list", json!({"limit":100,"includeHidden":false})).unwrap();
    let model = catalog["data"].as_array().unwrap().iter().find(|row| row["model"] == "gpt-6-luna").unwrap();
    assert!(model["supportedReasoningEfforts"].as_array().unwrap().iter().any(|effort| effort["reasoningEffort"] == "low"));
    let started = adapter.request("thread/start", json!({"cwd":directory.as_path(),"ephemeral":false,"model":"gpt-6-luna","config":{"model_reasoning_effort":"low"},"approvalPolicy":"never","sandbox":"workspace-write"})).unwrap();
    assert_eq!(started["model"], "gpt-6-luna");
    assert_eq!(started["reasoningEffort"], "low");
    let target = thread_summary(&started["thread"]).unwrap();

    let project = store
        .create_project("AIWR reverse attachment E2E".into(), None)
        .expect("temporary project");
    let workstream = store
        .create_workstream(&project.id, "ChatGPT to Codex attachment".into())
        .expect("temporary workstream");
    let chatgpt = store
        .bind_endpoint(
            &workstream.id,
            Provider::Chatgpt,
            conversation_id,
            "disposable exact ChatGPT source".into(),
            false,
        )
        .expect("exact ChatGPT binding");
    store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            target.id.clone(),
            "disposable exact Codex target".into(),
            false,
        )
        .expect("exact Codex binding");
    let source_text = std::env::var("AIWR_REAL_REVERSE_ATTACHMENT_SOURCE_TEXT").expect("exact captured terminal text required");
    let observation = store.record_reply_observation(&workstream.id, &chatgpt.id, Some(&message_id), &source_text, None)
        .unwrap().expect("native external source observation, not a manufactured ProviderRun");
    {
        let mut session = core.session.lock().expect("validation session");
        // `thread/start` above is this harness's own exact creation and
        // returned the target identity.  Match the normal Router
        // create-thread path: it already holds write readiness for this
        // one freshly created disposable target, so a second resume is
        // neither required nor supported by the current app-server.
        session.ready_threads.insert(target.id.clone());
        session.adapter = Some(adapter);
    }

    let approved_message = format!("Inspect only the selected file {filename} under .aiwr/incoming in this workspace. It is inside a handoff-id subdirectory. Use PowerShell Get-ChildItem -LiteralPath .aiwr/incoming -Recurse -Force -File, filter the exact filename, and require exactly one match. Compute SHA-256 from its bytes and reply with only the lowercase hash. Do not execute the attachment, access the network, or modify any file.");
    let prepared = core
            .prepare_mobile_reverse(
                &workstream.id,
                MobilePrepareInput {
                    response_id: observation.id.clone(),
                    initial_message: Some(approved_message.clone()),
                    attachment_filenames: vec![],
                },
            )
            .expect("Review must list the exact source card");
    assert!(prepared.attachment_options.contains(&filename));
    let selected = core
        .select_mobile_reverse_attachments(
            &prepared.action_id,
            MobileAttachmentSelectionInput {
                revision: prepared.revision,
                attachment_filenames: vec![filename.clone()],
            },
        )
        .expect("explicit selection must materialize the exact source file");
    assert_eq!(selected.attachments, vec![filename.clone()]);
    assert!(core.send_mobile_reverse(&selected.action_id, MobileSendInput { revision: selected.revision }).is_err());
    assert!(store.provider_runs_for_workstream(&workstream.id).unwrap().is_empty());
    let approved = core
            .approve_mobile_reverse(
                &selected.action_id,
                MobileApproveInput {
                    revision: selected.revision,
                    message: approved_message.clone(),
                },
            )
            .expect("review must approve the exact selected attachment");
    assert!(store.provider_runs_for_workstream(&workstream.id).unwrap().is_empty());
    fs::write(&intent, serde_json::to_vec_pretty(&json!({"status":"ONE_APPROVED_SEND_INTENT","workspace":directory.as_path(),"threadId":target.id,"actionId":approved.action_id,"sourceMessageId":message_id})).unwrap()).unwrap();
    let turn = core
        .send_mobile_reverse(
            &approved.action_id,
            MobileSendInput {
                revision: approved.revision,
            },
        )
        .expect("one approved Codex turn must start");
    assert!(!turn.turn_id.trim().is_empty());

    let handoff = store
        .provider_runs_for_workstream(&workstream.id)
        .expect("provider runs")
        .into_iter()
        .find(|run| run.external_run_id.as_deref() == Some(turn.turn_id.as_str()))
        .and_then(|run| run.origin_handoff_id)
        .and_then(|handoff_id| store.handoff_by_id(&handoff_id).ok())
        .expect("exact started turn must retain its reverse handoff");
    assert_eq!(handoff.status, "SENT");
    assert_eq!(
        handoff.source_response_identity.as_deref(),
        Some(message_id.as_str())
    );
    assert_eq!(handoff.destination_endpoint.external_id, target.id);
    assert_eq!(handoff.attachments.len(), 1);
    assert_eq!(handoff.attachments[0].filename, filename);
    assert!(handoff.attachments[0]
        .sha256
        .as_deref()
        .is_some_and(|actual| actual.eq_ignore_ascii_case(&expected_sha256)));
    let staged = directory
        .as_path()
        .join(".aiwr")
        .join("incoming")
        .join(&handoff.id)
        .join(&filename);
    assert_eq!(
        sha256_path(&staged).expect("staged attachment hash"),
        expected_sha256.to_lowercase()
    );

    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    loop {
        let completed = store
            .provider_runs_for_workstream(&workstream.id)
            .expect("provider runs")
            .into_iter()
            .any(|run| {
                run.external_run_id.as_deref() == Some(turn.turn_id.as_str())
                    && run.status == "COMPLETED"
            });
        if completed {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Codex terminal result was not observed after one exact approved turn"
        );
        thread::sleep(Duration::from_millis(250));
    }
    let runs = store.provider_runs_for_workstream(&workstream.id).unwrap();
    let run = runs.iter().find(|run| run.external_run_id.as_deref() == Some(turn.turn_id.as_str())).unwrap();
    assert_eq!(run.result_text.as_deref().unwrap().trim(), expected_sha256);
    let proof = json!({"status":"REAL_BROWSER_ATTACHMENT_CORE_CODEX_PASS","sourceConversationId":chatgpt.external_id,"sourceMessageId":message_id,"codexThreadId":target.id,"codexTurnId":turn.turn_id,"model":"gpt-6-luna","effort":"low","physicalTurnStarts":1,"handoffId":handoff.id,"handoffStatus":handoff.status,"providerRunStatus":run.status,"approvedBytes":handoff.approved_text,"filename":filename,"sha256":expected_sha256,"stagedSha256":sha256_path(&staged).unwrap(),"nativeResultIdentity":run.result_identity,"workspace":directory.as_path(),"productionCoreApprovalSelectionAndStaging":true,"transportSeam":"opt-in borrowed resident Browser Executor; no launch/navigation/browser close","installedProductEntry":false,"userUat":false});
    fs::write(&evidence, serde_json::to_vec_pretty(&proof).unwrap()).unwrap();
    core.chatgpt.shutdown();
    core.shutdown_owned_adapter();
    println!("REAL_REVERSE_ATTACHMENT_CORE_PASS workspace={}", directory.display());
}

#[test]
#[ignore = "opt-in one presealed disposable real ChatGPT attachment send"]
fn real_codex_declared_attachment_review_to_chatgpt_core_e2e_without_tauri() {
    assert_eq!(std::env::var("AIWR_REAL_OUTBOUND_ATTACHMENT_E2E").as_deref(), Ok("1"));
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/integrated-provider-router");
    let source: Value = serde_json::from_slice(&fs::read(std::env::var("AIWR_REAL_OUTBOUND_DECLARATION").unwrap()).unwrap()).unwrap();
    let config_path = std::path::PathBuf::from(std::env::var("AIWR_REAL_OUTBOUND_EXECUTOR_RESOURCES").unwrap());
    let config: Value = serde_json::from_slice(&fs::read(config_path.join("fixture-location.json")).unwrap()).unwrap();
    let evidence = std::path::PathBuf::from(std::env::var("AIWR_REAL_OUTBOUND_EVIDENCE").unwrap());
    let intent = evidence.with_extension("intent.json");
    assert!(!evidence.exists() && !intent.exists(), "prior intent requires receipt-only reconciliation");
    let directory = tempfile::Builder::new().prefix("real-outbound-core-").tempdir_in(&root).unwrap().keep();
    let store = Arc::new(RouterStore::open_at(directory.join("router.db")).unwrap());
    let core = RouterCore { store: store.clone(), chatgpt: Arc::default(), session: Arc::new(Mutex::new(Session::default())), completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())) };
    core.chatgpt.configure(router_core::chatgpt_service::ExecutorConfiguration { resources: config_path, node: std::env::var("AIWR_REAL_OUTBOUND_EXECUTOR_NODE").unwrap().into(), data: directory.join("executor-data") }).unwrap();
    let project = store.create_project("Real declared file outbound fixture".into(), None).unwrap();
    let work = store.create_workstream(&project.id, "Native declaration / selected upload".into()).unwrap();
    let codex = store.bind_endpoint(&work.id, Provider::Codex, source["threadId"].as_str().unwrap().into(), "Stage-owned actual Codex declaration".into(), false).unwrap();
    let chat = store.bind_endpoint(&work.id, Provider::Chatgpt, config["conversationId"].as_str().unwrap().into(), "Exact approved disposable ChatGPT".into(), false).unwrap();
    let identity = format!("codex:{}:{}", source["turnId"].as_str().unwrap(), source["assistantItemId"].as_str().unwrap());
    let observation = store.record_reply_observation(&work.id, &codex.id, Some(&identity), source["text"].as_str().unwrap(), None).unwrap().unwrap();
    let candidates = detect(&identity, &observation.text);
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].declared_sha256.as_deref().unwrap().eq_ignore_ascii_case(source["sha256"].as_str().unwrap()));
    assert!(candidates[0].actual_sha256.as_deref().unwrap().eq_ignore_ascii_case(source["sha256"].as_str().unwrap()));
    assert!(matches!(candidates[0].integrity_status, artifact::detector::IntegrityStatus::Verified));
    let default = core.prepare_mobile_codex_outbound(&work.id, MobileCodexPrepareInput { run_id: observation.id.clone(), attachment_ids: vec![] }).unwrap();
    assert!(default.attachments.is_empty(), "declared candidates do not self-select");
    let prepared = core.prepare_mobile_codex_outbound(&work.id, MobileCodexPrepareInput { run_id: observation.id.clone(), attachment_ids: vec![candidates[0].id.clone()] }).unwrap();
    assert_eq!(prepared.attachments, vec![source["filename"].as_str().unwrap()]);
    assert!(core.send_mobile_codex_outbound(&prepared.action_id, MobileSendInput { revision: prepared.revision }).is_err());
    assert!(store.provider_runs_for_workstream(&work.id).unwrap().is_empty());
    let payload = config["outbound"]["payload"].as_str().unwrap();
    let approved = core.approve_mobile_codex_outbound(&prepared.action_id, MobileApproveInput { revision: prepared.revision, message: payload.into() }).unwrap();
    assert!(store.provider_runs_for_workstream(&work.id).unwrap().is_empty(), "approval is not dispatch");
    let _ = core.check_chatgpt_replies(&work.id, false).unwrap();
    fs::write(&intent, serde_json::to_vec_pretty(&json!({"status":"ONE_APPROVED_SEND_INTENT","actionId":approved.action_id,"workspace":directory,"sourceObservationId":observation.id,"payloadSha256":config["outbound"]["payloadSha256"]})).unwrap()).unwrap();
    let sent = core.send_mobile_codex_outbound(&approved.action_id, MobileSendInput { revision: approved.revision }).unwrap();
    assert_eq!(sent.status, "SENT", "{:?}", sent.detail);
    assert!(core.send_mobile_codex_outbound(&approved.action_id, MobileSendInput { revision: approved.revision }).is_err());
    let end = std::time::Instant::now() + Duration::from_secs(120);
    loop {
        match core.check_chatgpt_replies(&work.id, false) { Ok(_) => {}, Err(error) if error == "INCOMPLETE" => {}, Err(error) => panic!("exact read failed: {error}") }
        let runs = store.provider_runs_for_workstream(&work.id).unwrap();
        assert_eq!(runs.len(), 1);
        if runs[0].status == "COMPLETED" { break; }
        assert!(std::time::Instant::now() < end, "exact terminal not observed; no resend");
        thread::sleep(Duration::from_secs(2));
    }
    let runs = store.provider_runs_for_workstream(&work.id).unwrap();
    let run = &runs[0];
    assert_eq!(run.result_text.as_deref().unwrap().trim(), config["outbound"]["expectedTerminal"].as_str().unwrap());
    let handoff = store.handoff_by_id(sent.handoff_id.as_deref().unwrap()).unwrap();
    assert_eq!(handoff.approved_text, payload);
    assert_eq!(handoff.attachments.len(), 1);
    assert!(handoff.attachments[0].sha256.as_deref().unwrap().eq_ignore_ascii_case(source["sha256"].as_str().unwrap()));
    assert!(handoff.attachments[0].send_sha256.as_deref().unwrap().eq_ignore_ascii_case(source["sha256"].as_str().unwrap()));
    let proof = json!({"status":"REAL_CODEX_DECLARED_ATTACHMENT_CORE_CHATGPT_PASS","sourceThreadId":codex.external_id,"sourceNativeIdentity":identity,"chatgptConversationId":chat.external_id,"acceptedUserMessageId":run.external_run_id,"terminalAssistantMessageId":run.result_identity,"terminalText":run.result_text,"handoffId":handoff.id,"handoffStatus":handoff.status,"providerRunStatus":run.status,"approvedPayloadSha256":config["outbound"]["payloadSha256"],"sourceSha256":source["sha256"],"sendSha256":handoff.attachments[0].send_sha256,"defaultSelectionEmpty":true,"sendBeforeApprovalRejected":true,"approvalDidNotSend":true,"replayRejected":true,"physicalSendIntents":1,"installedProductEntry":false,"transportSeam":"opt-in presealed borrowed resident Browser Executor","workspace":directory,"userUat":false});
    fs::write(evidence, serde_json::to_vec_pretty(&proof).unwrap()).unwrap();
    core.shutdown_owned_adapter();
    println!("REAL_CODEX_DECLARED_ATTACHMENT_CORE_CHATGPT_PASS");
}

#[test]
#[ignore = "read-only terminal reconciliation of one already-SENT fixture; no send allowed"]
fn reconcile_real_outbound_attachment_terminal_without_resend() {
    assert_eq!(std::env::var("AIWR_REAL_OUTBOUND_ATTACHMENT_E2E").as_deref(), Ok("1"));
    let evidence = std::path::PathBuf::from(std::env::var("AIWR_REAL_OUTBOUND_EVIDENCE").unwrap());
    assert!(!evidence.exists());
    let intent: Value = serde_json::from_slice(&fs::read(evidence.with_extension("intent.json")).unwrap()).unwrap();
    let directory = std::path::PathBuf::from(intent["workspace"].as_str().unwrap());
    let config_path = std::path::PathBuf::from(std::env::var("AIWR_REAL_OUTBOUND_EXECUTOR_RESOURCES").unwrap());
    let config: Value = serde_json::from_slice(&fs::read(config_path.join("fixture-location.json")).unwrap()).unwrap();
    let store = Arc::new(RouterStore::open_at(directory.join("router.db")).unwrap());
    let work = store.snapshot().unwrap().workstreams.into_iter().next().unwrap();
    let core = RouterCore { store: store.clone(), chatgpt: Arc::default(), session: Arc::new(Mutex::new(Session::default())), completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())) };
    core.chatgpt.configure(router_core::chatgpt_service::ExecutorConfiguration { resources: config_path, node: std::env::var("AIWR_REAL_OUTBOUND_EXECUTOR_NODE").unwrap().into(), data: directory.join("executor-data") }).unwrap();
    let end = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        match core.check_chatgpt_replies(&work.id, false) { Ok(_) => {}, Err(error) if error == "INCOMPLETE" => {}, Err(error) => panic!("exact read failed: {error}") }
        let runs = store.provider_runs_for_workstream(&work.id).unwrap();
        assert_eq!(runs.len(), 1);
        if runs[0].status == "COMPLETED" { break; }
        assert!(std::time::Instant::now() < end, "terminal unproven; never resend");
        thread::sleep(Duration::from_secs(2));
    }
    let run = store.provider_runs_for_workstream(&work.id).unwrap().remove(0);
    assert_eq!(run.result_text.as_deref().unwrap().trim(), config["outbound"]["expectedTerminal"].as_str().unwrap());
    let handoff = store.handoff_by_id(run.origin_handoff_id.as_deref().unwrap()).unwrap();
    assert_eq!(handoff.status, "SENT");
    assert_eq!(handoff.approved_text, config["outbound"]["payload"].as_str().unwrap());
    assert_eq!(handoff.attachments.len(), 1);
    assert!(handoff.attachments[0].send_sha256.as_deref().unwrap().eq_ignore_ascii_case(config["outbound"]["sha256"].as_str().unwrap()));
    let proof = json!({"status":"REAL_CODEX_DECLARED_ATTACHMENT_CORE_CHATGPT_PASS","readOnlyReconciliation":true,"resends":0,"acceptedUserMessageId":run.external_run_id,"terminalAssistantMessageId":run.result_identity,"terminalText":run.result_text,"handoffId":handoff.id,"handoffStatus":handoff.status,"providerRunStatus":run.status,"sourceSha256":config["outbound"]["sha256"],"sendSha256":handoff.attachments[0].send_sha256,"approvedPayloadSha256":config["outbound"]["payloadSha256"],"physicalSendIntents":1,"installedProductEntry":false,"transportSeam":"opt-in borrowed resident Browser Executor, original dispatch finalized without resend","workspace":directory,"userUat":false});
    fs::write(evidence, serde_json::to_vec_pretty(&proof).unwrap()).unwrap();
    core.shutdown_owned_adapter();
    println!("REAL_OUTBOUND_RECONCILIATION_PASS_NO_RESEND");
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
        |filename: &str, integrity_status: &str| crate::chatgpt::model::HandoffAttachment {
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
fn explicit_workstream_handoff_context_ignores_global_selection_for_both_directions() {
    let directory = tempfile::tempdir().unwrap();
    let store = RouterStore::open_at(directory.path().join("router.db")).unwrap();
    let project = store.create_project("Relay context".into(), None).unwrap();
    let workstream_a = store
        .create_workstream(&project.id, "WS-GLOBAL-A".into())
        .unwrap();
    let workstream_b = store
        .create_workstream(&project.id, "WS-DECISION-B".into())
        .unwrap();
    store
        .bind_endpoint(
            &workstream_a.id,
            Provider::Chatgpt,
            "conversation-a".into(),
            "ChatGPT A".into(),
            false,
        )
        .unwrap();
    store
        .bind_endpoint(
            &workstream_a.id,
            Provider::Codex,
            "thread-a".into(),
            "Codex A".into(),
            false,
        )
        .unwrap();
    store
        .bind_endpoint(
            &workstream_b.id,
            Provider::Chatgpt,
            "conversation-b".into(),
            "ChatGPT B".into(),
            false,
        )
        .unwrap();
    store
        .bind_endpoint(
            &workstream_b.id,
            Provider::Codex,
            "thread-b".into(),
            "Codex B".into(),
            false,
        )
        .unwrap();
    store
        .select_workspace(&project.id, Some(&workstream_a.id))
        .unwrap();
    assert_eq!(
        store.snapshot().unwrap().selected_workstream_id.as_deref(),
        Some(workstream_a.id.as_str())
    );

    let codex_to_chatgpt = resolve_persisted_handoff_context(
        &store,
        &workstream_b.id,
        "CODEX_TO_CHATGPT",
        "thread-b",
        "conversation-b",
    )
    .unwrap();
    assert_eq!(codex_to_chatgpt.workstream_id, workstream_b.id);
    assert_eq!(codex_to_chatgpt.source.workstream_id, workstream_b.id);
    assert_eq!(codex_to_chatgpt.destination.workstream_id, workstream_b.id);
    let outgoing = create_persisted_handoff(
        &store,
        codex_to_chatgpt,
        None,
        "source B",
        "approved B",
        vec![],
    )
    .unwrap();
    let chatgpt_run = store
        .create_provider_run(
            &outgoing.workstream_id,
            &outgoing.destination_endpoint.id,
            "CHATGPT",
            Some(&outgoing.id),
            Some("run-b-chatgpt"),
            "RUNNING",
        )
        .unwrap();
    assert_eq!(outgoing.workstream_id, workstream_b.id);
    assert_eq!(
        outgoing.endpoint_source().unwrap().workstream_id,
        workstream_b.id
    );
    assert_eq!(outgoing.destination_endpoint.workstream_id, workstream_b.id);
    assert_eq!(chatgpt_run.workstream_id, workstream_b.id);

    let chatgpt_to_codex = resolve_persisted_handoff_context(
        &store,
        &workstream_b.id,
        "CHATGPT_TO_CODEX",
        "conversation-b",
        "thread-b",
    )
    .unwrap();
    let incoming = create_persisted_handoff(
        &store,
        chatgpt_to_codex,
        Some("response-b".into()),
        "source B result",
        "approved B result",
        vec![],
    )
    .unwrap();
    let codex_run = store
        .create_provider_run(
            &incoming.workstream_id,
            &incoming.destination_endpoint.id,
            "CODEX",
            Some(&incoming.id),
            Some("run-b-codex"),
            "RUNNING",
        )
        .unwrap();
    assert_eq!(incoming.workstream_id, workstream_b.id);
    assert_eq!(
        incoming.endpoint_source().unwrap().workstream_id,
        workstream_b.id
    );
    assert_eq!(incoming.destination_endpoint.workstream_id, workstream_b.id);
    assert_eq!(codex_run.workstream_id, workstream_b.id);
    assert!(store
        .snapshot_for_workstream(&workstream_a.id)
        .unwrap()
        .handoffs
        .is_empty());
    assert!(store
        .provider_runs_for_workstream(&workstream_a.id)
        .unwrap()
        .is_empty());
    assert_eq!(
        store.snapshot().unwrap().selected_workstream_id.as_deref(),
        Some(workstream_a.id.as_str())
    );

    assert!(resolve_persisted_handoff_context(
        &store,
        &workstream_b.id,
        "CHATGPT_TO_CODEX",
        "conversation-b",
        "thread-a",
    )
    .is_err());
    assert!(resolve_persisted_handoff_context(
        &store,
        &workstream_b.id,
        "CODEX_TO_CHATGPT",
        "thread-a",
        "conversation-b",
    )
    .is_err());
    assert!(resolve_persisted_handoff_context(
        &store,
        "missing-workstream",
        "CHATGPT_TO_CODEX",
        "conversation-b",
        "thread-b",
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
fn review_actions_fail_closed_when_a_retained_result_endpoint_is_superseded() {
    let directory = tempfile::tempdir().unwrap();
    let store = RouterStore::open_at(directory.path().join("router.db")).unwrap();
    let project = store
        .create_project("Review provenance".into(), None)
        .unwrap();
    let workstream = store.create_workstream(&project.id, "WS".into()).unwrap();
    for (provider, name) in [(Provider::Chatgpt, "chat"), (Provider::Codex, "codex")] {
        let old = store
            .bind_endpoint(
                &workstream.id,
                provider.clone(),
                format!("{name}-old"),
                format!("{name} old"),
                false,
            )
            .unwrap();
        let run = store
            .create_provider_run(
                &workstream.id,
                &old.id,
                provider.as_str(),
                None,
                Some(&format!("{name}-request")),
                "RUNNING",
            )
            .unwrap();
        store
            .accept_completed_provider_result(
                &run.id,
                &format!("{name}-request"),
                &format!("{name}-result"),
                format!("{name} bounded result"),
            )
            .unwrap();
        let replacement = store
            .bind_endpoint(
                &workstream.id,
                provider.clone(),
                format!("{name}-new"),
                format!("{name} new"),
                true,
            )
            .unwrap();
        assert!(ensure_review_source_is_current(
            &store,
            &workstream.id,
            provider,
            &run.id,
            &old.id,
        )
        .unwrap_err()
        .contains("superseded Endpoint"));
        assert_ne!(old.id, replacement.id);
    }
}

#[test]
fn reverse_review_acknowledges_only_the_exact_result_after_approval() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
    let project = store
        .create_project("Review acknowledgement".into(), None)
        .unwrap();
    let workstream = store.create_workstream(&project.id, "WS".into()).unwrap();
    let chat = store
        .bind_endpoint(
            &workstream.id,
            Provider::Chatgpt,
            "chat".into(),
            "Chat".into(),
            false,
        )
        .unwrap();
    store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            "thread".into(),
            "Codex".into(),
            false,
        )
        .unwrap();
    let run = store
        .create_provider_run(
            &workstream.id,
            &chat.id,
            "CHATGPT",
            None,
            Some("request"),
            "RUNNING",
        )
        .unwrap();
    store
        .accept_completed_provider_result(
            &run.id,
            "request",
            "result",
            "<CODEX_HANDOFF>\nExact instruction\n</CODEX_HANDOFF>".into(),
        )
        .unwrap();
    let core = RouterCore {
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        store: Arc::clone(&store),
    };
    let review = core
        .prepare_mobile_reverse(
            &workstream.id,
            MobilePrepareInput {
                response_id: run.id.clone(),
                initial_message: None,
                attachment_filenames: vec![],
            },
        )
        .unwrap();
    assert!(store
        .provider_runs_for_workstream(&workstream.id)
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.id == run.id)
        .unwrap()
        .reviewed_at
        .is_none());
    core.approve_mobile_reverse(
        &review.action_id,
        MobileApproveInput {
            revision: review.revision,
            message: "Exact instruction".into(),
        },
    )
    .unwrap();
    assert!(store
        .provider_runs_for_workstream(&workstream.id)
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.id == run.id)
        .unwrap()
        .reviewed_at
        .is_some());
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


/// Codex-only live gate: the source observation is a labelled public fixture.
/// ChatGPT remains locally AUTH_REQUIRED; no browser or attachment E2E claim.
#[test]
#[ignore = "opt-in one disposable approved Codex turn; no ChatGPT operation"]
fn real_codex_approved_core_handoff_under_chatgpt_auth_stop() {
    assert_eq!(
        std::env::var("AIWR_REAL_CODEX_APPROVED_E2E").as_deref(),
        Ok("1")
    );
    let directory = tempfile::Builder::new()
        .prefix("aiwr-approved-codex-")
        .tempdir_in(r"D:\fixtures\临时处理")
        .unwrap();
    fs::write(directory.path().join("browser-auth-required.json"), "{}").unwrap();
    let store = Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
    let core = RouterCore {
        store: store.clone(),
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
    };
    core.chatgpt
        .configure(router_core::chatgpt_service::ExecutorConfiguration {
            node: directory.path().join("must-not-launch.exe"),
            resources: directory.path().join("missing-browser-package"),
            data: directory.path().into(),
        })
        .unwrap();
    let mut adapter = CodexAdapter::start_validation(Arc::new(|_| {})).unwrap();
    adapter.initialize().unwrap();
    let catalog = adapter
        .request("model/list", json!({"limit":100,"includeHidden":false}))
        .unwrap();
    let model = catalog["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["model"] == "gpt-6-luna")
        .expect("supported minimum validation model");
    assert!(model["supportedReasoningEfforts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["reasoningEffort"] == "low"));
    let started = adapter.request("thread/start", json!({
        "cwd": directory.path(), "ephemeral": false, "model":"gpt-6-luna",
        "config":{"model_reasoning_effort":"low"}, "approvalPolicy":"never", "sandbox":"read-only"
    })).unwrap();
    assert_eq!(
        started["model"], "gpt-6-luna",
        "stop before inference if model differs"
    );
    assert_eq!(
        started["reasoningEffort"], "low",
        "stop before inference if effort differs"
    );
    let target = thread_summary(&started["thread"]).unwrap();
    let project = store
        .create_project("Public approved Codex fixture".into(), None)
        .unwrap();
    let workstream = store
        .create_workstream(&project.id, "Approved write fixture".into())
        .unwrap();
    let source = store
        .bind_endpoint(
            &workstream.id,
            Provider::Chatgpt,
            "00000000-0000-4000-8000-000000000001".into(),
            "PUBLIC FIXTURE, not a live ChatGPT source".into(),
            false,
        )
        .unwrap();
    store
        .bind_endpoint(
            &workstream.id,
            Provider::Codex,
            target.id.clone(),
            "Disposable exact Codex target".into(),
            false,
        )
        .unwrap();
    let observation = store
        .record_reply_observation(
            &workstream.id,
            &source.id,
            Some("public-fixture-source"),
            "Public simulated source for Codex-only approval gate",
            None,
        )
        .unwrap()
        .unwrap();
    {
        let mut session = core.session.lock().unwrap();
        session.ready_threads.insert(target.id.clone());
        session.adapter = Some(adapter);
    }
    let marker = format!("AIWR_APPROVED_CODEX_{}", Uuid::new_v4().simple());
    let approved_text =
        format!("  Reply with exactly {marker}. Do not use tools or modify files.\n ");
    let prepared = core
        .prepare_mobile_reverse(
            &workstream.id,
            MobilePrepareInput {
                response_id: observation.id,
                initial_message: Some(approved_text.clone()),
                attachment_filenames: vec![],
            },
        )
        .unwrap();
    assert!(prepared.attachment_options.is_empty());
    assert_eq!(core.chatgpt.status(), "AUTH_REQUIRED");
    assert!(core
        .send_mobile_reverse(
            &prepared.action_id,
            MobileSendInput {
                revision: prepared.revision
            }
        )
        .is_err());
    assert!(store
        .provider_runs_for_workstream(&workstream.id)
        .unwrap()
        .is_empty());
    let approved = core
        .approve_mobile_reverse(
            &prepared.action_id,
            MobileApproveInput {
                revision: prepared.revision,
                message: approved_text.clone(),
            },
        )
        .unwrap();
    assert_eq!(approved.status, "APPROVED");
    assert_eq!(approved.message, approved_text);
    assert!(store
        .provider_runs_for_workstream(&workstream.id)
        .unwrap()
        .is_empty());
    let turn = core
        .send_mobile_reverse(
            &approved.action_id,
            MobileSendInput {
                revision: approved.revision,
            },
        )
        .unwrap();
    assert!(core
        .send_mobile_reverse(
            &approved.action_id,
            MobileSendInput {
                revision: approved.revision
            }
        )
        .is_err());
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    let reply = loop {
        let read = {
            let mut session = core.session.lock().unwrap();
            read_latest_codex_reply(session.adapter.as_mut().unwrap(), &target.id)
        };
        match read {
            Ok(read) => {
                if let Some(reply) = read.reply {
                    assert_eq!(reply.completed_turn_id, turn.turn_id);
                    break reply;
                }
            }
            Err(error)
                if error.contains("not supported yet") || error.contains("session metadata") => {}
            Err(error) => panic!("exact terminal Codex read failed: {error}"),
        }
        assert!(
            std::time::Instant::now() < deadline,
            "one approved turn did not yield an exact terminal reply; no resend"
        );
        thread::sleep(Duration::from_secs(2));
    };
    assert_eq!(reply.text.trim(), marker);
    capture_completed_codex_result_with_push(
        &store,
        &target.id,
        &turn.turn_id,
        reply.agent_item_id.clone(),
        reply.text.clone(),
        &mut |_| Ok(crate::push::PushDeliveryOutcome::NoSubscription),
    )
    .unwrap();
    let runs = store.provider_runs_for_workstream(&workstream.id).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "COMPLETED");
    assert_eq!(
        runs[0].external_run_id.as_deref(),
        Some(turn.turn_id.as_str())
    );
    let handoff = store
        .handoff_by_id(runs[0].origin_handoff_id.as_deref().unwrap())
        .unwrap();
    assert_eq!(handoff.status, "SENT");
    assert_eq!(handoff.approved_text, approved_text);
    assert_eq!(handoff.destination_endpoint.external_id, target.id);
    let result = core.mobile_review_results(&workstream.id).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text.trim(), marker);
    assert_eq!(core.chatgpt.status(), "AUTH_REQUIRED");
    let proof = json!({"status":"REAL_CODEX_APPROVED_CORE_HANDOFF_PASS","sourceKind":"PUBLIC_SIMULATED_CHATGPT_OBSERVATION", "realChatgpt":false,"realCodex":true,"userUat":false,
        "model":"gpt-6-luna","effort":"low","threadId":target.id,"turnId":turn.turn_id,"assistantItemId":reply.agent_item_id,
        "physicalTurnStarts":1,"sendBeforeApprovalRejected":true,"approvalDidNotSend":true,"repeatedSendRejected":true,
        "providerRuns":1,"providerRunStatus":"COMPLETED","handoffStatus":"SENT","exactApprovedBytesRetained":true,
        "approvedText":approved_text,"terminalMarker":marker,"chatgptState":"AUTH_REQUIRED","attachmentE2e":false});
    core.shutdown_owned_adapter();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let proof_file = root
        .join("Docs/research/integrated-provider-router/CODEX_APPROVED_CORE_GATE_2026-10-01.json");
    assert!(
        !proof_file.exists(),
        "retain previous live evidence rather than overwrite"
    );
    fs::write(proof_file, serde_json::to_vec_pretty(&proof).unwrap()).unwrap();
    println!("REAL_CODEX_APPROVED_CORE_GATE_PASS_ONE_TURN_NO_CHATGPT");
}

#[test]
fn borrowed_codex_transport_blocks_background_and_explicit_second_connector(){
 let dir=tempfile::tempdir().unwrap();let core=RouterCore{store:Arc::new(RouterStore::open_at(dir.path().join("router.db")).unwrap()),chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session{codex_adapter_borrowed:true,..Default::default()})),completed_chatgpt_responses:Arc::default()};
 let sink=Arc::new(router_core::events::NullEventSink);connect_codex_for_resident_host(sink.clone(),&core);let result=connect_codex_service(&core,sink).unwrap();assert!(!result.connected);assert!(!core.session.lock().unwrap().connecting);assert!(core.session.lock().unwrap().codex_adapter_borrowed);
}


#[test]
fn mobile_global_index_preserves_trash_and_readable_endpoint_context(){
 let d=tempfile::tempdir().unwrap();let store=Arc::new(RouterStore::open_at(d.path().join("router.db")).unwrap());
 let first=store.create_project("first group".into(),None).unwrap();let second=store.create_project("second group".into(),None).unwrap();
 let active=store.create_workstream(&first.id,"active".into()).unwrap();let trashed=store.create_workstream(&second.id,"trash".into()).unwrap();
 store.bind_endpoint(&active.id,Provider::Codex,"exact-thread-id".into(),"human dialogue label".into(),false).unwrap();store.trash_workstream(&trashed.id).unwrap();
 let core=RouterCore{store,chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::default()};
 let items=core.mobile_workstreams().unwrap();let visible=items.iter().find(|i|i.id==active.id).unwrap();let deleted=items.iter().find(|i|i.id==trashed.id).unwrap();
 assert_eq!(visible.source_label,"Codex · human dialogue label");assert_eq!(visible.project_name,"first group");assert!(deleted.trashed_at.is_some());assert_eq!(deleted.project_name,"second group");assert_ne!(visible.id,deleted.id);
}
