//! Explicit local HTTP/browser fixture. Live Codex parent metadata is read-only;
//! all Pi execution, pairing, grants and uploads belong to this isolated profile.
use super::{application, session::plain};
use crate::host_application::{RouterCore, Session};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[test]
#[ignore = "long-lived owned loopback QA host; no model task until reviewed MCP calls"]
fn isolated_browser_and_native_manager_host() {
    assert_eq!(std::env::var("AGBRIO_PI_QA_HOST").as_deref(), Ok("1"));
    let root = PathBuf::from(std::env::var_os("AGBRIO_PI_QA_ROOT").expect("owned QA root"));
    assert!(root.is_absolute() && root.is_dir());
    let profile = root.join("browser-profile");
    std::fs::create_dir_all(&profile).unwrap();
    let stop = profile.join("stop-host");
    assert!(
        !stop.exists(),
        "previous host stop marker must be reconciled first"
    );
    let manager = std::env::var("CODEX_THREAD_ID").expect("actual current Codex thread identity");
    assert!(uuid::Uuid::parse_str(&manager).is_ok());
    // This path connects only the existing owner-selected shared backend. It
    // cannot launch a fallback provider or submit a prompt to the parent.
    let command = crate::shared_codex::transport_command()
        .unwrap()
        .expect("existing shared Codex required");
    let mut adapter =
        crate::codex::adapter::CodexAdapter::start_shared_validation(command, Arc::new(|_| {}))
            .unwrap();
    adapter.initialize().unwrap();
    let parent = adapter
        .request(
            "thread/read",
            json!({"threadId":manager,"includeTurns":false}),
        )
        .unwrap();
    assert_eq!(parent["thread"]["id"], manager);
    let cwd = parent["thread"]["cwd"]
        .as_str()
        .expect("exact native parent cwd");
    assert!(PathBuf::from(cwd).is_absolute());
    let store =
        Arc::new(router_core::store::RouterStore::open_at(profile.join("router.db")).unwrap());
    let core = RouterCore {
        store: store.clone(),
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session {
            adapter: Some(adapter),
            ..Default::default()
        })),
        completed_chatgpt_responses: Arc::default(),
    };
    if !profile.join("ready.json").exists() {
        store
            .create_project("Pi native integration acceptance".into(), None)
            .unwrap();
    }
    store
        .enable_codex_watch(
            &manager,
            parent["thread"]["name"]
                .as_str()
                .unwrap_or("当前 Codex 开发管理对话"),
            cwd,
            &router_core::store::codex_watch::WatchSnapshot {
                state: "RUNNING".into(),
                turn_id: None,
                item_id: None,
                text: String::new(),
            },
        )
        .unwrap();
    let projects = profile.join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    if !store.pi_scopes().unwrap().iter().any(|s| {
        router_core::claude_code::same_root(&s.root, &plain(&projects))
    }) {
        application::owner_authorize(&core, &plain(&projects), false).unwrap();
    }
    let scope = store
        .pi_scopes()
        .unwrap()
        .into_iter()
        .find(|s| router_core::claude_code::same_root(&s.root, &plain(&projects)))
        .unwrap();
    let vision = profile.join("vision");
    std::fs::create_dir_all(&vision).unwrap();
    let mut fixtures = Vec::new();
    for (provider, model, ext) in [
        ("openai", "gpt-6-luna", "png"),
        ("anthropic", "claude-haiku-5-5", "jpg"),
    ] {
        let directory = projects.join(provider);
        std::fs::create_dir_all(&directory).unwrap();
        let image = vision.join(format!("{provider}.{ext}"));
        let fixture = directory.join("fixture.txt");
        // Reopening the browser host must preserve the fixtures referenced by
        // completed native conversations. An old image is never regenerated.
        let (token, code) = if fixture.exists() || image.exists() {
            assert!(fixture.is_file() && image.is_file(), "incomplete retained fixture");
            (std::fs::read_to_string(&fixture).unwrap(), Value::Null)
        } else {
            let token = format!("NATIVE_MANAGER_{}", uuid::Uuid::new_v4().simple());
            let code = uuid::Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase();
            std::fs::write(&fixture, &token).unwrap();
            crate::pi::live_tests::image(&code, &image);
            (token, Value::String(code))
        };
        fixtures.push(json!({"provider":provider,"model":model,"thinking":"low","directory":plain(&directory),"image":plain(&image),"expectedToken":token,"expectedCode":code}));
    }
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let origin = format!("http://127.0.0.1:{port}");
    let static_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dist");
    assert!(
        static_dir.join("index.html").is_file(),
        "fresh frontend build required"
    );
    let config = crate::mobile_http::MobileHttpConfig {
        port,
        allowed_host: format!("127.0.0.1:{port}"),
        allowed_origin: origin.clone(),
        access_issuer: String::new(),
        access_audience: String::new(),
        access_jwks_url: String::new(),
        static_dir,
    };
    let auth =
        Arc::new(crate::web_auth::WebAuth::open(Some(profile.join("browser-auth.json"))).unwrap());
    let pairing = auth.issue().unwrap();
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let grant = store
        .connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput {
            label: "Current native Codex Pi stage2 acceptance".into(),
            expires_at: at + 2 * 3600000,
        })
        .unwrap();
    let bearer = store
        .issue_assistant_access(&grant.id, &format!("{origin}/mcp"), grant.expires_at)
        .unwrap();
    let host = crate::HostRuntime::default();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let handle = runtime
        .block_on(crate::mobile_http::start_with_web_auth(
            core.clone(),
            config,
            host.clone(),
            auth.clone(),
        ))
        .unwrap();
    // Disposable credentials stay local and never enter logs or MCP content.
    let private = json!({"origin":origin,"mcpUrl":format!("{origin}/mcp"),"bearer":bearer,"pairingCode":pairing.code,"pairingExpiresAt":pairing.expires_at,"grantId":grant.id,"manager":{"provider":"CODEX","externalId":manager,"cwd":cwd},"scopeId":scope.id,"scopeGeneration":scope.generation,"fixtures":fixtures});
    std::fs::write(
        profile.join("private-session.json"),
        serde_json::to_vec_pretty(&private).unwrap(),
    )
    .unwrap();
    let ready = json!({"origin":origin,"pid":std::process::id(),"instanceId":host.instance_id(),"managerId":manager,"nativeParentMetadataVerified":true,"scope":"ISOLATED_QA_PROFILE","stopMarker":plain(&stop)});
    std::fs::write(
        profile.join("ready.json"),
        serde_json::to_vec_pretty(&ready).unwrap(),
    )
    .unwrap();
    eprintln!("{}", ready);
    let until = Instant::now() + Duration::from_secs(2 * 3600);
    while Instant::now() < until && !stop.exists() {
        std::thread::sleep(Duration::from_millis(200));
    }
    runtime.block_on(handle.shutdown());
    core.shutdown_owned_adapter();
    std::fs::remove_file(profile.join("private-session.json")).unwrap();
}
