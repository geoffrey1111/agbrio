//! Explicit paid, isolated OpenAI acceptance. Never touches a real parent/task.
use super::{application, session::plain, workspaces};
use crate::host_application::RouterCore;
use router_core::store::{assistant::AssistantConnectionInput, RouterStore};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
struct Owned {
    core: RouterCore,
}
impl Drop for Owned {
    fn drop(&mut self) {
        self.core.shutdown_owned_adapter();
    }
}
fn action(core: &RouterCore, grant: &str, request: &str, operation: &str, input: Value) -> Value {
    let a = crate::assistant_mcp::call(
        core,
        grant,
        "agbrio_prepare_action",
        json!({"requestId":request,"operation":operation,"input":input}),
    )
    .unwrap();
    let r=crate::assistant_mcp::call(core,grant,"agbrio_execute_action",json!({"actionId":a["id"],"expectedHash":a["payloadHash"],"ruleId":null,"useOwnerAnswer":false,"assessment":"Explicit owner-authorized isolated read-only OpenAI recovery acceptance; never touch a real management/project conversation."})).unwrap();
    r
}
fn settled(core: &RouterCore, id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(150);
    loop {
        let v = application::read_compact(core, id, None).unwrap();
        if matches!(
            v["state"].as_str(),
            Some("RESULT_READY" | "FAILED" | "PARTIAL" | "CANCELLED")
        ) {
            return v;
        }
        assert!(Instant::now() < deadline, "isolated Pi deadline exceeded");
        std::thread::sleep(Duration::from_millis(500));
    }
}
#[test]
#[ignore = "explicit owner-authorized paid OpenAI-only read-only Pi; two short prompts, closes afterward"]
fn real_openai_readonly_same_session_cold_failure_resume() {
    assert_eq!(
        std::env::var("AGBRIO_PI_OPENAI_RECOVERY_QA").as_deref(),
        Ok("1")
    );
    let base = PathBuf::from(std::env::var_os("AGBRIO_PI_QA_ROOT").unwrap());
    assert!(base.is_absolute() && base.is_dir());
    if let Some(existing) = std::env::var_os("AGBRIO_PI_RECOVERY_EXISTING_ROOT") {
        let root = PathBuf::from(existing).canonicalize().unwrap();
        assert!(root.starts_with(base.canonicalize().unwrap()));
        let core = RouterCore {
            store: Arc::new(RouterStore::open_at(root.join("router.db")).unwrap()),
            session: Arc::new(Mutex::default()),
            chatgpt: Arc::default(),
            completed_chatgpt_responses: Arc::default(),
        };
        let owned = Owned { core };
        let core = &owned.core;
        let tasks = core.store.pi_tasks().unwrap();
        assert_eq!(tasks.len(), 1);
        let task = &tasks[0];
        assert_eq!(task.label, "Disposable OpenAI connectivity recovery");
        assert_eq!(task.selected.as_ref().unwrap().provider, "openai");
        assert_eq!(task.selected.as_ref().unwrap().model, "gpt-6-luna");
        assert_eq!(
            std::path::Path::new(&task.directory)
                .canonicalize()
                .unwrap(),
            root.join("project").canonicalize().unwrap()
        );
        let native = task.native_session_id.as_ref().unwrap();
        let id = format!("pi:{native}");
        super::session::verify_session_header(
            std::path::Path::new(task.native_session_file.as_ref().unwrap()),
            native,
            std::path::Path::new(&task.directory),
        )
        .unwrap();
        let result = application::read_compact(core, &id, None).unwrap();
        assert_eq!(result["state"], "RESULT_READY");
        assert!(result["publicMessages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["text"].as_str().is_some_and(|s| s.contains("收到"))));
        finish_resume(
            core,
            &task.creator_grant_id,
            &id,
            json!(native),
            &root,
            &result,
        );
        return;
    }
    let root = base.join(format!("native-openai-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let project = root.join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("AGENTS.md"),"Read-only isolated connectivity test. Do not use tools or write files. Follow the exact requested short reply.").unwrap();
    let core = RouterCore {
        store: Arc::new(RouterStore::open_at(root.join("router.db")).unwrap()),
        session: Arc::new(Mutex::default()),
        chatgpt: Arc::default(),
        completed_chatgpt_responses: Arc::default(),
    };
    let owned = Owned { core };
    let core = &owned.core;
    let parent = uuid::Uuid::new_v4().to_string();
    core.store
        .enable_codex_watch(
            &parent,
            "ISOLATED FIXTURE, NOT A GENUINE MANAGER",
            &plain(&project),
            &router_core::store::codex_watch::WatchSnapshot {
                state: "IDLE".into(),
                turn_id: None,
                item_id: None,
                text: String::new(),
            },
        )
        .unwrap();
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let grant = core
        .store
        .connect_assistant_instance(AssistantConnectionInput {
            label: "Read-only OpenAI recovery QA".into(),
            expires_at: at + 900000,
        })
        .unwrap();
    workspaces::owner_authorize(
        core,
        workspaces::WorkspaceInput {
            roots: vec![plain(&project)],
            machine_scope: false,
            writable: false,
            terminal: false,
            administrator: super::elevation::identity(None).unwrap().administrator,
            inherit_context: false,
            grant_ids: vec![grant.id.clone()],
        },
    )
    .unwrap();
    let scope = core.store.pi_scopes().unwrap().remove(0);
    let created = action(
        core,
        &grant.id,
        "create-only",
        "CREATE_CHAT",
        json!({"provider":"PI","directory":plain(&project),"label":"Disposable OpenAI connectivity recovery","pi":{"scopeId":scope.id,"scopeGeneration":scope.generation,"parent":{"provider":"CODEX","externalId":parent,"cwd":plain(&project)},"accessRoots":[plain(&project)],"writable":false,"modelProvider":"openai","model":"gpt-6-luna","thinking":"low"}}),
    );
    assert_eq!(created["status"], "APPLIED", "{}", created["errorCode"]);
    let c = &created["result"];
    assert_eq!(c["firstMessageSent"], false);
    assert_eq!(c["execution"]["access"]["writable"], false);
    assert_eq!(c["execution"]["access"]["terminal"], false);
    let id = c["threadId"].as_str().unwrap();
    let native = c["nativeSessionId"].clone();
    let first = action(
        core,
        &grant.id,
        "first-connectivity",
        "SEND_CHAT",
        json!({"threadId":id,"generation":c["generation"],"mode":"AUTO","text":"管理连通测试，请只回复收到","options":{"attachments":[]}}),
    );
    assert_eq!(first["status"], "APPLIED", "{}", first["errorCode"]);
    let result = settled(core, id);
    std::fs::write(root.join("first-result.json"),serde_json::to_vec_pretty(&json!({"threadId":id,"state":result["state"],"failure":result["failure"],"nativeReceipt":result["task"]["result"]["nativeTerminalReceiptId"]})).unwrap()).unwrap();
    assert_eq!(result["state"], "RESULT_READY", "{}", result["failure"]);
    assert!(result["publicMessages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["text"].as_str().is_some_and(|s| s.contains("收到"))));
    finish_resume(core, &grant.id, id, native, &root, &result);
}
fn finish_resume(
    core: &RouterCore,
    grant: &str,
    id: &str,
    native: Value,
    root: &std::path::Path,
    result: &Value,
) {
    let closed = action(
        core,
        grant,
        "close-first",
        "STOP_CHAT",
        json!({"threadId":id,"turnId":result["task"]["runtimeEpoch"]}),
    );
    assert_eq!(closed["status"], "APPLIED");
    // Fault injection is ONLY the isolated store's state; not a claimed upstream
    // failure. The following resume and second model response are genuinely native.
    let task = core.store.pi_task(id).unwrap();
    core.store
        .pi_set_state(
            &task.id,
            "FAILED",
            None,
            Some("PI_PROVIDER_CONNECTION_ERROR"),
        )
        .unwrap();
    let pi = core.session.lock().unwrap().pi.clone();
    pi.refresh_failed_context(core.store.clone(), &task.id)
        .unwrap();
    let restored = application::read_compact(core, id, None).unwrap();
    assert_eq!(restored["task"]["nativeSessionId"], native);
    assert!(restored["generation"].as_i64().unwrap() > task.generation);
    let second = action(
        core,
        grant,
        "continue-same-session",
        "SEND_CHAT",
        json!({"threadId":id,"generation":restored["generation"],"mode":"AUTO","text":"继续","options":{"attachments":[]}}),
    );
    assert_eq!(second["status"], "APPLIED", "{}", second["errorCode"]);
    let second_result = settled(core, id);
    assert_eq!(
        second_result["state"], "RESULT_READY",
        "{}",
        second_result["failure"]
    );
    let end = action(
        core,
        grant,
        "close-final",
        "STOP_CHAT",
        json!({"threadId":id,"turnId":second_result["task"]["runtimeEpoch"]}),
    );
    assert_eq!(end["result"]["closed"], true);
    let final_view = application::read_compact(core, id, None).unwrap();
    assert_eq!(final_view["runtimeLoaded"], false);
    let proof = json!({"threadId":id,"nativeSessionId":native,"model":"gpt-6-luna","thinking":"low","firstReceipt":result["task"]["result"]["nativeTerminalReceiptId"],"secondReceipt":second_result["task"]["result"]["nativeTerminalReceiptId"],"generationBefore":task.generation,"generationAfter":restored["generation"],"closed":true,"runtimeLoaded":false,"nativeModelPrompts":2,"failureInjection":"ISOLATED_STORE_ONLY_NOT_UPSTREAM","genuineManager":false,"ownerAnthropicTasksChanged":false,"scope":"READ_ONLY_NO_TERMINAL","administrator":pi.audit(&task.id).map(|a|a["process"]["administrator"].clone())});
    std::fs::write(
        root.join("proof.json"),
        serde_json::to_vec_pretty(&proof).unwrap(),
    )
    .unwrap();
    eprintln!("OPENAI_RECOVERY_QA_PASS {}", plain(&root));
}
