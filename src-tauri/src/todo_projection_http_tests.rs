//! Isolated real authenticated HTTP handlers; no owner sessions or model calls.
use super::*;
use crate::mobile_http::{start_with_web_auth, MobileHttpConfig};
use std::sync::Arc;

#[test]
fn t_items_http_auth_read_projection_and_thread_cutoff() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime/t-items-fixtures");
    std::fs::create_dir_all(&root).unwrap();
    let d = tempfile::tempdir_in(root).unwrap();
    std::fs::write(d.path().join("index.html"), "fixture").unwrap();
    let core = RouterCore {
        store: Arc::new(RouterStore::open_at(d.path().join("router.db")).unwrap()),
        chatgpt: Arc::default(),
        session: Arc::default(),
        completed_chatgpt_responses: Arc::default(),
    };
    let snapshot = |state: &str, turn: &str| router_core::store::codex_watch::WatchSnapshot {
        state: state.into(),
        turn_id: Some(turn.into()),
        item_id: Some(turn.into()),
        text: "native result".into(),
    };
    let cwd = d.path().to_str().unwrap();
    for id in ["exact", "other"] {
        core.store
            .enable_codex_watch(id, id, cwd, &snapshot("IDLE", "0"))
            .unwrap();
        core.store
            .record_codex_watch(id, 1, &snapshot("RESULT_READY", "1"))
            .unwrap();
    }
    let cutoff = core
        .store
        .codex_watch_feed(0)
        .unwrap()
        .events
        .into_iter()
        .find(|e| e.thread_id == "exact")
        .unwrap()
        .sequence;
    let auth = Arc::new(crate::web_auth::WebAuth::open(None).unwrap());
    let token = auth
        .exchange(&auth.issue().unwrap().code, "fixture", false)
        .unwrap();
    let cookie = format!("{}={token}", crate::web_auth::COOKIE);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = crate::HostRuntime::default();
    let config = MobileHttpConfig {
        port: 0,
        allowed_host: "todo.fixture.invalid".into(),
        allowed_origin: "https://todo.fixture.invalid".into(),
        access_issuer: "https://unused.invalid".into(),
        access_audience: "unused".into(),
        access_jwks_url: "https://unused.invalid/keys".into(),
        static_dir: d.path().into(),
    };
    let server = rt
        .block_on(start_with_web_auth(
            core.clone(),
            config,
            host.clone(),
            auth,
        ))
        .unwrap();
    let base = format!("http://{}", server.address);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let get = |path: &str, credential: &str| {
        client
            .get(format!("{base}/v1/mobile/{path}"))
            .header("host", "todo.fixture.invalid")
            .header("cookie", credential)
            .send()
            .unwrap()
    };
    assert_eq!(get("computer-identity", "").status().as_u16(), 401);
    let name = get("computer-identity", &cookie);
    assert!(name.status().is_success());
    assert!(name.json::<Value>().unwrap().get("hostName").is_some());
    let project = core
        .store
        .create_project("T4 fixture".into(), None)
        .unwrap();
    let stream = core
        .store
        .create_workstream(&project.id, "T4 fixture".into())
        .unwrap();
    let side = |id: &str| router_core::store::role_bridge::RoleBindingInput {
        provider: "CODEX".into(),
        external_id: id.into(),
        label: "fixture".into(),
        cwd: Some(cwd.into()),
    };
    let binding = core
        .store
        .bind_role_bridge(
            &stream.id,
            0,
            side("fixture-decision"),
            side("fixture-execution"),
        )
        .unwrap();
    core.store
        .record_reply_observation(
            &stream.id,
            &binding.decision.as_ref().unwrap().endpoint.id,
            Some("native-t4"),
            "original source",
            None,
        )
        .unwrap();
    let activity = client
        .post(format!("{base}/v1/mobile/bridge-activity"))
        .header("host", "todo.fixture.invalid")
        .header("origin", "https://todo.fixture.invalid")
        .header("cookie", &cookie)
        .json(&json!({"workstreamIds":[stream.id]}))
        .send()
        .unwrap();
    assert!(activity.status().is_success());
    let activity = activity.json::<Value>().unwrap();
    assert_eq!(activity[0]["readyHandoffs"][0]["status"], "UNPREPARED");
    assert_eq!(activity[0]["bindingRevision"], binding.binding_revision);
    let list = get("codex-watches", &cookie).json::<Value>().unwrap();
    let exact = list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["threadId"] == "exact")
        .unwrap();
    assert_eq!(exact["unreadResult"], true);
    assert!(exact["pending"].as_array().unwrap().is_empty());
    assert_eq!(exact["pendingAvailable"], false);
    core.store
        .record_codex_watch("exact", 1, &snapshot("RUNNING", "2"))
        .unwrap();
    core.store
        .record_codex_watch("exact", 1, &snapshot("RESULT_READY", "2"))
        .unwrap();
    let post = |origin: &str, credential: &str, cutoff: Option<i64>| {
        client
            .post(format!("{base}/v1/mobile/codex-watches"))
            .header("host", "todo.fixture.invalid")
            .header("origin", origin)
            .header("cookie", credential)
            .json(&json!({"action":"MARK_THREAD_READ","threadId":"exact","upToSequence":cutoff}))
            .send()
            .unwrap()
    };
    assert_eq!(
        post("https://other.invalid", &cookie, None)
            .status()
            .as_u16(),
        403
    );
    assert_eq!(
        post("https://todo.fixture.invalid", "", None)
            .status()
            .as_u16(),
        401
    );
    assert_eq!(
        post("https://todo.fixture.invalid", &cookie, Some(cutoff))
            .json::<Value>()
            .unwrap()["unreadResult"],
        true
    );
    assert_eq!(
        post("https://todo.fixture.invalid", &cookie, None)
            .json::<Value>()
            .unwrap()["unreadResult"],
        false
    );
    assert!(core.store.todo_unread_result("other").unwrap());
    rt.block_on(server.shutdown());
    drop(client);
    drop(host);
    drop(rt);
}
