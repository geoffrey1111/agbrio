//! Router Host application/session services and presentation DTOs.
//! No Tauri handles, windows, IPC or browser implementation are authority here.
use crate::{
    artifact, codex,
    persistence, runtime_reconciler,
};
use artifact::detector::detect;
use crate::relay_model::exact_single_handoff_block;
use codex::adapter::{CodexAdapter, WRITE_READINESS_TIMEOUT};
use codex::protocol::{history_events, FeedEvent, FeedKind};
use persistence::{
    CompletedRolloverRun, Endpoint, EndpointPairingRequest,
    EndpointPairingSide, ExternalProjectLink, NewAttachment, NewExternalProjectLink, Provider,
    ReplyObservation, RouterStore, WorkspaceSnapshot, Workstream,
};
use router_core::events::EventSink;
use runtime_reconciler::RuntimeReconciler;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// A passive observer must never allow one unavailable exact thread to hold up
// every other bound workstream for the adapter's general-purpose 90 second
// request timeout.  This is deliberately a read-only availability budget;
// it does not affect the separate write-readiness contract.
pub(crate) const CODEX_OBSERVER_REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Default)]
pub(crate) struct Session {
    /// Independent Pi task workers share Host ownership, never the Codex native
    /// process, provider protocol, permission token or conversation identity.
    pub(crate) pi: Arc<crate::pi::PiService>,
    pub(crate) adapter: Option<CodexAdapter>,
    /// Incremented whenever the Host replaces its single Codex adapter.  A
    /// retired observer exits before another poll, so reconnects cannot leave
    /// two background readers racing the same external thread.
    pub(crate) codex_observer_epoch: u64,
    pub(crate) connecting: bool,
    pub(crate) codex_adapter_borrowed:bool,
    pub(crate) connection_detail: Option<String>,
    pub(crate) selected_thread_id: Option<String>,
    pub(crate) ready_threads: HashSet<String>,
    pub(crate) rollover_candidates: HashMap<String, RolloverCandidate>,
    pub(crate) pending_codex_requests: HashMap<String, PendingCodexRequest>,
    pub(crate) resolved_codex_requests: std::collections::VecDeque<(String, Value)>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExternalProjectLinkInput {
    pub(crate) provider: String,
    pub(crate) external_project_id: Option<String>,
    pub(crate) canonical_url: Option<String>,
    pub(crate) label: String,
    pub(crate) source_kind: String,
    pub(crate) source_version: Option<String>,
    pub(crate) verified_at: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EndpointPairingSideInput {
    pub(crate) expected_active_endpoint_id: Option<String>,
    pub(crate) external_id: String,
    pub(crate) label: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EndpointPairingInput {
    pub(crate) expected_binding_revision: i64,
    pub(crate) chatgpt: Option<EndpointPairingSideInput>,
    pub(crate) codex: Option<EndpointPairingSideInput>,
}



#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnprojectedThreadStart {
    pub(crate) thread: ThreadSummary,
    pub(crate) directory: String,
}

/// Candidate state is deliberately session-only.  A Router restart drops it
/// without changing the persisted ACTIVE binding.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RolloverCandidate {
    pub(crate) workstream_id: String,
    pub(crate) provider: String,
    pub(crate) expected_old_endpoint_id: String,
    pub(crate) expected_old_external_id: String,
    pub(crate) candidate_external_id: String,
    pub(crate) label: String,
    pub(crate) state: String,
    pub(crate) initialization_text: Option<String>,
    pub(crate) initialization_turn_id: Option<String>,
    pub(crate) initialization_result_identity: Option<String>,
    pub(crate) initialization_started_at: Option<i64>,
    pub(crate) initialization_terminal_at: Option<i64>,
    pub(crate) error: Option<String>,
}




/// The only live Router authority. Presentation adapters (Tauri IPC and the
/// loopback HTTP interface) share this value; neither owns a second Store or
/// Session state machine.
#[derive(Clone)]
pub(crate) struct RouterCore {
    pub(crate) session: Arc<Mutex<Session>>,
    /// Router-observed ChatGPT completions are independent of the long-lived
    /// Codex app-server adapter. Keeping this cache out of `Session` prevents
    /// an observational `thread/read` from delaying an already-completed
    /// ChatGPT result or its mobile review path.
    pub(crate) store: Arc<RouterStore>,
}

pub(crate) type RouterState = RouterCore;













/// Official app-server requests are live adapter state, not a persistence
/// stream. The opaque action ID is the only browser-facing identity; the raw
/// JSON-RPC ID remains typed so `1` and `"1"` cannot collide on response.
#[derive(Clone)]
pub(crate) struct PendingCodexRequest {
    pub(crate) action_id: String,
    pub(crate) raw_request_id: Value,
    pub(crate) method: String,
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
    pub(crate) item_id: Option<String>,
    pub(crate) params: Value,
    pub(crate) revision: u64,
    pub(crate) responded: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileWorkstreamItem {
    pub(crate) id: String,
    pub(crate) name: String,
    /// Read-only visual disambiguation for same-named workstreams. Routing
    /// remains on the full stable IDs held by the Router, never this label.
    pub(crate) binding_summary: String,
    pub(crate) source_label: String,
    pub(crate) project_name: String,
    pub(crate) status: String,
    pub(crate) trashed_at: Option<i64>,
    pub(crate) pinned_at: Option<i64>,
    pub(crate) updated_at: i64,
    pub(crate) chatgpt_status: Option<String>,
    pub(crate) codex_status: Option<String>,
    pub(crate) attention_count: usize,
    pub(crate) attention_items: Vec<MobileAttentionItem>,
    pub(crate) last_activity_at: i64,
    pub(crate) last_message_at: Option<i64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileAttentionItem {
    pub(crate) source_id: String,
    pub(crate) kind: String,
    pub(crate) priority: i32,
    pub(crate) message: String,
    pub(crate) activity_at: i64,
}


#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileReplyObservation {
    pub(crate) id: String,
    pub(crate) endpoint_id: String,
    pub(crate) text: String,
    pub(crate) observed_at: i64,
    pub(crate) read_at: Option<i64>,
    pub(crate) handled_at: Option<i64>,
    pub(crate) push_state: String,
    pub(crate) push_rendered_at: Option<i64>,
    pub(crate) marker_text: Option<String>,
    pub(crate) attachments: Vec<MobileAttachment>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileReviewResult {
    pub(crate) run_id: String,
    pub(crate) provider: String,
    pub(crate) result_identity: String,
    pub(crate) text: String,
    pub(crate) reviewed_at: Option<i64>,
    pub(crate) marker_text: Option<String>,
    pub(crate) attachments: Vec<MobileAttachment>,
    /// An opaque Router run handle exposed only when one exact unreadable
    /// direct ChatGPT result can be re-observed without guessing.
    pub(crate) recovery_run_id: Option<String>,
}

/// A bounded status projection for one exact ProviderRun. It excludes result
/// text and provider diagnostics: completed output remains on the separate
/// review-result surface, while a failed/cancelled/unknown run needs an
/// auditable owner-facing explanation without becoming a transcript.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileProviderRunStatus {
    pub(crate) run_id: String,
    pub(crate) provider: String,
    pub(crate) status: String,
    pub(crate) terminal_code: Option<String>,
    pub(crate) origin_handoff_id: Option<String>,
    pub(crate) started_at: Option<i64>,
    pub(crate) terminal_at: Option<i64>,
    pub(crate) updated_at: i64,
    pub(crate) has_reviewable_result: bool,
}


#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileAttachment {
    pub(crate) id: String,
    pub(crate) filename: String,
    pub(crate) integrity_status: String,
    pub(crate) default_selected: bool,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileCodexRequest {
    pub(crate) request_id: String,
    pub(crate) revision: u64,
    pub(crate) method: String,
    pub(crate) kind: String,
    /// The only displayable explanation verified in the current official
    /// request schema.  It is deliberately not presented as a command, cwd,
    /// or raw JSON-RPC payload.
    pub(crate) reason: Option<String>,
    pub(crate) choices: Vec<MobileCodexChoice>,
    pub(crate) questions: Vec<MobileCodexQuestion>,
    pub(crate) is_blocking: bool,
    pub(crate) response_sent: bool,
}

/// Sanitized, exact-ID projection of an app-server Goal.  Goal accounting is
/// reported by Codex; Router never derives a timer or rewrites usage.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileCodexGoal {
    pub(crate) fingerprint:String,
    pub(crate) thread_id: String,
    pub(crate) objective: String,
    pub(crate) status: String,
    pub(crate) token_budget: Option<i64>,
    pub(crate) tokens_used: Option<i64>,
    pub(crate) time_used_seconds: Option<i64>,
    pub(crate) created_at: Option<Value>,
    pub(crate) updated_at: Option<Value>,
    pub(crate) active_turn_id: Option<String>,
}

/// Every Goal mutation is an explicit human control. It is intentionally not
/// reused for `turn/interrupt`, which has a separate exact turn confirmation.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileCodexGoalControlInput {
    pub(crate) confirmed: bool,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileCodexTurnInterruptInput {
    pub(crate) confirmed: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileCodexTurnInterruptResult {
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
    pub(crate) requested: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileCodexChoice {
    pub(crate) id: String,
    pub(crate) label: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileCodexQuestion {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) placeholder: Option<String>,
    pub(crate) required: bool,
    pub(crate) options: Vec<MobileCodexInputOption>,
    pub(crate) is_other: bool,
    pub(crate) is_secret: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileCodexInputOption { pub(crate) label: String, pub(crate) description: String }











#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileFeedbackInput {
    pub(crate) run_id: String,
    pub(crate) feedback: String,
}


#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileCodexResponseInput {
    pub(crate) revision: u64,
    pub(crate) decision: Option<String>,
    pub(crate) answers: Option<HashMap<String, String>>,
}

pub(crate) fn mobile_codex_attachment_candidates(
    identity: &str,
    text: &str,
) -> Vec<MobileAttachment> {
    detect(identity, text)
        .into_iter()
        .map(|candidate| MobileAttachment {
            id: candidate.id,
            filename: candidate.filename,
            integrity_status: format!("{:?}", candidate.integrity_status).to_uppercase(),
            default_selected: candidate.default_selected,
            warnings: candidate.warnings,
        })
        .collect()
}

impl RouterCore {
    pub(crate) fn shutdown_owned_adapter(&self) {
        let pi=self.session.lock().ok().map(|session|session.pi.clone());
        if let Some(pi)=pi{pi.shutdown();}
        if let Ok(mut session) = self.session.lock() {
            if let Some(adapter) = session.adapter.as_mut() {
                adapter.shutdown();
            }
            session.adapter = None;
            session.ready_threads.clear();
        }
    }

    pub(crate) fn mobile_workstreams(&self) -> Result<Vec<MobileWorkstreamItem>, String> {
        let pending={let session=self.session.lock().map_err(|_|"Router session is unavailable")?;session.pending_codex_requests.values().filter(|request|!session.codex_adapter_borrowed&&!session.adapter.as_ref().is_some_and(|a|a.is_shared()&&!a.is_shared_subscribed(&request.thread_id)&&!a.is_exact_turn_active(&request.thread_id,&request.turn_id))).map(|request|request.thread_id.clone()).collect::<HashSet<_>>()};
        Ok(self
            .store
            .dashboard_projection()?
            .workstreams
            .into_iter()
            .map(|item| {
                let mut attention_items = item
                    .attention_items
                    .into_iter()
                    .map(|attention| MobileAttentionItem {
                        source_id: attention.source_id,
                        kind: attention.kind,
                        priority: attention.priority,
                        message: attention.message,
                        activity_at: attention.activity_at,
                    })
                    .collect::<Vec<_>>();
                if let Some(endpoint) = item
                    .codex_endpoint
                    .as_ref()
                    .filter(|endpoint| pending.contains(&endpoint.external_id))
                {
                    attention_items.push(MobileAttentionItem {
                        source_id: endpoint.id.clone(),
                        kind: "CODEX_STRUCTURED_REQUEST".into(),
                        priority: 0,
                        message: "Codex has a structured request waiting for one owner response"
                            .into(),
                        activity_at: endpoint.created_at,
                    });
                }
                attention_items.sort_by_key(|attention| attention.priority);
                let mut binding_parts = vec![format!(
                    "工作流 {}",
                    abbreviated_identity(&item.workstream.id)
                )];
                if let Some(endpoint) = item.chatgpt_endpoint.as_ref() {
                    binding_parts.push(format!(
                        "ChatGPT {}",
                        abbreviated_identity(&endpoint.external_id)
                    ));
                }
                if let Some(endpoint) = item.codex_endpoint.as_ref() {
                    binding_parts.push(format!(
                        "Codex {}",
                        abbreviated_identity(&endpoint.external_id)
                    ));
                }
                MobileWorkstreamItem {
                    source_label: [item.chatgpt_endpoint.as_ref(),item.codex_endpoint.as_ref()].into_iter().flatten().map(|endpoint|format!("{} · {}",if endpoint.provider=="CODEX"{"Codex"}else{"ChatGPT"},endpoint.label)).collect::<Vec<_>>().join(" / "),
                    project_name:item.project_name,
                    status:item.workstream.status.clone(),
                    trashed_at:item.workstream.trashed_at,
                    pinned_at:item.workstream.pinned_at,
                    updated_at:item.workstream.updated_at.max(item.last_activity_at),
                    id: item.workstream.id,
                    name: item.workstream.name,
                    binding_summary: binding_parts.join(" · "),
                    chatgpt_status: item.chatgpt_run.map(|run| run.status),
                    codex_status: item.codex_run.map(|run| run.status),
                    attention_count: attention_items.len(),
                    attention_items,
                    last_activity_at: item.last_activity_at,
                    last_message_at: item.last_message_at,
                }
            })
            .collect())
    }

    pub(crate) fn mobile_workstream_snapshot(
        &self,
        workstream_id: &str,
    ) -> Result<WorkspaceSnapshot, String> {
        self.store.snapshot_for_workstream(workstream_id)
    }


    /// Mobile exposes the same local project-link record as desktop, scoped by
    /// the exact selected Workstream. This is a saved reference only: it does
    /// not claim a provider directory or route a conversation.
    pub(crate) fn mobile_external_project_links(
        &self,
        workstream_id: &str,
    ) -> Result<Vec<ExternalProjectLink>, String> {
        let snapshot = self.store.snapshot_for_workstream(workstream_id)?;
        let workstream = snapshot
            .workstreams
            .iter()
            .find(|item| item.id == workstream_id)
            .ok_or("The selected Workstream is no longer available")?;
        self.store
            .external_project_links_for_project(&workstream.project_id)
    }

    pub(crate) fn upsert_mobile_external_project_link(
        &self,
        workstream_id: &str,
        input: ExternalProjectLinkInput,
    ) -> Result<ExternalProjectLink, String> {
        let snapshot = self.store.snapshot_for_workstream(workstream_id)?;
        let workstream = snapshot
            .workstreams
            .iter()
            .find(|item| item.id == workstream_id)
            .ok_or("The selected Workstream is no longer available")?;
        self.store.upsert_external_project_link(
            &workstream.project_id,
            external_project_link_from_input(input)?,
        )
    }

    /// The mobile command uses the same official `thread/start` path as the
    /// desktop no-project action. It neither sends a turn nor binds a Router
    /// Workstream or creates a Codex Project. `thread/start` is not treated as
    /// proof that the returned ID survives a fresh official app-server.
    pub(crate) fn start_mobile_unprojected_codex_thread(
        &self,
        directory: Option<String>,
    ) -> Result<UnprojectedThreadStart, String> {
        let directory = unprojected_thread_directory(directory)?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        let adapter = session
            .adapter
            .as_mut()
            .ok_or("Codex backend disconnected. Use Reconnect first.")?;
        let summary = start_verified_codex_thread(adapter, &directory)?;
        Ok(UnprojectedThreadStart {
            thread: summary,
            directory: directory.to_string_lossy().to_string(),
        })
    }

    pub(crate) fn codex_creation_models(&self)->Result<Value,String>{
        let mut session=self.session.lock().map_err(|_|"Router session unavailable")?;
        crate::codex::creation::models(session.adapter.as_mut().ok_or("Codex backend disconnected")?)
    }
    pub(crate) fn validate_codex_creation_settings(&self,settings:&crate::codex::creation::Settings)->Result<(),String>{
        settings.shape()?;if !settings.any(){return Ok(());}
        let mut session=self.session.lock().map_err(|_|"Router session unavailable")?;
        crate::codex::creation::validate(session.adapter.as_mut().ok_or("Codex backend disconnected")?,settings)
    }
    pub(crate) fn start_configured_codex_thread(&self,directory:&str,settings:&crate::codex::creation::Settings)->Result<Value,String>{
        let directory=unprojected_thread_directory(Some(directory.into()))?;
        let mut session=self.session.lock().map_err(|_|"Router session unavailable")?;
        crate::codex::creation::start(session.adapter.as_mut().ok_or("Codex backend disconnected")?,&directory,settings)
    }

    pub(crate) fn create_mobile_verified_local_backup(
        &self,
    ) -> Result<persistence::VerifiedBackup, String> {
        let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
        let backup_directory = std::path::PathBuf::from(local)
            .join("AIWorkRouter")
            .join("backups");
        fs::create_dir_all(&backup_directory)
            .map_err(|error| format!("Could not create Router backup directory: {error}"))?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("Could not timestamp Router backup: {error}"))?
            .as_secs();
        let target = backup_directory.join(format!("router-v3-{timestamp}-{}.db", Uuid::new_v4()));
        self.store.create_verified_backup(target)
    }

    pub(crate) fn mobile_workstream_draft(
        &self,
        workstream_id: &str,
    ) -> Result<Option<persistence::WorkstreamDraft>, String> {
        self.store.workstream_draft(workstream_id)
    }

    pub(crate) fn save_mobile_workstream_draft(
        &self,
        workstream_id: &str,
        text: String,
        expected_revision: Option<i64>,
    ) -> Result<persistence::WorkstreamDraft, String> {
        self.store
            .save_workstream_draft(workstream_id, text, expected_revision)
    }

    pub(crate) fn mobile_codex_feedback_draft(
        &self,
        workstream_id: &str,
        source_run_id: &str,
    ) -> Result<Option<persistence::CodexFeedbackDraft>, String> {
        self.store
            .codex_feedback_draft(workstream_id, source_run_id)
    }

    pub(crate) fn save_mobile_codex_feedback_draft(
        &self,
        workstream_id: &str,
        source_run_id: &str,
        text: String,
        expected_revision: Option<i64>,
    ) -> Result<persistence::CodexFeedbackDraft, String> {
        self.store
            .save_codex_feedback_draft(workstream_id, source_run_id, text, expected_revision)
    }

    pub(crate) fn archive_mobile_workstream(
        &self,
        workstream_id: &str,
    ) -> Result<Workstream, String> {
        self.store.archive_workstream(workstream_id)
    }

    pub(crate) fn set_mobile_workstream_pinned(
        &self,
        workstream_id: &str,
        pinned: bool,
    ) -> Result<Workstream, String> {
        self.store.set_workstream_pinned(workstream_id, pinned)
    }

    pub(crate) fn trash_mobile_workstream(
        &self,
        workstream_id: &str,
    ) -> Result<Workstream, String> {
        self.store.trash_workstream(workstream_id)
    }

    pub(crate) fn restore_mobile_workstream(
        &self,
        workstream_id: &str,
    ) -> Result<Workstream, String> {
        self.store.restore_workstream(workstream_id)
    }

    pub(crate) fn purge_mobile_trashed_workstream(
        &self,
        workstream_id: &str,
        expected_binding_revision: i64,
        confirmation: &str,
    ) -> Result<(), String> {
        self.store.purge_trashed_workstream_confirmed(
            workstream_id,
            expected_binding_revision,
            confirmation,
        )
    }

    pub(crate) fn pair_mobile_workstream_endpoints(
        &self,
        workstream_id: &str,
        input: EndpointPairingInput,
    ) -> Result<persistence::EndpointPairingResult, String> {
        if input.chatgpt.is_some() {
            return Err(
                "CHATGPT_REMOVED".into(),
            );
        }
        let side = |value: EndpointPairingSideInput| EndpointPairingSide {
            expected_active_endpoint_id: value.expected_active_endpoint_id,
            external_id: value.external_id,
            label: value.label,
        };
        self.store.pair_workstream_endpoints_checked(
            workstream_id,
            EndpointPairingRequest {
                expected_binding_revision: input.expected_binding_revision,
                chatgpt: None,
                codex: input.codex.map(side),
            },
        )
    }







    /// Mobile reads identify only a Router Workstream. The Core resolves the
    /// current ACTIVE Codex endpoint and retains the existing exact-ID read
    /// guard without acquiring writer ownership.
    pub(crate) fn read_active_codex_history(
        &self,
        workstream_id: &str,
    ) -> Result<ReadHistoryResult, String> {
        let endpoint = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Codex)?
            .ok_or("The Workstream has no ACTIVE Codex Endpoint")?;
        read_thread_history_blocking(endpoint.external_id, Arc::clone(&self.session))
    }

    pub(crate) fn mobile_reply_observations(
        &self,
        workstream_id: &str,
    ) -> Result<Vec<MobileReplyObservation>, String> {
        let codex_endpoint_id = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Codex)?
            .map(|endpoint| endpoint.id);
        // A phone must receive the same current, exact provider lanes as the
        // desktop reader.  Filtering this projection to ChatGPT made a
        // durable Codex observation invisible on mobile even though the
        // manual Codex check had correctly created it.
        let mut active_endpoint_ids = HashSet::new();
        for provider in [Provider::Chatgpt, Provider::Codex] {
            if let Some(endpoint) = self
                .store
                .active_endpoint_for_workstream(workstream_id, provider)?
            {
                active_endpoint_ids.insert(endpoint.id);
            }
        }
        if active_endpoint_ids.is_empty() {
            return Err("The Workstream has no ACTIVE provider Endpoint".into());
        }
        Ok(self
            .store
            .reply_observations_for_workstream(workstream_id)?
            .into_iter()
            .filter(|item| active_endpoint_ids.contains(&item.endpoint_id))
            .map(|item| {
                let attachments = if codex_endpoint_id.as_deref() == Some(item.endpoint_id.as_str())
                {
                    mobile_codex_attachment_candidates(
                        item.assistant_identity.as_deref().unwrap_or(&item.id),
                        &item.text,
                    )
                } else {
                    Vec::new()
                };
                MobileReplyObservation {
                    id: item.id,
                    endpoint_id: item.endpoint_id,
                    text: item.text.clone(),
                    observed_at: item.observed_at,
                    read_at: item.read_at,
                    handled_at: item.handled_at,
                    push_state: item.push_state,
                    push_rendered_at: item.push_rendered_at,
                    marker_text: exact_single_handoff_block(&item.text, "CODEX_HANDOFF"),
                    attachments,
                }
            })
            .collect())
    }



    /// Phone and desktop use the same passive exact Codex thread read. It
    /// does not resume/start a turn and does not create a ProviderRun.
    pub(crate) fn check_mobile_codex_replies(
        &self,
        workstream_id: &str,
    ) -> Result<CodexManualRefreshResult, String> {
        check_new_codex_replies_with(&self.store, &self.session, workstream_id, &mut |payload| {
            crate::push::send_payload(payload)
        })
    }

    /// Desktop needs the distinct active ChatGPT and Codex observation lanes
    /// at once. The endpoint ID is retained so presentation cannot merge them
    /// by title or text.
    pub(crate) fn desktop_reply_observations(
        &self,
        workstream_id: &str,
    ) -> Result<Vec<MobileReplyObservation>, String> {
        let codex_endpoint_id = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Codex)?
            .map(|endpoint| endpoint.id);
        Ok(self
            .store
            .reply_observations_for_workstream(workstream_id)?
            .into_iter()
            .map(|item| {
                let attachments = if codex_endpoint_id.as_deref() == Some(item.endpoint_id.as_str())
                {
                    mobile_codex_attachment_candidates(
                        item.assistant_identity.as_deref().unwrap_or(&item.id),
                        &item.text,
                    )
                } else {
                    Vec::new()
                };
                MobileReplyObservation {
                    id: item.id,
                    endpoint_id: item.endpoint_id,
                    text: item.text.clone(),
                    observed_at: item.observed_at,
                    read_at: item.read_at,
                    handled_at: item.handled_at,
                    push_state: item.push_state,
                    push_rendered_at: item.push_rendered_at,
                    marker_text: exact_single_handoff_block(&item.text, "CODEX_HANDOFF"),
                    attachments,
                }
            })
            .collect())
    }

    pub(crate) fn acknowledge_mobile_reply_observation(
        &self,
        workstream_id: &str,
        observation_id: &str,
        handled: bool,
    ) -> Result<(), String> {
        self.store
            .acknowledge_reply_observation(workstream_id, observation_id, handled)
    }




    /// Returns only the bounded, durable review result for each provider. It
    /// intentionally does not expose ProviderRun history as a transcript.
    pub(crate) fn mobile_review_results(
        &self,
        workstream_id: &str,
    ) -> Result<Vec<MobileReviewResult>, String> {
        let active_chatgpt = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?;
        let mut results = Vec::new();
        for (provider, marker) in [("CHATGPT", "CODEX_HANDOFF"), ("CODEX", "CHATGPT_HANDOFF")] {
            let Some(run) = self
                .store
                .latest_reviewable_provider_result(workstream_id, provider)?
            else {
                continue;
            };
            let text = run
                .result_text
                .clone()
                .ok_or("Reviewable ProviderRun has no result text")?;
            let result_identity = run
                .result_identity
                .clone()
                .ok_or("Reviewable ProviderRun has no exact result identity")?;
            let attachments = if provider == "CODEX" {
                mobile_codex_attachment_candidates(&result_identity, &text)
            } else {
                vec![]
            };
            results.push(MobileReviewResult {
                run_id: run.id.clone(),
                provider: provider.into(),
                result_identity,
                text: text.clone(),
                reviewed_at: run.reviewed_at,
                marker_text: exact_single_handoff_block(&text, marker),
                attachments,
                recovery_run_id: if provider == "CHATGPT" {
                    active_chatgpt.as_ref().map_or(Ok(None), |endpoint| {
                        self.store
                            .recoverable_chatgpt_feedback_run_for_source(&run.id, &endpoint.id)
                    })?
                } else {
                    None
                },
            });
        }
        Ok(results)
    }

    /// Returns status only for the exact run named by the owner-facing Inbox
    /// item. It is intentionally not a "latest" lookup and does no provider,
    /// browser, or retry work, so a failed historical run can never be
    /// replaced by a more recent result from the same Workstream.
    pub(crate) fn mobile_provider_run_status(
        &self,
        workstream_id: &str,
        run_id: &str,
    ) -> Result<MobileProviderRunStatus, String> {
        self.store.snapshot_for_workstream(workstream_id)?;
        let run = self
            .store
            .provider_runs_for_workstream(workstream_id)?
            .into_iter()
            .find(|candidate| candidate.id == run_id)
            .ok_or("The Inbox-selected ProviderRun is no longer available")?;
        Ok(MobileProviderRunStatus {
            run_id: run.id,
            provider: run.provider,
            status: run.status,
            terminal_code: run.terminal_code,
            origin_handoff_id: run.origin_handoff_id,
            started_at: run.started_at,
            terminal_at: run.terminal_at,
            updated_at: run.updated_at,
            has_reviewable_result: run.result_identity.is_some() && run.result_text.is_some(),
        })
    }

    pub(crate) fn mobile_codex_requests(
        &self,
        workstream_id: &str,
    ) -> Result<Vec<MobileCodexRequest>, String> {
        let active = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Codex)?
            .ok_or("The Workstream has no ACTIVE Codex Endpoint")?;
        let session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        Ok(session
            .pending_codex_requests
            .values()
            .filter(|request| request.thread_id == active.external_id)
            .filter(|request| !session.codex_adapter_borrowed&&!session.adapter.as_ref().is_some_and(|a|a.is_shared()&&!a.is_shared_subscribed(&request.thread_id)&&!a.is_exact_turn_active(&request.thread_id,&request.turn_id)))
            .filter_map(mobile_codex_request_projection)
            .collect())
    }

    /// Reads the Goal belonging to the exact persisted ACTIVE Codex endpoint.
    /// This is intentionally read-only: opening a Goal neither resumes a
    /// thread nor acquires a writer.
    pub(crate) fn mobile_codex_goal(
        &self,
        workstream_id: &str,
    ) -> Result<Option<MobileCodexGoal>, String> {
        let endpoint = self.active_codex_endpoint(workstream_id)?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        let adapter = session
            .adapter
            .as_mut()
            .ok_or("Codex Goal is unavailable because the backend is disconnected.")?;
        if adapter.is_closed() {
            return Err("Codex Goal is unavailable because the backend is disconnected.".into());
        }
        let mut goal = codex_goal_from_response(
            &endpoint.external_id,
            &adapter.get_goal(&endpoint.external_id)?,
        )?;
        if let Some(goal) = goal.as_mut() {
            goal.active_turn_id = adapter.active_turn_id(&endpoint.external_id);
        }
        Ok(goal)
    }

    pub(crate) fn pause_mobile_codex_goal(
        &self,
        workstream_id: &str,
        input: MobileCodexGoalControlInput,
    ) -> Result<MobileCodexGoal, String> {
        self.control_mobile_codex_goal(workstream_id, input, "paused")
    }

    pub(crate) fn resume_mobile_codex_goal(
        &self,
        workstream_id: &str,
        input: MobileCodexGoalControlInput,
    ) -> Result<MobileCodexGoal, String> {
        self.control_mobile_codex_goal(workstream_id, input, "active")
    }

    /// Goal deletion is separate from pausing and from interrupting a current
    /// turn. It clears only the official Goal record after an explicit user
    /// confirmation and exact endpoint revalidation.
    pub(crate) fn clear_mobile_codex_goal(
        &self,
        workstream_id: &str,
        input: MobileCodexGoalControlInput,
    ) -> Result<(), String> {
        require_goal_confirmation(input.confirmed)?;
        let endpoint = self.active_codex_endpoint(workstream_id)?;
        let thread_id = endpoint.external_id;
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        let current = read_codex_goal_from_session(&mut session, &thread_id)?
            .ok_or("The current Codex Goal no longer exists; it was not cleared again.")?;
        // Explicitly acquire the Router writer before the external mutation.
        // This fails closed when another application owns the thread and does
        // not retry or take over that writer.
        ensure_thread_ready_for_write(&mut session, &thread_id)?;
        let rechecked = read_codex_goal_from_session(&mut session, &thread_id)?
            .ok_or("The Codex Goal changed before deletion; refresh and confirm again.")?;
        if rechecked.updated_at != current.updated_at || rechecked.status != current.status {
            return Err(
                "The Codex Goal changed before deletion; refresh and confirm again.".into(),
            );
        }
        {
            let adapter = session
                .adapter
                .as_mut()
                .ok_or("Codex Goal is unavailable because the backend is disconnected.")?;
            adapter
                .clear_goal(&thread_id)
                .map_err(thread_write_readiness_error)?;
        }
        if read_codex_goal_from_session(&mut session, &thread_id)?.is_some() {
            return Err("Codex did not confirm that the exact Goal was cleared.".into());
        }
        Ok(())
    }

    /// An interrupt requests cancellation for one exact Router-observed active
    /// turn. It never changes Goal status and does not clear the active-turn
    /// registry until the matching lifecycle completion arrives.
    pub(crate) fn interrupt_mobile_codex_turn(
        &self,
        workstream_id: &str,
        turn_id: &str,
        input: MobileCodexTurnInterruptInput,
    ) -> Result<MobileCodexTurnInterruptResult, String> {
        if !input.confirmed {
            return Err("Confirm stopping this exact Codex turn before continuing.".into());
        }
        if turn_id.trim().is_empty() {
            return Err("A non-empty exact Codex turn ID is required to stop this turn.".into());
        }
        let endpoint = self.active_codex_endpoint(workstream_id)?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        let adapter = session
            .adapter
            .as_mut()
            .ok_or("Codex turn control is unavailable because the backend is disconnected.")?;
        if adapter.is_closed() {
            return Err(
                "Codex turn control is unavailable because the backend is disconnected.".into(),
            );
        }
        if !adapter.is_exact_turn_active(&endpoint.external_id, turn_id) {
            return Err("This is not the current Router-observed active Codex turn; it was not interrupted.".into());
        }
        adapter
            .interrupt_turn(&endpoint.external_id, turn_id)
            .map_err(thread_write_readiness_error)?;
        Ok(MobileCodexTurnInterruptResult {
            thread_id: endpoint.external_id,
            turn_id: turn_id.to_string(),
            requested: true,
        })
    }

    pub(crate) fn control_mobile_codex_goal(
        &self,
        workstream_id: &str,
        input: MobileCodexGoalControlInput,
        target_status: &str,
    ) -> Result<MobileCodexGoal, String> {
        require_goal_confirmation(input.confirmed)?;
        let endpoint = self.active_codex_endpoint(workstream_id)?;
        let thread_id = endpoint.external_id;
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        let current = read_codex_goal_from_session(&mut session, &thread_id)?
            .ok_or("The current Codex Goal no longer exists; refresh before changing it.")?;
        ensure_goal_transition(&current.status, target_status)?;
        ensure_thread_ready_for_write(&mut session, &thread_id)?;
        let rechecked = read_codex_goal_from_session(&mut session, &thread_id)?
            .ok_or("The Codex Goal changed before this action; refresh and confirm again.")?;
        if rechecked.updated_at != current.updated_at || rechecked.status != current.status {
            return Err(
                "The Codex Goal changed before this action; refresh and confirm again.".into(),
            );
        }
        let adapter = session
            .adapter
            .as_mut()
            .ok_or("Codex Goal is unavailable because the backend is disconnected.")?;
        let updated = adapter
            .set_goal_status(&thread_id, target_status)
            .map_err(thread_write_readiness_error)?;
        codex_goal_from_response(&thread_id, &updated)?
            .ok_or("Codex did not return the updated exact Goal".to_string())
    }

    pub(crate) fn active_codex_endpoint(&self, workstream_id: &str) -> Result<Endpoint, String> {
        self.store
            .active_endpoint_for_workstream(workstream_id, Provider::Codex)?
            .ok_or("The Workstream has no ACTIVE Codex Endpoint".into())
    }



    pub(crate) fn mobile_codex_feedback(
        &self,
        workstream_id: &str,
        input: MobileFeedbackInput,
    ) -> Result<TurnStartResult, String> {
        if input.feedback.trim().is_empty() {
            return Err("Codex revision feedback cannot be empty".into());
        }
        let retained = self
            .store
            .latest_reviewable_provider_result(workstream_id, "CODEX")?
            .ok_or("No current complete Codex result is available for feedback")?;
        if retained.id != input.run_id {
            return Err(
                "The Codex result changed; re-open the current review before sending feedback"
                    .into(),
            );
        }
        let endpoint = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Codex)?
            .ok_or("The Workstream has no ACTIVE Codex Endpoint")?;
        ensure_retained_result_is_active(&retained, &endpoint)?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        let adapter = session
            .adapter
            .as_mut()
            .ok_or("Codex backend disconnected. Use Reconnect first.")?;
        if adapter.is_turn_active(&endpoint.external_id) {
            return Err("Codex is already processing a turn for this Workstream".into());
        }
        ensure_thread_ready_for_write(&mut session, &endpoint.external_id)?;
        let run = self.store.create_provider_run(
            workstream_id,
            &endpoint.id,
            "CODEX",
            None,
            None,
            "STARTING",
        )?;
        let result = match start_turn(
            session
                .adapter
                .as_mut()
                .ok_or("Codex backend disconnected. Use Reconnect first.")?,
            &endpoint.external_id,
            &input.feedback,
        ) {
            Ok(result) => result,
            Err(error) => {
                let _ = self
                    .store
                    .fail_provider_run_by_id(&run.id, "CODEX_ADAPTER_ERROR");
                return Err(error);
            }
        };
        if let Err(error) =
            self.store
                .attach_provider_run_external_identity(&run.id, "CODEX", &result.turn_id)
        {
            let _ = self
                .store
                .fail_provider_run_by_id(&run.id, "IDENTITY_ATTACHMENT_ERROR");
            return Err(error);
        }
        self.store.mark_provider_run_reviewed(&retained.id)?;
        Ok(result)
    }






    pub(crate) fn respond_mobile_codex_request(
        &self,
        workstream_id: &str,
        action_id: &str,
        input: MobileCodexResponseInput,
    ) -> Result<(), String> {
        let request = {
            let session = self
                .session
                .lock()
                .map_err(|_| "Router session is unavailable")?;
            let request = session
                .pending_codex_requests
                .get(action_id)
                .ok_or("That Codex structured request is no longer live")?;
            if request.revision != input.revision || request.responded {
                return Err("That Codex structured request is stale".into());
            }
            request.clone()
        };
        let active = self
            .store
            .active_endpoint_for_workstream(
                &self
                    .store
                    .active_endpoint_for_external_id("CODEX", &request.thread_id)?
                    .ok_or("The structured request thread is not an ACTIVE Router endpoint")?
                    .workstream_id,
                Provider::Codex,
            )?
            .ok_or("The structured request no longer belongs to an ACTIVE Codex endpoint")?;
        if active.external_id != request.thread_id || active.workstream_id != workstream_id {
            return Err("The structured request endpoint changed before response".into());
        }
        let result = server_request_response_result(&request, input)?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        let current = session
            .pending_codex_requests
            .get(action_id)
            .ok_or("That Codex structured request is no longer live")?;
        if current.raw_request_id != request.raw_request_id
            || current.thread_id != request.thread_id
            || current.responded
        {
            return Err("The structured request changed before response".into());
        }
        let adapter=session.adapter.as_mut().ok_or("Codex backend disconnected. The live request cannot be reconstructed.")?;
        if adapter.is_shared()&&!adapter.is_shared_subscribed(&request.thread_id)&&!adapter.is_exact_turn_active(&request.thread_id,&request.turn_id){return Err("That request belongs to a turn controlled in Codex Desktop.".into());}
        adapter.respond_to_server_request(&request.raw_request_id, result)?;
        if let Some(current) = session.pending_codex_requests.get_mut(action_id) {
            current.responded = true;
            current.revision += 1;
        }
        // The server is authoritative for the final lifecycle. Keep the item
        // visible until its exact serverRequest/resolved notification arrives.
        Ok(())
    }











}

/// Creates the durable projection only after the same Router-owned provider
/// surface has established the exact endpoint and listener completion rules.
pub(crate) fn record_provider_surface_reply_with(
    store: &RouterStore,
    endpoint: &Endpoint,
    assistant_identity: &str,
    text: &str,
    push: &mut dyn FnMut(&[u8]) -> Result<crate::push::PushDeliveryOutcome, String>,
) -> Result<bool, String> {
    record_provider_surface_reply_with_completion(store,endpoint,assistant_identity,text,
        (endpoint.provider=="CHATGPT").then_some(None),push)
}

/// Persist a proven terminal source and its outbox before any mobile transport.
/// None means an unmarked projection; Some(None) is an exact native terminal
/// notification whose completion timestamp is not supplied by the provider.
fn record_provider_surface_reply_with_completion(
    store:&RouterStore,endpoint:&Endpoint,assistant_identity:&str,text:&str,
    completion:Option<Option<i64>>,
    push:&mut dyn FnMut(&[u8])->Result<crate::push::PushDeliveryOutcome,String>,
)->Result<bool,String>{
    let Some(observation) = store.record_reply_observation(
        &endpoint.workstream_id,
        &endpoint.id,
        Some(assistant_identity),
        text,
        None,
    )?
    else {
        if let Some(at)=completion{store.note_reply_completion(&endpoint.workstream_id,&endpoint.id,assistant_identity,at)?;}
        return Ok(false);
    };
    if let Some(at)=completion{store.note_reply_completion(&endpoint.workstream_id,&endpoint.id,assistant_identity,at)?;}
    attempt_reply_push_with(store, endpoint, &observation, push)?;
    Ok(true)
}

pub(crate) fn attempt_reply_push_with(
    store: &RouterStore,
    endpoint: &Endpoint,
    observation: &ReplyObservation,
    push: &mut dyn FnMut(&[u8]) -> Result<crate::push::PushDeliveryOutcome, String>,
) -> Result<(), String> {
    if !store.endpoint_notification_enabled(&endpoint.id)? {return Ok(());}
    let workstream_name = store.workstream_name(&endpoint.workstream_id)?;
    let payload = serde_json::json!({"type":if endpoint.provider == "CODEX" { "codex_reply" } else { "chatgpt_reply" },"workstreamId":endpoint.workstream_id,"observationId":observation.id,"workstreamName":workstream_name,"conversationName":endpoint.label});
    let (push_state, invalid_fingerprints) = match push(payload.to_string().as_bytes()) {
        Ok(crate::push::PushDeliveryOutcome::Suppressed)=>{store.record_reply_push_attempt(&observation.id,"SKIPPED")?;return Ok(());},
        Ok(crate::push::PushDeliveryOutcome::Sent {
            invalid_subscription_fingerprints,
        }) => ("SENT", invalid_subscription_fingerprints),
        Ok(crate::push::PushDeliveryOutcome::NoSubscription) => ("NO_SUBSCRIPTION", vec![]),
        Ok(crate::push::PushDeliveryOutcome::Failed {
            invalid_subscription_fingerprints,
            ..
        }) => ("FAILED", invalid_subscription_fingerprints),
        Err(_) => ("FAILED", vec![]),
    };
    store.record_reply_push_attempt(&observation.id, push_state)?;
    store.record_push_subscription_attempt(push_state)?;
    for fingerprint in invalid_fingerprints {
        store.deactivate_push_subscription_metadata(&fingerprint)?;
    }
    Ok(())
}



pub(crate) fn ensure_retained_result_is_active(
    run: &persistence::ProviderRun,
    active: &Endpoint,
) -> Result<(), String> {
    if run.endpoint_id != active.id {
        return Err("This result belongs to a superseded Endpoint; reopen or resolve the continuation before acting on it".into());
    }
    Ok(())
}


pub(crate) fn abbreviated_identity(value: &str) -> String {
    let mut characters = value.chars();
    let prefix = characters.by_ref().take(8).collect::<String>();
    if characters.next().is_none() {
        prefix
    } else {
        format!("{prefix}…")
    }
}

pub(crate) fn command_mobile_decisions(params: &Value) -> Vec<&'static str> {
    let offered = params.get("availableDecisions").and_then(Value::as_array);
    match offered {
        Some(values) => ["accept", "decline", "cancel"]
            .into_iter()
            .filter(|decision| values.iter().any(|value| value.as_str() == Some(*decision)))
            .collect(),
        // The official optional field is absent on legacy command approvals.
        None => vec!["accept", "decline", "cancel"],
    }
}

pub(crate) fn mobile_codex_request_projection(
    request: &PendingCodexRequest,
) -> Option<MobileCodexRequest> {
    if request.turn_id.trim().is_empty()
        || matches!(
            request.method.as_str(),
            "item/commandExecution/requestApproval"
                | "item/fileChange/requestApproval"
                | "item/permissions/requestApproval"
                | "item/tool/requestUserInput"
        ) && request.item_id.as_deref().is_none_or(str::is_empty)
    {
        return None;
    }
    let params = &request.params;
    let (kind, choices, questions, is_blocking) = match request.method.as_str() {
        "item/commandExecution/requestApproval" => {
            let choices = command_mobile_decisions(params)
                .into_iter()
                .map(mobile_choice)
                .collect();
            ("COMMAND_APPROVAL", choices, vec![], false)
        }
        "item/fileChange/requestApproval" => (
            "FILE_CHANGE_APPROVAL",
            ["accept", "decline", "cancel"]
                .into_iter()
                .map(mobile_choice)
                .collect(),
            vec![],
            false,
        ),
        "item/permissions/requestApproval" => (
            "PERMISSIONS_APPROVAL",
            vec![mobile_choice("accept"), mobile_choice("decline")],
            vec![],
            false,
        ),
        "item/tool/requestUserInput" => {
            let questions = params
                .get("questions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|question| {
                    let id = question.get("id")?.as_str()?.to_string();
                    let label = question
                        .get("header")
                        .or_else(|| question.get("question"))
                        .and_then(Value::as_str)
                        .unwrap_or("Codex needs your answer")
                        .to_string();
                    Some(MobileCodexQuestion {
                        id,
                        label,
                        placeholder: question
                            .get("question")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        required: true,
                        options: question.get("options").and_then(Value::as_array).into_iter().flatten().filter_map(|o|Some(MobileCodexInputOption{label:o.get("label")?.as_str()?.into(),description:o.get("description").and_then(Value::as_str).unwrap_or("").into()})).collect(),
                        is_other: question.get("isOther").and_then(Value::as_bool).unwrap_or(false),
                        is_secret: question.get("isSecret").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect();
            (
                "USER_INPUT",
                vec![],
                questions,
                params
                    .get("isBlocking")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )
        }
        _ => return None,
    };
    Some(MobileCodexRequest {
        request_id: request.action_id.clone(),
        revision: request.revision,
        method: request.method.clone(),
        kind: kind.into(),
        reason: params
            .get("reason")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string),
        choices,
        questions,
        is_blocking,
        response_sent: request.responded,
    })
}

pub(crate) fn mobile_choice(id: &str) -> MobileCodexChoice {
    MobileCodexChoice {
        id: id.into(),
        label: match id {
            "accept" => "Approve once",
            "decline" => "Decline",
            "cancel" => "Cancel",
            _ => id,
        }
        .into(),
    }
}

pub(crate) fn codex_request_state_event(message: &Value) -> bool {
    matches!(
        message.get("method").and_then(Value::as_str),
        Some(
            "item/commandExecution/requestApproval"
                | "item/fileChange/requestApproval"
                | "item/permissions/requestApproval"
                | "item/tool/requestUserInput"
                | "serverRequest/resolved"
        )
    )
}

pub(crate) fn capture_pending_codex_request(session: &Arc<Mutex<Session>>, message: &Value) {
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return;
    };
    if !matches!(
        method,
        "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/permissions/requestApproval"
            | "item/tool/requestUserInput"
    ) {
        return;
    }
    let Some(raw_request_id) = message.get("id").cloned() else {
        return;
    };
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let (Some(thread_id), Some(turn_id)) = (
        params.get("threadId").and_then(Value::as_str),
        params.get("turnId").and_then(Value::as_str),
    ) else {
        return;
    };
    let request = PendingCodexRequest {
        action_id: Uuid::new_v4().to_string(),
        raw_request_id,
        method: method.to_string(),
        thread_id: thread_id.to_string(),
        turn_id: turn_id.to_string(),
        item_id: params
            .get("itemId")
            .and_then(Value::as_str)
            .map(str::to_string),
        params,
        revision: 1,
        responded: false,
    };
    if mobile_codex_request_projection(&request).is_some() {
        if let Ok(mut current) = session.lock() {
            if current.resolved_codex_requests.iter().any(|(thread,id)|thread==&request.thread_id&&id==&request.raw_request_id)
                ||current.pending_codex_requests.values().any(|r|r.thread_id==request.thread_id&&r.raw_request_id==request.raw_request_id){return;}
            current
                .pending_codex_requests
                .insert(request.action_id.clone(), request);
        }
    }
}

pub(crate) fn clear_resolved_codex_request(session: &Arc<Mutex<Session>>, message: &Value) {
    if message.get("method").and_then(Value::as_str) != Some("serverRequest/resolved") {
        return;
    }
    let params = message.get("params").unwrap_or(&Value::Null);
    let (Some(thread_id), Some(request_id)) = (
        params.get("threadId").and_then(Value::as_str),
        params.get("requestId"),
    ) else {
        return;
    };
    if let Ok(mut current) = session.lock() {
        current.resolved_codex_requests.push_back((thread_id.into(),request_id.clone()));
        while current.resolved_codex_requests.len()>1024{current.resolved_codex_requests.pop_front();}
        current.pending_codex_requests.retain(|_, request| {
            !(request.thread_id == thread_id && request.raw_request_id == *request_id)
        });
    }
}

pub(crate) fn publish_mcp_request_state(store:&RouterStore,session:&Arc<Mutex<Session>>,message:&Value){
 let Some(thread)=message.pointer("/params/threadId").and_then(Value::as_str)else{return};
 if message["method"]=="serverRequest/resolved"{if let Some(id)=message.pointer("/params/requestId"){let _=store.resolve_mcp_native_request(thread,None,Some(&id.to_string()));}return;}
 let Some(id)=message.get("id")else{return};
 let request=session.lock().ok().and_then(|s|s.pending_codex_requests.values().find(|r|r.thread_id==thread&&r.raw_request_id==*id&&!r.responded).cloned());
 if let Some(r)=request{if let Some(public)=mobile_codex_request_projection(&r){if let Ok(value)=serde_json::to_value(public){let _=store.observe_mcp_native_request(&r.thread_id,&r.turn_id,&r.raw_request_id.to_string(),&value);}}}
}

pub(crate) fn server_request_response_result(
    request: &PendingCodexRequest,
    input: MobileCodexResponseInput,
) -> Result<Value, String> {
    match request.method.as_str() {
        "item/commandExecution/requestApproval" => {
            let decision = input
                .decision
                .as_deref()
                .ok_or("A command decision is required")?;
            if !matches!(decision, "accept" | "decline" | "cancel")
                || !command_mobile_decisions(&request.params).contains(&decision)
            {
                return Err(
                    "That command decision is not offered by the exact Codex request".into(),
                );
            }
            Ok(json!({ "decision": decision }))
        }
        "item/fileChange/requestApproval" => {
            let decision = input
                .decision
                .as_deref()
                .ok_or("A file-change decision is required")?;
            if !matches!(decision, "accept" | "decline" | "cancel") {
                return Err(
                    "File-change decisions are limited to one-time accept, decline, or cancel"
                        .into(),
                );
            }
            Ok(json!({ "decision": decision }))
        }
        "item/permissions/requestApproval" => {
            let decision = input
                .decision
                .as_deref()
                .ok_or("A permissions decision is required")?;
            match decision {
                "accept" => Ok(json!({
                    "permissions": request.params.get("permissions").cloned().unwrap_or_else(|| json!({})),
                    "scope": "turn"
                })),
                "decline" | "cancel" => Ok(json!({ "permissions": {}, "scope": "turn" })),
                _ => Err("Permissions decisions are limited to one-time accept or decline".into()),
            }
        }
        "item/tool/requestUserInput" => {
            if input.decision.as_deref()==Some("skip") {return Ok(json!({"answers":{}}));}
            let submitted = input.answers.ok_or("Codex requested an answer")?;
            let expected = request
                .params
                .get("questions")
                .and_then(Value::as_array)
                .ok_or("Codex user-input request lacks questions")?;
            let mut answers = serde_json::Map::new();
            for question in expected {
                let id = question
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or("Codex user-input question lacks exact ID")?;
                let answer = submitted.get(id).map(String::as_str).unwrap_or("").trim();
                if answer.is_empty() {
                    return Err("Every exact Codex user-input question requires an answer".into());
                }
                answers.insert(id.into(), json!({ "answers": [answer] }));
            }
            if submitted.keys().any(|id| {
                !expected
                    .iter()
                    .any(|question| question.get("id").and_then(Value::as_str) == Some(id.as_str()))
            }) {
                return Err(
                    "A submitted answer does not belong to this exact Codex request".into(),
                );
            }
            Ok(json!({ "answers": answers }))
        }
        _ => Err("Unsupported Codex server request".into()),
    }
}

#[derive(Serialize)]
pub(crate) struct BackendStatus {
    pub(crate) connected: bool,
    pub(crate) connecting: bool,
    pub(crate) detail: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HostEnvironmentStatus {
    pub(crate) host: String,
    pub(crate) mobile: String,
}





/// One exact external Codex reply observed without creating a ProviderRun.
#[derive(Clone)]
pub(crate) struct LatestCodexReply {
    pub(crate) thread: ThreadSummary,
    pub(crate) completed_turn_id: String,
    pub(crate) agent_item_id: String,
    pub(crate) text: String,
    pub(crate) completed_at:Option<i64>,
}

/// A passive read never turns an incomplete or interrupted Codex turn into a
/// reply.  Keeping the terminal classification separate lets both desktop and
/// mobile say what was actually read instead of collapsing it into an
/// "unreadable" error.
pub(crate) struct LatestCodexRead {
    pub(crate) reply: Option<LatestCodexReply>,
    pub(crate) state: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexManualRefreshResult {
    pub(crate) state: String,
    pub(crate) observation_created: bool,
    pub(crate) last_successful_check_at: Option<i64>,
}

#[derive(Serialize, Clone)]
pub(crate) struct ThreadSummary {
    pub(crate) id: String,
    pub(crate) name: Option<String>,
    pub(crate) preview: Option<String>,
}

/// A read-only existing-thread candidate. `id` is retained for exact routing
/// and verification only; presentation leads with the provider's name/preview.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExistingCodexThreadCandidate {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) preview: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) project_provenance: Option<String>,
    pub(crate) recency_at: Option<i64>,
    pub(crate) project_id: Option<String>,
    pub(crate) project_label: Option<String>,
    pub(crate) project_status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExistingCodexThreadCatalog {
    pub(crate) threads: Vec<ExistingCodexThreadCandidate>,
    pub(crate) complete: bool,
}

#[derive(Serialize)]
pub(crate) struct ResumeResult {
    pub(crate) thread: ThreadSummary,
    pub(crate) history: Vec<FeedEvent>,
}

/// Read-only thread inspection. This deliberately has the same data shape as
/// a resumed thread, while never changing selected-thread presentation state
/// or the in-memory writer-readiness registry.
#[derive(Serialize)]
pub(crate) struct ReadHistoryResult {
    pub(crate) thread: ThreadSummary,
    pub(crate) history: Vec<FeedEvent>,
}

#[derive(Serialize)]
pub(crate) struct TurnStartResult {
    pub(crate) turn_id: String,
}




/// A deliberately narrow, debug-build-only observation surface for the V0-006
/// Desktop smoke harness. It is read-only and excludes handoff payloads,
/// transcripts, raw protocol events, credentials, and full external IDs.
#[cfg(debug_assertions)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V0_006SmokeObservation {
    pub(crate) captured_at: u64,
    pub(crate) selected_project_id: Option<String>,
    pub(crate) selected_workstream_id: Option<String>,
    pub(crate) projects: Vec<V0_006SmokeProject>,
    pub(crate) workstreams: Vec<V0_006SmokeWorkstream>,
    pub(crate) attention_items: Vec<V0_006SmokeAttention>,
    pub(crate) selected_handoffs: Vec<V0_006SmokeHandoff>,
    pub(crate) record_counts: persistence::SmokeRecordCounts,
    pub(crate) codex_request_counts: HashMap<String, u64>,
}

#[cfg(debug_assertions)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V0_006SmokeProject {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[cfg(debug_assertions)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V0_006SmokeWorkstream {
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) chatgpt_endpoint: Option<V0_006SmokeEndpoint>,
    pub(crate) codex_endpoint: Option<V0_006SmokeEndpoint>,
    pub(crate) chatgpt_run: Option<V0_006SmokeRun>,
    pub(crate) codex_run: Option<V0_006SmokeRun>,
}

#[cfg(debug_assertions)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V0_006SmokeEndpoint {
    pub(crate) provider: String,
    pub(crate) external_id: String,
}

#[cfg(debug_assertions)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V0_006SmokeRun {
    pub(crate) record_id: String,
    pub(crate) id: String,
    pub(crate) provider: String,
    pub(crate) external_run_id: Option<String>,
    pub(crate) status: String,
    pub(crate) reviewed_at: Option<i64>,
}

#[cfg(debug_assertions)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V0_006SmokeAttention {
    pub(crate) kind: String,
    pub(crate) priority: i32,
    pub(crate) workstream_id: String,
    pub(crate) record_id: String,
    pub(crate) source_id: String,
}

#[cfg(debug_assertions)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V0_006SmokeHandoff {
    pub(crate) record_id: String,
    pub(crate) id: String,
    pub(crate) direction: String,
    pub(crate) status: String,
    pub(crate) source_provider: String,
    pub(crate) source_external_id: String,
    pub(crate) destination_provider: String,
    pub(crate) destination_external_id: String,
    pub(crate) attention_acknowledged: bool,
}

/// Debug-only evidence projection for the V0-008 Desktop gate. It contains
/// record IDs and shortened external identities only, never transcript text,
/// credentials, cookies, or raw provider events.
#[cfg(debug_assertions)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V0_008SmokeObservation {
    pub(crate) workstream_id: String,
    pub(crate) endpoints: Vec<V0_008SmokeEndpoint>,
    pub(crate) provider_runs: Vec<V0_008SmokeProviderRun>,
    pub(crate) handoffs: Vec<V0_008SmokeHandoff>,
}

#[cfg(debug_assertions)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V0_008SmokeEndpoint {
    pub(crate) record_id: String,
    pub(crate) provider: String,
    pub(crate) external_id: String,
    pub(crate) status: String,
    pub(crate) replaces_endpoint_id: Option<String>,
}

#[cfg(debug_assertions)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V0_008SmokeProviderRun {
    pub(crate) record_id: String,
    pub(crate) endpoint_id: String,
    pub(crate) provider: String,
    pub(crate) external_run_id: Option<String>,
    pub(crate) status: String,
    pub(crate) reviewed_at: Option<i64>,
}

#[cfg(debug_assertions)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V0_008SmokeHandoff {
    pub(crate) record_id: String,
    pub(crate) source_endpoint_id: String,
    pub(crate) destination_endpoint_id: String,
    pub(crate) status: String,
}

#[cfg(debug_assertions)]
pub(crate) fn v0_006_short_identity(value: &str) -> String {
    if value.len() <= 14 {
        return value.to_string();
    }
    format!("{}…{}", &value[..8], &value[value.len() - 6..])
}

#[cfg(debug_assertions)]
pub(crate) fn v0_006_endpoint(endpoint: &Endpoint) -> V0_006SmokeEndpoint {
    V0_006SmokeEndpoint {
        provider: endpoint.provider.clone(),
        external_id: v0_006_short_identity(&endpoint.external_id),
    }
}

#[cfg(debug_assertions)]
pub(crate) fn v0_006_run(run: &persistence::ProviderRun) -> V0_006SmokeRun {
    V0_006SmokeRun {
        record_id: run.id.clone(),
        id: v0_006_short_identity(&run.id),
        provider: run.provider.clone(),
        external_run_id: run.external_run_id.as_deref().map(v0_006_short_identity),
        status: run.status.clone(),
        reviewed_at: run.reviewed_at,
    }
}

pub(crate) fn external_project_link_from_input(
    input: ExternalProjectLinkInput,
) -> Result<NewExternalProjectLink, String> {
    let provider = parse_provider(&input.provider)?;
    let (external_project_id, canonical_url) = match provider {
        Provider::ClaudeCode => return Err("CLAUDE_PROJECT_LINK_UNSUPPORTED_USE_EXACT_SESSION_BINDING".into()),
        Provider::Chatgpt => return Err("CHATGPT_REMOVED".into()),
        Provider::Codex => (
            input.external_project_id.unwrap_or_default(),
            input.canonical_url,
        ),
    };
    Ok(NewExternalProjectLink {
        provider,
        external_project_id,
        canonical_url,
        label: input.label,
        source_kind: input.source_kind,
        source_version: input.source_version,
        verified_at: input.verified_at,
    })
}

pub(crate) fn rollover_now() -> Result<i64, String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "System clock predates Unix epoch")?
        .as_secs() as i64)
}

pub(crate) fn backend_status(session: &Session) -> BackendStatus {
    BackendStatus {
        connected: session
            .adapter
            .as_ref()
            .is_some_and(|adapter| !adapter.is_closed()),
        connecting: session.connecting,
        detail: session.connection_detail.clone(),
    }
}

pub(crate) fn connect_codex_in_background(
    sink: Arc<dyn EventSink>,
    store: Arc<RouterStore>,
    session: Arc<Mutex<Session>>,
) {
    // ChatGPT exact-ID recovery has no dependency on Codex availability.
    // It remains independently backgrounded, so either provider cannot stall
    // initial interaction with a Workspace.
    let chatgpt_store = Arc::clone(&store);
    std::thread::spawn(move || {
        let _ = RuntimeReconciler::reconcile_chatgpt(&chatgpt_store);
    });

    let listener_store = Arc::clone(&store);
    let listener_session = Arc::clone(&session);
    let listener: Arc<dyn Fn(&Value) + Send + Sync> = Arc::new(move |message| {
        // The adapter invokes this callback on its sole stdout-reader thread.
        // A first approved relay intentionally owns `Session` while it proves
        // exact thread readiness.  Never let an unrelated server request wait
        // on that lock here: doing so prevents this reader from consuming the
        // response that releases the relay's own `thread/read` request.
        // Streaming deltas and lifecycle events do not modify pending request
        // state. Filter before cloning a payload or creating an OS thread.
        if codex_request_state_event(message) {
            let pending_session = Arc::clone(&listener_session);
            let pending_message = message.clone();
            let pending_store=listener_store.clone();
            std::thread::spawn(move || {
                capture_pending_codex_request(&pending_session, &pending_message);
                clear_resolved_codex_request(&pending_session, &pending_message);
                publish_mcp_request_state(&pending_store,&pending_session,&pending_message);
            });
        }
        if message.get("method").and_then(Value::as_str) != Some("turn/completed") {
            return;
        }
        let Some(turn_id) = message.pointer("/params/turn/id").and_then(Value::as_str) else {
            return;
        };
        let thread_id = message
            .pointer("/params/threadId")
            .or_else(|| message.pointer("/params/turn/threadId"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let status = match message
            .pointer("/params/turn/status")
            .and_then(Value::as_str)
        {
            Some("completed") => "COMPLETED",
            Some("interrupted") => "CANCELLED",
            Some("failed") => "FAILED",
            _ => "UNKNOWN",
        };
        if status!="UNKNOWN"{if let Some(thread)=thread_id.clone(){let request_store=listener_store.clone();let request_turn=turn_id.to_string();std::thread::spawn(move||{let _=request_store.resolve_mcp_native_request(&thread,Some(&request_turn),None);});}}
        if status != "COMPLETED" {
            let _ = listener_store.complete_provider_run("CODEX", turn_id, status, Some(status));
            return;
        }
        let (Some(thread_id), turn_id) = (thread_id, turn_id.to_string()) else {
            return;
        };
        let (item_id, text) =
            match exact_codex_completed_event_result(&thread_id, &turn_id, message) {
                Ok(result) => result,
                Err(_) => {
                    let _ = listener_store.complete_provider_run(
                        "CODEX",
                        &turn_id,
                        "UNKNOWN",
                        Some("CODEX_RESULT_UNOBSERVED"),
                    );
                    return;
                }
            };
        let store = Arc::clone(&listener_store);
        std::thread::spawn(move || {
            let _ = capture_completed_codex_result(&store, &thread_id, &turn_id, item_id, text);
        });
    });

    let result = (|| -> Result<u64, String> {
        let mut adapter = CodexAdapter::start(Arc::clone(&sink), listener)?;
        adapter.initialize()?;
        RuntimeReconciler::reconcile_codex(&store, &mut adapter)?;
        let mut current = session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        current.adapter = Some(adapter);
        current.pending_codex_requests.clear();
        current.resolved_codex_requests.clear();
        current.codex_observer_epoch = current.codex_observer_epoch.wrapping_add(1);
        current.connecting = false;
        current.connection_detail = None;
        Ok(current.codex_observer_epoch)
    })();

    match result {
        Ok(observer_epoch) => {
            let observer_store = Arc::clone(&store);
            let observer_session = Arc::clone(&session);
            std::thread::spawn(move || {
                run_codex_existing_thread_observer(observer_store, observer_session, observer_epoch)
            });
            sink.connected();
        }
        Err(error) => {
            let _ = RuntimeReconciler::provider_unavailable(
                &store,
                "CODEX",
                "CODEX_ADAPTER_UNAVAILABLE",
            );
            if let Ok(mut current) = session.lock() {
                current.connecting = false;
                current.connection_detail = Some(error.clone());
            }
            sink.connection_failed(&error);
        }
    }
}

/// The resident Host owns the Codex app-server connection needed by the
/// read-only mobile controls.  Starting this connection never starts or
/// resumes a Codex turn; it only makes the existing exact-thread reader
/// available without requiring someone to first open the desktop dashboard.
pub(crate) fn connect_codex_for_resident_host(sink: Arc<dyn EventSink>, core: &RouterCore) {
    let should_connect = match core.session.lock() {
        Ok(mut session) => {
            if session
                .adapter
                .as_ref()
                .is_some_and(CodexAdapter::is_closed)
            {
                session.adapter.take();
                session.ready_threads.clear();
            }
            if session.adapter.is_some() || session.connecting || session.codex_adapter_borrowed {
                false
            } else {
                session.ready_threads.clear();
                session.connecting = true;
                session.connection_detail = Some("Connecting to Codex…".to_string());
                true
            }
        }
        Err(_) => false,
    };
    if should_connect {
        let store = Arc::clone(&core.store);
        let session = Arc::clone(&core.session);
        std::thread::spawn(move || connect_codex_in_background(sink, store, session));
    }
}

pub(crate) fn read_thread_history_blocking(
    thread_id: String,
    session: Arc<Mutex<Session>>,
) -> Result<ReadHistoryResult, String> {
    let mut session = session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    let adapter = session
        .adapter
        .as_mut()
        .ok_or("Codex history is unavailable because the backend is disconnected.")?;
    let latest = read_latest_codex_reply(adapter, &thread_id)?;
    Ok(ReadHistoryResult {
        thread: latest
            .reply
            .as_ref()
            .map(|reply| reply.thread.clone())
            .unwrap_or_else(|| ThreadSummary {
                id: thread_id,
                name: None,
                preview: None,
            }),
        history: latest
            .reply
            .map(|reply| {
                vec![FeedEvent {
                    id: format!(
                        "codex-observed:{}:{}",
                        reply.completed_turn_id, reply.agent_item_id
                    ),
                    kind: FeedKind::AgentMessage,
                    method: "thread/items/list".into(),
                    item_id: Some(reply.agent_item_id),
                    thread_id: Some(reply.thread.id),
                    turn_id: Some(reply.completed_turn_id),
                    text: Some(reply.text),
                    detail: Some("EXTERNAL_EXISTING_THREAD".into()),
                    raw: None,
                }]
            })
            .unwrap_or_default(),
    })
}

/// The only normal observation route for an existing Codex thread. It first
/// proves metadata identity, then uses the adapter's single latest-turn permit
/// and one bounded item page. It never resumes, starts, interrupts, or
/// controls a Goal.
pub(crate) fn read_latest_codex_reply(
    adapter: &mut CodexAdapter,
    thread_id: &str,
) -> Result<LatestCodexRead, String> {
    let metadata = adapter.request_with_timeout(
        "thread/read",
        json!({ "threadId": thread_id, "includeTurns": false }),
        CODEX_OBSERVER_REQUEST_TIMEOUT,
    )?;
    let thread = exact_read_history(thread_id, &metadata)?.thread;
    adapter.begin_latest_turn_observation(thread_id)?;
    let turns = match adapter.request_with_timeout(
        "thread/turns/list",
        json!({
            "threadId": thread_id, "cursor": Value::Null, "limit": 1,
            "sortDirection": "desc", "itemsView": "notLoaded"
        }),
        CODEX_OBSERVER_REQUEST_TIMEOUT,
    ) {
        Ok(turns) => turns,
        Err(error) if is_unmaterialized_empty_codex_thread(thread_id, &error) => {
            // The exact thread/read above proved identity. Official new threads
            // have no turn storage until the first user message; this is an
            // empty baseline, never permission to resume or start a turn.
            return Ok(LatestCodexRead { reply: None, state: "NO_NEW_TERMINAL_REPLY" });
        }
        Err(error) => return Err(error),
    };
    let rows = turns
        .get("data")
        .and_then(Value::as_array)
        .ok_or("CODEX_OBSERVATION_PROTOCOL_INVALID")?;
    if rows.len() > 1 {
        return Err("CODEX_OBSERVATION_PROTOCOL_INVALID".into());
    }
    let Some(turn) = rows.first() else {
        return Ok(LatestCodexRead {
            reply: None,
            state: "NO_NEW_TERMINAL_REPLY",
        });
    };
    let status = turn
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if status != "completed" {
        return Ok(LatestCodexRead {
            reply: None,
            state: match status {
                "interrupted" => "LATEST_TURN_INTERRUPTED",
                "inProgress" | "active" | "pending" => "LATEST_TURN_ACTIVE",
                _ => "NO_NEW_TERMINAL_REPLY",
            },
        });
    }
    let turn_id = turn
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .ok_or("CODEX_OBSERVATION_PROTOCOL_INVALID")?
        .to_string();
    if turn.get("itemsView").and_then(Value::as_str) != Some("notLoaded") {
        return Ok(LatestCodexRead {
            reply: None,
            state: "OBSERVATION_MATERIALIZATION_PENDING",
        });
    }
    // Request only the newest bounded item page. A `full` turn response can
    // embed an arbitrarily large user transcript even when the Router needs
    // only its latest terminal answer.
    adapter.begin_latest_turn_item_fallback(thread_id, &turn_id)?;
    let items = adapter.request_with_timeout(
        "thread/items/list",
        json!({
            "threadId": thread_id, "turnId": turn_id, "cursor": Value::Null,
            "limit": 20, "sortDirection": "desc"
        }),
        CODEX_OBSERVER_REQUEST_TIMEOUT,
    )?;
    let item_rows = items
        .get("data")
        .and_then(Value::as_array)
        .ok_or("CODEX_OBSERVATION_PROTOCOL_INVALID")?;
    // The newest bounded page may legitimately advertise older items.  Normal
    // observation intentionally does not follow that cursor: its authority is
    // only the final answer found in this one exact, newest completed turn.
    if item_rows.len() > 20 {
        return Err("CODEX_OBSERVATION_PROTOCOL_INVALID".into());
    }
    let result = item_rows
        .iter()
        .filter_map(|row| {
            if row.get("turnId").and_then(Value::as_str) != Some(turn_id.as_str()) {
                return None;
            }
            row.get("item")
        })
        .collect::<Vec<_>>();
    let result = final_agent_message_from_items(result);
    Ok(match result {
        Some((agent_item_id, text)) => LatestCodexRead {
            reply: Some(LatestCodexReply {
                thread,
                completed_turn_id: turn_id,
                agent_item_id,
                text,
                completed_at:turn.get("completedAt").and_then(Value::as_i64).filter(|v|*v>0).and_then(|v|v.checked_mul(1000)).filter(|v|*v<=253402300799000),
            }),
            state: "NO_NEW_TERMINAL_REPLY",
        },
        None => LatestCodexRead {
            reply: None,
            state: "OBSERVATION_MATERIALIZATION_PENDING",
        },
    })
}

pub(crate) fn is_unmaterialized_empty_codex_thread(thread_id: &str, error: &str) -> bool {
    router_core::codex::adapter::is_unmaterialized_empty_thread(thread_id,error)
}

/// Starts a Codex thread only through the official app-server and verifies the
/// exact returned thread object before exposing it as a session-only candidate.
/// Current app-server responses already carry that fresh thread, while a
/// redundant full `thread/read` asks its unsupported `list_turns` route. This
/// obtains no write readiness and changes no Router Endpoint.
pub(crate) fn start_verified_codex_thread(
    adapter: &mut CodexAdapter,
    cwd: &std::path::Path,
) -> Result<ThreadSummary, String> {
    let result = adapter.request("thread/start", json!({ "cwd": cwd, "ephemeral": false }))?;
    let thread = result
        .get("thread")
        .ok_or("Codex thread/start response did not include thread")?;
    thread_summary(thread)
}

/// The current app-server presents persisted Desktop threads as `not loaded`
/// until the one exact metadata-only resume completes. Do not attempt a full
/// history read first: it cannot establish identity on that server generation.
/// The metadata read and resume responses must each repeat the persisted
/// thread identity before the bounded history projection is accepted.
pub(crate) fn exact_not_loaded_metadata_error(error: &str, expected_thread_id: &str) -> bool {
    error.contains(&format!(
        "\"message\":\"thread not loaded: {expected_thread_id}\""
    ))
}

pub(crate) fn resume_exact_thread(
    adapter: &mut CodexAdapter,
    thread_id: &str,
) -> Result<ReadHistoryResult, String> {
    match adapter.request(
        "thread/read",
        json!({ "threadId": thread_id, "includeTurns": false }),
    ) {
        Ok(metadata) => {
            if metadata.pointer("/thread/id").and_then(Value::as_str) != Some(thread_id) {
                return Err("Codex metadata read returned a different thread identity".into());
            }
        }
        Err(error) if exact_not_loaded_metadata_error(&error, thread_id) => {}
        Err(error) => return Err(error),
    }
    let resumed = adapter.request(
        "thread/resume",
        json!({ "threadId": thread_id, "excludeTurns": true }),
    )?;
    let resumed_thread = resumed
        .get("thread")
        .ok_or("Codex thread/resume response did not include thread")?;
    let summary = thread_summary(resumed_thread)?;
    if summary.id != thread_id {
        return Err("Codex thread/resume returned a different thread identity".into());
    }
    Ok(ReadHistoryResult {
        thread: summary,
        history: history_events(resumed_thread),
    })
}

pub(crate) fn unprojected_thread_directory(
    requested: Option<String>,
) -> Result<std::path::PathBuf, String> {
    match requested.filter(|value| !value.trim().is_empty()) {
        Some(value) => {
            let path = std::path::PathBuf::from(value.trim());
            if !path.is_absolute() || !path.is_dir() {
                return Err(
                    "An explicit no-project directory must be an existing absolute directory"
                        .into(),
                );
            }
            Ok(path)
        }
        None => {
            let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
            let path = std::path::PathBuf::from(local)
                .join("AIWorkRouter")
                .join("scratch")
                .join(Uuid::new_v4().to_string());
            fs::create_dir_all(&path)
                .map_err(|error| format!("Could not create Router scratch directory: {error}"))?;
            Ok(path)
        }
    }
}


/// Reconciles exactly one active Codex endpoint. The first readable completed
/// turn becomes a durable baseline and intentionally produces no historic
/// notification. Later exact turn/item identities create one observation.
pub(crate) fn check_new_codex_replies_with(
    store:&RouterStore,session:&Arc<Mutex<Session>>,workstream_id:&str,push:&mut dyn FnMut(&[u8])->Result<crate::push::PushDeliveryOutcome,String>
)->Result<CodexManualRefreshResult,String>{
 let endpoints=store.snapshot_for_workstream(workstream_id)?.endpoint_lineage.into_iter().filter(|e|e.provider=="CODEX"&&e.status=="ACTIVE").collect::<Vec<_>>();
 let mut result=None;
 for endpoint in endpoints {result=Some(check_new_codex_endpoint_replies_with(store,session,workstream_id,endpoint,push)?);}
 result.ok_or("The Workstream has no ACTIVE Codex Endpoint".into())
}
pub(crate) fn check_new_codex_endpoint_replies_with(
    store:&RouterStore,session:&Arc<Mutex<Session>>,workstream_id:&str,endpoint:Endpoint,push:&mut dyn FnMut(&[u8])->Result<crate::push::PushDeliveryOutcome,String>
)->Result<CodexManualRefreshResult,String>{
    if !store.endpoint_notification_enabled(&endpoint.id)? {return Err("BRIDGE_NOT_ACTIVE".into());}
    let latest = {
        let mut current = session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        let adapter = current
            .adapter
            .as_mut()
            .ok_or("Codex observer is unavailable because the backend is disconnected.")?;
        if adapter.is_closed() {
            return Err(
                "Codex observer is unavailable because the backend is disconnected.".into(),
            );
        }
        read_latest_codex_reply(adapter, &endpoint.external_id)?
    };
    let state = latest.state;
    let Some(reply) = latest.reply else {
        if !store.codex_reply_observer_is_initialized(&endpoint.id)? {
            // A newly bound thread that is actively running has no completed
            // answer to inherit.  Record that fact now, so its first terminal
            // answer is treated as a future reply and can notify the owner.
            // A materializing completed turn remains deliberately unmarked:
            // it might still be historic content whose exact watermark has
            // not become readable yet.
            if should_initialize_empty_codex_baseline(state) {
                store.save_codex_reply_observer_empty_baseline(
                    workstream_id,
                    &endpoint.id,
                    &endpoint.external_id,
                )?;
            }
        }
        return Ok(CodexManualRefreshResult {
            state: state.into(),
            observation_created: false,
            last_successful_check_at: None,
        });
    };
    let completion_identity=format!("codex:{}:{}",reply.completed_turn_id,reply.agent_item_id);
    store.note_reply_completion(workstream_id,&endpoint.id,&completion_identity,reply.completed_at)?;
    let watermark = store.codex_reply_observer_watermark(&endpoint.id)?;
    let initialized = store.codex_reply_observer_is_initialized(&endpoint.id)?;
    let watermark_identity = format!(
        "codex-watermark:{}:{}:{}",
        endpoint.external_id, reply.completed_turn_id, reply.agent_item_id
    );
    let same = watermark
        .as_ref()
        .is_some_and(|(identity, _)| identity == &watermark_identity);
    if watermark.is_none() && !initialized {
        store.save_codex_reply_observer_watermark(
            workstream_id,
            &endpoint.id,
            &endpoint.external_id,
            &reply.completed_turn_id,
            &reply.agent_item_id,
        )?;
        let saved = store.codex_reply_observer_watermark(&endpoint.id)?;
        return Ok(CodexManualRefreshResult {
            state: "NO_NEW_TERMINAL_REPLY".into(),
            observation_created: false,
            last_successful_check_at: saved.map(|(_, seen_at)| seen_at),
        });
    }
    if same {
        store.save_codex_reply_observer_watermark(
            workstream_id,
            &endpoint.id,
            &endpoint.external_id,
            &reply.completed_turn_id,
            &reply.agent_item_id,
        )?;
        let saved = store.codex_reply_observer_watermark(&endpoint.id)?;
        return Ok(CodexManualRefreshResult {
            state: "NO_NEW_TERMINAL_REPLY".into(),
            observation_created: false,
            last_successful_check_at: saved.map(|(_, seen_at)| seen_at),
        });
    }
    let identity = format!("codex:{}:{}", reply.completed_turn_id, reply.agent_item_id);
    let created =
        record_provider_surface_reply_with(store, &endpoint, &identity, &reply.text, push)?;
    store.note_reply_completion(workstream_id,&endpoint.id,&identity,reply.completed_at)?;
    // Persist the new exact identity even if the independently idempotent
    // insert found it after a restart/retry. A text comparison never decides
    // deduplication.
    store.save_codex_reply_observer_watermark(
        workstream_id,
        &endpoint.id,
        &endpoint.external_id,
        &reply.completed_turn_id,
        &reply.agent_item_id,
    )?;
    let saved = store.codex_reply_observer_watermark(&endpoint.id)?;
    Ok(CodexManualRefreshResult {
        state: if created {
            "NEW_REPLY_OBSERVED"
        } else {
            "NO_NEW_TERMINAL_REPLY"
        }
        .into(),
        observation_created: created,
        last_successful_check_at: saved.map(|(_, seen_at)| seen_at),
    })
}

pub(crate) fn should_initialize_empty_codex_baseline(state: &str) -> bool {
    matches!(state, "NO_NEW_TERMINAL_REPLY" | "LATEST_TURN_ACTIVE")
}

/// Host-resident, single-flight reconciliation. It observes only current
/// active Codex bindings, sleeps between passes, and exits with its adapter;
/// it never creates a writer lease or a ProviderRun.
pub(crate) fn run_codex_existing_thread_observer(
    store: Arc<RouterStore>,
    session: Arc<Mutex<Session>>,
    observer_epoch: u64,
) {
    let mut activity_cursor=0;
    let mut watch_cursor=None;
    let mut notice_cursor=None;
    'observer: loop {
        match resident_observer_connection(&session,observer_epoch){
            ObserverConnection::Stopped=>return,
            ObserverConnection::Borrowed=>{std::thread::sleep(Duration::from_millis(200));continue;}
            ObserverConnection::Ready=>{}
        }
        crate::codex_notices::poll(&store,&session,observer_epoch,&mut notice_cursor);
        crate::role_bridge::refresh_resident_activity(&store,&session,observer_epoch,&mut activity_cursor);
        let active_workstream_ids = match active_initialized_codex_observer_workstreams(&store) {
            Ok(workstream_ids) => workstream_ids,
            Err(_) => return,
        };
        match resident_observer_connection(&session,observer_epoch){ObserverConnection::Stopped=>return,ObserverConnection::Borrowed=>continue,ObserverConnection::Ready=>{}}
        if let Ok(watches) = store.codex_watches() {
            let now=rollover_now().unwrap_or(0)*1000;
            let due:Vec<_>=watches.into_iter().filter(|w|w.enabled&&!w.thread_id.starts_with("pi:")&&!w.thread_id.starts_with(router_core::store::claude_manual::PREFIX)&&w.retry_after<=now).collect();
            let selected=crate::observer_batch::next_ids(due.iter().map(|w|w.thread_id.as_str()),&mut watch_cursor,crate::observer_batch::WATCH_READS_PER_PASS);
            for watch in due.into_iter().filter(|w|selected.contains(&w.thread_id)) {
                // Pause/Resume generation is checked again when the native read commits.
                let _ = crate::codex_watch::poll(&store,&session,&watch,observer_epoch);
            }
        }
        for workstream_id in active_workstream_ids {
            match resident_observer_connection(&session,observer_epoch){ObserverConnection::Stopped=>return,ObserverConnection::Borrowed=>continue 'observer,ObserverConnection::Ready=>{}}
            let _ =
                check_new_codex_replies_with(&store, &session, &workstream_id, &mut |payload| {
                    crate::push::send_payload(payload)
                });
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

enum ObserverConnection{Ready,Borrowed,Stopped}
fn resident_observer_connection(session:&Mutex<Session>,epoch:u64)->ObserverConnection{
    let Ok(s)=session.lock()else{return ObserverConnection::Stopped};
    if s.codex_observer_epoch!=epoch{return ObserverConnection::Stopped;}
    if s.adapter.as_ref().is_some_and(|a|!a.is_closed()){return ObserverConnection::Ready;}
    if s.adapter.is_none()&&s.codex_adapter_borrowed{return ObserverConnection::Borrowed;}
    ObserverConnection::Stopped
}

/// Existing historical ACTIVE endpoints may remain in SQLite forever for audit
/// purposes.  They must not starve the resident observer: only an endpoint
/// whose first exact baseline/watermark was deliberately established belongs
/// to the automatic polling fallback.  Fresh terminal events are handled
/// immediately by the app-server listener below, before this fallback runs.
pub(crate) fn active_initialized_codex_observer_workstreams(
    store: &RouterStore,
) -> Result<Vec<String>, String> {
    let mut workstream_ids = HashSet::new();
    for endpoint in store.active_workstreams()?.into_iter()
        .map(|w|store.snapshot_for_workstream(&w.id).map(|s|s.endpoint_lineage))
        .collect::<Result<Vec<_>,_>>()?.into_iter().flatten()
        .filter(|endpoint| endpoint.provider == "CODEX" && endpoint.status == "ACTIVE")
    {
        if store.endpoint_notification_enabled(&endpoint.id)? && store.codex_reply_observer_is_initialized(&endpoint.id)? {
            workstream_ids.insert(endpoint.workstream_id);
        }
    }
    Ok(workstream_ids.into_iter().collect())
}






pub(crate) fn restore_codex_adapter(
    core: &RouterCore,
    adapter: CodexAdapter,
    ready_thread_id: Option<&str>,
) -> Result<(), String> {
    let mut session = core
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    if let Some(thread_id) = ready_thread_id {
        session.ready_threads.insert(thread_id.to_string());
    }
    session.adapter = Some(adapter);
    session.codex_adapter_borrowed=false;
    Ok(())
}



pub(crate) fn persisted_attachment(
    attachment: &crate::relay_model::HandoffAttachment,
) -> NewAttachment {
    let size = fs::metadata(&attachment.path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len() as i64);
    NewAttachment {
        id: uuid::Uuid::new_v4().to_string(),
        filename: attachment.filename.clone(),
        original_path: attachment.path.clone(),
        size,
        sha256: attachment.actual_sha256.clone(),
        integrity_status: attachment.integrity_status.clone(),
    }
}

/// Acquires write ownership only for an explicit user-approved write. Navigation
/// and dashboard projection never call this method.
pub(crate) fn ensure_thread_ready_for_write(
    session: &mut Session,
    thread_id: &str,
) -> Result<(), String> {
    let needs_write_acquisition = thread_needs_write_acquisition(&session.ready_threads, thread_id);
    let adapter = session
        .adapter
        .as_mut()
        .ok_or("Codex backend disconnected. Use Reconnect first.")?;
    ensure_adapter_thread_ready_for_write(adapter, needs_write_acquisition, thread_id)?;
    if needs_write_acquisition {
        session.ready_threads.insert(thread_id.to_string());
    }
    Ok(())
}

pub(crate) fn read_codex_goal_from_session(
    session: &mut Session,
    thread_id: &str,
) -> Result<Option<MobileCodexGoal>, String> {
    let adapter = session
        .adapter
        .as_mut()
        .ok_or("Codex Goal is unavailable because the backend is disconnected.")?;
    if adapter.is_closed() {
        return Err("Codex Goal is unavailable because the backend is disconnected.".into());
    }
    let response = adapter.get_goal(thread_id)?;
    let mut goal = codex_goal_from_response(thread_id, &response)?;
    if let Some(goal) = goal.as_mut() {
        goal.active_turn_id = adapter.active_turn_id(thread_id);
    }
    Ok(goal)
}

pub(crate) fn require_goal_confirmation(confirmed: bool) -> Result<(), String> {
    if confirmed {
        Ok(())
    } else {
        Err("Confirm this Goal action before continuing.".into())
    }
}

pub(crate) fn ensure_goal_transition(
    current_status: &str,
    target_status: &str,
) -> Result<(), String> {
    match (current_status, target_status) {
        ("active", "paused") | ("paused", "active") => Ok(()),
        ("active", "active") | ("paused", "paused") => {
            Err("The Codex Goal is already in that state.".into())
        }
        (_, "paused") => Err("Only an active Codex Goal can be paused.".into()),
        (_, "active") => Err("Only a paused Codex Goal can be resumed.".into()),
        _ => Err("Unsupported Codex Goal state transition.".into()),
    }
}

pub(crate) fn codex_goal_from_response(
    expected_thread_id: &str,
    response: &Value,
) -> Result<Option<MobileCodexGoal>, String> {
    let goal = response.get("goal").unwrap_or(response);
    if goal.is_null() {
        return Ok(None);
    }
    let actual_thread_id = goal
        .get("threadId")
        .and_then(Value::as_str)
        .ok_or("Codex Goal response did not include a thread ID")?;
    if actual_thread_id != expected_thread_id {
        return Err("Codex Goal response belongs to a different thread identity.".into());
    }
    let objective = goal
        .get("objective")
        .and_then(Value::as_str)
        .ok_or("Codex Goal response did not include an objective")?
        .to_string();
    let status = goal
        .get("status")
        .and_then(Value::as_str)
        .ok_or("Codex Goal response did not include a status")?
        .to_string();
    let mut projected=MobileCodexGoal {
        fingerprint:String::new(),
        thread_id: actual_thread_id.to_string(),
        objective,
        status,
        token_budget: optional_goal_integer(goal, "tokenBudget")?,
        tokens_used: optional_goal_integer(goal, "tokensUsed")?,
        time_used_seconds: optional_goal_integer(goal, "timeUsedSeconds")?,
        created_at: goal.get("createdAt").cloned(),
        updated_at: goal.get("updatedAt").cloned(),
        active_turn_id: None,
    };
    projected.fingerprint=crate::goal_control::fingerprint(&projected);
    Ok(Some(projected))
}

pub(crate) fn optional_goal_integer(goal: &Value, field: &str) -> Result<Option<i64>, String> {
    let Some(value) = goal.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .map(Some)
        .ok_or_else(|| format!("Codex Goal {field} is not an integer"))
}

pub(crate) fn ensure_adapter_thread_ready_for_write(
    adapter: &mut CodexAdapter,
    needs_write_acquisition: bool,
    thread_id: &str,
) -> Result<(), String> {
    if !needs_write_acquisition {
        return Ok(());
    }
    let metadata = adapter.request_with_timeout(
        "thread/read",
        json!({ "threadId": thread_id, "includeTurns": false }),
        WRITE_READINESS_TIMEOUT,
    );
    match metadata {
        Ok(metadata) => {
            if metadata.pointer("/thread/id").and_then(Value::as_str) != Some(thread_id) {
                return Err("Persisted Codex binding returned a different thread identity.".into());
            }
            // On the shared backend, subscribe only after proving an already
            // loaded thread's Goal cannot auto-run. Resume has no competing
            // writer acquisition here and establishes this client's event feed.
            if adapter.is_shared() && metadata.pointer("/thread/status/type").and_then(Value::as_str).is_some_and(|s|s!="notLoaded"){
                let value=adapter.get_goal(thread_id)?;
                if !matches!(value.get("goal"),Some(Value::Null)){
                    let goal=value.get("goal").ok_or("SHARED_TARGET_GOAL_UNVERIFIED")?;
                    if goal.get("threadId").and_then(Value::as_str)!=Some(thread_id)||!matches!(goal.get("status").and_then(Value::as_str),Some("paused"|"complete"|"completed")){return Err("SHARED_TARGET_GOAL_NOT_IDLE".into());}
                }
            }
        }
        Err(error) if exact_not_loaded_metadata_error(&error, thread_id) => {}
        Err(error) => return Err(thread_write_readiness_error(error)),
    }
    let resumed = adapter
        .request_with_timeout(
            "thread/resume",
            json!({ "threadId": thread_id, "excludeTurns": true }),
            WRITE_READINESS_TIMEOUT,
        )
        .map_err(thread_write_readiness_error)?;
    if resumed.pointer("/thread/id").and_then(Value::as_str) != Some(thread_id) {
        return Err("Persisted Codex binding resumed a different thread identity.".into());
    }
    Ok(())
}

pub(crate) fn thread_needs_write_acquisition(
    ready_threads: &HashSet<String>,
    thread_id: &str,
) -> bool {
    !ready_threads.contains(thread_id)
}

pub(crate) fn thread_write_readiness_error(error: String) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("thread not found") || lower.contains("no rollout found") {
        "Persisted Codex binding is no longer readable. Replace the binding manually.".into()
    } else if lower.contains("writer")
        || lower.contains("owned by another")
        || lower.contains("conflict")
    {
        "Codex thread is currently owned by another application. Close/release it there, then retry.".into()
    } else {
        error
    }
}

pub(crate) fn start_turn(
    adapter: &mut CodexAdapter,
    thread_id: &str,
    text: &str,
) -> Result<TurnStartResult, String> {
    if text.trim().is_empty() {
        return Err("Cannot send an empty Codex turn".to_string());
    }
    let result = adapter.request(
        "turn/start",
        json!({ "threadId": thread_id, "input": [{ "type": "text", "text": text }] }),
    )?;
    let turn_id = result
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .ok_or("Codex turn/start response did not include turn id")?;
    adapter.mark_turn_started(thread_id, turn_id);
    Ok(TurnStartResult {
        turn_id: turn_id.to_string(),
    })
}



/// Stages a complete, already-reviewed attachment set atomically inside the
/// exact Codex workspace.  A rejected or changed later attachment must not
/// leave an earlier attachment visible to the target thread.
pub(crate) fn stage_reverse_attachment_files(
    workspace: &std::path::Path,
    handoff_id: &str,
    attachments: &mut [crate::relay_model::HandoffAttachment],
) -> Result<(), String> {
    if !workspace.is_dir()
        || handoff_id.trim().is_empty()
        || std::path::Path::new(handoff_id).components().count() != 1
    {
        return Err("Codex attachment workspace is unavailable".into());
    }
    let incoming = workspace.join(".aiwr").join("incoming");
    let target_root = incoming.join(handoff_id);
    if target_root.exists() {
        return Err("Codex attachment destination already exists".into());
    }

    // Validate every source before creating a visible per-Handoff directory.
    // The second hash check after copy closes the time-of-check/time-of-use
    // window between review and staging.
    let mut names = HashSet::new();
    let mut verified = Vec::with_capacity(attachments.len());
    for attachment in attachments.iter() {
        let source = std::path::Path::new(&attachment.path);
        let filename = std::path::Path::new(&attachment.filename);
        if source.file_name() != filename.file_name()
            || filename.components().count() != 1
            || !names.insert(attachment.filename.clone())
        {
            return Err("Invalid or duplicate reverse attachment filename".into());
        }
        let expected = attachment
            .actual_sha256
            .as_deref()
            .ok_or("Reverse attachment lacks a reviewed SHA-256")?;
        let actual = sha256_path(source)?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err("Reverse attachment changed after review".into());
        }
        verified.push((
            source.to_path_buf(),
            attachment.filename.clone(),
            expected.to_string(),
        ));
    }

    fs::create_dir_all(&incoming).map_err(|_| "Could not create Codex attachment workspace")?;
    let staging = incoming.join(format!(".{handoff_id}.staging-{}", Uuid::new_v4()));
    fs::create_dir(&staging).map_err(|_| "Could not create Codex attachment workspace")?;
    for (source, filename, expected) in &verified {
        let target = staging.join(filename);
        let copied = (|| -> Result<(), String> {
            fs::copy(source, &target).map_err(|_| "Could not stage reverse attachment")?;
            let copied = sha256_path(&target)?;
            if !copied.eq_ignore_ascii_case(expected) {
                return Err("Reverse attachment copy integrity check failed".into());
            }
            Ok(())
        })();
        if let Err(error) = copied {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    }
    if let Err(error) = fs::rename(&staging, &target_root) {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!(
            "Could not finalize Codex attachment workspace: {error}"
        ));
    }
    for attachment in attachments {
        attachment.path = target_root.join(&attachment.filename).display().to_string();
    }
    Ok(())
}

pub(crate) fn sha256_path(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read;
    let mut file = fs::File::open(path).map_err(|_| "Could not read reverse attachment")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "Could not read reverse attachment")?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Converts a Codex `thread/read` response into UI history only after its
/// stable identity matches exactly. Keeping this pure makes it impossible for
/// the Open path to gain writer ownership as a side effect of mapping data.
pub(crate) fn exact_read_history(
    thread_id: &str,
    read: &Value,
) -> Result<ReadHistoryResult, String> {
    let read_thread = read
        .get("thread")
        .ok_or("Codex thread/read response did not include thread")?;
    let summary = thread_summary(read_thread)?;
    if summary.id != thread_id {
        return Err("Codex thread/read returned a different thread identity".to_string());
    }
    Ok(ReadHistoryResult {
        thread: summary,
        history: history_events(read_thread),
    })
}

pub(crate) fn exact_codex_completed_event_result(
    thread_id: &str,
    turn_id: &str,
    event: &Value,
) -> Result<(String, String), String> {
    if event.get("method").and_then(Value::as_str) != Some("turn/completed") {
        return Err("Codex event is not a completed turn".into());
    }
    if event.pointer("/params/threadId").and_then(Value::as_str) != Some(thread_id) {
        return Err("Codex completed event belongs to a different thread".into());
    }
    let turn = event
        .pointer("/params/turn")
        .ok_or("Codex completed event has no turn")?;
    if turn.get("id").and_then(Value::as_str) != Some(turn_id) {
        return Err("Codex completed event belongs to a different turn".into());
    }
    if turn.get("status").and_then(Value::as_str) != Some("completed") {
        return Err("The exact Codex event is not terminal completed".into());
    }
    turn.get("items")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().rev().find_map(|item| {
                (item.get("type").and_then(Value::as_str) == Some("agentMessage"))
                    .then(|| {
                        Some((
                            item.get("id").and_then(Value::as_str)?.to_string(),
                            item.get("text").and_then(Value::as_str)?.to_string(),
                        ))
                    })
                    .flatten()
            })
        })
        .filter(|(_, text)| !text.trim().is_empty())
        .ok_or("The exact completed Codex event has no complete agent result".to_string())
}

pub(crate) fn capture_completed_codex_result(
    store: &RouterStore,
    thread_id: &str,
    turn_id: &str,
    item_id: String,
    text: String,
) -> Result<(), String> {
    capture_completed_codex_result_with_push(
        store,
        thread_id,
        turn_id,
        item_id,
        text,
        &mut |payload| crate::push::send_payload(payload),
    )?;
    Ok(())
}

/// A `turn/completed` event is the lowest-latency, exact-identity observation
/// path for an already-bound Codex thread.  It is read-only: the listener
/// never resumes a thread, creates a ProviderRun, or acquires a writer lease.
/// The durable observation insert de-duplicates a later polling fallback.
pub(crate) fn capture_completed_codex_result_with_push(
    store: &RouterStore,
    thread_id: &str,
    turn_id: &str,
    item_id: String,
    text: String,
    push: &mut dyn FnMut(&[u8]) -> Result<crate::push::PushDeliveryOutcome, String>,
) -> Result<(), String> {
    let endpoint = store
        .active_endpoint_for_external_id("CODEX", thread_id)?
        .ok_or("Completed Codex thread is not an ACTIVE Router endpoint")?;
    let runs = store.provider_runs_for_workstream(&endpoint.workstream_id)?;
    if let Some(run) = runs
        .into_iter()
        .find(|run| run.provider == "CODEX" && run.external_run_id.as_deref() == Some(turn_id))
    {
        store.accept_completed_provider_result(&run.id, turn_id, &item_id, text.clone())?;
    }
    let identity = format!("codex:{turn_id}:{item_id}");
    let _ = record_provider_surface_reply_with_completion(store, &endpoint, &identity, &text, Some(None),push)?;
    store.save_codex_reply_observer_watermark(
        &endpoint.workstream_id,
        &endpoint.id,
        &endpoint.external_id,
        turn_id,
        &item_id,
    )?;
    Ok(())
}

/// The final replacement can rely only on the exact candidate thread and the
/// exact initialization turn. A completed turn without its own observed agent
/// result is not enough to create a reviewed ProviderRun.
pub(crate) fn verified_rollover_result_identity(
    thread_id: &str,
    turn_id: &str,
    read: &Value,
) -> Result<String, String> {
    exact_read_history(thread_id, read)?;
    let turn = read
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .and_then(|turns| {
            turns
                .iter()
                .find(|turn| turn.get("id").and_then(Value::as_str) == Some(turn_id))
        })
        .ok_or("thread/read did not contain the exact initialization turn")?;
    if turn.get("status").and_then(Value::as_str) != Some("completed") {
        return Err("The exact initialization turn is not completed by thread/read".into());
    }
    turn.get("items")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().rev().find(|item| {
                item.get("type").and_then(Value::as_str) == Some("agentMessage")
                    && item.get("id").and_then(Value::as_str).is_some()
            })
        })
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or("The exact initialization turn has no observed agent result identity".into())
}




pub(crate) fn thread_summary(thread: &Value) -> Result<ThreadSummary, String> {
    let id = thread
        .get("id")
        .and_then(Value::as_str)
        .ok_or("Codex response did not include stable thread id")?;
    Ok(ThreadSummary {
        id: id.to_string(),
        name: thread
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        preview: thread
            .get("preview")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

pub(crate) fn existing_codex_thread_candidate(
    thread: &Value,
) -> Result<ExistingCodexThreadCandidate, String> {
    let id = thread
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or("Codex response did not include stable thread id")?
        .to_string();
    let name = thread
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let preview = thread
        .get("preview")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let updated_at = thread
        .get("updatedAt")
        .or_else(|| thread.get("createdAt"))
        .and_then(|value| match value {
            Value::String(value) => Some(value.to_string()),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        });
    // A canonical `projectId` is native provenance. Do not expose or invent a
    // filesystem directory and do not treat its absence as a binding failure.
    let project_provenance = thread
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(|_| "已核验原生 Codex 项目归属".to_string());
    Ok(ExistingCodexThreadCandidate {
        id,
        label: name
            .or_else(|| preview.clone())
            .unwrap_or_else(|| "未命名 Codex 对话".to_string()),
        preview,
        updated_at,
        project_provenance,
        recency_at: thread.get("recencyAt").and_then(Value::as_i64).filter(|n|*n>=0),
        project_id: None,
        project_label: None,
        project_status: "UNCONFIRMED".into(),
    })
}

pub(crate) fn parse_provider(provider: &str) -> Result<Provider, String> {
    match provider {
        "CHATGPT" => Ok(Provider::Chatgpt),
        "CODEX" => Ok(Provider::Codex),
        _ => Err("Endpoint provider must be CHATGPT or CODEX".to_string()),
    }
}

pub(crate) fn controlled_host_acceptance_requested() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::args().any(|argument| {
            matches!(
                argument.as_str(),
                "--host-lifecycle-acceptance"
                    | "--host-quit-acceptance"
                    | "--host-autostart-acceptance"
            )
        })
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

#[cfg(debug_assertions)]
pub(crate) fn host_lifecycle_acceptance_requested() -> bool {
    std::env::args().any(|argument| argument == "--host-lifecycle-acceptance")
}

#[cfg(debug_assertions)]
pub(crate) fn host_quit_acceptance_requested() -> bool {
    std::env::args().any(|argument| argument == "--host-quit-acceptance")
}

#[cfg(debug_assertions)]
pub(crate) fn host_autostart_acceptance_requested() -> bool {
    std::env::args().any(|argument| argument == "--host-autostart-acceptance")
}


pub(crate) fn final_agent_message_from_items<'a>(
    items: impl IntoIterator<Item = &'a Value>,
) -> Option<(String, String)> {
    items.into_iter().find_map(|item| {
        (item.get("type").and_then(Value::as_str) == Some("agentMessage")
            && item.get("phase").and_then(Value::as_str) == Some("final_answer"))
        .then(|| {
            let id = item.get("id").and_then(Value::as_str)?;
            let text = item.get("text").and_then(Value::as_str)?;
            (!id.is_empty()
                && id.len() <= 256
                && !text.trim().is_empty()
                && text.len() <= 1_000_000)
                .then(|| (id.to_string(), text.to_string()))
        })
        .flatten()
    })
}



#[cfg(debug_assertions)]
pub(crate) fn service_v0_008_smoke_observation(
    workstream_id: String,
    state: &RouterState,
) -> Result<V0_008SmokeObservation, String> {
    let snapshot = state.store.snapshot()?;
    let endpoints = snapshot
        .endpoint_lineage
        .into_iter()
        .filter(|endpoint| endpoint.workstream_id == workstream_id)
        .map(|endpoint| V0_008SmokeEndpoint {
            record_id: endpoint.id,
            provider: endpoint.provider,
            external_id: v0_006_short_identity(&endpoint.external_id),
            status: endpoint.status,
            replaces_endpoint_id: endpoint.replaces_endpoint_id,
        })
        .collect();
    let provider_runs = state
        .store
        .provider_runs_for_workstream(&workstream_id)?
        .into_iter()
        .map(|run| V0_008SmokeProviderRun {
            record_id: run.id,
            endpoint_id: run.endpoint_id,
            provider: run.provider,
            external_run_id: run.external_run_id.as_deref().map(v0_006_short_identity),
            status: run.status,
            reviewed_at: run.reviewed_at,
        })
        .collect();
    let handoffs = snapshot
        .handoffs
        .into_iter()
        .filter(|handoff| handoff.workstream_id == workstream_id)
        .map(|handoff| {
            let source_endpoint_id = handoff.endpoint_source()?.id.clone();
            Ok(V0_008SmokeHandoff {
                record_id: handoff.id,
                source_endpoint_id,
                destination_endpoint_id: handoff.destination_endpoint.id,
                status: handoff.status,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(V0_008SmokeObservation {
        workstream_id,
        endpoints,
        provider_runs,
        handoffs,
    })
}

#[cfg(debug_assertions)]
pub(crate) fn service_v0_006_smoke_observation(
    state: &RouterState,
) -> Result<V0_006SmokeObservation, String> {
    let snapshot = state.store.snapshot()?;
    let projection = state.store.dashboard_projection()?;
    let captured_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "System clock predates Unix epoch")?
        .as_secs();

    let codex_request_counts = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?
        .adapter
        .as_ref()
        .map(CodexAdapter::request_counts)
        .unwrap_or_default();

    Ok(V0_006SmokeObservation {
        captured_at,
        selected_project_id: snapshot.selected_project_id,
        selected_workstream_id: snapshot.selected_workstream_id,
        projects: snapshot
            .projects
            .into_iter()
            .map(|project| V0_006SmokeProject {
                id: project.id,
                name: project.name,
            })
            .collect(),
        workstreams: projection
            .workstreams
            .into_iter()
            .map(|workstream| V0_006SmokeWorkstream {
                project_id: workstream.project_id,
                project_name: workstream.project_name,
                id: workstream.workstream.id,
                name: workstream.workstream.name,
                chatgpt_endpoint: workstream.chatgpt_endpoint.as_ref().map(v0_006_endpoint),
                codex_endpoint: workstream.codex_endpoint.as_ref().map(v0_006_endpoint),
                chatgpt_run: workstream.chatgpt_run.as_ref().map(v0_006_run),
                codex_run: workstream.codex_run.as_ref().map(v0_006_run),
            })
            .collect(),
        attention_items: projection
            .attention_items
            .into_iter()
            .map(|attention| V0_006SmokeAttention {
                kind: attention.kind,
                priority: attention.priority,
                workstream_id: attention.workstream_id,
                record_id: attention.source_id.clone(),
                source_id: v0_006_short_identity(&attention.source_id),
            })
            .collect(),
        selected_handoffs: snapshot
            .handoffs
            .into_iter()
            .map(|handoff| {
                let source = handoff.endpoint_source()?.clone();
                Ok(V0_006SmokeHandoff {
                    record_id: handoff.id.clone(),
                    id: v0_006_short_identity(&handoff.id),
                    direction: handoff.direction,
                    status: handoff.status,
                    source_provider: source.provider,
                    source_external_id: v0_006_short_identity(&source.external_id),
                    destination_provider: handoff.destination_endpoint.provider,
                    destination_external_id: v0_006_short_identity(
                        &handoff.destination_endpoint.external_id,
                    ),
                    attention_acknowledged: handoff.attention_acknowledged_at.is_some(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        record_counts: state.store.smoke_record_counts()?,
        codex_request_counts,
    })
}

pub(crate) fn service_pair_workstream_endpoints(
    workstream_id: String,
    input: EndpointPairingInput,
    state: &RouterState,
) -> Result<persistence::EndpointPairingResult, String> {
    if input.chatgpt.is_some() {
        return Err("CHATGPT_REMOVED".into());
    }
    let side = |value: EndpointPairingSideInput| EndpointPairingSide {
        expected_active_endpoint_id: value.expected_active_endpoint_id,
        external_id: value.external_id,
        label: value.label,
    };
    state.store.pair_workstream_endpoints_checked(
        &workstream_id,
        EndpointPairingRequest {
            expected_binding_revision: input.expected_binding_revision,
            chatgpt: input.chatgpt.map(side),
            codex: input.codex.map(side),
        },
    )
}

pub(crate) fn service_create_verified_local_backup(
    state: &RouterState,
) -> Result<persistence::VerifiedBackup, String> {
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
    let backup_directory = std::path::PathBuf::from(local)
        .join("AIWorkRouter")
        .join("backups");
    fs::create_dir_all(&backup_directory)
        .map_err(|error| format!("Could not create Router backup directory: {error}"))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Could not timestamp Router backup: {error}"))?
        .as_secs();
    let target = backup_directory.join(format!("router-v3-{timestamp}-{}.db", Uuid::new_v4()));
    state.store.create_verified_backup(target)
}

pub(crate) fn service_bind_workspace_endpoint(
    workstream_id: String,
    provider: String,
    external_id: String,
    label: String,
    replace: bool,
    state: &RouterState,
) -> Result<Endpoint, String> {
    let provider = parse_provider(&provider)?;
    if matches!(provider, Provider::Chatgpt) {
        return Err("CHATGPT_REMOVED".into());
    }
    state
        .store
        .bind_endpoint(&workstream_id, provider, external_id, label, replace)
}


pub(crate) fn service_begin_codex_rollover(
    workstream_id: String,
    state: &RouterState,
) -> Result<RolloverCandidate, String> {
    let old = state
        .store
        .active_endpoint_for_workstream(&workstream_id, Provider::Codex)?
        .ok_or("This Workstream has no ACTIVE Codex Endpoint to continue")?;
    let mut session = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    let adapter = session
        .adapter
        .as_mut()
        .ok_or("Codex backend disconnected. Use Reconnect first.")?;
    let cwd = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Could not determine AI Work Router project directory")?;
    let result = adapter.request("thread/start", json!({ "cwd": cwd, "ephemeral": false }))?;
    let successor = thread_summary(
        result
            .get("thread")
            .ok_or("Codex thread/start response did not include thread")?,
    )?;
    if successor.id == old.external_id {
        return Err("Codex successor must have a new exact thread identity".into());
    }
    let candidate = RolloverCandidate {
        workstream_id: workstream_id.clone(),
        provider: "CODEX".into(),
        expected_old_endpoint_id: old.id,
        expected_old_external_id: old.external_id,
        candidate_external_id: successor.id,
        label: successor.name.unwrap_or_else(|| "Codex successor".into()),
        state: "CREATED".into(),
        initialization_text: None,
        initialization_turn_id: None,
        initialization_result_identity: None,
        initialization_started_at: None,
        initialization_terminal_at: None,
        error: None,
    };
    session
        .rollover_candidates
        .insert(workstream_id, candidate.clone());
    Ok(candidate)
}

pub(crate) fn service_initialize_codex_rollover(
    workstream_id: String,
    text: String,
    state: &RouterState,
) -> Result<RolloverCandidate, String> {
    if text.trim().is_empty() {
        return Err(
            "Write the visible initialization message before initializing the successor".into(),
        );
    }
    let mut session = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    let candidate = session
        .rollover_candidates
        .get(&workstream_id)
        .cloned()
        .ok_or("No session-only Codex rollover candidate exists. Start again.")?;
    if candidate.provider != "CODEX" || candidate.state != "CREATED" {
        return Err("This Codex successor is not eligible for initialization".into());
    }
    let adapter = session
        .adapter
        .as_mut()
        .ok_or("Codex backend disconnected. Use Reconnect first.")?;
    let started = start_turn(adapter, &candidate.candidate_external_id, &text)?;
    let updated = RolloverCandidate {
        state: "INITIALIZING".into(),
        initialization_text: Some(text),
        initialization_turn_id: Some(started.turn_id),
        initialization_started_at: Some(rollover_now()?),
        ..candidate
    };
    session
        .rollover_candidates
        .insert(workstream_id, updated.clone());
    Ok(updated)
}

pub(crate) fn service_verify_codex_rollover(
    workstream_id: String,
    state: &RouterState,
) -> Result<RolloverCandidate, String> {
    let mut session = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    let candidate = session
        .rollover_candidates
        .get(&workstream_id)
        .cloned()
        .ok_or("No session-only Codex rollover candidate exists. Start again.")?;
    let turn_id = candidate
        .initialization_turn_id
        .clone()
        .ok_or("Initialize the successor before verification")?;
    let adapter = session
        .adapter
        .as_mut()
        .ok_or("Codex backend disconnected. Use Reconnect first.")?;
    let read = adapter.request(
        "thread/read",
        json!({ "threadId": candidate.candidate_external_id, "includeTurns": true }),
    )?;
    let _history = exact_read_history(&candidate.candidate_external_id, &read)?;
    let updated = match verified_rollover_result_identity(
        &candidate.candidate_external_id,
        &turn_id,
        &read,
    ) {
        Ok(result_identity) => RolloverCandidate {
            state: "READY_TO_REPLACE".into(),
            initialization_result_identity: Some(result_identity),
            initialization_terminal_at: Some(rollover_now()?),
            ..candidate
        },
        Err(error) => RolloverCandidate {
            state: "FAILED".into(),
            error: Some(format!("{error} The old Endpoint remains active.")),
            ..candidate
        },
    };
    session
        .rollover_candidates
        .insert(workstream_id, updated.clone());
    Ok(updated)
}

pub(crate) fn service_confirm_rollover(
    workstream_id: String,
    state: &RouterState,
) -> Result<Endpoint, String> {
    let candidate = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?
        .rollover_candidates
        .get(&workstream_id)
        .cloned()
        .ok_or("No session-only rollover candidate exists. Start again.")?;
    if candidate.state != "READY_TO_REPLACE" {
        return Err(
            "Verify the successor and reopen the final replacement review before confirming".into(),
        );
    }
    let run = if candidate.provider == "CODEX" {
        Some(CompletedRolloverRun {
            external_run_id: candidate
                .initialization_turn_id
                .clone()
                .ok_or("Codex successor has no exact initialization turn")?,
            result_identity: candidate.initialization_result_identity.clone(),
            started_at: candidate
                .initialization_started_at
                .ok_or("Codex initialization start time is unavailable")?,
            terminal_at: candidate
                .initialization_terminal_at
                .ok_or("Codex initialization terminal time is unavailable")?,
        })
    } else {
        None
    };
    let endpoint = state.store.replace_active_endpoint_checked(
        &workstream_id,
        parse_provider(&candidate.provider)?,
        &candidate.expected_old_endpoint_id,
        candidate.candidate_external_id,
        candidate.label,
        run,
    )?;
    state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?
        .rollover_candidates
        .remove(&workstream_id);
    Ok(endpoint)
}

pub(crate) fn service_cancel_rollover(
    workstream_id: String,
    state: &RouterState,
) -> Result<(), String> {
    state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?
        .rollover_candidates
        .remove(&workstream_id);
    Ok(())
}

pub(crate) fn service_create_thread(state: &RouterState) -> Result<ThreadSummary, String> {
    let mut session = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    let adapter = session
        .adapter
        .as_mut()
        .ok_or("Codex backend disconnected. Use Reconnect first.")?;
    let cwd = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Could not determine AI Work Router project directory")?;
    let summary = start_verified_codex_thread(adapter, cwd)?;
    session.ready_threads.insert(summary.id.clone());
    session.selected_thread_id = Some(summary.id.clone());
    Ok(summary)
}

pub(crate) fn service_start_unprojected_codex_thread(
    directory: Option<String>,
    state: &RouterState,
) -> Result<UnprojectedThreadStart, String> {
    let directory = unprojected_thread_directory(directory)?;
    let mut session = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    let adapter = session
        .adapter
        .as_mut()
        .ok_or("Codex backend disconnected. Use Reconnect first.")?;
    let summary = start_verified_codex_thread(adapter, &directory)?;
    Ok(UnprojectedThreadStart {
        thread: summary,
        directory: directory.to_string_lossy().to_string(),
    })
}

pub(crate) fn service_list_existing_codex_threads(
    state: &RouterState,
) -> Result<ExistingCodexThreadCatalog, String> {
    let mut session = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    let adapter = session
        .adapter
        .as_mut()
        .ok_or("Codex backend disconnected. Use Reconnect first.")?;
    let mut cursor: Option<String> = None;
    let mut seen = HashSet::new();
    let mut threads = Vec::new();
    let mut seen_cursors = HashSet::new();
    for _ in 0..CODEX_THREAD_CATALOG_MAX_PAGES {
        let params = json!({"cursor":cursor,"limit":100});
        let page = adapter.request("thread/list", params)?;
        let data = page
            .get("data")
            .and_then(Value::as_array)
            .ok_or("Codex thread/list response did not include a data array")?;
        for thread in data {
            let candidate = existing_codex_thread_candidate(thread)?;
            if seen.insert(candidate.id.clone()) {
                threads.push(candidate);
                if threads.len() > CODEX_THREAD_CATALOG_MAX_THREADS {
                    return Err("Codex thread catalog exceeded the safe binding limit; no partial catalog was shown".into());
                }
            }
        }
        cursor = page
            .get("nextCursor")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if cursor.is_none() {
            if page.get("complete")==Some(&Value::Bool(false)) {
                return Err("Codex catalog reported an incomplete terminal page; no partial catalog was shown".into());
            }
            apply_desktop_thread_projects(&mut threads,adapter.read_desktop_catalog().ok().as_ref());
            return Ok(ExistingCodexThreadCatalog {
                threads,
                complete: true,
            });
        }
        if !seen_cursors.insert(cursor.clone()) {return Err("Codex catalog repeated a page cursor; no partial catalog was shown".into());}
    }
    Err("Codex thread catalog exceeded the safe page limit; no partial catalog was shown".into())
}

pub(crate) fn apply_desktop_thread_projects(threads:&mut [ExistingCodexThreadCandidate],desktop:Option<&router_core::codex::desktop_catalog::DesktopCatalog>){
 use router_core::codex::desktop_catalog::Membership;
 for t in threads {
  t.project_id=None;t.project_label=None;t.project_status="UNCONFIRMED".into();t.project_provenance=None;
  match desktop.and_then(|d|d.memberships.get(&t.id)) {
   Some(Membership::Project(id))=>if let Some(project)=desktop.and_then(|d|d.projects.get(id)) {t.project_id=Some(id.clone());t.project_label=Some(if project.name.trim().is_empty(){"未命名项目".into()}else{project.name.clone()});t.project_provenance=t.project_label.clone();t.project_status="PROJECT".into();},
   Some(Membership::Projectless)=>{t.project_label=Some("无项目".into());t.project_status="PROJECTLESS".into();},
   _=>{},
  }
 }
}

pub(crate) fn service_verify_existing_codex_thread(
    thread_id: String,
    state: &RouterState,
) -> Result<ExistingCodexThreadCandidate, String> {
    let expected = thread_id.trim();
    if expected.is_empty() {
        return Err("Codex thread ID is required for exact verification".into());
    }
    let mut session = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    let adapter = session
        .adapter
        .as_mut()
        .ok_or("Codex backend disconnected. Use Reconnect first.")?;
    let result = adapter.request(
        "thread/read",
        json!({ "threadId": expected, "includeTurns": false }),
    )?;
    let thread = result
        .get("thread")
        .ok_or("Codex thread/read response did not include thread")?;
    let candidate = existing_codex_thread_candidate(thread)?;
    if candidate.id != expected {
        return Err("Codex thread/read returned a different thread identity".into());
    }
    Ok(candidate)
}

pub(crate) fn service_resume_thread(
    thread_id: String,
    state: &RouterState,
) -> Result<ResumeResult, String> {
    let mut session = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    let adapter = session
        .adapter
        .as_mut()
        .ok_or("Codex backend disconnected. Use Reconnect first.")?;
    let ReadHistoryResult {
        thread: summary,
        history,
    } = resume_exact_thread(adapter, &thread_id)?;
    session.ready_threads.insert(summary.id.clone());
    session.selected_thread_id = Some(summary.id.clone());
    Ok(ResumeResult {
        thread: summary,
        history,
    })
}

pub(crate) fn service_send_turn(
    thread_id: String,
    text: String,
    state: &RouterState,
) -> Result<TurnStartResult, String> {
    state.store.require_no_watch_reply_writer(&thread_id)?;
    if text.trim().is_empty() {
        return Err("Cannot send an empty Codex turn".to_string());
    }
    let mut session = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    if session
        .adapter
        .as_ref()
        .ok_or("Codex backend disconnected. Use Reconnect first.")?
        .is_turn_active(&thread_id)
    {
        return Err("Codex is already processing a turn for this thread.".to_string());
    }
    ensure_thread_ready_for_write(&mut session, &thread_id)?;
    let adapter = session
        .adapter
        .as_mut()
        .ok_or("Codex backend disconnected. Use Reconnect first.")?;
    let result = start_turn(adapter, &thread_id, &text)?;
    if let Some(endpoint) = state
        .store
        .active_endpoint_for_external_id("CODEX", &thread_id)?
    {
        state.store.create_provider_run(
            &endpoint.workstream_id,
            &endpoint.id,
            "CODEX",
            None,
            Some(&result.turn_id),
            "RUNNING",
        )?;
    }
    Ok(result)
}

pub(crate) fn service_send_workstream_codex_feedback(
    workstream_id: String,
    expected_endpoint_id: String,
    result_run_id: String,
    text: String,
    state: &RouterState,
) -> Result<TurnStartResult, String> {
    let endpoint = state
        .store
        .active_endpoint_for_workstream(&workstream_id, Provider::Codex)?
        .ok_or("The Workstream has no ACTIVE Codex Endpoint")?;
    if endpoint.id != expected_endpoint_id {
        return Err(
            "The current Codex main thread changed; reopen the result before sending feedback"
                .into(),
        );
    }
    state.mobile_codex_feedback(
        &workstream_id,
        MobileFeedbackInput {
            run_id: result_run_id,
            feedback: text,
        },
    )
}







pub(crate) const CODEX_THREAD_CATALOG_MAX_PAGES: usize = 40;

pub(crate) const CODEX_THREAD_CATALOG_MAX_THREADS: usize = 1000;

pub(crate) fn connect_codex_service(
    state: &RouterCore,
    sink: Arc<dyn EventSink>,
) -> Result<BackendStatus, String> {
    let mut session = state
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    if session
        .adapter
        .as_ref()
        .is_some_and(CodexAdapter::is_closed)
    {
        session.adapter.take();
        session.ready_threads.clear();
    }
    if session.adapter.is_some() || session.connecting || session.codex_adapter_borrowed {
        return Ok(backend_status(&session));
    }
    // The native WebView must remain responsive while app-server starts. In
    // particular, initialize() may wait for an external process response, so
    // it must not run in the invoke handler or while this mutex is held.
    session.ready_threads.clear();
    session.connecting = true;
    session.connection_detail = Some("Connecting to Codex…".to_string());
    let status = backend_status(&session);
    drop(session);

    let session = Arc::clone(&state.session);
    let store = Arc::clone(&state.store);
    std::thread::spawn(move || connect_codex_in_background(sink, store, session));
    Ok(status)
}
