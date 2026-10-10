//! Router Host application/session services and presentation DTOs.
//! No Tauri handles, windows, IPC or browser implementation are authority here.
use crate::{
    artifact, chatgpt, chatgpt_direct_writer, codex, host::HostRuntime,
    persistence, runtime_reconciler,
};
#[cfg(test)]
use crate::normal_chrome_extension;
use artifact::detector::{detect, IntegrityStatus};
use chatgpt::direct::{parse_explicit_conversation_url, CompletedChatGptResponse};
use chatgpt::model::{
    exact_single_handoff_block, BoundConversation, HandoffDraft, HandoffStatus, ReverseHandoffDraft,
};
#[cfg(debug_assertions)]
use chatgpt::model::{relay_candidates, RelayCandidateKind};
use codex::adapter::{CodexAdapter, WRITE_READINESS_TIMEOUT};
use codex::protocol::{history_events, FeedEvent, FeedKind};
use persistence::{
    normalize_chatgpt_project_url, CompletedRolloverRun, Endpoint, EndpointPairingRequest,
    EndpointPairingSide, ExternalProjectLink, MobileChatGptInboundReviewRecord,
    MobileCodexOutboundReviewRecord, NewAttachment, NewExternalProjectLink, NewHandoff, Provider,
    ReplyObservation, RouterStore, WorkspaceSnapshot, Workstream,
};
use router_core::events::EventSink;
use runtime_reconciler::RuntimeReconciler;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{sync::mpsc, thread};
use uuid::Uuid;

// A passive observer must never allow one unavailable exact thread to hold up
// every other bound workstream for the adapter's general-purpose 90 second
// request timeout.  This is deliberately a read-only availability budget;
// it does not affect the separate write-readiness contract.
pub(crate) const CODEX_OBSERVER_REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Default)]
pub(crate) struct Session {
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
    pub(crate) sent_reverse_handoff_keys: HashSet<String>,
    pub(crate) rollover_candidates: HashMap<String, RolloverCandidate>,
    pub(crate) explicit_chatgpt_binding_candidates:
        HashMap<String, ExplicitChatGptBindingCandidate>,
    pub(crate) mobile_reviews: HashMap<String, MobileReverseReview>,
    pub(crate) mobile_outbound_reviews: HashMap<String, MobileCodexOutboundReview>,
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

/// Outcome of the default-browser exact-link affordance. Router never opens a
/// duplicate link when it cannot prove whether a background exact tab exists.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DefaultBrowserOpenResult {
    pub(crate) state: &'static str,
}

#[cfg(test)]
pub(crate) fn default_browser_open_decision(
    visible_exact_tab: bool,
    normal_browser_exact_tab: Result<bool, String>,
) -> Result<&'static str, String> {
    if visible_exact_tab {
        return Ok("ALREADY_OPEN");
    }
    match normal_browser_exact_tab {
        Ok(true) => Ok("ALREADY_OPEN"),
        Ok(false) => Ok("OPEN_REQUESTED"),
        // Do not invoke the default browser blindly. A background exact tab
        // cannot be determined from an omnibox-only UIA pass, and opening a
        // second tab would violate the owner's explicit no-duplicate rule.
        Err(error) if error == "CHATGPT_NORMAL_BROWSER_OBSERVER_UNAVAILABLE" => {
            Ok("CHECK_REQUIRED")
        }
        Err(error) => Err(error),
    }
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

/// An exact ChatGPT binding remains transient until the user separately
/// confirms it. Router restart intentionally drops this candidate.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExplicitChatGptBindingCandidate {
    pub(crate) workstream_id: String,
    pub(crate) external_id: String,
    pub(crate) canonical_url: String,
    pub(crate) label: String,
    /// Describes how the exact URL was established for this session-only
    /// candidate. It is disclosure, not an additional routing key.
    pub(crate) verification: String,
    pub(crate) expected_old_endpoint_id: Option<String>,
    pub(crate) expected_binding_revision: i64,
}

/// The durable facts a user reviews before an explicit binding can be
/// confirmed.  Host/browser validation is deliberately outside this lookup;
/// therefore a failed launch cannot create or mutate an Endpoint.
pub(crate) struct ExplicitChatGptBindingReview {
    pub(crate) expected_old_endpoint_id: Option<String>,
    pub(crate) expected_binding_revision: i64,
}

pub(crate) fn review_explicit_chatgpt_binding(
    store: &RouterStore,
    workstream_id: &str,
    external_id: &str,
) -> Result<ExplicitChatGptBindingReview, String> {
    let snapshot = store.snapshot_for_workstream(workstream_id)?;
    let workstream = snapshot
        .workstreams
        .iter()
        .find(|item| item.id == workstream_id)
        .ok_or("The selected Workstream no longer exists")?;
    let current = snapshot.active_chatgpt_endpoint;
    if current
        .as_ref()
        .is_some_and(|endpoint| endpoint.external_id == external_id)
    {
        return Err("This exact ChatGPT conversation is already the ACTIVE Endpoint".into());
    }
    Ok(ExplicitChatGptBindingReview {
        expected_old_endpoint_id: current.map(|endpoint| endpoint.id),
        expected_binding_revision: workstream.binding_revision,
    })
}

/// The only live Router authority. Presentation adapters (Tauri IPC and the
/// loopback HTTP interface) share this value; neither owns a second Store or
/// Session state machine.
#[derive(Clone)]
pub(crate) struct RouterCore {
    pub(crate) chatgpt: Arc<router_core::chatgpt_service::ChatGPTService>,
    pub(crate) session: Arc<Mutex<Session>>,
    /// Router-observed ChatGPT completions are independent of the long-lived
    /// Codex app-server adapter. Keeping this cache out of `Session` prevents
    /// an observational `thread/read` from delaying an already-completed
    /// ChatGPT result or its mobile review path.
    pub(crate) completed_chatgpt_responses: Arc<Mutex<HashMap<String, CompletedChatGptResponse>>>,
    pub(crate) store: Arc<RouterStore>,
}

pub(crate) type RouterState = RouterCore;

// This is deliberately enforced below the presentation layer as well. A
// hidden desktop/mobile button is not an account-safety boundary:
// authenticated HTTP and IPC callers must not be able to revive the retired
// Playwright carrier while the owner has paused ChatGPT browser automation.
pub(crate) const CHATGPT_BROWSER_AUTOMATION_PAUSED_BY_OWNER: bool = true;
pub(crate) const CHATGPT_BROWSER_AUTOMATION_PAUSED_ERROR: &str =
    "CHATGPT_BROWSER_AUTOMATION_PAUSED_BY_OWNER";

pub(crate) fn require_chatgpt_browser_automation_enabled() -> Result<(), String> {
    // This gate owns the retired Playwright carrier only. A connected normal
    // Chrome extension must never make the legacy carrier callable again.
    if CHATGPT_BROWSER_AUTOMATION_PAUSED_BY_OWNER {
        return Err(CHATGPT_BROWSER_AUTOMATION_PAUSED_ERROR.into());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn require_normal_chrome_extension_connected() -> Result<(), String> {
    if normal_chrome_extension::is_connected() {
        Ok(())
    } else {
        Err("CHATGPT_NORMAL_CHROME_CONNECTOR_NOT_CONNECTED".into())
    }
}

#[derive(Clone)]
pub(crate) struct MobileReverseReview {
    pub(crate) workstream_id: String,
    pub(crate) source_run_id: Option<String>,
    pub(crate) source_reference_id: String,
    pub(crate) source_endpoint_id: String,
    pub(crate) source_chatgpt_conversation_id: String,
    pub(crate) destination_codex_thread_id: String,
    pub(crate) source_response_identity: String,
    pub(crate) source_candidate_id: Option<String>,
    pub(crate) original_text: String,
    pub(crate) attachments: Vec<chatgpt::model::HandoffAttachment>,
    pub(crate) attachment_options: Vec<String>,
    pub(crate) approved_text: Option<String>,
    pub(crate) revision: u64,
    pub(crate) status: String,
    pub(crate) handoff_id: Option<String>,
}

#[derive(Clone)]
pub(crate) struct MobileCodexOutboundReview {
    pub(crate) workstream_id: String,
    /// Router execution is optional: existing external Codex replies are
    /// reviewable observations and must not be manufactured into ProviderRuns.
    pub(crate) source_run_id: Option<String>,
    /// The exact ReplyObservation or ProviderRun selected by the owner.
    pub(crate) source_reference_id: String,
    pub(crate) source_endpoint_id: String,
    pub(crate) source_codex_thread_id: String,
    pub(crate) destination_chatgpt_conversation_id: String,
    pub(crate) original_text: String,
    pub(crate) attachments: Vec<chatgpt::model::HandoffAttachment>,
    pub(crate) approved_text: Option<String>,
    pub(crate) revision: u64,
    pub(crate) status: String,
    pub(crate) handoff_id: Option<String>,
}

pub(crate) fn mobile_review_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub(crate) fn mobile_codex_outbound_review_record(
    action_id: &str,
    review: &MobileCodexOutboundReview,
    created_at: i64,
) -> Result<MobileCodexOutboundReviewRecord, String> {
    Ok(MobileCodexOutboundReviewRecord {
        action_id: action_id.into(),
        workstream_id: review.workstream_id.clone(),
        source_run_id: review.source_run_id.clone(),
        source_reference_id: review.source_reference_id.clone(),
        source_endpoint_id: review.source_endpoint_id.clone(),
        source_codex_thread_id: review.source_codex_thread_id.clone(),
        destination_chatgpt_conversation_id: review.destination_chatgpt_conversation_id.clone(),
        original_text: review.original_text.clone(),
        attachments_json: serde_json::to_string(&review.attachments)
            .map_err(|_| "Mobile Codex review attachments could not be serialized")?,
        approved_text: review.approved_text.clone(),
        revision: i64::try_from(review.revision)
            .map_err(|_| "Mobile Codex review revision is too large")?,
        status: review.status.clone(),
        handoff_id: review.handoff_id.clone(),
        created_at,
        updated_at: mobile_review_timestamp(),
    })
}

pub(crate) fn mobile_codex_outbound_review_from_record(
    record: MobileCodexOutboundReviewRecord,
) -> Result<MobileCodexOutboundReview, String> {
    if record.revision < 1 {
        return Err("Persisted mobile ChatGPT Handoff review has an invalid revision".into());
    }
    Ok(MobileCodexOutboundReview {
        workstream_id: record.workstream_id,
        source_run_id: record.source_run_id,
        source_reference_id: record.source_reference_id,
        source_endpoint_id: record.source_endpoint_id,
        source_codex_thread_id: record.source_codex_thread_id,
        destination_chatgpt_conversation_id: record.destination_chatgpt_conversation_id,
        original_text: record.original_text,
        attachments: serde_json::from_str(&record.attachments_json)
            .map_err(|_| "Persisted mobile ChatGPT Handoff attachments cannot be read")?,
        approved_text: record.approved_text,
        revision: u64::try_from(record.revision)
            .map_err(|_| "Persisted mobile ChatGPT Handoff review has an invalid revision")?,
        status: record.status,
        handoff_id: record.handoff_id,
    })
}

pub(crate) fn mobile_codex_outbound_review_response(
    action_id: &str,
    review: &MobileCodexOutboundReview,
    status: String,
) -> MobileHandoffReview {
    let message = review.approved_text.clone().unwrap_or_else(|| {
        exact_single_handoff_block(&review.original_text, "CHATGPT_HANDOFF")
            .unwrap_or_else(|| review.original_text.clone())
    });
    MobileHandoffReview {
        action_id: action_id.into(),
        revision: review.revision,
        status,
        message,
        attachments: review
            .attachments
            .iter()
            .map(|item| item.filename.clone())
            .collect(),
        attachment_options: vec![],
        requires_manual_dispatch: false,
        codex_outbound_identity: Some(MobileCodexOutboundReviewIdentity {
            workstream_id: review.workstream_id.clone(),
            source_id: review.source_reference_id.clone(),
            source_kind: if review.source_run_id.is_some() {
                "PROVIDER_RUN"
            } else {
                "REPLY_OBSERVATION"
            }
            .into(),
            source_endpoint_id: review.source_endpoint_id.clone(),
            source_codex_thread_id: review.source_codex_thread_id.clone(),
            destination_chatgpt_conversation_id: review.destination_chatgpt_conversation_id.clone(),
        }),
        chatgpt_inbound_identity: None,
    }
}

pub(crate) fn mobile_chatgpt_inbound_review_record(
    action_id: &str,
    review: &MobileReverseReview,
    created_at: i64,
) -> Result<MobileChatGptInboundReviewRecord, String> {
    Ok(MobileChatGptInboundReviewRecord {
        action_id: action_id.into(),
        workstream_id: review.workstream_id.clone(),
        source_run_id: review.source_run_id.clone(),
        source_reference_id: review.source_reference_id.clone(),
        source_endpoint_id: review.source_endpoint_id.clone(),
        source_chatgpt_conversation_id: review.source_chatgpt_conversation_id.clone(),
        source_response_identity: review.source_response_identity.clone(),
        destination_codex_thread_id: review.destination_codex_thread_id.clone(),
        original_text: review.original_text.clone(),
        attachments_json: serde_json::to_string(&review.attachments)
            .map_err(|_| "Mobile ChatGPT review attachments could not be serialized")?,
        approved_text: review.approved_text.clone(),
        revision: i64::try_from(review.revision)
            .map_err(|_| "Mobile ChatGPT review revision is too large")?,
        status: review.status.clone(),
        handoff_id: review.handoff_id.clone(),
        created_at,
        updated_at: mobile_review_timestamp(),
    })
}

pub(crate) fn mobile_chatgpt_inbound_review_from_record(
    record: MobileChatGptInboundReviewRecord,
) -> Result<MobileReverseReview, String> {
    if record.revision < 1 {
        return Err("Persisted mobile Codex Handoff review has an invalid revision".into());
    }
    Ok(MobileReverseReview {
        workstream_id: record.workstream_id,
        source_run_id: record.source_run_id,
        source_reference_id: record.source_reference_id,
        source_endpoint_id: record.source_endpoint_id,
        source_chatgpt_conversation_id: record.source_chatgpt_conversation_id,
        destination_codex_thread_id: record.destination_codex_thread_id,
        source_response_identity: record.source_response_identity,
        source_candidate_id: None,
        original_text: record.original_text,
        attachments: serde_json::from_str(&record.attachments_json)
            .map_err(|_| "Persisted mobile Codex Handoff attachments cannot be read")?,
        attachment_options: vec![],
        approved_text: record.approved_text,
        revision: u64::try_from(record.revision)
            .map_err(|_| "Persisted mobile Codex Handoff review has an invalid revision")?,
        status: record.status,
        handoff_id: record.handoff_id,
    })
}

pub(crate) fn mobile_chatgpt_inbound_review_response(
    action_id: &str,
    review: &MobileReverseReview,
) -> MobileHandoffReview {
    MobileHandoffReview {
        action_id: action_id.into(),
        revision: review.revision,
        status: review.status.clone(),
        message: review
            .approved_text
            .clone()
            .unwrap_or_else(|| review.original_text.clone()),
        attachments: review
            .attachments
            .iter()
            .map(|item| item.filename.clone())
            .collect(),
        attachment_options: review.attachment_options.clone(),
        requires_manual_dispatch: false,
        codex_outbound_identity: None,
        chatgpt_inbound_identity: Some(MobileChatGptInboundReviewIdentity {
            workstream_id: review.workstream_id.clone(),
            source_id: review.source_reference_id.clone(),
            source_kind: if review.source_run_id.is_some() {
                "PROVIDER_RUN"
            } else {
                "REPLY_OBSERVATION"
            }
            .into(),
            source_endpoint_id: review.source_endpoint_id.clone(),
            source_chatgpt_conversation_id: review.source_chatgpt_conversation_id.clone(),
            destination_codex_thread_id: review.destination_codex_thread_id.clone(),
        }),
    }
}

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
pub(crate) struct MobileObservedResponse {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) candidates: Vec<MobileRelayCandidate>,
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

/// Result of a user-triggered re-observation. It never creates a
/// provider request or Handoff; it can only repair the already persisted run
/// when the exact request and assistant turn are proven.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileChatGptResultRecovery {
    pub(crate) recovered: bool,
    pub(crate) message: String,
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileRelayCandidate {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) confidence: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileCodexOutboundReviewIdentity {
    pub(crate) workstream_id: String,
    pub(crate) source_id: String,
    pub(crate) source_kind: String,
    pub(crate) source_endpoint_id: String,
    pub(crate) source_codex_thread_id: String,
    pub(crate) destination_chatgpt_conversation_id: String,
}

/// A user-operated navigation target for one already-approved mobile
/// Codex-to-ChatGPT Handoff.  This is deliberately separate from provider
/// transport: resolving it reads only the persisted, exact Endpoint binding
/// and never starts, controls, or reads a browser page.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileManualChatGptDestination {
    pub(crate) canonical_url: String,
    pub(crate) conversation_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileChatGptInboundReviewIdentity {
    pub(crate) workstream_id: String,
    pub(crate) source_id: String,
    pub(crate) source_kind: String,
    pub(crate) source_endpoint_id: String,
    pub(crate) source_chatgpt_conversation_id: String,
    pub(crate) destination_codex_thread_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileHandoffReview {
    pub(crate) action_id: String,
    pub(crate) revision: u64,
    pub(crate) status: String,
    pub(crate) message: String,
    pub(crate) attachments: Vec<String>,
    pub(crate) attachment_options: Vec<String>,
    /// The presentation must not offer a send action which the Router's
    /// account-safety boundary will reject.  This is capability state, not a
    /// delivery outcome: the owner may copy the approved text and send it in
    /// their already-open, exact ChatGPT conversation.
    pub(crate) requires_manual_dispatch: bool,
    /// Present only for restart-safe Codex → ChatGPT review records. This lets
    /// the mobile client restore the exact review from its opaque URL action
    /// ID, not from a title, latest result, or workstream-level guess.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) codex_outbound_identity: Option<MobileCodexOutboundReviewIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) chatgpt_inbound_identity: Option<MobileChatGptInboundReviewIdentity>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobilePrepareInput {
    pub(crate) response_id: String,
    /// A human-selected or pasted starting payload. When absent, the exact
    /// marker block remains the only automatic candidate.
    #[serde(default)]
    pub(crate) initial_message: Option<String>,
    #[serde(default)]
    pub(crate) attachment_filenames: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileApproveInput {
    pub(crate) revision: u64,
    pub(crate) message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileAttachmentSelectionInput {
    pub(crate) revision: u64,
    #[serde(default)]
    pub(crate) attachment_filenames: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileSendInput {
    pub(crate) revision: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileCodexPrepareInput {
    pub(crate) run_id: String,
    #[serde(default)]
    pub(crate) attachment_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileFeedbackInput {
    pub(crate) run_id: String,
    pub(crate) feedback: String,
}

/// Opaque Router-owned feedback execution state. This deliberately exposes
/// product language rather than retired transport identifiers or terminal codes.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileFeedbackProgress {
    pub(crate) run_id: String,
    pub(crate) phase: String,
    pub(crate) message: String,
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
        self.chatgpt.shutdown();
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
                }
            })
            .collect())
    }

    pub(crate) fn mobile_workstream_snapshot(
        &self,
        workstream_id: &str,
    ) -> Result<WorkspaceSnapshot, String> {
        self.reconcile_proven_prewrite_mobile_handoffs(workstream_id)?;
        self.store.snapshot_for_workstream(workstream_id)
    }

    /// Older Hosts could leave a reverse handoff in SENDING after a sidecar
    /// rejected the page before the composer was available.  That exact
    /// failure proves no text was entered, unlike an ambiguous post-submit
    /// interruption.  Repair only that identity-linked historical shape when
    /// the mobile client asks for its existing workstream projection.
    pub(crate) fn reconcile_proven_prewrite_mobile_handoffs(
        &self,
        workstream_id: &str,
    ) -> Result<(), String> {
        let snapshot = self.store.snapshot_for_workstream(workstream_id)?;
        let runs = self.store.provider_runs_for_workstream(workstream_id)?;
        for handoff in snapshot.handoffs.iter().filter(|handoff| {
            handoff.direction == "CODEX_TO_CHATGPT" && handoff.status == "SENDING"
        }) {
            let Some(run) = runs.iter().find(|run| {
                run.origin_handoff_id.as_deref() == Some(handoff.id.as_str())
                    && run.provider == "CHATGPT"
                    && run.status == "UNKNOWN"
                    && run.external_run_id.is_none()
                    && run
                        .terminal_code
                        .as_deref()
                        .is_some_and(crate::chatgpt_direct_writer::is_proven_prewrite_failure)
            }) else {
                continue;
            };
            let code = run
                .terminal_code
                .as_deref()
                .expect("prewrite code was checked");
            self.store.reconcile_provider_run(&run.id, "FAILED", code)?;
            self.store.transition_handoff(
                &handoff.id,
                "FAILED",
                Some((
                    code.into(),
                    "ChatGPT rejected the carrier before text entry; no provider submission occurred".into(),
                )),
            )?;
        }
        Ok(())
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
                "Bind ChatGPT through exact URL validation and explicit confirmation".into(),
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

    /// Shared Desktop/Mobile prepare path. The direct carrier opens and proves
    /// only the supplied exact route; Core owns the read-only review candidate.
    pub(crate) fn prepare_explicit_chatgpt_endpoint_binding(
        &self,
        workstream_id: &str,
        input: &str,
    ) -> Result<ExplicitChatGptBindingCandidate, String> {
        self.prepare_explicit_chatgpt_endpoint_binding_with(workstream_id, input, |external_id| {
            self.chatgpt.verify_exact(external_id)?;
            Ok(BoundConversation {
                id: external_id.into(),
                title: None,
            })
        })
    }

    /// Prepares a replacement only after the owner has manually opened and
    /// checked the exact supplied URL in their normal browser. This route does
    /// not start, read, navigate, or otherwise control a browser. It remains
    /// session-only until the same revision-checked confirmation used by the
    /// direct carrier path.
    pub(crate) fn prepare_owner_confirmed_chatgpt_endpoint_binding(
        &self,
        workstream_id: &str,
        input: &str,
    ) -> Result<ExplicitChatGptBindingCandidate, String> {
        self.prepare_explicit_chatgpt_endpoint_binding_candidate(
            workstream_id,
            input,
            "用户已在默认浏览器核对的 ChatGPT 对话",
            "OWNER_CONFIRMED_EXACT_URL",
        )
    }

    pub(crate) fn prepare_explicit_chatgpt_endpoint_binding_with<F>(
        &self,
        workstream_id: &str,
        input: &str,
        validate: F,
    ) -> Result<ExplicitChatGptBindingCandidate, String>
    where
        F: FnOnce(&str) -> Result<chatgpt::model::BoundConversation, String>,
    {
        let external_id = parse_explicit_conversation_url(input)?;
        let review = review_explicit_chatgpt_binding(&self.store, workstream_id, &external_id)?;
        let bound = validate(&external_id)?;
        let candidate = ExplicitChatGptBindingCandidate {
            workstream_id: workstream_id.into(),
            external_id,
            canonical_url: input.trim().trim_end_matches('/').into(),
            label: bound.title.unwrap_or_else(|| "ChatGPT conversation".into()),
            verification: "BROWSER_EXECUTOR_EXACT_ROUTE".into(),
            expected_old_endpoint_id: review.expected_old_endpoint_id,
            expected_binding_revision: review.expected_binding_revision,
        };
        self.session
            .lock()
            .map_err(|_| "Router session is unavailable")?
            .explicit_chatgpt_binding_candidates
            .insert(workstream_id.into(), candidate.clone());
        Ok(candidate)
    }

    pub(crate) fn prepare_explicit_chatgpt_endpoint_binding_candidate(
        &self,
        workstream_id: &str,
        input: &str,
        label: &str,
        verification: &str,
    ) -> Result<ExplicitChatGptBindingCandidate, String> {
        let external_id = parse_explicit_conversation_url(input)?;
        let review = review_explicit_chatgpt_binding(&self.store, workstream_id, &external_id)?;
        let candidate = ExplicitChatGptBindingCandidate {
            workstream_id: workstream_id.into(),
            external_id,
            canonical_url: input.trim().trim_end_matches('/').into(),
            label: label.into(),
            verification: verification.into(),
            expected_old_endpoint_id: review.expected_old_endpoint_id,
            expected_binding_revision: review.expected_binding_revision,
        };
        self.session
            .lock()
            .map_err(|_| "Router session is unavailable")?
            .explicit_chatgpt_binding_candidates
            .insert(workstream_id.into(), candidate.clone());
        Ok(candidate)
    }

    /// Shared Desktop/Mobile confirmation path. Only a session candidate can
    /// supply the external identity to the checked persistence transaction.
    pub(crate) fn confirm_explicit_chatgpt_endpoint_binding(
        &self,
        workstream_id: &str,
    ) -> Result<Endpoint, String> {
        let candidate = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?
            .explicit_chatgpt_binding_candidates
            .get(workstream_id)
            .cloned()
            .ok_or("No validated ChatGPT binding is awaiting confirmation")?;
        if candidate.workstream_id != workstream_id {
            return Err("The pending ChatGPT binding belongs to a different Workstream".into());
        }
        let result = self.store.pair_workstream_endpoints_checked(
            workstream_id,
            EndpointPairingRequest {
                expected_binding_revision: candidate.expected_binding_revision,
                chatgpt: Some(EndpointPairingSide {
                    expected_active_endpoint_id: candidate.expected_old_endpoint_id,
                    external_id: candidate.external_id,
                    label: candidate.label,
                }),
                codex: None,
            },
        )?;
        let endpoint = result
            .chatgpt_endpoint
            .ok_or("The explicit ChatGPT confirmation did not return an ACTIVE Endpoint")?;
        self.store
            .save_chatgpt_endpoint_canonical_url(&endpoint.id, &candidate.canonical_url)?;
        self.session
            .lock()
            .map_err(|_| "Router session is unavailable")?
            .explicit_chatgpt_binding_candidates
            .remove(workstream_id);
        Ok(endpoint)
    }

    /// Persists the two independently verified session candidates in one Core
    /// transaction. The caller supplies only the fresh Codex exact-read
    /// candidate; ChatGPT identity can come only from the session candidate
    /// created by the Playwright exact-route proof above.
    pub(crate) fn confirm_explicit_chatgpt_codex_pairing(
        &self,
        workstream_id: &str,
        codex: EndpointPairingSideInput,
    ) -> Result<persistence::EndpointPairingResult, String> {
        let candidate = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?
            .explicit_chatgpt_binding_candidates
            .get(workstream_id)
            .cloned()
            .ok_or("No validated ChatGPT binding is awaiting confirmation")?;
        if candidate.workstream_id != workstream_id {
            return Err("The pending ChatGPT binding belongs to a different Workstream".into());
        }
        let result = self.store.pair_workstream_endpoints_checked(
            workstream_id,
            EndpointPairingRequest {
                expected_binding_revision: candidate.expected_binding_revision,
                chatgpt: Some(EndpointPairingSide {
                    expected_active_endpoint_id: candidate.expected_old_endpoint_id,
                    external_id: candidate.external_id,
                    label: candidate.label,
                }),
                codex: Some(EndpointPairingSide {
                    expected_active_endpoint_id: codex.expected_active_endpoint_id,
                    external_id: codex.external_id,
                    label: codex.label,
                }),
            },
        )?;
        if let Some(endpoint) = &result.chatgpt_endpoint {
            self.store
                .save_chatgpt_endpoint_canonical_url(&endpoint.id, &candidate.canonical_url)?;
        }
        self.session
            .lock()
            .map_err(|_| "Router session is unavailable")?
            .explicit_chatgpt_binding_candidates
            .remove(workstream_id);
        Ok(result)
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

    /// Phone and desktop use the same exact Conversation read. This can
    /// create one durable observation, but it never writes to ChatGPT.
    pub(crate) fn check_mobile_chatgpt_replies(
        &self,
        workstream_id: &str,
    ) -> Result<ChatGptManualRefreshResult, String> {
        self.check_chatgpt_replies(workstream_id, false)
    }

    pub(crate) fn check_chatgpt_replies(
        &self,
        workstream_id: &str,
        passive: bool,
    ) -> Result<ChatGptManualRefreshResult, String> {
        let endpoint = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
            .ok_or("The Workstream has no ACTIVE ChatGPT Endpoint")?;
        let reply = if passive {
            self.chatgpt.observe_passive(&endpoint.external_id)?
        } else {
            self.chatgpt.observe_exact(&endpoint.external_id)?
        };
        let current = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
            .ok_or("ACTIVE_ENDPOINT_CHANGED")?;
        if current.id != endpoint.id {
            return Err("ACTIVE_ENDPOINT_CHANGED".into());
        }
        // Recover only the original durable reservation; receipt never resends.
        for run in self
            .store
            .provider_runs_for_workstream(workstream_id)?
            .into_iter()
            .filter(|run| {
                run.provider == "CHATGPT"
                    && run.endpoint_id == endpoint.id
                    && matches!(run.status.as_str(), "STARTING" | "RUNNING" | "UNKNOWN")
            })
        {
            let accepted = match self.chatgpt.receipt(&run.id) {
                Ok(router_core::chatgpt::Receipt::Sent {
                    conversation_id,
                    accepted_message_id,
                    ..
                }) if conversation_id == endpoint.external_id => Some(accepted_message_id),
                _ => run.external_run_id.clone(),
            };
            if let Some(accepted) = accepted {
                if run.external_run_id.is_none() {
                    self.store
                        .attach_provider_run_external_identity(&run.id, "CHATGPT", &accepted)?;
                    if let Some(handoff_id) = &run.origin_handoff_id {
                        self.store.transition_handoff(handoff_id, "SENT", None)?;
                    }
                }
                if reply.preceding_user_message_id.as_deref() == Some(accepted.as_str()) {
                    self.store.accept_completed_provider_result(
                        &run.id,
                        &accepted,
                        &reply.message_id,
                        reply.text.clone(),
                    )?;
                }
            }
        }
        check_new_chatgpt_replies_with(
            &self.store,
            workstream_id,
            &mut |_| {
                Ok(Some(chatgpt::direct::TerminalReply {
                    message_id: reply.message_id.clone(),
                    text: reply.text.clone(),
                }))
            },
            &mut |payload| crate::push::send_payload(payload),
        )
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

    /// Re-reads a previously exact-correlated direct ChatGPT request only after
    /// the user asks to refresh it. No prompt is sent. An unreadable prior
    /// completion is corrected only when diagnostics and complete exact history
    /// agree on request, conversation, assistant turn, and nonempty result text.
    pub(crate) fn recover_active_chatgpt_result(
        &self,
        workstream_id: &str,
    ) -> Result<MobileChatGptResultRecovery, String> {
        self.recover_chatgpt_result(workstream_id, None)
    }

    /// Re-reads one explicitly selected feedback execution. The run ID is only
    /// a Router-owned opaque handle; the active endpoint, exact request ID,
    /// authoritative state, and complete history still must all agree.
    pub(crate) fn recover_chatgpt_feedback_result(
        &self,
        workstream_id: &str,
        run_id: &str,
    ) -> Result<MobileChatGptResultRecovery, String> {
        let retained = self
            .store
            .latest_reviewable_provider_result(workstream_id, "CHATGPT")?
            .ok_or("No current complete ChatGPT result is available for feedback recovery")?;
        if !self
            .store
            .feedback_run_is_sourced_from(run_id, &retained.id)?
        {
            return Err("The selected feedback execution is not sourced from the current exact ChatGPT result".into());
        }
        self.recover_chatgpt_result(workstream_id, Some(run_id))
    }

    pub(crate) fn recover_chatgpt_result(
        &self,
        _workstream_id: &str,
        _requested_run_id: Option<&str>,
    ) -> Result<MobileChatGptResultRecovery, String> {
        Ok(MobileChatGptResultRecovery {
            recovered: false,
            message: "历史 ChatGPT 结果恢复已退役；Router 未发送或重试任何 ChatGPT 请求。".into(),
        })
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

    pub(crate) fn mobile_chatgpt_feedback(
        &self,
        workstream_id: &str,
        input: MobileFeedbackInput,
    ) -> Result<MobileFeedbackProgress, String> {
        if input.feedback.trim().is_empty() {
            return Err("ChatGPT revision feedback cannot be empty".into());
        }
        let retained = self
            .store
            .latest_reviewable_provider_result(workstream_id, "CHATGPT")?
            .ok_or("No current complete ChatGPT result is available for feedback")?;
        if retained.id != input.run_id {
            return Err(
                "The ChatGPT result changed; re-open the current review before sending feedback"
                    .into(),
            );
        }
        let endpoint = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
            .ok_or("The Workstream has no ACTIVE ChatGPT Endpoint")?;
        ensure_retained_result_is_active(&retained, &endpoint)?;
        let run = self
            .store
            .create_chatgpt_dispatch_run(workstream_id, &endpoint.id, None)?;
        self.store
            .record_chatgpt_feedback_source(&run.id, &retained.id)?;
        let core = self.clone();
        let wid = workstream_id.to_string();
        let run_id = run.id.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let result = chatgpt_direct_writer::send_text(
                &core,
                &wid,
                &endpoint,
                &run_id,
                None,
                &input.feedback,
            );
            let _ = tx.send(result);
        });
        let _ = rx.recv_timeout(Duration::from_secs(46));
        let accepted = self
            .store
            .provider_runs_for_workstream(workstream_id)?
            .into_iter()
            .find(|candidate| candidate.id == run.id)
            .is_some_and(|candidate| candidate.external_run_id.is_some());
        if accepted {
            self.store.mark_provider_run_reviewed(&retained.id)?;
            return Ok(MobileFeedbackProgress {
                run_id: run.id,
                phase: "WAITING_FOR_REVISED_RESULT".into(),
                message: "ChatGPT accepted feedback; completion remains separate.".into(),
            });
        }
        Err("ChatGPT feedback acceptance unproved; no automatic retry".into())
    }

    /// Reads only the Router-owned feedback run created by the mobile review.
    /// It never sends, retries, or infers provider state from page content.
    pub(crate) fn mobile_chatgpt_feedback_progress(
        &self,
        workstream_id: &str,
        run_id: &str,
    ) -> Result<MobileFeedbackProgress, String> {
        let endpoint = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
            .ok_or("The Workstream has no ACTIVE ChatGPT Endpoint")?;
        let run = self
            .store
            .provider_runs_for_workstream(workstream_id)?
            .into_iter()
            .find(|candidate| candidate.id == run_id)
            .ok_or("The feedback execution does not belong to this Workstream")?;
        if run.provider != "CHATGPT" || run.endpoint_id != endpoint.id {
            return Err(
                "The feedback execution no longer matches the ACTIVE ChatGPT Endpoint".into(),
            );
        }
        let (phase, message) = match run.status.as_str() {
            "STARTING" | "RUNNING" => (
                "WAITING_FOR_REVISED_RESULT",
                "修改意见已确认送达；Router 正等待同一个精确 ChatGPT 工作的下一条完整结果。",
            ),
            "UNKNOWN" => (
                "DELIVERY_UNCONFIRMED",
                "Router 尚未确认修改意见是否送达；不会自动重试。",
            ),
            "FAILED" | "CANCELLED" => ("DELIVERY_FAILED", "修改意见未完成；Router 不会自动重试。"),
            "COMPLETED"
                if run
                    .result_text
                    .as_deref()
                    .is_some_and(|text| !text.trim().is_empty()) =>
            {
                (
                    "REVISED_RESULT_READY",
                    "新的完整 ChatGPT 结果已保留，可重新审阅。",
                )
            }
            "COMPLETED" => (
                "DELIVERY_UNCONFIRMED",
                "Router 尚未保留可审阅的修订结果；不会自动重试。",
            ),
            _ => return Err("The feedback execution has an unsupported persisted state".into()),
        };
        Ok(MobileFeedbackProgress {
            run_id: run.id,
            phase: phase.into(),
            message: message.into(),
        })
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

    pub(crate) fn mobile_observed_responses(
        &self,
        workstream_id: &str,
    ) -> Result<Vec<MobileObservedResponse>, String> {
        let endpoint = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
            .ok_or("The Workstream has no ACTIVE ChatGPT Endpoint")?;
        let responses = self
            .completed_chatgpt_responses
            .lock()
            .map_err(|_| "Router-observed ChatGPT results are unavailable")?;
        Ok(responses
            .iter()
            .filter(|(_, response)| response.conversation_id == endpoint.external_id)
            .map(|(id, response)| MobileObservedResponse {
                id: id.clone(),
                text: response.final_text.clone(),
                candidates: response
                    .relay_candidates
                    .iter()
                    .map(|candidate| MobileRelayCandidate {
                        id: candidate.id.clone(),
                        text: candidate.text.clone(),
                        confidence: candidate.confidence.clone(),
                    })
                    .collect(),
            })
            .collect())
    }

    pub(crate) fn prepare_mobile_reverse(
        &self,
        workstream_id: &str,
        input: MobilePrepareInput,
    ) -> Result<MobileHandoffReview, String> {
        let source_endpoint = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
            .ok_or("The Workstream has no ACTIVE ChatGPT Endpoint")?;
        let destination_endpoint = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Codex)?
            .ok_or("The Workstream has no ACTIVE Codex Endpoint")?;
        let observation = self
            .store
            .reply_observations_for_workstream(workstream_id)?
            .into_iter()
            .find(|item| item.id == input.response_id && item.endpoint_id == source_endpoint.id);
        let (source_run_id, source_response_identity, final_text) =
            if let Some(observation) = observation {
                (
                    None,
                    observation.assistant_identity.unwrap_or(observation.id),
                    observation.text,
                )
            } else {
                let retained = self
                    .store
                    .latest_reviewable_provider_result(workstream_id, "CHATGPT")?
                    .ok_or("No durable complete ChatGPT result is available for Handoff")?;
                ensure_retained_result_is_active(&retained, &source_endpoint)?;
                if retained.id != input.response_id
                    && retained.result_identity.as_deref() != Some(input.response_id.as_str())
                {
                    return Err(
                        "The ChatGPT review result changed; reopen it before preparing a Handoff"
                            .into(),
                    );
                }
                (
                    Some(retained.id.clone()),
                    retained
                        .result_identity
                        .clone()
                        .ok_or("The reviewable ChatGPT result lacks its exact response identity")?,
                    retained
                        .result_text
                        .clone()
                        .ok_or("The reviewable ChatGPT result has no retained text")?,
                )
            };
        let message = input
            .initial_message
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| exact_single_handoff_block(&final_text, "CODEX_HANDOFF"))
            .ok_or("Select or paste non-empty text for this Handoff, or provide one exact <CODEX_HANDOFF> block")?;
        // Listing is a bounded, read-only Adapter operation.  Do not materialize
        // anything until the reviewer explicitly confirms a selection below.
        // Its unavailability must not regress an otherwise eligible text-only
        // Handoff: an empty option set prevents attachment selection while the
        // pre-existing Review -> Edit -> Approve flow remains available.
        let attachment_options: Vec<String> = self
            .chatgpt
            .list_attachments(&source_endpoint.external_id, &source_response_identity)
            .map(|resources| {
                resources
                    .into_iter()
                    .map(|resource| resource.filename)
                    .collect()
            })
            .unwrap_or_default();
        if !input.attachment_filenames.is_empty() {
            return Err("Select ChatGPT attachments from the prepared Review surface".into());
        }
        // Reverse Handoff domain code deliberately verifies a Router-observed
        // completed response. Rehydrate only this bounded persisted result for
        // that check; it is not a second transcript cache.
        self.completed_chatgpt_responses
            .lock()
            .map_err(|_| "Router-observed ChatGPT results are unavailable")?
            .insert(
                source_response_identity.clone(),
                CompletedChatGptResponse {
                    conversation_id: source_endpoint.external_id.clone(),
                    response_identity: Some(source_response_identity.clone()),
                    final_text: final_text.clone(),
                    artifacts: None,
                    relay_candidates: vec![],
                },
            );
        let action_id = Uuid::new_v4().to_string();
        let review = MobileReverseReview {
            workstream_id: workstream_id.to_string(),
            source_run_id,
            source_reference_id: input.response_id,
            source_endpoint_id: source_endpoint.id.clone(),
            source_chatgpt_conversation_id: source_endpoint.external_id,
            destination_codex_thread_id: destination_endpoint.external_id,
            source_response_identity,
            source_candidate_id: None,
            original_text: message.clone(),
            attachments: vec![],
            attachment_options: attachment_options.clone(),
            approved_text: None,
            revision: 1,
            status: "READY".into(),
            handoff_id: None,
        };
        self.persist_mobile_chatgpt_inbound_review(&action_id, &review)?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        session
            .mobile_reviews
            .insert(action_id.clone(), review.clone());
        Ok(mobile_chatgpt_inbound_review_response(&action_id, &review))
    }

    pub(crate) fn select_mobile_reverse_attachments(
        &self,
        action_id: &str,
        input: MobileAttachmentSelectionInput,
    ) -> Result<MobileHandoffReview, String> {
        let snapshot = self.load_mobile_chatgpt_inbound_review(action_id)?;
        if snapshot.status != "READY" || snapshot.revision != input.revision {
            return Err(
                "Handoff changed; reopen the current review before selecting attachments".into(),
            );
        }
        let attachments = capture_reverse_attachments(
            self,
            &snapshot.source_chatgpt_conversation_id,
            &snapshot.source_response_identity,
            &input.attachment_filenames,
        )?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        let review = session
            .mobile_reviews
            .get_mut(action_id)
            .ok_or("The mobile Codex Handoff review is no longer available")?;
        if review.status != "READY" || review.revision != input.revision {
            return Err(
                "Handoff changed while attachments were captured; reopen the current review".into(),
            );
        }
        review.attachments = attachments;
        review.revision += 1;
        let persisted = review.clone();
        drop(session);
        self.persist_mobile_chatgpt_inbound_review(action_id, &persisted)?;
        Ok(mobile_chatgpt_inbound_review_response(
            action_id, &persisted,
        ))
    }

    pub(crate) fn approve_mobile_reverse(
        &self,
        action_id: &str,
        input: MobileApproveInput,
    ) -> Result<MobileHandoffReview, String> {
        let message = input.message.as_str();
        if message.trim().is_empty() {
            return Err("An approved Handoff payload cannot be empty".into());
        }
        let review = self.load_mobile_chatgpt_inbound_review(action_id)?;
        if review.revision != input.revision || review.status != "READY" {
            return Err("This mobile Handoff review is stale or is not ready for approval".into());
        }
        if let Some(source_run_id) = review.source_run_id.as_deref() {
            ensure_review_source_is_current(
                &self.store,
                &review.workstream_id,
                Provider::Chatgpt,
                source_run_id,
                &review.source_endpoint_id,
            )?;
            self.store.mark_provider_run_reviewed(source_run_id)?;
        } else {
            let exact_observation = self
                .store
                .reply_observations_for_workstream(&review.workstream_id)?
                .into_iter()
                .any(|observation| {
                    observation.id == review.source_reference_id
                        && observation.endpoint_id == review.source_endpoint_id
                        && observation
                            .assistant_identity
                            .as_deref()
                            .unwrap_or(observation.id.as_str())
                            == review.source_response_identity
                });
            if !exact_observation {
                return Err(
                    "The exact ChatGPT observation is no longer available for approval".into(),
                );
            }
        }
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        let review = session
            .mobile_reviews
            .get_mut(action_id)
            .ok_or("The mobile Handoff review is no longer available in this Router process")?;
        if review.revision != input.revision || review.status != "READY" {
            return Err("This mobile Handoff review is stale or is not ready for approval".into());
        }
        review.approved_text = Some(message.to_string());
        review.revision += 1;
        review.status = "APPROVED".into();
        let persisted = review.clone();
        drop(session);
        self.persist_mobile_chatgpt_inbound_review(action_id, &persisted)?;
        Ok(mobile_chatgpt_inbound_review_response(
            action_id, &persisted,
        ))
    }

    pub(crate) fn send_mobile_reverse(
        &self,
        action_id: &str,
        input: MobileSendInput,
    ) -> Result<TurnStartResult, String> {
        let _ = self.load_mobile_chatgpt_inbound_review(action_id)?;
        let review = {
            let mut session = self
                .session
                .lock()
                .map_err(|_| "Router session is unavailable")?;
            let review = session
                .mobile_reviews
                .get_mut(action_id)
                .ok_or("The mobile Handoff review is no longer available in this Router process")?;
            if review.revision != input.revision || review.status != "APPROVED" {
                return Err(
                    "This mobile Handoff approval is stale, already sent, or not approved".into(),
                );
            }
            // Claim the one permitted dispatch before dropping the session lock.
            // The existing persisted Handoff deduplication remains the durable
            // guard; this prevents a concurrent mobile replay from entering the
            // adapter acquisition path twice in one Router process.
            review.status = "SENDING".into();
            review.revision += 1;
            review.clone()
        };
        self.persist_mobile_chatgpt_inbound_review(action_id, &review)?;
        if let Err(error) = restore_mobile_chatgpt_review_source(self, &review) {
            let failed = {
                let mut session = self
                    .session
                    .lock()
                    .map_err(|_| "Router session is unavailable")?;
                let review = session
                    .mobile_reviews
                    .get_mut(action_id)
                    .ok_or("The mobile Codex Handoff review is no longer available")?;
                if review.status == "SENDING" {
                    review.status = "FAILED".into();
                    review.revision += 1;
                }
                review.clone()
            };
            self.persist_mobile_chatgpt_inbound_review(action_id, &failed)?;
            return Err(error);
        }
        let draft = ReverseHandoffDraft {
            workstream_id: review.workstream_id.clone(),
            source_chatgpt_conversation_id: review.source_chatgpt_conversation_id.clone(),
            source_response_identity: review.source_response_identity.clone(),
            source_candidate_id: review.source_candidate_id.clone(),
            destination_codex_thread_id: review.destination_codex_thread_id.clone(),
            message: review
                .approved_text
                .clone()
                .ok_or("Approved payload is missing")?,
            attachments: review.attachments.clone(),
            // The pre-existing Core-owned send path owns the domain transition
            // from READY through APPROVED to SENDING and persists that audit.
            // Mobile approval is an interface-session gate, not a second
            // Handoff state machine.
            status: HandoffStatus::Ready,
        };
        let result = match send_reverse_handoff(self, draft) {
            Ok(result) => result,
            Err(error) => {
                let mut session = self
                    .session
                    .lock()
                    .map_err(|_| "Router session is unavailable")?;
                if let Some(review) = session.mobile_reviews.get_mut(action_id) {
                    if review.status == "SENDING" {
                        review.status = "FAILED".into();
                        review.revision += 1;
                    }
                }
                let persisted = session.mobile_reviews.get(action_id).cloned();
                drop(session);
                if let Some(persisted) = persisted {
                    self.persist_mobile_chatgpt_inbound_review(action_id, &persisted)?;
                }
                return Err(error);
            }
        };
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        if let Some(review) = session.mobile_reviews.get_mut(action_id) {
            if review.status == "SENDING" {
                review.status = "SENT".into();
                review.revision += 1;
            }
        }
        let persisted = session.mobile_reviews.get(action_id).cloned();
        drop(session);
        if let Some(persisted) = persisted {
            self.persist_mobile_chatgpt_inbound_review(action_id, &persisted)?;
        }
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

    pub(crate) fn load_mobile_codex_outbound_review(
        &self,
        action_id: &str,
    ) -> Result<MobileCodexOutboundReview, String> {
        if let Some(review) = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?
            .mobile_outbound_reviews
            .get(action_id)
            .cloned()
        {
            return Ok(review);
        }
        let record = self
            .store
            .mobile_codex_outbound_review(action_id)?
            .ok_or("The mobile ChatGPT Handoff review is no longer available")?;
        let review = mobile_codex_outbound_review_from_record(record)?;
        self.session
            .lock()
            .map_err(|_| "Router session is unavailable")?
            .mobile_outbound_reviews
            .insert(action_id.into(), review.clone());
        Ok(review)
    }

    pub(crate) fn persist_mobile_codex_outbound_review(
        &self,
        action_id: &str,
        review: &MobileCodexOutboundReview,
    ) -> Result<(), String> {
        let existing = self.store.mobile_codex_outbound_review(action_id)?;
        let created_at = existing
            .as_ref()
            .map(|record| record.created_at)
            .unwrap_or_else(mobile_review_timestamp);
        let record = mobile_codex_outbound_review_record(action_id, review, created_at)?;
        if existing.is_some() {
            self.store.update_mobile_codex_outbound_review(&record)
        } else {
            self.store.create_mobile_codex_outbound_review(&record)
        }
    }

    pub(crate) fn load_mobile_chatgpt_inbound_review(
        &self,
        action_id: &str,
    ) -> Result<MobileReverseReview, String> {
        if let Some(review) = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?
            .mobile_reviews
            .get(action_id)
            .cloned()
        {
            return Ok(review);
        }
        let record = self
            .store
            .mobile_chatgpt_inbound_review(action_id)?
            .ok_or("The mobile Codex Handoff review is no longer available")?;
        let review = mobile_chatgpt_inbound_review_from_record(record)?;
        self.session
            .lock()
            .map_err(|_| "Router session is unavailable")?
            .mobile_reviews
            .insert(action_id.into(), review.clone());
        Ok(review)
    }

    pub(crate) fn persist_mobile_chatgpt_inbound_review(
        &self,
        action_id: &str,
        review: &MobileReverseReview,
    ) -> Result<(), String> {
        let existing = self.store.mobile_chatgpt_inbound_review(action_id)?;
        let created_at = existing
            .as_ref()
            .map(|record| record.created_at)
            .unwrap_or_else(mobile_review_timestamp);
        let record = mobile_chatgpt_inbound_review_record(action_id, review, created_at)?;
        if existing.is_some() {
            self.store.update_mobile_chatgpt_inbound_review(&record)
        } else {
            self.store.create_mobile_chatgpt_inbound_review(&record)
        }
    }

    pub(crate) fn mobile_chatgpt_inbound_review(
        &self,
        action_id: &str,
    ) -> Result<MobileHandoffReview, String> {
        let review = self.load_mobile_chatgpt_inbound_review(action_id)?;
        Ok(mobile_chatgpt_inbound_review_response(action_id, &review))
    }

    pub(crate) fn prepare_mobile_codex_outbound(
        &self,
        workstream_id: &str,
        input: MobileCodexPrepareInput,
    ) -> Result<MobileHandoffReview, String> {
        let source = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Codex)?
            .ok_or("The Workstream has no ACTIVE Codex Endpoint")?;
        let observed = self
            .store
            .reply_observations_for_workstream(workstream_id)?
            .into_iter()
            .find(|item| item.id == input.run_id && item.endpoint_id == source.id);
        let (source_run_id, source_identity, text) = if let Some(observation) = observed {
            (
                None,
                observation
                    .assistant_identity
                    .unwrap_or(observation.id.clone()),
                observation.text,
            )
        } else {
            let retained = self.store.latest_reviewable_provider_result(workstream_id, "CODEX")?
                .ok_or("No durable complete Codex result or external observation is available for ChatGPT Handoff")?;
            if retained.id != input.run_id {
                return Err(
                    "The Codex review source changed; reopen it before preparing a Handoff".into(),
                );
            }
            ensure_retained_result_is_active(&retained, &source)?;
            (
                Some(retained.id.clone()),
                retained.result_identity.unwrap_or(retained.id),
                retained
                    .result_text
                    .ok_or("The reviewable Codex result has no retained text")?,
            )
        };
        let destination = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
            .ok_or("The Workstream has no ACTIVE ChatGPT Endpoint")?;
        let selected = input.attachment_ids.into_iter().collect::<HashSet<_>>();
        let attachments = detect(&source_identity, &text)
            .into_iter()
            .filter(|candidate| selected.contains(&candidate.id))
            .map(|candidate| {
                if matches!(
                    candidate.integrity_status,
                    IntegrityStatus::Mismatch | IntegrityStatus::Error
                ) {
                    return Err(
                        "An attachment with mismatched or unreadable integrity cannot be selected"
                            .to_string(),
                    );
                }
                let path = candidate
                    .normalized_path
                    .ok_or("A selected attachment has no usable local path")?;
                Ok(chatgpt::model::HandoffAttachment {
                    id: candidate.id,
                    path,
                    filename: candidate.filename,
                    actual_sha256: candidate.actual_sha256,
                    integrity_status: Some(
                        format!("{:?}", candidate.integrity_status).to_uppercase(),
                    ),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let action_id = Uuid::new_v4().to_string();
        let review = MobileCodexOutboundReview {
            workstream_id: workstream_id.into(),
            source_run_id,
            source_reference_id: input.run_id,
            source_endpoint_id: source.id.clone(),
            source_codex_thread_id: source.external_id,
            destination_chatgpt_conversation_id: destination.external_id,
            original_text: text,
            attachments,
            approved_text: None,
            revision: 1,
            status: "READY".into(),
            handoff_id: None,
        };
        self.persist_mobile_codex_outbound_review(&action_id, &review)?;
        self.session
            .lock()
            .map_err(|_| "Router session is unavailable")?
            .mobile_outbound_reviews
            .insert(action_id.clone(), review.clone());
        Ok(mobile_codex_outbound_review_response(
            &action_id,
            &review,
            "READY".into(),
        ))
    }

    pub(crate) fn approve_mobile_codex_outbound(
        &self,
        action_id: &str,
        input: MobileApproveInput,
    ) -> Result<MobileHandoffReview, String> {
        let message = input.message.as_str();
        if message.trim().is_empty() {
            return Err("An approved Handoff payload cannot be empty".into());
        }
        let review = self.load_mobile_codex_outbound_review(action_id)?;
        if review.status != "READY" || review.revision != input.revision {
            return Err("This mobile ChatGPT Handoff review is stale".into());
        }
        if let Some(source_run_id) = review.source_run_id.as_deref() {
            ensure_review_source_is_current(
                &self.store,
                &review.workstream_id,
                Provider::Codex,
                source_run_id,
                &review.source_endpoint_id,
            )?;
            self.store.mark_provider_run_reviewed(source_run_id)?;
        } else {
            let exact_observation = self
                .store
                .reply_observations_for_workstream(&review.workstream_id)?
                .into_iter()
                .any(|observation| {
                    observation.id == review.source_reference_id
                        && observation.endpoint_id == review.source_endpoint_id
                });
            if !exact_observation {
                return Err(
                    "The exact external Codex observation is no longer available for approval"
                        .into(),
                );
            }
            if self
                .store
                .active_endpoint_for_workstream(&review.workstream_id, Provider::Codex)?
                .is_none_or(|endpoint| endpoint.id != review.source_endpoint_id)
            {
                return Err(
                    "The external Codex observation belongs to a superseded Endpoint".into(),
                );
            }
        }
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        let review = session
            .mobile_outbound_reviews
            .get_mut(action_id)
            .ok_or("The mobile ChatGPT Handoff review is no longer available")?;
        if review.status != "READY" || review.revision != input.revision {
            return Err("This mobile ChatGPT Handoff review is stale".into());
        }
        review.approved_text = Some(message.into());
        review.status = "APPROVED".into();
        review.revision += 1;
        let persisted = review.clone();
        self.persist_mobile_codex_outbound_review(action_id, &persisted)?;
        Ok(mobile_codex_outbound_review_response(
            action_id,
            review,
            review.status.clone(),
        ))
    }

    pub(crate) fn send_mobile_codex_outbound(
        &self,
        action_id: &str,
        input: MobileSendInput,
    ) -> Result<OutboundHandoffResult, String> {
        // A resumed mobile page carries the same opaque action ID after a
        // Host restart. Hydrate that exact persisted review before claiming
        // the one permitted dispatch.
        self.load_mobile_codex_outbound_review(action_id)?;
        let review = {
            let mut session = self
                .session
                .lock()
                .map_err(|_| "Router session is unavailable")?;
            let review = session
                .mobile_outbound_reviews
                .get_mut(action_id)
                .ok_or("The mobile ChatGPT Handoff review is no longer available")?;
            if review.status != "APPROVED" || review.revision != input.revision {
                return Err("This mobile ChatGPT Handoff approval is stale or not approved".into());
            }
            review.status = "SENDING".into();
            review.revision += 1;
            review.clone()
        };
        self.persist_mobile_codex_outbound_review(action_id, &review)?;
        let result = send_outbound_handoff(
            self,
            HandoffDraft {
                workstream_id: review.workstream_id,
                source_codex_thread_id: review.source_codex_thread_id,
                destination_chatgpt_conversation_id: review.destination_chatgpt_conversation_id,
                original_text: review.original_text,
                message: review.approved_text.ok_or("Approved payload is missing")?,
                attachments: review.attachments,
                status: HandoffStatus::Ready,
            },
        );
        if let Ok(mut session) = self.session.lock() {
            if let Some(current) = session.mobile_outbound_reviews.get_mut(action_id) {
                apply_mobile_outbound_result(current, &result);
                let persisted = current.clone();
                drop(session);
                let _ = self.persist_mobile_codex_outbound_review(action_id, &persisted);
            }
        }
        result
    }

    pub(crate) fn mobile_codex_outbound_review(
        &self,
        action_id: &str,
    ) -> Result<MobileHandoffReview, String> {
        let review = self.load_mobile_codex_outbound_review(action_id)?;
        let persisted_status = persisted_mobile_outbound_status(&self.store, &review)?;
        Ok(mobile_codex_outbound_review_response(
            action_id,
            &review,
            persisted_status.unwrap_or_else(|| review.status.clone()),
        ))
    }

    /// Resolves the one owner-operated ChatGPT location for a persisted,
    /// approved mobile review.  It fails closed when the review is no longer
    /// approved or its saved destination no longer equals the current ACTIVE
    /// Endpoint; it never substitutes a newer ChatGPT binding.
    pub(crate) fn mobile_codex_outbound_manual_destination(
        &self,
        action_id: &str,
    ) -> Result<MobileManualChatGptDestination, String> {
        if !CHATGPT_BROWSER_AUTOMATION_PAUSED_BY_OWNER {
            return Err(
                "Mobile manual ChatGPT navigation is unavailable while Router delivery is enabled"
                    .into(),
            );
        }
        let review = self.load_mobile_codex_outbound_review(action_id)?;
        let effective_status = persisted_mobile_outbound_status(&self.store, &review)?
            .unwrap_or_else(|| review.status.clone());
        if effective_status != "APPROVED" {
            return Err(
                "This mobile ChatGPT Handoff is not approved for owner-led manual dispatch".into(),
            );
        }
        let endpoint = self
            .store
            .active_endpoint_for_workstream(&review.workstream_id, Provider::Chatgpt)?
            .ok_or("The mobile ChatGPT Handoff no longer has an ACTIVE ChatGPT Endpoint")?;
        if endpoint.external_id != review.destination_chatgpt_conversation_id {
            return Err(
                "This mobile ChatGPT Handoff destination no longer matches the current binding"
                    .into(),
            );
        }
        let canonical_url = self
            .store
            .chatgpt_endpoint_canonical_url(&endpoint.id)?
            .unwrap_or_else(|| format!("https://chatgpt.com/c/{}", endpoint.external_id));
        Ok(MobileManualChatGptDestination {
            canonical_url,
            conversation_id: endpoint.external_id,
        })
    }

    /// Resolves the current ACTIVE ChatGPT location for an owner-led ordinary
    /// discussion.  This is deliberately a read of the persisted exact
    /// binding only: it does not create a ProviderRun, control a browser, or
    /// send the draft.  Unlike a Handoff destination, the current active
    /// binding is the explicit subject of this ordinary discussion action.
    pub(crate) fn mobile_current_chatgpt_manual_destination(
        &self,
        workstream_id: &str,
    ) -> Result<MobileManualChatGptDestination, String> {
        if !CHATGPT_BROWSER_AUTOMATION_PAUSED_BY_OWNER {
            return Err(
                "Mobile manual ChatGPT navigation is unavailable while Router delivery is enabled"
                    .into(),
            );
        }
        let endpoint = self
            .store
            .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
            .ok_or("This Workstream has no ACTIVE ChatGPT Endpoint")?;
        let canonical_url = self
            .store
            .chatgpt_endpoint_canonical_url(&endpoint.id)?
            .unwrap_or_else(|| format!("https://chatgpt.com/c/{}", endpoint.external_id));
        Ok(MobileManualChatGptDestination {
            canonical_url,
            conversation_id: endpoint.external_id,
        })
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

pub(crate) fn persisted_mobile_outbound_status(
    store: &RouterStore,
    review: &MobileCodexOutboundReview,
) -> Result<Option<String>, String> {
    let Some(handoff_id) = review.handoff_id.as_deref() else {
        return Ok(None);
    };
    let handoff = store.handoff_by_id(handoff_id)?;
    if handoff.workstream_id != review.workstream_id
        || handoff.direction != "CODEX_TO_CHATGPT"
        || handoff.endpoint_source()?.id != review.source_endpoint_id
        || handoff.endpoint_source()?.external_id != review.source_codex_thread_id
        || handoff.destination_endpoint.external_id != review.destination_chatgpt_conversation_id
    {
        return Err("Persisted Handoff identity no longer matches this mobile review".into());
    }
    Ok(Some(handoff.status))
}

pub(crate) fn apply_mobile_outbound_result(
    review: &mut MobileCodexOutboundReview,
    result: &Result<OutboundHandoffResult, String>,
) {
    review.status = match result {
        Ok(delivery) if delivery.status == "SENT" => "SENT",
        Ok(delivery) if delivery.status == "FAILED" => "FAILED",
        // Errors returned here precede the external worker, so no external
        // delivery was started and the state is proven not-sent.
        Err(_) => "FAILED",
        _ => "SENDING",
    }
    .into();
    if let Ok(delivery) = result {
        if delivery.handoff_id.is_some() {
            review.handoff_id = delivery.handoff_id.clone();
        }
    }
    review.revision += 1;
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

pub(crate) fn ensure_review_source_is_current(
    store: &RouterStore,
    workstream_id: &str,
    provider: Provider,
    run_id: &str,
    endpoint_id: &str,
) -> Result<(), String> {
    let active = store
        .active_endpoint_for_workstream(workstream_id, provider)?
        .ok_or("The Workstream no longer has the review source Endpoint")?;
    if active.id != endpoint_id {
        return Err("This result belongs to a superseded Endpoint; reopen or resolve the continuation before acting on it".into());
    }
    let current = store
        .latest_reviewable_provider_result(workstream_id, active.provider.as_str())?
        .ok_or("The reviewed result is no longer available")?;
    if current.id != run_id || current.endpoint_id != endpoint_id {
        return Err(
            "The reviewed result changed; reopen the current review before approval".into(),
        );
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
    pub(crate) browser_runtime: String,
    pub(crate) chatgpt_browser_mode: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BoundChatGptProviderSurfaceStatus {
    pub(crate) state: String,
    pub(crate) current_conversation_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatGptProviderSurfaceSnapshot {
    pub(crate) href: String,
    pub(crate) conversation_id: String,
    pub(crate) turns: Vec<DirectChatGptTerminalTurn>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DirectChatGptTerminalTurn {
    pub(crate) id: Option<String>,
    pub(crate) role: &'static str,
    pub(crate) text: String,
    pub(crate) streaming: bool,
    pub(crate) terminal: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatGptManualRefreshResult {
    pub(crate) state: String,
    pub(crate) observation_created: bool,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OutboundHandoffResult {
    pub(crate) status: String,
    pub(crate) handoff_id: Option<String>,
    pub(crate) final_text: Option<String>,
    pub(crate) detail: Option<String>,
}

/// The direct carrier can include browser stderr in its internal error. Keep
/// that diagnostic out of the desktop contract while still returning the one
/// stable category needed to distinguish a failed attempt from a timeout.
pub(crate) fn classified_chatgpt_carrier_failure(error: &str) -> String {
    if error.starts_with("CHATGPT_PLAYWRIGHT_EXIT_") {
        "CHATGPT_PLAYWRIGHT_EXIT".into()
    } else if error.starts_with("CHATGPT_PLAYWRIGHT_UNAVAILABLE") {
        "CHATGPT_PLAYWRIGHT_UNAVAILABLE".into()
    } else if error.starts_with("CHATGPT_SIDECAR_PROTOCOL_INVALID") {
        "CHATGPT_SIDECAR_PROTOCOL_INVALID".into()
    } else if error.starts_with("CHATGPT_ACCOUNT_SECURITY_REQUIRED") {
        "CHATGPT_ACCOUNT_SECURITY_REQUIRED".into()
    } else if error.starts_with("CHATGPT_APPROVED_TEXT_INVALID") {
        "CHATGPT_APPROVED_TEXT_INVALID".into()
    } else if error.starts_with("CHATGPT_TERMINAL_NOT_OBSERVED") {
        "CHATGPT_TERMINAL_NOT_OBSERVED".into()
    } else {
        match error {
            "ATTACHMENT_FILE_COUNT_INVALID"
            | "AUTH_REQUIRED"
            | "HUMAN_DRAFT"
            | "HASH_MISMATCH"
            | "INVALID_REQUEST"
            | "INVALID_CONVERSATION_ID"
            | "IMMUTABLE_OUTER_WHITESPACE_UNSUPPORTED"
            | "INVALID_ATTACHMENTS"
            | "ATTACHMENT_CHANGED_BEFORE_UPLOAD"
            | "AMBIGUOUS_ATTACHMENT_NAMES"
            | "CHATGPT_ACCEPTANCE_UNPROVEN"
            | "ATTACHMENT_INPUT_INVALID"
            | "ATTACHMENT_FILE_UNAVAILABLE"
            | "ATTACHMENT_CHANGED_BEFORE_ASSIGNMENT"
            | "ATTACHMENT_INPUT_REQUIRED"
            | "ATTACHMENT_ASSIGNMENT_UNPROVEN"
            | "CHATGPT_CONVERSATION_ID_INVALID"
            | "CHATGPT_CONVERSATION_NOT_OPEN"
            | "CHATGPT_EXACT_IDENTITY_MISMATCH"
            | "CHATGPT_AUTH_OR_COMPOSER_REQUIRED"
            | "CHATGPT_CARRIER_NOT_LIVE"
            | "CHATGPT_ACCOUNT_SECURITY_REQUIRED"
            | "CHATGPT_GENERATING"
            | "CHATGPT_HUMAN_DRAFT_PRESENT"
            | "CHATGPT_ROUTER_PROFILE_IN_USE"
            | "CHATGPT_ROUTER_PROFILE_REQUIRED"
            | "CHATGPT_SEND_CONTROL_REQUIRED"
            | "CHATGPT_SIDECAR_OPERATION_INVALID"
            | "CHATGPT_SIDECAR_REQUEST_INVALID"
            | "CHATGPT_SUBMIT_ACCEPTANCE_UNKNOWN" => error.into(),
            _ => "CHATGPT_DIRECT_CARRIER_FAILED".into(),
        }
    }
}

/// This value exists only in debug builds so the persistence-routing smoke can
/// exercise the normal reverse-handoff application path without depending on
/// a third-party terminal response. It is intentionally not exposed in
/// release builds or through product UI.
#[cfg(debug_assertions)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V0_005RoutingFixture {
    pub(crate) source_chatgpt_conversation_id: String,
    pub(crate) destination_codex_thread_id: String,
    pub(crate) source_response_identity: String,
    pub(crate) source_candidate_id: String,
    pub(crate) candidate_text: String,
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

/// A URL-only ChatGPT Project save derives the only accepted identity through
/// the strict canonical parser. This stays offline and never creates a
/// provider Project.
pub(crate) fn external_project_link_from_input(
    input: ExternalProjectLinkInput,
) -> Result<NewExternalProjectLink, String> {
    let provider = parse_provider(&input.provider)?;
    let (external_project_id, canonical_url) = match provider {
        Provider::Chatgpt => {
            let supplied_url = input.canonical_url.as_deref().ok_or(
                "ChatGPT Project links require a canonical https://chatgpt.com/g/<project-id>/project URL",
            )?;
            let (normalized_url, derived_id) = normalize_chatgpt_project_url(supplied_url)?;
            if let Some(supplied_id) = input
                .external_project_id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
            {
                if supplied_id != derived_id {
                    return Err("ChatGPT Project URL identity does not match the supplied external project identity".into());
                }
            }
            (derived_id, Some(normalized_url))
        }
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
    let Some(raw) = error.strip_prefix("Codex JSON-RPC error: ") else { return false; };
    let Ok(value) = serde_json::from_str::<Value>(raw) else { return false; };
    value["code"] == -32600
        && value["message"].as_str() == Some(format!("thread {thread_id} is not materialized yet; thread/turns/list is unavailable before first user message").as_str())
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

/// One exact, normal-Chrome ChatGPT observation. The first readable state is a
/// durable baseline, so binding or enabling observation never notifies for old
/// history. Later different terminal message IDs create at most one durable
/// observation. This function never navigates, focuses, writes, or reads
/// storage from the browser.
pub(crate) fn check_new_chatgpt_replies_with(
    store: &RouterStore,
    workstream_id: &str,
    read_latest_terminal: &mut dyn FnMut(
        &str,
    )
        -> Result<Option<chatgpt::direct::TerminalReply>, String>,
    push: &mut dyn FnMut(&[u8]) -> Result<crate::push::PushDeliveryOutcome, String>,
) -> Result<ChatGptManualRefreshResult, String> {
    let endpoint = store
        .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
        .ok_or("The Workstream has no ACTIVE ChatGPT Endpoint")?;
    let Some(reply) = read_latest_terminal(&endpoint.external_id)? else {
        if !store.chatgpt_reply_observer_is_initialized(&endpoint.id)? {
            store.save_chatgpt_reply_observer_empty_baseline(
                workstream_id,
                &endpoint.id,
                &endpoint.external_id,
            )?;
        }
        return Ok(ChatGptManualRefreshResult {
            state: "OBSERVER_BASELINE_ESTABLISHED".into(),
            observation_created: false,
        });
    };
    let watermark = store.chatgpt_reply_observer_watermark(&endpoint.id)?;
    let initialized = store.chatgpt_reply_observer_is_initialized(&endpoint.id)?;
    let watermark_identity = format!(
        "chatgpt-watermark:{}:{}",
        endpoint.external_id, reply.message_id
    );
    let same = watermark
        .as_ref()
        .is_some_and(|(identity, _)| identity == &watermark_identity);
    if watermark.is_none() && !initialized {
        store.save_chatgpt_reply_observer_watermark(
            workstream_id,
            &endpoint.id,
            &endpoint.external_id,
            &reply.message_id,
        )?;
        return Ok(ChatGptManualRefreshResult {
            state: "OBSERVER_BASELINE_ESTABLISHED".into(),
            observation_created: false,
        });
    }
    if same {
        store.save_chatgpt_reply_observer_watermark(
            workstream_id,
            &endpoint.id,
            &endpoint.external_id,
            &reply.message_id,
        )?;
        return Ok(ChatGptManualRefreshResult {
            state: "NO_NEW_TERMINAL_REPLY".into(),
            observation_created: false,
        });
    }
    let created =
        record_provider_surface_reply_with(store, &endpoint, &reply.message_id, &reply.text, push)?;
    // Persist even if the independently idempotent insert found an earlier
    // delivery after a restart. Text is never a deduplication key.
    store.save_chatgpt_reply_observer_watermark(
        workstream_id,
        &endpoint.id,
        &endpoint.external_id,
        &reply.message_id,
    )?;
    Ok(ChatGptManualRefreshResult {
        state: if created {
            "NEW_REPLY_OBSERVED"
        } else {
            "NO_NEW_TERMINAL_REPLY"
        }
        .into(),
        observation_created: created,
    })
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
    'observer: loop {
        match resident_observer_connection(&session,observer_epoch){
            ObserverConnection::Stopped=>return,
            ObserverConnection::Borrowed=>{std::thread::sleep(Duration::from_millis(200));continue;}
            ObserverConnection::Ready=>{}
        }
        crate::role_bridge::refresh_resident_activity(&store,&session,observer_epoch,&mut activity_cursor);
        let active_workstream_ids = match active_initialized_codex_observer_workstreams(&store) {
            Ok(workstream_ids) => workstream_ids,
            Err(_) => return,
        };
        match resident_observer_connection(&session,observer_epoch){ObserverConnection::Stopped=>return,ObserverConnection::Borrowed=>continue,ObserverConnection::Ready=>{}}
        if let Ok(watches) = store.codex_watches() {
            for watch in watches.into_iter().filter(|w|w.enabled && w.retry_after<=rollover_now().unwrap_or(0)*1000) {
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
    for endpoint in store
        .snapshot()?
        .endpoint_lineage
        .into_iter()
        .filter(|endpoint| endpoint.provider == "CODEX" && endpoint.status == "ACTIVE")
    {
        if store.endpoint_notification_enabled(&endpoint.id)? && store.codex_reply_observer_is_initialized(&endpoint.id)? {
            workstream_ids.insert(endpoint.workstream_id);
        }
    }
    Ok(workstream_ids.into_iter().collect())
}

/// ChatGPT uses the same durable baseline rule as Codex, but its normal
/// browser observer is intentionally much slower: it is a passive fallback,
/// not a page-refresh or a provider polling transport.  Before a workstream
/// has established a baseline this loop does not contact Chrome at all.
pub(crate) fn run_chatgpt_existing_conversation_observer(core: RouterCore) {
    loop {
        if core.chatgpt.status() == "STOPPED" {
            return;
        }
        if core.chatgpt.status() == "CONFIGURED" {
            let active_workstream_ids =
                match active_initialized_chatgpt_observer_workstreams(&core.store) {
                    Ok(workstream_ids) => workstream_ids,
                    Err(_) => return,
                };
            for workstream_id in active_workstream_ids {
                let result = core.check_chatgpt_replies(&workstream_id, true);
                if result.as_ref().is_err_and(|error| error == "AUTH_REQUIRED") {
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_secs(30));
    }
}

/// Historical ACTIVE endpoints are audit data, not permission to inspect all
/// old conversations. Only a deliberate exact baseline enables passive
/// observation for the current workstream.
pub(crate) fn active_initialized_chatgpt_observer_workstreams(
    store: &RouterStore,
) -> Result<Vec<String>, String> {
    let mut workstream_ids = HashSet::new();
    for endpoint in store
        .snapshot()?
        .endpoint_lineage
        .into_iter()
        .filter(|endpoint| endpoint.provider == "CHATGPT" && endpoint.status == "ACTIVE")
    {
        if store.endpoint_notification_enabled(&endpoint.id)? && store.chatgpt_reply_observer_is_initialized(&endpoint.id)? {
            workstream_ids.insert(endpoint.workstream_id);
        }
    }
    Ok(workstream_ids.into_iter().collect())
}

/// Router-owned ordinary discussion dispatch shared by the desktop IPC and
/// the authenticated mobile presentation.  Unlike a Handoff this creates one
/// exact ChatGPT ProviderRun only after resolving the persisted ACTIVE
/// endpoint; neither presentation layer may send by title, browser tab, or
/// an unbound conversation ID.
pub(crate) fn send_chatgpt_request_for_workstream(
    state: &RouterCore,
    workstream_id: &str,
    message: &str,
) -> Result<CompletedChatGptResponse, String> {
    let endpoint = state
        .store
        .active_endpoint_for_workstream(&workstream_id, Provider::Chatgpt)?
        .ok_or("The Workstream has no ACTIVE ChatGPT Endpoint")?;
    let run = state
        .store
        .create_chatgpt_dispatch_run(workstream_id, &endpoint.id, None)?;
    chatgpt_direct_writer::send_text(state, workstream_id, &endpoint, &run.id, None, message)
}

/// Shared Router-owned outbound path for desktop and authenticated mobile
/// presentation adapters. It preserves the existing Handoff/ProviderRun
/// separation and exact ACTIVE endpoint guards.
pub(crate) fn send_outbound_handoff(
    state: &RouterCore,
    draft: HandoffDraft,
) -> Result<OutboundHandoffResult, String> {
    let context = resolve_persisted_handoff_context(
        &state.store,
        &draft.workstream_id,
        "CODEX_TO_CHATGPT",
        &draft.source_codex_thread_id,
        &draft.destination_chatgpt_conversation_id,
    )?;
    let persisted_attachments = draft
        .attachments
        .iter()
        .map(persisted_attachment)
        .collect::<Vec<_>>();
    let mut dispatch_draft = draft.clone();
    for (attachment, persisted_attachment) in dispatch_draft
        .attachments
        .iter_mut()
        .zip(persisted_attachments.iter())
    {
        attachment.id = persisted_attachment.id.clone();
    }
    let persisted = create_persisted_handoff(
        &state.store,
        context,
        None,
        if draft.original_text.trim().is_empty() {
            &draft.message
        } else {
            &draft.original_text
        },
        &draft.message,
        persisted_attachments,
    )?;
    state
        .store
        .transition_handoff(&persisted.id, "APPROVED", None)?;
    state
        .store
        .transition_handoff(&persisted.id, "SENDING", None)?;
    let provider_run = match state.store.create_chatgpt_dispatch_run(
        &persisted.workstream_id,
        &persisted.destination_endpoint.id,
        Some(&persisted.id),
    ) {
        Ok(provider_run) => provider_run,
        Err(error) => {
            state.store.transition_handoff(
                &persisted.id,
                "FAILED",
                Some(("PROVIDER_RUN_CREATE_ERROR".into(), error.clone())),
            )?;
            return Ok(OutboundHandoffResult {
                status: "FAILED".into(),
                handoff_id: Some(persisted.id.clone()),
                final_text: None,
                detail: Some(error),
            });
        }
    };
    {
        let core = state.clone();
        let handoff_id = persisted.id.clone();
        let run_id = provider_run.id.clone();
        let endpoint = persisted.destination_endpoint.clone();
        let workstream_id = persisted.workstream_id.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let result = if dispatch_draft.attachments.is_empty() {
                chatgpt_direct_writer::send_text(
                    &core,
                    &workstream_id,
                    &endpoint,
                    &run_id,
                    Some(&handoff_id),
                    &dispatch_draft.message,
                )
            } else {
                chatgpt_direct_writer::send_handoff(
                    &core,
                    &workstream_id,
                    &endpoint,
                    &run_id,
                    &handoff_id,
                    &dispatch_draft.message,
                    &dispatch_draft.attachments,
                )
            };
            let _ = tx.send(result);
        });
        let detail = match rx.recv_timeout(Duration::from_secs(46)) {
            Ok(Ok(_)) => {
                "Provider acceptance and terminal completion are tracked separately; no automatic retry."
                    .into()
            }
            Ok(Err(error)) if error == "CHATGPT_ACCEPTED_PENDING_TERMINAL" =>
                "已发送到精确绑定的 ChatGPT 对话；正在等待终态回复。Router 不会重发，收到回复后会创建记录并按本机手机通知设置提醒。".into(),
            Ok(Err(error)) => format!(
                "发送未获精确确认（{}）；Router 不会自动重试。",
                classified_chatgpt_carrier_failure(&error)
            ),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                "Direct carrier timed out before exact completion; no automatic retry.".into()
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                "Direct carrier worker ended without exact completion; no automatic retry.".into()
            }
        };
        let latest = state.store.handoff_by_id(&persisted.id)?;
        return Ok(OutboundHandoffResult {
            status: latest.status,
            handoff_id: Some(persisted.id),
            final_text: None,
            detail: Some(detail),
        });
    }
}

pub(crate) fn send_reverse_handoff(
    core: &RouterCore,
    mut draft: ReverseHandoffDraft,
) -> Result<TurnStartResult, String> {
    let source = {
        let responses = core
            .completed_chatgpt_responses
            .lock()
            .map_err(|_| "Router-observed ChatGPT results are unavailable")?;
        completed_reverse_source(&responses, &draft)?.clone()
    };
    let _artifacts_metadata_is_retained_in_session = source.artifacts.as_ref();
    draft.approve()?;
    draft.sending()?;
    let deduplication_key = reverse_handoff_key(&draft);
    let context = resolve_persisted_handoff_context(
        &core.store,
        &draft.workstream_id,
        "CHATGPT_TO_CODEX",
        &draft.source_chatgpt_conversation_id,
        &draft.destination_codex_thread_id,
    )?;
    if core.store.has_sent_duplicate(
        Some(&draft.source_response_identity),
        &context.destination.id,
        &draft.message,
    )? {
        return Err("This completed ChatGPT response was already relayed to this Codex endpoint with the same approved text".to_string());
    }
    // The Human Gate has now been exercised.  Persist its delivery intent
    // before any external readiness probe so a slow or rejected Codex session
    // is auditable as pre-dispatch failure rather than an invisible UI wait.
    let persisted = create_persisted_handoff(
        &core.store,
        context,
        Some(draft.source_response_identity.clone()),
        &source.final_text,
        &draft.message,
        draft.attachments.iter().map(persisted_attachment).collect(),
    )?;
    core.store
        .transition_handoff(&persisted.id, "APPROVED", None)?;
    core.store
        .transition_handoff(&persisted.id, "SENDING", None)?;
    let provider_run = core.store.create_provider_run(
        &persisted.workstream_id,
        &persisted.destination_endpoint.id,
        "CODEX",
        Some(&persisted.id),
        None,
        "STARTING",
    )?;
    // Do not hold Session while app-server may ask a structured question.  Its
    // stdout listener must be able to retain that question for the mobile
    // review surface while this exact-thread readiness check is in flight.
    let (mut adapter, needs_write_acquisition) = {
        let mut session = core
            .session
            .lock()
            .map_err(|_| "Router session is unavailable")?;
        if session
            .sent_reverse_handoff_keys
            .contains(&deduplication_key)
        {
            return Err("This completed ChatGPT response was already relayed to this Codex thread with the same approved text".to_string());
        }
        let adapter = match session
            .adapter
            .take()
            .filter(|adapter| !adapter.is_closed())
        {
            Some(adapter) => adapter,
            None => {
                let _ = core
                    .store
                    .fail_provider_run_by_id(&provider_run.id, "CODEX_ADAPTER_UNAVAILABLE");
                let _ = core.store.transition_handoff(
                    &persisted.id,
                    "FAILED",
                    Some((
                        "CODEX_ADAPTER_UNAVAILABLE".into(),
                        "Codex backend disconnected. Use Reconnect first.".into(),
                    )),
                );
                return Err("Codex backend disconnected. Use Reconnect first.".to_string());
            }
        };
        if adapter.is_turn_active(&draft.destination_codex_thread_id) {
            session.adapter = Some(adapter);
            let _ = core
                .store
                .fail_provider_run_by_id(&provider_run.id, "CODEX_TURN_ACTIVE");
            let _ = core.store.transition_handoff(
                &persisted.id,
                "FAILED",
                Some((
                    "CODEX_TURN_ACTIVE".into(),
                    "Codex is still processing another turn.".into(),
                )),
            );
            return Err(
                "Codex is still processing another turn. Wait for it to complete before relaying ChatGPT's response."
                    .to_string(),
            );
        }
        session.codex_adapter_borrowed=true;
        (
            adapter,
            thread_needs_write_acquisition(
                &session.ready_threads,
                &draft.destination_codex_thread_id,
            ),
        )
    };
    if let Err(error) = ensure_adapter_thread_ready_for_write(
        &mut adapter,
        needs_write_acquisition,
        &draft.destination_codex_thread_id,
    ) {
        restore_codex_adapter(core, adapter, None)?;
        let _ = core
            .store
            .fail_provider_run_by_id(&provider_run.id, "CODEX_WRITE_READINESS_ERROR");
        let _ = core.store.transition_handoff(
            &persisted.id,
            "FAILED",
            Some(("CODEX_WRITE_READINESS_ERROR".into(), error.clone())),
        );
        return Err(error);
    }
    let mut dispatch_attachments = draft.attachments.clone();
    if let Err(error) = stage_reverse_attachments(
        &mut adapter,
        &draft.destination_codex_thread_id,
        &persisted.id,
        &mut dispatch_attachments,
    ) {
        let _ = core
            .store
            .fail_provider_run_by_id(&provider_run.id, "CODEX_ATTACHMENT_STAGE_ERROR");
        let _ = core.store.transition_handoff(
            &persisted.id,
            "FAILED",
            Some(("CODEX_ATTACHMENT_STAGE_ERROR".into(), error.clone())),
        );
        restore_codex_adapter(core, adapter, Some(&draft.destination_codex_thread_id))?;
        return Err(error);
    }
    let approved_payload = approved_reverse_payload(&draft);
    let result = match start_turn(
        &mut adapter,
        &draft.destination_codex_thread_id,
        &approved_payload,
    ) {
        Ok(result) => result,
        Err(error) => {
            let _ = core
                .store
                .fail_provider_run_by_id(&provider_run.id, "CODEX_ADAPTER_ERROR");
            let _ = core.store.transition_handoff(
                &persisted.id,
                "FAILED",
                Some(("CODEX_ADAPTER_ERROR".into(), error.clone())),
            );
            restore_codex_adapter(core, adapter, Some(&draft.destination_codex_thread_id))?;
            return Err(error);
        }
    };
    if let Err(error) =
        core.store
            .attach_provider_run_external_identity(&provider_run.id, "CODEX", &result.turn_id)
    {
        let _ = core
            .store
            .fail_provider_run_by_id(&provider_run.id, "IDENTITY_ATTACHMENT_ERROR");
        restore_codex_adapter(core, adapter, Some(&draft.destination_codex_thread_id))?;
        return Err(error);
    }
    core.store.transition_handoff(&persisted.id, "SENT", None)?;
    let mut session = core
        .session
        .lock()
        .map_err(|_| "Router session is unavailable")?;
    session
        .ready_threads
        .insert(draft.destination_codex_thread_id.clone());
    session.sent_reverse_handoff_keys.insert(deduplication_key);
    session.adapter = Some(adapter);
    session.codex_adapter_borrowed=false;
    Ok(result)
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

pub(crate) fn resolve_persisted_handoff_context(
    store: &RouterStore,
    workstream_id: &str,
    direction: &str,
    source_external_id: &str,
    destination_external_id: &str,
) -> Result<PersistedHandoffContext, String> {
    if workstream_id.trim().is_empty() {
        return Err("A Handoff must identify its Workstream".to_string());
    }
    // This is deliberately a focused read. Persisted global selection belongs
    // only to the main Router window and is never Handoff routing authority.
    let snapshot = store.snapshot_for_workstream(workstream_id)?;
    let persisted_workstream_id = snapshot
        .selected_workstream_id
        .ok_or("The requested Workstream no longer exists")?;
    if persisted_workstream_id != workstream_id {
        return Err("The requested Workstream could not be resolved exactly".to_string());
    }
    let (source, destination) = match direction {
        "CODEX_TO_CHATGPT" => (
            snapshot
                .active_codex_endpoint
                .ok_or("No active Codex binding")?,
            snapshot
                .active_chatgpt_endpoint
                .ok_or("No active ChatGPT binding")?,
        ),
        "CHATGPT_TO_CODEX" => (
            snapshot
                .active_chatgpt_endpoint
                .ok_or("No active ChatGPT binding")?,
            snapshot
                .active_codex_endpoint
                .ok_or("No active Codex binding")?,
        ),
        _ => return Err("Unsupported handoff direction".to_string()),
    };
    if source.workstream_id != workstream_id || destination.workstream_id != workstream_id {
        return Err("Handoff endpoints must belong to the requested Workstream".to_string());
    }
    if source.external_id != source_external_id
        || destination.external_id != destination_external_id
    {
        return Err("The current Workstream binding changed after review. Re-open relay review before sending.".to_string());
    }
    Ok(PersistedHandoffContext {
        workstream_id: persisted_workstream_id,
        source,
        destination,
        direction: direction.to_string(),
    })
}

pub(crate) fn create_persisted_handoff(
    store: &RouterStore,
    context: PersistedHandoffContext,
    source_response_identity: Option<String>,
    original_text: &str,
    approved_text: &str,
    attachments: Vec<NewAttachment>,
) -> Result<persistence::HandoffHistoryItem, String> {
    store.create_ready_handoff(NewHandoff {
        workstream_id: context.workstream_id,
        source_endpoint_id: context.source.id,
        destination_endpoint_id: context.destination.id,
        direction: context.direction,
        source_response_identity,
        original_text: original_text.to_string(),
        approved_text: approved_text.to_string(),
        attachments,
    })
}

pub(crate) fn persisted_attachment(
    attachment: &chatgpt::model::HandoffAttachment,
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
    Ok(Some(MobileCodexGoal {
        thread_id: actual_thread_id.to_string(),
        objective,
        status,
        token_budget: optional_goal_integer(goal, "tokenBudget")?,
        tokens_used: optional_goal_integer(goal, "tokensUsed")?,
        time_used_seconds: optional_goal_integer(goal, "timeUsedSeconds")?,
        created_at: goal.get("createdAt").cloned(),
        updated_at: goal.get("updatedAt").cloned(),
        active_turn_id: None,
    }))
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

pub(crate) fn capture_reverse_attachments(
    core: &RouterCore,
    conversation_id: &str,
    message_id: &str,
    selected_filenames: &[String],
) -> Result<Vec<chatgpt::model::HandoffAttachment>, String> {
    if selected_filenames.is_empty() {
        return Ok(vec![]);
    }
    let available = core.chatgpt.list_attachments(conversation_id, message_id)?;
    let mut selected = HashSet::new();
    let mut attachments = Vec::with_capacity(selected_filenames.len());
    // Resolve the owner's exact displayed selection to an opaque native control
    // from this exact terminal result. No regex/filename/title download guessing.
    let mut resources = Vec::new();
    for filename in selected_filenames {
        let matches = available
            .iter()
            .filter(|resource| &resource.filename == filename)
            .collect::<Vec<_>>();
        if !selected.insert(filename.clone()) || matches.len() != 1 {
            return Err("CHATGPT_ARTIFACT_SELECTION_REQUIRED".into());
        }
        resources.push(matches[0]);
    }
    for resource in resources {
        let materialized = core.chatgpt.materialize_attachment(
            conversation_id,
            message_id,
            &resource.resource_id,
        )?;
        if materialized.filename != resource.filename {
            return Err("RESOURCE_EVIDENCE_INVALID".into());
        }
        attachments.push(chatgpt::model::HandoffAttachment {
            id: Uuid::new_v4().to_string(),
            path: materialized.path,
            filename: materialized.filename,
            actual_sha256: Some(materialized.sha256),
            integrity_status: Some("VERIFIED".into()),
        });
    }
    Ok(attachments)
}

/// Copies already-reviewed reverse attachments into the exact Codex thread's
/// current workspace. This is deliberately a visible filesystem handoff, not
/// an invented app-server attachment type: the approved turn can refer to the
/// bounded `.aiwr/incoming/<handoff-id>` directory by its ordinary workspace
/// semantics. The persisted Handoff retains the original captured path.
pub(crate) fn stage_reverse_attachments(
    adapter: &mut CodexAdapter,
    thread_id: &str,
    handoff_id: &str,
    attachments: &mut [chatgpt::model::HandoffAttachment],
) -> Result<(), String> {
    if attachments.is_empty() {
        return Ok(());
    }
    let metadata = adapter.request_with_timeout(
        "thread/read",
        json!({ "threadId": thread_id, "includeTurns": false }),
        WRITE_READINESS_TIMEOUT,
    )?;
    let cwd = metadata
        .pointer("/thread/cwd")
        .and_then(Value::as_str)
        .ok_or("Codex thread workspace is unavailable")?;
    let workspace = fs::canonicalize(cwd).map_err(|_| "Codex thread workspace is unavailable")?;
    if !workspace.is_dir() {
        return Err("Codex thread workspace is unavailable".into());
    }
    stage_reverse_attachment_files(&workspace, handoff_id, attachments)
}

/// Stages a complete, already-reviewed attachment set atomically inside the
/// exact Codex workspace.  A rejected or changed later attachment must not
/// leave an earlier attachment visible to the target thread.
pub(crate) fn stage_reverse_attachment_files(
    workspace: &std::path::Path,
    handoff_id: &str,
    attachments: &mut [chatgpt::model::HandoffAttachment],
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

pub(crate) fn reverse_handoff_key(draft: &ReverseHandoffDraft) -> String {
    let mut hasher = Sha256::new();
    hasher.update(draft.source_chatgpt_conversation_id.as_bytes());
    hasher.update([0]);
    hasher.update(draft.source_response_identity.as_bytes());
    hasher.update([0]);
    hasher.update(draft.destination_codex_thread_id.as_bytes());
    hasher.update([0]);
    hasher.update(draft.message.trim().as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn approved_reverse_payload(draft: &ReverseHandoffDraft) -> String {
    draft.message.clone()
}

/// Rebuilds only the exact persisted source needed by an already-approved
/// ChatGPT → Codex review after a Host restart.  This is not a page read or a
/// provider action: it validates the original Router record against the still
/// ACTIVE binding and repopulates the existing in-memory send guard.
pub(crate) fn restore_mobile_chatgpt_review_source(
    core: &RouterCore,
    review: &MobileReverseReview,
) -> Result<(), String> {
    let active = core
        .store
        .active_endpoint_for_workstream(&review.workstream_id, Provider::Chatgpt)?
        .ok_or("The reviewed ChatGPT source no longer has an ACTIVE Endpoint")?;
    if active.id != review.source_endpoint_id
        || active.external_id != review.source_chatgpt_conversation_id
    {
        return Err("The reviewed ChatGPT source binding changed before delivery".into());
    }
    let final_text = if let Some(run_id) = review.source_run_id.as_deref() {
        let run = core
            .store
            .provider_runs_for_workstream(&review.workstream_id)?
            .into_iter()
            .find(|candidate| {
                candidate.id == run_id
                    && candidate.endpoint_id == review.source_endpoint_id
                    && candidate.provider == "CHATGPT"
            })
            .ok_or("The exact reviewed ChatGPT result is no longer available")?;
        if run.result_identity.as_deref() != Some(review.source_response_identity.as_str()) {
            return Err("The reviewed ChatGPT result identity changed before delivery".into());
        }
        run.result_text
            .ok_or("The exact reviewed ChatGPT result has no retained text")?
    } else {
        let observation = core
            .store
            .reply_observations_for_workstream(&review.workstream_id)?
            .into_iter()
            .find(|candidate| {
                candidate.id == review.source_reference_id
                    && candidate.endpoint_id == review.source_endpoint_id
            })
            .ok_or("The exact reviewed ChatGPT reply is no longer available")?;
        if observation
            .assistant_identity
            .as_deref()
            .unwrap_or(observation.id.as_str())
            != review.source_response_identity
        {
            return Err("The reviewed ChatGPT reply identity changed before delivery".into());
        }
        observation.text
    };
    core.completed_chatgpt_responses
        .lock()
        .map_err(|_| "Router-observed ChatGPT results are unavailable")?
        .insert(
            review.source_response_identity.clone(),
            CompletedChatGptResponse {
                conversation_id: review.source_chatgpt_conversation_id.clone(),
                response_identity: Some(review.source_response_identity.clone()),
                final_text,
                artifacts: None,
                relay_candidates: vec![],
            },
        );
    Ok(())
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

/// The installed executable recognizes only the fixed Connector origin that
/// Chrome supplies through the Native-Messaging manifest. The explicit bridge
/// argument is retained solely for the local protocol harness.
#[cfg(test)]
pub(crate) fn run_legacy_native_messaging_bridge() -> Result<(), String> {
    normal_chrome_extension::run_native_messaging_bridge()
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

pub(crate) fn completed_reverse_source<'a>(
    completed_responses: &'a HashMap<String, CompletedChatGptResponse>,
    draft: &ReverseHandoffDraft,
) -> Result<&'a CompletedChatGptResponse, String> {
    let source = completed_responses
        .get(&draft.source_response_identity)
        .ok_or(
            "This ChatGPT response is not a completed Router-observed response in this session",
        )?;
    if source.conversation_id != draft.source_chatgpt_conversation_id {
        return Err("The reverse handoff source conversation identity does not match the completed response".to_string());
    }
    if let Some(candidate_id) = &draft.source_candidate_id {
        if !source
            .relay_candidates
            .iter()
            .any(|candidate| &candidate.id == candidate_id)
        {
            return Err(
                "The selected relay candidate does not belong to this completed ChatGPT response"
                    .to_string(),
            );
        }
    }
    Ok(source)
}

pub(crate) struct PersistedHandoffContext {
    pub(crate) workstream_id: String,
    pub(crate) source: Endpoint,
    pub(crate) destination: Endpoint,
    pub(crate) direction: String,
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
        return Err("Bind ChatGPT through exact URL validation and explicit confirmation".into());
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
        return Err("Bind ChatGPT through exact URL validation and explicit confirmation".into());
    }
    state
        .store
        .bind_endpoint(&workstream_id, provider, external_id, label, replace)
}

pub(crate) fn service_prepare_current_chatgpt_endpoint_binding(
    workstream_id: String,
    state: &RouterState,
) -> Result<ExplicitChatGptBindingCandidate, String> {
    let selected = state.chatgpt.current_binding()?;
    if selected["status"] != "EXACT_CONVERSATION" {
        return Err(selected["status"]
            .as_str()
            .unwrap_or("EXACT_IDENTITY_UNPROVEN")
            .into());
    }
    let id = selected["conversationId"]
        .as_str()
        .ok_or("EXACT_IDENTITY_UNPROVEN")?;
    let title = selected["title"]
        .as_str()
        .filter(|title| !title.trim().is_empty() && title.len() <= 512)
        .unwrap_or("ChatGPT 对话");
    state.prepare_explicit_chatgpt_endpoint_binding_candidate(
        &workstream_id,
        &format!("https://chatgpt.com/c/{id}"),
        title,
        "BROWSER_EXECUTOR_EXACT_ROUTE",
    )
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

pub(crate) fn service_prepare_chatgpt_to_codex_handoff(
    workstream_id: String,
    response_id: String,
    initial_message: Option<String>,
    attachment_filenames: Option<Vec<String>>,
    state: &RouterState,
) -> Result<MobileHandoffReview, String> {
    state.prepare_mobile_reverse(
        &workstream_id,
        MobilePrepareInput {
            response_id,
            initial_message,
            attachment_filenames: attachment_filenames.unwrap_or_default(),
        },
    )
}

pub(crate) fn service_select_chatgpt_to_codex_handoff_attachments(
    action_id: String,
    revision: u64,
    attachment_filenames: Vec<String>,
    state: &RouterState,
) -> Result<MobileHandoffReview, String> {
    state.select_mobile_reverse_attachments(
        &action_id,
        MobileAttachmentSelectionInput {
            revision,
            attachment_filenames,
        },
    )
}

pub(crate) fn service_prepare_codex_to_chatgpt_handoff(
    workstream_id: String,
    run_id: String,
    attachment_ids: Vec<String>,
    state: &RouterState,
) -> Result<MobileHandoffReview, String> {
    state.prepare_mobile_codex_outbound(
        &workstream_id,
        MobileCodexPrepareInput {
            run_id,
            attachment_ids,
        },
    )
}

pub(crate) fn service_open_bound_chatgpt_conversation(
    workstream_id: String,
    state: &RouterState,
    host: &HostRuntime,
) -> Result<(), String> {
    let _operation = host
        .exact_host_operation
        .lock()
        .map_err(|_| "ChatGPT direct-carrier operation is unavailable")?;
    let endpoint = state
        .store
        .active_endpoint_for_workstream(&workstream_id, Provider::Chatgpt)?
        .ok_or("The Workstream has no ACTIVE ChatGPT Endpoint")?;
    state.chatgpt.verify_exact(&endpoint.external_id)
}

pub(crate) fn service_open_bound_chatgpt_in_default_browser(
    workstream_id: String,
    state: &RouterState,
) -> Result<DefaultBrowserOpenResult, String> {
    let endpoint = state
        .store
        .active_endpoint_for_workstream(&workstream_id, Provider::Chatgpt)?
        .ok_or("The Workstream has no ACTIVE ChatGPT Endpoint")?;
    // Compatibility entry reports the existing actionable Router-browser path.
    // It no longer scans daily Chrome, calls an old observer or opens a fallback.
    let _ = endpoint;
    Ok(DefaultBrowserOpenResult { state: "CHECK_REQUIRED" })
}

#[cfg(debug_assertions)]
pub(crate) fn service_seed_v0_005_routing_fixture(
    state: &RouterState,
) -> Result<V0_005RoutingFixture, String> {
    const FIXTURE_RESPONSE: &str =
        "<CODEX_HANDOFF>\nROUTER_PERSISTED_ROUTING_TO_CODEX03_OK\n</CODEX_HANDOFF>";

    let snapshot = state.store.snapshot()?;
    let source = snapshot
        .active_chatgpt_endpoint
        .ok_or("No active ChatGPT binding for the V0-005 routing fixture")?;
    let destination = snapshot
        .active_codex_endpoint
        .ok_or("No active Codex binding for the V0-005 routing fixture")?;
    let candidate = relay_candidates(FIXTURE_RESPONSE)
        .into_iter()
        .find(|candidate| candidate.kind == RelayCandidateKind::ExplicitMarker)
        .filter(|candidate| candidate.text == "ROUTER_PERSISTED_ROUTING_TO_CODEX03_OK")
        .ok_or("V0-005 routing fixture did not yield its explicit RelayCandidate")?;
    let response_identity = format!("v0-005-routing-fixture:{}", Uuid::new_v4());
    let mut responses = state
        .completed_chatgpt_responses
        .lock()
        .map_err(|_| "Router-observed ChatGPT results are unavailable")?;
    responses.insert(
        response_identity.clone(),
        CompletedChatGptResponse {
            conversation_id: source.external_id.clone(),
            response_identity: Some(response_identity.clone()),
            final_text: FIXTURE_RESPONSE.to_string(),
            artifacts: None,
            relay_candidates: vec![candidate.clone()],
        },
    );
    Ok(V0_005RoutingFixture {
        source_chatgpt_conversation_id: source.external_id,
        destination_codex_thread_id: destination.external_id,
        source_response_identity: response_identity,
        source_candidate_id: candidate.id,
        candidate_text: candidate.text,
    })
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
