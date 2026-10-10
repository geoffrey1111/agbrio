pub mod control;
pub mod assistant;
pub mod assistant_actions;
pub mod mcp_events;
pub mod dispatch;
pub mod migration;
pub mod relay;
pub mod role_bridge;
pub mod codex_watch;
pub mod watch_delivery;
pub mod watch_reply;
mod bridge_notifications;
#[cfg(test)]mod watch_usage_tests;
use crate::identity::{length_prefixed_hash, ScopeIdentity};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;
use uuid::Uuid;

const MAX_REPLY_OBSERVATION_BYTES: usize = 1_000_000;
const MAX_WORKSTREAM_DRAFT_BYTES: usize = 100_000;

const NORMAL_FEATURE_MIGRATIONS: &[(&str, &str)] = &[
    ("assistant-delegation-v1", include_str!("../../migrations/normal/012_assistant_delegation.sql")),
    ("reply-completion-v1", include_str!("../../migrations/normal/013_reply_completion.sql")),
    ("watch-removal-v1", include_str!("../../migrations/normal/011_watch_removal.sql")),
    ("watch-replies-v1", include_str!("../../migrations/normal/010_watch_replies.sql")),
    ("codex-watches-v1", include_str!("../../migrations/normal/007_codex_watches.sql")),
    ("watch-delivery-v1", include_str!("../../migrations/normal/008_watch_delivery.sql")),
    ("watch-retention-v1", include_str!("../../migrations/normal/009_watch_retention.sql")),
    (
        "endpoint-canonical-url-v1",
        include_str!("../../migrations/normal/001_endpoint_canonical_url.sql"),
    ),
    (
        "reply-push-render-receipt-v1",
        include_str!("../../migrations/normal/002_reply_push_render_receipt.sql"),
    ),
    (
        "mobile-codex-outbound-review-v1",
        include_str!("../../migrations/normal/003_mobile_codex_outbound_reviews.sql"),
    ),
    (
        "mobile-codex-outbound-review-source-reference-v1",
        include_str!("../../migrations/normal/004_mobile_codex_outbound_review_source_reference.sql"),
    ),
    (
        "mobile-chatgpt-inbound-review-v1",
        include_str!("../../migrations/normal/005_mobile_chatgpt_inbound_reviews.sql"),
    ),
    (
        "role-provider-results-v1",
        include_str!("../../migrations/normal/006_role_provider_results.sql"),
    ),
    ("assistant-instance-v1", include_str!("../../migrations/normal/014_assistant_instance.sql")),
    ("notification-seen-v1", include_str!("../../migrations/normal/015_notification_seen.sql")),
    ("assistant-conversation-review-v1", include_str!("../../migrations/normal/016_assistant_conversation_review.sql")),
    ("mcp-events-v1", include_str!("../../migrations/normal/017_mcp_events.sql")),
];

const MIGRATIONS: &[(i64, &str)] = &[
    (
        1,
        include_str!("../../migrations/001_initial_persistence.sql"),
    ),
    (
        2,
        include_str!("../../migrations/002_handoff_bridge_correlation.sql"),
    ),
    (
        3,
        include_str!("../../migrations/003_provider_runs_and_attention.sql"),
    ),
    (
        4,
        include_str!("../../migrations/004_handoff_attachment_send_evidence.sql"),
    ),
    (
        5,
        include_str!("../../migrations/005_provider_run_bounded_result_text.sql"),
    ),
    (
        6,
        include_str!("../../migrations/006_feedback_run_source.sql"),
    ),
    (
        7,
        include_str!("../../migrations/007_reply_observations.sql"),
    ),
    (
        8,
        include_str!("../../migrations/008_push_delivery_metadata.sql"),
    ),
    (
        9,
        include_str!("../../migrations/009_reply_observer_cursor.sql"),
    ),
    (
        10,
        include_str!("../../migrations/010_unified_workbench_v3_links.sql"),
    ),
    (
        11,
        include_str!("../../migrations/011_unified_workbench_v3_drafts.sql"),
    ),
    (
        12,
        include_str!("../../migrations/012_unified_workbench_v3_pins.sql"),
    ),
    (
        13,
        include_str!("../../migrations/013_unified_workbench_v3_codex_feedback_drafts.sql"),
    ),
];


#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workstream {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub binding_revision: i64,
    pub archived_at: Option<i64>,
    pub trashed_at: Option<i64>,
    pub pinned_at: Option<i64>,
}

/// A Router-owned reference to an existing external provider Project. It is
/// intentionally not an Endpoint: project association never authorizes a
/// conversation/thread binding or a provider write.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProjectLink {
    pub id: String,
    pub project_id: String,
    pub provider: String,
    pub external_project_id: String,
    pub canonical_url: Option<String>,
    pub label: String,
    pub source_kind: String,
    pub source_version: Option<String>,
    pub verified_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewExternalProjectLink {
    pub provider: Provider,
    pub external_project_id: String,
    pub canonical_url: Option<String>,
    pub label: String,
    pub source_kind: String,
    pub source_version: Option<String>,
    pub verified_at: Option<i64>,
}

/// Every requested side is checked against the exact ACTIVE Endpoint that was
/// visible at review time. `None` is valid only when that provider currently
/// has no ACTIVE Endpoint for the Workstream.
#[derive(Clone, Debug)]
pub struct EndpointPairingSide {
    pub expected_active_endpoint_id: Option<String>,
    pub external_id: String,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct EndpointPairingRequest {
    pub expected_binding_revision: i64,
    pub chatgpt: Option<EndpointPairingSide>,
    pub codex: Option<EndpointPairingSide>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointPairingResult {
    pub binding_revision: i64,
    pub chatgpt_endpoint: Option<Endpoint>,
    pub codex_endpoint: Option<Endpoint>,
}

/// Explicit same-owner reassignment of one active ChatGPT endpoint. It is
/// available only for an endpoint with no recorded product activity, so no
/// Handoff, ProviderRun, observation, or artifact history can be detached
/// from its original Workstream.
#[derive(Clone, Debug)]
pub struct ChatGptEndpointTransferRequest {
    pub source_workstream_id: String,
    pub target_workstream_id: String,
    pub source_endpoint_id: String,
    pub conversation_id: String,
    pub expected_source_binding_revision: i64,
    pub expected_target_binding_revision: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedBackup {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkstreamDraft {
    pub workstream_id: String,
    pub text: String,
    pub revision: i64,
    pub updated_at: i64,
}

/// A Router-local direct-Codex correction. It has no ProviderRun, Handoff, or
/// external message side effect until a separate explicit send succeeds.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexFeedbackDraft {
    pub workstream_id: String,
    pub source_run_id: String,
    pub text: String,
    pub revision: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Provider {
    Chatgpt,
    Codex,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Chatgpt => "CHATGPT",
            Self::Codex => "CODEX",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub id: String,
    pub workstream_id: String,
    pub provider: String,
    pub external_id: String,
    pub label: String,
    pub status: String,
    pub replaces_endpoint_id: Option<String>,
    pub created_at: i64,
    pub superseded_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffAttachmentRecord {
    pub id: String,
    pub handoff_id: String,
    pub filename: String,
    pub original_path: String,
    pub size: Option<i64>,
    pub sha256: Option<String>,
    pub integrity_status: Option<String>,
    pub send_sha256: Option<String>,
    pub send_verified_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "sourceKind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HandoffSource {
    Endpoint {
        #[serde(rename = "sourceEndpoint")]
        source_endpoint: Endpoint,
    },
    ControlContext {
        #[serde(rename = "sourceContextId")]
        source_context_id: String,
    },
}
impl HandoffHistoryItem {
    pub fn endpoint_source(&self) -> Result<&Endpoint, String> {
        match &self.source {
            HandoffSource::Endpoint { source_endpoint } => Ok(source_endpoint),
            HandoffSource::ControlContext { .. } => {
                Err("CONTROL_CONTEXT_REQUIRES_REVIEW_SERVICE".into())
            }
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffHistoryItem {
    pub id: String,
    pub workstream_id: String,
    pub direction: String,
    pub source_response_identity: Option<String>,
    pub original_text: String,
    pub approved_text: String,
    pub status: String,
    pub payload_hash: String,
    pub created_at: i64,
    pub approved_at: Option<i64>,
    pub sent_at: Option<i64>,
    pub failed_at: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub bridge_request_id: Option<String>,
    pub bridge_turn_key: Option<String>,
    pub bridge_correlation_observed_at: Option<i64>,
    pub attention_acknowledged_at: Option<i64>,
    #[serde(flatten)]
    pub source: HandoffSource,
    pub destination_endpoint: Endpoint,
    pub attachments: Vec<HandoffAttachmentRecord>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRun {
    pub id: String,
    pub workstream_id: String,
    pub endpoint_id: String,
    pub provider: String,
    pub origin_handoff_id: Option<String>,
    pub external_run_id: Option<String>,
    pub status: String,
    pub result_identity: Option<String>,
    pub result_text: Option<String>,
    pub terminal_code: Option<String>,
    pub started_at: Option<i64>,
    pub terminal_at: Option<i64>,
    pub reviewed_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A bounded, independently observed exact ChatGPT assistant reply. This is
/// deliberately not a ProviderRun and is never a conversation transcript.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyObservation {
    pub id: String,
    pub workstream_id: String,
    pub endpoint_id: String,
    pub assistant_identity: Option<String>,
    pub text: String,
    pub source_provider_run_id: Option<String>,
    pub observed_at: i64,
    /// Provider-reported terminal time in milliseconds; never inferred from refresh.
    #[serde(skip_serializing_if="Option::is_none")]
    pub completed_at:Option<i64>,
    #[serde(skip)]
    pub completion_checked:bool,
    pub read_at: Option<i64>,
    pub handled_at: Option<i64>,
    pub push_state: String,
    pub push_attempted_at: Option<i64>,
    /// This only proves that the same-origin PWA Service Worker completed the
    /// notification-rendering path. It is not a handset alert/user receipt.
    pub push_rendered_at: Option<i64>,
}


/// Tiny durable resume position for the pinned Bridge's observed-turn stream.
/// It contains no provider content and is scoped to one exact ACTIVE endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyObserverCursor {
    pub stream_epoch: String,
    pub last_sequence: u64,
}
#[derive(Clone, Debug)]
pub struct ReconciliationCandidate {
    pub run: ProviderRun,
    pub endpoint: Endpoint,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmokeRecordCounts {
    pub handoffs: i64,
    pub provider_runs: i64,
    pub endpoints: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionItem {
    pub kind: String,
    pub priority: i32,
    pub workstream_id: String,
    pub workstream_name: String,
    pub source_id: String,
    pub message: String,
    pub activity_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkstreamDashboardItem {
    pub project_id: String,
    pub project_name: String,
    pub workstream: Workstream,
    pub chatgpt_endpoint: Option<Endpoint>,
    pub codex_endpoint: Option<Endpoint>,
    pub chatgpt_run: Option<ProviderRun>,
    pub codex_run: Option<ProviderRun>,
    pub attention_items: Vec<AttentionItem>,
    pub last_activity_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardProjection {
    pub workstreams: Vec<WorkstreamDashboardItem>,
    pub attention_items: Vec<AttentionItem>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    pub projects: Vec<Project>,
    pub workstreams: Vec<Workstream>,
    pub selected_project_id: Option<String>,
    pub selected_workstream_id: Option<String>,
    pub active_chatgpt_endpoint: Option<Endpoint>,
    pub active_codex_endpoint: Option<Endpoint>,
    pub endpoint_lineage: Vec<Endpoint>,
    pub handoffs: Vec<HandoffHistoryItem>,
    /// Durable reverse-review records are intentionally distinct from Handoffs:
    /// an owner may have saved or approved a payload without any provider write.
    /// The focused projection exposes them so a normal workspace reopen does
    /// not make an approved manual dispatch appear to have disappeared.
    pub mobile_codex_outbound_reviews: Vec<MobileCodexOutboundReviewRecord>,
    /// The same restart-safe review projection for the ChatGPT → Codex
    /// direction.  It is distinct from a Handoff until the owner sends.
    pub mobile_chatgpt_inbound_reviews: Vec<MobileChatGptInboundReviewRecord>,
}

#[derive(Clone, Debug)]
pub struct NewAttachment {
    pub id: String,
    pub filename: String,
    pub original_path: String,
    pub size: Option<i64>,
    pub sha256: Option<String>,
    pub integrity_status: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AttachmentSendEvidence {
    pub attachment_id: String,
    pub review_sha256: Option<String>,
    pub integrity_status: Option<String>,
    pub send_sha256: String,
}

#[derive(Clone, Debug)]
pub struct NewHandoff {
    pub workstream_id: String,
    pub source_endpoint_id: String,
    pub destination_endpoint_id: String,
    pub direction: String,
    pub source_response_identity: Option<String>,
    pub original_text: String,
    pub approved_text: String,
    pub attachments: Vec<NewAttachment>,
}

/// A restart-safe, owner-editable review of one exact Codex result before it
/// is approved for the current ChatGPT binding. It is separate from
/// `handoffs`: a Handoff begins only after owner approval.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileCodexOutboundReviewRecord {
    pub action_id: String,
    pub workstream_id: String,
    pub source_run_id: Option<String>,
    /// Exact ReplyObservation or ProviderRun selected at review preparation.
    /// Empty is permitted only for records written before source-reference v1.
    pub source_reference_id: String,
    pub source_endpoint_id: String,
    pub source_codex_thread_id: String,
    pub destination_chatgpt_conversation_id: String,
    pub original_text: String,
    pub attachments_json: String,
    pub approved_text: Option<String>,
    pub revision: i64,
    pub status: String,
    pub handoff_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A restart-safe owner review of one exact ChatGPT observation or result
/// before it may be written to the active Codex thread.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileChatGptInboundReviewRecord {
    pub action_id: String,
    pub workstream_id: String,
    pub source_run_id: Option<String>,
    pub source_reference_id: String,
    pub source_endpoint_id: String,
    pub source_chatgpt_conversation_id: String,
    pub source_response_identity: String,
    pub destination_codex_thread_id: String,
    pub original_text: String,
    pub attachments_json: String,
    pub approved_text: Option<String>,
    pub revision: i64,
    pub status: String,
    pub handoff_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Evidence captured before a human confirms a Codex successor.  It is only
/// persisted together with the new Endpoint, never while the successor is a
/// session-only candidate.
#[derive(Clone, Debug)]
pub struct CompletedRolloverRun {
    pub external_run_id: String,
    pub result_identity: Option<String>,
    pub started_at: i64,
    pub terminal_at: i64,
}

pub struct RouterStore {
    connection: Mutex<Connection>,
    _preview_profile: Option<crate::runtime::PreviewProfile>,
}

impl RouterStore {
    /// Resolves the stable, Router-local identity for one authenticated ChatGPT
    /// management conversation. The caller has already authenticated `owner`;
    /// this method never writes raw host subject/session assertions or derives
    /// the durable scope from a rotating secret.
    pub fn resolve_mcp_scope(&self, owner: &str, meta: &Value) -> Result<ScopeIdentity, String> {
        if owner.len() != 64 || !owner.bytes().all(|value| value.is_ascii_hexdigit()) {
            return Err("SCOPE_INVALID".into());
        }
        let field = |name: &str| -> Result<&str, String> {
            let value = meta
                .get(name)
                .ok_or("SCOPE_REQUIRED")?
                .as_str()
                .ok_or("SCOPE_INVALID")?;
            if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
                return Err("SCOPE_INVALID".into());
            }
            Ok(value)
        };
        let subject = field("openai/subject")?;
        let session = field("openai/session")?;
        let provider = "openai-chatgpt-mcp";
        let subject_fingerprint = length_prefixed_hash(&[
            b"aiwr-principal-subject-v1",
            provider.as_bytes(),
            subject.as_bytes(),
        ]);
        self.with_connection(|connection| {
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let timestamp = now();
            tx.execute(
                "INSERT INTO principals(principal_id,provider,subject_fingerprint,created_at,updated_at) VALUES(?1,?2,?3,?4,?4) ON CONFLICT(provider,subject_fingerprint) DO UPDATE SET updated_at=excluded.updated_at",
                params![id(), provider, subject_fingerprint, timestamp],
            )
            .map_err(db_error)?;
            let principal_id: String = tx
                .query_row(
                    "SELECT principal_id FROM principals WHERE provider=?1 AND subject_fingerprint=?2",
                    params![provider, subject_fingerprint],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            let session_fingerprint = length_prefixed_hash(&[
                b"aiwr-host-session-v1",
                provider.as_bytes(),
                principal_id.as_bytes(),
                session.as_bytes(),
            ]);
            tx.execute(
                "INSERT INTO host_scopes(host_scope_id,principal_id,provider,session_fingerprint,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5) ON CONFLICT(principal_id,provider,session_fingerprint) DO UPDATE SET updated_at=excluded.updated_at",
                params![id(), principal_id, provider, session_fingerprint, timestamp],
            )
            .map_err(db_error)?;
            let host_scope_id: String = tx
                .query_row(
                    "SELECT host_scope_id FROM host_scopes WHERE principal_id=?1 AND provider=?2 AND session_fingerprint=?3",
                    params![principal_id, provider, session_fingerprint],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            tx.commit().map_err(db_error)?;
            Ok(ScopeIdentity {
                principal_key: owner.into(),
                scope_key: host_scope_id,
                key_version: 2,
            })
        })
    }

    /// Atomically moves private-owner records to a freshly generated owner
    /// identity and revokes every unconsumed browser-review capability.
    ///
    /// This is deliberately a maintenance-only operation: it preserves the
    /// immutable Handoff/ProviderRun rows, but it does not carry old approval
    /// page credentials or old local-MAC authority into the new key epoch.
    /// Historical receipts remain visible as records and are intentionally
    /// made unverifiable until new evidence is independently produced.
    pub fn rotate_private_owner_epoch(
        &self,
        old_principal: &str,
        new_principal: &str,
    ) -> Result<(), String> {
        if old_principal.len() != 64
            || new_principal.len() != 64
            || !old_principal.bytes().all(|value| value.is_ascii_hexdigit())
            || !new_principal.bytes().all(|value| value.is_ascii_hexdigit())
            || old_principal == new_principal
        {
            return Err("PRIVATE_CREDENTIAL_ROTATION_INVALID".into());
        }
        self.with_connection(|connection| {
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            // Do the context table first. It is the durable owner authority
            // used to read an existing CONTROL_TO_CODEX review such as P06.
            tx.execute(
                "UPDATE control_contexts SET principal_key=?1,updated_at=?3 WHERE principal_key=?2",
                params![new_principal, old_principal, now()],
            )
            .map_err(db_error)?;
            for statement in [
                "UPDATE control_handoff_details SET approved_principal_key=?1 WHERE approved_principal_key=?2",
                "UPDATE binding_requests SET approved_principal_key=?1 WHERE approved_principal_key=?2",
                "UPDATE operator_resolutions SET principal_key=?1 WHERE principal_key=?2",
                "UPDATE runtime_exit_state SET operator_principal_key=?1 WHERE operator_principal_key=?2",
                "UPDATE browser_endpoint_settings SET owner_principal_key=?1 WHERE owner_principal_key=?2",
                "UPDATE relay_handoff_details SET owner_principal_key=?1 WHERE owner_principal_key=?2",
                "UPDATE relay_handoff_details SET approved_principal_key=?1 WHERE approved_principal_key=?2",
                "UPDATE management_bridge_calls SET owner_principal_key=?1 WHERE owner_principal_key=?2",
            ] {
                tx.execute(statement, params![new_principal, old_principal])
                    .map_err(db_error)?;
            }
            // An old Review page must never remain an approval capability
            // after the ticket/principal rotation. Only pending review state
            // is cleared; consumed approvals retain their immutable history.
            tx.execute(
                "UPDATE control_handoff_details SET review_nonce_hash=NULL,approval_expires_at=NULL,approved_principal_key=NULL,approved_channel=NULL WHERE consumed_at IS NULL",
                [],
            )
            .map_err(db_error)?;
            tx.execute(
                "UPDATE relay_handoff_details SET review_nonce_hash=NULL,approval_expires_at=NULL,approved_principal_key=NULL WHERE consumed_at IS NULL",
                [],
            )
            .map_err(db_error)?;
            tx.execute(
                "UPDATE binding_requests SET approved_principal_key=NULL,review_nonce_hash=NULL WHERE state='REVIEW'",
                [],
            )
            .map_err(db_error)?;
            // The exposed evidence key must not verify any historical MAC.
            // Keep the receipt bodies for audit/reconciliation, but require
            // newly generated post-rotation evidence for any trusted claim.
            let revoked_mac = "0".repeat(64);
            tx.execute(
                "UPDATE dispatch_native_evidence SET integrity_mac=?1",
                params![&revoked_mac],
            )
            .map_err(db_error)?;
            tx.execute(
                "UPDATE bridge_dispatch_evidence SET integrity_mac=?1",
                params![&revoked_mac],
            )
            .map_err(db_error)?;
            // The replacement owner may read a pre-existing READY control
            // draft without rewriting its payload hash. This is not an
            // approval capability: all pending Review nonces were revoked
            // above and a fresh nonce is still required for any send.
            tx.execute(
                "INSERT INTO private_owner_rotation_continuity(handoff_id,payload_hash,principal_key,created_at) SELECT h.id,h.payload_hash,?1,?2 FROM handoffs h JOIN control_handoff_details d ON d.handoff_id=h.id LEFT JOIN handoff_dispatches x ON x.handoff_id=h.id WHERE h.source_kind='CONTROL_CONTEXT' AND h.status='READY' AND x.handoff_id IS NULL ON CONFLICT(handoff_id) DO UPDATE SET payload_hash=excluded.payload_hash,principal_key=excluded.principal_key,created_at=excluded.created_at",
                params![new_principal, now()],
            ).map_err(db_error)?;
            tx.commit().map_err(db_error)
        })
    }

    /// One-time schema recovery for an owner epoch that was already moved
    /// before continuity markers existed. It is intentionally limited to
    /// untouched READY control drafts and never creates a dispatch.
    pub fn attest_preserved_ready_controls(&self, principal: &str) -> Result<(), String> {
        if principal.len() != 64 || !principal.bytes().all(|value| value.is_ascii_hexdigit()) {
            return Err("PRIVATE_CREDENTIAL_ROTATION_INVALID".into());
        }
        self.with_connection(|connection| {
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            tx.execute(
                "INSERT INTO private_owner_rotation_continuity(handoff_id,payload_hash,principal_key,created_at) SELECT h.id,h.payload_hash,?1,?2 FROM handoffs h JOIN control_handoff_details d ON d.handoff_id=h.id JOIN control_contexts c ON c.id=d.context_id LEFT JOIN handoff_dispatches x ON x.handoff_id=h.id WHERE h.source_kind='CONTROL_CONTEXT' AND h.status='READY' AND x.handoff_id IS NULL AND c.principal_key=?1 ON CONFLICT(handoff_id) DO NOTHING",
                params![principal, now()],
            )
            .map_err(db_error)?;
            tx.commit().map_err(db_error)
        })
    }

    pub fn open_default() -> Result<Self, String> {
        let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
        Self::open_at(
            PathBuf::from(local)
                .join("AIWorkRouter")
                .join("data")
                .join("router.db"),
        )
    }

    pub fn open_at(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create Router data directory: {error}"))?;
        }
        let mut connection = Connection::open(&path).map_err(db_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")
            .map_err(db_error)?;
        migration::preflight(&connection, 13)?;
        run_migrations(&mut connection)?;
        run_normal_feature_migrations(&mut connection)?;
        role_bridge::migrate(&mut connection, &path)?;
        bridge_notifications::migrate(&mut connection)?;
        let store=Self {
            connection: Mutex::new(connection),
            _preview_profile: None,
        };
        store.prune_codex_watch_events()?;
        Ok(store)
    }

    pub fn create_project(
        &self,
        name: String,
        description: Option<String>,
    ) -> Result<Project, String> {
        let name = nonempty(&name, "Project name")?;
        let now = now();
        let project = Project {
            id: id(),
            name,
            description: clean_optional(description),
            created_at: now,
            updated_at: now,
        };
        self.with_connection(|connection| { connection.execute("INSERT INTO projects (id,name,description,created_at,updated_at) VALUES (?1,?2,?3,?4,?5)", params![project.id, project.name, project.description, now, now]).map_err(db_error)?; Ok(project) })
    }

    pub fn update_project(
        &self,
        project_id: &str,
        name: String,
        description: Option<String>,
    ) -> Result<Project, String> {
        let name = nonempty(&name, "Project name")?;
        let now = now();
        self.with_connection(|connection| {
            if connection
                .execute(
                    "UPDATE projects SET name=?2, description=?3, updated_at=?4 WHERE id=?1",
                    params![project_id, name, clean_optional(description), now],
                )
                .map_err(db_error)?
                != 1
            {
                return Err("Project not found".into());
            }
            project_by_id(connection, project_id)
        })
    }

    pub fn create_workstream(&self, project_id: &str, name: String) -> Result<Workstream, String> {
        let name = nonempty(&name, "Workstream name")?;
        let now = now();
        let workstream = Workstream {
            id: id(),
            project_id: project_id.to_string(),
            name,
            status: "ACTIVE".into(),
            created_at: now,
            updated_at: now,
            binding_revision: 0,
            archived_at: None,
            trashed_at: None,
            pinned_at: None,
        };
        self.with_connection(|connection| { connection.execute("INSERT INTO workstreams (id,project_id,name,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6)", params![workstream.id, workstream.project_id, workstream.name, workstream.status, now, now]).map_err(db_error)?; set_setting(connection, "last_project_id", project_id, now)?; set_setting(connection, "last_workstream_id", &workstream.id, now)?; Ok(workstream) })
    }

    /// Creates the durable identity chosen in a browser onboarding flow without
    /// selecting it as the daily default.  The caller supplies a UUID that is
    /// also its idempotency key, so a retried prepare cannot create another
    /// anonymous `Codex task` record.
    pub fn ensure_workstream(
        &self,
        workstream_id: &str,
        project_id: &str,
        name: String,
    ) -> Result<Workstream, String> {
        Uuid::parse_str(workstream_id).map_err(|_| "Invalid workstream ID")?;
        let name = nonempty(&name, "Workstream name")?;
        self.with_connection(|connection| {
            let existing = connection
                .query_row(
                    "SELECT id,project_id,name,status,created_at,updated_at,binding_revision,archived_at,trashed_at,pinned_at FROM workstreams WHERE id=?1",
                    params![workstream_id],
                    workstream_row,
                )
                .optional()
                .map_err(db_error)?;
            if let Some(workstream) = existing {
                if workstream.project_id == project_id
                    && workstream.name == name
                    && workstream.status == "ACTIVE"
                    && workstream.trashed_at.is_none()
                {
                    return Ok(workstream);
                }
                return Err("WORKSTREAM_IDEMPOTENCY_CONFLICT".into());
            }
            let timestamp = now();
            connection
                .execute(
                    "INSERT INTO workstreams (id,project_id,name,status,created_at,updated_at) VALUES (?1,?2,?3,'ACTIVE',?4,?4)",
                    params![workstream_id, project_id, name, timestamp],
                )
                .map_err(db_error)?;
            workstream_by_id(connection, workstream_id)
        })
    }

    /// Display metadata only. Routing and binding revision remain unchanged.
    pub fn rename_workstream(&self, workstream_id: &str, name: &str) -> Result<Workstream, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 100 || (name.contains('\r') || name.contains('\n')) {
            return Err("BRIDGE_NAME_INVALID".into());
        }
        self.with_connection(|connection| {
            let before = workstream_by_id(connection, workstream_id)?;
            if before.status != "ACTIVE" { return Err("BRIDGE_NOT_ACTIVE".into()); }
            connection.execute("UPDATE workstreams SET name=?1,updated_at=?2 WHERE id=?3", params![name, now(), workstream_id]).map_err(db_error)?;
            workstream_by_id(connection, workstream_id)
        })
    }

    pub fn select_workspace(
        &self,
        project_id: &str,
        workstream_id: Option<&str>,
    ) -> Result<(), String> {
        self.with_connection(|connection| {
            project_by_id(connection, project_id)?;
            if let Some(workstream_id) = workstream_id {
                let workstream = workstream_by_id(connection, workstream_id)?;
                if workstream.project_id != project_id {
                    return Err("Workstream does not belong to the selected Project".into());
                }
            }
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            set_setting(&transaction, "last_project_id", project_id, now())?;
            if let Some(workstream_id) = workstream_id {
                set_setting(&transaction, "last_workstream_id", workstream_id, now())?;
            } else {
                transaction
                    .execute(
                        "DELETE FROM app_settings WHERE key='last_workstream_id'",
                        [],
                    )
                    .map_err(db_error)?;
            }
            transaction.commit().map_err(db_error)
        })
    }

    /// Owner-resident work must cover every active project, independently of UI selection.
    pub fn active_workstreams(&self) -> Result<Vec<Workstream>, String> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare("SELECT id,project_id,name,status,created_at,updated_at,binding_revision,archived_at,trashed_at,pinned_at FROM workstreams WHERE trashed_at IS NULL AND archived_at IS NULL AND status!='ARCHIVED' ORDER BY id").map_err(db_error)?;
            let rows = statement.query_map([], workstream_row).map_err(db_error)?
                .collect::<Result<Vec<_>, _>>().map_err(db_error)?;
            Ok(rows)
        })
    }

    pub fn snapshot(&self) -> Result<WorkspaceSnapshot, String> {
        self.with_connection(|connection| {
            let projects = list_projects(connection)?;
            let selected_project_id = setting(connection, "last_project_id")?;
            let selected_workstream_id = setting(connection, "last_workstream_id")?;
            let workstreams = match selected_project_id.as_deref() {
                Some(project_id) => list_workstreams(connection, project_id)?,
                None => vec![],
            };
            let valid_workstream_id = selected_workstream_id.filter(|workstream_id| {
                workstreams
                    .iter()
                    .any(|workstream| workstream.id == *workstream_id)
            });
            let active_chatgpt_endpoint = match valid_workstream_id.as_deref() {
                Some(id) => role_bridge::projection_endpoint(connection, id, "CHATGPT")?,
                None => None,
            };
            let active_codex_endpoint = match valid_workstream_id.as_deref() {
                Some(id) => role_bridge::projection_endpoint(connection, id, "CODEX")?,
                None => None,
            };
            let handoffs = match valid_workstream_id.as_deref() {
                Some(id) => list_handoffs(connection, id)?,
                None => vec![],
            };
            let endpoint_lineage = match valid_workstream_id.as_deref() {
                Some(id) => list_endpoints(connection, id)?,
                None => vec![],
            };
            let mobile_codex_outbound_reviews = match valid_workstream_id.as_deref() {
                Some(id) => list_mobile_codex_outbound_reviews(connection, id)?,
                None => vec![],
            };
            let mobile_chatgpt_inbound_reviews = match valid_workstream_id.as_deref() {
                Some(id) => list_mobile_chatgpt_inbound_reviews(connection, id)?,
                None => vec![],
            };
            Ok(WorkspaceSnapshot {
                projects,
                workstreams,
                selected_project_id,
                selected_workstream_id: valid_workstream_id,
                active_chatgpt_endpoint,
                active_codex_endpoint,
                endpoint_lineage,
                handoffs,
                mobile_codex_outbound_reviews,
                mobile_chatgpt_inbound_reviews,
            })
        })
    }

    /// Builds a focused projection without mutating the persisted global
    /// workspace preference. Native windows own their presentation context;
    /// that context must never become routing or selection authority.
    pub fn snapshot_for_workstream(
        &self,
        workstream_id: &str,
    ) -> Result<WorkspaceSnapshot, String> {
        self.with_connection(|connection| {
            let workstream = workstream_by_id(connection, workstream_id)?;
            let projects = list_projects(connection)?;
            let workstreams = list_workstreams(connection, &workstream.project_id)?;
            Ok(WorkspaceSnapshot {
                projects,
                workstreams,
                selected_project_id: Some(workstream.project_id.clone()),
                selected_workstream_id: Some(workstream.id.clone()),
                active_chatgpt_endpoint: role_bridge::projection_endpoint(connection, &workstream.id, "CHATGPT")?,
                active_codex_endpoint: role_bridge::projection_endpoint(connection, &workstream.id, "CODEX")?,
                endpoint_lineage: list_endpoints(connection, &workstream.id)?,
                handoffs: list_handoffs(connection, &workstream.id)?,
                mobile_codex_outbound_reviews: list_mobile_codex_outbound_reviews(connection, &workstream.id)?,
                mobile_chatgpt_inbound_reviews: list_mobile_chatgpt_inbound_reviews(connection, &workstream.id)?,
            })
        })
    }

    pub fn smoke_record_counts(&self) -> Result<SmokeRecordCounts, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Router store is unavailable")?;
        let count = |table: &str| -> Result<i64, String> {
            connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .map_err(|error| error.to_string())
        };
        Ok(SmokeRecordCounts {
            handoffs: count("handoffs")?,
            provider_runs: count("provider_runs")?,
            endpoints: count("endpoints")?,
        })
    }

    /// Saves an association with an existing external Project. This is safe
    /// while offline: no adapter is consulted and no provider project is ever
    /// created. For ChatGPT the canonical URL and external project ID must
    /// agree exactly after strict normalization.
    pub fn upsert_external_project_link(
        &self,
        project_id: &str,
        input: NewExternalProjectLink,
    ) -> Result<ExternalProjectLink, String> {
        let provider = input.provider.as_str().to_string();
        let external_project_id =
            nonempty(&input.external_project_id, "External project identity")?;
        let label = nonempty(&input.label, "External project label")?;
        let source_kind = nonempty(&input.source_kind, "External project source kind")?;
        let source_version = clean_optional(input.source_version);
        let canonical_url = match provider.as_str() {
            "CHATGPT" => {
                let input_url = input
                    .canonical_url
                    .as_deref()
                    .ok_or("ChatGPT Project links require a canonical https://chatgpt.com/g/<project-id>/project URL")?;
                let (normalized, url_project_id) = normalize_chatgpt_project_url(input_url)?;
                if external_project_id != url_project_id {
                    return Err("ChatGPT Project URL identity does not match the supplied external project identity".into());
                }
                Some(normalized)
            }
            "CODEX" => {
                if input.canonical_url.is_some() {
                    return Err("Codex Project associations must use a verified native source, not a provider URL".into());
                }
                None
            }
            _ => return Err("External project provider is unsupported".into()),
        };
        let timestamp = now();
        self.with_connection(|connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            project_by_id(&transaction, project_id)?;
            let existing = external_project_link_for_project_provider(&transaction, project_id, &provider)?;
            let link = if let Some(existing) = existing {
                transaction.execute(
                    "UPDATE external_project_links SET external_project_id=?2,canonical_url=?3,label=?4,source_kind=?5,source_version=?6,verified_at=?7,updated_at=?8 WHERE id=?1",
                    params![existing.id, external_project_id, canonical_url, label, source_kind, source_version, input.verified_at, timestamp],
                ).map_err(db_error)?;
                external_project_link_by_id(&transaction, &existing.id)?
            } else {
                let link = ExternalProjectLink {
                    id: id(), project_id: project_id.into(), provider: provider.clone(),
                    external_project_id, canonical_url, label, source_kind, source_version,
                    verified_at: input.verified_at, created_at: timestamp, updated_at: timestamp,
                };
                transaction.execute(
                    "INSERT INTO external_project_links (id,project_id,provider,external_project_id,canonical_url,label,source_kind,source_version,verified_at,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                    params![link.id,link.project_id,link.provider,link.external_project_id,link.canonical_url,link.label,link.source_kind,link.source_version,link.verified_at,link.created_at,link.updated_at],
                ).map_err(db_error)?;
                link
            };
            transaction.commit().map_err(db_error)?;
            Ok(link)
        })
    }

    pub fn external_project_links_for_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<ExternalProjectLink>, String> {
        self.with_connection(|connection| {
            project_by_id(connection, project_id)?;
            list_external_project_links(connection, project_id)
        })
    }

    /// Atomically saves one or both explicit provider conversation choices.
    /// The caller must present the current binding revision and each side's
    /// exact currently ACTIVE Endpoint identity; display labels never select a
    /// provider object. A failed second side rolls back the first side.
    pub fn pair_workstream_endpoints_checked(
        &self,
        workstream_id: &str,
        request: EndpointPairingRequest,
    ) -> Result<EndpointPairingResult, String> {
        if request.chatgpt.is_none() && request.codex.is_none() {
            return Err("Endpoint pairing requires at least one explicit provider side".into());
        }
        self.with_connection(|connection| {
            role_bridge::ensure_legacy_binding(connection, workstream_id)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let workstream = workstream_by_id(&transaction, workstream_id)?;
            ensure_workstream_not_trashed(&workstream)?;
            if workstream.binding_revision != request.expected_binding_revision {
                return Err("The Workstream binding changed after this pairing review. Reopen the pairing review before saving.".into());
            }
            let timestamp = now();
            let (chatgpt_endpoint, chatgpt_changed) = pair_endpoint_side(
                &transaction,
                workstream_id,
                "CHATGPT",
                request.chatgpt,
                timestamp,
            )?;
            let (codex_endpoint, codex_changed) = pair_endpoint_side(
                &transaction,
                workstream_id,
                "CODEX",
                request.codex,
                timestamp,
            )?;
            let binding_revision = if chatgpt_changed || codex_changed {
                increment_binding_revision(&transaction, workstream_id, timestamp)?
            } else {
                workstream.binding_revision
            };
            transaction.commit().map_err(db_error)?;
            Ok(EndpointPairingResult { binding_revision, chatgpt_endpoint, codex_endpoint })
        })
    }

    /// Moves an exact active ChatGPT Endpoint only when it has no durable
    /// product activity. This preserves endpoint identity without creating a
    /// duplicate provider identity or rewriting historical records.
    pub fn transfer_active_chatgpt_endpoint_checked(
        &self,
        request: ChatGptEndpointTransferRequest,
    ) -> Result<Endpoint, String> {
        self.with_connection(|connection| {
            if migration::preflight(connection, migration::PREVIEW_SCHEMA)?
                != migration::PREVIEW_SCHEMA
            {
                return Err("EXPLICIT_PREVIEW_MIGRATION_REQUIRED".into());
            }
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let source = workstream_by_id(&transaction, &request.source_workstream_id)?;
            let target = workstream_by_id(&transaction, &request.target_workstream_id)?;
            ensure_workstream_not_trashed(&source)?;
            ensure_workstream_not_trashed(&target)?;
            if source.id == target.id
                || source.binding_revision != request.expected_source_binding_revision
                || target.binding_revision != request.expected_target_binding_revision
            {
                return Err("STALE_BINDING".into());
            }
            if active_endpoint(&transaction, &target.id, "CHATGPT")?.is_some() {
                return Err("CHATGPT_ENDPOINT_TRANSFER_TARGET_BOUND".into());
            }
            let mut endpoint = endpoint_by_id(&transaction, &request.source_endpoint_id)?;
            if endpoint.workstream_id != source.id
                || endpoint.provider != "CHATGPT"
                || endpoint.status != "ACTIVE"
                || endpoint.external_id != request.conversation_id
            {
                return Err("STALE_BINDING".into());
            }
            let has_history: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM handoffs WHERE source_endpoint_id=?1 OR destination_endpoint_id=?1) OR EXISTS(SELECT 1 FROM provider_runs WHERE endpoint_id=?1) OR EXISTS(SELECT 1 FROM reply_observations WHERE endpoint_id=?1) OR EXISTS(SELECT 1 FROM artifact_materializations WHERE source_endpoint_id=?1)",
                    params![endpoint.id],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            if has_history {
                return Err("CHATGPT_ENDPOINT_TRANSFER_HISTORY_PRESENT".into());
            }
            let timestamp = now();
            transaction
                .execute(
                    "UPDATE endpoints SET workstream_id=?2 WHERE id=?1 AND workstream_id=?3 AND provider='CHATGPT' AND status='ACTIVE'",
                    params![endpoint.id, target.id, source.id],
                )
                .map_err(db_error)?;
            increment_binding_revision(&transaction, &source.id, timestamp)?;
            increment_binding_revision(&transaction, &target.id, timestamp)?;
            transaction.commit().map_err(db_error)?;
            endpoint.workstream_id = target.id;
            Ok(endpoint)
        })
    }

    /// Archive is a reversible local presentation state. It changes no
    /// Endpoint, Handoff, ProviderRun, provider Project, or provider thread.
    pub fn archive_workstream(&self, workstream_id: &str) -> Result<Workstream, String> {
        self.with_connection(|connection| {
            let workstream = workstream_by_id(connection, workstream_id)?;
            ensure_workstream_not_trashed(&workstream)?;
            if workstream.status == "ARCHIVED" {
                return Ok(workstream);
            }
            let timestamp = now();
            connection.execute(
                "UPDATE workstreams SET status='ARCHIVED',archived_at=?2,updated_at=?2 WHERE id=?1 AND trashed_at IS NULL",
                params![workstream_id, timestamp],
            ).map_err(db_error)?;
            workstream_by_id(connection, workstream_id)
        })
    }

    /// Archive eligibility for an abandoned onboarding attempt.  It is a
    /// read-only guard: archive itself remains the established reversible
    /// presentation transition and retains every endpoint/binding record.
    pub fn workstream_has_no_activity(&self, workstream_id: &str) -> Result<bool, String> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT NOT EXISTS(SELECT 1 FROM handoffs WHERE workstream_id=?1) AND NOT EXISTS(SELECT 1 FROM provider_runs WHERE workstream_id=?1) AND NOT EXISTS(SELECT 1 FROM handoff_dispatches WHERE workstream_id=?1)",
                    params![workstream_id],
                    |row| row.get(0),
                )
                .map_err(db_error)
        })
    }

    /// Pinning is local directory presentation state. Recycle-bin records
    /// remain governed by their destructive lifecycle and cannot be promoted.
    pub fn set_workstream_pinned(
        &self,
        workstream_id: &str,
        pinned: bool,
    ) -> Result<Workstream, String> {
        self.with_connection(|connection| {
            let workstream = workstream_by_id(connection, workstream_id)?;
            ensure_workstream_not_trashed(&workstream)?;
            connection
                .execute(
                    "UPDATE workstreams SET pinned_at=?2 WHERE id=?1",
                    params![workstream_id, if pinned { Some(now()) } else { None }],
                )
                .map_err(db_error)?;
            workstream_by_id(connection, workstream_id)
        })
    }

    /// Moves a local Workstream to the recoverable recycle bin. It never
    /// deletes external provider resources or historical records.
    pub fn trash_workstream(&self, workstream_id: &str) -> Result<Workstream, String> {
        self.with_connection(|connection| {
            let transaction=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let connection=&transaction;
            let workstream = workstream_by_id(connection, workstream_id)?;
            if workstream.trashed_at.is_some() {
                return Ok(workstream);
            }
            let timestamp = now();
            connection.execute(
                "UPDATE workstreams SET lifecycle_previous_status=status,trashed_at=?2,updated_at=?2 WHERE id=?1 AND trashed_at IS NULL",
                params![workstream_id, timestamp],
            ).map_err(db_error)?;
            bridge_notifications::suspend_bound_watches(connection,workstream_id,timestamp)?;
            let updated=workstream_by_id(connection,workstream_id)?;transaction.commit().map_err(db_error)?;Ok(updated)
        })
    }

    /// Restores a trashed Workstream to its pre-trash ACTIVE/ARCHIVED state;
    /// when not trashed, it unarchives an archived Workstream. Neither path
    /// resumes a provider, reacquires a writer, or changes bindings.
    pub fn restore_workstream(&self, workstream_id: &str) -> Result<Workstream, String> {
        self.with_connection(|connection| {
            let workstream = workstream_by_id(connection, workstream_id)?;
            let timestamp = now();
            if workstream.trashed_at.is_some() {
                connection.execute(
                    "UPDATE workstreams SET status=COALESCE(lifecycle_previous_status,'ACTIVE'),lifecycle_previous_status=NULL,trashed_at=NULL,updated_at=?2 WHERE id=?1",
                    params![workstream_id, timestamp],
                ).map_err(db_error)?;
            } else if workstream.status == "ARCHIVED" {
                connection.execute(
                    "UPDATE workstreams SET status='ACTIVE',archived_at=NULL,updated_at=?2 WHERE id=?1",
                    params![workstream_id, timestamp],
                ).map_err(db_error)?;
            }
            workstream_by_id(connection, workstream_id)
        })
    }

    /// Stores only a local Router draft. A caller must present the revision it
    /// actually read, so independent desktop/mobile views cannot silently
    /// overwrite one another. Saving never creates a provider turn.
    pub fn save_workstream_draft(
        &self,
        workstream_id: &str,
        text: String,
        expected_revision: Option<i64>,
    ) -> Result<WorkstreamDraft, String> {
        if text.len() > MAX_WORKSTREAM_DRAFT_BYTES {
            return Err(format!(
                "Workstream draft exceeds the {} byte local limit",
                MAX_WORKSTREAM_DRAFT_BYTES
            ));
        }
        self.with_connection(|connection| {
            let workstream = workstream_by_id(connection, workstream_id)?;
            ensure_workstream_not_trashed(&workstream)?;
            let current = workstream_draft(connection, workstream_id)?;
            if current.as_ref().map(|draft| draft.revision) != expected_revision {
                return Err("The local draft changed in another view. Reload it before saving.".into());
            }
            let updated_at = now();
            let revision = current.map(|draft| draft.revision + 1).unwrap_or(1);
            connection.execute(
                "INSERT INTO workstream_drafts(workstream_id,text,revision,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(workstream_id) DO UPDATE SET text=excluded.text,revision=excluded.revision,updated_at=excluded.updated_at",
                params![workstream_id, text, revision, updated_at],
            ).map_err(db_error)?;
            workstream_draft(connection, workstream_id)?.ok_or("Draft was not saved".into())
        })
    }

    pub fn workstream_draft(&self, workstream_id: &str) -> Result<Option<WorkstreamDraft>, String> {
        self.with_connection(|connection| {
            workstream_by_id(connection, workstream_id)?;
            workstream_draft(connection, workstream_id)
        })
    }

    /// Stores an unsent correction against its exact retained Codex result.
    /// Revision matching prevents a desktop and mobile view from silently
    /// overwriting each other; saving never starts a Codex turn.
    pub fn save_codex_feedback_draft(
        &self,
        workstream_id: &str,
        source_run_id: &str,
        text: String,
        expected_revision: Option<i64>,
    ) -> Result<CodexFeedbackDraft, String> {
        if text.len() > MAX_WORKSTREAM_DRAFT_BYTES {
            return Err(format!(
                "Codex feedback draft exceeds the {} byte local limit",
                MAX_WORKSTREAM_DRAFT_BYTES
            ));
        }
        self.with_connection(|connection| {
            let workstream = workstream_by_id(connection, workstream_id)?;
            ensure_workstream_not_trashed(&workstream)?;
            ensure_codex_feedback_source_run(connection, workstream_id, source_run_id)?;
            let current = codex_feedback_draft(connection, workstream_id, source_run_id)?;
            if current.as_ref().map(|draft| draft.revision) != expected_revision {
                return Err("The Codex feedback draft changed in another view. Reload it before saving.".into());
            }
            let updated_at = now();
            let revision = current.map(|draft| draft.revision + 1).unwrap_or(1);
            connection.execute(
                "INSERT INTO codex_feedback_drafts(workstream_id,source_run_id,text,revision,updated_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(workstream_id,source_run_id) DO UPDATE SET text=excluded.text,revision=excluded.revision,updated_at=excluded.updated_at",
                params![workstream_id, source_run_id, text, revision, updated_at],
            ).map_err(db_error)?;
            codex_feedback_draft(connection, workstream_id, source_run_id)?.ok_or("Codex feedback draft was not saved".into())
        })
    }

    pub fn codex_feedback_draft(
        &self,
        workstream_id: &str,
        source_run_id: &str,
    ) -> Result<Option<CodexFeedbackDraft>, String> {
        self.with_connection(|connection| {
            workstream_by_id(connection, workstream_id)?;
            ensure_codex_feedback_source_run(connection, workstream_id, source_run_id)?;
            codex_feedback_draft(connection, workstream_id, source_run_id)
        })
    }

    #[cfg(test)]
    pub fn trashed_workstreams_for_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<Workstream>, String> {
        self.with_connection(|connection| {
            project_by_id(connection, project_id)?;
            list_trashed_workstreams(connection, project_id)
        })
    }

    /// Permanently removes only local Router rows after an explicit second
    /// confirmation. It is deliberately unavailable while execution/delivery
    /// is non-terminal or an exact reply is unread; provider conversations,
    /// projects, files, and browser state are never touched.
    pub fn purge_trashed_workstream_confirmed(
        &self,
        workstream_id: &str,
        expected_binding_revision: i64,
        confirmation: &str,
    ) -> Result<(), String> {
        if confirmation != "PURGE_LOCAL_WORKSTREAM" {
            return Err(
                "Permanent local purge requires the explicit PURGE_LOCAL_WORKSTREAM confirmation"
                    .into(),
            );
        }
        self.with_connection(|connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let workstream = workstream_by_id(&transaction, workstream_id)?;
            if workstream.trashed_at.is_none() {
                return Err("Only a trashed Workstream can be permanently purged".into());
            }
            if workstream.binding_revision != expected_binding_revision {
                return Err("The Workstream binding changed after the permanent-purge review".into());
            }
            ensure_workstream_purge_safe(&transaction, workstream_id)?;
            transaction.execute("DELETE FROM reply_observer_seen_identities WHERE endpoint_id IN (SELECT id FROM endpoints WHERE workstream_id=?1)", params![workstream_id]).map_err(db_error)?;
            transaction.execute("DELETE FROM reply_observations WHERE workstream_id=?1", params![workstream_id]).map_err(db_error)?;
            transaction.execute("DELETE FROM workstream_drafts WHERE workstream_id=?1", params![workstream_id]).map_err(db_error)?;
            transaction.execute("DELETE FROM codex_feedback_drafts WHERE workstream_id=?1", params![workstream_id]).map_err(db_error)?;
            transaction.execute("DELETE FROM provider_run_feedback_sources WHERE feedback_run_id IN (SELECT id FROM provider_runs WHERE workstream_id=?1) OR source_run_id IN (SELECT id FROM provider_runs WHERE workstream_id=?1)", params![workstream_id]).map_err(db_error)?;
            transaction.execute("DELETE FROM provider_runs WHERE workstream_id=?1", params![workstream_id]).map_err(db_error)?;
            transaction.execute("DELETE FROM handoff_attachments WHERE handoff_id IN (SELECT id FROM handoffs WHERE workstream_id=?1)", params![workstream_id]).map_err(db_error)?;
            transaction.execute("DELETE FROM handoffs WHERE workstream_id=?1", params![workstream_id]).map_err(db_error)?;
            transaction.execute("DELETE FROM endpoints WHERE workstream_id=?1", params![workstream_id]).map_err(db_error)?;
            transaction.execute("DELETE FROM app_settings WHERE key='last_workstream_id' AND value=?1", params![workstream_id]).map_err(db_error)?;
            if transaction.execute("DELETE FROM workstreams WHERE id=?1", params![workstream_id]).map_err(db_error)? != 1 {
                return Err("Workstream was not found for permanent purge".into());
            }
            transaction.commit().map_err(db_error)
        })
    }

    /// Creates a consistent SQLite snapshot without copying an in-flight WAL
    /// file. `VACUUM INTO` reads a SQLite-consistent view while the Store mutex
    /// protects this Router process; the result is reopened and integrity
    /// checked before its hash is returned. The destination must not exist.
    pub fn backup_before_application_update(&self)->Result<VerifiedBackup,String>{
        let file:String=self.with_connection(|c|c.query_row("SELECT file FROM pragma_database_list WHERE name='main'",[],|r|r.get(0)).map_err(db_error))?;
        let root=Path::new(&file).parent().ok_or("UPDATE_BACKUP_ROOT_UNAVAILABLE")?.join("backups");std::fs::create_dir_all(&root).map_err(|_|"UPDATE_BACKUP_FAILED")?;
        self.create_verified_backup(root.join(format!("before-app-update-{}.db",id())))
    }
    pub fn create_verified_backup(
        &self,
        target: impl AsRef<Path>,
    ) -> Result<VerifiedBackup, String> {
        let target = target.as_ref().to_path_buf();
        if target.as_os_str().is_empty() || target.exists() {
            return Err("Verified backup target must be a new, non-existing file".into());
        }
        let parent = target
            .parent()
            .ok_or("Verified backup target has no parent directory")?;
        if !parent.is_dir() {
            return Err("Verified backup target parent directory does not exist".into());
        }
        let target_text = target.to_string_lossy().to_string();
        self.with_connection(|connection| {
            connection
                .execute("VACUUM INTO ?1", params![target_text])
                .map_err(db_error)?;
            let verification = Connection::open(&target).map_err(db_error)?;
            verification
                .execute_batch("PRAGMA foreign_keys = ON;")
                .map_err(db_error)?;
            let integrity: String = verification
                .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                .map_err(db_error)?;
            if integrity != "ok" {
                return Err(format!(
                    "Verified backup integrity check failed: {integrity}"
                ));
            }
            let metadata = fs::metadata(&target)
                .map_err(|error| format!("Could not inspect verified backup: {error}"))?;
            Ok(VerifiedBackup {
                path: target_text,
                sha256: file_sha256(&target)?,
                bytes: metadata.len(),
                created_at: now(),
            })
        })
    }

    pub fn bind_endpoint(
        &self,
        workstream_id: &str,
        provider: Provider,
        external_id: String,
        label: String,
        replace: bool,
    ) -> Result<Endpoint, String> {
        let external_id = nonempty(&external_id, "External endpoint identity")?;
        let label = nonempty(&label, "Endpoint label")?;
        let provider = provider.as_str().to_string();
        let now = now();
        self.with_connection(|connection| {
            role_bridge::ensure_legacy_binding(connection, workstream_id)?;
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let workstream = workstream_by_id(&transaction, workstream_id)?;
            ensure_workstream_not_trashed(&workstream)?;
            if let Some(current) = active_endpoint(&transaction, workstream_id, &provider)? {
                if current.external_id == external_id { transaction.commit().map_err(db_error)?; return Ok(current); }
                if !replace { return Err(format!("An ACTIVE {provider} endpoint already exists. Use Replace binding.")); }
                transaction.execute("UPDATE endpoints SET status='SUPERSEDED', superseded_at=?2 WHERE id=?1 AND status='ACTIVE'", params![current.id, now]).map_err(db_error)?;
                let endpoint = Endpoint { id: id(), workstream_id: workstream_id.into(), provider, external_id, label, status: "ACTIVE".into(), replaces_endpoint_id: Some(current.id), created_at: now, superseded_at: None };
                transaction.execute("INSERT INTO endpoints (id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at,superseded_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)", endpoint_params(&endpoint)).map_err(db_error)?;
                increment_binding_revision(&transaction, workstream_id, now)?;
                transaction.commit().map_err(db_error)?; return Ok(endpoint);
            }
            let endpoint = Endpoint { id: id(), workstream_id: workstream_id.into(), provider, external_id, label, status: "ACTIVE".into(), replaces_endpoint_id: None, created_at: now, superseded_at: None };
            transaction.execute("INSERT INTO endpoints (id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at,superseded_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)", endpoint_params(&endpoint)).map_err(db_error)?;
            increment_binding_revision(&transaction, workstream_id, now)?;
            transaction.commit().map_err(db_error)?; Ok(endpoint)
        })
    }

    /// Replaces exactly the Endpoint the user reviewed.  The expected old ID
    /// is deliberately checked inside the same transaction as the supersede,
    /// insert, and optional Codex initialization audit record so a stale
    /// review cannot replace whichever binding happens to be current.
    pub fn replace_active_endpoint_checked(
        &self,
        workstream_id: &str,
        provider: Provider,
        expected_old_endpoint_id: &str,
        external_id: String,
        label: String,
        completed_rollover_run: Option<CompletedRolloverRun>,
    ) -> Result<Endpoint, String> {
        let external_id = nonempty(&external_id, "Candidate external endpoint identity")?;
        let label = nonempty(&label, "Candidate endpoint label")?;
        let provider = provider.as_str().to_string();
        if completed_rollover_run.is_some() && provider != "CODEX" {
            return Err("Only a Codex successor may create an initialization ProviderRun".into());
        }
        self.with_connection(|connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let workstream = workstream_by_id(&transaction, workstream_id)?;
            ensure_workstream_not_trashed(&workstream)?;
            let current = active_endpoint(&transaction, workstream_id, &provider)?
                .ok_or("No ACTIVE Endpoint exists for this rollover")?;
            if current.id != expected_old_endpoint_id {
                return Err("The current Endpoint changed after this rollover review. Reopen the review before replacing it.".into());
            }
            if current.external_id == external_id {
                return Err("A successor must use a new exact external identity".into());
            }
            let timestamp = now();
            transaction.execute(
                "UPDATE endpoints SET status='SUPERSEDED', superseded_at=?2 WHERE id=?1 AND status='ACTIVE'",
                params![current.id, timestamp],
            ).map_err(db_error)?;
            let endpoint = Endpoint {
                id: id(), workstream_id: workstream_id.into(), provider: provider.clone(),
                external_id, label, status: "ACTIVE".into(),
                replaces_endpoint_id: Some(current.id), created_at: timestamp, superseded_at: None,
            };
            transaction.execute(
                "INSERT INTO endpoints (id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at,superseded_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                endpoint_params(&endpoint),
            ).map_err(db_error)?;
            if let Some(run) = completed_rollover_run {
                let external_run_id = nonempty(&run.external_run_id, "Initialization turn ID")?;
                let reviewed_at = timestamp;
                transaction.execute(
                    "INSERT INTO provider_runs (id,workstream_id,endpoint_id,provider,origin_handoff_id,external_run_id,status,result_identity,terminal_code,started_at,terminal_at,reviewed_at,created_at,updated_at) VALUES (?1,?2,?3,'CODEX',NULL,?4,'COMPLETED',?5,'COMPLETED',?6,?7,?8,?9,?9)",
                    params![id(), workstream_id, endpoint.id, external_run_id, run.result_identity, run.started_at, run.terminal_at, reviewed_at, timestamp],
                ).map_err(db_error)?;
            }
            increment_binding_revision(&transaction, workstream_id, timestamp)?;
            transaction.commit().map_err(db_error)?;
            Ok(endpoint)
        })
    }

    pub fn active_endpoint_for_workstream(
        &self,
        workstream_id: &str,
        provider: Provider,
    ) -> Result<Option<Endpoint>, String> {
        self.with_connection(|connection| {
            active_endpoint(connection, workstream_id, provider.as_str())
        })
    }

    pub fn save_chatgpt_endpoint_canonical_url(&self, endpoint_id: &str, canonical_url: &str) -> Result<(), String> {
        let canonical_url = normalize_chatgpt_conversation_url(canonical_url)?;
        self.with_connection(|connection| {
            let endpoint: (String, String) = connection.query_row(
                "SELECT provider,external_id FROM endpoints WHERE id=?1 AND status='ACTIVE'", params![endpoint_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ).map_err(db_error)?;
            if endpoint.0 != "CHATGPT" || !canonical_url.ends_with(&format!("/{}", endpoint.1)) {
                return Err("CHATGPT_CANONICAL_URL_ENDPOINT_MISMATCH".into());
            }
            connection.execute(
                "INSERT INTO endpoint_canonical_urls(endpoint_id,canonical_url,updated_at) VALUES(?1,?2,?3) ON CONFLICT(endpoint_id) DO UPDATE SET canonical_url=excluded.canonical_url,updated_at=excluded.updated_at",
                params![endpoint_id, canonical_url, now()],
            ).map_err(db_error)?;
            Ok(())
        })
    }

    pub fn chatgpt_endpoint_canonical_url(&self, endpoint_id: &str) -> Result<Option<String>, String> {
        self.with_connection(|connection| connection.query_row(
            "SELECT canonical_url FROM endpoint_canonical_urls WHERE endpoint_id=?1", params![endpoint_id], |row| row.get(0),
        ).optional().map_err(db_error))
    }

    pub fn active_endpoint_for_external_id(
        &self,
        provider: &str,
        external_id: &str,
    ) -> Result<Option<Endpoint>, String> {
        self.with_connection(|connection| connection.query_row("SELECT id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at,superseded_at FROM endpoints WHERE provider=?1 AND external_id=?2 AND status='ACTIVE'",params![provider,external_id],endpoint_row).optional().map_err(db_error))
    }

    pub fn active_chatgpt_endpoints(&self) -> Result<Vec<Endpoint>, String> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at,superseded_at FROM endpoints WHERE provider='CHATGPT' AND status='ACTIVE' ORDER BY created_at,rowid",
            ).map_err(db_error)?;
            let endpoints = statement
                .query_map([], endpoint_row)
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?;
            Ok(endpoints)
        })
    }

    pub fn provider_runs_for_workstream(
        &self,
        workstream_id: &str,
    ) -> Result<Vec<ProviderRun>, String> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare("SELECT id,workstream_id,endpoint_id,provider,origin_handoff_id,external_run_id,status,result_identity,terminal_code,started_at,terminal_at,reviewed_at,created_at,updated_at,result_text FROM provider_runs WHERE workstream_id=?1 ORDER BY created_at,rowid")
                .map_err(db_error)?;
            let runs = statement
                .query_map(params![workstream_id], provider_run_row)
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?;
            Ok(runs)
        })
    }

    /// Inserts one exact assistant observation. Only a concrete adapter
    /// message ID deduplicates; text is never used to suppress a reply.
    pub fn record_reply_observation(
        &self,
        workstream_id: &str,
        endpoint_id: &str,
        assistant_identity: Option<&str>,
        text: &str,
        source_provider_run_id: Option<&str>,
    ) -> Result<Option<ReplyObservation>, String> {
        let text = nonempty(text, "Observed provider reply")?;
        if text.len() > MAX_REPLY_OBSERVATION_BYTES {
            return Err("Observed provider reply exceeds the bounded retention limit".into());
        }
        self.with_connection(|connection| {
            let endpoint = endpoint_by_id(connection, endpoint_id)?;
            if endpoint.workstream_id != workstream_id
                || !["CHATGPT", "CODEX"].contains(&endpoint.provider.as_str())
                || endpoint.status != "ACTIVE"
            {
                return Err("Reply observation requires the ACTIVE provider Endpoint for this Workstream".into());
            }
            let record = ReplyObservation { id: id(), workstream_id: workstream_id.into(), endpoint_id: endpoint_id.into(), assistant_identity: assistant_identity.map(str::to_string), text: text.to_string(), source_provider_run_id: source_provider_run_id.map(str::to_string), observed_at: now(), completed_at:None,completion_checked:false,read_at: None, handled_at: None, push_state: "PENDING".into(), push_attempted_at: None, push_rendered_at: None };
            let changed = connection.execute(
                "INSERT OR IGNORE INTO reply_observations(id,workstream_id,endpoint_id,assistant_identity,text,source_provider_run_id,observed_at,read_at,handled_at,push_state,push_attempted_at,push_rendered_at) VALUES(?1,?2,?3,?4,?5,?6,?7,NULL,NULL,?8,NULL,NULL)",
                params![record.id,record.workstream_id,record.endpoint_id,record.assistant_identity,record.text,record.source_provider_run_id,record.observed_at,record.push_state],
            ).map_err(db_error)?;
            // Unread observations are delivery authority and are never
            // trimmed.  Retain only the newest bounded handled/read history.
            connection.execute("DELETE FROM reply_observations WHERE endpoint_id=?1 AND read_at IS NOT NULL AND id NOT IN (SELECT id FROM reply_observations WHERE endpoint_id=?1 AND read_at IS NOT NULL ORDER BY observed_at DESC,rowid DESC LIMIT 20)", params![endpoint_id]).map_err(db_error)?;
            Ok((changed == 1).then_some(record))
        })
    }

    /// The existing bounded exact-identity journal also persists the passive
    /// Codex watermark. The encoded value contains no reply text and is scoped
    /// to one endpoint; it avoids creating a parallel migration domain.
    pub fn codex_reply_observer_watermark(
        &self,
        endpoint_id: &str,
    ) -> Result<Option<(String, i64)>, String> {
        self.with_connection(|connection| {
            connection.query_row(
                "SELECT assistant_identity,seen_at FROM reply_observer_seen_identities WHERE endpoint_id=?1 AND assistant_identity LIKE 'codex-watermark:%' ORDER BY seen_at DESC,rowid DESC LIMIT 1",
                params![endpoint_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ).optional().map_err(db_error)
        })
    }

    /// A thread with no completed answer still needs a durable baseline.
    /// Otherwise its first future answer would be incorrectly classified as
    /// pre-existing history after a Host restart.  This marker carries no
    /// reply content or synthetic provider identity.
    pub fn codex_reply_observer_is_initialized(&self, endpoint_id: &str) -> Result<bool, String> {
        self.with_connection(|connection| {
            connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM reply_observer_seen_identities WHERE endpoint_id=?1 AND (assistant_identity LIKE 'codex-watermark:%' OR assistant_identity LIKE 'codex-baseline:%'))",
                params![endpoint_id],
                |row| row.get(0),
            ).map_err(db_error)
        })
    }

    pub fn save_codex_reply_observer_empty_baseline(
        &self,
        workstream_id: &str,
        endpoint_id: &str,
        thread_id: &str,
    ) -> Result<(), String> {
        self.with_connection(|connection| {
            let endpoint = endpoint_by_id(connection, endpoint_id)?;
            if endpoint.workstream_id != workstream_id || endpoint.provider != "CODEX"
                || endpoint.status != "ACTIVE" || endpoint.external_id != thread_id
            {
                return Err("Codex observer baseline requires the exact ACTIVE Codex Endpoint".into());
            }
            connection.execute(
                "INSERT INTO reply_observer_seen_identities(endpoint_id,assistant_identity,seen_at) VALUES(?1,?2,?3) ON CONFLICT(endpoint_id,assistant_identity) DO UPDATE SET seen_at=excluded.seen_at",
                params![endpoint_id, format!("codex-baseline:{thread_id}:empty"), now()],
            ).map_err(db_error)?;
            Ok(())
        })
    }

    pub fn save_codex_reply_observer_watermark(
        &self,
        workstream_id: &str,
        endpoint_id: &str,
        thread_id: &str,
        completed_turn_id: &str,
        agent_item_id: &str,
    ) -> Result<(), String> {
        self.with_connection(|connection| {
            let endpoint = endpoint_by_id(connection, endpoint_id)?;
            if endpoint.workstream_id != workstream_id || endpoint.provider != "CODEX"
                || endpoint.status != "ACTIVE" || endpoint.external_id != thread_id
            {
                return Err("Codex observer watermark requires the exact ACTIVE Codex Endpoint".into());
            }
            let identity = format!("codex-watermark:{thread_id}:{completed_turn_id}:{agent_item_id}");
            connection.execute(
                "INSERT INTO reply_observer_seen_identities(endpoint_id,assistant_identity,seen_at) VALUES(?1,?2,?3) ON CONFLICT(endpoint_id,assistant_identity) DO UPDATE SET seen_at=excluded.seen_at",
                params![endpoint_id, identity, now()],
            ).map_err(db_error)?;
            connection.execute(
                "DELETE FROM reply_observer_seen_identities WHERE endpoint_id=?1 AND (assistant_identity LIKE 'codex-watermark:%' OR assistant_identity LIKE 'codex-baseline:%') AND assistant_identity NOT IN (SELECT assistant_identity FROM reply_observer_seen_identities WHERE endpoint_id=?1 AND assistant_identity LIKE 'codex-watermark:%' ORDER BY seen_at DESC,rowid DESC LIMIT 1)",
                params![endpoint_id],
            ).map_err(db_error)?;
            Ok(())
        })
    }

    /// The normal-Chrome ChatGPT observer has the same durable, exact-ID
    /// baseline semantics as the Codex observer.  It retains only a provider
    /// message identity, never reply text or browser/profile data.
    pub fn chatgpt_reply_observer_watermark(
        &self,
        endpoint_id: &str,
    ) -> Result<Option<(String, i64)>, String> {
        self.with_connection(|connection| {
            connection.query_row(
                "SELECT assistant_identity,seen_at FROM reply_observer_seen_identities WHERE endpoint_id=?1 AND assistant_identity LIKE 'chatgpt-watermark:%' ORDER BY seen_at DESC,rowid DESC LIMIT 1",
                params![endpoint_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ).optional().map_err(db_error)
        })
    }

    pub fn chatgpt_reply_observer_is_initialized(&self, endpoint_id: &str) -> Result<bool, String> {
        self.with_connection(|connection| {
            connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM reply_observer_seen_identities WHERE endpoint_id=?1 AND (assistant_identity LIKE 'chatgpt-watermark:%' OR assistant_identity LIKE 'chatgpt-baseline:%'))",
                params![endpoint_id],
                |row| row.get(0),
            ).map_err(db_error)
        })
    }

    pub fn save_chatgpt_reply_observer_empty_baseline(
        &self,
        workstream_id: &str,
        endpoint_id: &str,
        conversation_id: &str,
    ) -> Result<(), String> {
        self.with_connection(|connection| {
            let endpoint = endpoint_by_id(connection, endpoint_id)?;
            if endpoint.workstream_id != workstream_id || endpoint.provider != "CHATGPT"
                || endpoint.status != "ACTIVE" || endpoint.external_id != conversation_id
            {
                return Err("ChatGPT observer baseline requires the exact ACTIVE ChatGPT Endpoint".into());
            }
            connection.execute(
                "INSERT INTO reply_observer_seen_identities(endpoint_id,assistant_identity,seen_at) VALUES(?1,?2,?3) ON CONFLICT(endpoint_id,assistant_identity) DO UPDATE SET seen_at=excluded.seen_at",
                params![endpoint_id, format!("chatgpt-baseline:{conversation_id}:empty"), now()],
            ).map_err(db_error)?;
            Ok(())
        })
    }

    pub fn save_chatgpt_reply_observer_watermark(
        &self,
        workstream_id: &str,
        endpoint_id: &str,
        conversation_id: &str,
        message_id: &str,
    ) -> Result<(), String> {
        self.with_connection(|connection| {
            let endpoint = endpoint_by_id(connection, endpoint_id)?;
            if endpoint.workstream_id != workstream_id || endpoint.provider != "CHATGPT"
                || endpoint.status != "ACTIVE" || endpoint.external_id != conversation_id
            {
                return Err("ChatGPT observer watermark requires the exact ACTIVE ChatGPT Endpoint".into());
            }
            let identity = format!("chatgpt-watermark:{conversation_id}:{message_id}");
            connection.execute(
                "INSERT INTO reply_observer_seen_identities(endpoint_id,assistant_identity,seen_at) VALUES(?1,?2,?3) ON CONFLICT(endpoint_id,assistant_identity) DO UPDATE SET seen_at=excluded.seen_at",
                params![endpoint_id, identity, now()],
            ).map_err(db_error)?;
            connection.execute(
                "DELETE FROM reply_observer_seen_identities WHERE endpoint_id=?1 AND (assistant_identity LIKE 'chatgpt-watermark:%' OR assistant_identity LIKE 'chatgpt-baseline:%') AND assistant_identity NOT IN (SELECT assistant_identity FROM reply_observer_seen_identities WHERE endpoint_id=?1 AND assistant_identity LIKE 'chatgpt-watermark:%' ORDER BY seen_at DESC,rowid DESC LIMIT 1)",
                params![endpoint_id],
            ).map_err(db_error)?;
            Ok(())
        })
    }

    pub fn reply_observer_cursor(&self) -> Result<Option<ReplyObserverCursor>, String> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT stream_epoch,last_sequence FROM reply_observer_stream_cursor WHERE id=1",
                    [],
                    |row| Ok(ReplyObserverCursor { stream_epoch: row.get(0)?, last_sequence: row.get::<_, i64>(1)?.max(0) as u64 }),
                )
                .optional()
                .map_err(db_error)
        })
    }

    pub fn save_reply_observer_cursor(
        &self,
        stream_epoch: &str,
        last_sequence: u64,
    ) -> Result<(), String> {
        let stream_epoch = nonempty(stream_epoch, "Observed-turn stream epoch")?;
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO reply_observer_stream_cursor(id,stream_epoch,last_sequence,updated_at) VALUES(1,?1,?2,?3) ON CONFLICT(id) DO UPDATE SET stream_epoch=excluded.stream_epoch,last_sequence=excluded.last_sequence,updated_at=excluded.updated_at",
                params![stream_epoch, last_sequence.min(i64::MAX as u64) as i64, now()],
            ).map_err(db_error)?;
            Ok(())
        })
    }

    /// Returns whether the exact identity has already been seeded or observed.
    /// The bounded record deliberately excludes reply text and stream payloads.
    pub fn remember_reply_observer_identity(
        &self,
        endpoint_id: &str,
        assistant_identity: &str,
    ) -> Result<bool, String> {
        let assistant_identity = nonempty(assistant_identity, "Observed assistant identity")?;
        self.with_connection(|connection| {
            let inserted = connection.execute(
                "INSERT OR IGNORE INTO reply_observer_seen_identities(endpoint_id,assistant_identity,seen_at) VALUES(?1,?2,?3)",
                params![endpoint_id, assistant_identity, now()],
            ).map_err(db_error)? == 1;
            connection.execute(
                "DELETE FROM reply_observer_seen_identities WHERE endpoint_id=?1 AND assistant_identity NOT IN (SELECT assistant_identity FROM reply_observer_seen_identities WHERE endpoint_id=?1 ORDER BY seen_at DESC,rowid DESC LIMIT 200)",
                params![endpoint_id],
            ).map_err(db_error)?;
            Ok(inserted)
        })
    }

    pub fn reply_observer_identity_seen(
        &self,
        endpoint_id: &str,
        assistant_identity: &str,
    ) -> Result<bool, String> {
        self.with_connection(|connection| {
            connection.query_row(
                "SELECT 1 FROM reply_observer_seen_identities WHERE endpoint_id=?1 AND assistant_identity=?2",
                params![endpoint_id, assistant_identity],
                |_| Ok(()),
            ).optional().map_err(db_error).map(|value| value.is_some())
        })
    }

    pub fn reply_observations_for_workstream(
        &self,
        workstream_id: &str,
    ) -> Result<Vec<ReplyObservation>, String> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare("SELECT id,workstream_id,endpoint_id,assistant_identity,text,source_provider_run_id,observed_at,read_at,handled_at,push_state,push_attempted_at,push_rendered_at,completed_at,completion_checked FROM reply_observations WHERE workstream_id=?1 AND (read_at IS NULL OR id IN (SELECT id FROM reply_observations WHERE workstream_id=?1 AND read_at IS NOT NULL ORDER BY observed_at DESC,rowid DESC LIMIT 20)) ORDER BY (read_at IS NULL) DESC,observed_at DESC,rowid DESC").map_err(db_error)?;
            let observations = statement
                .query_map(params![workstream_id], reply_observation_row)
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?;
            Ok(observations)
        })
    }

    pub fn note_reply_completion(&self,workstream:&str,endpoint:&str,identity:&str,completed:Option<i64>)->Result<bool,String>{
        if completed.is_some_and(|v|v<=0||v>253402300799000){return Err("REPLY_COMPLETION_INVALID".into());}
        self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let prior:Option<Option<i64>>=tx.query_row("SELECT completed_at FROM reply_observations WHERE workstream_id=?1 AND endpoint_id=?2 AND assistant_identity=?3",params![workstream,endpoint,identity],|r|r.get(0)).optional().map_err(db_error)?;
            let Some(prior)=prior else{return Ok(false)};
            if prior.zip(completed).is_some_and(|(a,b)|a!=b){return Err("REPLY_COMPLETION_CHANGED".into());}
            tx.execute("UPDATE reply_observations SET completed_at=COALESCE(completed_at,?4),completion_checked=1 WHERE workstream_id=?1 AND endpoint_id=?2 AND assistant_identity=?3",params![workstream,endpoint,identity,completed]).map_err(db_error)?;
            mcp_events::completed(&tx,workstream,endpoint,identity)?;
            tx.commit().map_err(db_error)?;Ok(true)
        })
    }
    pub fn acknowledge_reply_observation(
        &self,
        workstream_id: &str,
        observation_id: &str,
        handled: bool,
    ) -> Result<(), String> {
        self.with_connection(|connection| {
            let timestamp = now();
            let changed = connection.execute(if handled { "UPDATE reply_observations SET read_at=COALESCE(read_at,?3),handled_at=COALESCE(handled_at,?3) WHERE id=?1 AND workstream_id=?2" } else { "UPDATE reply_observations SET read_at=COALESCE(read_at,?3) WHERE id=?1 AND workstream_id=?2" }, params![observation_id,workstream_id,timestamp]).map_err(db_error)?;
            if changed == 1 { Ok(()) } else { Err("Reply observation was not found in this Workstream".into()) }
        })
    }

    pub fn record_reply_push_attempt(
        &self,
        observation_id: &str,
        state: &str,
    ) -> Result<(), String> {
        if !["SENT", "FAILED", "NO_SUBSCRIPTION"].contains(&state) {
            return Err("Unsupported ReplyObservation push state".into());
        }
        self.with_connection(|connection| {
            let changed = connection
                .execute(
                    "UPDATE reply_observations SET push_state=?2,push_attempted_at=?3 WHERE id=?1",
                    params![observation_id, state, now()],
                )
                .map_err(db_error)?;
            if changed == 1 {
                Ok(())
            } else {
                Err("Reply observation was not found".into())
            }
        })
    }

    /// Records only that the authenticated same-origin PWA Service Worker
    /// completed `showNotification` for this exact delivered observation. It
    /// cannot turn a failed/no-subscription attempt into a rendered receipt.
    pub fn record_reply_push_rendered(
        &self,
        workstream_id: &str,
        observation_id: &str,
    ) -> Result<(), String> {
        self.with_connection(|connection| {
            let changed = connection
                .execute(
                    "UPDATE reply_observations SET push_rendered_at=COALESCE(push_rendered_at,?3) WHERE id=?1 AND workstream_id=?2 AND push_state='SENT'",
                    params![observation_id, workstream_id, now()],
                )
                .map_err(db_error)?;
            if changed == 1 {
                Ok(())
            } else {
                Err("A rendered Push receipt requires this exact delivered ReplyObservation".into())
            }
        })
    }

    pub fn workstream_name(&self, workstream_id: &str) -> Result<String, String> {
        self.with_connection(|connection| Ok(workstream_by_id(connection, workstream_id)?.name))
    }

    pub fn upsert_push_subscription_metadata(&self, fingerprint: &str) -> Result<(), String> {
        self.with_connection(|connection| { connection.execute("INSERT INTO push_subscriptions(fingerprint,status,created_at,updated_at,last_push_at,last_push_state) VALUES(?1,'ACTIVE',?2,?2,NULL,NULL) ON CONFLICT(fingerprint) DO UPDATE SET status='ACTIVE',updated_at=excluded.updated_at", params![fingerprint,now()]).map_err(db_error)?; Ok(()) })
    }

    pub fn deactivate_push_subscription_metadata(&self, fingerprint: &str) -> Result<(), String> {
        self.with_connection(|connection| { connection.execute("UPDATE push_subscriptions SET status='REMOVED',updated_at=?2 WHERE fingerprint=?1", params![fingerprint,now()]).map_err(db_error)?; Ok(()) })
    }

    pub fn record_push_subscription_attempt(&self, state: &str) -> Result<(), String> {
        if !["SENT", "FAILED", "NO_SUBSCRIPTION"].contains(&state) {
            return Err("Unsupported Web Push delivery state".into());
        }
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE push_subscriptions SET last_push_at=?1,last_push_state=?2,updated_at=?1 WHERE status='ACTIVE'",
                    params![now(), state],
                )
                .map_err(db_error)?;
            Ok(())
        })
    }

    pub fn create_ready_handoff(&self, handoff: NewHandoff) -> Result<HandoffHistoryItem, String> {
        if handoff.original_text.trim().is_empty() || handoff.approved_text.trim().is_empty() {
            return Err("A persisted handoff requires original and approved text".into());
        }
        self.with_connection(|connection| {
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let source = endpoint_by_id(&transaction, &handoff.source_endpoint_id)?; let destination = endpoint_by_id(&transaction, &handoff.destination_endpoint_id)?;
            if source.workstream_id != handoff.workstream_id || destination.workstream_id != handoff.workstream_id { return Err("Handoff endpoints must belong to the selected Workstream".into()); }
            let record_id = id(); let timestamp = now(); let hash = payload_hash(&handoff.source_response_identity, &handoff.destination_endpoint_id, &handoff.approved_text);
            transaction.execute("INSERT INTO handoffs (id,workstream_id,source_endpoint_id,destination_endpoint_id,direction,source_response_identity,original_text,approved_text,status,payload_hash,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'READY',?9,?10)", params![record_id, handoff.workstream_id, handoff.source_endpoint_id, handoff.destination_endpoint_id, handoff.direction, handoff.source_response_identity, handoff.original_text, handoff.approved_text, hash, timestamp]).map_err(db_error)?;
            for attachment in handoff.attachments { transaction.execute("INSERT INTO handoff_attachments (id,handoff_id,filename,original_path,size,sha256,integrity_status,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![attachment.id, record_id, attachment.filename, attachment.original_path, attachment.size, attachment.sha256, attachment.integrity_status, timestamp]).map_err(db_error)?; }
            transaction.commit().map_err(db_error)?; handoff_by_id(connection, &record_id)
        })
    }

    pub fn create_mobile_codex_outbound_review(&self, review: &MobileCodexOutboundReviewRecord) -> Result<(), String> {
        if review.action_id.trim().is_empty() || review.workstream_id.trim().is_empty() || review.source_reference_id.trim().is_empty() || review.source_endpoint_id.trim().is_empty() || review.source_codex_thread_id.trim().is_empty() || review.destination_chatgpt_conversation_id.trim().is_empty() || review.original_text.trim().is_empty() || review.attachments_json.trim().is_empty() || review.revision < 1 || !matches!(review.status.as_str(), "READY" | "APPROVED" | "SENDING" | "SENT" | "FAILED") {
            return Err("Invalid mobile Codex outbound review".into());
        }
        self.with_connection(|connection| {
            let source = endpoint_by_id(connection, &review.source_endpoint_id)?;
            if source.workstream_id != review.workstream_id || source.provider != "CODEX" || source.external_id != review.source_codex_thread_id || source.status != "ACTIVE" {
                return Err("Mobile Codex review source is no longer the ACTIVE exact Endpoint".into());
            }
            let destination = active_endpoint(connection, &review.workstream_id, "CHATGPT")?.ok_or("Mobile Codex review has no ACTIVE ChatGPT destination")?;
            if destination.external_id != review.destination_chatgpt_conversation_id {
                return Err("Mobile Codex review destination changed before it was saved".into());
            }
            connection.execute("INSERT INTO mobile_codex_outbound_reviews(action_id,workstream_id,source_run_id,source_reference_id,source_endpoint_id,source_codex_thread_id,destination_chatgpt_conversation_id,original_text,attachments_json,approved_text,revision,status,handoff_id,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)", params![review.action_id, review.workstream_id, review.source_run_id, review.source_reference_id, review.source_endpoint_id, review.source_codex_thread_id, review.destination_chatgpt_conversation_id, review.original_text, review.attachments_json, review.approved_text, review.revision, review.status, review.handoff_id, review.created_at, review.updated_at]).map_err(db_error)?;
            Ok(())
        })
    }

    pub fn create_mobile_chatgpt_inbound_review(&self, review: &MobileChatGptInboundReviewRecord) -> Result<(), String> {
        if review.action_id.trim().is_empty() || review.workstream_id.trim().is_empty() || review.source_reference_id.trim().is_empty() || review.source_endpoint_id.trim().is_empty() || review.source_chatgpt_conversation_id.trim().is_empty() || review.source_response_identity.trim().is_empty() || review.destination_codex_thread_id.trim().is_empty() || review.original_text.trim().is_empty() || review.attachments_json.trim().is_empty() || review.revision < 1 || !matches!(review.status.as_str(), "READY" | "APPROVED" | "SENDING" | "SENT" | "FAILED") {
            return Err("Invalid mobile ChatGPT inbound review".into());
        }
        self.with_connection(|connection| {
            let source = endpoint_by_id(connection, &review.source_endpoint_id)?;
            if source.workstream_id != review.workstream_id || source.provider != "CHATGPT" || source.external_id != review.source_chatgpt_conversation_id || source.status != "ACTIVE" {
                return Err("Mobile ChatGPT review source is no longer the ACTIVE exact Endpoint".into());
            }
            let destination = active_endpoint(connection, &review.workstream_id, "CODEX")?.ok_or("Mobile ChatGPT review has no ACTIVE Codex destination")?;
            if destination.external_id != review.destination_codex_thread_id {
                return Err("Mobile ChatGPT review destination changed before it was saved".into());
            }
            connection.execute("INSERT INTO mobile_chatgpt_inbound_reviews(action_id,workstream_id,source_run_id,source_reference_id,source_endpoint_id,source_chatgpt_conversation_id,source_response_identity,destination_codex_thread_id,original_text,attachments_json,approved_text,revision,status,handoff_id,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)", params![review.action_id, review.workstream_id, review.source_run_id, review.source_reference_id, review.source_endpoint_id, review.source_chatgpt_conversation_id, review.source_response_identity, review.destination_codex_thread_id, review.original_text, review.attachments_json, review.approved_text, review.revision, review.status, review.handoff_id, review.created_at, review.updated_at]).map_err(db_error)?;
            Ok(())
        })
    }

    pub fn mobile_chatgpt_inbound_review(&self, action_id: &str) -> Result<Option<MobileChatGptInboundReviewRecord>, String> {
        self.with_connection(|connection| connection.query_row("SELECT action_id,workstream_id,source_run_id,source_reference_id,source_endpoint_id,source_chatgpt_conversation_id,source_response_identity,destination_codex_thread_id,original_text,attachments_json,approved_text,revision,status,handoff_id,created_at,updated_at FROM mobile_chatgpt_inbound_reviews WHERE action_id=?1", params![action_id], |row| Ok(MobileChatGptInboundReviewRecord { action_id: row.get(0)?, workstream_id: row.get(1)?, source_run_id: row.get(2)?, source_reference_id: row.get(3)?, source_endpoint_id: row.get(4)?, source_chatgpt_conversation_id: row.get(5)?, source_response_identity: row.get(6)?, destination_codex_thread_id: row.get(7)?, original_text: row.get(8)?, attachments_json: row.get(9)?, approved_text: row.get(10)?, revision: row.get(11)?, status: row.get(12)?, handoff_id: row.get(13)?, created_at: row.get(14)?, updated_at: row.get(15)? })).optional().map_err(db_error))
    }

    pub fn update_mobile_chatgpt_inbound_review(&self, review: &MobileChatGptInboundReviewRecord) -> Result<(), String> {
        if review.revision < 1 || !matches!(review.status.as_str(), "READY" | "APPROVED" | "SENDING" | "SENT" | "FAILED") { return Err("Invalid mobile ChatGPT inbound review update".into()); }
        self.with_connection(|connection| {
            let updated = connection.execute("UPDATE mobile_chatgpt_inbound_reviews SET attachments_json=?2,approved_text=?3,revision=?4,status=?5,handoff_id=?6,updated_at=?7 WHERE action_id=?1", params![review.action_id, review.attachments_json, review.approved_text, review.revision, review.status, review.handoff_id, review.updated_at]).map_err(db_error)?;
            if updated != 1 { return Err("Mobile ChatGPT inbound review was not found".into()); }
            Ok(())
        })
    }

    pub fn mobile_codex_outbound_review(&self, action_id: &str) -> Result<Option<MobileCodexOutboundReviewRecord>, String> {
        self.with_connection(|connection| connection.query_row("SELECT action_id,workstream_id,source_run_id,source_reference_id,source_endpoint_id,source_codex_thread_id,destination_chatgpt_conversation_id,original_text,attachments_json,approved_text,revision,status,handoff_id,created_at,updated_at FROM mobile_codex_outbound_reviews WHERE action_id=?1", params![action_id], |row| Ok(MobileCodexOutboundReviewRecord { action_id: row.get(0)?, workstream_id: row.get(1)?, source_run_id: row.get(2)?, source_reference_id: row.get(3)?, source_endpoint_id: row.get(4)?, source_codex_thread_id: row.get(5)?, destination_chatgpt_conversation_id: row.get(6)?, original_text: row.get(7)?, attachments_json: row.get(8)?, approved_text: row.get(9)?, revision: row.get(10)?, status: row.get(11)?, handoff_id: row.get(12)?, created_at: row.get(13)?, updated_at: row.get(14)? })).optional().map_err(db_error))
    }

    pub fn update_mobile_codex_outbound_review(&self, review: &MobileCodexOutboundReviewRecord) -> Result<(), String> {
        if review.revision < 1 || !matches!(review.status.as_str(), "READY" | "APPROVED" | "SENDING" | "SENT" | "FAILED") { return Err("Invalid mobile Codex outbound review update".into()); }
        self.with_connection(|connection| {
            let updated = connection.execute("UPDATE mobile_codex_outbound_reviews SET approved_text=?2,revision=?3,status=?4,handoff_id=?5,updated_at=?6 WHERE action_id=?1", params![review.action_id, review.approved_text, review.revision, review.status, review.handoff_id, review.updated_at]).map_err(db_error)?;
            if updated != 1 { return Err("Mobile Codex outbound review was not found".into()); }
            Ok(())
        })
    }

    /// Stores all final local rehashes together and only while their reviewed
    /// attachment rows remain unchanged.
    pub fn record_attachment_send_evidence(
        &self,
        handoff_id: &str,
        evidence: &[AttachmentSendEvidence],
    ) -> Result<(), String> {
        if evidence.is_empty() {
            return Ok(());
        }
        self.with_connection(|connection| {
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let count: i64 = transaction.query_row("SELECT COUNT(*) FROM handoff_attachments WHERE handoff_id=?1", params![handoff_id], |row| row.get(0)).map_err(db_error)?;
            if count != evidence.len() as i64 { return Err("The approved attachment set changed before dispatch".into()); }
            for item in evidence {
                let unchanged: i64 = transaction.query_row("SELECT COUNT(*) FROM handoff_attachments WHERE id=?1 AND handoff_id=?2 AND sha256 IS ?3 AND integrity_status IS ?4 AND send_sha256 IS NULL AND send_verified_at IS NULL", params![item.attachment_id, handoff_id, item.review_sha256, item.integrity_status], |row| row.get(0)).map_err(db_error)?;
                if unchanged != 1 { return Err("The approved attachment record changed before dispatch".into()); }
            }
            let verified_at = now();
            for item in evidence {
                let updated = transaction.execute("UPDATE handoff_attachments SET send_sha256=?3,send_verified_at=?4 WHERE id=?1 AND handoff_id=?2 AND send_sha256 IS NULL AND send_verified_at IS NULL", params![item.attachment_id, handoff_id, item.send_sha256, verified_at]).map_err(db_error)?;
                if updated != 1 { return Err("The approved attachment record changed while recording pre-dispatch evidence".into()); }
            }
            transaction.commit().map_err(db_error)
        })
    }

    pub fn transition_handoff(
        &self,
        handoff_id: &str,
        status: &str,
        error: Option<(String, String)>,
    ) -> Result<(), String> {
        if !["APPROVED", "SENDING", "SENT", "FAILED", "CANCELLED"].contains(&status) {
            return Err("Unsupported persisted handoff status".into());
        }
        let timestamp = now();
        self.with_connection(|connection| {
            let transaction=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let connection=&transaction;
            if migration::preflight(connection, migration::PREVIEW_SCHEMA)? >= 14 {
                let source_kind:String=connection.query_row("SELECT source_kind FROM handoffs WHERE id=?1",params![handoff_id],|r|r.get(0)).map_err(db_error)?;
                if source_kind != "ENDPOINT" { return Err("CONTROL_CONTEXT_REQUIRES_REVIEW_SERVICE".into()); }
            }
            let current: String = connection
                .query_row(
                    "SELECT status FROM handoffs WHERE id=?1",
                    params![handoff_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_error)?
                .ok_or("Persisted handoff not found")?;
            if !valid_handoff_transition(&current, status) {
                return Err(format!(
                    "Invalid persisted handoff transition: {current} -> {status}"
                ));
            }
            connection.execute("UPDATE handoffs SET status=?2, approved_at=CASE WHEN ?2='APPROVED' THEN ?3 ELSE approved_at END, sent_at=CASE WHEN ?2='SENT' THEN ?3 ELSE sent_at END, failed_at=CASE WHEN ?2='FAILED' THEN ?3 ELSE failed_at END, error_code=CASE WHEN ?2='SENT' THEN NULL WHEN ?2='FAILED' THEN ?4 ELSE error_code END, error_message=CASE WHEN ?2='SENT' THEN NULL WHEN ?2='FAILED' THEN ?5 ELSE error_message END WHERE id=?1", params![handoff_id,status,timestamp,error.as_ref().map(|value| &value.0),error.as_ref().map(|value| &value.1)]).map_err(db_error)?;
            if status=="SENT"{bridge_notifications::mark_sent_source_handled(connection,handoff_id,timestamp)?;}
            transaction.commit().map_err(db_error)
        })
    }

    /// Retains a bounded, non-terminal explanation for a Handoff whose
    /// delivery may have reached the Bridge but lacks Router's required exact
    /// request identity. This never changes the Handoff state or fabricates a
    /// correlation; a later exact `SENT` transition clears this disclosure.
    pub fn record_sending_handoff_uncertainty(
        &self,
        handoff_id: &str,
        code: &str,
        message: &str,
    ) -> Result<(), String> {
        let code = nonempty(code, "Handoff uncertainty code")?;
        let message = nonempty(message, "Handoff uncertainty message")?;
        self.with_connection(|connection| {
            let changed = connection
                .execute(
                    "UPDATE handoffs SET error_code=?2,error_message=?3 WHERE id=?1 AND status='SENDING' AND bridge_request_id IS NULL",
                    params![handoff_id, code, message],
                )
                .map_err(db_error)?;
            if changed != 1 {
                return Err("Handoff is no longer eligible for an uncertainty disclosure".into());
            }
            Ok(())
        })
    }

    /// Stores only a Bridge-issued correlation identifier after an outbound
    /// request has been accepted. It never fabricates or backfills history.
    #[allow(dead_code)] // retained for V0-005 legacy history and focused tests
    pub fn attach_bridge_correlation(
        &self,
        handoff_id: &str,
        request_id: &str,
        turn_key: Option<&str>,
    ) -> Result<(), String> {
        let request_id = nonempty(request_id, "Bridge request ID")?;
        let turn_key = clean_optional(turn_key.map(str::to_string));
        self.with_connection(|connection| {
            let current: (String, String, Option<String>, Option<String>) = connection
                .query_row(
                    "SELECT status,direction,bridge_request_id,bridge_turn_key FROM handoffs WHERE id=?1",
                    params![handoff_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(db_error)?
                .ok_or("Persisted handoff not found")?;
            if current.0 != "SENDING" || current.1 != "CODEX_TO_CHATGPT" {
                return Err("Bridge correlation can only be attached to a sending Codex-to-ChatGPT handoff".into());
            }
            if let Some(existing) = current.2 {
                if existing == request_id {
                    if let (Some(existing_turn_key), Some(observed_turn_key)) = (current.3, turn_key.as_deref())
                    {
                        if existing_turn_key != observed_turn_key {
                            return Err("Bridge correlation turn identity changed after it was stored".into());
                        }
                    }
                    return Ok(());
                }
                return Err("A different Bridge request is already correlated to this handoff".into());
            }
            connection
                .execute(
                    "UPDATE handoffs SET bridge_request_id=?2,bridge_turn_key=?3,bridge_correlation_observed_at=?4 WHERE id=?1",
                    params![handoff_id, request_id, turn_key, now()],
                )
                .map_err(db_error)?;
            Ok(())
        })
    }

    /// Atomically records the first accepted Bridge request against both sides
    /// of a V0-006 outbound execution. This closes the restart window between
    /// Handoff correlation and ProviderRun identity attachment.
    pub fn attach_bridge_correlation_and_provider_run(
        &self,
        handoff_id: &str,
        provider_run_id: &str,
        request_id: &str,
        turn_key: Option<&str>,
    ) -> Result<(), String> {
        let request_id = nonempty(request_id, "Bridge request ID")?;
        let turn_key = clean_optional(turn_key.map(str::to_string));
        self.with_connection(|connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let handoff = handoff_by_id(&transaction, handoff_id)?;
            if handoff.status != "SENDING" || handoff.direction != "CODEX_TO_CHATGPT" {
                return Err("Bridge correlation can only be attached to a sending Codex-to-ChatGPT handoff".into());
            }
            if let Some(existing) = handoff.bridge_request_id.as_deref() {
                if existing != request_id {
                    return Err("A different Bridge request is already correlated to this handoff".into());
                }
                if let (Some(stored), Some(observed)) =
                    (handoff.bridge_turn_key.as_deref(), turn_key.as_deref())
                {
                    if stored != observed {
                        return Err("Bridge correlation turn identity changed after it was stored".into());
                    }
                }
            }
            let run = provider_run_by_id(&transaction, provider_run_id)?;
            if run.origin_handoff_id.as_deref() != Some(handoff_id) || run.provider != "CHATGPT" {
                return Err("ProviderRun does not belong to this ChatGPT Handoff".into());
            }
            if let Some(existing) = run.external_run_id.as_deref() {
                if existing != request_id {
                    return Err("ProviderRun identity differs from the accepted Bridge request".into());
                }
            }
            // A bounded acceptance observer may have truthfully marked this
            // same reverse delivery UNKNOWN before a late SSE frame reaches
            // Router. Only that named, non-terminal state is recoverable by
            // its exact Bridge identity; every other UNKNOWN remains
            // fail-closed and cannot be backfilled.
            let can_attach_after_acceptance_timeout = run.status == "UNKNOWN"
                && run.external_run_id.is_none()
                && run.terminal_code.as_deref() == Some("CHATGPT_ACCEPTANCE_UNPROVEN");
            if !["STARTING", "RUNNING"].contains(&run.status.as_str())
                && !can_attach_after_acceptance_timeout
            {
                return Err("ProviderRun is not eligible for Bridge correlation".into());
            }
            let timestamp = now();
            if handoff.bridge_request_id.is_none() {
                transaction
                    .execute(
                        "UPDATE handoffs SET bridge_request_id=?2,bridge_turn_key=?3,bridge_correlation_observed_at=?4 WHERE id=?1",
                        params![handoff_id, request_id, turn_key, timestamp],
                    )
                    .map_err(db_error)?;
            }
            transaction
                .execute(
                    "UPDATE provider_runs SET external_run_id=?2,status='RUNNING',terminal_code=NULL,terminal_at=NULL,updated_at=?3 WHERE id=?1",
                    params![provider_run_id, request_id, timestamp],
                )
                .map_err(db_error)?;
            transaction.commit().map_err(db_error)
        })
    }

    pub fn handoff_by_id(&self, handoff_id: &str) -> Result<HandoffHistoryItem, String> {
        self.with_connection(|connection| handoff_by_id(connection, handoff_id))
    }

    pub fn create_provider_run(
        &self,
        workstream_id: &str,
        endpoint_id: &str,
        provider: &str,
        origin_handoff_id: Option<&str>,
        external_run_id: Option<&str>,
        status: &str,
    ) -> Result<ProviderRun, String> {
        if !["CHATGPT", "CODEX"].contains(&provider) || !["STARTING", "RUNNING"].contains(&status) {
            return Err("Invalid ProviderRun provider or initial status".into());
        }
        self.with_connection(|connection| {
            let endpoint = endpoint_by_id(connection, endpoint_id)?;
            if provider=="CODEX" && connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='watch_replies'",[],|r|r.get::<_,i64>(0)).map_err(db_error)?!=0 {
                let pending:i64=connection.query_row("SELECT COUNT(*) FROM watch_replies WHERE thread_id=?1 AND status IN ('QUEUED','SENDING','UNKNOWN')",[&endpoint.external_id],|r|r.get(0)).map_err(db_error)?;
                if pending!=0{return Err("REPLY_PREVIOUS_PENDING_CHECK_FIRST".into());}
            }
            if endpoint.workstream_id != workstream_id || endpoint.provider != provider { return Err("ProviderRun endpoint must belong to its Workstream and provider".into()); }
            if provider == "CODEX" { let active: i64 = connection.query_row("SELECT COUNT(*) FROM provider_runs WHERE endpoint_id=?1 AND status IN ('STARTING','RUNNING')", params![endpoint_id], |row| row.get(0)).map_err(db_error)?; if active != 0 { return Err("A Codex endpoint already has a running ProviderRun".into()); } }
            if let Some(handoff_id) = origin_handoff_id { let handoff = handoff_by_id(connection, handoff_id)?; if handoff.workstream_id != workstream_id { return Err("ProviderRun Handoff must belong to its Workstream".into()); } }
            let timestamp = now(); let record = ProviderRun { id: id(), workstream_id: workstream_id.into(), endpoint_id: endpoint_id.into(), provider: provider.into(), origin_handoff_id: origin_handoff_id.map(str::to_string), external_run_id: clean_optional(external_run_id.map(str::to_string)), status: status.into(), result_identity: None, result_text: None, terminal_code: None, started_at: Some(timestamp), terminal_at: None, reviewed_at: None, created_at: timestamp, updated_at: timestamp };
            connection.execute("INSERT INTO provider_runs (id,workstream_id,endpoint_id,provider,origin_handoff_id,external_run_id,status,result_identity,terminal_code,started_at,terminal_at,reviewed_at,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,NULL,NULL,?8,NULL,NULL,?8,?8)", params![record.id,record.workstream_id,record.endpoint_id,record.provider,record.origin_handoff_id,record.external_run_id,record.status,timestamp]).map_err(db_error)?;
            provider_run_by_id(connection, &record.id)
        })
    }

    /// Reserve one normal-product ChatGPT write in the existing Store. The
    /// shared connection serializes HTTP/IPC claims; retained UNKNOWN excludes
    /// another write across restart. No new schema or run state is introduced.
    pub fn create_chatgpt_dispatch_run(&self, workstream_id: &str, endpoint_id: &str,
        origin_handoff_id: Option<&str>) -> Result<ProviderRun, String> {
        self.with_connection(|connection| {
            let endpoint = endpoint_by_id(connection, endpoint_id)?;
            if endpoint.workstream_id != workstream_id || endpoint.provider != "CHATGPT" || endpoint.status != "ACTIVE" {
                return Err("ACTIVE_ENDPOINT_CHANGED".into());
            }
            let pending: i64 = connection.query_row("SELECT COUNT(*) FROM provider_runs WHERE endpoint_id=?1 AND status IN ('STARTING','RUNNING','UNKNOWN')",
                params![endpoint_id], |row| row.get(0)).map_err(db_error)?;
            if pending != 0 { return Err("CHATGPT_EXISTING_DELIVERY_PENDING_OR_UNKNOWN".into()); }
            if let Some(handoff_id) = origin_handoff_id {
                let handoff = handoff_by_id(connection, handoff_id)?;
                if handoff.workstream_id != workstream_id || handoff.destination_endpoint.id != endpoint_id || handoff.status != "SENDING" {
                    return Err("CHATGPT_APPROVED_HANDOFF_INTENT_REQUIRED".into());
                }
            }
            let timestamp = now();
            let run_id = id();
            connection.execute("INSERT INTO provider_runs (id,workstream_id,endpoint_id,provider,origin_handoff_id,external_run_id,status,result_identity,terminal_code,started_at,terminal_at,reviewed_at,created_at,updated_at) VALUES (?1,?2,?3,'CHATGPT',?4,NULL,'STARTING',NULL,NULL,?5,NULL,NULL,?5,?5)",
                params![run_id, workstream_id, endpoint_id, origin_handoff_id, timestamp]).map_err(db_error)?;
            provider_run_by_id(connection, &run_id)
        })
    }

    /// Records the exact retained ChatGPT result which produced a feedback
    /// run. Historical runs are intentionally never inferred or backfilled.
    pub fn record_chatgpt_feedback_source(
        &self,
        feedback_run_id: &str,
        source_run_id: &str,
    ) -> Result<(), String> {
        self.with_connection(|connection| {
            let feedback = provider_run_by_id(connection, feedback_run_id)?;
            let source = provider_run_by_id(connection, source_run_id)?;
            if feedback.provider != "CHATGPT"
                || source.provider != "CHATGPT"
                || feedback.origin_handoff_id.is_some()
                || feedback.workstream_id != source.workstream_id
                || feedback.endpoint_id != source.endpoint_id
                || source.status != "COMPLETED"
                || source.result_identity.is_none()
                || source.result_text.is_none()
            {
                return Err("Feedback source must be the exact retained ChatGPT result on the same route".into());
            }
            connection
                .execute(
                    "INSERT INTO provider_run_feedback_sources (feedback_run_id,source_run_id,created_at) VALUES (?1,?2,?3)",
                    params![feedback_run_id, source_run_id, now()],
                )
                .map_err(db_error)?;
            Ok(())
        })
    }

    pub fn recoverable_chatgpt_feedback_run_for_source(
        &self,
        source_run_id: &str,
        active_endpoint_id: &str,
    ) -> Result<Option<String>, String> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT r.id FROM provider_run_feedback_sources f JOIN provider_runs r ON r.id=f.feedback_run_id WHERE f.source_run_id=?1 AND r.provider='CHATGPT' AND r.endpoint_id=?2 AND r.origin_handoff_id IS NULL AND r.external_run_id IS NOT NULL AND r.result_identity IS NULL AND r.result_text IS NULL AND ((r.status='FAILED' AND r.terminal_code='CHATGPT_BRIDGE_TERMINAL_ERROR') OR (r.status='UNKNOWN' AND r.terminal_code='CHATGPT_RESULT_UNREADABLE_AFTER_ACCEPTANCE')) ORDER BY r.created_at,r.rowid"
            ).map_err(db_error)?;
            let rows = statement.query_map(params![source_run_id, active_endpoint_id], |row| row.get::<_, String>(0)).map_err(db_error)?.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
            Ok(match rows.as_slice() { [run_id] => Some(run_id.clone()), _ => None })
        })
    }

    pub fn feedback_run_is_sourced_from(
        &self,
        feedback_run_id: &str,
        source_run_id: &str,
    ) -> Result<bool, String> {
        self.with_connection(|connection| {
            connection.query_row(
                "SELECT 1 FROM provider_run_feedback_sources WHERE feedback_run_id=?1 AND source_run_id=?2",
                params![feedback_run_id, source_run_id],
                |_| Ok(()),
            ).optional().map(|value| value.is_some()).map_err(db_error)
        })
    }

    pub fn complete_provider_run(
        &self,
        provider: &str,
        external_run_id: &str,
        status: &str,
        terminal_code: Option<&str>,
    ) -> Result<(), String> {
        if !["COMPLETED", "FAILED", "CANCELLED", "UNKNOWN"].contains(&status) {
            return Err("Invalid terminal ProviderRun status".into());
        }
        self.with_connection(|connection| { let timestamp=now(); let terminal=if ["COMPLETED","FAILED","CANCELLED"].contains(&status){Some(timestamp)}else{None}; let changed=connection.execute("UPDATE provider_runs SET status=?3,terminal_code=?4,terminal_at=?5,updated_at=?6 WHERE provider=?1 AND external_run_id=?2 AND status IN ('STARTING','RUNNING','UNKNOWN')", params![provider,external_run_id,status,terminal_code,terminal, timestamp]).map_err(db_error)?; if changed == 0 { return Err("No mutable ProviderRun has this exact provider run ID".into()); } Ok(()) })
    }

    /// Attaches the exact external run identity observed after a provider
    /// dispatch. This is intentionally separate from result acceptance: a
    /// correlated request can remain RUNNING without changing any retained
    /// reviewable result.
    pub fn attach_provider_run_external_identity(
        &self,
        run_id: &str,
        provider: &str,
        external_run_id: &str,
    ) -> Result<(), String> {
        if !["CHATGPT", "CODEX"].contains(&provider) {
            return Err("Invalid ProviderRun provider".into());
        }
        let external_run_id = nonempty(external_run_id, "Provider external run ID")?;
        self.with_connection(|connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let run = provider_run_by_id(&transaction, run_id)?;
            if run.provider != provider {
                return Err("ProviderRun provider does not match the exact external run".into());
            }
            let can_attach_after_acceptance_timeout = run.provider == "CHATGPT"
                && run.status == "UNKNOWN"
                && run.external_run_id.is_none()
                && run.terminal_code.as_deref() == Some("CHATGPT_ACCEPTANCE_UNPROVEN");
            if !["STARTING", "RUNNING"].contains(&run.status.as_str())
                && !can_attach_after_acceptance_timeout
            {
                return Err("ProviderRun is not eligible for external identity attachment".into());
            }
            if let Some(existing) = run.external_run_id.as_deref() {
                if existing != external_run_id {
                    return Err("ProviderRun external run identity changed after it was stored".into());
                }
            }
            let changed = transaction
                .execute(
                    "UPDATE provider_runs SET external_run_id=?3,status='RUNNING',terminal_code=NULL,updated_at=?4 WHERE id=?1 AND provider=?2 AND status IN ('STARTING','RUNNING','UNKNOWN')",
                    params![run.id, provider, external_run_id, now()],
                )
                .map_err(db_error)?;
            if changed != 1 {
                return Err("ProviderRun changed before external identity attachment".into());
            }
            transaction.commit().map_err(db_error)?;
            Ok(())
        })
    }

    /// Stores a proven, exact terminal result without turning ProviderRun history
    /// into a transcript archive. The partial unique index and this transaction
    /// jointly retain at most one current result for each Workstream/provider.
    pub fn accept_completed_provider_result(
        &self,
        run_id: &str,
        external_run_id: &str,
        result_identity: &str,
        result_text: String,
    ) -> Result<ProviderRun, String> {
        let external_run_id = nonempty(external_run_id, "Provider external run ID")?;
        let result_identity = nonempty(result_identity, "Provider result identity")?;
        if result_text.trim().is_empty() {
            return Err("A completed ProviderRun requires nonempty result text".into());
        }
        self.with_connection(|connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let run = provider_run_by_id(&transaction, run_id)?;
            if run.external_run_id.as_deref() != Some(external_run_id.as_str()) {
                return Err("ProviderRun external run identity does not match the exact completion".into());
            }
            if !["STARTING", "RUNNING", "UNKNOWN"].contains(&run.status.as_str()) {
                return Err("ProviderRun is not eligible for completed-result acceptance".into());
            }
            let timestamp = now();
            let role_schema:bool=transaction.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='endpoint_role_details')",[],|r|r.get(0)).map_err(db_error)?;
            let role_mode=role_schema&&transaction.query_row("SELECT EXISTS(SELECT 1 FROM endpoints WHERE workstream_id=?1 AND status='ACTIVE' AND bridge_role!='LEGACY')",params![run.workstream_id],|r|r.get::<_,bool>(0)).map_err(db_error)?;
            if role_mode {
                transaction.execute("UPDATE provider_runs SET result_text=NULL WHERE workstream_id=?1 AND endpoint_id=?2 AND id<>?3 AND result_text IS NOT NULL",params![run.workstream_id,run.endpoint_id,run.id]).map_err(db_error)?;
            }else {
                transaction.execute("UPDATE provider_runs SET result_text=NULL WHERE workstream_id=?1 AND provider=?2 AND id<>?3 AND result_text IS NOT NULL",params![run.workstream_id,run.provider,run.id]).map_err(db_error)?;
            }
            let changed = transaction
                .execute(
                    "UPDATE provider_runs SET status='COMPLETED',result_identity=?2,result_text=?3,terminal_code='COMPLETED',terminal_at=?4,updated_at=?4 WHERE id=?1 AND external_run_id=?5 AND status IN ('STARTING','RUNNING','UNKNOWN')",
                    params![run.id, result_identity, result_text, timestamp, external_run_id],
                )
                .map_err(db_error)?;
            if changed != 1 {
                return Err("ProviderRun changed before completed-result acceptance".into());
            }
            transaction.commit().map_err(db_error)?;
            provider_run_by_id(connection, run_id)
        })
    }

    /// Completes one narrowly-scoped direct ChatGPT result after a later exact
    /// history read proves the correlated assistant turn and nonempty text.
    /// This is deliberately not a generic terminal-state rewrite: only the
    /// designated empty-response terminal or accepted-but-unreadable state is
    /// eligible, and both retain the exact persisted Bridge request identity.
    pub fn recover_chatgpt_result_from_exact_history(
        &self,
        run_id: &str,
        external_run_id: &str,
        result_identity: &str,
        result_text: String,
    ) -> Result<ProviderRun, String> {
        let external_run_id = nonempty(external_run_id, "Provider external run ID")?;
        let result_identity = nonempty(result_identity, "Provider result identity")?;
        if result_text.trim().is_empty() {
            return Err("Exact history recovery requires nonempty result text".into());
        }
        self.with_connection(|connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let run = provider_run_by_id(&transaction, run_id)?;
            if run.provider != "CHATGPT"
                || run.origin_handoff_id.is_some()
                || run.external_run_id.as_deref() != Some(external_run_id.as_str())
                || !matches!(
                    (run.status.as_str(), run.terminal_code.as_deref()),
                    ("FAILED", Some("CHATGPT_BRIDGE_TERMINAL_ERROR"))
                        | ("UNKNOWN", Some("CHATGPT_RESULT_UNREADABLE_AFTER_ACCEPTANCE"))
                )
                || run.result_identity.is_some()
                || run.result_text.is_some()
            {
                return Err("ProviderRun is not eligible for exact-history result recovery".into());
            }
            let timestamp = now();
            transaction
                .execute(
                    "UPDATE provider_runs SET result_text=NULL WHERE workstream_id=?1 AND provider='CHATGPT' AND id<>?2 AND result_text IS NOT NULL",
                    params![run.workstream_id, run.id],
                )
                .map_err(db_error)?;
            let changed = transaction
                .execute(
                    "UPDATE provider_runs SET status='COMPLETED',result_identity=?2,result_text=?3,terminal_code='COMPLETED_AFTER_EXACT_HISTORY_RECOVERY',terminal_at=?4,updated_at=?4 WHERE id=?1 AND provider='CHATGPT' AND origin_handoff_id IS NULL AND external_run_id=?5 AND ((status='FAILED' AND terminal_code='CHATGPT_BRIDGE_TERMINAL_ERROR') OR (status='UNKNOWN' AND terminal_code='CHATGPT_RESULT_UNREADABLE_AFTER_ACCEPTANCE')) AND result_identity IS NULL AND result_text IS NULL",
                    params![run.id, result_identity, result_text, timestamp, external_run_id],
                )
                .map_err(db_error)?;
            if changed != 1 {
                return Err("ProviderRun changed before exact-history result recovery".into());
            }
            transaction.commit().map_err(db_error)?;
            provider_run_by_id(connection, run_id)
        })
    }
    pub fn reconciliation_candidates(&self) -> Result<Vec<ReconciliationCandidate>, String> {
        self.with_connection(|connection| {
            let mut statement=connection.prepare("SELECT r.id,r.workstream_id,r.endpoint_id,r.provider,r.origin_handoff_id,r.external_run_id,r.status,r.result_identity,r.terminal_code,r.started_at,r.terminal_at,r.reviewed_at,r.created_at,r.updated_at,r.result_text,e.id,e.workstream_id,e.provider,e.external_id,e.label,e.status,e.replaces_endpoint_id,e.created_at,e.superseded_at FROM provider_runs r JOIN endpoints e ON e.id=r.endpoint_id WHERE r.status IN ('STARTING','RUNNING') OR (r.status='UNKNOWN' AND (r.terminal_code='ROUTER_RESTART_UNPROVEN' OR (r.provider='CODEX' AND r.terminal_code IN ('EXACT_EVIDENCE_UNAVAILABLE','CODEX_RESULT_UNOBSERVED','CODEX_ADAPTER_UNAVAILABLE','CODEX_TURN_NOT_PROVEN_TERMINAL')))) ORDER BY r.updated_at").map_err(db_error)?;
            let candidates=statement.query_map([],|row|Ok(ReconciliationCandidate { run:provider_run_row(row)?, endpoint:Endpoint { id:row.get(15)?,workstream_id:row.get(16)?,provider:row.get(17)?,external_id:row.get(18)?,label:row.get(19)?,status:row.get(20)?,replaces_endpoint_id:row.get(21)?,created_at:row.get(22)?,superseded_at:row.get(23)? } })).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            Ok(candidates)
        })
    }

    pub fn latest_reviewable_provider_result(
        &self,
        workstream_id: &str,
        provider: &str,
    ) -> Result<Option<ProviderRun>, String> {
        if !["CHATGPT", "CODEX"].contains(&provider) {
            return Err("Invalid ProviderRun provider".into());
        }
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id,workstream_id,endpoint_id,provider,origin_handoff_id,external_run_id,status,result_identity,terminal_code,started_at,terminal_at,reviewed_at,created_at,updated_at,result_text FROM provider_runs WHERE workstream_id=?1 AND provider=?2 AND status='COMPLETED' AND result_text IS NOT NULL ORDER BY terminal_at DESC,rowid DESC LIMIT 1",
                    params![workstream_id, provider],
                    provider_run_row,
                )
                .optional()
                .map_err(db_error)
        })
    }

    pub fn reconcile_provider_run(
        &self,
        run_id: &str,
        status: &str,
        code: &str,
    ) -> Result<(), String> {
        if !["RUNNING", "COMPLETED", "FAILED", "CANCELLED", "UNKNOWN"].contains(&status) {
            return Err("Invalid reconciled ProviderRun status".into());
        }
        self.with_connection(|connection| { let timestamp=now(); let terminal=if ["COMPLETED","FAILED","CANCELLED"].contains(&status) { Some(timestamp) } else { None }; let changed=connection.execute("UPDATE provider_runs SET status=?2,terminal_code=?3,terminal_at=?4,updated_at=?5 WHERE id=?1 AND status IN ('STARTING','RUNNING','UNKNOWN')",params![run_id,status,code,terminal,timestamp]).map_err(db_error)?; if changed != 1 { return Err("ProviderRun is not eligible for reconciliation".into()); } Ok(()) })
    }

    /// Records that Router's bounded ChatGPT acceptance observation elapsed
    /// without an exact Bridge request ID. The identity predicate is part of
    /// the same mutation so a racing accepted callback can never be hidden as
    /// UNKNOWN after it has durably attached its external request.
    pub fn mark_chatgpt_acceptance_unproven_if_unattached(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.with_connection(|connection| {
            let timestamp = now();
            let changed = connection
                .execute(
                    "UPDATE provider_runs SET status='UNKNOWN',terminal_code='CHATGPT_ACCEPTANCE_UNPROVEN',terminal_at=NULL,updated_at=?2 WHERE id=?1 AND provider='CHATGPT' AND external_run_id IS NULL AND status IN ('STARTING','RUNNING')",
                    params![run_id, timestamp],
                )
                .map_err(db_error)?;
            Ok(changed == 1)
        })
    }

    /// Atomically records the bounded no-correlation observation for the one
    /// reverse Handoff that owns this ChatGPT ProviderRun. The persisted
    /// Handoff remains `SENDING`: no exact Bridge identity means Router cannot
    /// call it either delivered or failed. A racing exact callback either wins
    /// first (this returns false), or can later recover only this named
    /// provisional ProviderRun and clear the Handoff disclosure on `SENT`.
    pub fn mark_chatgpt_handoff_acceptance_unproven_if_unattached(
        &self,
        handoff_id: &str,
        run_id: &str,
    ) -> Result<bool, String> {
        self.with_connection(|connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let handoff = handoff_by_id(&transaction, handoff_id)?;
            if handoff.status != "SENDING" || handoff.bridge_request_id.is_some() {
                return Ok(false);
            }
            let run = provider_run_by_id(&transaction, run_id)?;
            if run.provider != "CHATGPT" || run.origin_handoff_id.as_deref() != Some(handoff_id) {
                return Err("ChatGPT ProviderRun does not belong to this reverse Handoff".into());
            }
            let timestamp = now();
            let changed = transaction
                .execute(
                    "UPDATE provider_runs SET status='UNKNOWN',terminal_code='CHATGPT_ACCEPTANCE_UNPROVEN',terminal_at=NULL,updated_at=?2 WHERE id=?1 AND external_run_id IS NULL AND status IN ('STARTING','RUNNING')",
                    params![run_id, timestamp],
                )
                .map_err(db_error)?;
            if changed != 1 {
                return Ok(false);
            }
            transaction
                .execute(
                    "UPDATE handoffs SET error_code='CHATGPT_ACCEPTANCE_UNPROVEN',error_message='Waiting for exact ChatGPT acceptance proof' WHERE id=?1 AND status='SENDING' AND bridge_request_id IS NULL",
                    params![handoff_id],
                )
                .map_err(db_error)?;
            transaction.commit().map_err(db_error)?;
            Ok(true)
        })
    }

    pub fn mark_provider_run_reviewed(&self, run_id: &str) -> Result<(), String> {
        self.with_connection(|connection| { let changed=connection.execute("UPDATE provider_runs SET reviewed_at=?2,updated_at=?2 WHERE id=?1 AND status IN ('COMPLETED','FAILED','CANCELLED')",params![run_id,now()]).map_err(db_error)?; if changed != 1 { return Err("Only terminal ProviderRuns can be marked reviewed".into()); } Ok(()) })
    }
    pub fn fail_provider_run_by_id(&self, run_id: &str, terminal_code: &str) -> Result<(), String> {
        self.with_connection(|connection| { let timestamp=now(); let changed=connection.execute("UPDATE provider_runs SET status='FAILED',terminal_code=?2,terminal_at=?3,updated_at=?3 WHERE id=?1 AND status IN ('STARTING','RUNNING','UNKNOWN')",params![run_id,terminal_code,timestamp]).map_err(db_error)?; if changed != 1 { return Err("No mutable ProviderRun has this ID".into()); } Ok(()) })
    }
    pub fn confirm_handoff_and_provider_run(
        &self,
        handoff_id: &str,
        request_id: &str,
        handoff_status: &str,
        run_status: &str,
        handoff_error: Option<(&str, &str)>,
        provider_terminal_code: Option<&str>,
    ) -> Result<(), String> {
        self.with_connection(|connection| {
            let transaction=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let handoff=handoff_by_id(&transaction, handoff_id)?;
            if handoff.status!="SENDING" || handoff.bridge_request_id.as_deref()!=Some(request_id) { return Err("Handoff recovery identity changed before confirmation".into()); }
            if !["COMPLETED", "FAILED", "CANCELLED"].contains(&run_status) {
                return Err("Exact Bridge recovery requires a terminal ProviderRun status".into());
            }
            let timestamp=now();
            let mut statement=transaction.prepare("SELECT id FROM provider_runs WHERE origin_handoff_id=?1 AND provider='CHATGPT'").map_err(db_error)?;
            let runs=statement.query_map(params![handoff_id],|row|row.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            drop(statement);
            for id in runs {
                let run=provider_run_by_id(&transaction, &id)?;
                if run.external_run_id.as_deref()!=Some(request_id) {
                    return Err("Matching ProviderRun request ID differs from exact Bridge request ID".into());
                }
                if ["STARTING", "RUNNING", "UNKNOWN"].contains(&run.status.as_str()) {
                    transaction.execute(
                        "UPDATE provider_runs SET status=?2,terminal_code=?3,terminal_at=?4,updated_at=?4 WHERE id=?1",
                        params![id,run_status,provider_terminal_code,timestamp],
                    ).map_err(db_error)?;
                } else if run.status == run_status {
                    transaction.execute(
                        "UPDATE provider_runs SET terminal_code=COALESCE(?2,terminal_code),updated_at=?3 WHERE id=?1",
                        params![id,provider_terminal_code,timestamp],
                    ).map_err(db_error)?;
                } else {
                    return Err("Matching ProviderRun terminal state conflicts with exact Bridge diagnostics".into());
                }
            }
            match handoff_status { "SENT" => { transaction.execute("UPDATE handoffs SET status='SENT',sent_at=?2 WHERE id=?1",params![handoff_id,timestamp]).map_err(db_error)?; bridge_notifications::mark_sent_source_handled(&transaction,handoff_id,timestamp)?; }, "FAILED" => { let (error_code,error_message)=handoff_error.ok_or("Failed recovery needs error detail")?; transaction.execute("UPDATE handoffs SET status='FAILED',failed_at=?2,error_code=?3,error_message=?4 WHERE id=?1",params![handoff_id,timestamp,error_code,error_message]).map_err(db_error)?; }, _=>return Err("Unsupported recovery Handoff status".into()) }
            transaction.commit().map_err(db_error)
        })
    }
    pub fn acknowledge_handoff_attention(&self, handoff_id: &str) -> Result<(), String> {
        self.with_connection(|connection| { let changed=connection.execute("UPDATE handoffs SET attention_acknowledged_at=?2 WHERE id=?1 AND status IN ('SENDING','FAILED')",params![handoff_id,now()]).map_err(db_error)?; if changed != 1 { return Err("Only SENDING or FAILED Handoffs can be acknowledged".into()); } Ok(()) })
    }

    pub fn dashboard_projection(&self) -> Result<DashboardProjection, String> {
        self.with_connection(|connection| {
            let mut cards = Vec::new();
            let mut attention = Vec::new();
            for project in list_projects(connection)? {
                for workstream in list_workstreams(connection, &project.id)? {
                    let chatgpt_endpoint = role_bridge::projection_endpoint(connection, &workstream.id, "CHATGPT")?;
                    let codex_endpoint = role_bridge::projection_endpoint(connection, &workstream.id, "CODEX")?;
                    let chatgpt_run = latest_provider_run(connection, &workstream.id, "CHATGPT")?;
                    let codex_run = latest_provider_run(connection, &workstream.id, "CODEX")?;
                    let provider_run_activity_at =
                        latest_provider_run_activity(connection, &workstream.id)?;
                    let mut items = Vec::new();
                    for handoff in list_handoffs(connection, &workstream.id)? {
                        if handoff.attention_acknowledged_at.is_none()
                            && handoff.status == "SENDING"
                        {
                            items.push(attention_item(
                                "DELIVERY_UNCERTAIN",
                                1,
                                &workstream,
                                &handoff.id,
                                "Delivery uncertain — no automatic retry",
                                handoff.created_at,
                            ));
                        } else if handoff.attention_acknowledged_at.is_none()
                            && handoff.status == "FAILED"
                        {
                            items.push(attention_item(
                                "HANDOFF_FAILED",
                                2,
                                &workstream,
                                &handoff.id,
                                "Handoff failed — review history",
                                handoff.failed_at.unwrap_or(handoff.created_at),
                            ));
                        }
                    }
                    let mut unread_replies_statement = connection.prepare(
                        "SELECT r.id,r.observed_at,e.provider FROM reply_observations r JOIN endpoints e ON e.id=r.endpoint_id WHERE r.workstream_id=?1 AND r.read_at IS NULL ORDER BY r.observed_at DESC,r.rowid DESC",
                    ).map_err(db_error)?;
                    let unread_replies = unread_replies_statement
                        .query_map(params![workstream.id], |row| {
                            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                        })
                        .map_err(db_error)?
                        .collect::<Result<Vec<(String, i64, String)>, _>>()
                        .map_err(db_error)?;
                    drop(unread_replies_statement);
                    for (observation_id, observed_at, provider) in unread_replies {
                        items.push(attention_item(
                            if provider == "CODEX" { "CODEX_REPLY_OBSERVED" } else { "CHATGPT_REPLY_OBSERVED" },
                            3, &workstream, &observation_id,
                            if provider == "CODEX" { "Codex has a newly observed reply" } else { "ChatGPT has a newly observed reply" }, observed_at,
                        ));
                    }
                    for run in unresolved_provider_runs(connection, &workstream.id)? {
                        if run.status == "COMPLETED"
                            && run.result_text.is_some()
                            && run.reviewed_at.is_none()
                        {
                            items.push(attention_item(
                                if run.provider == "CODEX" {
                                    "CODEX_RESULT_READY"
                                } else {
                                    "CHATGPT_RESULT_READY"
                                },
                                4,
                                &workstream,
                                &run.id,
                                if run.provider == "CODEX" {
                                    "Codex result ready for review"
                                } else {
                                    "ChatGPT result ready for review"
                                },
                                run.terminal_at.unwrap_or(run.updated_at),
                            ));
                        } else if run.status == "FAILED" && run.reviewed_at.is_none() {
                            items.push(attention_item(
                                "PROVIDER_RUN_FAILED",
                                2,
                                &workstream,
                                &run.id,
                                "Provider execution failed — review",
                                run.terminal_at.unwrap_or(run.updated_at),
                            ));
                        } else if run.status == "CANCELLED" && run.reviewed_at.is_none() {
                            items.push(attention_item(
                                "PROVIDER_RUN_CANCELLED",
                                2,
                                &workstream,
                                &run.id,
                                "Provider execution cancelled — review",
                                run.terminal_at.unwrap_or(run.updated_at),
                            ));
                        } else if run.status == "UNKNOWN" {
                            items.push(attention_item(
                                "UNKNOWN_RUN",
                                3,
                                &workstream,
                                &run.id,
                                "Provider execution state unknown after restart",
                                run.updated_at,
                            ));
                        }
                    }
                    let role_complete = role_bridge::complete_roles(connection, &workstream.id)?;
                    if chatgpt_endpoint.is_none() && !role_complete {
                        items.push(attention_item(
                            "MISSING_CHATGPT_BINDING",
                            5,
                            &workstream,
                            &workstream.id,
                            "Missing active ChatGPT binding",
                            workstream.updated_at,
                        ));
                    }
                    if codex_endpoint.is_none() && !role_complete {
                        items.push(attention_item(
                            "MISSING_CODEX_BINDING",
                            5,
                            &workstream,
                            &workstream.id,
                            "Missing active Codex binding",
                            workstream.updated_at,
                        ));
                    }
                    let last_activity_at = [
                        workstream.updated_at,
                        chatgpt_endpoint.as_ref().map(|e| e.created_at).unwrap_or(0),
                        codex_endpoint.as_ref().map(|e| e.created_at).unwrap_or(0),
                        provider_run_activity_at,
                        list_handoffs(connection, &workstream.id)?
                            .iter()
                            .map(|h| {
                                h.failed_at
                                    .or(h.sent_at)
                                    .or(h.approved_at)
                                    .unwrap_or(h.created_at)
                            })
                            .max()
                            .unwrap_or(0),
                    ]
                    .into_iter()
                    .max()
                    .unwrap_or(workstream.updated_at);
                    attention.extend(items.clone());
                    cards.push(WorkstreamDashboardItem {
                        project_id: project.id.clone(),
                        project_name: project.name.clone(),
                        workstream,
                        chatgpt_endpoint,
                        codex_endpoint,
                        chatgpt_run,
                        codex_run,
                        attention_items: items,
                        last_activity_at,
                    });
                }
            }
            attention.sort_by(|a, b| {
                a.priority
                    .cmp(&b.priority)
                    .then_with(|| b.activity_at.cmp(&a.activity_at))
            });
            cards.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));
            Ok(DashboardProjection {
                workstreams: cards,
                attention_items: attention,
            })
        })
    }

    pub fn has_sent_duplicate(
        &self,
        source_response_identity: Option<&str>,
        destination_endpoint_id: &str,
        approved_text: &str,
    ) -> Result<bool, String> {
        self.with_connection(|connection| { let hash = payload_hash(&source_response_identity.map(str::to_string), destination_endpoint_id, approved_text); let count: i64 = connection.query_row("SELECT COUNT(*) FROM handoffs WHERE source_response_identity IS ?1 AND destination_endpoint_id=?2 AND payload_hash=?3 AND status='SENT'", params![source_response_identity, destination_endpoint_id, hash], |row| row.get(0)).map_err(db_error)?; Ok(count > 0) })
    }

    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "Router persistence is unavailable")?;
        operation(&mut connection)
    }
}

fn run_migrations(connection: &mut Connection) -> Result<(), String> {
    migration::require_foreign_keys(connection)?;
    connection.execute_batch("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY NOT NULL, applied_at INTEGER NOT NULL);").map_err(db_error)?;
    for (version, sql) in MIGRATIONS {
        let applied: Option<i64> = connection
            .query_row(
                "SELECT version FROM schema_migrations WHERE version=?1",
                params![version],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?;
        if applied.is_none() {
            migration::require_foreign_keys(connection)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            transaction.execute_batch(sql).map_err(db_error)?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations (version,applied_at) VALUES (?1,?2)",
                    params![version, now()],
                )
                .map_err(db_error)?;
            transaction.commit().map_err(db_error)?;
            migration::require_foreign_keys(connection)?;
        }
    }
    Ok(())
}

/// The daily Router database and the isolated Preview database deliberately
/// have separate migration authorities.  Feature migrations must never reuse
/// Preview's numbered `schema_migrations` stream.
fn run_normal_feature_migrations(connection: &mut Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS router_feature_migrations (key TEXT PRIMARY KEY NOT NULL, applied_at INTEGER NOT NULL);",
        )
        .map_err(db_error)?;
    for (key, sql) in NORMAL_FEATURE_MIGRATIONS {
        let applied: Option<String> = connection
            .query_row(
                "SELECT key FROM router_feature_migrations WHERE key=?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?;
        if applied.is_none() {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            transaction.execute_batch(sql).map_err(db_error)?;
            transaction
                .execute(
                    "INSERT INTO router_feature_migrations (key,applied_at) VALUES (?1,?2)",
                    params![key, now()],
                )
                .map_err(db_error)?;
            transaction.commit().map_err(db_error)?;
        }
    }
    migration::require_foreign_keys(connection)
}


fn endpoint_params(endpoint: &Endpoint) -> [&dyn rusqlite::ToSql; 9] {
    [
        &endpoint.id,
        &endpoint.workstream_id,
        &endpoint.provider,
        &endpoint.external_id,
        &endpoint.label,
        &endpoint.status,
        &endpoint.replaces_endpoint_id,
        &endpoint.created_at,
        &endpoint.superseded_at,
    ]
}

fn normalize_chatgpt_conversation_url(input: &str) -> Result<String, String> {
    let mut url = Url::parse(input.trim()).map_err(|_| "CHATGPT_CANONICAL_URL_INVALID")?;
    if url.scheme() != "https" || url.host_str() != Some("chatgpt.com") || url.port().is_some()
        || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err("CHATGPT_CANONICAL_URL_INVALID".into());
    }
    let segments = url.path_segments().ok_or("CHATGPT_CANONICAL_URL_INVALID")?.collect::<Vec<_>>();
    match segments.as_slice() {
        ["c", id] if is_chatgpt_conversation_id(id) => {}
        ["g", project, "c", id] if !project.is_empty() && is_chatgpt_conversation_id(id) => {}
        _ => return Err("CHATGPT_CANONICAL_URL_INVALID".into()),
    }
    let path = url.path().trim_end_matches('/').to_string();
    url.set_path(&path);
    Ok(url.into())
}

fn is_chatgpt_conversation_id(value: &str) -> bool {
    value.len() >= 8 && value.chars().all(|character| character.is_ascii_alphanumeric() || character == '-')
}
fn project_by_id(connection: &Connection, id: &str) -> Result<Project, String> {
    connection
        .query_row(
            "SELECT id,name,description,created_at,updated_at FROM projects WHERE id=?1",
            params![id],
            project_row,
        )
        .optional()
        .map_err(db_error)?
        .ok_or("Project not found".into())
}
fn workstream_by_id(connection: &Connection, id: &str) -> Result<Workstream, String> {
    connection
        .query_row(
            "SELECT id,project_id,name,status,created_at,updated_at,binding_revision,archived_at,trashed_at,pinned_at FROM workstreams WHERE id=?1",
            params![id],
            workstream_row,
        )
        .optional()
        .map_err(db_error)?
        .ok_or("Workstream not found".into())
}
fn workstream_draft(
    connection: &Connection,
    workstream_id: &str,
) -> Result<Option<WorkstreamDraft>, String> {
    connection.query_row(
        "SELECT workstream_id,text,revision,updated_at FROM workstream_drafts WHERE workstream_id=?1",
        params![workstream_id],
        |row| Ok(WorkstreamDraft {
            workstream_id: row.get(0)?,
            text: row.get(1)?,
            revision: row.get(2)?,
            updated_at: row.get(3)?,
        }),
    ).optional().map_err(db_error)
}
fn ensure_codex_feedback_source_run(
    connection: &Connection,
    workstream_id: &str,
    source_run_id: &str,
) -> Result<(), String> {
    let provider: Option<String> = connection
        .query_row(
            "SELECT provider FROM provider_runs WHERE id=?1 AND workstream_id=?2",
            params![source_run_id, workstream_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    match provider.as_deref() {
        Some("CODEX") => Ok(()),
        Some(_) => Err("A Codex feedback draft requires a Codex result run".into()),
        None => Err("The Codex feedback result does not belong to this Workstream".into()),
    }
}
fn codex_feedback_draft(
    connection: &Connection,
    workstream_id: &str,
    source_run_id: &str,
) -> Result<Option<CodexFeedbackDraft>, String> {
    connection.query_row(
        "SELECT workstream_id,source_run_id,text,revision,updated_at FROM codex_feedback_drafts WHERE workstream_id=?1 AND source_run_id=?2",
        params![workstream_id, source_run_id],
        |row| Ok(CodexFeedbackDraft {
            workstream_id: row.get(0)?,
            source_run_id: row.get(1)?,
            text: row.get(2)?,
            revision: row.get(3)?,
            updated_at: row.get(4)?,
        }),
    ).optional().map_err(db_error)
}
fn endpoint_by_id(connection: &Connection, id: &str) -> Result<Endpoint, String> {
    connection.query_row("SELECT id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at,superseded_at FROM endpoints WHERE id=?1", params![id], endpoint_row).optional().map_err(db_error)?.ok_or("Endpoint not found".into())
}
fn reply_observation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReplyObservation> {
    Ok(ReplyObservation {
        id: row.get(0)?,
        workstream_id: row.get(1)?,
        endpoint_id: row.get(2)?,
        assistant_identity: row.get(3)?,
        text: row.get(4)?,
        source_provider_run_id: row.get(5)?,
        observed_at: row.get(6)?,
        read_at: row.get(7)?,
        handled_at: row.get(8)?,
        push_state: row.get(9)?,
        push_attempted_at: row.get(10)?,
        push_rendered_at: row.get(11)?,
        completed_at:row.get(12)?,completion_checked:row.get(13)?,
    })
}
fn active_endpoint(
    connection: &Connection,
    workstream_id: &str,
    provider: &str,
) -> Result<Option<Endpoint>, String> {
    let count: i64 = connection.query_row("SELECT count(*) FROM endpoints WHERE workstream_id=?1 AND provider=?2 AND status='ACTIVE'", params![workstream_id,provider], |row| row.get(0)).map_err(db_error)?;
    if count > 1 { return Err("BRIDGE_ROLE_REQUIRED_FOR_AMBIGUOUS_PROVIDER".into()); }
    connection.query_row("SELECT id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at,superseded_at FROM endpoints WHERE workstream_id=?1 AND provider=?2 AND status='ACTIVE'", params![workstream_id,provider], endpoint_row).optional().map_err(db_error)
}
fn list_endpoints(connection: &Connection, workstream_id: &str) -> Result<Vec<Endpoint>, String> {
    let mut statement = connection.prepare("SELECT id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at,superseded_at FROM endpoints WHERE workstream_id=?1 ORDER BY provider, created_at DESC, rowid DESC").map_err(db_error)?;
    let endpoints = statement
        .query_map(params![workstream_id], endpoint_row)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(endpoints)
}
fn list_projects(connection: &Connection) -> Result<Vec<Project>, String> {
    let mut statement=connection.prepare("SELECT id,name,description,created_at,updated_at FROM projects ORDER BY updated_at DESC,name").map_err(db_error)?;
    let projects = statement
        .query_map([], project_row)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(projects)
}
fn list_workstreams(connection: &Connection, project_id: &str) -> Result<Vec<Workstream>, String> {
    let mut statement=connection.prepare("SELECT id,project_id,name,status,created_at,updated_at,binding_revision,archived_at,trashed_at,pinned_at FROM workstreams WHERE project_id=?1 ORDER BY pinned_at IS NOT NULL DESC,pinned_at DESC,updated_at DESC,name").map_err(db_error)?;
    let workstreams = statement
        .query_map(params![project_id], workstream_row)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(workstreams)
}
#[cfg(test)]
fn list_trashed_workstreams(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<Workstream>, String> {
    let mut statement=connection.prepare("SELECT id,project_id,name,status,created_at,updated_at,binding_revision,archived_at,trashed_at,pinned_at FROM workstreams WHERE project_id=?1 AND trashed_at IS NOT NULL ORDER BY trashed_at DESC,name").map_err(db_error)?;
    let workstreams = statement
        .query_map(params![project_id], workstream_row)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(workstreams)
}
fn list_external_project_links(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<ExternalProjectLink>, String> {
    let mut statement = connection.prepare("SELECT id,project_id,provider,external_project_id,canonical_url,label,source_kind,source_version,verified_at,created_at,updated_at FROM external_project_links WHERE project_id=?1 ORDER BY provider").map_err(db_error)?;
    let links = statement
        .query_map(params![project_id], external_project_link_row)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(links)
}
fn external_project_link_for_project_provider(
    connection: &Connection,
    project_id: &str,
    provider: &str,
) -> Result<Option<ExternalProjectLink>, String> {
    connection.query_row("SELECT id,project_id,provider,external_project_id,canonical_url,label,source_kind,source_version,verified_at,created_at,updated_at FROM external_project_links WHERE project_id=?1 AND provider=?2", params![project_id,provider], external_project_link_row).optional().map_err(db_error)
}
fn external_project_link_by_id(
    connection: &Connection,
    id: &str,
) -> Result<ExternalProjectLink, String> {
    connection.query_row("SELECT id,project_id,provider,external_project_id,canonical_url,label,source_kind,source_version,verified_at,created_at,updated_at FROM external_project_links WHERE id=?1", params![id], external_project_link_row).map_err(db_error)
}
fn setting(connection: &Connection, key: &str) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT value FROM app_settings WHERE key=?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)
}
fn set_setting(
    connection: &Connection,
    key: &str,
    value: &str,
    timestamp: i64,
) -> Result<(), String> {
    connection.execute("INSERT INTO app_settings (key,value,updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at", params![key,value,timestamp]).map_err(db_error)?;
    Ok(())
}
fn project_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}
fn workstream_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workstream> {
    Ok(Workstream {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        binding_revision: row.get(6)?,
        archived_at: row.get(7)?,
        trashed_at: row.get(8)?,
        pinned_at: row.get(9)?,
    })
}
fn external_project_link_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExternalProjectLink> {
    Ok(ExternalProjectLink {
        id: row.get(0)?,
        project_id: row.get(1)?,
        provider: row.get(2)?,
        external_project_id: row.get(3)?,
        canonical_url: row.get(4)?,
        label: row.get(5)?,
        source_kind: row.get(6)?,
        source_version: row.get(7)?,
        verified_at: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}
fn endpoint_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Endpoint> {
    Ok(Endpoint {
        id: row.get(0)?,
        workstream_id: row.get(1)?,
        provider: row.get(2)?,
        external_id: row.get(3)?,
        label: row.get(4)?,
        status: row.get(5)?,
        replaces_endpoint_id: row.get(6)?,
        created_at: row.get(7)?,
        superseded_at: row.get(8)?,
    })
}
fn provider_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderRun> {
    Ok(ProviderRun {
        id: row.get(0)?,
        workstream_id: row.get(1)?,
        endpoint_id: row.get(2)?,
        provider: row.get(3)?,
        origin_handoff_id: row.get(4)?,
        external_run_id: row.get(5)?,
        status: row.get(6)?,
        result_identity: row.get(7)?,
        terminal_code: row.get(8)?,
        started_at: row.get(9)?,
        terminal_at: row.get(10)?,
        reviewed_at: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        result_text: row.get(14)?,
    })
}
fn provider_run_by_id(connection: &Connection, run_id: &str) -> Result<ProviderRun, String> {
    connection.query_row("SELECT id,workstream_id,endpoint_id,provider,origin_handoff_id,external_run_id,status,result_identity,terminal_code,started_at,terminal_at,reviewed_at,created_at,updated_at,result_text FROM provider_runs WHERE id=?1",params![run_id],provider_run_row).optional().map_err(db_error)?.ok_or("ProviderRun not found".into())
}
fn latest_provider_run(
    connection: &Connection,
    workstream_id: &str,
    provider: &str,
) -> Result<Option<ProviderRun>, String> {
    connection.query_row("SELECT id,workstream_id,endpoint_id,provider,origin_handoff_id,external_run_id,status,result_identity,terminal_code,started_at,terminal_at,reviewed_at,created_at,updated_at,result_text FROM provider_runs WHERE workstream_id=?1 AND provider=?2 ORDER BY created_at DESC,rowid DESC LIMIT 1",params![workstream_id,provider],provider_run_row).optional().map_err(db_error)
}

fn latest_provider_run_activity(
    connection: &Connection,
    workstream_id: &str,
) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT MAX(updated_at) FROM provider_runs WHERE workstream_id=?1",
            params![workstream_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .map(|value| value.unwrap_or(0))
        .map_err(db_error)
}

fn unresolved_provider_runs(
    connection: &Connection,
    workstream_id: &str,
) -> Result<Vec<ProviderRun>, String> {
    let mut statement = connection.prepare("SELECT id,workstream_id,endpoint_id,provider,origin_handoff_id,external_run_id,status,result_identity,terminal_code,started_at,terminal_at,reviewed_at,created_at,updated_at,result_text FROM provider_runs WHERE workstream_id=?1 AND ((status IN ('COMPLETED','FAILED','CANCELLED') AND reviewed_at IS NULL) OR status='UNKNOWN') ORDER BY updated_at DESC").map_err(db_error)?;
    let records = statement
        .query_map(params![workstream_id], provider_run_row)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(records)
}
fn attention_item(
    kind: &str,
    priority: i32,
    workstream: &Workstream,
    source_id: &str,
    message: &str,
    activity_at: i64,
) -> AttentionItem {
    AttentionItem {
        kind: kind.into(),
        priority,
        workstream_id: workstream.id.clone(),
        workstream_name: workstream.name.clone(),
        source_id: source_id.into(),
        message: message.into(),
        activity_at,
    }
}

fn handoff_by_id(connection: &Connection, id: &str) -> Result<HandoffHistoryItem, String> {
    let mut values = list_handoffs_where(connection, "h.id=?1", params![id])?;
    values.pop().ok_or("Persisted handoff not found".into())
}
fn list_handoffs(
    connection: &Connection,
    workstream_id: &str,
) -> Result<Vec<HandoffHistoryItem>, String> {
    list_handoffs_where(connection, "h.workstream_id=?1", params![workstream_id])
}

fn list_mobile_codex_outbound_reviews(
    connection: &Connection,
    workstream_id: &str,
) -> Result<Vec<MobileCodexOutboundReviewRecord>, String> {
    // Isolated preview databases intentionally do not use the daily Router's
    // feature-migration stream. Their ordinary Workspace projections remain
    // readable without inventing an empty feature table.
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='mobile_codex_outbound_reviews')",
        [],
        |row| row.get(0),
    ).map_err(db_error)?;
    if !exists {
        return Ok(vec![]);
    }
    let mut statement = connection.prepare(
        "SELECT action_id,workstream_id,source_run_id,source_reference_id,source_endpoint_id,source_codex_thread_id,destination_chatgpt_conversation_id,original_text,attachments_json,approved_text,revision,status,handoff_id,created_at,updated_at FROM mobile_codex_outbound_reviews WHERE workstream_id=?1 ORDER BY updated_at DESC,rowid DESC",
    ).map_err(db_error)?;
    let reviews = statement
        .query_map(params![workstream_id], |row| Ok(MobileCodexOutboundReviewRecord {
            action_id: row.get(0)?,
            workstream_id: row.get(1)?,
            source_run_id: row.get(2)?,
            source_reference_id: row.get(3)?,
            source_endpoint_id: row.get(4)?,
            source_codex_thread_id: row.get(5)?,
            destination_chatgpt_conversation_id: row.get(6)?,
            original_text: row.get(7)?,
            attachments_json: row.get(8)?,
            approved_text: row.get(9)?,
            revision: row.get(10)?,
            status: row.get(11)?,
            handoff_id: row.get(12)?,
            created_at: row.get(13)?,
            updated_at: row.get(14)?,
        }))
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(reviews)
}

fn list_mobile_chatgpt_inbound_reviews(
    connection: &Connection,
    workstream_id: &str,
) -> Result<Vec<MobileChatGptInboundReviewRecord>, String> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='mobile_chatgpt_inbound_reviews')",
        [],
        |row| row.get(0),
    ).map_err(db_error)?;
    if !exists { return Ok(vec![]); }
    let mut statement = connection.prepare(
        "SELECT action_id,workstream_id,source_run_id,source_reference_id,source_endpoint_id,source_chatgpt_conversation_id,source_response_identity,destination_codex_thread_id,original_text,attachments_json,approved_text,revision,status,handoff_id,created_at,updated_at FROM mobile_chatgpt_inbound_reviews WHERE workstream_id=?1 ORDER BY updated_at DESC,rowid DESC",
    ).map_err(db_error)?;
    let reviews = statement.query_map(params![workstream_id], |row| Ok(MobileChatGptInboundReviewRecord {
        action_id: row.get(0)?, workstream_id: row.get(1)?, source_run_id: row.get(2)?, source_reference_id: row.get(3)?, source_endpoint_id: row.get(4)?, source_chatgpt_conversation_id: row.get(5)?, source_response_identity: row.get(6)?, destination_codex_thread_id: row.get(7)?, original_text: row.get(8)?, attachments_json: row.get(9)?, approved_text: row.get(10)?, revision: row.get(11)?, status: row.get(12)?, handoff_id: row.get(13)?, created_at: row.get(14)?, updated_at: row.get(15)?,
    })).map_err(db_error)?.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
    Ok(reviews)
}
fn list_handoffs_where<P: rusqlite::Params>(
    connection: &Connection,
    predicate: &str,
    parameters: P,
) -> Result<Vec<HandoffHistoryItem>, String> {
    let source_columns = if migration::preflight(connection, migration::PREVIEW_SCHEMA)? >= 14 {
        "h.source_kind,h.source_context_id"
    } else {
        "'ENDPOINT',NULL"
    };
    let sql=format!("SELECT h.id,h.workstream_id,h.direction,h.source_response_identity,h.original_text,h.approved_text,h.status,h.payload_hash,h.created_at,h.approved_at,h.sent_at,h.failed_at,h.error_code,h.error_message,h.bridge_request_id,h.bridge_turn_key,h.bridge_correlation_observed_at,h.attention_acknowledged_at, s.id,s.workstream_id,s.provider,s.external_id,s.label,s.status,s.replaces_endpoint_id,s.created_at,s.superseded_at, d.id,d.workstream_id,d.provider,d.external_id,d.label,d.status,d.replaces_endpoint_id,d.created_at,d.superseded_at,{source_columns} FROM handoffs h LEFT JOIN endpoints s ON s.id=h.source_endpoint_id JOIN endpoints d ON d.id=h.destination_endpoint_id WHERE {predicate} ORDER BY h.created_at DESC");
    let mut statement = connection.prepare(&sql).map_err(db_error)?;
    let rows = statement
        .query_map(parameters, |row| {
            Ok(HandoffHistoryItem {
                id: row.get(0)?,
                workstream_id: row.get(1)?,
                direction: row.get(2)?,
                source_response_identity: row.get(3)?,
                original_text: row.get(4)?,
                approved_text: row.get(5)?,
                status: row.get(6)?,
                payload_hash: row.get(7)?,
                created_at: row.get(8)?,
                approved_at: row.get(9)?,
                sent_at: row.get(10)?,
                failed_at: row.get(11)?,
                error_code: row.get(12)?,
                error_message: row.get(13)?,
                bridge_request_id: row.get(14)?,
                bridge_turn_key: row.get(15)?,
                bridge_correlation_observed_at: row.get(16)?,
                attention_acknowledged_at: row.get(17)?,
                source: if row.get::<_, String>(36)? == "ENDPOINT" {
                    HandoffSource::Endpoint {
                        source_endpoint: Endpoint {
                            id: row.get(18)?,
                            workstream_id: row.get(19)?,
                            provider: row.get(20)?,
                            external_id: row.get(21)?,
                            label: row.get(22)?,
                            status: row.get(23)?,
                            replaces_endpoint_id: row.get(24)?,
                            created_at: row.get(25)?,
                            superseded_at: row.get(26)?,
                        },
                    }
                } else {
                    HandoffSource::ControlContext {
                        source_context_id: row.get(37)?,
                    }
                },
                destination_endpoint: Endpoint {
                    id: row.get(27)?,
                    workstream_id: row.get(28)?,
                    provider: row.get(29)?,
                    external_id: row.get(30)?,
                    label: row.get(31)?,
                    status: row.get(32)?,
                    replaces_endpoint_id: row.get(33)?,
                    created_at: row.get(34)?,
                    superseded_at: row.get(35)?,
                },
                attachments: vec![],
            })
        })
        .map_err(db_error)?;
    let mut handoffs = rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
    for handoff in &mut handoffs {
        let mut attachment_statement=connection.prepare("SELECT id,handoff_id,filename,original_path,size,sha256,integrity_status,send_sha256,send_verified_at,created_at FROM handoff_attachments WHERE handoff_id=?1 ORDER BY created_at,id").map_err(db_error)?;
        handoff.attachments = attachment_statement
            .query_map(params![handoff.id], |row| {
                Ok(HandoffAttachmentRecord {
                    id: row.get(0)?,
                    handoff_id: row.get(1)?,
                    filename: row.get(2)?,
                    original_path: row.get(3)?,
                    size: row.get(4)?,
                    sha256: row.get(5)?,
                    integrity_status: row.get(6)?,
                    send_sha256: row.get(7)?,
                    send_verified_at: row.get(8)?,
                    created_at: row.get(9)?,
                })
            })
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
    }
    Ok(handoffs)
}

fn ensure_workstream_not_trashed(workstream: &Workstream) -> Result<(), String> {
    if workstream.trashed_at.is_some() {
        Err("This Workstream is in the recycle bin. Restore it before changing bindings or execution state.".into())
    } else {
        Ok(())
    }
}

fn increment_binding_revision(
    connection: &Connection,
    workstream_id: &str,
    timestamp: i64,
) -> Result<i64, String> {
    connection
        .execute(
            "UPDATE workstreams SET binding_revision=binding_revision+1,updated_at=?2 WHERE id=?1",
            params![workstream_id, timestamp],
        )
        .map_err(db_error)?;
    connection
        .query_row(
            "SELECT binding_revision FROM workstreams WHERE id=?1",
            params![workstream_id],
            |row| row.get(0),
        )
        .map_err(db_error)
}

fn pair_endpoint_side(
    connection: &Connection,
    workstream_id: &str,
    provider: &str,
    side: Option<EndpointPairingSide>,
    timestamp: i64,
) -> Result<(Option<Endpoint>, bool), String> {
    let Some(side) = side else {
        return Ok((None, false));
    };
    let external_id = nonempty(&side.external_id, "External endpoint identity")?;
    let label = nonempty(&side.label, "Endpoint label")?;
    // The durable uniqueness constraint spans endpoint history. Check the
    // other-Workstream case before an INSERT so Review receives an actionable
    // conflict rather than a redacted SQLite constraint failure. A matching
    // historical identity on this Workstream remains eligible for explicit
    // reactivation below.
    let claimed_elsewhere: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM endpoints WHERE provider=?1 AND external_id=?2 AND workstream_id!=?3)",
            params![provider, external_id, workstream_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if claimed_elsewhere {
        return Err(match provider {
            "CHATGPT" => "CHATGPT_CONVERSATION_ALREADY_BOUND",
            "CODEX" => "CODEX_THREAD_ALREADY_BOUND",
            _ => "EXTERNAL_ENDPOINT_ALREADY_BOUND",
        }
        .into());
    }
    let current = active_endpoint(connection, workstream_id, provider)?;
    match current {
        Some(current) => {
            if side.expected_active_endpoint_id.as_deref() != Some(current.id.as_str()) {
                return Err(format!("The ACTIVE {provider} Endpoint changed after this pairing review. Reopen the pairing review before saving."));
            }
            if current.external_id == external_id {
                return Ok((Some(current), false));
            }
            // External provider identity is globally unique. A user may
            // deliberately return a Workstream to one of its own previously
            // paired exact conversations, so do not try to insert a duplicate
            // Endpoint row (which would fail the uniqueness guard and hide a
            // valid reviewed pairing behind an INTERNAL error). Reactivate
            // only that same Workstream/provider's historical identity; its
            // existing handoffs and runs continue to point to the same row.
            let historical = connection
                .query_row(
                    "SELECT id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at,superseded_at FROM endpoints WHERE workstream_id=?1 AND provider=?2 AND external_id=?3 AND status='SUPERSEDED'",
                    params![workstream_id, provider, external_id],
                    endpoint_row,
                )
                .optional()
                .map_err(db_error)?;
            connection.execute(
                "UPDATE endpoints SET status='SUPERSEDED',superseded_at=?2 WHERE id=?1 AND status='ACTIVE'",
                params![current.id, timestamp],
            ).map_err(db_error)?;
            if let Some(mut historical) = historical {
                connection.execute(
                    "UPDATE endpoints SET status='ACTIVE',superseded_at=NULL WHERE id=?1 AND status='SUPERSEDED'",
                    params![historical.id],
                ).map_err(db_error)?;
                historical.status = "ACTIVE".into();
                historical.superseded_at = None;
                return Ok((Some(historical), true));
            }
            let endpoint = Endpoint {
                id: id(),
                workstream_id: workstream_id.into(),
                provider: provider.into(),
                external_id,
                label,
                status: "ACTIVE".into(),
                replaces_endpoint_id: Some(current.id),
                created_at: timestamp,
                superseded_at: None,
            };
            connection.execute(
                "INSERT INTO endpoints (id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at,superseded_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                endpoint_params(&endpoint),
            ).map_err(db_error)?;
            Ok((Some(endpoint), true))
        }
        None => {
            if side.expected_active_endpoint_id.is_some() {
                return Err(format!("The reviewed ACTIVE {provider} Endpoint no longer exists. Reopen the pairing review before saving."));
            }
            // A Workstream can have no ACTIVE endpoint after an earlier
            // replacement/cleanup while still retaining this exact identity
            // in its lineage. Re-activate that local history instead of
            // attempting a duplicate insert against the global uniqueness
            // constraint. The `claimed_elsewhere` check above keeps this
            // strictly local to the reviewed Workstream.
            let historical = connection
                .query_row(
                    "SELECT id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at,superseded_at FROM endpoints WHERE workstream_id=?1 AND provider=?2 AND external_id=?3 AND status!='ACTIVE'",
                    params![workstream_id, provider, external_id],
                    endpoint_row,
                )
                .optional()
                .map_err(db_error)?;
            if let Some(mut historical) = historical {
                connection
                    .execute(
                        "UPDATE endpoints SET status='ACTIVE',superseded_at=NULL WHERE id=?1 AND status!='ACTIVE'",
                        params![historical.id],
                    )
                    .map_err(db_error)?;
                historical.status = "ACTIVE".into();
                historical.superseded_at = None;
                return Ok((Some(historical), true));
            }
            let endpoint = Endpoint {
                id: id(),
                workstream_id: workstream_id.into(),
                provider: provider.into(),
                external_id,
                label,
                status: "ACTIVE".into(),
                replaces_endpoint_id: None,
                created_at: timestamp,
                superseded_at: None,
            };
            connection.execute(
                "INSERT INTO endpoints (id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at,superseded_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                endpoint_params(&endpoint),
            ).map_err(db_error)?;
            Ok((Some(endpoint), true))
        }
    }
}

fn ensure_workstream_purge_safe(
    connection: &Connection,
    workstream_id: &str,
) -> Result<(), String> {
    let active_or_unknown_runs: i64 = connection.query_row(
        "SELECT COUNT(*) FROM provider_runs WHERE workstream_id=?1 AND status IN ('STARTING','RUNNING','UNKNOWN')",
        params![workstream_id], |row| row.get(0),
    ).map_err(db_error)?;
    if active_or_unknown_runs > 0 {
        return Err(
            "Permanent local purge is blocked while a ProviderRun is STARTING, RUNNING, or UNKNOWN"
                .into(),
        );
    }
    let pending_handoffs: i64 = connection.query_row(
        "SELECT COUNT(*) FROM handoffs WHERE workstream_id=?1 AND status IN ('DETECTED','READY','APPROVED','SENDING')",
        params![workstream_id], |row| row.get(0),
    ).map_err(db_error)?;
    if pending_handoffs > 0 {
        return Err("Permanent local purge is blocked while a Handoff is not terminal".into());
    }
    let unread_replies: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM reply_observations WHERE workstream_id=?1 AND read_at IS NULL",
            params![workstream_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if unread_replies > 0 {
        return Err("Permanent local purge is blocked while an exact reply is unread".into());
    }
    Ok(())
}

/// Normalizes only the provider's real Project URL shape. It does not accept
/// conversation URLs, a root URL, query/fragment hints, titles, or paths from
/// a different host as a substitute for a provider Project identity.
pub fn normalize_chatgpt_project_url(value: &str) -> Result<(String, String), String> {
    let url = Url::parse(value.trim())
        .map_err(|_| "ChatGPT Project link must be a valid URL".to_string())?;
    if url.scheme() != "https"
        || url.host_str() != Some("chatgpt.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("ChatGPT Project link must be an exact https://chatgpt.com/g/<project-id>/project URL without credentials, query, or fragment".into());
    }
    let segments = url
        .path_segments()
        .ok_or("ChatGPT Project link path is invalid")?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.len() != 3 || segments[0] != "g" || segments[2] != "project" {
        return Err(
            "ChatGPT Project link must use the canonical /g/<project-id>/project path".into(),
        );
    }
    let project_id = segments[1];
    if !(3..=128).contains(&project_id.len())
        || !project_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err("ChatGPT Project identity has an invalid canonical shape".into());
    }
    Ok((
        format!("https://chatgpt.com/g/{project_id}/project"),
        project_id.to_string(),
    ))
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|error| format!("Could not read verified backup: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not hash verified backup: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn id() -> String {
    Uuid::new_v4().to_string()
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}
fn nonempty(value: &str, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(value.to_string())
    }
}
fn payload_hash(source_identity: &Option<String>, destination: &str, text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_identity.as_deref().unwrap_or_default().as_bytes());
    hasher.update([0]);
    hasher.update(destination.as_bytes());
    hasher.update([0]);
    hasher.update(text.trim().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn valid_handoff_transition(current: &str, next: &str) -> bool {
    matches!(
        (current, next),
        ("READY", "APPROVED")
            | ("READY", "CANCELLED")
            | ("APPROVED", "SENDING")
            | ("APPROVED", "CANCELLED")
            | ("SENDING", "SENT")
            | ("SENDING", "FAILED")
    )
}

fn db_error(error: rusqlite::Error) -> String {
    format!("Router persistence error: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    fn store() -> (tempfile::TempDir, RouterStore) {
        let dir = tempdir().unwrap();
        let store = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        (dir, store)
    }
    fn project_workstream(store: &RouterStore) -> (Project, Workstream) {
        let project = store
            .create_project("Persistence Smoke".into(), None)
            .unwrap();
        let workstream = store
            .create_workstream(&project.id, "WS-001".into())
            .unwrap();
        (project, workstream)
    }
    #[test]
    fn migrations_are_idempotent_and_create_expected_tables() {
        let (dir, store) = store();
        drop(store);
        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        let snapshot = reopened.snapshot().unwrap();
        assert!(snapshot.projects.is_empty());
        reopened
            .with_connection(|connection| {
                let mut statement = connection
                    .prepare("SELECT name, sql FROM sqlite_master WHERE type='table' ORDER BY name")
                    .map_err(db_error)?;
                let schema = statement
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .map_err(db_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(db_error)?;
                let names = schema
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>();
                assert_eq!(
                    names,
                    vec![
                        "app_settings",
                        "assistant_access",
                        "assistant_actions",
                        "assistant_approvals",
                        "assistant_decisions",
                        "assistant_drafts",
                        "assistant_grants",
                        "codex_feedback_drafts",
                        "codex_watch_events",
                        "codex_watches",
                        "endpoint_canonical_urls",
                        "endpoint_role_details",
                        "endpoints",
                        "external_project_links",
                        "handoff_attachments",
                        "handoffs",
                        "mcp_event_actions",
                        "mcp_event_deliveries",
                        "mcp_event_initial_observations",
                        "mcp_event_native_causes",
                        "mcp_event_request_sources",
                        "mcp_event_request_tombstones",
                        "mcp_event_subscriptions",
                        "mcp_events",
                        "mobile_chatgpt_inbound_reviews",
                        "mobile_codex_outbound_reviews",
                        "projects",
                        "provider_run_feedback_sources",
                        "provider_runs",
                        "push_subscriptions",
                        "reply_observations",
                        "reply_observer_seen_identities",
                        "reply_observer_stream_cursor",
                        "role_handoff_details",
                        "router_feature_migrations",
                        "schema_migrations",
                        "sqlite_sequence",
                        "watch_deliveries",
                        "watch_delivery_channels",
                        "watch_delivery_settings",
                        "watch_removed_items",
                        "watch_replies",
                        "watch_reply_cancellations",
                        "watch_seen_events",
                        "workstream_drafts",
                        "workstreams"
                    ]
                );
                let sql = schema
                    .into_iter()
                    .map(|(_, sql)| sql.to_lowercase())
                    .collect::<Vec<_>>()
                    .join("\n");
                for forbidden in [
                    "api_token",
                    "bridge_token",
                    "cookie",
                    "credential",
                    "chat_messages",
                    "protocol_events",
                    "raw_chatgpt_transcripts",
                    "p256dh",
                    "vapid_private_key",
                    "subscription_auth",
                ] {
                    assert!(
                        !sql.contains(forbidden),
                        "schema unexpectedly contains {forbidden}"
                    );
                }
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn exact_chatgpt_canonical_url_is_endpoint_scoped_and_survives_reopen() {
        let (dir, store) = store();
        let (_, workstream) = project_workstream(&store);
        let chatgpt = store.bind_endpoint(
            &workstream.id, Provider::Chatgpt, "12345678-abcd".into(), "ChatGPT".into(), false,
        ).unwrap();
        let codex = store.bind_endpoint(
            &workstream.id, Provider::Codex, "thread-12345678".into(), "Codex".into(), false,
        ).unwrap();
        let exact = "https://chatgpt.com/g/g-p-owner/c/12345678-abcd";
        store.save_chatgpt_endpoint_canonical_url(&chatgpt.id, exact).unwrap();
        assert_eq!(store.chatgpt_endpoint_canonical_url(&chatgpt.id).unwrap().as_deref(), Some(exact));
        assert!(store.save_chatgpt_endpoint_canonical_url(&codex.id, exact).is_err());
        assert!(store.save_chatgpt_endpoint_canonical_url(&chatgpt.id, "https://chatgpt.com/c/not-the-bound-id").is_err());
        drop(store);
        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        assert_eq!(reopened.chatgpt_endpoint_canonical_url(&chatgpt.id).unwrap().as_deref(), Some(exact));
    }

    #[test]
    fn external_project_links_normalize_chatgpt_and_keep_provider_associations_independent() {
        let (dir, store) = store();
        let (project, _) = project_workstream(&store);
        let chatgpt = store
            .upsert_external_project_link(
                &project.id,
                NewExternalProjectLink {
                    provider: Provider::Chatgpt,
                    external_project_id: "g-p-stage_123".into(),
                    canonical_url: Some("https://chatgpt.com/g/g-p-stage_123/project/".into()),
                    label: "Stage project".into(),
                    source_kind: "saved-project-url".into(),
                    source_version: Some("web".into()),
                    verified_at: None,
                },
            )
            .unwrap();
        assert_eq!(
            chatgpt.canonical_url.as_deref(),
            Some("https://chatgpt.com/g/g-p-stage_123/project")
        );
        assert!(store
            .upsert_external_project_link(
                &project.id,
                NewExternalProjectLink {
                    provider: Provider::Chatgpt,
                    external_project_id: "wrong-id".into(),
                    canonical_url: Some("https://chatgpt.com/g/g-p-stage_123/project".into()),
                    label: "Wrong".into(),
                    source_kind: "saved-project-url".into(),
                    source_version: None,
                    verified_at: None,
                },
            )
            .is_err());
        let codex = store
            .upsert_external_project_link(
                &project.id,
                NewExternalProjectLink {
                    provider: Provider::Codex,
                    external_project_id: "native-project-directory-id".into(),
                    canonical_url: None,
                    label: "Existing Codex project".into(),
                    source_kind: "native-registered-directory".into(),
                    source_version: Some("current-desktop".into()),
                    verified_at: Some(10),
                },
            )
            .unwrap();
        assert_eq!(codex.provider, "CODEX");
        assert_eq!(
            store
                .external_project_links_for_project(&project.id)
                .unwrap()
                .len(),
            2
        );
        assert!(normalize_chatgpt_project_url("https://chatgpt.com/c/not-a-project").is_err());
        assert!(normalize_chatgpt_project_url("https://chatgpt.com/g/g-p-stage_123").is_err());
        assert!(normalize_chatgpt_project_url("https://chatgpt.com/g/g-p-stage_123?x=1").is_err());
        assert!(normalize_chatgpt_project_url(
            "https://chatgpt.com/g/g-p-stage_123/c/not-a-project"
        )
        .is_err());
        drop(store);
        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        assert_eq!(
            reopened
                .external_project_links_for_project(&project.id)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn endpoint_pairing_is_atomic_revision_guarded_and_exact() {
        let (_dir, store) = store();
        let (project, first) = project_workstream(&store);
        let second = store
            .create_workstream(&project.id, "Second".into())
            .unwrap();
        let taken_codex = store
            .bind_endpoint(
                &second.id,
                Provider::Codex,
                "taken-thread".into(),
                "Taken".into(),
                false,
            )
            .unwrap();
        let before = store.snapshot_for_workstream(&first.id).unwrap();
        let failed = store.pair_workstream_endpoints_checked(
            &first.id,
            EndpointPairingRequest {
                expected_binding_revision: 0,
                chatgpt: Some(EndpointPairingSide {
                    expected_active_endpoint_id: None,
                    external_id: "new-conversation".into(),
                    label: "New conversation".into(),
                }),
                codex: Some(EndpointPairingSide {
                    expected_active_endpoint_id: None,
                    external_id: taken_codex.external_id.clone(),
                    label: "Collision".into(),
                }),
            },
        );
        assert_eq!(failed.unwrap_err(), "CODEX_THREAD_ALREADY_BOUND");
        let after_failed = store.snapshot_for_workstream(&first.id).unwrap();
        assert!(after_failed.active_chatgpt_endpoint.is_none());
        assert!(after_failed.active_codex_endpoint.is_none());
        assert_eq!(
            after_failed
                .workstreams
                .iter()
                .find(|workstream| workstream.id == first.id)
                .unwrap()
                .binding_revision,
            before
                .workstreams
                .iter()
                .find(|workstream| workstream.id == first.id)
                .unwrap()
                .binding_revision
        );

        let paired = store
            .pair_workstream_endpoints_checked(
                &first.id,
                EndpointPairingRequest {
                    expected_binding_revision: 0,
                    chatgpt: Some(EndpointPairingSide {
                        expected_active_endpoint_id: None,
                        external_id: "new-conversation".into(),
                        label: "New conversation".into(),
                    }),
                    codex: Some(EndpointPairingSide {
                        expected_active_endpoint_id: None,
                        external_id: "new-thread".into(),
                        label: "New thread".into(),
                    }),
                },
            )
            .unwrap();
        assert_eq!(paired.binding_revision, 1);
        let first_chatgpt = paired.chatgpt_endpoint.unwrap();
        assert!(store
            .pair_workstream_endpoints_checked(
                &first.id,
                EndpointPairingRequest {
                    expected_binding_revision: 0,
                    chatgpt: Some(EndpointPairingSide {
                        expected_active_endpoint_id: Some(first_chatgpt.id.clone()),
                        external_id: "changed".into(),
                        label: "Changed".into(),
                    }),
                    codex: None,
                },
            )
            .is_err());
        assert_eq!(
            store
                .active_endpoint_for_workstream(&first.id, Provider::Chatgpt)
                .unwrap()
                .unwrap()
                .external_id,
            "new-conversation"
        );
    }

    #[test]
    fn chatgpt_pairing_reports_an_exact_other_workstream_conflict() {
        let (_dir, store) = store();
        let (project, first) = project_workstream(&store);
        let second = store
            .create_workstream(&project.id, "Second".into())
            .unwrap();
        store
            .bind_endpoint(
                &second.id,
                Provider::Chatgpt,
                "exact-conversation".into(),
                "Taken chat".into(),
                false,
            )
            .unwrap();

        let error = store
            .pair_workstream_endpoints_checked(
                &first.id,
                EndpointPairingRequest {
                    expected_binding_revision: 0,
                    chatgpt: Some(EndpointPairingSide {
                        expected_active_endpoint_id: None,
                        external_id: "exact-conversation".into(),
                        label: "Requested chat".into(),
                    }),
                    codex: None,
                },
            )
            .unwrap_err();
        assert_eq!(error, "CHATGPT_CONVERSATION_ALREADY_BOUND");
        assert!(store
            .active_endpoint_for_workstream(&first.id, Provider::Chatgpt)
            .unwrap()
            .is_none());
    }

    #[test]
    fn pairing_reactivates_its_own_historical_identity_without_an_active_endpoint() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let original = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "same-conversation".into(),
                "Original chat".into(),
                false,
            )
            .unwrap();
        store
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE endpoints SET status='SUPERSEDED',superseded_at=1 WHERE id=?1",
                        params![original.id],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let revision = store
            .snapshot_for_workstream(&workstream.id)
            .unwrap()
            .workstreams
            .into_iter()
            .find(|item| item.id == workstream.id)
            .unwrap()
            .binding_revision;

        let restored = store
            .pair_workstream_endpoints_checked(
                &workstream.id,
                EndpointPairingRequest {
                    expected_binding_revision: revision,
                    chatgpt: Some(EndpointPairingSide {
                        expected_active_endpoint_id: None,
                        external_id: "same-conversation".into(),
                        label: "Ignored display label".into(),
                    }),
                    codex: None,
                },
            )
            .unwrap()
            .chatgpt_endpoint
            .unwrap();
        assert_eq!(restored.id, original.id);
        assert_eq!(restored.status, "ACTIVE");
    }

    #[test]
    fn pristine_chatgpt_endpoint_transfer_preserves_exact_identity_and_revisions() {
        let dir = tempdir().unwrap();
        let profile =
            crate::runtime::PreviewProfile::acquire(dir.path().join("mcp-preview")).unwrap();
        RouterStore::initialize_preview(&profile).unwrap();
        let store = RouterStore::open_preview(profile).unwrap();
        let (project, source) = project_workstream(&store);
        let target = store
            .create_workstream(&project.id, "Target".into())
            .unwrap();
        let endpoint = store
            .bind_endpoint(
                &source.id,
                Provider::Chatgpt,
                "exact-chat".into(),
                "Source chat".into(),
                false,
            )
            .unwrap();
        let source_revision = store
            .snapshot_for_workstream(&source.id)
            .unwrap()
            .workstreams
            .into_iter()
            .find(|item| item.id == source.id)
            .unwrap()
            .binding_revision;
        let target_revision = store
            .snapshot_for_workstream(&target.id)
            .unwrap()
            .workstreams
            .into_iter()
            .find(|item| item.id == target.id)
            .unwrap()
            .binding_revision;
        let moved = store
            .transfer_active_chatgpt_endpoint_checked(ChatGptEndpointTransferRequest {
                source_workstream_id: source.id.clone(),
                target_workstream_id: target.id.clone(),
                source_endpoint_id: endpoint.id.clone(),
                conversation_id: endpoint.external_id.clone(),
                expected_source_binding_revision: source_revision,
                expected_target_binding_revision: target_revision,
            })
            .unwrap();
        assert_eq!(moved.id, endpoint.id);
        assert_eq!(moved.workstream_id, target.id);
        assert!(store
            .active_endpoint_for_workstream(&source.id, Provider::Chatgpt)
            .unwrap()
            .is_none());
        assert_eq!(
            store
                .active_endpoint_for_workstream(&target.id, Provider::Chatgpt)
                .unwrap()
                .unwrap()
                .external_id,
            "exact-chat"
        );
    }

    #[test]
    fn endpoint_pairing_reactivates_its_own_historical_exact_identity() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let first = store
            .pair_workstream_endpoints_checked(
                &workstream.id,
                EndpointPairingRequest {
                    expected_binding_revision: 0,
                    chatgpt: Some(EndpointPairingSide {
                        expected_active_endpoint_id: None,
                        external_id: "original-conversation".into(),
                        label: "Original conversation".into(),
                    }),
                    codex: None,
                },
            )
            .unwrap()
            .chatgpt_endpoint
            .unwrap();
        let replacement = store
            .pair_workstream_endpoints_checked(
                &workstream.id,
                EndpointPairingRequest {
                    expected_binding_revision: 1,
                    chatgpt: Some(EndpointPairingSide {
                        expected_active_endpoint_id: Some(first.id.clone()),
                        external_id: "replacement-conversation".into(),
                        label: "Replacement conversation".into(),
                    }),
                    codex: None,
                },
            )
            .unwrap()
            .chatgpt_endpoint
            .unwrap();
        let restored = store
            .pair_workstream_endpoints_checked(
                &workstream.id,
                EndpointPairingRequest {
                    expected_binding_revision: 2,
                    chatgpt: Some(EndpointPairingSide {
                        expected_active_endpoint_id: Some(replacement.id.clone()),
                        external_id: "original-conversation".into(),
                        label: "Original conversation".into(),
                    }),
                    codex: None,
                },
            )
            .unwrap();
        let active = restored.chatgpt_endpoint.unwrap();
        assert_eq!(restored.binding_revision, 3);
        assert_eq!(active.id, first.id);
        assert_eq!(active.external_id, "original-conversation");
        assert_eq!(active.status, "ACTIVE");
        assert_eq!(
            store
                .active_endpoint_for_workstream(&workstream.id, Provider::Chatgpt)
                .unwrap()
                .unwrap()
                .id,
            first.id
        );
        let snapshot = store.snapshot_for_workstream(&workstream.id).unwrap();
        assert_eq!(
            snapshot
                .endpoint_lineage
                .iter()
                .filter(|endpoint| endpoint.provider == "CHATGPT")
                .count(),
            2
        );
    }

    #[test]
    fn pin_is_local_reopenable_and_trash_remains_visible_but_not_pinnable() {
        let (dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let pinned = store.set_workstream_pinned(&workstream.id, true).unwrap();
        assert!(pinned.pinned_at.is_some());
        assert!(store
            .snapshot()
            .unwrap()
            .workstreams
            .iter()
            .find(|item| item.id == workstream.id)
            .unwrap()
            .pinned_at
            .is_some());
        drop(store);
        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        assert!(reopened
            .snapshot_for_workstream(&workstream.id)
            .unwrap()
            .workstreams
            .iter()
            .find(|item| item.id == workstream.id)
            .unwrap()
            .pinned_at
            .is_some());
        let trashed = reopened.trash_workstream(&workstream.id).unwrap();
        assert!(trashed.trashed_at.is_some());
        assert!(reopened
            .set_workstream_pinned(&workstream.id, false)
            .is_err());
        assert!(reopened
            .snapshot()
            .unwrap()
            .workstreams
            .iter()
            .any(|item| item.id == workstream.id && item.trashed_at.is_some()));
    }

    #[test]
    fn lifecycle_is_recoverable_and_purge_is_guarded() {
        let (_dir, store) = store();
        let (project, workstream) = project_workstream(&store);
        let other_project = store
            .create_project("Separate recycle scope".into(), None)
            .unwrap();
        let other_workstream = store
            .create_workstream(&other_project.id, "WS-other".into())
            .unwrap();
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-lifecycle".into(),
                "Thread".into(),
                false,
            )
            .unwrap();
        let archived = store.archive_workstream(&workstream.id).unwrap();
        assert_eq!(archived.status, "ARCHIVED");
        let trashed = store.trash_workstream(&workstream.id).unwrap();
        assert!(trashed.trashed_at.is_some());
        let other_trashed = store.trash_workstream(&other_workstream.id).unwrap();
        assert!(other_trashed.trashed_at.is_some());
        assert_eq!(
            store
                .trashed_workstreams_for_project(&project.id)
                .unwrap()
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec![workstream.id.as_str()]
        );
        assert_eq!(
            store
                .trashed_workstreams_for_project(&other_project.id)
                .unwrap()
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec![other_workstream.id.as_str()]
        );
        assert!(store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "blocked".into(),
                "Blocked".into(),
                false
            )
            .is_err());
        assert!(store
            .purge_trashed_workstream_confirmed(&workstream.id, trashed.binding_revision, "no")
            .is_err());
        let restored = store.restore_workstream(&workstream.id).unwrap();
        assert_eq!(restored.status, "ARCHIVED");
        assert!(restored.trashed_at.is_none());
        let active = store.restore_workstream(&workstream.id).unwrap();
        assert_eq!(active.status, "ACTIVE");
        assert_eq!(
            store
                .active_endpoint_for_workstream(&workstream.id, Provider::Codex)
                .unwrap()
                .unwrap()
                .id,
            endpoint.id
        );
        let trashed_again = store.trash_workstream(&workstream.id).unwrap();
        store
            .purge_trashed_workstream_confirmed(
                &workstream.id,
                trashed_again.binding_revision,
                "PURGE_LOCAL_WORKSTREAM",
            )
            .unwrap();
        assert!(store.snapshot_for_workstream(&workstream.id).is_err());
    }

    #[test]
    fn workstream_draft_is_revision_guarded_and_never_creates_provider_state() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        assert!(store.workstream_draft(&workstream.id).unwrap().is_none());
        let first = store
            .save_workstream_draft(&workstream.id, "local only".into(), None)
            .unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(first.text, "local only");
        assert!(store
            .save_workstream_draft(&workstream.id, "stale overwrite".into(), None)
            .is_err());
        let second = store
            .save_workstream_draft(&workstream.id, "revised".into(), Some(first.revision))
            .unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(
            store
                .provider_runs_for_workstream(&workstream.id)
                .unwrap()
                .len(),
            0
        );
        assert!(store
            .snapshot_for_workstream(&workstream.id)
            .unwrap()
            .handoffs
            .is_empty());
    }

    #[test]
    fn codex_feedback_draft_is_result_scoped_revision_guarded_and_local_only() {
        let (dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let codex = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-feedback-draft".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        let first_result = store
            .create_provider_run(
                &workstream.id,
                &codex.id,
                "CODEX",
                None,
                Some("turn-feedback-draft-a"),
                "RUNNING",
            )
            .unwrap();
        store
            .accept_completed_provider_result(
                &first_result.id,
                "turn-feedback-draft-a",
                "result-feedback-draft-a",
                "first result".into(),
            )
            .unwrap();
        let second_result = store
            .create_provider_run(
                &workstream.id,
                &codex.id,
                "CODEX",
                None,
                Some("turn-feedback-draft-b"),
                "RUNNING",
            )
            .unwrap();
        store
            .accept_completed_provider_result(
                &second_result.id,
                "turn-feedback-draft-b",
                "result-feedback-draft-b",
                "second result".into(),
            )
            .unwrap();

        let first = store
            .save_codex_feedback_draft(
                &workstream.id,
                &first_result.id,
                "only result A".into(),
                None,
            )
            .unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(first.text, "only result A");
        assert!(store
            .codex_feedback_draft(&workstream.id, &second_result.id)
            .unwrap()
            .is_none());
        assert!(store
            .save_codex_feedback_draft(
                &workstream.id,
                &first_result.id,
                "stale overwrite".into(),
                None,
            )
            .is_err());
        let second = store
            .save_codex_feedback_draft(
                &workstream.id,
                &first_result.id,
                "revised A".into(),
                Some(first.revision),
            )
            .unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(
            store
                .codex_feedback_draft(&workstream.id, &first_result.id)
                .unwrap()
                .unwrap()
                .text,
            "revised A"
        );
        assert!(store
            .snapshot_for_workstream(&workstream.id)
            .unwrap()
            .handoffs
            .is_empty());
        drop(store);

        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        let restored = reopened
            .codex_feedback_draft(&workstream.id, &first_result.id)
            .unwrap()
            .unwrap();
        assert_eq!(restored.text, "revised A");
        assert_eq!(restored.revision, 2);
    }

    #[test]
    fn verified_backup_is_consistent_reopenable_and_never_overwrites() {
        let (dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "backup-conversation".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let target = dir.path().join("router-backup.db");
        let backup = store.create_verified_backup(&target).unwrap();
        assert_eq!(backup.path, target.to_string_lossy());
        assert_eq!(backup.sha256.len(), 64);
        assert!(backup.bytes > 0);
        assert!(store.create_verified_backup(&target).is_err());
        let reopened = RouterStore::open_at(&target).unwrap();
        assert_eq!(reopened.snapshot().unwrap().projects.len(), 1);
    }

    #[test]
    fn reply_observations_are_exact_durable_unread_and_trim_only_read_history() {
        let (dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "exact-conversation".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let first = store
            .record_reply_observation(
                &workstream.id,
                &endpoint.id,
                Some("assistant-message-1"),
                "First complete reply",
                None,
            )
            .unwrap()
            .unwrap();
        let second = store
            .record_reply_observation(
                &workstream.id,
                &endpoint.id,
                Some("assistant-message-2"),
                "Second complete reply",
                None,
            )
            .unwrap()
            .unwrap();
        assert!(store
            .record_reply_observation(
                &workstream.id,
                &endpoint.id,
                Some("assistant-message-1"),
                "Changed text is not identity authority",
                None,
            )
            .unwrap()
            .is_none());
        assert_eq!(
            store
                .provider_runs_for_workstream(&workstream.id)
                .unwrap()
                .len(),
            0
        );
        store.record_reply_push_attempt(&first.id, "SENT").unwrap();
        store
            .record_reply_push_attempt(&second.id, "FAILED")
            .unwrap();
        let observed = store
            .reply_observations_for_workstream(&workstream.id)
            .unwrap();
        assert_eq!(observed.len(), 2);
        assert!(observed
            .iter()
            .all(|item| item.source_provider_run_id.is_none()));
        assert!(observed.iter().all(|item| item.read_at.is_none()));
        assert_eq!(
            observed
                .iter()
                .find(|item| item.id == first.id)
                .unwrap()
                .push_state,
            "SENT"
        );
        assert!(observed
            .iter()
            .find(|item| item.id == first.id)
            .unwrap()
            .push_rendered_at
            .is_none());
        store
            .record_reply_push_rendered(&workstream.id, &first.id)
            .unwrap();
        assert!(store
            .reply_observations_for_workstream(&workstream.id)
            .unwrap()
            .iter()
            .find(|item| item.id == first.id)
            .unwrap()
            .push_rendered_at
            .is_some());
        assert!(store
            .record_reply_push_rendered(&workstream.id, &second.id)
            .is_err());
        assert!(store
            .record_reply_push_rendered("other-workstream", &first.id)
            .is_err());
        assert_eq!(
            observed
                .iter()
                .find(|item| item.id == second.id)
                .unwrap()
                .push_state,
            "FAILED"
        );
        store
            .acknowledge_reply_observation(&workstream.id, &first.id, true)
            .unwrap();
        drop(store);
        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        let after_reopen = reopened
            .reply_observations_for_workstream(&workstream.id)
            .unwrap();
        assert!(after_reopen
            .iter()
            .find(|item| item.id == first.id)
            .unwrap()
            .handled_at
            .is_some());
        assert!(after_reopen
            .iter()
            .find(|item| item.id == second.id)
            .unwrap()
            .read_at
            .is_none());

        for index in 0..21 {
            let observation = reopened
                .record_reply_observation(
                    &workstream.id,
                    &endpoint.id,
                    Some(&format!("bounded-message-{index}")),
                    &format!("Bounded reply {index}"),
                    None,
                )
                .unwrap()
                .unwrap();
            reopened
                .acknowledge_reply_observation(&workstream.id, &observation.id, true)
                .unwrap();
        }
        let trigger = reopened
            .record_reply_observation(
                &workstream.id,
                &endpoint.id,
                Some("bounded-trigger"),
                "Trigger bounded retention",
                None,
            )
            .unwrap()
            .unwrap();
        assert!(reopened
            .reply_observations_for_workstream(&workstream.id)
            .unwrap()
            .iter()
            .any(|item| item.id == trigger.id));
        let count: i64 = reopened
            .with_connection(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM reply_observations", [], |row| {
                        row.get(0)
                    })
                    .map_err(db_error)
            })
            .unwrap();
        assert_eq!(count, 22);
        let retained = reopened
            .reply_observations_for_workstream(&workstream.id)
            .unwrap();
        assert_eq!(retained.len(), 22);
        assert!(retained
            .iter()
            .any(|item| item.id == second.id && item.read_at.is_none()));
    }

    #[test]
    fn dashboard_surfaces_every_unread_reply_as_its_own_exact_attention_item() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let chatgpt = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "attention-conversation".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let codex = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "attention-thread".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        let first = store
            .record_reply_observation(
                &workstream.id,
                &chatgpt.id,
                Some("chatgpt-reply-one"),
                "First ChatGPT reply",
                None,
            )
            .unwrap()
            .unwrap();
        let second = store
            .record_reply_observation(
                &workstream.id,
                &codex.id,
                Some("codex-reply-one"),
                "Codex reply",
                None,
            )
            .unwrap()
            .unwrap();
        let third = store
            .record_reply_observation(
                &workstream.id,
                &chatgpt.id,
                Some("chatgpt-reply-two"),
                "Second ChatGPT reply",
                None,
            )
            .unwrap()
            .unwrap();

        let attention = store
            .dashboard_projection()
            .unwrap()
            .attention_items
            .into_iter()
            .filter(|item| {
                matches!(
                    item.kind.as_str(),
                    "CHATGPT_REPLY_OBSERVED" | "CODEX_REPLY_OBSERVED"
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(attention.len(), 3);
        assert_eq!(
            attention
                .iter()
                .map(|item| item.source_id.as_str())
                .collect::<Vec<_>>(),
            vec![third.id.as_str(), second.id.as_str(), first.id.as_str()]
        );
        assert_eq!(attention[0].kind, "CHATGPT_REPLY_OBSERVED");
        assert_eq!(attention[1].kind, "CODEX_REPLY_OBSERVED");

        store
            .acknowledge_reply_observation(&workstream.id, &second.id, true)
            .unwrap();
        let remaining_ids = store
            .dashboard_projection()
            .unwrap()
            .attention_items
            .into_iter()
            .filter(|item| {
                matches!(
                    item.kind.as_str(),
                    "CHATGPT_REPLY_OBSERVED" | "CODEX_REPLY_OBSERVED"
                )
            })
            .map(|item| item.source_id)
            .collect::<Vec<_>>();
        assert_eq!(remaining_ids, vec![third.id, first.id]);
    }

    #[test]
    fn reply_observation_rejects_an_endpoint_outside_its_exact_workstream() {
        let (_dir, store) = store();
        let (_project, first) = project_workstream(&store);
        let second = store
            .create_workstream(&first.project_id, "WS-002".into())
            .unwrap();
        let endpoint = store
            .bind_endpoint(
                &second.id,
                Provider::Chatgpt,
                "other-conversation".into(),
                "Other".into(),
                false,
            )
            .unwrap();
        assert!(store
            .record_reply_observation(&first.id, &endpoint.id, Some("message"), "Reply", None)
            .unwrap_err()
            .contains("ACTIVE provider Endpoint"));
    }

    #[test]
    fn passive_observer_cursor_and_exact_seen_identities_survive_reopen() {
        let (dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "exact-conversation".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        assert!(store.reply_observer_cursor().unwrap().is_none());
        assert!(store
            .remember_reply_observer_identity(&endpoint.id, "assistant-message-1")
            .unwrap());
        assert!(!store
            .remember_reply_observer_identity(&endpoint.id, "assistant-message-1")
            .unwrap());
        store.save_reply_observer_cursor("epoch-1", 7).unwrap();
        drop(store);
        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        assert!(reopened
            .reply_observer_identity_seen(&endpoint.id, "assistant-message-1")
            .unwrap());
        assert_eq!(
            reopened.reply_observer_cursor().unwrap(),
            Some(ReplyObserverCursor {
                stream_epoch: "epoch-1".into(),
                last_sequence: 7,
            })
        );
    }

    #[test]
    fn codex_empty_baseline_and_exact_reply_watermark_survive_reopen() {
        let (dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-exact".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        assert!(!store
            .codex_reply_observer_is_initialized(&endpoint.id)
            .unwrap());
        store
            .save_codex_reply_observer_empty_baseline(
                &workstream.id,
                &endpoint.id,
                "thread-exact",
            )
            .unwrap();
        assert!(store
            .codex_reply_observer_is_initialized(&endpoint.id)
            .unwrap());
        assert!(store
            .codex_reply_observer_watermark(&endpoint.id)
            .unwrap()
            .is_none());
        store
            .save_codex_reply_observer_watermark(
                &workstream.id,
                &endpoint.id,
                "thread-exact",
                "turn-exact",
                "item-exact",
            )
            .unwrap();
        drop(store);

        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        let (identity, _) = reopened
            .codex_reply_observer_watermark(&endpoint.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            identity,
            "codex-watermark:thread-exact:turn-exact:item-exact"
        );
        assert!(reopened
            .codex_reply_observer_is_initialized(&endpoint.id)
            .unwrap());
    }

    #[test]
    fn reply_observation_refuses_only_an_over_bound_complete_text() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        assert!(store
            .record_reply_observation(
                &workstream.id,
                &endpoint.id,
                Some("too-large"),
                &"a".repeat(MAX_REPLY_OBSERVATION_BYTES + 1),
                None,
            )
            .unwrap_err()
            .contains("bounded retention"));
    }
    #[test]
    fn project_workstream_and_restart_selection_persist() {
        let (dir, store) = store();
        let (project, workstream) = project_workstream(&store);
        store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "chat-1".into(),
                "Chat".into(),
                false,
            )
            .unwrap();
        store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-1".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        drop(store);
        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        let snapshot = reopened.snapshot().unwrap();
        assert_eq!(
            snapshot.selected_project_id.as_deref(),
            Some(project.id.as_str())
        );
        assert_eq!(
            snapshot.selected_workstream_id.as_deref(),
            Some(workstream.id.as_str())
        );
        assert_eq!(
            snapshot.active_chatgpt_endpoint.unwrap().external_id,
            "chat-1"
        );
        assert_eq!(
            snapshot.active_codex_endpoint.unwrap().external_id,
            "thread-1"
        );
    }

    #[test]
    fn focused_snapshot_is_exact_and_never_mutates_the_persisted_selection() {
        let (_dir, store) = store();
        let (project, first) = project_workstream(&store);
        let second = store
            .create_workstream(&project.id, "Second Workstream".into())
            .unwrap();
        store
            .select_workspace(&project.id, Some(&first.id))
            .unwrap();
        store
            .bind_endpoint(
                &second.id,
                Provider::Codex,
                "thread-second".into(),
                "Second Codex".into(),
                false,
            )
            .unwrap();

        let focused = store.snapshot_for_workstream(&second.id).unwrap();
        assert_eq!(
            focused.selected_workstream_id.as_deref(),
            Some(second.id.as_str())
        );
        assert_eq!(
            focused
                .active_codex_endpoint
                .as_ref()
                .map(|endpoint| endpoint.external_id.as_str()),
            Some("thread-second")
        );

        let persisted = store.snapshot().unwrap();
        assert_eq!(
            persisted.selected_workstream_id.as_deref(),
            Some(first.id.as_str())
        );
    }
    #[test]
    fn active_and_external_identity_constraints_hold() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-1".into(),
                "One".into(),
                false,
            )
            .unwrap();
        assert!(store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-2".into(),
                "Two".into(),
                false
            )
            .is_err());
        let project2 = store.create_project("Other".into(), None).unwrap();
        let workstream2 = store
            .create_workstream(&project2.id, "WS-002".into())
            .unwrap();
        assert!(store
            .bind_endpoint(
                &workstream2.id,
                Provider::Codex,
                "thread-1".into(),
                "Duplicate".into(),
                false
            )
            .is_err());
    }
    #[test]
    fn replacement_is_atomic_and_retains_lineage() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let old = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-old".into(),
                "Old".into(),
                false,
            )
            .unwrap();
        let next = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-new".into(),
                "New".into(),
                true,
            )
            .unwrap();
        assert_eq!(next.replaces_endpoint_id.as_deref(), Some(old.id.as_str()));
        assert_eq!(
            store
                .active_endpoint_for_workstream(&workstream.id, Provider::Codex)
                .unwrap()
                .unwrap()
                .id,
            next.id
        );
    }

    #[test]
    fn checked_rollover_requires_the_reviewed_active_endpoint_and_audits_codex_initialization() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let old = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-old".into(),
                "Old".into(),
                false,
            )
            .unwrap();
        let successor = store
            .replace_active_endpoint_checked(
                &workstream.id,
                Provider::Codex,
                &old.id,
                "thread-successor".into(),
                "Codex successor".into(),
                Some(CompletedRolloverRun {
                    external_run_id: "turn-initialize".into(),
                    result_identity: Some("item-result".into()),
                    started_at: 10,
                    terminal_at: 20,
                }),
            )
            .unwrap();
        assert_eq!(
            successor.replaces_endpoint_id.as_deref(),
            Some(old.id.as_str())
        );
        assert_eq!(
            store
                .active_endpoint_for_workstream(&workstream.id, Provider::Codex)
                .unwrap()
                .unwrap()
                .id,
            successor.id
        );
        store.with_connection(|connection| {
            let historical_status: String = connection.query_row("SELECT status FROM endpoints WHERE id=?1", params![old.id], |row| row.get(0)).map_err(db_error)?;
            let run: (String, String, Option<String>, Option<i64>) = connection.query_row(
                "SELECT endpoint_id,status,external_run_id,reviewed_at FROM provider_runs WHERE endpoint_id=?1",
                params![successor.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            ).map_err(db_error)?;
            assert_eq!(historical_status, "SUPERSEDED");
            assert_eq!(run.1, "COMPLETED");
            assert_eq!(run.2.as_deref(), Some("turn-initialize"));
            assert!(run.3.is_some());
            Ok(())
        }).unwrap();
    }

    #[test]
    fn checked_rollover_fails_closed_when_the_reviewed_endpoint_is_stale() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let old = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "chat-old".into(),
                "Old".into(),
                false,
            )
            .unwrap();
        let current = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "chat-current".into(),
                "Current".into(),
                true,
            )
            .unwrap();
        let error = store
            .replace_active_endpoint_checked(
                &workstream.id,
                Provider::Chatgpt,
                &old.id,
                "chat-candidate".into(),
                "Candidate".into(),
                None,
            )
            .unwrap_err();
        assert!(error.contains("changed after this rollover review"));
        assert_eq!(
            store
                .active_endpoint_for_workstream(&workstream.id, Provider::Chatgpt)
                .unwrap()
                .unwrap()
                .id,
            current.id
        );
    }

    #[test]
    fn restart_snapshot_projects_active_and_superseded_endpoint_lineage() {
        let (dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let old = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-old".into(),
                "Old".into(),
                false,
            )
            .unwrap();
        let successor = store
            .replace_active_endpoint_checked(
                &workstream.id,
                Provider::Codex,
                &old.id,
                "thread-new".into(),
                "New".into(),
                None,
            )
            .unwrap();
        drop(store);
        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        let snapshot = reopened.snapshot().unwrap();
        assert_eq!(snapshot.active_codex_endpoint.unwrap().id, successor.id);
        assert_eq!(snapshot.endpoint_lineage.len(), 2);
        assert!(snapshot
            .endpoint_lineage
            .iter()
            .any(|endpoint| endpoint.id == old.id && endpoint.status == "SUPERSEDED"));
        assert!(snapshot
            .endpoint_lineage
            .iter()
            .any(|endpoint| endpoint.id == successor.id && endpoint.status == "ACTIVE"));
    }
    #[test]
    fn failed_replacement_rolls_back_old_active() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let old = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-old".into(),
                "Old".into(),
                false,
            )
            .unwrap();
        let project2 = store.create_project("Other".into(), None).unwrap();
        let workstream2 = store
            .create_workstream(&project2.id, "WS-002".into())
            .unwrap();
        store
            .bind_endpoint(
                &workstream2.id,
                Provider::Codex,
                "taken".into(),
                "Taken".into(),
                false,
            )
            .unwrap();
        assert!(store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "taken".into(),
                "Bad".into(),
                true
            )
            .is_err());
        assert_eq!(
            store
                .active_endpoint_for_workstream(&workstream.id, Provider::Codex)
                .unwrap()
                .unwrap()
                .id,
            old.id
        );
    }
    #[test]
    fn handoff_history_keeps_old_endpoint_and_metadata_only_attachment() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let chat = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "chat-1".into(),
                "GPT-01".into(),
                false,
            )
            .unwrap();
        let old = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-1".into(),
                "CODEX-01".into(),
                false,
            )
            .unwrap();
        let handoff = store
            .create_ready_handoff(NewHandoff {
                workstream_id: workstream.id.clone(),
                source_endpoint_id: chat.id.clone(),
                destination_endpoint_id: old.id.clone(),
                direction: "CHATGPT_TO_CODEX".into(),
                source_response_identity: Some("response-1".into()),
                original_text: "original".into(),
                approved_text: "approved".into(),
                attachments: vec![NewAttachment {
                    id: id(),
                    filename: "proof.txt".into(),
                    original_path: "D:\\private\\proof.txt".into(),
                    size: Some(12),
                    sha256: Some("abc".into()),
                    integrity_status: Some("VERIFIED".into()),
                }],
            })
            .unwrap();
        store
            .transition_handoff(&handoff.id, "APPROVED", None)
            .unwrap();
        store
            .transition_handoff(&handoff.id, "SENDING", None)
            .unwrap();
        store.transition_handoff(&handoff.id, "SENT", None).unwrap();
        let historic_run = store
            .create_provider_run(
                &workstream.id,
                &old.id,
                "CODEX",
                None,
                Some("old-turn"),
                "RUNNING",
            )
            .unwrap();
        store
            .complete_provider_run("CODEX", "old-turn", "COMPLETED", Some("completed"))
            .unwrap();
        store
            .replace_active_endpoint_checked(
                &workstream.id,
                Provider::Codex,
                &old.id,
                "thread-2".into(),
                "CODEX-02".into(),
                None,
            )
            .unwrap();
        let history = store.snapshot().unwrap().handoffs;
        assert_eq!(history[0].destination_endpoint.external_id, "thread-1");
        assert_eq!(history[0].approved_text, "approved");
        assert_eq!(history[0].attachments[0].size, Some(12));
        assert_eq!(
            store
                .with_connection(|connection| provider_run_by_id(connection, &historic_run.id))
                .unwrap()
                .endpoint_id,
            old.id
        );
    }

    #[test]
    fn handoff_lifecycle_rejects_impossible_transitions() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let chat = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "chat-1".into(),
                "GPT".into(),
                false,
            )
            .unwrap();
        let codex = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-1".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        let handoff = store
            .create_ready_handoff(NewHandoff {
                workstream_id: workstream.id,
                source_endpoint_id: chat.id,
                destination_endpoint_id: codex.id,
                direction: "CHATGPT_TO_CODEX".into(),
                source_response_identity: None,
                original_text: "original".into(),
                approved_text: "approved".into(),
                attachments: vec![],
            })
            .unwrap();
        assert!(store.transition_handoff(&handoff.id, "SENT", None).is_err());
        store
            .transition_handoff(&handoff.id, "APPROVED", None)
            .unwrap();
        store
            .transition_handoff(&handoff.id, "SENDING", None)
            .unwrap();
        store
            .transition_handoff(
                &handoff.id,
                "FAILED",
                Some(("TEST".into(), "failure".into())),
            )
            .unwrap();
    }

    fn sending_codex_to_chatgpt(store: &RouterStore) -> HandoffHistoryItem {
        let (_project, workstream) = project_workstream(store);
        let codex = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-1".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        let chatgpt = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "chat-1".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let handoff = store
            .create_ready_handoff(NewHandoff {
                workstream_id: workstream.id,
                source_endpoint_id: codex.id.clone(),
                destination_endpoint_id: chatgpt.id.clone(),
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
        handoff
    }

    #[test]
    fn migration_two_upgrades_an_existing_migration_one_database_without_rewriting_rows() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("router.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(include_str!("../../migrations/001_initial_persistence.sql"))
            .unwrap();
        connection
            .execute_batch("CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY NOT NULL, applied_at INTEGER NOT NULL);")
            .unwrap();
        connection
            .execute(
                "INSERT INTO schema_migrations(version,applied_at) VALUES(1,1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO projects(id,name,created_at,updated_at) VALUES('p','P',1,1)",
                [],
            )
            .unwrap();
        connection.execute("INSERT INTO workstreams(id,project_id,name,status,created_at,updated_at) VALUES('w','p','W','ACTIVE',1,1)", []).unwrap();
        connection.execute("INSERT INTO endpoints(id,workstream_id,provider,external_id,label,status,created_at) VALUES('c','w','CODEX','thread','Codex','ACTIVE',1)", []).unwrap();
        connection.execute("INSERT INTO endpoints(id,workstream_id,provider,external_id,label,status,created_at) VALUES('g','w','CHATGPT','chat','ChatGPT','ACTIVE',1)", []).unwrap();
        connection.execute("INSERT INTO handoffs(id,workstream_id,source_endpoint_id,destination_endpoint_id,direction,original_text,approved_text,status,payload_hash,created_at) VALUES('h','w','c','g','CODEX_TO_CHATGPT','original','approved','SENDING','hash',1)", []).unwrap();
        drop(connection);

        let store = RouterStore::open_at(&path).unwrap();
        let handoff = store.handoff_by_id("h").unwrap();
        assert_eq!(handoff.approved_text, "approved");
        assert_eq!(handoff.bridge_request_id, None);
        store
            .with_connection(|connection| {
                let columns = connection
                    .prepare("PRAGMA table_info(handoffs)")
                    .map_err(db_error)?
                    .query_map([], |row| row.get::<_, String>(1))
                    .map_err(db_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(db_error)?;
                assert!(columns.contains(&"bridge_request_id".to_string()));
                assert!(columns.contains(&"bridge_turn_key".to_string()));
                assert!(columns.contains(&"bridge_correlation_observed_at".to_string()));
                assert!(columns.contains(&"attention_acknowledged_at".to_string()));
                let run_table: i64 = connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='provider_runs'", [], |row| row.get(0)).map_err(db_error)?;
                assert_eq!(run_table, 1);
                let run_columns = connection
                    .prepare("PRAGMA table_info(provider_runs)")
                    .map_err(db_error)?
                    .query_map([], |row| row.get::<_, String>(1))
                    .map_err(db_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(db_error)?;
                assert!(run_columns.contains(&"result_text".to_string()));
                let current_result_index: i64 = connection
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='provider_runs_current_result_unique'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(db_error)?;
                assert_eq!(current_result_index, 1);
                Ok(())
            })
            .unwrap();
        drop(store);
        RouterStore::open_at(&path).unwrap();
    }

    #[test]
    fn migration_four_preserves_historical_attachment_review_metadata_with_null_send_evidence() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("router.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(include_str!("../../migrations/001_initial_persistence.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!(
                "../../migrations/002_handoff_bridge_correlation.sql"
            ))
            .unwrap();
        connection
            .execute_batch(include_str!(
                "../../migrations/003_provider_runs_and_attention.sql"
            ))
            .unwrap();
        connection.execute_batch("CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY NOT NULL, applied_at INTEGER NOT NULL);").unwrap();
        for version in 1..=3 {
            connection
                .execute(
                    "INSERT INTO schema_migrations(version,applied_at) VALUES(?1,1)",
                    params![version],
                )
                .unwrap();
        }
        connection
            .execute(
                "INSERT INTO projects(id,name,created_at,updated_at) VALUES('p','P',1,1)",
                [],
            )
            .unwrap();
        connection.execute("INSERT INTO workstreams(id,project_id,name,status,created_at,updated_at) VALUES('w','p','W','ACTIVE',1,1)", []).unwrap();
        connection.execute("INSERT INTO endpoints(id,workstream_id,provider,external_id,label,status,created_at) VALUES('c','w','CODEX','thread','Codex','ACTIVE',1)", []).unwrap();
        connection.execute("INSERT INTO endpoints(id,workstream_id,provider,external_id,label,status,created_at) VALUES('g','w','CHATGPT','chat','ChatGPT','ACTIVE',1)", []).unwrap();
        connection.execute("INSERT INTO handoffs(id,workstream_id,source_endpoint_id,destination_endpoint_id,direction,original_text,approved_text,status,payload_hash,created_at) VALUES('h','w','c','g','CODEX_TO_CHATGPT','original','approved','SENT','hash',1)", []).unwrap();
        connection.execute("INSERT INTO handoff_attachments(id,handoff_id,filename,original_path,size,sha256,integrity_status,created_at) VALUES('a','h','proof.txt','D:\\proof.txt',12,'review-hash','MISMATCH',1)", []).unwrap();
        drop(connection);

        let store = RouterStore::open_at(&path).unwrap();
        let attachment = store.handoff_by_id("h").unwrap().attachments.remove(0);
        assert_eq!(attachment.sha256.as_deref(), Some("review-hash"));
        assert_eq!(attachment.integrity_status.as_deref(), Some("MISMATCH"));
        assert_eq!(attachment.send_sha256, None);
        assert_eq!(attachment.send_verified_at, None);
        drop(store);
        let reopened = RouterStore::open_at(&path).unwrap();
        let attachment = reopened.handoff_by_id("h").unwrap().attachments.remove(0);
        assert_eq!(attachment.sha256.as_deref(), Some("review-hash"));
        assert_eq!(attachment.integrity_status.as_deref(), Some("MISMATCH"));
        assert_eq!(attachment.send_sha256, None);
        assert_eq!(attachment.send_verified_at, None);
    }

    #[test]
    fn send_evidence_is_atomic_and_preserves_review_metadata_after_failure() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let codex = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-send-evidence".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        let chatgpt = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation-send-evidence".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let handoff = store
            .create_ready_handoff(NewHandoff {
                workstream_id: workstream.id,
                source_endpoint_id: codex.id,
                destination_endpoint_id: chatgpt.id,
                direction: "CODEX_TO_CHATGPT".into(),
                source_response_identity: None,
                original_text: "original".into(),
                approved_text: "approved".into(),
                attachments: vec![
                    NewAttachment {
                        id: "a-verified".into(),
                        filename: "verified.txt".into(),
                        original_path: "D:\\verified.txt".into(),
                        size: Some(1),
                        sha256: Some("review-verified".into()),
                        integrity_status: Some("VERIFIED".into()),
                    },
                    NewAttachment {
                        id: "a-undelcared".into(),
                        filename: "undeclared.txt".into(),
                        original_path: "D:\\undeclared.txt".into(),
                        size: Some(1),
                        sha256: Some("review-undeclared".into()),
                        integrity_status: Some("NOT_PROVIDED".into()),
                    },
                    NewAttachment {
                        id: "a-mismatch".into(),
                        filename: "mismatch.txt".into(),
                        original_path: "D:\\mismatch.txt".into(),
                        size: Some(1),
                        sha256: Some("review-mismatch".into()),
                        integrity_status: Some("MISMATCH".into()),
                    },
                ],
            })
            .unwrap();
        let evidence = [
            AttachmentSendEvidence {
                attachment_id: "a-verified".into(),
                review_sha256: Some("review-verified".into()),
                integrity_status: Some("VERIFIED".into()),
                send_sha256: "review-verified".into(),
            },
            AttachmentSendEvidence {
                attachment_id: "a-undelcared".into(),
                review_sha256: Some("review-undeclared".into()),
                integrity_status: Some("NOT_PROVIDED".into()),
                send_sha256: "review-undeclared".into(),
            },
            AttachmentSendEvidence {
                attachment_id: "a-mismatch".into(),
                review_sha256: Some("review-mismatch".into()),
                integrity_status: Some("MISMATCH".into()),
                send_sha256: "review-mismatch".into(),
            },
        ];
        store
            .record_attachment_send_evidence(&handoff.id, &evidence)
            .unwrap();
        store
            .transition_handoff(&handoff.id, "APPROVED", None)
            .unwrap();
        store
            .transition_handoff(&handoff.id, "SENDING", None)
            .unwrap();
        store
            .transition_handoff(
                &handoff.id,
                "FAILED",
                Some(("PROVIDER_FAILURE".into(), "after verification".into())),
            )
            .unwrap();
        let persisted = store.handoff_by_id(&handoff.id).unwrap();
        assert_eq!(persisted.status, "FAILED");
        for attachment in &persisted.attachments {
            assert_eq!(attachment.send_sha256, attachment.sha256);
            assert!(attachment.send_verified_at.is_some());
        }
        assert_eq!(
            persisted
                .attachments
                .iter()
                .find(|attachment| attachment.id == "a-mismatch")
                .unwrap()
                .integrity_status
                .as_deref(),
            Some("MISMATCH")
        );

        let stale = [
            AttachmentSendEvidence {
                attachment_id: "a-verified".into(),
                review_sha256: Some("review-verified".into()),
                integrity_status: Some("VERIFIED".into()),
                send_sha256: "review-verified".into(),
            },
            AttachmentSendEvidence {
                attachment_id: "a-undelcared".into(),
                review_sha256: Some("WRONG".into()),
                integrity_status: Some("NOT_PROVIDED".into()),
                send_sha256: "review-undeclared".into(),
            },
            AttachmentSendEvidence {
                attachment_id: "a-mismatch".into(),
                review_sha256: Some("review-mismatch".into()),
                integrity_status: Some("MISMATCH".into()),
                send_sha256: "review-mismatch".into(),
            },
        ];
        assert!(store
            .record_attachment_send_evidence(&handoff.id, &stale)
            .is_err());

        let atomic_handoff = store
            .create_ready_handoff(NewHandoff {
                workstream_id: persisted.workstream_id.clone(),
                source_endpoint_id: persisted.endpoint_source().unwrap().id.clone(),
                destination_endpoint_id: persisted.destination_endpoint.id.clone(),
                direction: "CODEX_TO_CHATGPT".into(),
                source_response_identity: None,
                original_text: "second original".into(),
                approved_text: "second approved".into(),
                attachments: vec![
                    NewAttachment {
                        id: "atomic-a".into(),
                        filename: "a.txt".into(),
                        original_path: "D:\\a.txt".into(),
                        size: Some(1),
                        sha256: Some("review-a".into()),
                        integrity_status: Some("VERIFIED".into()),
                    },
                    NewAttachment {
                        id: "atomic-b".into(),
                        filename: "b.txt".into(),
                        original_path: "D:\\b.txt".into(),
                        size: Some(1),
                        sha256: Some("review-b".into()),
                        integrity_status: Some("NOT_PROVIDED".into()),
                    },
                ],
            })
            .unwrap();
        let partial = [
            AttachmentSendEvidence {
                attachment_id: "atomic-a".into(),
                review_sha256: Some("review-a".into()),
                integrity_status: Some("VERIFIED".into()),
                send_sha256: "review-a".into(),
            },
            AttachmentSendEvidence {
                attachment_id: "atomic-b".into(),
                review_sha256: Some("changed-after-review".into()),
                integrity_status: Some("NOT_PROVIDED".into()),
                send_sha256: "changed-after-review".into(),
            },
        ];
        assert!(store
            .record_attachment_send_evidence(&atomic_handoff.id, &partial)
            .is_err());
        let atomic = store.handoff_by_id(&atomic_handoff.id).unwrap();
        assert!(atomic
            .attachments
            .iter()
            .all(|attachment| attachment.send_sha256.is_none()
                && attachment.send_verified_at.is_none()));
        let verified = persisted
            .attachments
            .iter()
            .find(|attachment| attachment.id == "a-verified")
            .unwrap();
        assert_eq!(verified.filename, "verified.txt");
        assert_eq!(verified.original_path, "D:\\verified.txt");
        assert_eq!(verified.sha256.as_deref(), Some("review-verified"));
        assert_eq!(verified.integrity_status.as_deref(), Some("VERIFIED"));
    }

    #[test]
    fn provider_run_completion_review_and_attention_acknowledgement_are_durable() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let codex = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-runtime".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        let chatgpt = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation-runtime".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let run = store
            .create_provider_run(
                &workstream.id,
                &codex.id,
                "CODEX",
                None,
                Some("turn-runtime"),
                "RUNNING",
            )
            .unwrap();
        store
            .accept_completed_provider_result(
                &run.id,
                "turn-runtime",
                "runtime-result",
                "full runtime result".into(),
            )
            .unwrap();
        let projection = store.dashboard_projection().unwrap();
        assert!(projection
            .attention_items
            .iter()
            .any(|item| item.kind == "CODEX_RESULT_READY"));
        store.mark_provider_run_reviewed(&run.id).unwrap();
        assert!(!store
            .dashboard_projection()
            .unwrap()
            .attention_items
            .iter()
            .any(|item| item.kind == "CODEX_RESULT_READY"));
        let handoff = store
            .create_ready_handoff(NewHandoff {
                workstream_id: workstream.id.clone(),
                source_endpoint_id: codex.id.clone(),
                destination_endpoint_id: chatgpt.id.clone(),
                direction: "CODEX_TO_CHATGPT".into(),
                source_response_identity: None,
                original_text: "source".into(),
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
        assert!(store
            .dashboard_projection()
            .unwrap()
            .attention_items
            .iter()
            .any(|item| item.kind == "DELIVERY_UNCERTAIN"));
        store.acknowledge_handoff_attention(&handoff.id).unwrap();
        assert_eq!(store.handoff_by_id(&handoff.id).unwrap().status, "SENDING");
        assert!(!store
            .dashboard_projection()
            .unwrap()
            .attention_items
            .iter()
            .any(|item| item.kind == "DELIVERY_UNCERTAIN"));
        let failed = store
            .create_ready_handoff(NewHandoff {
                workstream_id: workstream.id,
                source_endpoint_id: chatgpt.id,
                destination_endpoint_id: codex.id,
                direction: "CHATGPT_TO_CODEX".into(),
                source_response_identity: None,
                original_text: "failed source".into(),
                approved_text: "failed approved".into(),
                attachments: vec![],
            })
            .unwrap();
        store
            .transition_handoff(&failed.id, "APPROVED", None)
            .unwrap();
        store
            .transition_handoff(&failed.id, "SENDING", None)
            .unwrap();
        store
            .transition_handoff(
                &failed.id,
                "FAILED",
                Some(("TEST".into(), "expected".into())),
            )
            .unwrap();
        assert!(store
            .dashboard_projection()
            .unwrap()
            .attention_items
            .iter()
            .any(|item| item.kind == "HANDOFF_FAILED"));
        store.acknowledge_handoff_attention(&failed.id).unwrap();
        assert_eq!(store.handoff_by_id(&failed.id).unwrap().status, "FAILED");
        assert!(!store
            .dashboard_projection()
            .unwrap()
            .attention_items
            .iter()
            .any(|item| item.kind == "HANDOFF_FAILED"));
    }

    #[test]
    fn provider_runs_are_thread_independent_and_unique_by_provider_external_id() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let codex_a = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-a".into(),
                "A".into(),
                false,
            )
            .unwrap();
        let project_b = store.create_project("Project B".into(), None).unwrap();
        let workstream_b = store
            .create_workstream(&project_b.id, "WS-B".into())
            .unwrap();
        let codex_b = store
            .bind_endpoint(
                &workstream_b.id,
                Provider::Codex,
                "thread-b".into(),
                "B".into(),
                false,
            )
            .unwrap();
        let run_a = store
            .create_provider_run(
                &workstream.id,
                &codex_a.id,
                "CODEX",
                None,
                Some("turn-a"),
                "RUNNING",
            )
            .unwrap();
        let run_b = store
            .create_provider_run(
                &workstream_b.id,
                &codex_b.id,
                "CODEX",
                None,
                Some("turn-b"),
                "RUNNING",
            )
            .unwrap();
        assert!(store
            .create_provider_run(
                &workstream.id,
                &codex_a.id,
                "CODEX",
                None,
                Some("turn-a-second"),
                "RUNNING"
            )
            .is_err());
        assert!(store
            .create_provider_run(
                &workstream.id,
                &codex_a.id,
                "CODEX",
                None,
                Some("turn-a"),
                "RUNNING"
            )
            .is_err());
        store
            .complete_provider_run("CODEX", "turn-a", "COMPLETED", Some("completed"))
            .unwrap();
        let candidates = store.reconciliation_candidates().unwrap();
        assert!(candidates
            .iter()
            .any(|candidate| candidate.run.id == run_b.id));
        assert!(!candidates
            .iter()
            .any(|candidate| candidate.run.id == run_a.id));
        assert_eq!(
            store
                .dashboard_projection()
                .unwrap()
                .workstreams
                .iter()
                .find(|item| item.workstream.id == workstream_b.id)
                .unwrap()
                .codex_run
                .as_ref()
                .unwrap()
                .status,
            "RUNNING"
        );
    }

    #[test]
    fn projection_has_specific_missing_bindings_and_priority_order() {
        let (_dir, store) = store();
        let (project, only_codex) = project_workstream(&store);
        store
            .bind_endpoint(
                &only_codex.id,
                Provider::Codex,
                "thread-missing-chat".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        let only_chat = store
            .create_workstream(&project.id, "Missing Codex".into())
            .unwrap();
        store
            .bind_endpoint(
                &only_chat.id,
                Provider::Chatgpt,
                "chat-missing-codex".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let projection = store.dashboard_projection().unwrap();
        assert!(projection
            .attention_items
            .iter()
            .any(|item| item.kind == "MISSING_CHATGPT_BINDING"));
        assert!(projection
            .attention_items
            .iter()
            .any(|item| item.kind == "MISSING_CODEX_BINDING"));
        assert!(projection
            .attention_items
            .windows(2)
            .all(|pair| pair[0].priority <= pair[1].priority));
    }

    #[test]
    fn bridge_correlation_persists_across_reopen_and_is_unique() {
        let (dir, store) = store();
        let first = sending_codex_to_chatgpt(&store);
        store
            .attach_bridge_correlation(&first.id, "request-1", Some("assistant-turn-1"))
            .unwrap();
        let persisted = store.handoff_by_id(&first.id).unwrap();
        assert_eq!(persisted.bridge_request_id.as_deref(), Some("request-1"));
        assert_eq!(
            persisted.bridge_turn_key.as_deref(),
            Some("assistant-turn-1")
        );
        assert!(persisted.bridge_correlation_observed_at.is_some());
        drop(store);
        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        assert_eq!(
            reopened
                .handoff_by_id(&first.id)
                .unwrap()
                .bridge_request_id
                .as_deref(),
            Some("request-1")
        );

        let project = reopened.create_project("Second".into(), None).unwrap();
        let workstream = reopened
            .create_workstream(&project.id, "WS-002".into())
            .unwrap();
        let codex = reopened
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-2".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        let chatgpt = reopened
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "chat-2".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let duplicate = reopened
            .create_ready_handoff(NewHandoff {
                workstream_id: workstream.id,
                source_endpoint_id: codex.id,
                destination_endpoint_id: chatgpt.id,
                direction: "CODEX_TO_CHATGPT".into(),
                source_response_identity: None,
                original_text: "original".into(),
                approved_text: "approved".into(),
                attachments: vec![],
            })
            .unwrap();
        reopened
            .transition_handoff(&duplicate.id, "APPROVED", None)
            .unwrap();
        reopened
            .transition_handoff(&duplicate.id, "SENDING", None)
            .unwrap();
        assert!(reopened
            .attach_bridge_correlation(&duplicate.id, "request-1", None)
            .is_err());
    }

    #[test]
    fn bridge_correlation_cannot_rewrite_legacy_or_terminal_history() {
        let (_dir, store) = store();
        let handoff = sending_codex_to_chatgpt(&store);
        store
            .attach_bridge_correlation(&handoff.id, "request-1", None)
            .unwrap();
        assert!(store
            .attach_bridge_correlation(&handoff.id, "request-2", None)
            .is_err());
        store.transition_handoff(&handoff.id, "SENT", None).unwrap();
        assert!(store
            .attach_bridge_correlation(&handoff.id, "request-1", None)
            .is_err());
    }

    #[test]
    fn every_unreviewed_terminal_provider_run_is_actionable_and_review_preserves_status() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation-terminal".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let completed_a = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("completed-a"),
                "RUNNING",
            )
            .unwrap();
        store
            .accept_completed_provider_result(
                &completed_a.id,
                "completed-a",
                "completed-a-result",
                "full completed a result".into(),
            )
            .unwrap();
        let completed_b = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("completed-b"),
                "RUNNING",
            )
            .unwrap();
        store
            .accept_completed_provider_result(
                &completed_b.id,
                "completed-b",
                "completed-b-result",
                "full completed b result".into(),
            )
            .unwrap();
        let failed = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("failed"),
                "RUNNING",
            )
            .unwrap();
        store
            .complete_provider_run("CHATGPT", "failed", "FAILED", Some("adapter_error"))
            .unwrap();
        let cancelled = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("cancelled"),
                "RUNNING",
            )
            .unwrap();
        store
            .complete_provider_run("CHATGPT", "cancelled", "CANCELLED", Some("cancelled"))
            .unwrap();

        let before = store.dashboard_projection().unwrap();
        assert_eq!(
            before
                .attention_items
                .iter()
                .filter(|item| item.kind == "CHATGPT_RESULT_READY")
                .count(),
            1
        );
        assert!(before
            .attention_items
            .iter()
            .any(|item| item.kind == "PROVIDER_RUN_FAILED" && item.source_id == failed.id));
        assert!(before.attention_items.iter().any(|item| {
            item.kind == "PROVIDER_RUN_CANCELLED" && item.source_id == cancelled.id
        }));

        for (run, expected_status) in [
            (&completed_a, "COMPLETED"),
            (&completed_b, "COMPLETED"),
            (&failed, "FAILED"),
            (&cancelled, "CANCELLED"),
        ] {
            store.mark_provider_run_reviewed(&run.id).unwrap();
            let persisted = store
                .with_connection(|connection| provider_run_by_id(connection, &run.id))
                .unwrap();
            assert_eq!(persisted.status, expected_status);
            assert!(persisted.reviewed_at.is_some());
        }
        let after = store.dashboard_projection().unwrap();
        assert!(!after.attention_items.iter().any(|item| {
            matches!(
                item.kind.as_str(),
                "CHATGPT_RESULT_READY" | "PROVIDER_RUN_FAILED" | "PROVIDER_RUN_CANCELLED"
            )
        }));
    }

    #[test]
    fn exact_bridge_confirmation_updates_handoff_and_matching_run_atomically() {
        let (_dir, store) = store();
        let handoff = sending_codex_to_chatgpt(&store);
        store
            .attach_bridge_correlation(&handoff.id, "request-exact", None)
            .unwrap();
        let run = store
            .create_provider_run(
                &handoff.workstream_id,
                &handoff.destination_endpoint.id,
                "CHATGPT",
                Some(&handoff.id),
                Some("request-exact"),
                "RUNNING",
            )
            .unwrap();
        store
            .confirm_handoff_and_provider_run(
                &handoff.id,
                "request-exact",
                "SENT",
                "COMPLETED",
                None,
                Some("completed"),
            )
            .unwrap();
        assert_eq!(store.handoff_by_id(&handoff.id).unwrap().status, "SENT");
        let persisted = store
            .with_connection(|connection| provider_run_by_id(connection, &run.id))
            .unwrap();
        assert_eq!(persisted.status, "COMPLETED");
        assert!(persisted.terminal_at.is_some());

        let mismatch = store
            .create_ready_handoff(NewHandoff {
                workstream_id: handoff.workstream_id.clone(),
                source_endpoint_id: handoff.endpoint_source().unwrap().id.clone(),
                destination_endpoint_id: handoff.destination_endpoint.id.clone(),
                direction: "CODEX_TO_CHATGPT".into(),
                source_response_identity: None,
                original_text: "mismatch source".into(),
                approved_text: "mismatch approved".into(),
                attachments: vec![],
            })
            .unwrap();
        store
            .transition_handoff(&mismatch.id, "APPROVED", None)
            .unwrap();
        store
            .transition_handoff(&mismatch.id, "SENDING", None)
            .unwrap();
        store
            .attach_bridge_correlation(&mismatch.id, "request-expected", None)
            .unwrap();
        store
            .create_provider_run(
                &mismatch.workstream_id,
                &mismatch.destination_endpoint.id,
                "CHATGPT",
                Some(&mismatch.id),
                Some("request-different"),
                "RUNNING",
            )
            .unwrap();
        assert!(store
            .confirm_handoff_and_provider_run(
                &mismatch.id,
                "request-expected",
                "FAILED",
                "FAILED",
                Some(("TEST", "must roll back")),
                Some("failed"),
            )
            .is_err());
        assert_eq!(
            store.handoff_by_id(&mismatch.id).unwrap().status,
            "SENDING",
            "identity mismatch must leave the Handoff unchanged"
        );
    }

    #[test]
    fn exact_bridge_confirmation_keeps_legacy_handoff_recoverable_without_provider_run() {
        let (_dir, store) = store();
        let handoff = sending_codex_to_chatgpt(&store);
        store
            .attach_bridge_correlation(&handoff.id, "request-legacy", None)
            .unwrap();
        store
            .confirm_handoff_and_provider_run(
                &handoff.id,
                "request-legacy",
                "FAILED",
                "CANCELLED",
                Some(("CHATGPT_EXACT_CANCELLED", "cancelled")),
                Some("cancelled"),
            )
            .unwrap();
        let persisted = store.handoff_by_id(&handoff.id).unwrap();
        assert_eq!(persisted.status, "FAILED");
        assert!(persisted.failed_at.is_some());
    }

    #[test]
    fn unknown_runs_have_no_terminal_timestamp_and_handoff_lifecycle_affects_activity() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let codex = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread-activity".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        let chatgpt = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation-activity".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let run = store
            .create_provider_run(
                &workstream.id,
                &chatgpt.id,
                "CHATGPT",
                None,
                Some("unknown"),
                "RUNNING",
            )
            .unwrap();
        store
            .reconcile_provider_run(&run.id, "UNKNOWN", "EXACT_EVIDENCE_UNAVAILABLE")
            .unwrap();
        let unknown = store
            .with_connection(|connection| provider_run_by_id(connection, &run.id))
            .unwrap();
        assert_eq!(unknown.status, "UNKNOWN");
        assert_eq!(unknown.terminal_at, None);

        let handoff = store
            .create_ready_handoff(NewHandoff {
                workstream_id: workstream.id.clone(),
                source_endpoint_id: codex.id,
                destination_endpoint_id: chatgpt.id,
                direction: "CODEX_TO_CHATGPT".into(),
                source_response_identity: None,
                original_text: "source".into(),
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
        store
            .transition_handoff(
                &handoff.id,
                "FAILED",
                Some(("TEST".into(), "failed".into())),
            )
            .unwrap();
        let failed_at = store.handoff_by_id(&handoff.id).unwrap().failed_at.unwrap();
        let card = store
            .dashboard_projection()
            .unwrap()
            .workstreams
            .into_iter()
            .find(|card| card.workstream.id == workstream.id)
            .unwrap();
        assert!(card.last_activity_at >= failed_at);
    }

    #[test]
    fn exact_bridge_confirmation_accepts_matching_reconciled_terminal_state_only() {
        for (run_status, handoff_status, error, code) in [
            ("COMPLETED", "SENT", None, "completed"),
            (
                "FAILED",
                "FAILED",
                Some(("CHATGPT_BRIDGE_CORRELATED_TERMINAL_ERROR", "failed")),
                "failed",
            ),
            (
                "CANCELLED",
                "FAILED",
                Some(("CHATGPT_BRIDGE_CORRELATED_TERMINAL_ERROR", "cancelled")),
                "cancelled",
            ),
        ] {
            let (_dir, store) = store();
            let handoff = sending_codex_to_chatgpt(&store);
            store
                .attach_bridge_correlation(&handoff.id, "request-reconciled", None)
                .unwrap();
            let run = store
                .create_provider_run(
                    &handoff.workstream_id,
                    &handoff.destination_endpoint.id,
                    "CHATGPT",
                    Some(&handoff.id),
                    Some("request-reconciled"),
                    "RUNNING",
                )
                .unwrap();
            store
                .complete_provider_run("CHATGPT", "request-reconciled", run_status, Some(code))
                .unwrap();
            store
                .confirm_handoff_and_provider_run(
                    &handoff.id,
                    "request-reconciled",
                    handoff_status,
                    run_status,
                    error,
                    Some(code),
                )
                .unwrap();
            assert_eq!(
                store.handoff_by_id(&handoff.id).unwrap().status,
                handoff_status
            );
            let persisted = store
                .with_connection(|connection| provider_run_by_id(connection, &run.id))
                .unwrap();
            assert_eq!(persisted.status, run_status);
            assert_eq!(persisted.terminal_code.as_deref(), Some(code));
        }

        let (_dir, store) = store();
        let handoff = sending_codex_to_chatgpt(&store);
        store
            .attach_bridge_correlation(&handoff.id, "request-conflict", None)
            .unwrap();
        let run = store
            .create_provider_run(
                &handoff.workstream_id,
                &handoff.destination_endpoint.id,
                "CHATGPT",
                Some(&handoff.id),
                Some("request-conflict"),
                "RUNNING",
            )
            .unwrap();
        store
            .complete_provider_run("CHATGPT", "request-conflict", "FAILED", Some("failed"))
            .unwrap();
        assert!(store
            .confirm_handoff_and_provider_run(
                &handoff.id,
                "request-conflict",
                "SENT",
                "COMPLETED",
                None,
                Some("completed"),
            )
            .is_err());
        assert_eq!(store.handoff_by_id(&handoff.id).unwrap().status, "SENDING");
        assert_eq!(
            store
                .with_connection(|connection| provider_run_by_id(connection, &run.id))
                .unwrap()
                .status,
            "FAILED"
        );
    }

    #[test]
    fn bridge_correlation_and_chatgpt_run_identity_commit_atomically() {
        let (_dir, store) = store();
        let handoff = sending_codex_to_chatgpt(&store);
        let run = store
            .create_provider_run(
                &handoff.workstream_id,
                &handoff.destination_endpoint.id,
                "CHATGPT",
                Some(&handoff.id),
                None,
                "STARTING",
            )
            .unwrap();
        store
            .attach_bridge_correlation_and_provider_run(
                &handoff.id,
                &run.id,
                "request-atomic",
                Some("turn-atomic"),
            )
            .unwrap();
        store
            .attach_bridge_correlation_and_provider_run(
                &handoff.id,
                &run.id,
                "request-atomic",
                Some("turn-atomic"),
            )
            .unwrap();
        let persisted_handoff = store.handoff_by_id(&handoff.id).unwrap();
        let persisted_run = store
            .with_connection(|connection| provider_run_by_id(connection, &run.id))
            .unwrap();
        assert_eq!(
            persisted_handoff.bridge_request_id.as_deref(),
            Some("request-atomic")
        );
        assert_eq!(
            persisted_handoff.bridge_turn_key.as_deref(),
            Some("turn-atomic")
        );
        assert_eq!(
            persisted_run.external_run_id.as_deref(),
            Some("request-atomic")
        );
        assert_eq!(persisted_run.status, "RUNNING");
        assert!(store
            .attach_bridge_correlation_and_provider_run(
                &handoff.id,
                &run.id,
                "request-different",
                None,
            )
            .is_err());
        assert_eq!(
            store
                .handoff_by_id(&handoff.id)
                .unwrap()
                .bridge_request_id
                .as_deref(),
            Some("request-atomic")
        );

        let wrong_origin = store
            .create_provider_run(
                &handoff.workstream_id,
                &handoff.destination_endpoint.id,
                "CHATGPT",
                None,
                None,
                "STARTING",
            )
            .unwrap();
        let second = store
            .create_ready_handoff(NewHandoff {
                workstream_id: handoff.workstream_id.clone(),
                source_endpoint_id: handoff.endpoint_source().unwrap().id.clone(),
                destination_endpoint_id: handoff.destination_endpoint.id.clone(),
                direction: "CODEX_TO_CHATGPT".into(),
                source_response_identity: None,
                original_text: "second source".into(),
                approved_text: "second approved".into(),
                attachments: vec![],
            })
            .unwrap();
        store
            .transition_handoff(&second.id, "APPROVED", None)
            .unwrap();
        store
            .transition_handoff(&second.id, "SENDING", None)
            .unwrap();
        assert!(store
            .attach_bridge_correlation_and_provider_run(
                &second.id,
                &wrong_origin.id,
                "request-wrong-origin",
                None,
            )
            .is_err());
        assert_eq!(
            store.handoff_by_id(&second.id).unwrap().bridge_request_id,
            None
        );
        assert_eq!(
            store
                .with_connection(|connection| provider_run_by_id(connection, &wrong_origin.id))
                .unwrap()
                .external_run_id,
            None
        );
    }

    #[test]
    fn bridge_correlation_rolls_back_handoff_when_provider_identity_is_not_unique() {
        let (_dir, store) = store();
        let handoff = sending_codex_to_chatgpt(&store);
        let target = store
            .create_provider_run(
                &handoff.workstream_id,
                &handoff.destination_endpoint.id,
                "CHATGPT",
                Some(&handoff.id),
                None,
                "STARTING",
            )
            .unwrap();
        let existing_handoff = store
            .create_ready_handoff(NewHandoff {
                workstream_id: handoff.workstream_id.clone(),
                source_endpoint_id: handoff.endpoint_source().unwrap().id.clone(),
                destination_endpoint_id: handoff.destination_endpoint.id.clone(),
                direction: "CODEX_TO_CHATGPT".into(),
                source_response_identity: None,
                original_text: "existing source".into(),
                approved_text: "existing approved".into(),
                attachments: vec![],
            })
            .unwrap();
        store
            .transition_handoff(&existing_handoff.id, "APPROVED", None)
            .unwrap();
        store
            .transition_handoff(&existing_handoff.id, "SENDING", None)
            .unwrap();
        store
            .create_provider_run(
                &existing_handoff.workstream_id,
                &existing_handoff.destination_endpoint.id,
                "CHATGPT",
                Some(&existing_handoff.id),
                Some("request-taken"),
                "RUNNING",
            )
            .unwrap();
        assert!(store
            .attach_bridge_correlation_and_provider_run(
                &handoff.id,
                &target.id,
                "request-taken",
                None,
            )
            .is_err());
        assert_eq!(
            store.handoff_by_id(&handoff.id).unwrap().bridge_request_id,
            None
        );
        let target_after = store
            .with_connection(|connection| provider_run_by_id(connection, &target.id))
            .unwrap();
        assert_eq!(target_after.external_run_id, None);
        assert_eq!(target_after.status, "STARTING");
    }

    #[test]
    fn latest_execution_is_not_replaced_by_review_activity() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation-recency".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let old = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("old-completed"),
                "RUNNING",
            )
            .unwrap();
        store
            .complete_provider_run("CHATGPT", "old-completed", "COMPLETED", Some("completed"))
            .unwrap();
        let newer = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("new-running"),
                "RUNNING",
            )
            .unwrap();
        store.mark_provider_run_reviewed(&old.id).unwrap();
        let card = store
            .dashboard_projection()
            .unwrap()
            .workstreams
            .into_iter()
            .find(|card| card.workstream.id == workstream.id)
            .unwrap();
        assert_eq!(card.chatgpt_run.as_ref().unwrap().id, newer.id);
        assert_eq!(card.chatgpt_run.as_ref().unwrap().status, "RUNNING");
    }

    #[test]
    fn older_review_activity_advances_last_activity_without_replacing_current_execution() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation-aggregate-activity".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let older = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("older-completed"),
                "RUNNING",
            )
            .unwrap();
        store
            .complete_provider_run("CHATGPT", "older-completed", "COMPLETED", Some("completed"))
            .unwrap();
        let newer = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("newer-running"),
                "RUNNING",
            )
            .unwrap();
        store
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE provider_runs SET created_at=100,started_at=100,updated_at=100 WHERE id=?1",
                        params![older.id],
                    )
                    .map_err(db_error)?;
                connection
                    .execute(
                        "UPDATE provider_runs SET created_at=200,started_at=200,updated_at=200 WHERE id=?1",
                        params![newer.id],
                    )
                    .map_err(db_error)?;
                connection
                    .execute(
                        "UPDATE workstreams SET updated_at=50 WHERE id=?1",
                        params![workstream.id],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        store.mark_provider_run_reviewed(&older.id).unwrap();
        let older_after_review = store
            .with_connection(|connection| provider_run_by_id(connection, &older.id))
            .unwrap();
        assert!(older_after_review.updated_at > 200);
        let card = store
            .dashboard_projection()
            .unwrap()
            .workstreams
            .into_iter()
            .find(|card| card.workstream.id == workstream.id)
            .unwrap();
        assert_eq!(card.chatgpt_run.as_ref().unwrap().id, newer.id);
        assert_eq!(card.chatgpt_run.as_ref().unwrap().status, "RUNNING");
        assert_eq!(card.last_activity_at, older_after_review.updated_at);
    }

    #[test]
    fn completed_result_is_bounded_atomic_and_survives_store_reopen() {
        let (dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation-bounded-result".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let old = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("old-request"),
                "RUNNING",
            )
            .unwrap();
        let accepted_old = store
            .accept_completed_provider_result(
                &old.id,
                "old-request",
                "old-result",
                "full old result".into(),
            )
            .unwrap();
        assert_eq!(accepted_old.status, "COMPLETED");
        assert_eq!(accepted_old.result_identity.as_deref(), Some("old-result"));
        assert_eq!(accepted_old.result_text.as_deref(), Some("full old result"));
        assert_eq!(
            store
                .latest_reviewable_provider_result(&workstream.id, "CHATGPT")
                .unwrap()
                .unwrap()
                .id,
            old.id
        );

        let starting = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("starting-request"),
                "STARTING",
            )
            .unwrap();
        let running = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("running-request"),
                "RUNNING",
            )
            .unwrap();
        let unknown = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("unknown-request"),
                "RUNNING",
            )
            .unwrap();
        store
            .reconcile_provider_run(&unknown.id, "UNKNOWN", "unproven")
            .unwrap();
        let failed = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("failed-request"),
                "RUNNING",
            )
            .unwrap();
        store
            .complete_provider_run("CHATGPT", "failed-request", "FAILED", Some("failed"))
            .unwrap();
        let cancelled = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("cancelled-request"),
                "RUNNING",
            )
            .unwrap();
        store
            .complete_provider_run(
                "CHATGPT",
                "cancelled-request",
                "CANCELLED",
                Some("cancelled"),
            )
            .unwrap();
        let retained = store
            .provider_runs_for_workstream(&workstream.id)
            .unwrap()
            .into_iter()
            .filter(|run| run.result_text.is_some())
            .collect::<Vec<_>>();
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].id, old.id);
        assert!(starting.result_text.is_none());
        assert!(running.result_text.is_none());
        let all_runs = store.provider_runs_for_workstream(&workstream.id).unwrap();
        assert_eq!(
            all_runs
                .iter()
                .find(|run| run.id == failed.id)
                .unwrap()
                .status,
            "FAILED"
        );
        assert_eq!(
            all_runs
                .iter()
                .find(|run| run.id == cancelled.id)
                .unwrap()
                .status,
            "CANCELLED"
        );

        let replacement = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("replacement-request"),
                "RUNNING",
            )
            .unwrap();
        assert!(store
            .accept_completed_provider_result(
                &replacement.id,
                "different-request",
                "replacement-result",
                "full replacement result".into(),
            )
            .is_err());
        assert!(store
            .accept_completed_provider_result(
                &replacement.id,
                "replacement-request",
                "replacement-result",
                "   ".into(),
            )
            .is_err());
        assert_eq!(
            store
                .provider_runs_for_workstream(&workstream.id)
                .unwrap()
                .into_iter()
                .find(|run| run.id == old.id)
                .and_then(|run| run.result_text),
            Some("full old result".into())
        );

        let accepted_replacement = store
            .accept_completed_provider_result(
                &replacement.id,
                "replacement-request",
                "replacement-result",
                "full replacement result".into(),
            )
            .unwrap();
        assert_eq!(
            accepted_replacement.result_text.as_deref(),
            Some("full replacement result")
        );
        let rows = store.provider_runs_for_workstream(&workstream.id).unwrap();
        let retained = rows
            .iter()
            .filter(|run| run.result_text.is_some())
            .collect::<Vec<_>>();
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].id, replacement.id);
        assert_eq!(
            store
                .latest_reviewable_provider_result(&workstream.id, "CHATGPT")
                .unwrap()
                .unwrap()
                .id,
            replacement.id
        );
        assert_eq!(
            rows.iter()
                .find(|run| run.id == old.id)
                .unwrap()
                .result_text,
            None
        );
        assert!(store
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE provider_runs SET result_text='must conflict' WHERE id=?1",
                        params![old.id],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .is_err());

        drop(store);
        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        let retained = reopened
            .provider_runs_for_workstream(&workstream.id)
            .unwrap()
            .into_iter()
            .filter(|run| run.result_text.is_some())
            .collect::<Vec<_>>();
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].id, replacement.id);
        assert_eq!(
            retained[0].result_identity.as_deref(),
            Some("replacement-result")
        );
        assert_eq!(
            retained[0].result_text.as_deref(),
            Some("full replacement result")
        );
    }

    #[test]
    fn external_identity_attachment_is_exact_and_keeps_run_running() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation-direct-request".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let run = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                None,
                "STARTING",
            )
            .unwrap();
        store
            .attach_provider_run_external_identity(&run.id, "CHATGPT", "exact-request")
            .unwrap();
        let attached = store
            .provider_runs_for_workstream(&workstream.id)
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == run.id)
            .unwrap();
        assert_eq!(attached.status, "RUNNING");
        assert_eq!(attached.external_run_id.as_deref(), Some("exact-request"));
        assert!(store
            .attach_provider_run_external_identity(&run.id, "CHATGPT", "exact-request")
            .is_ok());
        assert!(store
            .attach_provider_run_external_identity(&run.id, "CHATGPT", "different-request")
            .is_err());
        assert!(store
            .attach_provider_run_external_identity(&run.id, "CODEX", "exact-request")
            .is_err());
    }

    #[test]
    fn exact_chatgpt_correlation_can_recover_only_the_bounded_acceptance_timeout() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation-late-correlation".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let run = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                None,
                "STARTING",
            )
            .unwrap();
        store
            .reconcile_provider_run(&run.id, "UNKNOWN", "CHATGPT_ACCEPTANCE_UNPROVEN")
            .unwrap();

        store
            .attach_provider_run_external_identity(&run.id, "CHATGPT", "late-exact-request")
            .unwrap();
        let recovered = store
            .provider_runs_for_workstream(&workstream.id)
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == run.id)
            .unwrap();
        assert_eq!(recovered.status, "RUNNING");
        assert_eq!(recovered.terminal_code, None);
        assert_eq!(
            recovered.external_run_id.as_deref(),
            Some("late-exact-request")
        );

        store
            .reconcile_provider_run(&run.id, "UNKNOWN", "EXACT_EVIDENCE_UNAVAILABLE")
            .unwrap();
        assert!(store
            .attach_provider_run_external_identity(&run.id, "CHATGPT", "other-request")
            .is_err());
    }

    #[test]
    fn exact_history_recovery_can_correct_only_the_designated_empty_chatgpt_states() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation-history-recovery".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let run = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                None,
                "STARTING",
            )
            .unwrap();
        store
            .attach_provider_run_external_identity(&run.id, "CHATGPT", "exact-request")
            .unwrap();
        store
            .fail_provider_run_by_id(&run.id, "CHATGPT_BRIDGE_TERMINAL_ERROR")
            .unwrap();

        let recovered = store
            .recover_chatgpt_result_from_exact_history(
                &run.id,
                "exact-request",
                "exact-request",
                "Proven complete response".into(),
            )
            .unwrap();
        assert_eq!(recovered.status, "COMPLETED");
        assert_eq!(
            recovered.terminal_code.as_deref(),
            Some("COMPLETED_AFTER_EXACT_HISTORY_RECOVERY")
        );
        assert_eq!(recovered.result_identity.as_deref(), Some("exact-request"));
        assert_eq!(
            recovered.result_text.as_deref(),
            Some("Proven complete response")
        );
        assert!(store
            .recover_chatgpt_result_from_exact_history(
                &run.id,
                "exact-request",
                "exact-request",
                "Different response".into(),
            )
            .is_err());

        let unreadable = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                None,
                "STARTING",
            )
            .unwrap();
        store
            .attach_provider_run_external_identity(&unreadable.id, "CHATGPT", "accepted-request")
            .unwrap();
        store
            .reconcile_provider_run(
                &unreadable.id,
                "UNKNOWN",
                "CHATGPT_RESULT_UNREADABLE_AFTER_ACCEPTANCE",
            )
            .unwrap();
        let recovered = store
            .recover_chatgpt_result_from_exact_history(
                &unreadable.id,
                "accepted-request",
                "accepted-request",
                "Recovered accepted result".into(),
            )
            .unwrap();
        assert_eq!(recovered.status, "COMPLETED");
    }

    #[test]
    fn late_exact_bridge_correlation_recovers_only_the_named_acceptance_timeout() {
        let (_dir, store) = store();
        let handoff = sending_codex_to_chatgpt(&store);
        let run = store
            .create_provider_run(
                &handoff.workstream_id,
                &handoff.destination_endpoint.id,
                "CHATGPT",
                Some(&handoff.id),
                None,
                "STARTING",
            )
            .unwrap();
        assert!(store
            .mark_chatgpt_acceptance_unproven_if_unattached(&run.id)
            .unwrap());

        store
            .attach_bridge_correlation_and_provider_run(
                &handoff.id,
                &run.id,
                "late-exact-request",
                Some("late-exact-turn"),
            )
            .unwrap();

        let recovered = store
            .provider_runs_for_workstream(&handoff.workstream_id)
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == run.id)
            .unwrap();
        assert_eq!(recovered.status, "RUNNING");
        assert_eq!(
            recovered.external_run_id.as_deref(),
            Some("late-exact-request")
        );
        assert_eq!(recovered.terminal_code, None);

        let unproven = store
            .create_provider_run(
                &handoff.workstream_id,
                &handoff.destination_endpoint.id,
                "CHATGPT",
                None,
                None,
                "STARTING",
            )
            .unwrap();
        store
            .reconcile_provider_run(&unproven.id, "UNKNOWN", "MISSING_REQUEST_ID")
            .unwrap();
        assert!(store
            .attach_bridge_correlation_and_provider_run(
                &handoff.id,
                &unproven.id,
                "must-not-attach",
                None,
            )
            .is_err());
    }

    #[test]
    fn reverse_acceptance_deadline_keeps_sending_explained_and_late_sent_clears_it() {
        let (_dir, store) = store();
        let handoff = sending_codex_to_chatgpt(&store);
        let run = store
            .create_provider_run(
                &handoff.workstream_id,
                &handoff.destination_endpoint.id,
                "CHATGPT",
                Some(&handoff.id),
                None,
                "STARTING",
            )
            .unwrap();

        assert!(store
            .mark_chatgpt_handoff_acceptance_unproven_if_unattached(&handoff.id, &run.id)
            .unwrap());
        let waiting_handoff = store.handoff_by_id(&handoff.id).unwrap();
        assert_eq!(waiting_handoff.status, "SENDING");
        assert_eq!(
            waiting_handoff.error_code.as_deref(),
            Some("CHATGPT_ACCEPTANCE_UNPROVEN")
        );
        assert_eq!(
            waiting_handoff.error_message.as_deref(),
            Some("Waiting for exact ChatGPT acceptance proof")
        );
        let waiting_run = store
            .provider_runs_for_workstream(&handoff.workstream_id)
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == run.id)
            .unwrap();
        assert_eq!(waiting_run.status, "UNKNOWN");
        assert_eq!(
            waiting_run.terminal_code.as_deref(),
            Some("CHATGPT_ACCEPTANCE_UNPROVEN")
        );

        store
            .attach_bridge_correlation_and_provider_run(
                &handoff.id,
                &run.id,
                "late-exact-request",
                None,
            )
            .unwrap();
        store.transition_handoff(&handoff.id, "SENT", None).unwrap();
        let sent_handoff = store.handoff_by_id(&handoff.id).unwrap();
        assert_eq!(sent_handoff.status, "SENT");
        assert_eq!(sent_handoff.error_code, None);
        assert_eq!(sent_handoff.error_message, None);
        assert!(!store
            .mark_chatgpt_handoff_acceptance_unproven_if_unattached(&handoff.id, &run.id)
            .unwrap());
    }

    #[test]
    fn chatgpt_acceptance_deadline_never_overwrites_an_exact_correlation() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation-acceptance-deadline".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let run = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                None,
                "STARTING",
            )
            .unwrap();
        assert!(store
            .mark_chatgpt_acceptance_unproven_if_unattached(&run.id)
            .unwrap());
        store
            .attach_provider_run_external_identity(&run.id, "CHATGPT", "late-request")
            .unwrap();
        assert!(!store
            .mark_chatgpt_acceptance_unproven_if_unattached(&run.id)
            .unwrap());
        let recovered = store
            .provider_runs_for_workstream(&workstream.id)
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == run.id)
            .unwrap();
        assert_eq!(recovered.status, "RUNNING");
        assert_eq!(recovered.external_run_id.as_deref(), Some("late-request"));
    }

    #[test]
    fn feedback_recovery_is_scoped_to_one_durable_exact_source_result() {
        let (_dir, store) = store();
        let (_project, workstream) = project_workstream(&store);
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation-feedback-source".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let source = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("source-request"),
                "RUNNING",
            )
            .unwrap();
        store
            .accept_completed_provider_result(
                &source.id,
                "source-request",
                "source-response",
                "Retained exact source result".into(),
            )
            .unwrap();
        let feedback = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("feedback-request"),
                "RUNNING",
            )
            .unwrap();
        store
            .reconcile_provider_run(
                &feedback.id,
                "UNKNOWN",
                "CHATGPT_RESULT_UNREADABLE_AFTER_ACCEPTANCE",
            )
            .unwrap();
        store
            .record_chatgpt_feedback_source(&feedback.id, &source.id)
            .unwrap();

        assert!(store
            .feedback_run_is_sourced_from(&feedback.id, &source.id)
            .unwrap());
        assert_eq!(
            store
                .recoverable_chatgpt_feedback_run_for_source(&source.id, &endpoint.id)
                .unwrap()
                .as_deref(),
            Some(feedback.id.as_str())
        );

        let second_feedback = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("second-feedback-request"),
                "RUNNING",
            )
            .unwrap();
        store
            .reconcile_provider_run(
                &second_feedback.id,
                "UNKNOWN",
                "CHATGPT_RESULT_UNREADABLE_AFTER_ACCEPTANCE",
            )
            .unwrap();
        store
            .record_chatgpt_feedback_source(&second_feedback.id, &source.id)
            .unwrap();
        assert_eq!(
            store
                .recoverable_chatgpt_feedback_run_for_source(&source.id, &endpoint.id)
                .unwrap(),
            None
        );
    }
}

#[cfg(test)]
mod control_tests;
#[cfg(test)]
mod chatgpt_dispatch_tests;
#[cfg(test)]
mod rename_tests;

#[cfg(test)]
mod completion_tests;

#[cfg(test)]
#[path="instance_tests.rs"]
mod instance_tests;
