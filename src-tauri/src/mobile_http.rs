//! Loopback-only mobile interface adapter. It deliberately exposes a small
//! Router-owned command vocabulary instead of forwarding Tauri commands or
//! provider identifiers over HTTP.

use crate::{
    send_chatgpt_request_for_workstream, EndpointPairingInput, EndpointPairingSideInput,
    ExternalProjectLinkInput, MobileApproveInput, MobileAttachmentSelectionInput, MobileCodexGoalControlInput,
    MobileCodexPrepareInput, MobileCodexResponseInput, MobileCodexTurnInterruptInput,
    MobileFeedbackInput, MobileFeedbackProgress, MobilePrepareInput, MobileSendInput, RouterCore,
};
use axum::{
    extract::{Path, State, Query},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle, time::timeout};
use tower_http::services::{ServeDir, ServeFile};

const ACCESS_JWT_HEADER: &str = "cf-access-jwt-assertion";
#[path="assistant_http.rs"]
mod assistant_http;

#[cfg(all(test, windows))]
#[path = "mobile_real_provider_ui_tests.rs"]
mod real_provider_ui_tests;

/// These values define the authenticated mobile routing boundary, not an
/// account credential or Push capability. Explorer may retain an old user
/// environment after the deployment variables are changed, so a Start Menu
/// launch must be able to reuse the last valid, user-local deployment config.
/// Environment values always take precedence and refresh this cache.
#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct PersistedMobileRuntimeConfig {
    access_issuer: String,
    access_audience: String,
    access_jwks_url: String,
    allowed_host: String,
    allowed_origin: String,
    port: u16,
    #[serde(default,skip_serializing_if="Option::is_none")] connection_method:Option<String>,
    #[serde(default)] managed:bool,
}

pub(crate) fn mobile_runtime_config_path() -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
    Ok(PathBuf::from(local).join("AIWorkRouter").join("data").join("mobile-runtime.json"))
}

pub(crate) fn load_persisted_mobile_runtime_config() -> Option<PersistedMobileRuntimeConfig> {
    let path = mobile_runtime_config_path().ok()?;
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Web Push signs against the same public mobile origin as the authenticated
/// HTTP surface.  A Start Menu process can lack Explorer's freshly-updated
/// environment, so push delivery must share the already validated runtime
/// fallback instead of silently becoming unavailable while the PWA still
/// opens.
pub(crate) fn configured_mobile_allowed_origin() -> Option<String> {
    mobile_allowed_origin_from(
        if load_persisted_mobile_runtime_config().is_some_and(|p|p.managed){None}else{std::env::var("AI_WORK_ROUTER_MOBILE_ALLOWED_ORIGIN").ok()},
        load_persisted_mobile_runtime_config().as_ref(),
    )
}

fn mobile_allowed_origin_from(
    environment_origin: Option<String>,
    persisted: Option<&PersistedMobileRuntimeConfig>,
) -> Option<String> {
    environment_origin.or_else(|| persisted.map(|config| config.allowed_origin.clone()))
}

fn save_persisted_mobile_runtime_config(config: &PersistedMobileRuntimeConfig) -> Result<(), String> {
    let path = mobile_runtime_config_path()?;
    let parent = path.parent().ok_or("Mobile runtime configuration has no parent directory")?;
    fs::create_dir_all(parent).map_err(|error| format!("Mobile runtime configuration directory is unavailable: {error}"))?;
    let bytes = serde_json::to_vec(config).map_err(|_| "Mobile runtime configuration cannot be encoded")?;
    let temporary = path.with_extension("json.pending");
    fs::write(&temporary, bytes).map_err(|error| format!("Mobile runtime configuration cannot be written: {error}"))?;
    fs::rename(&temporary, &path).map_err(|error| format!("Mobile runtime configuration cannot be finalized: {error}"))
}

#[derive(Clone,PartialEq)]
pub(crate) struct MobileHttpConfig {
    pub(crate) port: u16,
    pub(crate) allowed_host: String,
    pub(crate) allowed_origin: String,
    pub(crate) access_issuer: String,
    pub(crate) access_audience: String,
    pub(crate) access_jwks_url: String,
    pub(crate) static_dir: PathBuf,
}

impl MobileHttpConfig {
    fn validate_boundary(self)->Result<Self,String>{
        if self.allowed_origin==format!("http://127.0.0.1:{}",self.port)&&self.allowed_host==format!("127.0.0.1:{}",self.port){return Ok(self);}
        let (_,authority)=crate::mobile_connection::validate(&crate::mobile_connection::ConnectionInput{method:"CUSTOM_HTTPS".into(),origin:self.allowed_origin.clone()})?;
        if authority!=self.allowed_host{return Err("MOBILE_HOST_ORIGIN_MISMATCH".into());}Ok(self)
    }
    #[cfg(debug_assertions)]
    pub(crate) fn controlled_host_acceptance(static_dir: PathBuf) -> Self {
        Self {
            port: 47115,
            allowed_host: "router.local.test".into(),
            allowed_origin: "https://router.local.test".into(),
            access_issuer: "https://test.cloudflareaccess.invalid".into(),
            access_audience: "controlled-host-acceptance".into(),
            access_jwks_url: "https://test.cloudflareaccess.invalid/cdn-cgi/access/certs".into(),
            static_dir,
        }
    }

    /// Runtime deployment configuration belongs outside the repository. A
    /// missing config fails closed instead of treating loopback arrival as
    /// mobile authentication.
    pub(crate) fn from_env(static_dir: PathBuf) -> Result<Self, String> {
        let persisted = load_persisted_mobile_runtime_config();
        if let Some(p)=persisted.as_ref().filter(|p|p.managed){return p.config(static_dir).validate_boundary();}
        let access_issuer = std::env::var("AI_WORK_ROUTER_CLOUDFLARE_ACCESS_ISSUER").ok().or_else(||persisted.as_ref().map(|p|p.access_issuer.clone())).unwrap_or_default();
        let access_audience = std::env::var("AI_WORK_ROUTER_CLOUDFLARE_ACCESS_AUDIENCE").ok().or_else(||persisted.as_ref().map(|p|p.access_audience.clone())).unwrap_or_default();
        let access_jwks_url = std::env::var("AI_WORK_ROUTER_CLOUDFLARE_ACCESS_JWKS_URL").ok().or_else(||persisted.as_ref().map(|p|p.access_jwks_url.clone())).unwrap_or_else(||if access_issuer.is_empty(){String::new()}else{format!("{}/cdn-cgi/access/certs",access_issuer.trim_end_matches('/'))});
        let allowed_origin=std::env::var("AI_WORK_ROUTER_MOBILE_ALLOWED_ORIGIN").ok().or_else(||persisted.as_ref().map(|p|p.allowed_origin.clone()));
        let allowed_host=std::env::var("AI_WORK_ROUTER_MOBILE_ALLOWED_HOST").ok().or_else(||persisted.as_ref().map(|p|p.allowed_host.clone()));
        let port = std::env::var("AI_WORK_ROUTER_MOBILE_PORT")
            .ok()
            .map(|value| {
                value
                    .parse::<u16>()
                    .map_err(|_| "AI_WORK_ROUTER_MOBILE_PORT must be a valid u16")
            })
            .transpose()?
            .or_else(|| persisted.as_ref().map(|config| config.port))
            .unwrap_or(47114);
        if !static_dir.join("index.html").is_file() {
            return Err(format!(
                "Mobile HTTP is disabled: frontend build is missing at {}",
                static_dir.display()
            ));
        }
        let allowed_origin=allowed_origin.unwrap_or_else(||format!("http://127.0.0.1:{port}"));
        let allowed_host=allowed_host.unwrap_or_else(||url::Url::parse(&allowed_origin).map(|u|u[url::Position::BeforeHost..url::Position::AfterPort].to_owned()).unwrap_or_default());
        let config = Self {
            port,
            allowed_host,
            allowed_origin,
            access_issuer,
            access_audience,
            access_jwks_url,
            static_dir,
        };
        if config.allowed_origin.starts_with("https://"){
            let cached=PersistedMobileRuntimeConfig::from_config(&config,None,false);
            if persisted.as_ref()!=Some(&cached){save_persisted_mobile_runtime_config(&cached)?;}
        }
        config.validate_boundary()
    }
}

impl PersistedMobileRuntimeConfig{
    fn config(&self,static_dir:PathBuf)->MobileHttpConfig{MobileHttpConfig{port:self.port,allowed_host:self.allowed_host.clone(),allowed_origin:self.allowed_origin.clone(),access_issuer:self.access_issuer.clone(),access_audience:self.access_audience.clone(),access_jwks_url:self.access_jwks_url.clone(),static_dir}}
    fn from_config(c:&MobileHttpConfig,method:Option<String>,managed:bool)->Self{Self{port:c.port,allowed_host:c.allowed_host.clone(),allowed_origin:c.allowed_origin.clone(),access_issuer:c.access_issuer.clone(),access_audience:c.access_audience.clone(),access_jwks_url:c.access_jwks_url.clone(),connection_method:method,managed}}
}
pub(crate) fn config_method(c:&MobileHttpConfig)->String{
    if !c.allowed_origin.starts_with("https://"){return "NONE".into();}
    load_persisted_mobile_runtime_config().filter(|p|p.allowed_origin==c.allowed_origin).and_then(|p|p.connection_method).unwrap_or_else(||if c.access_issuer.is_empty(){"CUSTOM_HTTPS"}else{"CLOUDFLARE"}.into())
}
pub(crate) fn legacy_cloudflare_recovery(c:&MobileHttpConfig)->bool{config_method(c)=="CLOUDFLARE"&&!load_persisted_mobile_runtime_config().is_some_and(|p|p.managed)}
pub(crate) fn managed_config(current:&MobileHttpConfig)->MobileHttpConfig{load_persisted_mobile_runtime_config().filter(|p|p.managed).map(|p|p.config(current.static_dir.clone())).unwrap_or_else(||current.clone())}
pub(crate) fn config_bytes(c:&MobileHttpConfig,method:&str)->Result<Vec<u8>,String>{serde_json::to_vec(&PersistedMobileRuntimeConfig::from_config(c,Some(method.into()),true)).map_err(|_|"MOBILE_CONFIG_INVALID".into())}

#[derive(Clone)]
struct MobileHttpState {
    core: RouterCore,
    host: crate::HostRuntime,
    config: Arc<MobileHttpConfig>,
    jwks: Arc<Mutex<Option<JwkSet>>>,
    web_auth: Arc<crate::web_auth::WebAuth>,
    jwks_refresh: Arc<tokio::sync::Mutex<Option<std::time::Instant>>>,
    assistant_oauth: Arc<Mutex<assistant_http::AssistantOAuth>>,
}

pub(crate) struct MobileHttpHandle {
    shutdown: oneshot::Sender<()>,
    pub(crate) task: JoinHandle<()>,
    pub(crate) address: SocketAddr,
}

pub(crate) async fn start(
    core: RouterCore,
    config: MobileHttpConfig,
    host: crate::HostRuntime,
) -> Result<MobileHttpHandle, String> {
    #[cfg(test)]
    let web_auth=Arc::new(crate::web_auth::WebAuth::open(None)?);
    #[cfg(not(test))]
    let web_auth=crate::web_auth::global()?;
    start_with_web_auth(core,config,host,web_auth).await
}

pub(crate) async fn start_with_web_auth(core:RouterCore,config:MobileHttpConfig,host:crate::HostRuntime,web_auth:Arc<crate::web_auth::WebAuth>)->Result<MobileHttpHandle,String>{
    let address = SocketAddr::from(([127, 0, 0, 1], config.port));
    let listener = TcpListener::bind(address).await.map_err(|error| {
        format!(
            "Mobile HTTP could not bind 127.0.0.1:{}: {error}",
            config.port
        )
    })?;
    let address = listener.local_addr().map_err(|error| error.to_string())?;
    if !address.ip().is_loopback() {
        return Err("Mobile HTTP listener refused a non-loopback address".into());
    }
    let static_dir = config.static_dir.clone();
    #[cfg(test)]
    let assistant_oauth=Arc::new(Mutex::new(assistant_http::AssistantOAuth::default()));
    #[cfg(not(test))]
    let assistant_oauth=Arc::new(Mutex::new(assistant_http::AssistantOAuth::open(mobile_runtime_config_path()?.with_file_name("mcp-oauth-clients.json"))));
    let state = MobileHttpState {
        core,
        host,
        config: Arc::new(config),
        jwks: Arc::new(Mutex::new(None)),
        web_auth,
        jwks_refresh:Arc::new(tokio::sync::Mutex::new(None)),
        assistant_oauth,
    };
    let watch_api = Router::new()
        .route("/v1/mobile/codex-quota",get(codex_quota_read))
        .route("/v1/mobile/bridges",post(create_bridge))
        .route("/v1/mobile/bridges/{workstream_id}/rename",post(rename_bridge))
        .route("/v1/mobile/media",post(read_message_media))
        .route("/v1/mobile/connection/probe",get(connection_probe))
        .route("/v1/mobile/codex-watches", get(codex_watches).post(codex_watch_command))
        .route("/v1/mobile/codex-watches/events", get(codex_watch_feed))
        .route("/v1/mobile/codex-watches/events/{sequence}", get(codex_watch_event))
        .route("/v1/mobile/codex-watches/delivery", get(codex_delivery_settings).post(codex_delivery_command))
        .route("/v1/mobile/codex-watches/chat/{thread_id}",get(watch_chat_state))
        .route("/v1/mobile/codex-watches/chat",post(watch_chat_command).layer(axum::extract::DefaultBodyLimit::max(12*1024*1024)))
        .layer(axum::middleware::map_response(watch_no_store));
    let api = Router::new()
        .merge(watch_api)
        .route("/mcp",post(assistant_http::mcp))
        .route("/.well-known/oauth-protected-resource/mcp",get(assistant_http::metadata))
        .route("/.well-known/oauth-protected-resource",get(assistant_http::metadata))
        .route("/.well-known/oauth-authorization-server",get(assistant_http::authorization_metadata))
        .route("/oauth/register",post(assistant_http::register))
        .route("/oauth/authorize",get(assistant_http::authorize))
        .route("/oauth/token",post(assistant_http::token))
        .route("/assistant/connect",get(assistant_http::page))
        .route("/v1/mobile/assistant/consent",get(assistant_http::consent_info).post(assistant_http::consent))
        .route("/v1/mobile/assistant/settings",get(assistant_settings_view).post(assistant_connect_instance))
        .route("/v1/mobile/assistant/grants",get(assistant_http::grants))
        .route("/v1/mobile/assistant/grants/{gid}/revoke",post(assistant_http::revoke))
        .route("/v1/mobile/auth/session",get(web_auth_session))
        .route("/v1/mobile/auth/pair",post(web_auth_pair).layer(axum::extract::DefaultBodyLimit::max(1024)))
        .route("/v1/mobile/auth/logout",post(web_auth_logout))
        // A stale Service Worker can accept a real push but omit the exact
        // reply-render receipt.  It must always be revalidated when the PWA
        // opens; versioned registration alone cannot repair an older client
        // that still checks its original, unversioned script URL.
        .route("/service-worker.js", get(service_worker))
        .route("/v1/mobile/health", get(health))
        .route("/v1/mobile/push/config", get(push_config))
        .route("/v1/mobile/push/status", get(push_status))
        .route(
            "/v1/mobile/push/subscription",
            post(upsert_push_subscription).delete(remove_push_subscription),
        )
        .route("/v1/mobile/push/test", post(test_push))
        .route("/v1/mobile/push/test-reply", post(test_reply_push))
        .route("/v1/mobile/push/rendered", post(record_rendered_push))
        .route("/v1/mobile/workstreams", get(workstreams))
        .route("/v1/mobile/bridge-activity", post(bridge_activity))
        .route("/v1/mobile/workstreams/{workstream_id}", get(workstream))
        .route("/v1/mobile/workstreams/{workstream_id}/role-bridge", get(role_bridge_state).post(role_bridge_command))
        .route(
            "/v1/mobile/workstreams/{workstream_id}/project-links",
            get(project_links).post(upsert_project_link),
        )
        .route(
            "/v1/mobile/chatgpt-browser/setup",
            post(open_chatgpt_browser_setup),
        )
        .route("/v1/mobile/chatgpt-browser/authentication-completed", post(confirm_chatgpt_authentication_completed))
        .route(
            "/v1/mobile/workstreams/{workstream_id}/no-project-codex-thread",
            post(start_unprojected_codex_thread),
        )
        .route("/v1/mobile/backup", post(create_verified_local_backup))
        .route(
            "/v1/mobile/workstreams/{workstream_id}/draft",
            get(workstream_draft).post(save_workstream_draft),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/codex-feedback-drafts/{source_run_id}",
            get(codex_feedback_draft).post(save_codex_feedback_draft),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/lifecycle/archive",
            post(archive_workstream),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/lifecycle/pin",
            post(set_workstream_pinned),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/lifecycle/trash",
            post(trash_workstream),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/lifecycle/restore",
            post(restore_workstream),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/lifecycle/purge",
            post(purge_trashed_workstream),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/endpoint-pairing",
            post(pair_workstream_endpoints),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/chatgpt-binding/prepare",
            post(prepare_owner_confirmed_chatgpt_endpoint_binding),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/chatgpt-binding/confirm",
            post(confirm_explicit_chatgpt_endpoint_binding),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/codex-history",
            get(codex_history),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/codex-goal",
            get(codex_goal),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/codex-goal/pause",
            post(pause_codex_goal),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/codex-goal/resume",
            post(resume_codex_goal),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/codex-goal/clear",
            post(clear_codex_goal),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/codex-turns/{turn_id}/interrupt",
            post(interrupt_codex_turn),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/chatgpt-history",
            get(chatgpt_history),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/observed-responses",
            get(observed_responses),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/reply-observations",
            get(reply_observations),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/chatgpt-replies/check",
            post(check_new_chatgpt_replies),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/codex-replies/check",
            post(check_new_codex_replies),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/reply-observations/{observation_id}/read",
            post(mark_reply_observation_read),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/reply-observations/{observation_id}/handled",
            post(mark_reply_observation_handled),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/review-results",
            get(review_results),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/provider-runs/{run_id}",
            get(provider_run_status),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/chatgpt-result-recovery",
            post(recover_chatgpt_result),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/chatgpt-feedback/{run_id}/result-recovery",
            post(recover_chatgpt_feedback_result),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/codex-requests",
            get(codex_requests),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/chatgpt-feedback",
            post(chatgpt_feedback),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/chatgpt-feedback/{run_id}",
            get(chatgpt_feedback_progress),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/codex-feedback",
            post(codex_feedback),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/chatgpt-discussion",
            post(send_chatgpt_discussion),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/manual-chatgpt-destination",
            get(manual_chatgpt_discussion_destination),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/codex-requests/{request_id}/respond",
            post(respond_codex_request),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/handoff-review",
            post(prepare_handoff),
        )
        .route("/v1/mobile/handoff-review/{action_id}/attachments", post(select_handoff_attachments))
        .route(
            "/v1/mobile/handoff-review/{action_id}/approve",
            post(approve_handoff),
        )
        .route(
            "/v1/mobile/handoff-review/{action_id}/send",
            post(send_handoff),
        )
        .route(
            "/v1/mobile/workstreams/{workstream_id}/codex-handoff-review",
            post(prepare_codex_handoff),
        )
        .route(
            "/v1/mobile/codex-handoff-review/{action_id}/approve",
            post(approve_codex_handoff),
        )
        .route(
            "/v1/mobile/codex-handoff-review/{action_id}/send",
            post(send_codex_handoff),
        )
        .route(
            "/v1/mobile/codex-handoff-review/{action_id}/manual-destination",
            get(codex_handoff_manual_destination),
        )
        .route(
            "/v1/mobile/codex-handoff-review/{action_id}",
            get(codex_handoff_review),
        )
        .route(
            "/v1/mobile/chatgpt-handoff-review/{action_id}",
            get(chatgpt_handoff_review),
        )
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024))
        .with_state(state);
    let index = static_dir.join("index.html");
    // The mobile workbench is a client-side route (`/mobile`). Unknown browser
    // paths must therefore receive the SPA entry point with its successful
    // status, while explicit API routes above keep their own responses.
    let app = api.fallback_service(ServeDir::new(static_dir).fallback(ServeFile::new(index)));
    let (shutdown, receiver) = oneshot::channel();
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = receiver.await;
            })
            .await;
    });
    Ok(MobileHttpHandle {
        shutdown,
        task,
        address,
    })
}

impl MobileHttpHandle {
    pub(crate) fn is_finished(&self)->bool { self.task.is_finished() }
    pub(crate) async fn shutdown(self) {
        let _ = self.shutdown.send(());
        let _ = timeout(Duration::from_secs(2), self.task).await;
    }
}

#[derive(Serialize)]
struct MobileHealth {
    router: &'static str,
    mobile: &'static str,
}

type ApiResult<T> = Result<Json<T>, ApiError>;

#[cfg(test)]
#[test]
fn pairing_real_http_authenticates_and_revokes_without_access_jwt() {
 use crate::{RouterStore,Session};
 let directory=tempfile::tempdir().unwrap();std::fs::write(directory.path().join("index.html"),"public-login-shell").unwrap();
 let store=Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
 let core=RouterCore{store,chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::default()};
 let auth=Arc::new(crate::web_auth::WebAuth::open(Some(directory.path().join("auth.json"))).unwrap());
 let runtime=tokio::runtime::Runtime::new().unwrap();let config=MobileHttpConfig{port:0,allowed_host:"router.fixture.invalid".into(),allowed_origin:"https://router.fixture.invalid".into(),access_issuer:"https://unused.invalid".into(),access_audience:"unused".into(),access_jwks_url:"https://unused.invalid/keys".into(),static_dir:directory.path().into()};
 let host=crate::HostRuntime::default();let handle=runtime.block_on(start_with_web_auth(core,config,host.clone(),auth.clone())).unwrap();let base=format!("http://{}",handle.address);
 let client=reqwest::blocking::Client::new();let get=|path:&str,cookie:&str|client.get(format!("{base}{path}")).header("host","router.fixture.invalid").header("cookie",cookie).send().unwrap();
 assert_eq!(get("/v1/mobile/health","").status(),StatusCode::UNAUTHORIZED);
 assert_eq!(get("/v1/mobile/auth/session","").json::<serde_json::Value>().unwrap()["authenticated"],false);
 let c=auth.issue().unwrap();
 let pair=|origin:&str,code:&str|client.post(format!("{base}/v1/mobile/auth/pair")).header("host","router.fixture.invalid").header("origin",origin).json(&serde_json::json!({"code":code,"deviceName":"phone","remember":true})).send().unwrap();
 assert_eq!(pair("https://wrong.invalid",&c.code).status(),StatusCode::FORBIDDEN);
 let response=pair("https://router.fixture.invalid",&c.code);assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["cache-control"],"no-store");
 let raw=response.headers()["set-cookie"].to_str().unwrap().to_string();assert!(raw.contains("Secure; HttpOnly; SameSite=Strict"));assert!(raw.contains("Max-Age=7776000"));let cookie=raw.split(';').next().unwrap();
 assert_eq!(get("/v1/mobile/health",cookie).status(),StatusCode::OK);
 for path in ["/v1/mobile/codex-quota","/v1/mobile/assistant/settings"] {assert_eq!(get(path,"").status(),StatusCode::UNAUTHORIZED);assert_eq!(get(path,cookie).status(),StatusCode::OK);}
 let quota=get("/v1/mobile/codex-quota",cookie).json::<serde_json::Value>().unwrap();assert_eq!(quota["status"],"UNAVAILABLE");assert_eq!(quota["buckets"],serde_json::json!([]));
 let expiry=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64+30*86400000;
 let create=|origin:&str|client.post(format!("{base}/v1/mobile/assistant/settings")).header("host","router.fixture.invalid").header("cookie",cookie).header("origin",origin).json(&serde_json::json!({"label":"Fixture assistant","expiresAt":expiry})).send().unwrap();
 assert_eq!(create("https://wrong.invalid").status(),StatusCode::FORBIDDEN);
 assert!(get("/v1/mobile/assistant/settings",cookie).json::<serde_json::Value>().unwrap()["grants"].as_array().unwrap().is_empty());
 let grant=create("https://router.fixture.invalid").json::<serde_json::Value>().unwrap();assert_eq!(grant["scope"],"INSTANCE");assert_eq!(grant["approvalMode"],"CONVERSATION_REVIEW");


 let session=get("/v1/mobile/auth/session",cookie);assert!(session.headers()["set-cookie"].to_str().unwrap().contains("Max-Age=7776000"));assert_eq!(session.headers()["x-aiwr-host-instance"],host.instance_id());let session=session.json::<serde_json::Value>().unwrap();assert_eq!(session["method"],"DEVICE");assert_eq!(session["cacheScope"],format!("device:{}",auth.devices().unwrap()[0].id));assert!(!session.to_string().contains(cookie.split('=').nth(1).unwrap()));
 assert_eq!(client.get(format!("{base}/v1/mobile/auth/session")).header("host","router.fixture.invalid").header("cookie",cookie).header("origin","https://other.router.fixture.invalid").send().unwrap().status(),StatusCode::FORBIDDEN);
 assert_eq!(pair("https://router.fixture.invalid",&c.code).status(),StatusCode::UNAUTHORIZED);
 let forbidden=client.post(format!("{base}/v1/mobile/auth/logout")).header("host","router.fixture.invalid").header("cookie",cookie).header("origin","https://wrong.invalid").send().unwrap();assert_eq!(forbidden.status(),StatusCode::FORBIDDEN);assert_eq!(get("/v1/mobile/health",cookie).status(),StatusCode::OK);
 let logout=client.post(format!("{base}/v1/mobile/auth/logout")).header("host","router.fixture.invalid").header("cookie",cookie).header("origin","https://router.fixture.invalid").send().unwrap();assert_eq!(logout.status(),StatusCode::OK);assert!(logout.headers()["set-cookie"].to_str().unwrap().contains("Max-Age=0"));
 assert_eq!(get("/v1/mobile/health",cookie).status(),StatusCode::UNAUTHORIZED);
 let c=auth.issue().unwrap();let response=pair("https://router.fixture.invalid",&c.code);let raw=response.headers()["set-cookie"].to_str().unwrap().to_string();let cookie=raw.split(';').next().unwrap();let id=auth.devices().unwrap()[0].id.clone();auth.revoke(Some(&id),None).unwrap();assert_eq!(get("/v1/mobile/health",cookie).status(),StatusCode::UNAUTHORIZED);
 runtime.block_on(handle.shutdown());
}
async fn watch_no_store(mut response:axum::response::Response)->axum::response::Response {
    response.headers_mut().insert("cache-control",HeaderValue::from_static("no-store"));response
}

#[derive(Deserialize)]
#[serde(tag="action",rename_all="SCREAMING_SNAKE_CASE")]
enum WatchCommand { MarkSeen {sequence:i64},MarkRead {sequence:i64}, Remove {kind:String,id:String,removed:bool}, Enable { #[serde(rename="threadId")] thread_id:String }, Pause { #[serde(rename="threadId")] thread_id:String }, Threads, Connect }
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct WatchFeedQuery { #[serde(default)] after:i64, #[serde(default)] wait_seconds:u64 }
async fn codex_watches(State(state):State<MobileHttpState>,headers:HeaderMap)->ApiResult<Vec<router_core::store::codex_watch::CodexWatch>> {
    authenticated(&headers,&state,false).await?;
    Ok(Json(state.core.store.codex_watches().map_err(|e|ApiError(StatusCode::BAD_REQUEST,e))?))
}
async fn assistant_settings_view(State(state):State<MobileHttpState>,headers:HeaderMap)->ApiResult<serde_json::Value>{
 authenticated(&headers,&state,false).await?;crate::assistant_settings_view(&state.core).map(Json).map_err(core_error)
}
async fn assistant_connect_instance(State(state):State<MobileHttpState>,headers:HeaderMap,Json(input):Json<router_core::store::assistant::AssistantConnectionInput>)->ApiResult<router_core::store::assistant::AssistantGrant>{
 authenticated(&headers,&state,true).await?;state.core.store.connect_assistant_instance(input).map(Json).map_err(core_error)
}
async fn codex_quota_read(State(state):State<MobileHttpState>,headers:HeaderMap)->ApiResult<crate::codex_quota::Quota>{
 authenticated(&headers,&state,false).await?;
 Ok(Json(run_core_blocking("Codex quota",move||Ok(crate::codex_quota::read(&state.core))).await?))
}
async fn watch_chat_state(State(state):State<MobileHttpState>,headers:HeaderMap,Path(thread_id):Path<String>)->ApiResult<crate::watch_chat::ChatState>{authenticated(&headers,&state,false).await?;Ok(Json(run_core_blocking("Codex conversation",move||crate::watch_chat::state(&state.core,&thread_id)).await?))}
async fn watch_chat_command(State(state):State<MobileHttpState>,headers:HeaderMap,Json(input):Json<crate::watch_chat::ChatCommand>)->ApiResult<serde_json::Value>{authenticated(&headers,&state,true).await?;Ok(Json(run_core_blocking("Codex reply",move||crate::watch_chat::command(&state.core,input)).await?))}
async fn codex_watch_command(State(state):State<MobileHttpState>,headers:HeaderMap,Json(input):Json<WatchCommand>)->ApiResult<serde_json::Value>{
    authenticated(&headers,&state,true).await?;let core=state.core.clone();
    Ok(Json(run_core_blocking("Codex watch",move||{
        match input {
            WatchCommand::Enable{thread_id}=>{crate::codex_watch::enable(&core,&thread_id)?;Ok(serde_json::json!({"ok":true}))},
            WatchCommand::Remove{kind,id,removed}=>{core.store.set_watch_item_removed(&kind,&id,removed)?;Ok(serde_json::json!({"ok":true}))},
            WatchCommand::MarkSeen{sequence}=>{core.store.mark_codex_watch_event_seen(sequence)?;Ok(serde_json::json!({"ok":true}))},
              WatchCommand::MarkRead{sequence}=>{core.store.acknowledge_codex_watch_event(sequence)?;Ok(serde_json::json!({"ok":true}))},
            WatchCommand::Pause{thread_id}=>{core.store.pause_codex_watch(&thread_id)?;Ok(serde_json::json!({"ok":true}))},
            WatchCommand::Threads=>serde_json::to_value(crate::codex_watch::candidates(&core)?).map_err(|_|"WATCH_RESPONSE_INVALID".into()),
            WatchCommand::Connect=>serde_json::to_value(crate::host_application::connect_codex_service(&core,std::sync::Arc::new(router_core::events::NullEventSink))?).map_err(|_|"WATCH_RESPONSE_INVALID".into()),
        }
    }).await?))
}
async fn codex_watch_feed(State(state):State<MobileHttpState>,headers:HeaderMap,Query(query):Query<WatchFeedQuery>)->ApiResult<router_core::store::codex_watch::WatchFeed>{
    authenticated(&headers,&state,false).await?;
    if query.after<0 || query.wait_seconds>20 {return Err(ApiError(StatusCode::BAD_REQUEST,"WATCH_CURSOR_OR_WAIT_INVALID".into()));}
    let deadline=tokio::time::Instant::now()+Duration::from_secs(query.wait_seconds);
    loop {
        let feed=state.core.store.codex_watch_feed(query.after).map_err(|e|ApiError(StatusCode::BAD_REQUEST,e))?;
        if !feed.events.is_empty() || tokio::time::Instant::now()>=deadline {return Ok(Json(feed));}
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
async fn codex_watch_event(Path(sequence):Path<i64>,State(state):State<MobileHttpState>,headers:HeaderMap)->ApiResult<router_core::store::codex_watch::WatchEvent>{
    authenticated(&headers,&state,false).await?;
    Ok(Json(state.core.store.codex_watch_event(sequence).map_err(|_|ApiError(StatusCode::NOT_FOUND,"WATCH_EVENT_NOT_FOUND".into()))?))
}

async fn codex_delivery_settings(State(state):State<MobileHttpState>,headers:HeaderMap)->ApiResult<crate::watch_notifications::SettingsView>{
    authenticated(&headers,&state,false).await?;
    Ok(Json(run_core_blocking("Notification settings",move||crate::watch_notifications::view(&state.core.store)).await?))
}
async fn codex_delivery_command(State(state):State<MobileHttpState>,headers:HeaderMap,Json(input):Json<crate::watch_notifications::DeliveryCommand>)->ApiResult<String>{
    authenticated(&headers,&state,true).await?;
    match &input {
        crate::watch_notifications::DeliveryCommand::Channel{channel,..}|crate::watch_notifications::DeliveryCommand::Test{channel} if channel=="WEB"=>{},
        _=>return Err(ApiError(StatusCode::FORBIDDEN,"DELIVERY_DESKTOP_SETUP_REQUIRED".into()))
    }
    Ok(Json(run_core_blocking("Notification delivery",move||crate::watch_notifications::command(&state.core.store,input)).await?))
}

#[derive(Debug)]
struct ApiError(StatusCode, String);

/// Serve the Worker outside the generic static fallback so intermediaries and
/// browsers must revalidate it.  The Worker contains no reply text or
/// credentials, and its scope remains `/`.
async fn service_worker(
    State(state): State<MobileHttpState>,
) -> Result<(HeaderMap, Vec<u8>), ApiError> {
    service_worker_response(&state.config.static_dir)
}

fn service_worker_response(static_dir: &std::path::Path) -> Result<(HeaderMap, Vec<u8>), ApiError> {
    let source = std::fs::read(static_dir.join("service-worker.js")).map_err(|_| {
        ApiError(
            StatusCode::NOT_FOUND,
            "Mobile Service Worker is unavailable".into(),
        )
    })?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "cache-control",
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    headers.insert(
        "content-type",
        HeaderValue::from_static("application/javascript; charset=utf-8"),
    );
    Ok((headers, source))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MobileWorkstreamDraftInput {
    text: String,
    expected_revision: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MobileCodexFeedbackDraftInput {
    text: String,
    expected_revision: Option<i64>,
}

#[derive(Deserialize)]
struct MobileWorkstreamPinInput {
    pinned: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MobileWorkstreamPurgeInput {
    expected_binding_revision: i64,
    confirmation: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MobileEndpointPairingInput {
    expected_binding_revision: i64,
    chatgpt: Option<MobileEndpointPairingSideInput>,
    codex: Option<MobileEndpointPairingSideInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MobileEndpointPairingSideInput {
    expected_active_endpoint_id: Option<String>,
    external_id: String,
    label: String,
}

#[derive(Deserialize)]
struct MobileExplicitChatGptBindingPrepareInput {
    input: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MobileProjectLinkInput {
    provider: String,
    external_project_id: Option<String>,
    canonical_url: Option<String>,
    label: String,
    source_kind: String,
    source_version: Option<String>,
    verified_at: Option<i64>,
}

#[derive(Deserialize)]
struct MobileUnprojectedThreadInput {
    directory: Option<String>,
}

/// A normal discussion is deliberately distinct from an approved Handoff.
/// The exact workstream path selects the persisted destination; no provider
/// conversation identity is accepted from the mobile browser.
#[derive(Deserialize)]
struct MobileChatGptDiscussionInput {
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

#[derive(Clone, Deserialize)]
struct AccessClaims {
    #[serde(rename = "exp")]
    _exp: u64,
    #[serde(rename = "iss")]
    _iss: String,
    #[serde(rename = "aud")]
    _aud: serde_json::Value,
}

async fn fetch_access_jwks(state: &MobileHttpState) -> Result<JwkSet, ApiError> {
    reqwest::Client::builder().timeout(Duration::from_secs(5)).build().map_err(|_|ApiError(StatusCode::SERVICE_UNAVAILABLE,"Access key client unavailable".into()))?.get(&state.config.access_jwks_url).send()
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "Cloudflare Access signing keys are unavailable".into(),
            )
        })?
        .error_for_status()
        .map_err(|_| {
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "Cloudflare Access signing keys are unavailable".into(),
            )
        })?
        .json::<JwkSet>()
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "Cloudflare Access signing keys are invalid".into(),
            )
        })
}

#[cfg(test)]
fn access_jwks_for_kid_with_fetch<F>(
    cache: &Mutex<Option<JwkSet>>,
    kid: &str,
    fetch: F,
) -> Result<JwkSet, ApiError>
where
    F: FnOnce() -> Result<JwkSet, ApiError>,
{
    if let Some(keys) = cache
        .lock()
        .map_err(|_| {
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "Access key cache is unavailable".into(),
            )
        })?
        .clone()
    {
        if keys.find(kid).is_some() {
            return Ok(keys);
        }
    }
    let keys = fetch()?;
    *cache.lock().map_err(|_| {
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "Access key cache is unavailable".into(),
        )
    })? = Some(keys.clone());
    Ok(keys)
}

/// Use a cached signing set only when it contains this token's key. An
/// unknown `kid` causes exactly one refresh, so routine signing
/// key rotation works without a scheduler or an authentication downgrade.
async fn access_jwks_for_kid(state: &MobileHttpState, kid: &str) -> Result<JwkSet, ApiError> {
    if let Some(keys) = state
        .jwks
        .lock()
        .map_err(|_| {
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "Access key cache is unavailable".into(),
            )
        })?
        .clone()
    {
        if keys.find(kid).is_some() {
            return Ok(keys);
        }
    }
    // Public pairing ingress must not permit arbitrary JWT kids to fan out
    // unlimited signing-key fetches. Serialize and bound refresh attempts.
    let mut refresh=state.jwks_refresh.lock().await;
    if let Some(keys)=state.jwks.lock().map_err(|_|ApiError(StatusCode::SERVICE_UNAVAILABLE,"Access key cache unavailable".into()))?.clone(){if keys.find(kid).is_some(){return Ok(keys);}}
    if refresh.is_some_and(|last|last.elapsed()<Duration::from_secs(30)){return Err(ApiError(StatusCode::UNAUTHORIZED,"Cloudflare Access signing key is unknown".into()));}
    *refresh=Some(std::time::Instant::now());
    let keys = fetch_access_jwks(state).await?;
    *state.jwks.lock().map_err(|_| {
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "Access key cache is unavailable".into(),
        )
    })? = Some(keys.clone());
    Ok(keys)
}

async fn verified_access(headers: &HeaderMap, state: &MobileHttpState) -> Result<(), ApiError> {
    if state.config.access_issuer.is_empty()||state.config.access_audience.is_empty(){return Err(ApiError(StatusCode::UNAUTHORIZED,"Device pairing is required".into()));}
    let token = headers
        .get(ACCESS_JWT_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            ApiError(
                StatusCode::UNAUTHORIZED,
                "Cloudflare Access authentication is required".into(),
            )
        })?;
    let header = decode_header(token).map_err(|_| {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "Cloudflare Access token is invalid".into(),
        )
    })?;
    if header.alg != Algorithm::RS256 {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Cloudflare Access token algorithm is invalid".into(),
        ));
    }
    let kid = header.kid.ok_or_else(|| {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "Cloudflare Access token key is missing".into(),
        )
    })?;
    let keys = access_jwks_for_kid(state, &kid).await?;
    let key = DecodingKey::from_jwk(keys.find(&kid).ok_or_else(|| {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "Cloudflare Access signing key is unknown".into(),
        )
    })?)
    .map_err(|_| {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "Cloudflare Access signing key is invalid".into(),
        )
    })?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[&state.config.access_issuer]);
    validation.set_audience(&[&state.config.access_audience]);
    validation.set_required_spec_claims(&["exp", "iss", "aud"]);
    decode::<AccessClaims>(token, &key, &validation).map_err(|_| {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "Cloudflare Access token verification failed".into(),
        )
    })?;
    Ok(())
}

async fn connection_probe(State(state):State<MobileHttpState>,headers:HeaderMap,Query(input):Query<crate::mobile_connection::ProbeQuery>)->ApiResult<serde_json::Value>{
    state.host.verify_mobile_probe(headers.get("host").and_then(|h|h.to_str().ok()).unwrap_or(""),&input.nonce).map(Json).map_err(|_|ApiError(StatusCode::FORBIDDEN,"Connection probe unavailable".into()))
}

async fn authenticated(
    headers: &HeaderMap,
    state: &MobileHttpState,
    requires_origin: bool,
) -> Result<(), ApiError> {
    let host = headers.get("host").and_then(|value| value.to_str().ok());
    if host != Some(state.config.allowed_host.as_str()) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Host is not allowed".into(),
        ));
    }
    if requires_origin {
        let origin = headers.get("origin").and_then(|value| value.to_str().ok());
        if origin != Some(state.config.allowed_origin.as_str()) {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "Origin is not allowed".into(),
            ));
        }
    }
    if let Some(token)=crate::web_auth::cookie_token(headers){
        if state.web_auth.valid(&token){
            if headers.get("origin").is_some_and(|origin|origin.to_str().ok()!=Some(state.config.allowed_origin.as_str())){return Err(ApiError(StatusCode::FORBIDDEN,"Origin is not allowed".into()));}
            return Ok(());
        }
    }
    verified_access(headers, state).await
}

fn pairing_boundary(headers:&HeaderMap,state:&MobileHttpState,requires_origin:bool)->Result<(),ApiError>{
    if headers.get("host").and_then(|v|v.to_str().ok())!=Some(state.config.allowed_host.as_str()){return Err(ApiError(StatusCode::FORBIDDEN,"Host is not allowed".into()));}
    if requires_origin&&headers.get("origin").and_then(|v|v.to_str().ok())!=Some(state.config.allowed_origin.as_str()){return Err(ApiError(StatusCode::FORBIDDEN,"Origin is not allowed".into()));}
    Ok(())
}
async fn web_auth_session(State(state):State<MobileHttpState>,headers:HeaderMap)->Result<(HeaderMap,Json<serde_json::Value>),ApiError>{
    pairing_boundary(&headers,&state,false)?;
    if headers.get("origin").is_some_and(|value|value.to_str().ok()!=Some(state.config.allowed_origin.as_str())){return Err(ApiError(StatusCode::FORBIDDEN,"Origin is not allowed".into()));}
    let token=crate::web_auth::cookie_token(&headers);
    let renewed=token.as_ref().map(|t|state.web_auth.renew(t)).transpose().map_err(|e|ApiError(StatusCode::SERVICE_UNAVAILABLE,e))?.flatten();
    let device=renewed.is_some();
    let cf=!device&&headers.contains_key(ACCESS_JWT_HEADER)&&authenticated(&headers,&state,false).await.is_ok();
    let mut out=HeaderMap::new();out.insert("cache-control",HeaderValue::from_static("no-store"));
    if let (Some(token),Some(remember))=(&token,renewed){out.insert("set-cookie",crate::web_auth::session_cookie(token,remember).parse().map_err(|_|ApiError(StatusCode::INTERNAL_SERVER_ERROR,"WEB_AUTH_UNAVAILABLE".into()))?);}
    out.insert("x-aiwr-host-instance",state.host.instance_id().parse().unwrap());
    Ok((out,Json(serde_json::json!({"authenticated":device||cf,"cacheScope":if device{token.as_deref().and_then(|t|state.web_auth.cache_scope(t))}else{None},"method":if device{Some("DEVICE")}else if cf{Some("CLOUDFLARE")}else{None}}))))
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct WebPairInput {code:String,#[serde(default)]device_name:String,#[serde(default)]remember:bool}
async fn web_auth_pair(State(state):State<MobileHttpState>,headers:HeaderMap,Json(input):Json<WebPairInput>)->Result<(HeaderMap,Json<serde_json::Value>),ApiError>{
    pairing_boundary(&headers,&state,true)?;
    let token=state.web_auth.exchange(&input.code,&input.device_name,input.remember).map_err(|e|ApiError(if e=="WEB_PAIRING_INVALID_OR_EXPIRED"{StatusCode::UNAUTHORIZED}else{StatusCode::SERVICE_UNAVAILABLE},e))?;
    let mut out=HeaderMap::new();out.insert("cache-control",HeaderValue::from_static("no-store"));out.insert("set-cookie",crate::web_auth::session_cookie(&token,input.remember).parse().map_err(|_|ApiError(StatusCode::INTERNAL_SERVER_ERROR,"WEB_AUTH_UNAVAILABLE".into()))?);
    Ok((out,Json(serde_json::json!({"authenticated":true}))))
}
async fn web_auth_logout(State(state):State<MobileHttpState>,headers:HeaderMap)->Result<(HeaderMap,Json<serde_json::Value>),ApiError>{
    pairing_boundary(&headers,&state,true)?;
    if let Some(token)=crate::web_auth::cookie_token(&headers){state.web_auth.revoke(None,Some(&token)).map_err(|e|ApiError(StatusCode::SERVICE_UNAVAILABLE,e))?;}
    let mut out=HeaderMap::new();out.insert("cache-control",HeaderValue::from_static("no-store"));out.insert("set-cookie",crate::web_auth::clear_cookie().parse().unwrap());
    Ok((out,Json(serde_json::json!({"authenticated":false}))))
}

fn core_error(error: String) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, error)
}

/// Router Core owns several bounded, synchronous provider and app-server
/// operations.  A mobile HTTP request must await their result without holding
/// an Axum executor thread: otherwise one slow exact read or acceptance check
/// makes unrelated Workbench refreshes appear frozen.
async fn run_core_blocking<T, F>(operation: &'static str, task: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|error| core_error(format!("{operation} worker ended unexpectedly: {error}")))?
        .map_err(core_error)
}

async fn health(
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<MobileHealth> {
    authenticated(&headers, &state, false).await?;
    Ok(Json(MobileHealth {
        router: "ONLINE",
        mobile: "AVAILABLE",
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PushConfigResponse {
    public_key: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PushStatusResponse {
    /// Count only; never expose a subscription endpoint or capability.
    active_subscription_count: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemovePushSubscription {
    endpoint: String,
}

/// Sent by the same-origin PWA Service Worker only after its privacy-safe
/// `showNotification` promise resolves. It contains opaque Router IDs, never
/// reply text, provider identities, or PushSubscription capability material.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushRenderedReceipt {
    workstream_id: String,
    observation_id: String,
}

async fn push_config(
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<PushConfigResponse> {
    authenticated(&headers, &state, false).await?;
    crate::push::public_key()
        .map(|public_key| Json(PushConfigResponse { public_key }))
        .map_err(core_error)
}
async fn push_status(
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<PushStatusResponse> {
    authenticated(&headers, &state, false).await?;
    crate::push::subscription_count()
        .map(|active_subscription_count| Json(PushStatusResponse { active_subscription_count }))
        .map_err(core_error)
}
async fn upsert_push_subscription(
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<crate::push::PushSubscriptionInput>,
) -> ApiResult<serde_json::Value> {
    authenticated(&headers, &state, true).await?;
    let fingerprint = crate::push::subscription_fingerprint(&input);
    crate::push::upsert_subscription(input).map_err(core_error)?;
    state
        .core
        .store
        .upsert_push_subscription_metadata(&fingerprint)
        .map_err(core_error)?;
    Ok(Json(serde_json::json!({})))
}
async fn remove_push_subscription(
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<RemovePushSubscription>,
) -> ApiResult<serde_json::Value> {
    authenticated(&headers, &state, true).await?;
    let input = crate::push::PushSubscriptionInput {
        endpoint: input.endpoint,
        keys: crate::push::PushSubscriptionKeys {
            p256dh: String::new(),
            auth: String::new(),
        },
    };
    let fingerprint = crate::push::subscription_fingerprint(&input);
    crate::push::remove_subscription(&input.endpoint).map_err(core_error)?;
    state
        .core
        .store
        .deactivate_push_subscription_metadata(&fingerprint)
        .map_err(core_error)?;
    Ok(Json(serde_json::json!({})))
}
async fn test_push(
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    let delivery = run_core_blocking("Mobile Web Push test", move || {
        let delivery = crate::push::send_test()?;
        record_push_delivery(&core, &delivery)?;
        Ok(delivery)
    })
    .await?;
    push_delivery_response(delivery)
}

fn record_push_delivery(
    core: &crate::RouterCore,
    delivery: &crate::push::PushDeliveryOutcome,
) -> Result<(), String> {
    let delivery_state = match delivery {
        crate::push::PushDeliveryOutcome::Sent { .. } => "SENT",
        crate::push::PushDeliveryOutcome::NoSubscription => "NO_SUBSCRIPTION",
        crate::push::PushDeliveryOutcome::Failed { .. } => "FAILED",
    };
    core.store.record_push_subscription_attempt(delivery_state)?;
    let invalid_fingerprints = match delivery {
        crate::push::PushDeliveryOutcome::Sent {
            invalid_subscription_fingerprints,
        }
        | crate::push::PushDeliveryOutcome::Failed {
            invalid_subscription_fingerprints,
            ..
        } => invalid_subscription_fingerprints.clone(),
        crate::push::PushDeliveryOutcome::NoSubscription => vec![],
    };
    for fingerprint in invalid_fingerprints {
        core.store.deactivate_push_subscription_metadata(&fingerprint)?;
    }
    Ok(())
}

fn push_delivery_response(
    delivery: crate::push::PushDeliveryOutcome,
) -> ApiResult<serde_json::Value> {
    match delivery {
        crate::push::PushDeliveryOutcome::Sent { .. } => {}
        crate::push::PushDeliveryOutcome::NoSubscription => {
            return Err(core_error(
                "No local Web Push subscription is registered".into(),
            ))
        }
        crate::push::PushDeliveryOutcome::Failed { diagnostic, .. } => {
            return Err(core_error(
                diagnostic
                    .as_ref()
                    .map(crate::push::PushFailureDiagnostic::sanitized_message)
                    .unwrap_or_else(|| "Web Push delivery failed without an HTTP response".into()),
            ))
        }
    }
    Ok(Json(serde_json::json!({})))
}

/// This does not create an Observation or contact either provider.  It sends
/// the exact privacy-safe JSON shape handled by the PWA for a real reply, so
/// an owner can verify the production notification renderer separately from
/// the plain capability probe.
async fn test_reply_push(
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    let delivery = run_core_blocking("Mobile Web Push reply-shape test", move || {
        let delivery = crate::push::send_reply_notification_probe()?;
        record_push_delivery(&core, &delivery)?;
        Ok(delivery)
    })
    .await?;
    push_delivery_response(delivery)
}

async fn record_rendered_push(
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(receipt): Json<PushRenderedReceipt>,
) -> ApiResult<serde_json::Value> {
    authenticated(&headers, &state, true).await?;
    if receipt.workstream_id.len() > 128 || receipt.observation_id.len() > 128 {
        return Err(core_error("Push render receipt identity is invalid".into()));
    }
    state
        .core
        .store
        .record_reply_push_rendered(&receipt.workstream_id, &receipt.observation_id)
        .map_err(core_error)?;
    Ok(Json(serde_json::json!({})))
}

#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]
struct BridgeActivityInput { workstream_ids:Vec<String> }
async fn bridge_activity(State(state):State<MobileHttpState>,headers:HeaderMap,Json(input):Json<BridgeActivityInput>)->ApiResult<Vec<crate::role_bridge::DirectoryActivity>>{
 authenticated(&headers,&state,true).await?;let core=state.core.clone();
 run_core_blocking("Bridge activity",move||crate::role_bridge::directory_activity(&core,&input.workstream_ids)).await.map(Json)
}
async fn workstreams(
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<Vec<crate::MobileWorkstreamItem>> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_workstreams()
        .map(Json)
        .map_err(core_error)
}

async fn workstream(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::WorkspaceSnapshot> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_workstream_snapshot(&workstream_id)
        .map(Json)
        .map_err(core_error)
}

async fn project_links(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<Vec<crate::ExternalProjectLink>> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_external_project_links(&workstream_id)
        .map(Json)
        .map_err(core_error)
}

async fn upsert_project_link(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileProjectLinkInput>,
) -> ApiResult<crate::ExternalProjectLink> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .upsert_mobile_external_project_link(
            &workstream_id,
            ExternalProjectLinkInput {
                provider: input.provider,
                external_project_id: input.external_project_id,
                canonical_url: input.canonical_url,
                label: input.label,
                source_kind: input.source_kind,
                source_version: input.source_version,
                verified_at: input.verified_at,
            },
        )
        .map(Json)
        .map_err(core_error)
}

/// A user-visible mobile action may request the same local, dedicated setup
/// browser as desktop. It never accepts a provider URL, browser identity, or
/// credential, and cannot run as part of the normal directory-read route.
#[derive(Deserialize)]
#[serde(tag="action",rename_all="SCREAMING_SNAKE_CASE",rename_all_fields="camelCase")]
enum RoleBridgeCommand {
 Bind{revision:i64,decision:router_core::store::role_bridge::RoleBindingInput,execution:router_core::store::role_bridge::RoleBindingInput},
 Read{role:String},Sync,Prepare{role:String,observation_id:String,text:String,#[serde(default)] attachment_ids:Vec<String>},Attachments{role:String,observation_id:String},Blocks{role:String,observation_id:String},Edit{handoff_id:String,expected_hash:String,text:String},Approve{handoff_id:String,expected_hash:String},Send{handoff_id:String},Threads,Connect,
}
async fn role_bridge_state(Path(workstream_id):Path<String>,State(state):State<MobileHttpState>,headers:HeaderMap)->ApiResult<crate::role_bridge::BridgeState>{
 authenticated(&headers,&state,false).await?;
 crate::role_bridge::state(&state.core,&workstream_id).map(Json).map_err(core_error)
}
async fn role_bridge_command(Path(workstream_id):Path<String>,State(state):State<MobileHttpState>,headers:HeaderMap,Json(input):Json<RoleBridgeCommand>)->ApiResult<serde_json::Value>{
 authenticated(&headers,&state,true).await?;let core=state.core.clone();
 let result=run_core_blocking("Role Bridge command",move||{
  let encode=|v|serde_json::to_value(v).map_err(|_|"BRIDGE_RESPONSE_UNAVAILABLE".to_string());
  match input {
   RoleBridgeCommand::Bind{revision,decision,execution}=>encode(crate::role_bridge::bind(&core,&workstream_id,revision,decision,execution)?),
   RoleBridgeCommand::Read{role}=>serde_json::to_value(crate::role_bridge::read(&core,&workstream_id,&role)?).map_err(|_|"BRIDGE_RESPONSE_UNAVAILABLE".to_string()),
   RoleBridgeCommand::Sync=>serde_json::to_value(crate::role_bridge::sync(&core,&workstream_id)?).map_err(|_|"BRIDGE_RESPONSE_UNAVAILABLE".to_string()),
   RoleBridgeCommand::Prepare{role,observation_id,text,attachment_ids}=>serde_json::to_value(crate::role_bridge::prepare(&core,&workstream_id,&role,&observation_id,&text,&attachment_ids)?).map_err(|_|"BRIDGE_RESPONSE_UNAVAILABLE".to_string()),
   RoleBridgeCommand::Approve{handoff_id,expected_hash}=>{if core.store.role_handoff(&handoff_id)?.workstream_id!=workstream_id{return Err("BRIDGE_WORKSTREAM_MISMATCH".into());}serde_json::to_value(crate::role_bridge::approve(&core,&handoff_id,&expected_hash)?).map_err(|_|"BRIDGE_RESPONSE_UNAVAILABLE".to_string())},
   RoleBridgeCommand::Send{handoff_id}=>{if core.store.role_handoff(&handoff_id)?.workstream_id!=workstream_id{return Err("BRIDGE_WORKSTREAM_MISMATCH".into());}serde_json::to_value(crate::role_bridge::send(&core,&handoff_id)?).map_err(|_|"BRIDGE_RESPONSE_UNAVAILABLE".to_string())},
   RoleBridgeCommand::Blocks{role,observation_id}=>serde_json::to_value(crate::role_bridge::blocks(&core,&workstream_id,&role,&observation_id)?).map_err(|_|"BRIDGE_RESPONSE_UNAVAILABLE".to_string()),
   RoleBridgeCommand::Attachments{role,observation_id}=>serde_json::to_value(crate::role_bridge::attachments(&core,&workstream_id,&role,&observation_id)?).map_err(|_|"BRIDGE_RESPONSE_UNAVAILABLE".to_string()),
   RoleBridgeCommand::Edit{handoff_id,expected_hash,text}=>{if core.store.role_handoff(&handoff_id)?.workstream_id!=workstream_id{return Err("BRIDGE_WORKSTREAM_MISMATCH".into());}serde_json::to_value(core.store.edit_role_handoff(&handoff_id,&expected_hash,&text)?).map_err(|_|"BRIDGE_RESPONSE_UNAVAILABLE".to_string())},
   RoleBridgeCommand::Threads=>serde_json::to_value(crate::role_bridge::catalog(&core)?).map_err(|_|"BRIDGE_RESPONSE_UNAVAILABLE".to_string()),
   RoleBridgeCommand::Connect=>serde_json::to_value(crate::host_application::connect_codex_service(&core,std::sync::Arc::new(router_core::events::NullEventSink))?).map_err(|_|"BRIDGE_RESPONSE_UNAVAILABLE".to_string()),
  }
 }).await?;Ok(Json(result))
}

async fn open_chatgpt_browser_setup(
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    authenticated(&headers, &state, true).await?;
    let host = state.host.clone();
    let core = state.core.clone();
    run_core_blocking("Mobile ChatGPT browser setup", move || {
        host.open_chatgpt_browser_setup(&core)
    })
    .await?;
    Ok(Json(serde_json::json!({})))
}

async fn confirm_chatgpt_authentication_completed(
    State(state): State<MobileHttpState>, headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    let result = run_core_blocking("Owner ChatGPT authentication confirmation", move ||
        core.chatgpt.owner_confirmed_authentication()).await?;
    Ok(Json(result))
}

async fn start_unprojected_codex_thread(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileUnprojectedThreadInput>,
) -> ApiResult<crate::UnprojectedThreadStart> {
    authenticated(&headers, &state, true).await?;
    // Scope the action to an exact existing Workstream, but do not bind the
    // new thread to it: no-project creation is deliberately independent.
    state
        .core
        .mobile_workstream_snapshot(&workstream_id)
        .map_err(core_error)?;
    let core = state.core.clone();
    run_core_blocking("Mobile no-project Codex thread", move || {
        core.start_mobile_unprojected_codex_thread(input.directory)
    })
    .await
    .map(Json)
}

async fn create_verified_local_backup(
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::persistence::VerifiedBackup> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .create_mobile_verified_local_backup()
        .map(Json)
        .map_err(core_error)
}

async fn workstream_draft(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<Option<crate::persistence::WorkstreamDraft>> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_workstream_draft(&workstream_id)
        .map(Json)
        .map_err(core_error)
}

async fn save_workstream_draft(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileWorkstreamDraftInput>,
) -> ApiResult<crate::persistence::WorkstreamDraft> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .save_mobile_workstream_draft(&workstream_id, input.text, input.expected_revision)
        .map(Json)
        .map_err(core_error)
}

async fn codex_feedback_draft(
    Path((workstream_id, source_run_id)): Path<(String, String)>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<Option<crate::persistence::CodexFeedbackDraft>> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_codex_feedback_draft(&workstream_id, &source_run_id)
        .map(Json)
        .map_err(core_error)
}

async fn save_codex_feedback_draft(
    Path((workstream_id, source_run_id)): Path<(String, String)>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileCodexFeedbackDraftInput>,
) -> ApiResult<crate::persistence::CodexFeedbackDraft> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .save_mobile_codex_feedback_draft(
            &workstream_id,
            &source_run_id,
            input.text,
            input.expected_revision,
        )
        .map(Json)
        .map_err(core_error)
}

async fn archive_workstream(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::persistence::Workstream> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .archive_mobile_workstream(&workstream_id)
        .map(Json)
        .map_err(core_error)
}

async fn set_workstream_pinned(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileWorkstreamPinInput>,
) -> ApiResult<crate::persistence::Workstream> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .set_mobile_workstream_pinned(&workstream_id, input.pinned)
        .map(Json)
        .map_err(core_error)
}

async fn trash_workstream(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::persistence::Workstream> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .trash_mobile_workstream(&workstream_id)
        .map(Json)
        .map_err(core_error)
}

async fn restore_workstream(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::persistence::Workstream> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .restore_mobile_workstream(&workstream_id)
        .map(Json)
        .map_err(core_error)
}

async fn purge_trashed_workstream(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileWorkstreamPurgeInput>,
) -> ApiResult<serde_json::Value> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .purge_mobile_trashed_workstream(
            &workstream_id,
            input.expected_binding_revision,
            &input.confirmation,
        )
        .map(|_| Json(serde_json::json!({})))
        .map_err(core_error)
}

async fn codex_history(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::ReadHistoryResult> {
    authenticated(&headers, &state, false).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile Codex history", move || {
        core.read_active_codex_history(&workstream_id)
    })
    .await
    .map(Json)
}

async fn pair_workstream_endpoints(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileEndpointPairingInput>,
) -> ApiResult<crate::persistence::EndpointPairingResult> {
    authenticated(&headers, &state, true).await?;
    let side = |value: MobileEndpointPairingSideInput| EndpointPairingSideInput {
        expected_active_endpoint_id: value.expected_active_endpoint_id,
        external_id: value.external_id,
        label: value.label,
    };
    state
        .core
        .pair_mobile_workstream_endpoints(
            &workstream_id,
            EndpointPairingInput {
                expected_binding_revision: input.expected_binding_revision,
                chatgpt: input.chatgpt.map(side),
                codex: input.codex.map(side),
            },
        )
        .map(Json)
        .map_err(core_error)
}

/// Mobile accepts only an exact URL the owner already checked in the default
/// browser. This endpoint must not start, read, navigate, or control a browser.
async fn prepare_owner_confirmed_chatgpt_endpoint_binding(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileExplicitChatGptBindingPrepareInput>,
) -> ApiResult<crate::ExplicitChatGptBindingCandidate> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile owner-confirmed ChatGPT binding prepare", move || {
        core.prepare_owner_confirmed_chatgpt_endpoint_binding(&workstream_id, &input.input)
    })
    .await
    .map(Json)
}

/// Confirmation deliberately accepts no request body. The Core can persist
/// only the exact identity held by its current session candidate.
async fn confirm_explicit_chatgpt_endpoint_binding(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::persistence::Endpoint> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile explicit ChatGPT binding confirmation", move || {
        core.confirm_explicit_chatgpt_endpoint_binding(&workstream_id)
    })
    .await
    .map(Json)
}

async fn codex_goal(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<Option<crate::MobileCodexGoal>> {
    authenticated(&headers, &state, false).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile Codex Goal read", move || {
        core.mobile_codex_goal(&workstream_id)
    })
    .await
    .map(Json)
}

async fn pause_codex_goal(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileCodexGoalControlInput>,
) -> ApiResult<crate::MobileCodexGoal> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile Codex Goal pause", move || {
        core.pause_mobile_codex_goal(&workstream_id, input)
    })
    .await
    .map(Json)
}

async fn resume_codex_goal(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileCodexGoalControlInput>,
) -> ApiResult<crate::MobileCodexGoal> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile Codex Goal resume", move || {
        core.resume_mobile_codex_goal(&workstream_id, input)
    })
    .await
    .map(Json)
}

async fn clear_codex_goal(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileCodexGoalControlInput>,
) -> ApiResult<serde_json::Value> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile Codex Goal clear", move || {
        core.clear_mobile_codex_goal(&workstream_id, input)
    })
    .await?;
    Ok(Json(serde_json::json!({})))
}

async fn interrupt_codex_turn(
    Path((workstream_id, turn_id)): Path<(String, String)>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileCodexTurnInterruptInput>,
) -> ApiResult<crate::MobileCodexTurnInterruptResult> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile Codex turn interrupt", move || {
        core.interrupt_mobile_codex_turn(&workstream_id, &turn_id, input)
    })
    .await
    .map(Json)
}

async fn chatgpt_history(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::ConversationHistory> {
    authenticated(&headers, &state, false).await?;
    let core = state.core.clone();
    let host = state.host.clone();
    run_core_blocking("Mobile ChatGPT history", move || {
        host.read_active_chatgpt_history(&core, &workstream_id)
    })
    .await
    .map(Json)
}

async fn observed_responses(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<Vec<crate::MobileObservedResponse>> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_observed_responses(&workstream_id)
        .map(Json)
        .map_err(core_error)
}

async fn reply_observations(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<Vec<crate::MobileReplyObservation>> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_reply_observations(&workstream_id)
        .map(Json)
        .map_err(core_error)
}

/// Explicit mobile intent for one exact, no-send ChatGPT check. An
/// observation/push attempt is durable state, so this retains the normal
/// authenticated mutation boundary even though it never sends a provider turn.
async fn check_new_chatgpt_replies(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::ChatGptManualRefreshResult> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile ChatGPT reply check", move || {
        core.check_mobile_chatgpt_replies(&workstream_id)
    })
    .await
    .map(Json)
}

/// Explicit mobile intent for one passive, exact Codex thread read. It never
/// resumes or starts a Codex turn.
async fn check_new_codex_replies(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::CodexManualRefreshResult> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile Codex reply check", move || {
        core.check_mobile_codex_replies(&workstream_id)
    })
    .await
    .map(Json)
}

async fn mark_reply_observation_read(
    Path((workstream_id, observation_id)): Path<(String, String)>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .acknowledge_mobile_reply_observation(&workstream_id, &observation_id, false)
        .map_err(core_error)?;
    Ok(Json(serde_json::json!({})))
}

async fn mark_reply_observation_handled(
    Path((workstream_id, observation_id)): Path<(String, String)>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .acknowledge_mobile_reply_observation(&workstream_id, &observation_id, true)
        .map_err(core_error)?;
    Ok(Json(serde_json::json!({})))
}

async fn review_results(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<Vec<crate::MobileReviewResult>> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_review_results(&workstream_id)
        .map(Json)
        .map_err(core_error)
}

async fn provider_run_status(
    Path((workstream_id, run_id)): Path<(String, String)>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::MobileProviderRunStatus> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_provider_run_status(&workstream_id, &run_id)
        .map(Json)
        .map_err(core_error)
}

async fn recover_chatgpt_result(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::MobileChatGptResultRecovery> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile ChatGPT result recovery", move || {
        core.recover_active_chatgpt_result(&workstream_id)
    })
    .await
    .map(Json)
}

async fn recover_chatgpt_feedback_result(
    Path((workstream_id, run_id)): Path<(String, String)>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::MobileChatGptResultRecovery> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile ChatGPT feedback result recovery", move || {
        core.recover_chatgpt_feedback_result(&workstream_id, &run_id)
    })
    .await
    .map(Json)
}

async fn codex_requests(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<Vec<crate::MobileCodexRequest>> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_codex_requests(&workstream_id)
        .map(Json)
        .map_err(core_error)
}

async fn chatgpt_feedback(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileFeedbackInput>,
) -> ApiResult<crate::MobileFeedbackProgress> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    Ok(Json(
        run_core_blocking("Mobile ChatGPT feedback", move || {
            core.mobile_chatgpt_feedback(&workstream_id, input)
        })
        .await?,
    ))
}

async fn send_chatgpt_discussion(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileChatGptDiscussionInput>,
) -> ApiResult<crate::chatgpt::direct::CompletedChatGptResponse> {
    authenticated(&headers, &state, true).await?;
    if input.message.trim().is_empty() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "A discussion message cannot be empty".into(),
        ));
    }
    let core = state.core.clone();
    run_core_blocking("Mobile ChatGPT discussion", move || {
        send_chatgpt_request_for_workstream(&core, &workstream_id, &input.message)
    })
    .await
    .map(Json)
}

/// Resolves the exact current ChatGPT binding for an owner-led discussion.
/// This authenticated read neither opens a browser nor sends the draft.
async fn manual_chatgpt_discussion_destination(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::MobileManualChatGptDestination> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_current_chatgpt_manual_destination(&workstream_id)
        .map(Json)
        .map_err(core_error)
}

async fn chatgpt_feedback_progress(
    Path((workstream_id, run_id)): Path<(String, String)>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<MobileFeedbackProgress> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_chatgpt_feedback_progress(&workstream_id, &run_id)
        .map(Json)
        .map_err(core_error)
}

async fn codex_feedback(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileFeedbackInput>,
) -> ApiResult<crate::TurnStartResult> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile Codex feedback", move || {
        core.mobile_codex_feedback(&workstream_id, input)
    })
    .await
    .map(Json)
}

async fn respond_codex_request(
    Path((workstream_id, request_id)): Path<(String, String)>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileCodexResponseInput>,
) -> ApiResult<serde_json::Value> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Mobile Codex request response", move || {
        core.respond_mobile_codex_request(&workstream_id, &request_id, input)
    })
    .await?;
    Ok(Json(serde_json::json!({})))
}

async fn prepare_handoff(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobilePrepareInput>,
) -> ApiResult<crate::MobileHandoffReview> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .prepare_mobile_reverse(&workstream_id, input)
        .map(Json)
        .map_err(core_error)
}

async fn select_handoff_attachments(
    Path(action_id): Path<String>, State(state): State<MobileHttpState>,
    headers: HeaderMap, Json(input): Json<MobileAttachmentSelectionInput>,
) -> ApiResult<crate::MobileHandoffReview> {
    authenticated(&headers, &state, true).await?;
    let core = state.core.clone();
    run_core_blocking("Exact selected ChatGPT attachments", move || core.select_mobile_reverse_attachments(&action_id, input))
        .await.map(Json)
}

async fn approve_handoff(
    Path(action_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileApproveInput>,
) -> ApiResult<crate::MobileHandoffReview> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .approve_mobile_reverse(&action_id, input)
        .map(Json)
        .map_err(core_error)
}

async fn send_handoff(
    Path(action_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileSendInput>,
) -> ApiResult<crate::TurnStartResult> {
    authenticated(&headers, &state, true).await?;
    // Codex app-server calls are synchronous by contract.  Never occupy the
    // mobile HTTP executor while the first-write exact-ID readiness probe is
    // awaiting its bounded response; the review surface must remain readable
    // and a timeout must return a normal failed-closed API response.
    let core = state.core.clone();
    run_core_blocking("Mobile Codex dispatch", move || {
        core.send_mobile_reverse(&action_id, input)
    })
    .await
    .map(Json)
}

async fn prepare_codex_handoff(
    Path(workstream_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileCodexPrepareInput>,
) -> ApiResult<crate::MobileHandoffReview> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .prepare_mobile_codex_outbound(&workstream_id, input)
        .map(Json)
        .map_err(core_error)
}

async fn approve_codex_handoff(
    Path(action_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileApproveInput>,
) -> ApiResult<crate::MobileHandoffReview> {
    authenticated(&headers, &state, true).await?;
    state
        .core
        .approve_mobile_codex_outbound(&action_id, input)
        .map(Json)
        .map_err(core_error)
}

async fn send_codex_handoff(
    Path(action_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
    Json(input): Json<MobileSendInput>,
) -> ApiResult<crate::OutboundHandoffResult> {
    authenticated(&headers, &state, true).await?;
    // A reverse Handoff may wait through the bounded provider-acceptance
    // observation. Keep that synchronous Core path out of Axum's executor so
    // the mobile request resolves to its truthful SENDING/SENT/FAILED result
    // and the Workbench never remains visually stuck at APPROVED.
    let core = state.core.clone();
    let worker_core = core.clone();
    run_core_blocking("Mobile ChatGPT delivery", move || {
        worker_core.send_mobile_codex_outbound(&action_id, input)
    })
    .await
    .map(Json)
}

async fn codex_handoff_review(
    Path(action_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::MobileHandoffReview> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_codex_outbound_review(&action_id)
        .map(Json)
        .map_err(core_error)
}

async fn codex_handoff_manual_destination(
    Path(action_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::MobileManualChatGptDestination> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_codex_outbound_manual_destination(&action_id)
        .map(Json)
        .map_err(core_error)
}

async fn chatgpt_handoff_review(
    Path(action_id): Path<String>,
    State(state): State<MobileHttpState>,
    headers: HeaderMap,
) -> ApiResult<crate::MobileHandoffReview> {
    authenticated(&headers, &state, false).await?;
    state
        .core
        .mobile_chatgpt_inbound_review(&action_id)
        .map(Json)
        .map_err(core_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        fs,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };

    fn key_set(kid: &str) -> JwkSet {
        serde_json::from_value(json!({
            "keys": [{ "kid": kid, "kty": "RSA", "alg": "RS256", "use": "sig", "n": "AA", "e": "AQAB" }]
        }))
        .expect("test JWK set")
    }

    #[test]
    fn push_origin_uses_environment_first_and_cached_mobile_origin_when_absent() {
        let cached = PersistedMobileRuntimeConfig {connection_method:None,managed:false,
            access_issuer: "https://access.example".into(),
            access_audience: "audience".into(),
            access_jwks_url: "https://access.example/certs".into(),
            allowed_host: "router.example".into(),
            allowed_origin: "https://router.example".into(),
            port: 47114,
        };
        assert_eq!(
            mobile_allowed_origin_from(None, Some(&cached)).as_deref(),
            Some("https://router.example")
        );
        assert_eq!(
            mobile_allowed_origin_from(Some("https://override.example".into()), Some(&cached))
                .as_deref(),
            Some("https://override.example")
        );
        assert_eq!(mobile_allowed_origin_from(None, None), None);
    }

    #[test]
    fn known_cached_kid_does_not_refresh() {
        let cache = Mutex::new(Some(key_set("existing")));
        let fetched = AtomicUsize::new(0);
        let selected = access_jwks_for_kid_with_fetch(&cache, "existing", || {
            fetched.fetch_add(1, Ordering::SeqCst);
            Ok(key_set("replacement"))
        })
        .expect("cached key is accepted");
        assert!(selected.find("existing").is_some());
        assert_eq!(fetched.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn unknown_kid_refreshes_once_and_replaces_the_cache() {
        let cache = Mutex::new(Some(key_set("retired")));
        let fetched = AtomicUsize::new(0);
        let selected = access_jwks_for_kid_with_fetch(&cache, "rotated", || {
            fetched.fetch_add(1, Ordering::SeqCst);
            Ok(key_set("rotated"))
        })
        .expect("fresh rotated key is accepted");
        assert!(selected.find("rotated").is_some());
        assert_eq!(fetched.load(Ordering::SeqCst), 1);
        assert!(cache
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .find("retired")
            .is_none());
    }

    #[test]
    fn unknown_key_after_the_single_refresh_is_rejected_by_the_caller() {
        let cache = Mutex::new(Some(key_set("retired")));
        let selected = access_jwks_for_kid_with_fetch(&cache, "missing", || Ok(key_set("rotated")))
            .expect("a fetch can succeed even when it lacks the requested kid");
        assert!(selected.find("missing").is_none());
    }

    #[test]
    fn refresh_failure_preserves_cached_keys_and_fails_closed() {
        let cache = Mutex::new(Some(key_set("retired")));
        let error = access_jwks_for_kid_with_fetch(&cache, "rotated", || {
            Err(ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "unavailable".into(),
            ))
        })
        .expect_err("unavailable refresh must not admit a token");
        assert_eq!(error.0, StatusCode::SERVICE_UNAVAILABLE);
        assert!(cache
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .find("retired")
            .is_some());
    }

    #[test]
    fn service_worker_is_never_served_from_a_stale_http_cache() {
        let directory = tempfile::tempdir().expect("temporary static directory");
        fs::write(directory.path().join("service-worker.js"), b"self.skipWaiting()")
            .expect("Worker fixture");

        let (headers, source) = service_worker_response(directory.path()).expect("Worker response");
        assert_eq!(source, b"self.skipWaiting()");
        assert_eq!(
            headers.get("cache-control").and_then(|value| value.to_str().ok()),
            Some("no-cache, no-store, must-revalidate")
        );
        assert_eq!(
            headers.get("content-type").and_then(|value| value.to_str().ok()),
            Some("application/javascript; charset=utf-8")
        );
    }

    #[tokio::test]
    async fn core_blocking_worker_keeps_the_mobile_async_executor_responsive() {
        let (started, wait_for_start) = tokio::sync::oneshot::channel();
        let release = Arc::new(AtomicBool::new(false));
        let release_for_worker = Arc::clone(&release);
        let worker = tokio::spawn(run_core_blocking("test Core operation", move || {
            let _ = started.send(());
            while !release_for_worker.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok::<_, String>(())
        }));
        wait_for_start.await.expect("blocking task started");

        let progressed = Arc::new(AtomicBool::new(false));
        let progressed_task = Arc::clone(&progressed);
        tokio::spawn(async move {
            progressed_task.store(true, Ordering::SeqCst);
        })
        .await
        .expect("mobile executor remains schedulable");
        assert!(progressed.load(Ordering::SeqCst));

        release.store(true, Ordering::SeqCst);
        worker
            .await
            .expect("blocking wrapper task joined")
            .expect("blocking Core task completed");
    }
    #[cfg(windows)]
    pub(super) fn ephemeral_access_fixture() -> serde_json::Value {
        let generated = std::process::Command::new("node.exe").args(["-e", r#"
const c=require('node:crypto');
const {publicKey,privateKey}=c.generateKeyPairSync('rsa',{modulusLength:2048});
const jwk={...publicKey.export({format:'jwk'}),kid:'fixture-key',alg:'RS256',use:'sig'};
const enc=x=>Buffer.from(JSON.stringify(x)).toString('base64url');
const payload=enc({alg:'RS256',kid:jwk.kid})+'.'+enc({iss:'https://fixture.invalid',aud:'fixture-audience',exp:Math.floor(Date.now()/1000)+900});
const token=payload+'.'+c.sign('RSA-SHA256',Buffer.from(payload),privateKey).toString('base64url');
process.stdout.write(JSON.stringify({keys:[jwk],token}));
"#]).output().expect("Node builtin crypto generates ephemeral auth fixture");
        assert!(generated.status.success());
        serde_json::from_slice(&generated.stdout).unwrap()
    }

    #[test]
    #[cfg(windows)]
    fn authenticated_http_review_preserves_exact_approval_and_selected_hash_across_sqlite_reopen() {
        use crate::{Provider, RouterStore, Session};
        use std::collections::HashMap;
        // Generate a disposable signing key in Node memory. Only the public JWK
        // and short-lived local-test JWT enter Rust; no private key is saved.
        let generated = ephemeral_access_fixture();
        let token = generated["token"].as_str().unwrap().to_owned();
        let public_keys = json!({"keys":generated["keys"]});
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runtime/integrated-provider-router");
        fs::create_dir_all(&root).unwrap();
        let directory = tempfile::tempdir_in(root).unwrap();
        fs::write(directory.path().join("index.html"), "public mobile fixture").unwrap();
        fs::write(directory.path().join("browser-auth-required.json"), "{}").unwrap();
        let attachment = directory.path().join("selected.txt");
        fs::write(&attachment, "public exact selected bytes\n").unwrap();
        let hash = crate::sha256_path(&attachment).unwrap();
        let full_result = format!(
            "Public complete result\nARTIFACT:\n{}\nSHA256:\n{}",
            attachment.display(),
            hash
        );
        let database = directory.path().join("http-review.db");
        let store = Arc::new(RouterStore::open_at(&database).unwrap());
        let project = store
            .create_project("public HTTP Bridge fixture".into(), None)
            .unwrap();
        let bridge = store
            .create_workstream(&project.id, "Shared Bridge".into())
            .unwrap();
        store
            .bind_endpoint(
                &bridge.id,
                Provider::Chatgpt,
                "00000000-0000-4000-8000-000000000001".into(),
                "Decision".into(),
                false,
            )
            .unwrap();
        let source = store
            .bind_endpoint(
                &bridge.id,
                Provider::Codex,
                "fixture-exact-thread".into(),
                "Execution".into(),
                false,
            )
            .unwrap();
        let observation = store
            .record_reply_observation(
                &bridge.id,
                &source.id,
                Some("fixture-turn:fixture-item"),
                &full_result,
                None,
            )
            .unwrap()
            .unwrap();
        let candidate =
            crate::artifact::detector::detect("fixture-turn:fixture-item", &full_result)
                .into_iter()
                .find(|candidate| candidate.filename == "selected.txt")
                .unwrap();
        assert!(candidate
            .actual_sha256
            .as_deref()
            .unwrap()
            .eq_ignore_ascii_case(&hash));
        let make_core = |store: Arc<RouterStore>| {
            let core = RouterCore {
                store,
                chatgpt: Arc::default(),
                session: Arc::new(Mutex::new(Session::default())),
                completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
            };
            core.chatgpt
                .configure(router_core::chatgpt_service::ExecutorConfiguration {
                    node: directory.path().join("must-not-launch.exe"),
                    resources: directory.path().join("missing-package"),
                    data: directory.path().into(),
                })
                .unwrap();
            core
        };
        let core = make_core(store.clone());
        let host = crate::HostRuntime::default();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (keys_url, keys_task) = runtime.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = Router::new().route(
                "/certs",
                get(move || {
                    let keys = public_keys.clone();
                    async move { Json(keys) }
                }),
            );
            let task = tokio::spawn(async move {
                axum::serve(listener, server).await.unwrap();
            });
            (format!("http://{address}/certs"), task)
        });
        let config = || MobileHttpConfig {
            port: 0,
            allowed_host: "router.fixture.invalid".into(),
            allowed_origin: "https://router.fixture.invalid".into(),
            access_issuer: "https://fixture.invalid".into(),
            access_audience: "fixture-audience".into(),
            access_jwks_url: keys_url.clone(),
            static_dir: directory.path().into(),
        };
        let handle = runtime
            .block_on(start(core.clone(), config(), host.clone()))
            .unwrap();
        let origin = format!("http://{}", handle.address);
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let get = |url: String| {
            client
                .get(url)
                .header("host", "router.fixture.invalid")
                .header(ACCESS_JWT_HEADER, &token)
        };
        let post = |url: String| {
            client
                .post(url)
                .header("host", "router.fixture.invalid")
                .header("origin", "https://router.fixture.invalid")
                .header(ACCESS_JWT_HEADER, &token)
        };
        let list_url = format!("{origin}/v1/mobile/workstreams");
        assert_eq!(
            client
                .get(&list_url)
                .header("host", "router.fixture.invalid")
                .send()
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            client
                .get(&list_url)
                .header("host", "wrong.invalid")
                .header(ACCESS_JWT_HEADER, &token)
                .send()
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(get(list_url).send().unwrap().status(), StatusCode::OK);
        let source_url = format!(
            "{origin}/v1/mobile/workstreams/{}/reply-observations",
            bridge.id
        );
        let result = get(source_url).send().unwrap();
        assert_eq!(result.status(), StatusCode::OK);
        let source: serde_json::Value = result.json().unwrap();
        assert!(source[0]["text"]
            .as_str()
            .unwrap()
            .contains("Public complete result"));
        assert_eq!(source[0]["attachments"][0]["id"], candidate.id);
        assert_eq!(source[0]["attachments"][0]["filename"], "selected.txt");
        let prepare_url = format!(
            "{origin}/v1/mobile/workstreams/{}/codex-handoff-review",
            bridge.id
        );
        assert_eq!(
            client
                .post(prepare_url.clone())
                .header("host", "router.fixture.invalid")
                .header("origin", "https://wrong.invalid")
                .header(ACCESS_JWT_HEADER, &token)
                .json(&json!({"runId":observation.id}))
                .send()
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let prepared = post(prepare_url)
            .json(&json!({"runId":observation.id,"attachmentIds":[candidate.id]}))
            .send()
            .unwrap();
        assert_eq!(prepared.status(), StatusCode::OK);
        let prepared: serde_json::Value = prepared.json().unwrap();
        let action = prepared["actionId"].as_str().unwrap();
        let revision = prepared["revision"].as_u64().unwrap();
        assert_eq!(prepared["status"], "READY");
        assert_eq!(prepared["attachments"], json!(["selected.txt"]));
        let send_url = format!("{origin}/v1/mobile/codex-handoff-review/{action}/send");
        assert_eq!(
            post(send_url)
                .json(&json!({"revision":revision}))
                .send()
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert!(store
            .provider_runs_for_workstream(&bridge.id)
            .unwrap()
            .is_empty());
        let approved_bytes = "  Exact owner edit\n中文\n ";
        let approve_url = format!("{origin}/v1/mobile/codex-handoff-review/{action}/approve");
        assert_eq!(
            post(approve_url.clone())
                .json(&json!({"revision":revision+10,"message":approved_bytes}))
                .send()
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        let approved = post(approve_url)
            .json(&json!({"revision":revision,"message":approved_bytes}))
            .send()
            .unwrap();
        assert_eq!(approved.status(), StatusCode::OK);
        let approved: serde_json::Value = approved.json().unwrap();
        assert_eq!(approved["status"], "APPROVED");
        assert_eq!(
            approved["message"], approved_bytes,
            "HTTP approval must preserve the exact owner edit"
        );
        let retained = store.mobile_codex_outbound_review(action).unwrap().unwrap();
        assert_eq!(retained.approved_text.as_deref(), Some(approved_bytes));
        assert!(retained
            .attachments_json
            .to_ascii_lowercase()
            .contains(&hash));
        let action = action.to_owned();
        runtime.block_on(handle.shutdown());
        drop(core);
        drop(store);
        let reopened = Arc::new(RouterStore::open_at(&database).unwrap());
        let restored_core = make_core(reopened.clone());
        let restored = runtime
            .block_on(start(restored_core.clone(), config(), host.clone()))
            .unwrap();
        let url = format!(
            "http://{}/v1/mobile/codex-handoff-review/{action}",
            restored.address
        );
        let response = get(url).send().unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response: serde_json::Value = response.json().unwrap();
        assert_eq!(response["status"], "APPROVED");
        assert_eq!(response["message"], approved_bytes);
        assert_eq!(response["attachments"], json!(["selected.txt"]));
        assert_eq!(
            response["codexOutboundIdentity"]["sourceCodexThreadId"],
            "fixture-exact-thread"
        );
        assert_eq!(restored_core.chatgpt.status(), "AUTH_REQUIRED");
        assert!(restored_core.session.lock().unwrap().adapter.is_none());
        assert!(reopened
            .provider_runs_for_workstream(&bridge.id)
            .unwrap()
            .is_empty());
        // Explicit Send after reopen reaches the same Core stop, never a browser.
        let send_url = format!(
            "http://{}/v1/mobile/codex-handoff-review/{action}/send",
            restored.address
        );
        let blocked = post(send_url)
            .json(&json!({"revision":response["revision"]}))
            .send()
            .unwrap();
        assert_eq!(blocked.status(), StatusCode::OK);
        let blocked: serde_json::Value = blocked.json().unwrap();
        assert_eq!(blocked["status"], "FAILED");
        assert!(blocked["detail"]
            .as_str()
            .unwrap()
            .contains("AUTH_REQUIRED"));
        let runs = reopened.provider_runs_for_workstream(&bridge.id).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "FAILED");
        assert_eq!(runs[0].terminal_code.as_deref(), Some("AUTH_REQUIRED"));
        assert!(runs[0].external_run_id.is_none());
        let handoff = reopened
            .handoff_by_id(runs[0].origin_handoff_id.as_deref().unwrap())
            .unwrap();
        assert_eq!(handoff.approved_text, approved_bytes);
        assert_eq!(handoff.attachments.len(), 1);
        assert!(handoff.attachments[0]
            .sha256
            .as_deref()
            .unwrap()
            .eq_ignore_ascii_case(&hash));
        assert_eq!(restored_core.chatgpt.status(), "AUTH_REQUIRED");
        let history_url = format!(
            "http://{}/v1/mobile/codex-handoff-review/{action}",
            restored.address
        );
        let history = get(history_url).send().unwrap();
        assert_eq!(history.status(), StatusCode::OK);
        let history: serde_json::Value = history.json().unwrap();
        assert_eq!(history["status"], "FAILED");
        runtime.block_on(restored.shutdown());
        keys_task.abort();
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "opt-in local mobile UI validation; no real provider"]
    fn local_mobile_ui_http_harness() {
        assert_eq!(
            std::env::var("AIWR_LOCAL_MOBILE_UI_GATE").as_deref(),
            Ok("1")
        );
        use crate::{Provider, RouterStore, Session};
        use std::collections::HashMap;
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let evidence = root.join("runtime/integrated-provider-router");
        fs::create_dir_all(&evidence).unwrap();
        let scratch = PathBuf::from(r"D:\fixtures\临时处理");
        fs::create_dir_all(&scratch).unwrap();
        let directory = tempfile::Builder::new()
            .prefix("aiwr-mobile-ui-")
            .tempdir_in(scratch)
            .unwrap();
        fs::write(directory.path().join("browser-auth-required.json"), "{}").unwrap();
        let attachment = directory.path().join("selected.txt");
        fs::write(&attachment, "public local UI selected bytes\n").unwrap();
        let hash = crate::sha256_path(&attachment).unwrap();
        let text=format!("# Public complete HTTP result\n\nAIWR_LOCAL_MOBILE_UI_RESULT\n\nThis is an isolated public fixture served by the actual Router HTTP/Core.\n\nARTIFACT:\n{}\nSHA256:\n{}",attachment.display(),hash);
        let store = Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
        let project = store
            .create_project("Public local HTTP fixture".into(), None)
            .unwrap();
        let bridge = store
            .create_workstream(&project.id, "HTTP UI Bridge".into())
            .unwrap();
        store
            .bind_endpoint(
                &bridge.id,
                Provider::Chatgpt,
                "00000000-0000-4000-8000-000000000001".into(),
                "Decision fixture".into(),
                false,
            )
            .unwrap();
        let endpoint = store
            .bind_endpoint(
                &bridge.id,
                Provider::Codex,
                "fixture-exact-thread".into(),
                "Execution fixture".into(),
                false,
            )
            .unwrap();
        let observation = store
            .record_reply_observation(
                &bridge.id,
                &endpoint.id,
                Some("fixture-ui-turn:fixture-ui-item"),
                &text,
                None,
            )
            .unwrap()
            .unwrap();
        let core = RouterCore {
            store: store.clone(),
            chatgpt: Arc::default(),
            session: Arc::new(Mutex::new(Session::default())),
            completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        };
        core.chatgpt
            .configure(router_core::chatgpt_service::ExecutorConfiguration {
                node: directory.path().join("must-not-launch.exe"),
                resources: directory.path().join("missing-package"),
                data: directory.path().into(),
            })
            .unwrap();
        let host = crate::HostRuntime::default();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let signed = ephemeral_access_fixture();
        let token = signed["token"].as_str().unwrap().to_owned();
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
        let handle = runtime
            .block_on(start(core.clone(), config, host.clone()))
            .unwrap();
        let stop = evidence.join("local-mobile-ui-stop.json");
        assert!(
            !stop.exists(),
            "previous gate stop record must be retained/renamed before a new run"
        );
        let manifest = json!({"scope":"LOCAL_FIXTURE_ONLY_NO_PROVIDER","origin":origin,"token":token,"workstreamId":bridge.id,"observationId":observation.id,"attachmentFilename":"selected.txt","attachmentSha256":hash,"stopFile":stop,"expiresSeconds":900});
        // Ignored runtime evidence only. No private key or real account token.
        fs::write(
            evidence.join("local-mobile-ui-harness.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        println!("LOCAL_MOBILE_UI_HTTP_READY_NO_PROVIDER");
        let deadline = std::time::Instant::now() + Duration::from_secs(600);
        while !stop.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "local UI validation timed out; no provider action or retry"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        let outcome: serde_json::Value = serde_json::from_slice(&fs::read(&stop).unwrap()).unwrap();
        let review_id = outcome["actionId"]
            .as_str()
            .expect("one UI-created action ID");
        let review = store
            .mobile_codex_outbound_review(review_id)
            .unwrap()
            .unwrap();
        assert_eq!(review.workstream_id, bridge.id);
        assert_eq!(review.status, "FAILED");
        assert_eq!(
            review.approved_text.as_deref(),
            outcome["approvedText"].as_str()
        );
        assert!(review.attachments_json.to_ascii_lowercase().contains(&hash));
        let runs = store.provider_runs_for_workstream(&bridge.id).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "FAILED");
        assert_eq!(runs[0].terminal_code.as_deref(), Some("AUTH_REQUIRED"));
        assert!(runs[0].external_run_id.is_none());
        assert!(core.session.lock().unwrap().adapter.is_none());
        assert_eq!(core.chatgpt.status(), "AUTH_REQUIRED");
        runtime.block_on(handle.shutdown());
        keys_task.abort();
        println!("LOCAL_MOBILE_UI_HTTP_STORE_GATE_PASS_NO_PROVIDER");
    }
}

#[cfg(all(test,windows))]
mod role_api_tests {
 use super::*;use serde_json::{json,Value};use std::fs;
 #[test]
 fn role_http_scope_exact_hash_approval_and_disconnected_send_stop(){
  use crate::{RouterStore,Session};use router_core::store::role_bridge::RoleBindingInput;
  let dir=tempfile::tempdir().unwrap();fs::write(dir.path().join("index.html"),"fixture").unwrap();let store=Arc::new(RouterStore::open_at(dir.path().join("router.db")).unwrap());let p=store.create_project("roles".into(),None).unwrap();let w=store.create_workstream(&p.id,"roles".into()).unwrap();let other=store.create_workstream(&p.id,"other".into()).unwrap();
  let input=|id:&str|RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:id.into(),cwd:Some(dir.path().to_string_lossy().into_owned())};let b=store.bind_role_bridge(&w.id,store.role_bridge(&w.id).unwrap().binding_revision,input("thread-source"),input("thread-target")).unwrap();let src=b.decision.unwrap().endpoint;let obs=store.record_reply_observation(&w.id,&src.id,Some("fixture-native"),"source",None).unwrap().unwrap();let h=store.prepare_role_handoff(&w.id,"DECISION",&obs.id,"edited bytes").unwrap();
  let core=RouterCore{store:store.clone(),chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::default()};let signed=tests::ephemeral_access_fixture();let token=signed["token"].as_str().unwrap();let keys=json!({"keys":signed["keys"]});let runtime=tokio::runtime::Runtime::new().unwrap();
  let (keys_url,task)=runtime.block_on(async{let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();let server=Router::new().route("/certs",get(move||{let value=keys.clone();async move{Json(value)}}));let task=tokio::spawn(async move{axum::serve(listener,server).await.unwrap()});(format!("http://{address}/certs"),task)});
  let config=MobileHttpConfig{port:0,allowed_host:"router.fixture.invalid".into(),allowed_origin:"https://router.fixture.invalid".into(),access_issuer:"https://fixture.invalid".into(),access_audience:"fixture-audience".into(),access_jwks_url:keys_url,static_dir:dir.path().into()};let handle=runtime.block_on(start(core,config,crate::HostRuntime::default())).unwrap();let base=format!("http://{}/v1/mobile/workstreams",handle.address);let url=format!("{}/{}/role-bridge",base,w.id);let client=reqwest::blocking::Client::new();
  assert_eq!(client.get(&url).header("host","router.fixture.invalid").send().unwrap().status(),StatusCode::UNAUTHORIZED);
  let reply:Value=client.get(&url).header("host","router.fixture.invalid").header(ACCESS_JWT_HEADER,token).send().unwrap().json().unwrap();assert_eq!(reply["bindings"]["decision"]["endpoint"]["externalId"],"thread-source");assert_eq!(reply["bindings"]["execution"]["endpoint"]["externalId"],"thread-target");
  let post=|url:&str,body:Value|client.post(url).header("host","router.fixture.invalid").header("origin","https://router.fixture.invalid").header(ACCESS_JWT_HEADER,token).json(&body).send().unwrap();
  assert!(!post(&url,json!({"action":"SEND","handoffId":h.id})).status().is_success());
  assert!(!post(&format!("{}/{}/role-bridge",base,other.id),json!({"action":"APPROVE","handoffId":h.id,"expectedHash":h.payload_hash})).status().is_success());assert!(!post(&url,json!({"action":"APPROVE","handoffId":h.id,"expectedHash":"stale"})).status().is_success());assert!(post(&url,json!({"action":"APPROVE","handoffId":h.id,"expectedHash":h.payload_hash})).status().is_success());assert!(store.provider_runs_for_workstream(&w.id).unwrap().is_empty());
  assert!(!post(&url,json!({"action":"SEND","handoffId":h.id})).status().is_success());assert_eq!(store.role_handoff(&h.id).unwrap().status,"APPROVED");assert!(store.provider_runs_for_workstream(&w.id).unwrap().is_empty());
  let server_base=format!("http://{}/v1/mobile",handle.address);
  let created=post(&format!("{server_base}/bridges"),json!({"name":"new exact bridge"}));assert_eq!(created.status(),StatusCode::OK);let created:String=created.json().unwrap();assert!(!store.role_bridge(&created).unwrap().explicit_roles);assert!(store.provider_runs_for_workstream(&created).unwrap().is_empty());
  let renamed=post(&format!("{server_base}/bridges/{created}/rename"),json!({"name":"  renamed exact bridge  "}));assert_eq!(renamed.status(),StatusCode::OK);assert_eq!(store.workstream_name(&created).unwrap(),"renamed exact bridge");assert!(store.provider_runs_for_workstream(&created).unwrap().is_empty());assert!(!post(&format!("{server_base}/bridges/{created}/rename"),json!({"name":" "})).status().is_success());
  fs::write(dir.path().join("image.png"),b"\x89PNG\r\n\x1a\npublic media fixture").unwrap();let image_observation=store.record_reply_observation(&w.id,&src.id,Some("fixture-media"),"![plot](image.png)",None).unwrap().unwrap();let body=json!({"scope":{"kind":"ROLE","workstreamId":w.id,"role":"DECISION","observationId":image_observation.id},"source":"image.png"});
  let media_url=format!("{server_base}/media");assert_eq!(client.post(&media_url).header("host","router.fixture.invalid").header("origin","https://router.fixture.invalid").json(&body).send().unwrap().status(),StatusCode::UNAUTHORIZED);
  let image=post(&media_url,body);assert_eq!(image.status(),StatusCode::OK);let image:Value=image.json().unwrap();assert_eq!(image["mime"],"image/png");assert_eq!(image["filename"],"image.png");assert!(!post(&media_url,json!({"scope":{"kind":"ROLE","workstreamId":w.id,"role":"EXECUTION","observationId":image_observation.id},"source":"image.png"})).status().is_success());
  runtime.block_on(handle.shutdown());task.abort();
 }
}

#[cfg(all(test,windows))]
mod watch_http_tests {
 use super::*;use serde_json::{json,Value};
 #[test] fn watch_http_auth_cursor_original_content_and_pause(){
  use crate::{RouterStore,Session};use router_core::store::codex_watch::WatchSnapshot;
  let dir=tempfile::tempdir().unwrap();fs::write(dir.path().join("index.html"),"fixture").unwrap();let store=Arc::new(RouterStore::open_at(dir.path().join("router.db")).unwrap());
  let snap=|text:&str|WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some(text.into()),item_id:Some("item".into()),text:text.into()};
  store.enable_codex_watch("native-exact","Fixture",dir.path().to_str().unwrap(),&snap("baseline")).unwrap();store.record_codex_watch("native-exact",1,&snap("complete original result")).unwrap();
  let core=RouterCore{store:store.clone(),chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::default()};let signed=tests::ephemeral_access_fixture();let token=signed["token"].as_str().unwrap();let keys=json!({"keys":signed["keys"]});let runtime=tokio::runtime::Runtime::new().unwrap();
  let (keys_url,task)=runtime.block_on(async{let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();let server=Router::new().route("/certs",get(move||{let value=keys.clone();async move{Json(value)}}));let task=tokio::spawn(async move{axum::serve(listener,server).await.unwrap()});(format!("http://{address}/certs"),task)});
  let config=MobileHttpConfig{port:0,allowed_host:"router.fixture.invalid".into(),allowed_origin:"https://router.fixture.invalid".into(),access_issuer:"https://fixture.invalid".into(),access_audience:"fixture-audience".into(),access_jwks_url:keys_url,static_dir:dir.path().into()};let handle=runtime.block_on(start(core,config,crate::HostRuntime::default())).unwrap();let base=format!("http://{}/v1/mobile/codex-watches",handle.address);let client=reqwest::blocking::Client::new();
  assert!(!client.get(format!("{base}/events")).header("host","router.fixture.invalid").send().unwrap().status().is_success());
  let get=|suffix:&str|client.get(format!("{base}{suffix}")).header("host","router.fixture.invalid").header(ACCESS_JWT_HEADER,token).send().unwrap();
  let feed:Value=get("/events?after=0").json().unwrap();assert_eq!(feed["events"][0]["threadId"],"native-exact");let sequence=feed["nextCursor"].as_i64().unwrap();let full:Value=get(&format!("/events/{sequence}")).json().unwrap();assert_eq!(full["snapshot"]["text"],"complete original result");assert!(get("/events?after=-1").status().is_client_error());assert!(get("/events?waitSeconds=21").status().is_client_error());assert_eq!(get(&format!("/events?after={sequence}")).json::<Value>().unwrap()["events"],json!([]));
  let post=|origin:&str|client.post(&base).header("host","router.fixture.invalid").header("origin",origin).header(ACCESS_JWT_HEADER,token).json(&json!({"action":"PAUSE","threadId":"native-exact"})).send().unwrap();assert!(!post("https://wrong.invalid").status().is_success());assert!(post("https://router.fixture.invalid").status().is_success());assert!(!store.codex_watches().unwrap()[0].enabled);assert_eq!(store.codex_watch_event(sequence).unwrap().snapshot.text,"complete original result");
  let delivery=get("/delivery");assert_eq!(delivery.headers()["cache-control"],"no-store");let settings:Value=delivery.json().unwrap();assert_eq!(settings["recipient"],"");assert!(settings.get("password").is_none());
  let send=|origin:&str,input:Value|client.post(format!("{base}/delivery")).header("host","router.fixture.invalid").header("origin",origin).header(ACCESS_JWT_HEADER,token).json(&input).send().unwrap();
  assert!(!send("https://wrong.invalid",json!({"action":"CHANNEL","channel":"WEB","enabled":true})).status().is_success());
  assert!(send("https://router.fixture.invalid",json!({"action":"CHANNEL","channel":"WEB","enabled":true})).status().is_success());
  assert_eq!(send("https://router.fixture.invalid",json!({"action":"EMAIL","account":"sender@gmail.com","recipient":"agbrio.fixture@gmail.com","password":"fixture-secret"})).status(),StatusCode::FORBIDDEN);
  let removal=json!({"action":"REMOVE","kind":"EVENT","id":"1","removed":true});
  assert_eq!(client.post(&base).header("host","router.fixture.invalid").header("origin","https://router.fixture.invalid").json(&removal).send().unwrap().status(),StatusCode::UNAUTHORIZED);
  assert_eq!(client.post(&base).header("host","router.fixture.invalid").header("origin","https://wrong.invalid").header(ACCESS_JWT_HEADER,token).json(&removal).send().unwrap().status(),StatusCode::FORBIDDEN);
  let remove=|kind:&str,id:&str,removed:bool|client.post(&base).header("host","router.fixture.invalid").header("origin","https://router.fixture.invalid").header(ACCESS_JWT_HEADER,token).json(&json!({"action":"REMOVE","kind":kind,"id":id,"removed":removed})).send().unwrap();
  assert_eq!(remove("EVENT","1",true).status(),StatusCode::OK);let hidden:Value=get("/events?after=0").json().unwrap();assert!(hidden["events"].as_array().unwrap().is_empty());assert_eq!(hidden["hiddenSequences"],json!([1]));
  assert_eq!(remove("EVENT","1",false).status(),StatusCode::OK);assert_eq!(get("/events?after=0").json::<Value>().unwrap()["events"].as_array().unwrap().len(),1);
  assert_eq!(remove("WATCH","native-exact",true).status(),StatusCode::OK);assert!(store.codex_watches().unwrap().is_empty());assert_eq!(remove("WATCH","native-exact",false).status(),StatusCode::OK);assert!(!store.codex_watches().unwrap()[0].enabled);
  assert!(!remove("WATCH","wrong",true).status().is_success());
  runtime.block_on(handle.shutdown());task.abort();
 }
}

#[cfg(test)]
#[test]
#[ignore = "Explicit disposable PWA browser gate; no production auth/store/provider"]
fn disposable_pwa_http_browser_gate(){
 if std::env::var("AIWR_PWA_BROWSER_GATE").as_deref()!=Ok("1"){return;}
 let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();let evidence=root.join("runtime/web-availability");let directory=tempfile::tempdir().unwrap();
 let port=std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();let authority=format!("127.0.0.1:{port}");let origin=format!("http://{authority}");
 let core=RouterCore{chatgpt:Arc::default(),store:Arc::new(crate::RouterStore::open_at(directory.path().join("router.db")).unwrap()),session:Arc::default(),completed_chatgpt_responses:Arc::default()};
 let auth=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());let code=auth.issue().unwrap().code;let stop=evidence.join(format!("pwa-stop-{}",uuid::Uuid::new_v4()));
 let host=crate::HostRuntime::default();let runtime=tokio::runtime::Runtime::new().unwrap();let config=MobileHttpConfig{port,allowed_host:authority,allowed_origin:origin.clone(),access_issuer:"https://unused.invalid".into(),access_audience:"unused".into(),access_jwks_url:"https://unused.invalid/keys".into(),static_dir:root.join("dist")};
 let handle=runtime.block_on(start_with_web_auth(core,config,host.clone(),auth)).unwrap();
 std::fs::write(evidence.join("pwa-fixture-ready.json"),serde_json::to_vec(&serde_json::json!({"origin":origin,"code":code,"stop":stop,"disposable":true})).unwrap()).unwrap();
 for _ in 0..180{if stop.exists(){break;}std::thread::sleep(Duration::from_secs(1));}
 runtime.block_on(handle.shutdown());
}

#[derive(Deserialize)]
struct CreateBridgeInput{name:String}
async fn create_bridge(State(state):State<MobileHttpState>,headers:HeaderMap,Json(input):Json<CreateBridgeInput>)->ApiResult<String>{
 authenticated(&headers,&state,true).await?;let core=state.core.clone();run_core_blocking("Create Bridge",move||crate::role_bridge::create(&core,&input.name)).await.map(Json)
}
async fn rename_bridge(State(state):State<MobileHttpState>,headers:HeaderMap,Path(workstream_id):Path<String>,Json(input):Json<CreateBridgeInput>)->ApiResult<()>{
 authenticated(&headers,&state,true).await?;let core=state.core.clone();run_core_blocking("Rename Bridge",move||core.store.rename_workstream(&workstream_id,&input.name).map(|_|())).await.map(Json)
}

async fn read_message_media(State(state):State<MobileHttpState>,headers:HeaderMap,Json(input):Json<crate::message_media::MediaInput>)->ApiResult<crate::message_media::MediaFile>{authenticated(&headers,&state,true).await?;let core=state.core.clone();run_core_blocking("Message media",move||crate::message_media::read(&core,input)).await.map(Json)}
