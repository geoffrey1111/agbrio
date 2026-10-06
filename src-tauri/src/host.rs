//! Resident Router Host services shared by desktop IPC and authenticated HTTP.
//! This module has no Tauri handle/window/protocol dependency. Desktop lifecycle
//! observations are presentation DTOs; service clones share the same resources.
#[cfg(debug_assertions)]
use crate::HostLifecycleObservation;
use crate::{
    mobile_http, ConversationHistory, ConversationHistoryDiagnostics, ConversationHistoryMessage,
    HostEnvironmentStatus, Provider, RouterCore,
};
use std::sync::{Arc, Mutex};

/// Resident service resources. A hidden dashboard has no bearing on Core,
/// persistence, adapters, or the loopback listener.
#[derive(Clone)]
pub(crate) struct HostRuntime {
    runtime: Arc<tokio::runtime::Runtime>,
    instance:Arc<String>,
    pub(crate) web:Arc<crate::web_availability::Watchdog>,
    shared_stop:Arc<std::sync::atomic::AtomicBool>,
    shared_worker:Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    setup_worker:Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    /// Serializes only exact Host browser operations, preventing concurrent
    /// reads/opens from creating competing Router-owned tabs or roots.
    pub(super) exact_host_operation: Arc<Mutex<()>>,
    mobile: Arc<Mutex<Option<mobile_http::MobileHttpHandle>>>,
    mobile_config:Arc<Mutex<Option<mobile_http::MobileHttpConfig>>>,
    #[cfg(test)] pub(crate) test_web_auth:Arc<Mutex<Option<Arc<crate::web_auth::WebAuth>>>>,
    pub(crate) mobile_probe:Arc<Mutex<Option<crate::mobile_connection::ProbeGrant>>>,
    mobile_detail: Arc<Mutex<String>>,
    #[cfg(debug_assertions)]
    pub(super) lifecycle: Arc<Mutex<HostLifecycleObservation>>,
    #[cfg(debug_assertions)]
    pub(super) tray_quit_enabled: Arc<Mutex<Option<bool>>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RouterStore, Session};
    use std::collections::HashMap;

    #[test]
    fn failed_local_listener_recovers_using_the_same_host_store_and_identity() {
        let directory=tempfile::tempdir().unwrap();std::fs::write(directory.path().join("index.html"),"recovery-shell").unwrap();
        let core=RouterCore{chatgpt:Arc::default(),store:Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap()),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::default()};
        let host=HostRuntime::default();let mut config=mobile_http::MobileHttpConfig::controlled_host_acceptance(directory.path().into());config.port=0;
        host.ensure_mobile(core.clone(),config.clone());let first=host.mobile.lock().unwrap().as_ref().unwrap().address;
        let client=reqwest::blocking::Client::new();let verify=|address|client.get(format!("http://{address}/v1/mobile/auth/session")).header("host","router.local.test").send().unwrap();
        let first_reply=verify(first);assert_eq!(first_reply.headers()["x-aiwr-host-instance"],host.instance_id());
        host.ensure_mobile(core.clone(),config.clone());assert_eq!(host.mobile.lock().unwrap().as_ref().unwrap().address,first);
        host.mobile.lock().unwrap().as_ref().unwrap().task.abort();std::thread::sleep(std::time::Duration::from_millis(20));
        host.ensure_mobile(core.clone(),config);let next=host.mobile.lock().unwrap().as_ref().unwrap().address;assert_eq!(verify(next).headers()["x-aiwr-host-instance"],host.instance_id());assert!(core.session.lock().unwrap().adapter.is_none());
        host.shutdown(&core);assert!(client.get(format!("http://{next}/mobile")).send().is_err());
    }

    #[test]
    fn mobile_host_starts_and_stops_without_a_tauri_window_or_handle() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("index.html"), "headless-host-fixture").unwrap();
        let core = RouterCore {
            chatgpt: Arc::default(),
            store: Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap()),
            session: Arc::new(Mutex::new(Session::default())),
            completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        };
        let host = HostRuntime::default();
        let clone = host.clone();
        assert!(Arc::ptr_eq(&host.mobile, &clone.mobile));
        assert!(Arc::ptr_eq(
            &host.exact_host_operation,
            &clone.exact_host_operation
        ));
        let mut config =
            mobile_http::MobileHttpConfig::controlled_host_acceptance(directory.path().into());
        config.port = 0;
        let handle = host
            .runtime
            .block_on(mobile_http::start(core.clone(), config, clone))
            .unwrap();
        assert!(handle.address.ip().is_loopback());
        let origin = format!("http://{}", handle.address);
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap();
        let page = client.get(format!("{origin}/mobile")).send().unwrap();
        assert_eq!(page.status(), reqwest::StatusCode::OK);
        assert_eq!(page.text().unwrap(), "headless-host-fixture");
        let protected = client
            .get(format!("{origin}/v1/mobile/workstreams"))
            .header("host", "router.local.test")
            .send()
            .unwrap();
        assert_eq!(protected.status(), reqwest::StatusCode::UNAUTHORIZED);
        // No JWT bypass, second Store or provider process is introduced by startup.
        assert!(core.session.lock().unwrap().adapter.is_none());
        host.runtime.block_on(handle.shutdown());
        assert!(client.get(format!("{origin}/mobile")).send().is_err());
    }
}

impl Default for HostRuntime {
    fn default() -> Self {
        Self {
            runtime: Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Router Host Tokio runtime"),
            ),
            instance:Arc::new(uuid::Uuid::new_v4().to_string()),
            web:Default::default(),
            shared_stop:Default::default(),shared_worker:Default::default(),setup_worker:Default::default(),
            exact_host_operation: Default::default(),
            mobile: Default::default(),mobile_config:Default::default(),mobile_probe:Default::default(),
            #[cfg(test)] test_web_auth:Default::default(),
            mobile_detail: Default::default(),
            #[cfg(debug_assertions)]
            lifecycle: Default::default(),
            #[cfg(debug_assertions)]
            tray_quit_enabled: Default::default(),
        }
    }
}

impl HostRuntime {
    pub(crate) fn start_shared_reconnection(&self,core:RouterCore,sink:Arc<dyn router_core::events::EventSink>){
        let Ok(mut worker)=self.shared_worker.lock()else{return;};if worker.is_some(){return;}
        let stop=self.shared_stop.clone();*worker=Some(std::thread::spawn(move||{
            let mut delay=5;while !stop.load(std::sync::atomic::Ordering::Acquire){
                for _ in 0..delay*10{if stop.load(std::sync::atomic::Ordering::Acquire){return;}std::thread::sleep(std::time::Duration::from_millis(100));}
                if !crate::shared_codex::enabled(){delay=5;continue;}
                let disconnected=core.session.lock().is_ok_and(|s|!s.connecting&&!s.codex_adapter_borrowed&&s.adapter.as_ref().is_none_or(|a|a.is_closed()));
                if disconnected{crate::host_application::connect_codex_for_resident_host(sink.clone(),&core);delay=(delay*2).min(60);}else{delay=5;}
            }
        }));
    }
    fn start_setup_requests(&self,core:RouterCore){
        let Ok(mut worker)=self.setup_worker.lock()else{return;};if worker.is_some(){return;}
        let stop=self.shared_stop.clone();let host=self.clone();*worker=Some(std::thread::spawn(move||{
            while !stop.load(std::sync::atomic::Ordering::Acquire){crate::mobile_connection::process_setup_request(&core,&host);for _ in 0..20{if stop.load(std::sync::atomic::Ordering::Acquire){return;}std::thread::sleep(std::time::Duration::from_millis(100));}}
        }));
    }
    /// Explicit human intent only. AUTH_REQUIRED opens native human-only
    /// Chromium; no automation connection or latch clearance occurs.
    pub(crate) fn open_chatgpt_browser_setup(&self, core: &RouterCore) -> Result<(), String> {
        let result = core.chatgpt.open_for_authentication()?;
        if result["status"] == "OPEN" {
            if result["focus"] == "ATTENTION" {
                return Err("SECURITY_BROWSER_FOCUS_BLOCKED".into());
            }
            Ok(())
        } else {
            Err(result["code"].as_str().or(result["status"].as_str()).unwrap_or("UNAVAILABLE").into())
        }
    }

    /// Reads one exact, terminal ChatGPT reply through the persistent
    /// Playwright carrier. The direct provider contract intentionally exposes
    /// no complete transcript, so this returns an explicit terminal-only
    /// projection rather than inferring a browser-session history.
    pub(crate) fn read_active_chatgpt_history(
        &self,
        core: &RouterCore,
        workstream_id: &str,
    ) -> Result<ConversationHistory, String> {
        let endpoint = core
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
            .ok_or("No ACTIVE ChatGPT Endpoint")?;
        let latest = core.chatgpt.observe_exact(&endpoint.external_id)?;
        let messages = std::iter::once(latest)
            .map(|reply| ConversationHistoryMessage {
                id: reply.message_id,
                role: "assistant".into(),
                text: reply.text,
                blocks: Vec::new(),
                code_blocks: Vec::new(),
                resources: Vec::new(),
            })
            .collect::<Vec<_>>();
        Ok(ConversationHistory {
            id: endpoint.external_id,
            title: endpoint.label,
            branch_scope: "EXACT_CONVERSATION_LATEST_TERMINAL".into(),
            completeness: "TERMINAL_ONLY".into(),
            diagnostics: ConversationHistoryDiagnostics {
                beginning_reached: false,
                end_reached: true,
                beginning_steps: 0,
                end_steps: 0,
                initial_dom_message_count: messages.len() as u32,
                scroll_target: "playwright-exact-conversation".into(),
                viewport_height: 0,
                scrollable_height: 0,
                end_last_scroll_top: 0,
                end_last_scrollable_height: 0,
                end_stalled_steps: 0,
                scroll_restored: true,
            },
            messages,
        })
    }

    pub(crate) fn environment_status(&self, core: &RouterCore) -> HostEnvironmentStatus {
        HostEnvironmentStatus {
            host: "ONLINE".into(),
            mobile: self.mobile_status(),
            browser_runtime: "OFFICIAL_NON_BRANDED_CHROMIUM".into(),
            chatgpt_browser_mode: core.chatgpt.status().into(),
        }
    }

    pub(crate) fn start_mobile(
        &self,
        core: RouterCore,
        static_dir: std::path::PathBuf,
        _controlled_acceptance: bool,
    ) {
        #[cfg(debug_assertions)]
        let config = if _controlled_acceptance {
            Ok(mobile_http::MobileHttpConfig::controlled_host_acceptance(
                static_dir,
            ))
        } else {
            mobile_http::MobileHttpConfig::from_env(static_dir)
        };
        #[cfg(not(debug_assertions))]
        let config = mobile_http::MobileHttpConfig::from_env(static_dir);
        if let Ok(config)=config {self.ensure_mobile(core.clone(),config.clone());self.web.start(self.clone(),core.clone(),config);self.start_setup_requests(core);}else if let Err(error)=config {if let Ok(mut current)=self.mobile_detail.lock(){*current=format!("UNAVAILABLE: {error}");}}
    }
    pub(crate) fn instance_id(&self)->&str {self.instance.as_str()}
    pub(crate) fn ensure_mobile(&self,core:RouterCore,config:mobile_http::MobileHttpConfig){
        let Ok(mut mobile)=self.mobile.lock() else{return;};
        let same=self.mobile_config.lock().is_ok_and(|c|c.as_ref()==Some(&config));
        if same&&mobile.as_ref().is_some_and(|handle|!handle.is_finished()){return;}
        if let Some(handle)=mobile.take(){self.runtime.block_on(handle.shutdown());}
        if let Ok(mut c)=self.mobile_config.lock(){*c=Some(config.clone());}
        #[cfg(test)] let custom=self.test_web_auth.lock().ok().and_then(|a|a.clone());
        #[cfg(test)] let result=if let Some(auth)=custom{self.runtime.block_on(mobile_http::start_with_web_auth(core,config,self.clone(),auth))}else{self.runtime.block_on(mobile_http::start(core,config,self.clone()))};
        #[cfg(not(test))] let result=self.runtime.block_on(mobile_http::start(core,config,self.clone()));
        match result {
            Ok(handle)=>{if let Ok(mut current)=self.mobile_detail.lock(){*current=format!("AVAILABLE on {}",handle.address);}*mobile=Some(handle);}
            Err(error)=>{if let Ok(mut current)=self.mobile_detail.lock(){*current=format!("UNAVAILABLE: {error}");}}
        }
    }

    pub(crate) fn restart_mobile_listener(&self,core:RouterCore,config:mobile_http::MobileHttpConfig){
        if let Ok(mut mobile)=self.mobile.lock(){if let Some(handle)=mobile.take(){self.runtime.block_on(handle.shutdown());}}
        self.ensure_mobile(core,config);
    }
    pub(crate) fn mobile_configuration(&self)->Option<mobile_http::MobileHttpConfig>{self.mobile_config.lock().ok()?.clone()}
    pub(crate) fn mobile_address(&self)->Option<std::net::SocketAddr>{self.mobile.lock().ok()?.as_ref().map(|h|h.address)}
    pub(crate) fn verify_mobile_probe(&self,host:&str,nonce:&str)->Result<serde_json::Value,String>{let lock=self.mobile_probe.lock().map_err(|_|"MOBILE_PROBE_INVALID")?;let p=lock.as_ref().ok_or("MOBILE_PROBE_INVALID")?;if p.host!=host||p.nonce!=nonce||p.expires<std::time::Instant::now(){return Err("MOBILE_PROBE_INVALID".into());}Ok(serde_json::json!({"nonce":nonce,"instance":self.instance_id()}))}
    pub(crate) fn mobile_status(&self) -> String {
        self.mobile_detail
            .lock()
            .map(|detail| detail.clone())
            .unwrap_or_else(|_| "UNKNOWN".into())
    }

    pub(crate) fn shutdown(&self, core: &RouterCore) {
        self.shared_stop.store(true,std::sync::atomic::Ordering::Release);
        if let Ok(mut worker)=self.shared_worker.lock(){if let Some(worker)=worker.take(){let _=worker.join();}}
        if let Ok(mut worker)=self.setup_worker.lock(){if let Some(worker)=worker.take(){let _=worker.join();}}
        self.web.stop();
        if let Ok(mut mobile) = self.mobile.lock() {
            if let Some(handle) = mobile.take() {
                self.runtime.block_on(handle.shutdown());
            }
        }
        core.shutdown_owned_adapter();
    }
}
