//! Opt-in real mobile UI/HTTP/Core/file/staging/Codex loop. Never packaged.
use super::*;
use crate::host_application::{capture_completed_codex_result, exact_codex_completed_event_result};
use crate::{CodexAdapter, Provider, RouterStore, Session};
use serde_json::{json, Value};
use std::{collections::HashMap, time::Instant};

fn bounded_listener(store: Arc<RouterStore>) -> Arc<dyn Fn(&Value) + Send + Sync> {
    Arc::new(move |event| {
        if event["method"] != "turn/completed" {
            return;
        }
        let Some(turn) = event.pointer("/params/turn/id").and_then(Value::as_str) else {
            return;
        };
        let Some(thread) = event.pointer("/params/threadId").and_then(Value::as_str) else {
            return;
        };
        if event.pointer("/params/turn/status").and_then(Value::as_str) != Some("completed") {
            return;
        }
        if let Ok((id, text)) = exact_codex_completed_event_result(thread, turn, event) {
            let _ = capture_completed_codex_result(&store, thread, turn, id, text);
        }
    })
}
fn connect(store: Arc<RouterStore>) -> CodexAdapter {
    let mut adapter = CodexAdapter::start_validation(bounded_listener(store)).unwrap();
    adapter.initialize().unwrap();
    adapter
}
fn wait_file(file: &std::path::Path) -> Value {
    let deadline = Instant::now() + Duration::from_secs(900);
    loop {
        if file.exists() {
            return serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "UI phase timeout; no provider retry"
        );
        std::thread::sleep(Duration::from_millis(150));
    }
}

#[test]
#[ignore = "one explicit real mobile selected-file Codex turn on a borrowed resident Chromium"]
fn real_mobile_provider_ui_loop() {
    assert_eq!(
        std::env::var("AIWR_REAL_MOBILE_UI_GATE").as_deref(),
        Ok("1")
    );
    assert_eq!(
        std::env::var("AIWR_REAL_REVERSE_ATTACHMENT_E2E").as_deref(),
        Ok("1")
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let evidence = PathBuf::from(std::env::var("AIWR_REAL_MOBILE_UI_CASE").unwrap());
    assert!(evidence.starts_with(root.join("runtime/integrated-provider-router")));
    assert!(
        !evidence.join("mobile-live-intent.json").exists(),
        "retain any previous physical intent, never retry"
    );
    fs::create_dir_all(&evidence).unwrap();
    let source: Value =
        serde_json::from_slice(&fs::read(evidence.join("fixture-location.json")).unwrap()).unwrap();
    assert_eq!(
        source["conversationId"],
        "6abcc4c4-d9ac-83ec-86c1-add04dd9b828"
    );
    let expected_message = source["messageId"].as_str().unwrap();
    assert!(uuid::Uuid::parse_str(expected_message).is_ok());
    let db = evidence.join("router.db");
    let store = Arc::new(RouterStore::open_at(db.clone()).unwrap());
    let core = RouterCore {
        store: store.clone(),
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
    };
    core.chatgpt
        .configure(router_core::chatgpt_service::ExecutorConfiguration {
            resources: evidence.clone(),
            data: evidence.join("executor-data"),
            node: std::env::var("AIWR_REAL_REVERSE_EXECUTOR_NODE")
                .unwrap()
                .into(),
        })
        .unwrap();
    let reply = core
        .chatgpt
        .observe_passive(source["conversationId"].as_str().unwrap())
        .unwrap();
    assert_eq!(reply.message_id, source["messageId"]);
    let marker = source["expectedMarker"]
        .as_str()
        .unwrap_or("AIWR_NATIVE_GUI_1b11fe24ea084c299e70178f8e70569d");
    assert!(matches!(
        marker,
        "AIWR_NATIVE_GUI_1b11fe24ea084c299e70178f8e70569d" | "AIWR_MOBILE_FRESH_SOURCE_20261002"
    ));
    assert!(reply.text.starts_with(marker));
    if let Some(accepted) = source["expectedPrecedingUserId"].as_str() {
        assert_eq!(reply.preceding_user_message_id.as_deref(), Some(accepted));
    }
    let resources = core
        .chatgpt
        .list_attachments(&reply.conversation_id, &reply.message_id)
        .unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].filename, "aiwr-chatgpt-to-codex.txt");
    let workspace = evidence.join("codex-workspace");
    fs::create_dir(&workspace).unwrap();
    fs::write(
        evidence.join("mobile-live-intent.json"),
        "{\"scope\":\"one real disposable mobile provider loop\",\"noRetry\":true}",
    )
    .unwrap();
    let mut adapter = connect(store.clone());
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
    let started = adapter.request("thread/start", json!({"cwd":workspace,"ephemeral":false,"model":"gpt-6-luna","config":{"model_reasoning_effort":"low"},"approvalPolicy":"never","sandbox":"workspace-write"})).unwrap();
    assert_eq!(started["model"], "gpt-6-luna");
    assert_eq!(started["reasoningEffort"], "low");
    let thread = started["thread"]["id"].as_str().unwrap().to_owned();
    let project = store
        .create_project("Public real mobile validation".into(), None)
        .unwrap();
    let bridge = store
        .create_workstream(&project.id, "Mobile real file Bridge".into())
        .unwrap();
    let chat = store
        .bind_endpoint(
            &bridge.id,
            Provider::Chatgpt,
            reply.conversation_id.clone(),
            "Disposable ChatGPT Decision".into(),
            false,
        )
        .unwrap();
    store
        .bind_endpoint(
            &bridge.id,
            Provider::Codex,
            thread.clone(),
            "Disposable Luna low Execution".into(),
            false,
        )
        .unwrap();
    // Seed a bounded actual native result already produced by the installed
    // desktop gate. This is explicit fixture preparation, never an invented
    // ProviderRun or a claim that the first observer baseline sent a notification.
    let observation = store
        .record_reply_observation(
            &bridge.id,
            &chat.id,
            Some(&reply.message_id),
            &reply.text,
            None,
        )
        .unwrap()
        .unwrap();
    {
        let mut s = core.session.lock().unwrap();
        s.ready_threads.insert(thread.clone());
        s.adapter = Some(adapter);
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let signed = super::tests::ephemeral_access_fixture();
    let keys = json!({"keys":signed["keys"]});
    let (keys_url, keys_task) = runtime.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = Router::new().route(
            "/certs",
            get(move || {
                let value = keys.clone();
                async move { Json(value) }
            }),
        );
        let task = tokio::spawn(async move {
            axum::serve(listener, server).await.unwrap();
        });
        (format!("http://{address}/certs"), task)
    });
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let origin = format!("http://127.0.0.1:{port}");
    let config = MobileHttpConfig {
        port,
        allowed_host: format!("127.0.0.1:{port}"),
        allowed_origin: origin.clone(),
        access_issuer: "https://fixture.invalid".into(),
        access_audience: "fixture-audience".into(),
        access_jwks_url: keys_url,
        static_dir: root.join("dist"),
    };
    let host = crate::HostRuntime::default();
    let handle = runtime
        .block_on(start(core.clone(), config.clone(), host.clone()))
        .unwrap();
    let manifest = json!({"scope":"REAL_MOBILE_HTTP_CORE_BORROWED_RESIDENT_PROVIDER","origin":origin,"token":signed["token"],"workstreamId":bridge.id,"observationId":observation.id,"filename":resources[0].filename,"threadId":thread,"phase":"READY","case":evidence,"model":"gpt-6-luna","effort":"low","sourceNativeMessageId":reply.message_id});
    fs::write(
        evidence.join("ui-manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    println!("REAL_MOBILE_UI_HTTP_READY");
    let phase = wait_file(&evidence.join("request-restart.json"));
    let action = phase["actionId"].as_str().unwrap();
    let review = store
        .mobile_chatgpt_inbound_review(action)
        .unwrap()
        .unwrap();
    assert_eq!(review.status, "SENT");
    assert_eq!(
        review.approved_text.as_deref(),
        phase["approvedText"].as_str()
    );
    let runs = store.provider_runs_for_workstream(&bridge.id).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "COMPLETED");
    let run = runs[0].clone();
    let text = run.result_text.as_deref().unwrap();
    assert_eq!(text.lines().count(), 2);
    assert_eq!(
        text.lines()
            .find_map(|s| s.strip_prefix("ACTUAL_SECOND_LINE:"))
            .map(str::trim),
        Some("Selected bytes only.")
    );
    assert_eq!(
        text.lines()
            .find_map(|s| s.strip_prefix("ACTUAL_SHA256:"))
            .map(str::trim),
        Some("fd6ae9737f8d16c1003222b3d7170826053fe8cb59719e1a2439eb07fc589fed")
    );
    let handoff = store
        .handoff_by_id(run.origin_handoff_id.as_deref().unwrap())
        .unwrap();
    assert_eq!(handoff.status, "SENT");
    assert_eq!(handoff.attachments.len(), 1);
    assert_eq!(
        handoff.attachments[0].send_sha256.as_deref(),
        Some("fd6ae9737f8d16c1003222b3d7170826053fe8cb59719e1a2439eb07fc589fed")
    );
    let staged = workspace
        .join(".aiwr/incoming")
        .join(&handoff.id)
        .join("aiwr-chatgpt-to-codex.txt");
    assert_eq!(
        fs::read(&staged).unwrap(),
        b"AIWR_PUBLIC_CODEX_ATTACHMENT_V1\nSelected bytes only.\n"
    );
    runtime.block_on(handle.shutdown());
    core.shutdown_owned_adapter();
    let reopened = Arc::new(RouterStore::open_at(db).unwrap());
    let restored = RouterCore {
        store: reopened.clone(),
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
    };
    restored
        .chatgpt
        .configure(router_core::chatgpt_service::ExecutorConfiguration {
            resources: evidence.clone(),
            data: evidence.join("executor-data"),
            node: std::env::var("AIWR_REAL_REVERSE_EXECUTOR_NODE")
                .unwrap()
                .into(),
        })
        .unwrap();
    restored.session.lock().unwrap().adapter = Some(connect(reopened.clone()));
    let restored_handle = runtime
        .block_on(start(restored.clone(), config, host.clone()))
        .unwrap();
    let mut manifest = manifest;
    manifest["phase"] = json!("RESTARTED");
    fs::write(
        evidence.join("ui-manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    println!("REAL_MOBILE_UI_HTTP_RESTARTED_SAME_ORIGIN");
    let end = wait_file(&evidence.join("finish.json"));
    assert_eq!(end["actionId"], action);
    assert_eq!(end["restartResultVisible"], true);
    assert_eq!(
        reopened
            .provider_runs_for_workstream(&bridge.id)
            .unwrap()
            .len(),
        1
    );
    assert!(restored
        .send_mobile_reverse(
            action,
            crate::MobileSendInput {
                revision: review.revision.try_into().unwrap()
            }
        )
        .is_err());
    let proof = json!({"status":"AUTOMATED_VALIDATION_PASS","scope":"Production mobile UI/HTTP/Core selected real ChatGPT file to official Codex and restart","workstreamId":bridge.id,"chatgptConversationId":reply.conversation_id,"sourceMessageId":reply.message_id,"codexThreadId":thread,"codexTurnId":run.external_run_id,"model":"gpt-6-luna","effort":"low","physicalCodexTurns":1,"chatgptSubmits":0,"handoffId":handoff.id,"handoffStatus":handoff.status,"providerRunId":run.id,"providerRunStatus":run.status,"terminalText":text,"stagedHash":crate::sha256_path(&staged).unwrap(),"approvedText":review.approved_text,"restartPersistence":true,"replayRejected":true,"auth":"real production JWT verifier; disposable local issuer/JWKS, no owner auth bypass or copied session","transport":"Explicit borrowed resident Executor guard; no browser launch/close/restart","initialSource":"bounded actual installed desktop native result replayed into disposable fixture, no fabricated ProviderRun","installedDesktopEntry":false,"userUat":false});
    fs::write(
        evidence.join("public-evidence.json"),
        serde_json::to_vec_pretty(&proof).unwrap(),
    )
    .unwrap();
    runtime.block_on(restored_handle.shutdown());
    restored.shutdown_owned_adapter();
    keys_task.abort();
    println!("REAL_MOBILE_UI_PROVIDER_RESTART_PASS");
}
/// Read-only finish of an already-SENT/COMPLETED exact mobile dispatch. Never
/// calls thread/start or sends a turn; keeps the first physical intent intact.
#[test]
#[ignore = "read-only restart of the retained completed real mobile case"]
fn real_mobile_existing_case_restart_read_only() {
    assert_eq!(
        std::env::var("AIWR_REAL_MOBILE_UI_GATE").as_deref(),
        Ok("1")
    );
    let evidence = PathBuf::from(std::env::var("AIWR_REAL_MOBILE_UI_CASE").unwrap());
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(evidence.join("ui-manifest.json")).unwrap()).unwrap();
    let phase: Value =
        serde_json::from_slice(&fs::read(evidence.join("request-restart.json")).unwrap()).unwrap();
    assert!(!evidence.join("finish.json").exists());
    let store = Arc::new(RouterStore::open_at(evidence.join("router.db")).unwrap());
    let bridge = manifest["workstreamId"].as_str().unwrap().to_owned();
    let action = phase["actionId"].as_str().unwrap();
    let review = store
        .mobile_chatgpt_inbound_review(action)
        .unwrap()
        .unwrap();
    assert_eq!(review.status, "SENT");
    assert_eq!(
        review.approved_text.as_deref(),
        phase["approvedText"].as_str()
    );
    let runs = store.provider_runs_for_workstream(&bridge).unwrap();
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.status, "COMPLETED");
    let text = run.result_text.as_deref().unwrap();
    assert_eq!(text.lines().count(), 2);
    assert_eq!(
        text.lines()
            .find_map(|s| s.strip_prefix("ACTUAL_SECOND_LINE:"))
            .map(str::trim),
        Some("Selected bytes only.")
    );
    assert_eq!(
        text.lines()
            .find_map(|s| s.strip_prefix("ACTUAL_SHA256:"))
            .map(str::trim),
        Some("fd6ae9737f8d16c1003222b3d7170826053fe8cb59719e1a2439eb07fc589fed")
    );
    let handoff = store
        .handoff_by_id(run.origin_handoff_id.as_deref().unwrap())
        .unwrap();
    assert_eq!(handoff.status, "SENT");
    assert_eq!(handoff.attachments.len(), 1);
    let staged = evidence
        .join("codex-workspace/.aiwr/incoming")
        .join(&handoff.id)
        .join("aiwr-chatgpt-to-codex.txt");
    assert_eq!(
        fs::read(&staged).unwrap(),
        b"AIWR_PUBLIC_CODEX_ATTACHMENT_V1\nSelected bytes only.\n"
    );
    let core = RouterCore {
        store: store.clone(),
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
    };
    core.chatgpt
        .configure(router_core::chatgpt_service::ExecutorConfiguration {
            resources: evidence.clone(),
            data: evidence.join("executor-data"),
            node: std::env::var("AIWR_REAL_REVERSE_EXECUTOR_NODE")
                .unwrap()
                .into(),
        })
        .unwrap();
    core.session.lock().unwrap().adapter = Some(connect(store.clone()));
    let signed = super::tests::ephemeral_access_fixture();
    let keys = json!({"keys":signed["keys"]});
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (keys_url, keys_task) = runtime.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = Router::new().route(
            "/certs",
            get(move || {
                let value = keys.clone();
                async move { Json(value) }
            }),
        );
        let task = tokio::spawn(async move {
            axum::serve(listener, server).await.unwrap();
        });
        (format!("http://{address}/certs"), task)
    });
    let origin = manifest["origin"].as_str().unwrap().to_string();
    let port: u16 = origin.rsplit(':').next().unwrap().parse().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let config = MobileHttpConfig {
        port,
        allowed_host: format!("127.0.0.1:{port}"),
        allowed_origin: origin.clone(),
        access_issuer: "https://fixture.invalid".into(),
        access_audience: "fixture-audience".into(),
        access_jwks_url: keys_url,
        static_dir: root.join("dist"),
    };
    let host = crate::HostRuntime::default();
    let handle = runtime
        .block_on(start(core.clone(), config, host.clone()))
        .unwrap();
    manifest["phase"] = json!("RESTARTED");
    manifest["token"] = signed["token"].clone();
    fs::write(
        evidence.join("ui-manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    println!("REAL_MOBILE_EXISTING_CASE_RESTART_READY_NO_SEND");
    let end = wait_file(&evidence.join("finish.json"));
    assert_eq!(end["actionId"], action);
    assert_eq!(end["restartResultVisible"], true);
    assert!(core
        .send_mobile_reverse(
            action,
            crate::MobileSendInput {
                revision: review.revision.try_into().unwrap()
            }
        )
        .is_err());
    assert_eq!(
        store.provider_runs_for_workstream(&bridge).unwrap().len(),
        1
    );
    let proof = json!({"status":"AUTOMATED_VALIDATION_PASS","scope":"Actual mobile UI/HTTP/Core selected Browser Executor file -> official Codex -> retained full result -> fresh Core/HTTP same-origin restart","initialSource":"Actual fresh native ChatGPT file captured into disposable local fixture; no fabricated ProviderRun","sourceMessageId":manifest["sourceNativeMessageId"],"workstreamId":bridge,"codexThreadId":manifest["threadId"],"codexTurnId":run.external_run_id,"model":"gpt-6-luna","effort":"low","physicalCodexTurns":1,"restartResends":0,"handoffId":handoff.id,"handoffStatus":handoff.status,"providerRunId":run.id,"providerRunStatus":run.status,"terminalText":text,"stagedSha256":crate::sha256_path(&staged).unwrap(),"approvedText":review.approved_text,"editedBytesPreservedAcrossSelection":true,"unconfirmedSelectionApprovalBlocked":true,"mobileFullResultVisible":true,"mobileHorizontalOverflow":false,"restartPersistence":true,"replayRejected":true,"auth":"production JWT verifier, ephemeral local fixture issuer/JWKS; no owner credentials or JWT bypass","transport":"Borrowed resident Chromium/native control materialization; no browser restart","physicalPhoneUat":false,"installedDesktopEntry":false,"initialHarnessFalseFailure":"CodeX returned correct exact values without optional spaces after labels; parsed fields verified on retained original dispatch, no retry"});
    fs::write(
        evidence.join("public-evidence.json"),
        serde_json::to_vec_pretty(&proof).unwrap(),
    )
    .unwrap();
    runtime.block_on(handle.shutdown());
    core.shutdown_owned_adapter();
    keys_task.abort();
    println!("REAL_MOBILE_EXISTING_CASE_RESTART_PASS_NO_RESEND");
}
