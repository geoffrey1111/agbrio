mod codex_quota;
mod desktop_update;
use desktop_update::{desktop_update_check,desktop_update_status,desktop_update_download,desktop_update_install,desktop_update_open_release};
mod message_media;
mod artifact;
mod chatgpt;
mod chatgpt_direct_writer;
mod codex;
mod host;
mod host_application;
mod mobile_http;
mod mobile_connection;
mod hosted_relay;
mod web_auth;
mod web_availability;
mod shared_codex;
use host::HostRuntime;
use host_application::*;
#[cfg(test)]
mod normal_browser;
#[cfg(test)]
mod normal_chrome_extension;
mod persistence;
#[cfg(test)]
mod playwright_chatgpt;
mod presentation;
mod push;
mod runtime_reconciler;
mod role_bridge;
mod assistant_mcp;
mod assistant_events;
mod assistant_help;
mod assistant_operations;

#[tauri::command]
fn assistant_settings(state:tauri::State<'_,RouterState>)->Result<serde_json::Value,String>{
    assistant_settings_view(state.inner())
}
pub(crate) fn assistant_settings_view(core:&RouterCore)->Result<serde_json::Value,String>{
    let bridges=core.store.snapshot()?.workstreams.into_iter().filter(|w|w.trashed_at.is_none()&&w.archived_at.is_none()).filter_map(|w|{
        core.store.role_bridge(&w.id).ok().filter(|b|b.decision.is_some()&&b.execution.is_some()).map(|bindings|serde_json::json!({"id":w.id,"name":w.name,"bindings":bindings}))
    }).collect::<Vec<_>>();
    Ok(serde_json::json!({"grants":core.store.assistant_grants()?,"bridges":bridges,"mcpUrl":mobile_http::configured_mobile_allowed_origin().map(|o|format!("{}/mcp",o.trim_end_matches('/')))}))
}
#[tauri::command]
fn assistant_create_grant(input:router_core::store::assistant::GrantInput,state:tauri::State<'_,RouterState>)->Result<router_core::store::assistant::AssistantGrant,String>{state.store.create_assistant_grant(input)}
#[tauri::command]
fn assistant_connect_instance(input:router_core::store::assistant::AssistantConnectionInput,state:tauri::State<'_,RouterState>)->Result<router_core::store::assistant::AssistantGrant,String>{state.store.connect_assistant_instance(input)}
#[tauri::command]
fn assistant_create_instance_grant(input:router_core::store::assistant::InstanceGrantInput,state:tauri::State<'_,RouterState>)->Result<router_core::store::assistant::AssistantGrant,String>{state.store.create_assistant_instance_grant(input)}
#[tauri::command]
fn assistant_revoke_grant(grant_id:String,state:tauri::State<'_,RouterState>)->Result<(),String>{state.store.revoke_assistant_grant(&grant_id)}
mod codex_watch;
mod watch_notifications;
mod watch_chat;
#[cfg(windows)]
mod notification_activation;

#[tauri::command]
async fn codex_watch_chat(thread_id:String,state:State<'_,RouterState>)->Result<watch_chat::ChatState,String>{let core=state.inner().clone();tauri::async_runtime::spawn_blocking(move||watch_chat::state(&core,&thread_id)).await.map_err(|_|"WATCH_CHAT_FAILED".to_string())?}
#[tauri::command]
async fn codex_watch_chat_command(input:watch_chat::ChatCommand,state:State<'_,RouterState>)->Result<serde_json::Value,String>{let core=state.inner().clone();tauri::async_runtime::spawn_blocking(move||watch_chat::command(&core,input)).await.map_err(|_|"WATCH_CHAT_FAILED".to_string())?}

#[tauri::command]
async fn shared_codex_status()->Result<shared_codex::Status,String>{tauri::async_runtime::spawn_blocking(||Ok(shared_codex::status())).await.map_err(|_|"SHARED_STATUS_UNAVAILABLE".to_string())?}
#[tauri::command]
async fn shared_codex_setup(app:AppHandle,state:State<'_,RouterState>)->Result<shared_codex::Status,String>{
    let core=state.inner().clone();let sink=Arc::new(TauriEventSink(app));
    tauri::async_runtime::spawn_blocking(move||{
        let mut session=core.session.lock().map_err(|_|"SHARED_ROUTER_BUSY")?;
        if session.connecting||session.codex_adapter_borrowed||session.adapter.as_ref().is_some_and(|a|a.has_owned_turns()) {return Err("SHARED_ROUTER_BUSY".into());}
        let status=shared_codex::setup()?;
        if status.enabled && session.adapter.as_ref().is_some_and(|a|!a.is_shared()) {
            if let Some(mut adapter)=session.adapter.take(){adapter.shutdown();}
            session.ready_threads.clear();session.codex_observer_epoch=session.codex_observer_epoch.wrapping_add(1);
        }
        drop(session);if status.enabled {connect_codex_for_resident_host(sink,&core);}Ok(status)
    }).await.map_err(|_|"SHARED_SETUP_UNAVAILABLE".to_string())?
}
#[tauri::command]
async fn shared_codex_launch(app:AppHandle,state:State<'_,RouterState>)->Result<shared_codex::Status,String>{
    shared_codex_change(app,state,true).await
}
#[tauri::command]
async fn shared_codex_disable(app:AppHandle,state:State<'_,RouterState>)->Result<shared_codex::Status,String>{
    shared_codex_change(app,state,false).await
}
async fn shared_codex_change(app:AppHandle,state:State<'_,RouterState>,launch:bool)->Result<shared_codex::Status,String>{
    let core=state.inner().clone();let sink=Arc::new(TauriEventSink(app));
    tauri::async_runtime::spawn_blocking(move||{
        let mut session=core.session.lock().map_err(|_|"SHARED_ROUTER_BUSY")?;
        if session.connecting||session.codex_adapter_borrowed||session.adapter.as_ref().is_some_and(|a|a.has_owned_turns()){return Err("SHARED_ROUTER_BUSY".into());}
        let status=if launch{shared_codex::launch()?}else{shared_codex::disable()?};
        if let Some(mut adapter)=session.adapter.take(){adapter.shutdown();}
        session.ready_threads.clear();session.codex_observer_epoch=session.codex_observer_epoch.wrapping_add(1);
        drop(session);connect_codex_for_resident_host(sink,&core);Ok(status)
    }).await.map_err(|_|"SHARED_SETUP_UNAVAILABLE".to_string())?
}
#[tauri::command]
fn web_availability_status(host:State<'_,HostRuntime>)->web_availability::Status{host.web.status()}
#[tauri::command]
fn web_availability_retry(host:State<'_,HostRuntime>){host.web.retry();}
#[tauri::command]
fn web_availability_setup(app:tauri::AppHandle)->Result<(),String>{web_availability::request_setup(app.path().resource_dir().map_err(|_|"WEB_RECOVERY_SETUP_UNAVAILABLE")?)}
#[tauri::command]
fn mobile_connection_view(host:State<'_,HostRuntime>)->Result<mobile_connection::ConnectionView,String>{mobile_connection::view(host.inner())}
#[tauri::command]
fn hosted_connection_status(host:State<'_,HostRuntime>)->hosted_relay::Status{hosted_relay::status(host.inner())}
#[tauri::command]
async fn hosted_connection_redeem(input:hosted_relay::RedeemInput,core:State<'_,RouterCore>,host:State<'_,HostRuntime>)->Result<hosted_relay::Status,String>{let core=core.inner().clone();let host=host.inner().clone();tauri::async_runtime::spawn_blocking(move||hosted_relay::redeem(&core,&host,input)).await.map_err(|_|"HOSTED_ACTIVATION_FAILED".to_string())?}
#[tauri::command]
async fn mobile_connection_configure(input:mobile_connection::ConnectionInput,core:State<'_,RouterCore>,host:State<'_,HostRuntime>)->Result<mobile_connection::ConnectionView,String>{let core=core.inner().clone();let host=host.inner().clone();tauri::async_runtime::spawn_blocking(move||mobile_connection::configure(&core,&host,input)).await.map_err(|_|"MOBILE_CONFIG_FAILED".to_string())?}
#[tauri::command]
fn web_pairing_code(replace_device_id:Option<String>)->Result<web_auth::PairingCode,String>{web_auth::global()?.issue_replacing(replace_device_id)}
#[tauri::command]
fn web_paired_devices()->Result<Vec<web_auth::Device>,String>{web_auth::global()?.devices()}
#[tauri::command]
fn web_revoke_device(device_id:String)->Result<(),String>{web_auth::global()?.revoke(Some(&device_id),None)}

#[tauri::command]
fn codex_delivery_settings(state:State<'_,RouterState>)->Result<watch_notifications::SettingsView,String>{watch_notifications::view(&state.store)}
#[tauri::command]
async fn codex_delivery_command(input:watch_notifications::DeliveryCommand,state:State<'_,RouterState>)->Result<String,String>{let store=state.store.clone();tauri::async_runtime::spawn_blocking(move||watch_notifications::command(&store,input)).await.map_err(|_|"DELIVERY_COMMAND_FAILED".to_string())?}
#[tauri::command]
fn codex_notification_navigation(state:State<'_,watch_notifications::NotificationNavigation>)->Option<i64>{state.0.lock().ok()?.take()}

#[tauri::command]
fn codex_watch_candidates(state:State<'_,RouterState>)->Result<ExistingCodexThreadCatalog,String>{codex_watch::candidates(state.inner())}
#[tauri::command]
fn codex_watch_list(state:State<'_,RouterState>)->Result<Vec<router_core::store::codex_watch::CodexWatch>,String>{state.store.codex_watches()}
#[tauri::command]
fn codex_watch_enable(thread_id:String,state:State<'_,RouterState>)->Result<(),String>{codex_watch::enable(state.inner(),&thread_id)}
#[tauri::command]
fn codex_watch_pause(thread_id:String,state:State<'_,RouterState>)->Result<(),String>{state.store.pause_codex_watch(&thread_id)}
#[tauri::command]
fn codex_watch_remove(kind:String,id:String,removed:bool,state:State<'_,RouterState>)->Result<(),String>{state.store.set_watch_item_removed(&kind,&id,removed)}
#[tauri::command]
fn codex_watch_mark_seen(sequence:i64,state:State<'_,RouterState>)->Result<(),String>{state.store.mark_codex_watch_event_seen(sequence)}
#[tauri::command]
fn codex_watch_mark_read(sequence:i64,state:State<'_,RouterState>)->Result<(),String>{state.store.acknowledge_codex_watch_event(sequence)}
#[tauri::command]
fn codex_watch_feed(after:i64,state:State<'_,RouterState>)->Result<router_core::store::codex_watch::WatchFeed,String>{state.store.codex_watch_feed(after)}
#[tauri::command]
fn codex_watch_event(sequence:i64,state:State<'_,RouterState>)->Result<router_core::store::codex_watch::WatchEvent,String>{state.store.codex_watch_event(sequence)}
#[tauri::command]
fn codex_notifications_web_url()->Option<String>{crate::mobile_http::configured_mobile_allowed_origin().and_then(|s|{let mut u=url::Url::parse(&s).ok()?;if u.scheme()!="https"||u.host_str().is_none()||!u.username().is_empty()||u.password().is_some(){return None;}u.set_path("/mobile/notifications");u.set_query(None);u.set_fragment(None);Some(u.to_string())})}

use artifact::detector::{detect, AttachmentCandidate};
use chatgpt::direct::CompletedChatGptResponse;
use chatgpt::model::{
    ConversationHistory, ConversationHistoryDiagnostics, ConversationHistoryMessage,
};
use codex::adapter::CodexAdapter;
use persistence::{
    DashboardProjection, Endpoint, ExternalProjectLink, Project, Provider, RouterStore,
    WorkspaceSnapshot, Workstream,
};
use presentation::TauriEventSink;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State, WebviewWindow, WindowEvent};

#[tauri::command]
fn read_message_media(input:message_media::MediaInput,state:State<'_,RouterState>)->Result<message_media::MediaFile,String>{message_media::read(state.inner(),input)}
#[tauri::command]
async fn bridge_directory_activity(workstream_ids:Vec<String>,state:State<'_,RouterState>)->Result<Vec<role_bridge::DirectoryActivity>,String>{
 let core=state.inner().clone();tauri::async_runtime::spawn_blocking(move||role_bridge::directory_activity(&core,&workstream_ids)).await.map_err(|_|"BRIDGE_ACTIVITY_UNAVAILABLE".to_string())?
}
#[tauri::command]
fn role_bridge_state(workstream_id:String,state:State<'_,RouterState>)->Result<role_bridge::BridgeState,String>{role_bridge::state(state.inner(),&workstream_id)}
#[tauri::command]
fn create_bridge_workstream(name:String,state:State<'_,RouterState>)->Result<String,String>{role_bridge::create(state.inner(),&name)}
#[tauri::command]
fn rename_bridge_workstream(workstream_id:String,name:String,state:State<'_,RouterState>)->Result<(),String>{state.store.rename_workstream(&workstream_id,&name).map(|_|())}
#[tauri::command]
async fn sync_role_bridge(workstream_id:String,state:State<'_,RouterState>)->Result<role_bridge::BridgeState,String>{let core=state.inner().clone();tauri::async_runtime::spawn_blocking(move||role_bridge::sync_cached(&core,&workstream_id)).await.map_err(|_|"BRIDGE_SYNC_UNAVAILABLE".to_string())?}
#[tauri::command]
fn role_bridge_threads(state:State<'_,RouterState>)->Result<ExistingCodexThreadCatalog,String>{role_bridge::catalog(state.inner())}
#[tauri::command]
fn bind_role_bridge(workstream_id:String,revision:i64,decision:router_core::store::role_bridge::RoleBindingInput,execution:router_core::store::role_bridge::RoleBindingInput,state:State<'_,RouterState>)->Result<router_core::store::role_bridge::RoleBridge,String>{role_bridge::bind(state.inner(),&workstream_id,revision,decision,execution)}
#[tauri::command]
fn read_role_bridge(workstream_id:String,role:String,state:State<'_,RouterState>)->Result<role_bridge::BridgeState,String>{role_bridge::read(state.inner(),&workstream_id,&role)}
#[tauri::command]
fn prepare_role_handoff(workstream_id:String,role:String,observation_id:String,text:String,attachment_ids:Option<Vec<String>>,state:State<'_,RouterState>)->Result<persistence::HandoffHistoryItem,String>{role_bridge::prepare(state.inner(),&workstream_id,&role,&observation_id,&text,&attachment_ids.unwrap_or_default())}
#[tauri::command]
fn approve_role_handoff(handoff_id:String,expected_hash:String,state:State<'_,RouterState>)->Result<persistence::HandoffHistoryItem,String>{role_bridge::approve(state.inner(),&handoff_id,&expected_hash)}
#[tauri::command]
fn role_bridge_attachments(workstream_id:String,role:String,observation_id:String,state:State<'_,RouterState>)->Result<Vec<role_bridge::AttachmentOption>,String>{role_bridge::attachments(state.inner(),&workstream_id,&role,&observation_id)}
#[tauri::command]
fn role_bridge_blocks(workstream_id:String,role:String,observation_id:String,state:State<'_,RouterState>)->Result<Vec<chatgpt::model::RelayTextBlock>,String>{role_bridge::blocks(state.inner(),&workstream_id,&role,&observation_id)}
#[tauri::command]
fn edit_role_handoff(handoff_id:String,expected_hash:String,text:String,state:State<'_,RouterState>)->Result<persistence::HandoffHistoryItem,String>{state.store.edit_role_handoff(&handoff_id,&expected_hash,&text)}
#[tauri::command]
fn send_role_handoff(handoff_id:String,state:State<'_,RouterState>)->Result<role_bridge::BridgeState,String>{role_bridge::send(state.inner(),&handoff_id)}

#[cfg(debug_assertions)]
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostLifecycleObservation {
    close_requested: bool,
    target_window_label: Option<String>,
    hide_result: Option<String>,
    visible_immediately_after_hide: Option<bool>,
    visible_after_short_delay: Option<bool>,
    direct_close_request_result: Option<String>,
}

impl HostRuntime {
    #[cfg(debug_assertions)]
    fn reset_lifecycle(&self) {
        if let Ok(mut lifecycle) = self.lifecycle.lock() {
            *lifecycle = HostLifecycleObservation::default();
        }
    }

    #[cfg(debug_assertions)]
    fn record_close(
        &self,
        label: String,
        hide: Result<(), tauri::Error>,
        visible: Result<bool, tauri::Error>,
    ) {
        if let Ok(mut lifecycle) = self.lifecycle.lock() {
            lifecycle.close_requested = true;
            lifecycle.target_window_label = Some(label);
            lifecycle.hide_result = Some(match hide {
                Ok(()) => "OK".into(),
                Err(error) => format!("ERR: {error}"),
            });
            lifecycle.visible_immediately_after_hide = visible.ok();
        }
    }

    #[cfg(debug_assertions)]
    fn record_delayed_visibility(&self, visible: Result<bool, tauri::Error>) {
        if let Ok(mut lifecycle) = self.lifecycle.lock() {
            lifecycle.visible_after_short_delay = visible.ok();
        }
    }

    #[cfg(debug_assertions)]
    fn record_direct_close_request(&self, result: Result<(), tauri::Error>) {
        if let Ok(mut lifecycle) = self.lifecycle.lock() {
            lifecycle.direct_close_request_result = Some(match result {
                Ok(()) => "OK".into(),
                Err(error) => format!("ERR: {error}"),
            });
        }
    }

    #[cfg(debug_assertions)]
    fn lifecycle_observation(&self) -> HostLifecycleObservation {
        self.lifecycle
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default()
    }

    #[cfg(debug_assertions)]
    fn record_tray_quit_enabled(&self, enabled: Result<bool, tauri::Error>) {
        if let Ok(mut value) = self.tray_quit_enabled.lock() {
            *value = enabled.ok();
        }
    }

    #[cfg(debug_assertions)]
    fn tray_quit_enabled(&self) -> Option<bool> {
        self.tray_quit_enabled.lock().ok().and_then(|value| *value)
    }
}

#[cfg(debug_assertions)]
#[tauri::command]
fn v0_008_smoke_observation(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<V0_008SmokeObservation, String> {
    host_application::service_v0_008_smoke_observation(workstream_id, state.inner())
}

#[tauri::command]
fn workspace_snapshot(state: State<'_, RouterState>) -> Result<WorkspaceSnapshot, String> {
    state.store.snapshot()
}

/// Returns an exact Workstream projection without changing any persisted
/// selection. This powers independently contextual native Router windows.
#[tauri::command]
fn workstream_snapshot(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<WorkspaceSnapshot, String> {
    state.store.snapshot_for_workstream(&workstream_id)
}

/// Runtime-only native window evidence. The calling WebView is supplied by
/// Tauri, so a Board can never inherit a monitor identity from another view.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeWindowMonitorEvidence {
    window_label: String,
    current_monitor: Option<NativeMonitor>,
    available_monitors: Vec<NativeMonitor>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeMonitor {
    name: Option<String>,
    width: u32,
    height: u32,
    scale_factor: f64,
}

fn native_monitor(monitor: tauri::Monitor) -> NativeMonitor {
    NativeMonitor {
        name: monitor.name().cloned(),
        width: monitor.size().width,
        height: monitor.size().height,
        scale_factor: monitor.scale_factor(),
    }
}

#[tauri::command]
fn native_window_monitor_evidence(
    window: WebviewWindow,
) -> Result<NativeWindowMonitorEvidence, String> {
    let current_monitor = window
        .current_monitor()
        .map_err(|error| format!("Could not read the current monitor: {error}"))?
        .map(native_monitor);
    let available_monitors = window
        .available_monitors()
        .map_err(|error| format!("Could not list available monitors: {error}"))?
        .into_iter()
        .map(native_monitor)
        .collect();
    Ok(NativeWindowMonitorEvidence {
        window_label: window.label().to_string(),
        current_monitor,
        available_monitors,
    })
}

#[tauri::command]
fn dashboard_projection(state: State<'_, RouterState>) -> Result<DashboardProjection, String> {
    state.store.dashboard_projection()
}

/// Read-only, sanitized state for the local V0-006 WebDriver harness. This is
/// deliberately unavailable from release builds and is never rendered by the
/// product UI.
#[cfg(debug_assertions)]
#[tauri::command]
fn v0_006_smoke_observation(
    state: State<'_, RouterState>,
) -> Result<V0_006SmokeObservation, String> {
    host_application::service_v0_006_smoke_observation(state.inner())
}

#[tauri::command]
fn mark_provider_run_reviewed(run_id: String, state: State<'_, RouterState>) -> Result<(), String> {
    state.store.mark_provider_run_reviewed(&run_id)
}
#[tauri::command]
fn acknowledge_handoff_attention(
    handoff_id: String,
    state: State<'_, RouterState>,
) -> Result<(), String> {
    state.store.acknowledge_handoff_attention(&handoff_id)
}

#[tauri::command]
fn create_project(
    name: String,
    description: Option<String>,
    state: State<'_, RouterState>,
) -> Result<Project, String> {
    let project = state.store.create_project(name, description)?;
    state.store.select_workspace(&project.id, None)?;
    Ok(project)
}

#[tauri::command]
fn update_project(
    project_id: String,
    name: String,
    description: Option<String>,
    state: State<'_, RouterState>,
) -> Result<Project, String> {
    state.store.update_project(&project_id, name, description)
}

#[tauri::command]
fn create_workstream(
    project_id: String,
    name: String,
    state: State<'_, RouterState>,
) -> Result<Workstream, String> {
    state.store.create_workstream(&project_id, name)
}

#[tauri::command]
fn upsert_external_project_link(
    project_id: String,
    input: ExternalProjectLinkInput,
    state: State<'_, RouterState>,
) -> Result<ExternalProjectLink, String> {
    state
        .store
        .upsert_external_project_link(&project_id, external_project_link_from_input(input)?)
}

#[tauri::command]
fn list_external_project_links(
    project_id: String,
    state: State<'_, RouterState>,
) -> Result<Vec<ExternalProjectLink>, String> {
    state.store.external_project_links_for_project(&project_id)
}

#[tauri::command]
fn pair_workstream_endpoints(
    workstream_id: String,
    input: EndpointPairingInput,
    state: State<'_, RouterState>,
) -> Result<persistence::EndpointPairingResult, String> {
    host_application::service_pair_workstream_endpoints(workstream_id, input, state.inner())
}

#[tauri::command]
fn archive_workstream(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<Workstream, String> {
    state.store.archive_workstream(&workstream_id)
}

#[tauri::command]
fn set_workstream_pinned(
    workstream_id: String,
    pinned: bool,
    state: State<'_, RouterState>,
) -> Result<Workstream, String> {
    state.store.set_workstream_pinned(&workstream_id, pinned)
}

#[tauri::command]
fn trash_workstream(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<Workstream, String> {
    state.store.trash_workstream(&workstream_id)
}

#[tauri::command]
fn restore_workstream(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<Workstream, String> {
    state.store.restore_workstream(&workstream_id)
}

#[tauri::command]
fn purge_trashed_workstream(
    workstream_id: String,
    expected_binding_revision: i64,
    confirmation: String,
    state: State<'_, RouterState>,
) -> Result<(), String> {
    state.store.purge_trashed_workstream_confirmed(
        &workstream_id,
        expected_binding_revision,
        &confirmation,
    )
}

/// Creates a consistent local Router database snapshot.  The caller never
/// supplies a path, preventing a UI action from overwriting a project file or
/// exporting provider data; the backup stays under Router-owned local data.
#[tauri::command]
fn create_verified_local_backup(
    state: State<'_, RouterState>,
) -> Result<persistence::VerifiedBackup, String> {
    host_application::service_create_verified_local_backup(state.inner())
}

#[tauri::command]
fn read_workstream_draft(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<Option<persistence::WorkstreamDraft>, String> {
    state.store.workstream_draft(&workstream_id)
}

#[tauri::command]
fn save_workstream_draft(
    workstream_id: String,
    text: String,
    expected_revision: Option<i64>,
    state: State<'_, RouterState>,
) -> Result<persistence::WorkstreamDraft, String> {
    state
        .store
        .save_workstream_draft(&workstream_id, text, expected_revision)
}

#[tauri::command]
fn read_codex_feedback_draft(
    workstream_id: String,
    source_run_id: String,
    state: State<'_, RouterState>,
) -> Result<Option<persistence::CodexFeedbackDraft>, String> {
    state
        .store
        .codex_feedback_draft(&workstream_id, &source_run_id)
}

#[tauri::command]
fn save_codex_feedback_draft(
    workstream_id: String,
    source_run_id: String,
    text: String,
    expected_revision: Option<i64>,
    state: State<'_, RouterState>,
) -> Result<persistence::CodexFeedbackDraft, String> {
    state
        .store
        .save_codex_feedback_draft(&workstream_id, &source_run_id, text, expected_revision)
}

#[tauri::command]
fn select_workspace(
    project_id: String,
    workstream_id: Option<String>,
    state: State<'_, RouterState>,
) -> Result<WorkspaceSnapshot, String> {
    state
        .store
        .select_workspace(&project_id, workstream_id.as_deref())?;
    state.store.snapshot()
}

#[tauri::command]
fn bind_workspace_endpoint(
    workstream_id: String,
    provider: String,
    external_id: String,
    label: String,
    replace: bool,
    state: State<'_, RouterState>,
) -> Result<Endpoint, String> {
    host_application::service_bind_workspace_endpoint(
        workstream_id,
        provider,
        external_id,
        label,
        replace,
        state.inner(),
    )
}

/// Validates the exact user-supplied ChatGPT URL through the native provider surface
/// but deliberately persists nothing. The candidate is scoped to one
/// Workstream and one reviewed binding revision for this Router session.
// Native UI-window creation must run off the Windows IPC/UI thread.
#[tauri::command(async)]
fn prepare_explicit_chatgpt_endpoint_binding(
    workstream_id: String,
    input: String,
    state: State<'_, RouterState>,
) -> Result<ExplicitChatGptBindingCandidate, String> {
    state.prepare_explicit_chatgpt_endpoint_binding(&workstream_id, &input)
}

/// Creates a session-only candidate from an exact URL the owner has already
/// checked in the Windows default browser. It deliberately performs no
/// browser/provider operation and cannot change an Endpoint by itself.
#[tauri::command]
fn prepare_owner_confirmed_chatgpt_endpoint_binding(
    workstream_id: String,
    input: String,
    state: State<'_, RouterState>,
) -> Result<ExplicitChatGptBindingCandidate, String> {
    state.prepare_owner_confirmed_chatgpt_endpoint_binding(&workstream_id, &input)
}

/// Performs the only Endpoint write for an explicit ChatGPT binding. The
/// existing revision-checked atomic transaction preserves lineage and rejects
/// stale review before any ACTIVE Endpoint changes.
/// Prepare the exact provider-native browser selection; confirmation is still separate.
#[tauri::command(async)]
fn prepare_current_chatgpt_endpoint_binding(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<ExplicitChatGptBindingCandidate, String> {
    host_application::service_prepare_current_chatgpt_endpoint_binding(workstream_id, state.inner())
}

/// Only the owner-facing explicit login/security completion action calls this.
#[tauri::command(async)]
fn confirm_chatgpt_authentication_completed(state: State<'_, RouterState>) -> Result<(), String> {
    state.chatgpt.owner_confirmed_authentication()?;
    Ok(())
}

#[tauri::command]
fn confirm_explicit_chatgpt_endpoint_binding(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<Endpoint, String> {
    state.confirm_explicit_chatgpt_endpoint_binding(&workstream_id)
}

#[tauri::command]
fn confirm_explicit_chatgpt_codex_pairing(
    workstream_id: String,
    codex: EndpointPairingSideInput,
    state: State<'_, RouterState>,
) -> Result<persistence::EndpointPairingResult, String> {
    state.confirm_explicit_chatgpt_codex_pairing(&workstream_id, codex)
}

#[tauri::command]
fn begin_codex_rollover(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<RolloverCandidate, String> {
    host_application::service_begin_codex_rollover(workstream_id, state.inner())
}

#[tauri::command]
fn initialize_codex_rollover(
    workstream_id: String,
    text: String,
    state: State<'_, RouterState>,
) -> Result<RolloverCandidate, String> {
    host_application::service_initialize_codex_rollover(workstream_id, text, state.inner())
}

#[tauri::command]
fn verify_codex_rollover(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<RolloverCandidate, String> {
    host_application::service_verify_codex_rollover(workstream_id, state.inner())
}

#[tauri::command]
fn confirm_rollover(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<Endpoint, String> {
    host_application::service_confirm_rollover(workstream_id, state.inner())
}

#[tauri::command]
fn cancel_rollover(workstream_id: String, state: State<'_, RouterState>) -> Result<(), String> {
    host_application::service_cancel_rollover(workstream_id, state.inner())
}

#[tauri::command]
async fn codex_quota_read(state: State<'_, RouterState>) -> Result<codex_quota::Quota,String> {
 let core=state.inner().clone();tauri::async_runtime::spawn_blocking(move||codex_quota::read(&core)).await.map_err(|_|"QUOTA_UNAVAILABLE".to_string())
}

#[tauri::command]
fn codex_status(state: State<'_, RouterState>) -> Result<BackendStatus, String> {
    let session = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    Ok(backend_status(&session))
}

/// Local Host facts only; external login and provider connectivity are separate.
#[tauri::command]
fn host_environment_status(
    host: State<'_, HostRuntime>,
    state: State<'_, RouterState>,
) -> HostEnvironmentStatus {
    host.environment_status(&state)
}

#[tauri::command]
fn connect_codex(app: AppHandle, state: State<'_, RouterState>) -> Result<BackendStatus, String> {
    host_application::connect_codex_service(state.inner(), Arc::new(TauriEventSink(app)))
}

#[tauri::command]
fn create_thread(state: State<'_, RouterState>) -> Result<ThreadSummary, String> {
    host_application::service_create_thread(state.inner())
}

/// Starts one explicit no-project Codex conversation. Its directory is a
/// Router-owned scratch directory unless the user supplies an existing path.
/// This calls only `thread/start`: it sends no model input, creates no Codex
/// Project, and does not bind the new thread to a Router Workstream. A
/// non-ephemeral start response is not a fresh-server reopen proof.
#[tauri::command]
fn start_unprojected_codex_thread(
    directory: Option<String>,
    state: State<'_, RouterState>,
) -> Result<UnprojectedThreadStart, String> {
    host_application::service_start_unprojected_codex_thread(directory, state.inner())
}

/// Lists the official app-server catalog without deriving identity from cwd,
/// title, recency, or sidebar state. A cap is fail-closed: an incomplete
/// catalog is not presented as a complete binding selection.
#[tauri::command]
fn list_existing_codex_threads(
    state: State<'_, RouterState>,
) -> Result<ExistingCodexThreadCatalog, String> {
    host_application::service_list_existing_codex_threads(state.inner())
}

/// Fresh exact metadata verification for a selected existing thread. It does
/// not resume, start a turn, read history, mutate a Goal, or claim writer state.
#[tauri::command]
fn verify_existing_codex_thread(
    thread_id: String,
    state: State<'_, RouterState>,
) -> Result<ExistingCodexThreadCandidate, String> {
    host_application::service_verify_existing_codex_thread(thread_id, state.inner())
}

#[tauri::command]
fn resume_thread(thread_id: String, state: State<'_, RouterState>) -> Result<ResumeResult, String> {
    host_application::service_resume_thread(thread_id, state.inner())
}

/// Opens persisted Codex history by exact stable identity only. Navigation
/// must remain observational: it does not resume a thread, create a run, or
/// acquire write ownership.
#[tauri::command]
async fn read_thread_history(
    thread_id: String,
    state: State<'_, RouterState>,
) -> Result<ReadHistoryResult, String> {
    // thread/read is an external app-server request. It must not occupy the
    // native invoke/UI thread while a large or unavailable thread is read.
    let session = Arc::clone(&state.session);
    tauri::async_runtime::spawn_blocking(move || read_thread_history_blocking(thread_id, session))
        .await
        .map_err(|error| format!("Codex history worker ended unexpectedly: {error}"))?
}

#[tauri::command]
fn send_turn(
    thread_id: String,
    text: String,
    state: State<'_, RouterState>,
) -> Result<TurnStartResult, String> {
    host_application::service_send_turn(thread_id, text, state.inner())
}

/// D23 sends a user-authored correction only to the exact Codex endpoint that
/// was current when the user opened the feedback surface. It deliberately
/// rejects a rollover rather than delivering a stale draft to historical work.
#[tauri::command]
fn send_workstream_codex_feedback(
    workstream_id: String,
    expected_endpoint_id: String,
    result_run_id: String,
    text: String,
    state: State<'_, RouterState>,
) -> Result<TurnStartResult, String> {
    host_application::service_send_workstream_codex_feedback(
        workstream_id,
        expected_endpoint_id,
        result_run_id,
        text,
        state.inner(),
    )
}

#[tauri::command]
async fn read_codex_goal(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<Option<MobileCodexGoal>, String> {
    let core = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.mobile_codex_goal(&workstream_id))
        .await
        .map_err(|error| format!("Codex Goal read worker ended unexpectedly: {error}"))?
}

#[tauri::command]
async fn pause_codex_goal(
    workstream_id: String,
    confirmed: bool,
    state: State<'_, RouterState>,
) -> Result<MobileCodexGoal, String> {
    let core = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        core.pause_mobile_codex_goal(&workstream_id, MobileCodexGoalControlInput { confirmed })
    })
    .await
    .map_err(|error| format!("Codex Goal pause worker ended unexpectedly: {error}"))?
}

#[tauri::command]
async fn resume_codex_goal(
    workstream_id: String,
    confirmed: bool,
    state: State<'_, RouterState>,
) -> Result<MobileCodexGoal, String> {
    let core = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        core.resume_mobile_codex_goal(&workstream_id, MobileCodexGoalControlInput { confirmed })
    })
    .await
    .map_err(|error| format!("Codex Goal resume worker ended unexpectedly: {error}"))?
}

#[tauri::command]
async fn clear_codex_goal(
    workstream_id: String,
    confirmed: bool,
    state: State<'_, RouterState>,
) -> Result<(), String> {
    let core = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        core.clear_mobile_codex_goal(&workstream_id, MobileCodexGoalControlInput { confirmed })
    })
    .await
    .map_err(|error| format!("Codex Goal clear worker ended unexpectedly: {error}"))?
}

#[tauri::command]
async fn interrupt_codex_turn(
    workstream_id: String,
    turn_id: String,
    confirmed: bool,
    state: State<'_, RouterState>,
) -> Result<MobileCodexTurnInterruptResult, String> {
    let core = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        core.interrupt_mobile_codex_turn(
            &workstream_id,
            &turn_id,
            MobileCodexTurnInterruptInput { confirmed },
        )
    })
    .await
    .map_err(|error| format!("Codex turn interrupt worker ended unexpectedly: {error}"))?
}

/// Desktop and mobile deliberately share this exact active-Workstream Core
/// projection. The opaque action ID never becomes a UI routing identity.
#[tauri::command]
async fn codex_requests(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<Vec<MobileCodexRequest>, String> {
    let core = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.mobile_codex_requests(&workstream_id))
        .await
        .map_err(|error| format!("Codex request read worker ended unexpectedly: {error}"))?
}

/// One response for one live official server request. Core validates both the
/// opaque action and the optimistic revision before it speaks to the adapter.
#[tauri::command]
async fn respond_codex_request(
    workstream_id: String,
    request_id: String,
    input: MobileCodexResponseInput,
    state: State<'_, RouterState>,
) -> Result<(), String> {
    let core = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        core.respond_mobile_codex_request(&workstream_id, &request_id, input)
    })
    .await
    .map_err(|error| format!("Codex request response worker ended unexpectedly: {error}"))?
}

#[tauri::command]
fn review_workstream_results(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<Vec<MobileReviewResult>, String> {
    state.mobile_review_results(&workstream_id)
}

/// Desktop and mobile share the same exact-ID status projection. This never
/// reads a provider page or manufactures a substitute "latest" result.
#[tauri::command]
fn provider_run_status(
    workstream_id: String,
    run_id: String,
    state: State<'_, RouterState>,
) -> Result<MobileProviderRunStatus, String> {
    state.mobile_provider_run_status(&workstream_id, &run_id)
}

/// Desktop receives both provider lanes with their exact endpoint IDs; it
/// remains a bounded observation projection, not a transcript or ProviderRun.
#[tauri::command]
fn reply_observations(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<Vec<MobileReplyObservation>, String> {
    state.desktop_reply_observations(&workstream_id)
}

#[tauri::command]
fn mark_reply_observation_read(
    workstream_id: String,
    observation_id: String,
    state: State<'_, RouterState>,
) -> Result<(), String> {
    state.acknowledge_mobile_reply_observation(&workstream_id, &observation_id, false)
}

#[tauri::command]
fn mark_reply_observation_handled(
    workstream_id: String,
    observation_id: String,
    state: State<'_, RouterState>,
) -> Result<(), String> {
    state.acknowledge_mobile_reply_observation(&workstream_id, &observation_id, true)
}

#[tauri::command]
fn prepare_chatgpt_to_codex_handoff(
    workstream_id: String,
    response_id: String,
    initial_message: Option<String>,
    attachment_filenames: Option<Vec<String>>,
    state: State<'_, RouterState>,
) -> Result<MobileHandoffReview, String> {
    host_application::service_prepare_chatgpt_to_codex_handoff(
        workstream_id,
        response_id,
        initial_message,
        attachment_filenames,
        state.inner(),
    )
}

#[tauri::command]
fn select_chatgpt_to_codex_handoff_attachments(
    action_id: String,
    revision: u64,
    attachment_filenames: Vec<String>,
    state: State<'_, RouterState>,
) -> Result<MobileHandoffReview, String> {
    host_application::service_select_chatgpt_to_codex_handoff_attachments(
        action_id,
        revision,
        attachment_filenames,
        state.inner(),
    )
}

#[tauri::command]
fn approve_chatgpt_to_codex_handoff(
    action_id: String,
    revision: u64,
    message: String,
    state: State<'_, RouterState>,
) -> Result<MobileHandoffReview, String> {
    state.approve_mobile_reverse(&action_id, MobileApproveInput { revision, message })
}

#[tauri::command]
async fn send_chatgpt_to_codex_handoff_review(
    action_id: String,
    revision: u64,
    state: State<'_, RouterState>,
) -> Result<TurnStartResult, String> {
    let core = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        core.send_mobile_reverse(&action_id, MobileSendInput { revision })
    })
    .await
    .map_err(|error| format!("ChatGPT-to-Codex Handoff worker ended unexpectedly: {error}"))?
}

#[tauri::command]
fn prepare_codex_to_chatgpt_handoff(
    workstream_id: String,
    run_id: String,
    attachment_ids: Vec<String>,
    state: State<'_, RouterState>,
) -> Result<MobileHandoffReview, String> {
    host_application::service_prepare_codex_to_chatgpt_handoff(
        workstream_id,
        run_id,
        attachment_ids,
        state.inner(),
    )
}

#[tauri::command]
fn approve_codex_to_chatgpt_handoff(
    action_id: String,
    revision: u64,
    message: String,
    state: State<'_, RouterState>,
) -> Result<MobileHandoffReview, String> {
    state.approve_mobile_codex_outbound(&action_id, MobileApproveInput { revision, message })
}

#[tauri::command]
async fn send_codex_to_chatgpt_handoff(
    action_id: String,
    revision: u64,
    state: State<'_, RouterState>,
) -> Result<OutboundHandoffResult, String> {
    let core = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        core.send_mobile_codex_outbound(&action_id, MobileSendInput { revision })
    })
    .await
    .map_err(|error| format!("Codex-to-ChatGPT Handoff worker ended unexpectedly: {error}"))?
}

#[tauri::command]
fn detect_attachments(source_message_id: String, text: String) -> Vec<AttachmentCandidate> {
    detect(&source_message_id, &text)
}

/// Historical DevTools setup. The retired observer is hard-paused and this
/// command cannot be used as a fallback from the installed Connector.
#[tauri::command]
async fn connect_normal_chrome_read_only() -> Result<(), String> {
    Err("LEGACY_CHATGPT_TRANSPORT_RETIRED".into())
}

#[tauri::command]
async fn check_new_chatgpt_replies(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<ChatGptManualRefreshResult, String> {
    state.check_mobile_chatgpt_replies(&workstream_id)
}

/// One explicit, read-only check against the selected Workstream's exact
/// active Codex endpoint. It does not reconnect, resume, or create a run.
#[tauri::command]
fn check_new_codex_replies(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<CodexManualRefreshResult, String> {
    check_new_codex_replies_with(
        &state.store,
        &state.session,
        &workstream_id,
        &mut |payload| crate::push::send_payload(payload),
    )
}

/// Proves the persisted ACTIVE endpoint through the Router-owned Playwright
/// carrier. The sidecar opens only the canonical exact route and rejects a
/// redirect or non-matching URL; no embedded surface, client, title, or page text
/// is used as routing authority.
#[tauri::command]
async fn bound_chatgpt_provider_surface_status(
    workstream_id: String,
    state: State<'_, RouterState>,
    host: State<'_, HostRuntime>,
) -> Result<BoundChatGptProviderSurfaceStatus, String> {
    let _operation = host
        .exact_host_operation
        .lock()
        .map_err(|_| "ChatGPT direct-carrier operation is unavailable")?;
    let endpoint = state
        .store
        .active_endpoint_for_workstream(&workstream_id, Provider::Chatgpt)?
        .ok_or("The Workstream has no ACTIVE ChatGPT Endpoint")?;
    state.chatgpt.verify_exact(&endpoint.external_id)?;
    Ok(BoundChatGptProviderSurfaceStatus {
        state: "EXACT_BOUND".into(),
        current_conversation_id: Some(endpoint.external_id),
    })
}

/// Returns one bounded terminal-assistant summary from the exact ACTIVE
/// conversation. This direct-carrier projection is not a transcript read.
#[tauri::command]
async fn read_active_chatgpt_provider_latest_snapshot(
    workstream_id: String,
    state: State<'_, RouterState>,
    host: State<'_, HostRuntime>,
) -> Result<ChatGptProviderSurfaceSnapshot, String> {
    let _operation = host
        .exact_host_operation
        .lock()
        .map_err(|_| "ChatGPT direct-carrier operation is unavailable")?;
    let endpoint = state
        .store
        .active_endpoint_for_workstream(&workstream_id, Provider::Chatgpt)?
        .ok_or("The Workstream has no ACTIVE ChatGPT Endpoint")?;
    let turns = std::iter::once(state.chatgpt.observe_exact(&endpoint.external_id)?)
        .map(|reply| DirectChatGptTerminalTurn {
            id: Some(reply.message_id),
            role: "ASSISTANT",
            text: reply.text,
            streaming: false,
            terminal: true,
        })
        .collect();
    Ok(ChatGptProviderSurfaceSnapshot {
        href: format!("https://chatgpt.com/c/{}", endpoint.external_id),
        conversation_id: endpoint.external_id,
        turns,
    })
}

/// Opens and URL-verifies the exact persisted ACTIVE endpoint through the
/// Router-owned Playwright carrier. It never changes an Endpoint or sends a
/// provider request.
#[tauri::command(async)]
fn open_bound_chatgpt_conversation(
    workstream_id: String,
    state: State<'_, RouterState>,
    host: State<'_, HostRuntime>,
) -> Result<(), String> {
    host_application::service_open_bound_chatgpt_conversation(
        workstream_id,
        state.inner(),
        host.inner(),
    )
}

/// Opens the exact ACTIVE conversation in the user's default browser only
/// after the narrow read-only observer proved no matching normal-Chrome tab is
/// already open. This never starts/restarts the observer or opens a duplicate.
#[tauri::command]
fn open_bound_chatgpt_in_default_browser(
    workstream_id: String,
    state: State<'_, RouterState>,
) -> Result<DefaultBrowserOpenResult, String> {
    host_application::service_open_bound_chatgpt_in_default_browser(workstream_id, state.inner())
}

/// Opens the shared native provider profile at its fixed sign-in home.
/// No provider startup or Router binding side effect.
#[tauri::command(async)]
fn open_host_chatgpt_browser_setup(
    host: State<'_, HostRuntime>,
    state: State<'_, RouterState>,
) -> Result<(), String> {
    host.open_chatgpt_browser_setup(&state)
}

#[tauri::command]
async fn send_chatgpt_request(
    workstream_id: String,
    message: String,
    state: State<'_, RouterState>,
) -> Result<CompletedChatGptResponse, String> {
    let core = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        send_chatgpt_request_for_workstream(&core, &workstream_id, &message)
    })
    .await
    .map_err(|_| "ChatGPT request worker stopped")?
}

/// Test-only V0-005 source fixture. The only synthetic part of the routing
/// smoke is the completed ChatGPT response; downstream candidate extraction,
/// approval, persistence, endpoint resolution, and Codex delivery remain the
/// production application path.
#[cfg(debug_assertions)]
#[tauri::command]
fn seed_v0_005_routing_fixture(
    state: State<'_, RouterState>,
) -> Result<V0_005RoutingFixture, String> {
    host_application::service_seed_v0_005_routing_fixture(state.inner())
}

fn show_dashboard<R: tauri::Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn quit_host<R: tauri::Runtime>(app: &AppHandle<R>) {
    let core = app.state::<RouterCore>();
    let host = app.state::<HostRuntime>();
    host.shutdown(&core);
    app.exit(0);
}

fn on_host_window_event<R: tauri::Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        #[cfg(not(debug_assertions))]
        {
            let _ = window.hide();
        }
        #[cfg(debug_assertions)]
        {
            let host = window.state::<HostRuntime>();
            let hide = window.hide();
            let visible = window.is_visible();
            host.record_close(window.label().to_string(), hide, visible);
            let handle = window.app_handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(125));
                let visible = handle
                    .get_webview_window("main")
                    .ok_or_else(|| tauri::Error::AssetNotFound("main".into()))
                    .and_then(|main| main.is_visible());
                handle
                    .state::<HostRuntime>()
                    .record_delayed_visibility(visible);
            });
        }
    }
}

#[cfg(debug_assertions)]
fn schedule_host_acceptance(app: &tauri::App) {
    let handle = app.handle().clone();
    if host_lifecycle_acceptance_requested() {
        tauri::async_runtime::spawn(async move {
            std::thread::sleep(Duration::from_millis(750));
            let host = handle.state::<HostRuntime>();
            host.reset_lifecycle();
            let close = handle
                .get_webview_window("main")
                .ok_or_else(|| tauri::Error::AssetNotFound("main".into()))
                .and_then(|window| window.close());
            host.record_direct_close_request(close);
            std::thread::sleep(Duration::from_millis(350));
            println!(
                "HOST_LIFECYCLE_ACCEPTANCE={}",
                serde_json::to_string(&host.lifecycle_observation())
                    .unwrap_or_else(|_| "{}".into())
            );
            println!("HOST_TRAY_QUIT_MENU={:?}", host.tray_quit_enabled());
        });
    } else if host_quit_acceptance_requested() {
        tauri::async_runtime::spawn(async move {
            std::thread::sleep(Duration::from_millis(750));
            println!("HOST_QUIT_ACCEPTANCE=DISPATCHED");
            let quit_handle = handle.clone();
            let _ = handle.run_on_main_thread(move || quit_host(&quit_handle));
        });
    } else if host_autostart_acceptance_requested() {
        tauri::async_runtime::spawn(async move {
            use tauri_plugin_autostart::ManagerExt;
            let autostart = handle.autolaunch();
            let original = autostart.is_enabled();
            let enabled = if original.is_ok() {
                autostart.enable()
            } else {
                Ok(())
            };
            let after_enable = autostart.is_enabled();
            println!(
                "HOST_AUTOSTART_ACCEPTANCE_READY=original={:?};enable={:?};afterEnable={:?}",
                original, enabled, after_enable
            );
            std::thread::sleep(Duration::from_secs(12));
            let restore = match original {
                Ok(true) => autostart.enable(),
                Ok(false) => autostart.disable(),
                Err(_) => Ok(()),
            };
            let after_restore = autostart.is_enabled();
            println!(
                "HOST_AUTOSTART_ACCEPTANCE_RESTORED=restore={:?};afterRestore={:?}",
                restore, after_restore
            );
            let quit_handle = handle.clone();
            let _ = handle.run_on_main_thread(move || quit_host(&quit_handle));
        });
    }
}

fn configure_host(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let core = app.state::<RouterCore>().inner().clone();
    // Notification registration affects this application's own Windows identity only.
    let _ = watch_notifications::register_windows();
    #[cfg(windows)]
    let _ = notification_activation::register(app.handle());
    #[cfg(windows)]
    let _ = watch_notifications::repair_legacy_toasts(&core.store);
    watch_notifications::start(core.store.clone());
    watch_chat::start_queue(core.clone());
    watch_notifications::navigate(app.handle(),std::env::args());
    let host = app.state::<HostRuntime>();
    let development_dist = || {
        let cwd = std::env::current_dir().unwrap_or_default();
        if cwd.file_name().is_some_and(|name| name == "src-tauri") {
            cwd.parent().unwrap_or(&cwd).join("dist")
        } else {
            cwd.join("dist")
        }
    };
    let packaged_dist = app.path().resource_dir().ok().map(|path| path.join("dist"));
    let static_dir = packaged_dist
        .filter(|path| path.join("index.html").is_file())
        .unwrap_or_else(development_dist);
    let resources = app.path().resource_dir()?.join("browser-executor");
    let node = std::env::current_exe()?
        .parent()
        .ok_or("Host executable directory unavailable")?
        .join("browser-executor-node.exe");
    #[cfg(debug_assertions)]
    let (resources, node) = if resources.join("production-entry.mjs").is_file() {
        (resources, node)
    } else {
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        (
            source.join("resources/browser-executor"),
            source.join("binaries/browser-executor-node-x86_64-pc-windows-msvc.exe"),
        )
    };
    let data = std::path::PathBuf::from(
        std::env::var_os("LOCALAPPDATA").ok_or("Router local data unavailable")?,
    )
    .join("AIWorkRouter/browser-executor");
    core.chatgpt
        .configure(router_core::chatgpt_service::ExecutorConfiguration {
            node,
            resources,
            data,
        })?;
    let shared_resources=app.path().resource_dir()?.join("codex-shared");
    let shared_node=std::env::current_exe()?.parent().ok_or("Host directory unavailable")?.join("browser-executor-node.exe");
    #[cfg(debug_assertions)]let(shared_resources,shared_node)=if shared_resources.join("runtime.mjs").is_file(){(shared_resources,shared_node)}else{let source=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));(source.join("resources/codex-shared"),source.join("binaries/browser-executor-node-x86_64-pc-windows-msvc.exe"))};
    let hosted_resources=app.path().resource_dir()?.join("hosted-relay");
    #[cfg(debug_assertions)]let hosted_resources=if hosted_resources.join("connector.mjs").is_file(){hosted_resources}else{std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/hosted-relay")};
    crate::hosted_relay::configure_assets(hosted_resources,shared_node.clone());
    crate::shared_codex::configure(shared_resources,shared_node);
    connect_codex_for_resident_host(Arc::new(TauriEventSink(app.handle().clone())), &core);
    host.start_shared_reconnection(core.clone(),Arc::new(TauriEventSink(app.handle().clone())));
    let observer_core = core.clone();
    std::thread::spawn(move || run_chatgpt_existing_conversation_observer(observer_core));
    // Integrated startup has exactly one ChatGPT transport: Core's Executor.
    // Clearing AUTH_REQUIRED must not reactivate Connector/Native Messaging
    // registration, provisioning or its listener on the next normal startup.
    // Keep the paused legacy implementation/resources until replacement
    // coverage permits their separate retirement; do not initialize them here.
    host.start_mobile(core, static_dir, controlled_host_acceptance_requested());

    let handle = app.handle();
    let mobile_status = host.mobile_status();
    let quit = MenuItem::with_id(app, "quit-router", "Quit", true, None::<&str>)?;
    let menu = MenuBuilder::new(app)
        .text("router-host", "Router: ONLINE")
        .text("mobile-host", format!("Mobile: {mobile_status}"))
        .separator()
        .text("open-dashboard", "Open Dashboard")
        .item(&quit)
        .build()?;
    #[cfg(debug_assertions)]
    host.record_tray_quit_enabled(quit.is_enabled());
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.ico"))?;
    if let Some(window) = app.get_webview_window("main") {
        window.set_icon(icon.clone())?;
        #[cfg(windows)]
        notification_activation::install_window_identity(&window)?;
    }
    TrayIconBuilder::with_id("ai-work-router-host")
        .icon(icon)
        .tooltip("Agbrio · Agent Bridge")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open-dashboard" => show_dashboard(app),
            "quit-router" => quit_host(app),
            _ => {}
        })
        .build(handle)?;
    Ok(())
}

#[cfg(windows)]
#[tauri::command]
fn autostart_enabled(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|error| format!("Could not read Windows sign-in startup state: {error}"))
}

#[cfg(windows)]
#[tauri::command]
fn set_autostart_enabled(enabled: bool, app: AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let autostart = app.autolaunch();
    if enabled {
        autostart.enable()
    } else {
        autostart.disable()
    }
    .map_err(|error| format!("Could not update Windows sign-in startup state: {error}"))
}

pub fn run() {
    #[cfg(debug_assertions)]
    let probe_mode = false;
    #[cfg(not(debug_assertions))]
    let probe_mode = false;
    let store = Arc::new(
        if probe_mode {
            RouterStore::open_at(
                std::path::PathBuf::from(r"D:\fixtures\临时处理")
                    .join(format!("aiwr-provider-probe-{}", std::process::id()))
                    .join("fixture.db"),
            )
        } else {
            RouterStore::open_default()
        }
        .expect("Could not open Router persistence database"),
    );
    let core = RouterCore {
        chatgpt: Arc::default(),
        session: Arc::new(Mutex::new(Session::default())),
        completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
        store,
    };
    let builder = tauri::Builder::default()
        .manage(core)
        .manage(desktop_update::UpdateState::default())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(HostRuntime::default())
        .manage(watch_notifications::NotificationNavigation::default())
        .on_window_event(on_host_window_event);
    #[cfg(windows)]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            watch_notifications::navigate(app,args);
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || show_dashboard(&handle));
        }))
        .plugin(tauri_plugin_autostart::init(Default::default(), None));
    #[cfg(debug_assertions)]
    let builder = builder.setup(|app| {
        configure_host(app)?;
        schedule_host_acceptance(app);
        Ok(())
    });
    #[cfg(not(debug_assertions))]
    let builder = builder.setup(|app| configure_host(app));
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(tauri::generate_handler![
        desktop_update_status,desktop_update_check,desktop_update_download,desktop_update_install,desktop_update_open_release,hosted_connection_status,hosted_connection_redeem,assistant_help::assistant_open_help,assistant_settings,assistant_create_grant,assistant_create_instance_grant,assistant_connect_instance,assistant_revoke_grant,mobile_connection_view,mobile_connection_configure,web_pairing_code, web_paired_devices, web_revoke_device, web_availability_status, web_availability_retry, web_availability_setup, shared_codex_status, shared_codex_setup, shared_codex_launch, shared_codex_disable,
        read_message_media, bridge_directory_activity,role_bridge_state, create_bridge_workstream, rename_bridge_workstream, sync_role_bridge, role_bridge_threads, role_bridge_attachments, role_bridge_blocks, edit_role_handoff, bind_role_bridge, read_role_bridge, prepare_role_handoff, approve_role_handoff, send_role_handoff,
        codex_watch_candidates,codex_watch_list, codex_watch_enable, codex_watch_pause, codex_watch_remove, codex_watch_mark_read, codex_watch_mark_seen, codex_watch_feed, codex_watch_event, codex_notifications_web_url,
        codex_delivery_settings, codex_delivery_command, codex_notification_navigation, codex_watch_chat, codex_watch_chat_command,
        workspace_snapshot,
        workstream_snapshot,
        native_window_monitor_evidence,
        dashboard_projection,
        v0_006_smoke_observation,
        v0_008_smoke_observation,
        mark_provider_run_reviewed,
        acknowledge_handoff_attention,
        create_project,
        update_project,
        create_workstream,
        upsert_external_project_link,
        list_external_project_links,
        pair_workstream_endpoints,
        archive_workstream,
        set_workstream_pinned,
        trash_workstream,
        restore_workstream,
        purge_trashed_workstream,
        create_verified_local_backup,
        read_workstream_draft,
        save_workstream_draft,
        read_codex_feedback_draft,
        save_codex_feedback_draft,
        select_workspace,
        bind_workspace_endpoint,
        prepare_explicit_chatgpt_endpoint_binding,
        prepare_owner_confirmed_chatgpt_endpoint_binding,
        confirm_explicit_chatgpt_endpoint_binding,
        prepare_current_chatgpt_endpoint_binding,
        confirm_chatgpt_authentication_completed,
        confirm_explicit_chatgpt_codex_pairing,
        begin_codex_rollover,
        initialize_codex_rollover,
        verify_codex_rollover,
        confirm_rollover,
        cancel_rollover,
        codex_status,codex_quota_read,
        host_environment_status,
        connect_codex,
        list_existing_codex_threads,
        verify_existing_codex_thread,
        create_thread,
        start_unprojected_codex_thread,
        read_thread_history,
        resume_thread,
        send_turn,
        send_workstream_codex_feedback,
        read_codex_goal,
        pause_codex_goal,
        resume_codex_goal,
        clear_codex_goal,
        interrupt_codex_turn,
        codex_requests,
        respond_codex_request,
        review_workstream_results,
        provider_run_status,
        reply_observations,
        mark_reply_observation_read,
        mark_reply_observation_handled,
        prepare_chatgpt_to_codex_handoff,
        select_chatgpt_to_codex_handoff_attachments,
        approve_chatgpt_to_codex_handoff,
        send_chatgpt_to_codex_handoff_review,
        prepare_codex_to_chatgpt_handoff,
        approve_codex_to_chatgpt_handoff,
        send_codex_to_chatgpt_handoff,
        detect_attachments,
        connect_normal_chrome_read_only,
        check_new_chatgpt_replies,
        check_new_codex_replies,
        bound_chatgpt_provider_surface_status,
        read_active_chatgpt_provider_latest_snapshot,
        open_bound_chatgpt_conversation,
        open_bound_chatgpt_in_default_browser,
        open_host_chatgpt_browser_setup,
        send_chatgpt_request,
        autostart_enabled,
        set_autostart_enabled,
        seed_v0_005_routing_fixture
    ]);
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        desktop_update_status,desktop_update_check,desktop_update_download,desktop_update_install,desktop_update_open_release,hosted_connection_status,hosted_connection_redeem,assistant_help::assistant_open_help,assistant_settings,assistant_create_grant,assistant_create_instance_grant,assistant_connect_instance,assistant_revoke_grant,mobile_connection_view,mobile_connection_configure,web_pairing_code, web_paired_devices, web_revoke_device, web_availability_status, web_availability_retry, web_availability_setup, shared_codex_status, shared_codex_setup, shared_codex_launch, shared_codex_disable,
        codex_delivery_settings, codex_delivery_command, codex_notification_navigation, codex_watch_chat, codex_watch_chat_command,
        read_message_media, bridge_directory_activity,role_bridge_state, create_bridge_workstream, rename_bridge_workstream, sync_role_bridge, role_bridge_threads, role_bridge_attachments, role_bridge_blocks, edit_role_handoff, bind_role_bridge, read_role_bridge, prepare_role_handoff, approve_role_handoff, send_role_handoff,
        codex_watch_candidates,codex_watch_list, codex_watch_enable, codex_watch_pause, codex_watch_remove, codex_watch_mark_read, codex_watch_mark_seen, codex_watch_feed, codex_watch_event, codex_notifications_web_url,
        workspace_snapshot,
        workstream_snapshot,
        native_window_monitor_evidence,
        dashboard_projection,
        mark_provider_run_reviewed,
        acknowledge_handoff_attention,
        create_project,
        update_project,
        create_workstream,
        upsert_external_project_link,
        list_external_project_links,
        pair_workstream_endpoints,
        archive_workstream,
        set_workstream_pinned,
        trash_workstream,
        restore_workstream,
        purge_trashed_workstream,
        create_verified_local_backup,
        read_workstream_draft,
        save_workstream_draft,
        read_codex_feedback_draft,
        save_codex_feedback_draft,
        select_workspace,
        bind_workspace_endpoint,
        prepare_explicit_chatgpt_endpoint_binding,
        prepare_owner_confirmed_chatgpt_endpoint_binding,
        confirm_explicit_chatgpt_endpoint_binding,
        prepare_current_chatgpt_endpoint_binding,
        confirm_chatgpt_authentication_completed,
        confirm_explicit_chatgpt_codex_pairing,
        begin_codex_rollover,
        initialize_codex_rollover,
        verify_codex_rollover,
        confirm_rollover,
        cancel_rollover,
        codex_status,codex_quota_read,
        host_environment_status,
        connect_codex,
        list_existing_codex_threads,
        verify_existing_codex_thread,
        create_thread,
        start_unprojected_codex_thread,
        read_thread_history,
        resume_thread,
        send_turn,
        send_workstream_codex_feedback,
        read_codex_goal,
        pause_codex_goal,
        resume_codex_goal,
        clear_codex_goal,
        interrupt_codex_turn,
        codex_requests,
        respond_codex_request,
        review_workstream_results,
        provider_run_status,
        reply_observations,
        mark_reply_observation_read,
        mark_reply_observation_handled,
        prepare_chatgpt_to_codex_handoff,
        select_chatgpt_to_codex_handoff_attachments,
        approve_chatgpt_to_codex_handoff,
        send_chatgpt_to_codex_handoff_review,
        prepare_codex_to_chatgpt_handoff,
        approve_codex_to_chatgpt_handoff,
        send_codex_to_chatgpt_handoff,
        detect_attachments,
        connect_normal_chrome_read_only,
        check_new_chatgpt_replies,
        check_new_codex_replies,
        bound_chatgpt_provider_surface_status,
        read_active_chatgpt_provider_latest_snapshot,
        open_bound_chatgpt_conversation,
        open_bound_chatgpt_in_default_browser,
        open_host_chatgpt_browser_setup,
        send_chatgpt_request,
        autostart_enabled,
        set_autostart_enabled
    ]);
    builder
        .run(tauri::generate_context!())
        .expect("error while running AI Work Router");
}

#[cfg(test)]
#[path = "host_application_tests.rs"]
mod tests;

/// Historical protocol fixtures only; absent from the production executable.
#[cfg(test)]
pub fn run_native_messaging_bridge() -> Result<(), String> {
    host_application::run_legacy_native_messaging_bridge()
}
