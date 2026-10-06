//! Durable, provider-neutral relay facts.  This module intentionally stores
//! only Router-created identifiers and relative artifact references: browser
//! DOM, HTTP clients, and local absolute paths remain adapter concerns.
use super::*;
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::{Digest, Sha256};

// Relay tables were introduced at 15, but the module must accept the current
// preview schema rather than treating a later, compatible migration as an
// unknown database.  The store-level maintenance gate remains authoritative.
const RELAY_SCHEMA: i64 = migration::PREVIEW_SCHEMA;
const RELAY_ENVELOPE_VERSION: &str = "relay-envelope-v1";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserEndpointSettings {
    pub endpoint_id: String,
    pub owner_principal_key: String,
    pub observe_enabled: bool,
    pub settings_revision: i64,
    pub paired_by: String,
    pub paired_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct BrowserEndpointSettingsInput {
    pub endpoint_id: String,
    pub owner_principal_key: String,
    pub observe_enabled: bool,
    pub paired_by: String,
}

#[derive(Clone, Debug)]
pub struct RelayArtifactInput {
    pub id: String,
    pub source_endpoint_id: String,
    pub source_run_id: Option<String>,
    pub source_message_ref: String,
    pub source_artifact_ref: String,
    pub kind: String,
    pub filename: String,
    pub mime: String,
    pub size: i64,
    pub source_declared_sha256: Option<String>,
    pub transfer_declared_sha256: String,
    pub actual_sha256: Option<String>,
    pub source_hash_status: String,
    pub integrity_status: String,
    /// Relative to the Router-owned artifact root; never a user path.
    pub storage_ref: String,
    pub state: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayArtifact {
    pub id: String,
    pub source_endpoint_id: String,
    pub source_run_id: Option<String>,
    pub source_message_ref: String,
    pub source_artifact_ref: String,
    pub kind: String,
    pub filename: String,
    pub mime: String,
    pub size: i64,
    pub source_declared_sha256: Option<String>,
    pub transfer_declared_sha256: String,
    pub actual_sha256: Option<String>,
    pub source_hash_status: String,
    pub integrity_status: String,
    pub storage_ref: String,
    pub state: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct RelayAttachmentSelection {
    pub attachment_id: String,
    pub artifact_id: String,
    pub mime: String,
    pub approved_sha256: String,
    pub approved_size: i64,
    pub manifest_entry_json: String,
    pub target_relative_path: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewRelayHandoff {
    pub workstream_id: String,
    pub source_endpoint_id: String,
    pub source_run_id: Option<String>,
    pub source_message_ref: String,
    pub capture_digest: String,
    pub source_completeness: String,
    pub destination_endpoint_id: String,
    pub direction: String,
    pub original_text: String,
    pub approved_text: String,
    pub owner_principal_key: String,
    pub binding_revision: i64,
    pub root_revision: i64,
    pub destination_policy_json: String,
    pub destination_policy_hash: String,
    pub draft_revision: i64,
    pub client_request_id: String,
    pub attachments: Vec<RelayAttachmentSelection>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayReview {
    pub handoff: HandoffHistoryItem,
    pub owner_principal_key: String,
    pub source_endpoint_id: String,
    pub destination_endpoint_id: String,
    pub source_message_ref: String,
    pub capture_digest: String,
    pub binding_revision: i64,
    pub root_revision: i64,
    pub destination_policy_json: String,
    pub destination_policy_hash: String,
    pub draft_revision: i64,
    pub manifest_hash: String,
    pub approval_state: String,
    pub attachments: Vec<RelayArtifact>,
    /// Server-owned destination paths, aligned with `attachments` by ordinal.
    pub target_relative_paths: Vec<String>,
}

/// The only Endpoint-source data a dispatcher may consume.  It is rebuilt
/// from the sealed row immediately before a physical write; client handles,
/// DOM references and local absolute paths intentionally never enter it.
#[derive(Clone, Debug)]
pub struct RelayDispatchSnapshot {
    pub owner_principal_key: String,
    pub handoff_id: String,
    pub run_id: String,
    pub dispatch_id: String,
    pub workstream_id: String,
    pub source_endpoint_id: String,
    pub destination_endpoint_id: String,
    pub destination_provider: String,
    pub destination_external_id: String,
    pub direction: String,
    pub approved_text: String,
    pub payload_hash: String,
    pub manifest_hash: String,
    pub binding_revision: i64,
    pub root_revision: i64,
    pub canonical_path: String,
    pub policy: super::control::PolicySnapshot,
    pub destination_policy_json: String,
    pub destination_policy_hash: String,
    pub attachments: Vec<(RelayArtifact, String)>,
}

/// Historical dispatch truth for an already-consumed relay approval.  This is
/// intentionally assembled from durable rows only: response-loss recovery
/// must not re-run current binding, endpoint, policy, or artifact validation.
#[derive(Clone, Debug, Serialize)]
pub struct RelayApprovalReadback {
    pub handoff_id: String,
    pub handoff_status: String,
    pub run_id: String,
    pub run_status: String,
    pub dispatch_id: String,
    pub dispatch_phase: String,
    pub last_code: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BridgeDispatchEvidence {
    pub id: String,
    pub dispatch_id: String,
    pub kind: String,
    pub endpoint_id: String,
    pub conversation_id: String,
    pub source_message_id: String,
    pub request_id: String,
    pub turn_key: Option<String>,
    pub adapter_epoch: String,
    pub bundle_identity: String,
    pub payload_hash: String,
    pub manifest_hash: String,
    pub evidence_digest: String,
    pub integrity_mac: String,
    pub recorded_at: i64,
}

fn bridge_evidence_bytes(value: &BridgeDispatchEvidence) -> Result<Vec<u8>, String> {
    let mut unsigned = value.clone();
    unsigned.evidence_digest.clear();
    unsigned.integrity_mac.clear();
    let bytes = serde_json::to_vec(&unsigned).map_err(|_| "INTERNAL")?;
    if bytes.len() > 4096 {
        return Err("EVIDENCE_LIMIT".into());
    }
    Ok(bytes)
}

fn seal_bridge_evidence(value: &mut BridgeDispatchEvidence, key: &[u8; 32]) -> Result<(), String> {
    let bytes = bridge_evidence_bytes(value)?;
    value.evidence_digest =
        crate::identity::length_prefixed_hash(&[b"bridge-dispatch-evidence-v1", &bytes]);
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| "INTERNAL")?;
    mac.update(&bytes);
    mac.update(value.evidence_digest.as_bytes());
    value.integrity_mac = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(())
}

fn valid_bridge_evidence(value: &BridgeDispatchEvidence, key: &[u8; 32]) -> Result<(), String> {
    if value.evidence_digest.len() != 64
        || value.integrity_mac.len() != 64
        || !value.evidence_digest.bytes().all(|b| b.is_ascii_hexdigit())
        || !value.integrity_mac.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("EVIDENCE_INVALID".into());
    }
    let bytes = bridge_evidence_bytes(value)?;
    let expected = crate::identity::length_prefixed_hash(&[b"bridge-dispatch-evidence-v1", &bytes]);
    if expected != value.evidence_digest {
        return Err("EVIDENCE_INVALID".into());
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| "INTERNAL")?;
    mac.update(&bytes);
    mac.update(value.evidence_digest.as_bytes());
    let supplied = (0..64)
        .step_by(2)
        .map(|index| u8::from_str_radix(&value.integrity_mac[index..index + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "EVIDENCE_INVALID")?;
    mac.verify_slice(&supplied)
        .map_err(|_| "EVIDENCE_INVALID".into())
}

fn require_relay_schema(connection: &Connection) -> Result<(), String> {
    if migration::preflight(connection, RELAY_SCHEMA)? != RELAY_SCHEMA {
        return Err("EXPLICIT_PREVIEW_MIGRATION_REQUIRED".into());
    }
    Ok(())
}

fn relative_ref(value: &str, label: &str) -> Result<String, String> {
    let value = nonempty(value, label)?;
    if value.starts_with('/') || value.starts_with('\\') || value.contains(':') {
        return Err(format!("{label} must be a Router-relative reference"));
    }
    let path = Path::new(&value);
    if path
        .components()
        .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(format!("{label} must be a Router-relative reference"));
    }
    Ok(value)
}

fn supported_artifact(kind: &str, mime: &str, size: i64) -> bool {
    let image = matches!(mime, "image/png" | "image/jpeg") && size <= 10 * 1024 * 1024;
    let document = matches!(
        mime,
        "text/plain"
            | "text/markdown"
            | "application/json"
            | "text/csv"
            | "application/pdf"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    ) && size <= 20 * 1024 * 1024;
    (kind == "IMAGE" && image) || (kind == "DOCUMENT" && document)
}

fn sha256(value: &str, label: &str) -> Result<String, String> {
    let value = nonempty(value, label)?;
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{label} must be a SHA-256 hex digest"));
    }
    Ok(value.to_ascii_lowercase())
}

fn relay_artifact_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RelayArtifact> {
    Ok(RelayArtifact {
        id: row.get(0)?,
        source_endpoint_id: row.get(1)?,
        source_run_id: row.get(2)?,
        source_message_ref: row.get(3)?,
        source_artifact_ref: row.get(4)?,
        kind: row.get(5)?,
        filename: row.get(6)?,
        mime: row.get(7)?,
        size: row.get(8)?,
        source_declared_sha256: row.get(9)?,
        transfer_declared_sha256: row.get(10)?,
        actual_sha256: row.get(11)?,
        source_hash_status: row.get(12)?,
        integrity_status: row.get(13)?,
        storage_ref: row.get(14)?,
        state: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

fn server_target_relative_path(
    handoff_id: &str,
    ordinal: usize,
    artifact: &RelayArtifact,
) -> String {
    let safe_name: String = artifact
        .filename
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() || matches!(value, '.' | '_' | '-') {
                value
            } else {
                '_'
            }
        })
        .collect();
    format!(
        ".ai-work-router/handoffs/{handoff_id}/in/{ordinal:02}-{}-{safe_name}",
        artifact.id
    )
}

fn relay_manifest_hash(attachments: &[(RelayArtifact, String)]) -> String {
    let mut hasher = Sha256::new();
    let mut add = |value: &str| {
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    };
    for (ordinal, (artifact, target)) in attachments.iter().enumerate() {
        for value in [
            ordinal.to_string().as_str(),
            artifact.id.as_str(),
            artifact.source_endpoint_id.as_str(),
            artifact.source_message_ref.as_str(),
            artifact.source_artifact_ref.as_str(),
            artifact.kind.as_str(),
            artifact.actual_sha256.as_deref().unwrap_or_default(),
            artifact.mime.as_str(),
            artifact.size.to_string().as_str(),
            target.as_str(),
        ] {
            add(value);
        }
    }
    format!("{:x}", hasher.finalize())
}

fn relay_envelope_hash(
    source_endpoint_id: &str,
    source_external_id: &str,
    source_message_ref: &str,
    capture_digest: &str,
    destination_endpoint_id: &str,
    destination_external_id: &str,
    workstream_id: &str,
    binding_revision: i64,
    root_revision: i64,
    destination_policy_json: &str,
    destination_policy_hash: &str,
    draft_revision: i64,
    approved_text: &str,
    attachments: &[(RelayArtifact, String)],
) -> String {
    let mut hasher = Sha256::new();
    let mut add = |value: &str| {
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    };
    for value in [
        RELAY_ENVELOPE_VERSION,
        "codex-relay-instruction-v1",
        source_endpoint_id,
        source_external_id,
        source_message_ref,
        capture_digest,
        destination_endpoint_id,
        destination_external_id,
        workstream_id,
        &binding_revision.to_string(),
        &root_revision.to_string(),
        destination_policy_json,
        destination_policy_hash,
        &draft_revision.to_string(),
        approved_text,
        &relay_manifest_hash(attachments),
    ] {
        add(value);
    }
    format!("{:x}", hasher.finalize())
}

fn endpoint_external_id(connection: &Connection, endpoint_id: &str) -> Result<String, String> {
    connection
        .query_row(
            "SELECT external_id FROM endpoints WHERE id=?1",
            params![endpoint_id],
            |row| row.get(0),
        )
        .map_err(db_error)
}

fn attachment_manifest_entry(artifact: &RelayArtifact, ordinal: usize, target: &str) -> String {
    serde_json::json!({"ordinal":ordinal,"artifactId":artifact.id,"sourceEndpointId":artifact.source_endpoint_id,"sourceMessageRef":artifact.source_message_ref,"sourceArtifactRef":artifact.source_artifact_ref,"kind":artifact.kind,"mime":artifact.mime,"sha256":artifact.actual_sha256,"size":artifact.size,"targetRelativePath":target}).to_string()
}

const RELAY_MANIFEST_MARKER: &str = "\n\n[Router attachment manifest — fixed, approved]\n";
const RELAY_OUTPUT_MARKER: &str = "\n[Router output location — fixed, approved]\n";
fn relay_input_text(
    body: &str,
    attachments: &[(RelayArtifact, String)],
    output_directory: Option<&str>,
) -> String {
    let mut value = body.trim_end().to_owned();
    value.push_str(RELAY_MANIFEST_MARKER);
    if attachments.is_empty() {
        value.push_str("No attachments.\n");
    } else {
        value.push_str(
            "Read document paths only when needed. Images are supplied as native image inputs.\n",
        );
        for (ordinal, (artifact, target)) in attachments.iter().enumerate() {
            value.push_str(&format!(
                "{ordinal}: kind={}, mime={}, bytes={}, sha256={}, path={}\n",
                artifact.kind,
                artifact.mime,
                artifact.size,
                artifact.actual_sha256.as_deref().unwrap_or(""),
                target
            ));
        }
    }
    if let Some(output_directory) = output_directory {
        value.push_str(RELAY_OUTPUT_MARKER);
        value.push_str("For this exact Codex run, create every file intended for return only in `");
        value.push_str(output_directory);
        value.push_str("/` relative to the approved workspace root. This Router-owned directory is the only return-file location; do not substitute workspace-root files or other paths.\n");
    } else if value.ends_with('\n') {
        // Browser-bound packets are rendered before review/hash/approval.
        // ChatGPT Web removes outer whitespace on submit; do not generate an
        // extra suffix LF that cannot later prove exact approved-byte acceptance.
        // Existing sealed rows and Codex-bound output contracts are untouched.
        value.pop();
    }
    value
}
fn editable_relay_body(text: &str) -> &str {
    text.split_once(RELAY_MANIFEST_MARKER)
        .map(|(body, _)| body)
        .unwrap_or(text)
}

impl RouterStore {
    /// Locates the one exact completed forward run whose bounded result text
    /// was evicted by the one-result-per-provider retention rule. The owner,
    /// relay direction, terminal dispatch, provider, and persisted native turn
    /// are all checked before an adapter may make an exact paginated read.
    pub fn completed_codex_history_recovery_target(
        &self,
        owner: &str,
        workstream_id: &str,
        run_id: &str,
    ) -> Result<(String, String), String> {
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|c| {
            require_relay_schema(c)?;
            c.query_row(
                "SELECT e.external_id,p.external_run_id FROM provider_runs p JOIN endpoints e ON e.id=p.endpoint_id JOIN provider_result_details r ON r.run_id=p.id JOIN handoff_dispatches d ON d.run_id=p.id JOIN handoffs h ON h.id=d.handoff_id JOIN relay_handoff_details x ON x.handoff_id=h.id WHERE p.id=?1 AND p.workstream_id=?2 AND p.provider='CODEX' AND p.status='COMPLETED' AND r.result_state='NOT_RETAINED' AND h.direction='CHATGPT_TO_CODEX' AND x.owner_principal_key=?3 AND d.phase='TERMINAL'",
                params![run_id, workstream_id, owner],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            ).optional().map_err(db_error)?.filter(|(thread, turn)| !thread.is_empty() && !turn.is_empty()).ok_or_else(|| "EXACT_CODEX_RESULT_REQUIRED".to_string())
        })
    }

    /// Commits only a byte-for-byte match to the result identity, hash and
    /// size persisted at terminal observation. It neither changes dispatch
    /// phase nor reconstructs a missing result from a different turn.
    pub fn restore_completed_codex_history_result(
        &self,
        owner: &str,
        workstream_id: &str,
        run_id: &str,
        item_id: &str,
        text: &str,
    ) -> Result<(), String> {
        let owner = nonempty(owner, "Relay owner")?;
        if item_id.is_empty()
            || item_id.len() > 256
            || text.trim().is_empty()
            || text.len() > 1024 * 1024
        {
            return Err("RESULT_INVALID".into());
        }
        self.with_connection(|c| {
            require_relay_schema(c)?;
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let row: (String, String, String, String, i64) = tx.query_row(
                "SELECT p.external_run_id,r.final_item_id,r.final_item_phase,r.result_hash,r.result_bytes FROM provider_runs p JOIN provider_result_details r ON r.run_id=p.id JOIN handoff_dispatches d ON d.run_id=p.id JOIN handoffs h ON h.id=d.handoff_id JOIN relay_handoff_details x ON x.handoff_id=h.id WHERE p.id=?1 AND p.workstream_id=?2 AND p.provider='CODEX' AND p.status='COMPLETED' AND r.result_state='NOT_RETAINED' AND h.direction='CHATGPT_TO_CODEX' AND x.owner_principal_key=?3 AND d.phase='TERMINAL'",
                params![run_id, workstream_id, owner],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            ).optional().map_err(db_error)?.ok_or("EXACT_CODEX_RESULT_REQUIRED")?;
            let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
            if row.0.is_empty() || row.1 != item_id || row.2 != "final_answer" || row.3 != digest || row.4 != text.len() as i64 {
                return Err("HISTORY_CHANGED".into());
            }
            tx.execute("UPDATE provider_result_details SET result_state='NOT_RETAINED',updated_at=?3 WHERE run_id IN (SELECT id FROM provider_runs WHERE workstream_id=?1 AND provider='CODEX' AND id!=?2 AND result_text IS NOT NULL)", params![workstream_id,run_id,now()]).map_err(db_error)?;
            tx.execute("UPDATE provider_runs SET result_text=NULL WHERE workstream_id=?1 AND provider='CODEX' AND id!=?2 AND result_text IS NOT NULL", params![workstream_id,run_id]).map_err(db_error)?;
            let changed = tx.execute("UPDATE provider_runs SET result_text=?2,result_identity=?3,updated_at=?4 WHERE id=?1 AND workstream_id=?5 AND provider='CODEX' AND status='COMPLETED'", params![run_id,text,item_id,now(),workstream_id]).map_err(db_error)?;
            if changed != 1 { return Err("HISTORY_CHANGED".into()); }
            let detail = tx.execute("UPDATE provider_result_details SET result_state='AVAILABLE',source='HISTORY_PAGED',result_error_code=NULL,updated_at=?2 WHERE run_id=?1 AND result_state='NOT_RETAINED'", params![run_id,now()]).map_err(db_error)?;
            if detail != 1 { return Err("HISTORY_CHANGED".into()); }
            tx.commit().map_err(db_error)
        })
    }

    /// Returns only the exact terminal text bound to a completed native run.
    /// This intentionally has no "latest turn" fallback and gives reverse
    /// relay preparation a stable source identity before it creates a draft.
    pub fn completed_codex_relay_source(
        &self,
        workstream_id: &str,
        run_id: &str,
    ) -> Result<(Endpoint, String, String), String> {
        self.with_connection(|c| {
            require_relay_schema(c)?;
            let row: (String, String, String, String, String) = c.query_row(
                "SELECT p.endpoint_id,e.provider,e.status,p.external_run_id,p.result_text FROM provider_runs p JOIN endpoints e ON e.id=p.endpoint_id JOIN provider_result_details d ON d.run_id=p.id WHERE p.id=?1 AND p.workstream_id=?2 AND p.status='COMPLETED' AND d.result_state='AVAILABLE'",
                params![run_id, workstream_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            ).optional().map_err(db_error)?.ok_or("EXACT_CODEX_RESULT_REQUIRED")?;
            if row.1 != "CODEX" || row.2 != "ACTIVE" || row.3.is_empty() || row.4.is_empty() { return Err("EXACT_CODEX_RESULT_REQUIRED".into()); }
            Ok((endpoint_by_id(c, &row.0)?, row.3, row.4))
        })
    }

    /// Resolves the one forward relay that produced an exact completed Codex
    /// run. Output collection uses this only to rebuild the already-approved
    /// sealed snapshot; there is no latest-run or thread-history fallback.
    pub fn completed_codex_relay_dispatch(
        &self,
        owner: &str,
        workstream_id: &str,
        run_id: &str,
    ) -> Result<(String, String), String> {
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|c| {
            require_relay_schema(c)?;
            c.query_row(
                "SELECT h.id,d.dispatch_id FROM provider_runs p JOIN provider_result_details r ON r.run_id=p.id JOIN handoff_dispatches d ON d.run_id=p.id JOIN handoffs h ON h.id=d.handoff_id JOIN relay_handoff_details x ON x.handoff_id=h.id WHERE p.id=?1 AND p.workstream_id=?2 AND p.status='COMPLETED' AND r.result_state='AVAILABLE' AND h.direction='CHATGPT_TO_CODEX' AND x.owner_principal_key=?3 AND d.phase='TERMINAL'",
                params![run_id, workstream_id, owner],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ).optional().map_err(db_error)?.ok_or("EXACT_CODEX_RESULT_REQUIRED".into())
        })
    }

    /// Rebuilds the sealed forward-dispatch view solely to collect output from
    /// its exact completed Codex run.  Unlike a physical write, this local
    /// read must remain available after a later ChatGPT endpoint rebind: the
    /// forward source endpoint and its binding revision are historical facts,
    /// while the output directory remains rooted in the still-validated
    /// project execution root.
    pub fn completed_codex_output_snapshot(
        &self,
        owner: &str,
        workstream_id: &str,
        run_id: &str,
    ) -> Result<RelayDispatchSnapshot, String> {
        let (handoff_id, dispatch_id) =
            self.completed_codex_relay_dispatch(owner, workstream_id, run_id)?;
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            historical_output_snapshot(connection, &owner, &handoff_id, run_id, &dispatch_id)
        })
    }

    pub fn completed_codex_relay_artifacts(
        &self,
        workstream_id: &str,
        run_id: &str,
        artifact_ids: &[String],
    ) -> Result<Vec<RelayArtifact>, String> {
        if artifact_ids.len() > 10 {
            return Err("RELAY_ATTACHMENT_LIMIT".into());
        }
        self.with_connection(|c| {
            require_relay_schema(c)?;
            let source_id: String = c.query_row(
                "SELECT p.endpoint_id FROM provider_runs p JOIN provider_result_details d ON d.run_id=p.id WHERE p.id=?1 AND p.workstream_id=?2 AND p.status='COMPLETED' AND d.result_state='AVAILABLE'",
                params![run_id, workstream_id], |r| r.get(0),
            ).optional().map_err(db_error)?.ok_or("EXACT_CODEX_RESULT_REQUIRED")?;
            let mut seen = std::collections::HashSet::new();
            let mut artifacts = Vec::with_capacity(artifact_ids.len());
            for artifact_id in artifact_ids {
                if !seen.insert(artifact_id.as_str()) { return Err("RELAY_ATTACHMENT_DUPLICATE".into()); }
                let artifact = c.query_row("SELECT id,source_endpoint_id,source_run_id,source_message_ref,source_artifact_ref,kind,filename,mime,size,source_declared_sha256,transfer_declared_sha256,actual_sha256,source_hash_status,integrity_status,storage_ref,state,created_at,updated_at FROM artifact_materializations WHERE id=?1", params![artifact_id], relay_artifact_row).optional().map_err(db_error)?.ok_or("EXACT_CODEX_ARTIFACT_REQUIRED")?;
                if artifact.source_endpoint_id != source_id || artifact.source_run_id.as_deref() != Some(run_id) || artifact.state != "READY" || artifact.integrity_status != "VERIFIED" { return Err("EXACT_CODEX_ARTIFACT_REQUIRED".into()); }
                artifacts.push(artifact);
            }
            Ok(artifacts)
        })
    }
    pub fn browser_endpoint_settings(
        &self,
        endpoint_id: &str,
        owner_principal_key: &str,
    ) -> Result<Option<BrowserEndpointSettings>, String> {
        let owner = nonempty(owner_principal_key, "Browser endpoint owner")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            connection
                .query_row(
                    "SELECT endpoint_id,owner_principal_key,observe_enabled,settings_revision,paired_by,paired_at,updated_at FROM browser_endpoint_settings WHERE endpoint_id=?1 AND owner_principal_key=?2",
                    params![endpoint_id, owner],
                    |row| Ok(BrowserEndpointSettings { endpoint_id: row.get(0)?, owner_principal_key: row.get(1)?, observe_enabled: row.get::<_, i64>(2)? != 0, settings_revision: row.get(3)?, paired_by: row.get(4)?, paired_at: row.get(5)?, updated_at: row.get(6)? }),
                )
                .optional()
                .map_err(db_error)
        })
    }

    pub fn configure_browser_endpoint(
        &self,
        input: BrowserEndpointSettingsInput,
    ) -> Result<BrowserEndpointSettings, String> {
        let owner = nonempty(&input.owner_principal_key, "Browser endpoint owner")?;
        let paired_by = nonempty(&input.paired_by, "Browser endpoint pairing principal")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            let endpoint = endpoint_by_id(connection, &input.endpoint_id)?;
            if endpoint.provider != "CHATGPT" || endpoint.status != "ACTIVE" {
                return Err("ACTIVE_CHATGPT_ENDPOINT_REQUIRED".into());
            }
            let timestamp = now();
            let current: Option<BrowserEndpointSettings> = connection
                .query_row(
                    "SELECT endpoint_id,owner_principal_key,observe_enabled,settings_revision,paired_by,paired_at,updated_at FROM browser_endpoint_settings WHERE endpoint_id=?1",
                    params![input.endpoint_id],
                    |row| Ok(BrowserEndpointSettings { endpoint_id: row.get(0)?, owner_principal_key: row.get(1)?, observe_enabled: row.get::<_, i64>(2)? != 0, settings_revision: row.get(3)?, paired_by: row.get(4)?, paired_at: row.get(5)?, updated_at: row.get(6)? }),
                )
                .optional()
                .map_err(db_error)?;
            let revision = current.as_ref().map_or(1, |value| value.settings_revision + 1);
            connection.execute(
                "INSERT INTO browser_endpoint_settings(endpoint_id,owner_principal_key,observe_enabled,settings_revision,paired_by,paired_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6) ON CONFLICT(endpoint_id) DO UPDATE SET owner_principal_key=excluded.owner_principal_key,observe_enabled=excluded.observe_enabled,settings_revision=excluded.settings_revision,paired_by=excluded.paired_by,paired_at=excluded.paired_at,updated_at=excluded.updated_at",
                params![input.endpoint_id,owner,input.observe_enabled as i64,revision,paired_by,timestamp],
            ).map_err(db_error)?;
            Ok(BrowserEndpointSettings { endpoint_id: input.endpoint_id, owner_principal_key: owner, observe_enabled: input.observe_enabled, settings_revision: revision, paired_by, paired_at: timestamp, updated_at: timestamp })
        })
    }

    pub fn set_browser_observation(
        &self,
        endpoint_id: &str,
        owner_principal_key: &str,
        expected_settings_revision: i64,
        enabled: bool,
    ) -> Result<BrowserEndpointSettings, String> {
        let owner = nonempty(owner_principal_key, "Browser endpoint owner")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            let timestamp = now();
            let changed = connection.execute(
                "UPDATE browser_endpoint_settings SET observe_enabled=?4,settings_revision=settings_revision+1,updated_at=?5 WHERE endpoint_id=?1 AND owner_principal_key=?2 AND settings_revision=?3",
                params![endpoint_id,owner,expected_settings_revision,enabled as i64,timestamp],
            ).map_err(db_error)?;
            if changed != 1 { return Err("BROWSER_SETTINGS_CHANGED".into()); }
            connection.query_row(
                "SELECT endpoint_id,owner_principal_key,observe_enabled,settings_revision,paired_by,paired_at,updated_at FROM browser_endpoint_settings WHERE endpoint_id=?1",
                params![endpoint_id],
                |row| Ok(BrowserEndpointSettings { endpoint_id: row.get(0)?, owner_principal_key: row.get(1)?, observe_enabled: row.get::<_, i64>(2)? != 0, settings_revision: row.get(3)?, paired_by: row.get(4)?, paired_at: row.get(5)?, updated_at: row.get(6)? }),
            ).map_err(db_error)
        })
    }

    pub fn persist_relay_artifact(
        &self,
        input: RelayArtifactInput,
    ) -> Result<RelayArtifact, String> {
        let storage_ref = relative_ref(&input.storage_ref, "Artifact storage reference")?;
        let transfer_sha = sha256(&input.transfer_declared_sha256, "Artifact transfer hash")?;
        let actual_sha = input
            .actual_sha256
            .as_deref()
            .map(|value| sha256(value, "Artifact actual hash"))
            .transpose()?;
        let source_sha = input
            .source_declared_sha256
            .as_deref()
            .map(|value| sha256(value, "Artifact source hash"))
            .transpose()?;
        if input.size < 0
            || !["DOCUMENT", "IMAGE"].contains(&input.kind.as_str())
            || !["MATERIALIZING", "READY", "FAILED", "PURGED"].contains(&input.state.as_str())
            || !["DECLARED", "UNAVAILABLE", "MISMATCH"].contains(&input.source_hash_status.as_str())
            || !["PENDING", "VERIFIED", "MISMATCH", "FAILED"]
                .contains(&input.integrity_status.as_str())
        {
            return Err("INVALID_RELAY_ARTIFACT".into());
        }
        if !supported_artifact(&input.kind, &input.mime, input.size)
            || input.filename.is_empty()
            || input.filename.chars().count() > 180
            || input.filename.chars().any(char::is_control)
        {
            return Err("RELAY_ARTIFACT_UNSUPPORTED".into());
        }
        if input.state == "READY"
            && (input.integrity_status != "VERIFIED"
                || actual_sha.as_deref() != Some(transfer_sha.as_str()))
        {
            return Err("RELAY_ARTIFACT_NOT_VERIFIED".into());
        }
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            let endpoint = endpoint_by_id(connection, &input.source_endpoint_id)?;
            if endpoint.status != "ACTIVE" { return Err("ACTIVE_SOURCE_ENDPOINT_REQUIRED".into()); }
            if let Some(run_id) = &input.source_run_id {
                let run = provider_run_by_id(connection, run_id)?;
                if run.workstream_id != endpoint.workstream_id || run.endpoint_id != endpoint.id { return Err("ARTIFACT_SOURCE_RUN_MISMATCH".into()); }
            }
            let timestamp = now();
            connection.execute(
                "INSERT INTO artifact_materializations(id,source_endpoint_id,source_run_id,source_message_ref,source_artifact_ref,kind,filename,mime,size,source_declared_sha256,transfer_declared_sha256,actual_sha256,source_hash_status,integrity_status,storage_ref,state,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?17)",
                params![input.id,input.source_endpoint_id,input.source_run_id,input.source_message_ref,input.source_artifact_ref,input.kind,input.filename,input.mime,input.size,source_sha,transfer_sha,actual_sha,input.source_hash_status,input.integrity_status,storage_ref,input.state,timestamp],
            ).map_err(db_error)?;
            connection.query_row("SELECT id,source_endpoint_id,source_run_id,source_message_ref,source_artifact_ref,kind,filename,mime,size,source_declared_sha256,transfer_declared_sha256,actual_sha256,source_hash_status,integrity_status,storage_ref,state,created_at,updated_at FROM artifact_materializations WHERE id=?1",params![input.id],relay_artifact_row).map_err(db_error)
        })
    }

    pub fn create_ready_relay_handoff(
        &self,
        handoff: NewRelayHandoff,
    ) -> Result<HandoffHistoryItem, String> {
        // An exact ChatGPT reply may legitimately consist solely of
        // downloadable artifacts.  In that case `relay_input_text` below
        // creates the non-empty, hash-bound manifest instruction.  Keep the
        // original body requirement for a handoff with no attachments, so an
        // empty message can never become a content-less draft.
        if (handoff.original_text.trim().is_empty() || handoff.approved_text.trim().is_empty())
            && handoff.attachments.is_empty()
            || handoff.root_revision <= 0
            || handoff.draft_revision <= 0
            || handoff.attachments.len() > 10
            || !["CHATGPT_TO_CODEX", "CODEX_TO_CHATGPT"].contains(&handoff.direction.as_str())
        {
            return Err("INVALID_RELAY_HANDOFF".into());
        }
        let owner = nonempty(&handoff.owner_principal_key, "Relay handoff owner")?;
        let client_request_id = nonempty(&handoff.client_request_id, "Relay client request ID")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let source = endpoint_by_id(&transaction, &handoff.source_endpoint_id)?;
            let destination = endpoint_by_id(&transaction, &handoff.destination_endpoint_id)?;
            if source.workstream_id != handoff.workstream_id || destination.workstream_id != handoff.workstream_id || source.status != "ACTIVE" || destination.status != "ACTIVE" { return Err("RELAY_HANDOFF_ENDPOINT_MISMATCH".into()); }
            let binding_revision: i64 = transaction.query_row("SELECT binding_revision FROM workstreams WHERE id=?1 AND status='ACTIVE' AND trashed_at IS NULL", params![handoff.workstream_id], |row| row.get(0)).map_err(db_error)?;
            if binding_revision != handoff.binding_revision { return Err("RELAY_BINDING_CHANGED".into()); }
            if let Some(run_id) = &handoff.source_run_id {
                let run = provider_run_by_id(&transaction, run_id)?;
                if run.workstream_id != handoff.workstream_id || run.endpoint_id != source.id { return Err("RELAY_SOURCE_RUN_MISMATCH".into()); }
            }
            let mut artifacts = Vec::with_capacity(handoff.attachments.len());
            let mut selected_ids = std::collections::HashSet::new();
            let mut total_bytes = 0i64;
            for selection in &handoff.attachments {
                if !selected_ids.insert(selection.artifact_id.as_str()) { return Err("RELAY_ATTACHMENT_DUPLICATE".into()); }
                let artifact = transaction.query_row("SELECT id,source_endpoint_id,source_run_id,source_message_ref,source_artifact_ref,kind,filename,mime,size,source_declared_sha256,transfer_declared_sha256,actual_sha256,source_hash_status,integrity_status,storage_ref,state,created_at,updated_at FROM artifact_materializations WHERE id=?1",params![selection.artifact_id],relay_artifact_row).optional().map_err(db_error)?.ok_or("RELAY_ARTIFACT_NOT_FOUND")?;
                if artifact.source_endpoint_id != source.id || artifact.source_message_ref != handoff.source_message_ref || artifact.state != "READY" || artifact.integrity_status != "VERIFIED" || artifact.actual_sha256.as_deref() != Some(selection.approved_sha256.as_str()) || artifact.size != selection.approved_size { return Err("RELAY_ARTIFACT_CHANGED".into()); }
                if !supported_artifact(&artifact.kind, &artifact.mime, artifact.size) || artifact.mime != selection.mime { return Err("RELAY_ATTACHMENT_TYPE_MISMATCH".into()); }
                total_bytes = total_bytes.checked_add(artifact.size).ok_or("RELAY_ATTACHMENT_LIMIT")?;
                if total_bytes > 50 * 1024 * 1024 { return Err("RELAY_ATTACHMENT_LIMIT".into()); }
                artifacts.push(artifact);
            }
            let record_id = id();
            let manifest = artifacts.iter().enumerate().map(|(ordinal, artifact)| (artifact.clone(), server_target_relative_path(&record_id, ordinal, artifact))).collect::<Vec<_>>();
            let output_directory = (handoff.direction == "CHATGPT_TO_CODEX"
                && destination.provider == "CODEX")
                .then(|| format!(".ai-work-router/handoffs/{record_id}/out"));
            let approved_text = relay_input_text(
                &handoff.approved_text,
                &manifest,
                output_directory.as_deref(),
            );
            let payload_hash = relay_envelope_hash(&source.id, &source.external_id, &handoff.source_message_ref, &handoff.capture_digest, &destination.id, &destination.external_id, &handoff.workstream_id, handoff.binding_revision, handoff.root_revision, &handoff.destination_policy_json, &handoff.destination_policy_hash, handoff.draft_revision, &approved_text, &manifest);
            let timestamp = now();
            transaction.execute("INSERT INTO handoffs(id,workstream_id,source_endpoint_id,destination_endpoint_id,direction,source_response_identity,original_text,approved_text,status,payload_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'READY',?9,?10)",params![record_id,handoff.workstream_id,handoff.source_endpoint_id,handoff.destination_endpoint_id,handoff.direction,handoff.source_message_ref,handoff.original_text,approved_text,payload_hash,timestamp]).map_err(db_error)?;
            transaction.execute("INSERT INTO relay_handoff_details(handoff_id,owner_principal_key,source_run_id,source_message_ref,capture_digest,source_completeness,binding_revision,root_revision,destination_policy_json,destination_policy_hash,draft_revision,envelope_version,manifest_hash,client_request_id,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?15)",params![record_id,owner,handoff.source_run_id,handoff.source_message_ref,handoff.capture_digest,handoff.source_completeness,handoff.binding_revision,handoff.root_revision,handoff.destination_policy_json,handoff.destination_policy_hash,handoff.draft_revision,RELAY_ENVELOPE_VERSION,relay_manifest_hash(&manifest),client_request_id,timestamp]).map_err(db_error)?;
            for (ordinal, (selection, artifact)) in handoff.attachments.iter().zip(artifacts.iter()).enumerate() {
                let target = &manifest[ordinal].1;
                transaction.execute("INSERT INTO handoff_attachments(id,handoff_id,filename,original_path,size,sha256,integrity_status,created_at) VALUES(?1,?2,?3,?4,?5,?6,'VERIFIED',?7)",params![selection.attachment_id,record_id,artifact.filename,format!("artifact://{}",artifact.id),artifact.size,selection.approved_sha256,timestamp]).map_err(db_error)?;
                transaction.execute("INSERT INTO handoff_attachment_details(attachment_id,handoff_id,artifact_id,ordinal,mime,approved_sha256,approved_size,manifest_entry_json,target_relative_path) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![selection.attachment_id,record_id,artifact.id,ordinal as i64,selection.mime,selection.approved_sha256,selection.approved_size,attachment_manifest_entry(artifact,ordinal,target),target]).map_err(db_error)?;
            }
            transaction.commit().map_err(db_error)?;
            handoff_by_id(connection, &record_id)
        })
    }

    /// A completed materialization is immutable Router-owned state. Callers
    /// use this before asking a browser to download again, so a second review
    /// of the same exact source never becomes a second page action.
    pub fn ready_relay_artifact(
        &self,
        source_endpoint_id: &str,
        source_message_ref: &str,
        source_artifact_ref: &str,
    ) -> Result<Option<RelayArtifact>, String> {
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            connection.query_row(
                "SELECT id,source_endpoint_id,source_run_id,source_message_ref,source_artifact_ref,kind,filename,mime,size,source_declared_sha256,transfer_declared_sha256,actual_sha256,source_hash_status,integrity_status,storage_ref,state,created_at,updated_at FROM artifact_materializations WHERE source_endpoint_id=?1 AND source_message_ref=?2 AND source_artifact_ref=?3 AND state='READY' AND integrity_status='VERIFIED'",
                params![source_endpoint_id, source_message_ref, source_artifact_ref],
                relay_artifact_row,
            ).optional().map_err(db_error)
        })
    }

    pub fn relay_artifact_for_review(
        &self,
        owner: &str,
        handoff_id: &str,
        artifact_id: &str,
    ) -> Result<RelayArtifact, String> {
        let owner = nonempty(owner, "Relay artifact owner")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            connection.query_row(
                "SELECT a.id,a.source_endpoint_id,a.source_run_id,a.source_message_ref,a.source_artifact_ref,a.kind,a.filename,a.mime,a.size,a.source_declared_sha256,a.transfer_declared_sha256,a.actual_sha256,a.source_hash_status,a.integrity_status,a.storage_ref,a.state,a.created_at,a.updated_at FROM relay_handoff_details d JOIN handoff_attachment_details x ON x.handoff_id=d.handoff_id JOIN artifact_materializations a ON a.id=x.artifact_id WHERE d.handoff_id=?1 AND d.owner_principal_key=?2 AND a.id=?3",
                params![handoff_id, owner, artifact_id], relay_artifact_row,
            ).optional().map_err(db_error)?.ok_or("NOT_FOUND".into())
        })
    }

    pub fn relay_review(&self, owner: &str, handoff_id: &str) -> Result<RelayReview, String> {
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            let row: (String, String, String, String, String, String, i64, i64, String, String, i64, String, Option<String>, Option<i64>, Option<i64>) = connection.query_row(
                "SELECT d.owner_principal_key,h.source_endpoint_id,h.destination_endpoint_id,d.source_message_ref,d.capture_digest,h.workstream_id,d.binding_revision,d.root_revision,d.destination_policy_json,d.destination_policy_hash,d.draft_revision,d.manifest_hash,d.review_nonce_hash,d.approval_expires_at,d.consumed_at FROM relay_handoff_details d JOIN handoffs h ON h.id=d.handoff_id WHERE d.handoff_id=?1 AND d.owner_principal_key=?2",
                params![handoff_id, owner], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?,row.get(10)?,row.get(11)?,row.get(12)?,row.get(13)?,row.get(14)?))
            ).optional().map_err(db_error)?.ok_or("NOT_FOUND")?;
            let handoff = handoff_by_id(connection, handoff_id)?;
            let mut statement = connection.prepare("SELECT a.id,a.source_endpoint_id,a.source_run_id,a.source_message_ref,a.source_artifact_ref,a.kind,a.filename,a.mime,a.size,a.source_declared_sha256,a.transfer_declared_sha256,a.actual_sha256,a.source_hash_status,a.integrity_status,a.storage_ref,a.state,a.created_at,a.updated_at,x.target_relative_path FROM handoff_attachment_details x JOIN artifact_materializations a ON a.id=x.artifact_id WHERE x.handoff_id=?1 ORDER BY x.ordinal").map_err(db_error)?;
            let attachment_rows = statement.query_map(params![handoff_id], |record| Ok((relay_artifact_row(record)?, record.get::<_, String>(18)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            let (attachments, target_relative_paths): (Vec<_>, Vec<_>) = attachment_rows.into_iter().unzip();
            let approval_state = if row.14.is_some() { "APPROVED" } else { match (row.12, row.13) { (Some(_), Some(expires)) if expires > now() => "PENDING", (Some(_), _) => "EXPIRED", _ => "REQUIRED" } }.into();
            Ok(RelayReview { handoff, owner_principal_key: row.0, source_endpoint_id: row.1, destination_endpoint_id: row.2, source_message_ref: row.3, capture_digest: row.4, binding_revision: row.6, root_revision: row.7, destination_policy_json: row.8, destination_policy_hash: row.9, draft_revision: row.10, manifest_hash: row.11, approval_state, attachments, target_relative_paths })
        })
    }

    pub fn edit_relay_draft(
        &self,
        owner: &str,
        handoff_id: &str,
        expected_draft_revision: i64,
        text: &str,
        artifact_ids: &[String],
    ) -> Result<RelayReview, String> {
        if text.trim().is_empty() || text.as_bytes().len() > 1024 * 1024 || artifact_ids.len() > 10
        {
            return Err("INVALID_RELAY_DRAFT".into());
        }
        let owner = nonempty(owner, "Relay owner")?;
        let mut seen = std::collections::HashSet::new();
        if artifact_ids
            .iter()
            .any(|value| !seen.insert(value.as_str()))
        {
            return Err("RELAY_ATTACHMENT_DUPLICATE".into());
        }
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let (source_endpoint, destination_endpoint, workstream, direction, source_ref, capture_digest, binding_revision, root_revision, policy_json, policy_hash, revision): (String,String,String,String,String,String,i64,i64,String,String,i64) = transaction.query_row("SELECT h.source_endpoint_id,h.destination_endpoint_id,h.workstream_id,h.direction,d.source_message_ref,d.capture_digest,d.binding_revision,d.root_revision,d.destination_policy_json,d.destination_policy_hash,d.draft_revision FROM relay_handoff_details d JOIN handoffs h ON h.id=d.handoff_id WHERE d.handoff_id=?1 AND d.owner_principal_key=?2",params![handoff_id,owner],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?,row.get(10)?))).optional().map_err(db_error)?.ok_or("NOT_FOUND")?;
            if revision != expected_draft_revision { return Err("STALE_RELAY_DRAFT".into()); }
            let mut artifacts = Vec::with_capacity(artifact_ids.len());
            let mut total = 0i64;
            for artifact_id in artifact_ids {
                let artifact = transaction.query_row("SELECT id,source_endpoint_id,source_run_id,source_message_ref,source_artifact_ref,kind,filename,mime,size,source_declared_sha256,transfer_declared_sha256,actual_sha256,source_hash_status,integrity_status,storage_ref,state,created_at,updated_at FROM artifact_materializations WHERE id=?1",params![artifact_id],relay_artifact_row).optional().map_err(db_error)?.ok_or("RELAY_ARTIFACT_NOT_FOUND")?;
                if artifact.source_endpoint_id != source_endpoint || artifact.source_message_ref != source_ref || artifact.state != "READY" || artifact.integrity_status != "VERIFIED" || artifact.actual_sha256.is_none() || !supported_artifact(&artifact.kind,&artifact.mime,artifact.size) { return Err("RELAY_ARTIFACT_CHANGED".into()); }
                total = total.checked_add(artifact.size).ok_or("RELAY_ATTACHMENT_LIMIT")?;
                if total > 50 * 1024 * 1024 { return Err("RELAY_ATTACHMENT_LIMIT".into()); }
                artifacts.push(artifact);
            }
            transaction.execute("DELETE FROM handoff_attachment_details WHERE handoff_id=?1",params![handoff_id]).map_err(db_error)?;
            transaction.execute("DELETE FROM handoff_attachments WHERE handoff_id=?1",params![handoff_id]).map_err(db_error)?;
            let timestamp = now();
            let manifest_entries = artifacts.iter().enumerate().map(|(ordinal, artifact)| (artifact.clone(), server_target_relative_path(handoff_id, ordinal, artifact))).collect::<Vec<_>>();
            for (ordinal, artifact) in artifacts.iter().enumerate() {
                let attachment_id = id();
                let digest = artifact.actual_sha256.as_deref().ok_or("RELAY_ARTIFACT_CHANGED")?;
                transaction.execute("INSERT INTO handoff_attachments(id,handoff_id,filename,original_path,size,sha256,integrity_status,created_at) VALUES(?1,?2,?3,?4,?5,?6,'VERIFIED',?7)",params![attachment_id,handoff_id,artifact.filename,format!("artifact://{}",artifact.id),artifact.size,digest,timestamp]).map_err(db_error)?;
                let target = &manifest_entries[ordinal].1;
                transaction.execute("INSERT INTO handoff_attachment_details(attachment_id,handoff_id,artifact_id,ordinal,mime,approved_sha256,approved_size,manifest_entry_json,target_relative_path) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![attachment_id,handoff_id,artifact.id,ordinal as i64,artifact.mime,digest,artifact.size,attachment_manifest_entry(artifact,ordinal,target),target]).map_err(db_error)?;
            }
            let next_revision = revision + 1;
            let manifest = relay_manifest_hash(&manifest_entries);
            let source_external = endpoint_external_id(&transaction, &source_endpoint)?;
            let destination_external = endpoint_external_id(&transaction, &destination_endpoint)?;
            let output_directory = (direction == "CHATGPT_TO_CODEX")
                .then(|| format!(".ai-work-router/handoffs/{handoff_id}/out"));
            let approved_text = relay_input_text(
                editable_relay_body(text),
                &manifest_entries,
                output_directory.as_deref(),
            );
            let payload = relay_envelope_hash(&source_endpoint, &source_external, &source_ref, &capture_digest, &destination_endpoint, &destination_external, &workstream, binding_revision, root_revision, &policy_json, &policy_hash, next_revision, &approved_text, &manifest_entries);
            transaction.execute("UPDATE handoffs SET approved_text=?2,payload_hash=?3 WHERE id=?1 AND status='READY'",params![handoff_id,approved_text,payload]).map_err(db_error)?;
            transaction.execute("UPDATE relay_handoff_details SET draft_revision=?2,manifest_hash=?3,review_nonce_hash=NULL,approved_principal_key=NULL,approval_expires_at=NULL,consumed_at=NULL,updated_at=?4 WHERE handoff_id=?1",params![handoff_id,next_revision,manifest,timestamp]).map_err(db_error)?;
            transaction.commit().map_err(db_error)?;
            Ok(())
        })?;
        self.relay_review(&owner, handoff_id)
    }

    pub fn issue_relay_approval_nonce(
        &self,
        owner: &str,
        handoff_id: &str,
        expected_draft_revision: i64,
        nonce_hash: &str,
        expires_at: i64,
    ) -> Result<(), String> {
        let owner = nonempty(owner, "Relay owner")?;
        let nonce_hash = sha256(nonce_hash, "Relay review nonce")?;
        if expires_at <= now() {
            return Err("RELAY_APPROVAL_EXPIRED".into());
        }
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            let changed = connection.execute("UPDATE relay_handoff_details SET review_nonce_hash=?4,approval_expires_at=?5,updated_at=?6 WHERE handoff_id=?1 AND owner_principal_key=?2 AND draft_revision=?3 AND consumed_at IS NULL",params![handoff_id,owner,expected_draft_revision,nonce_hash,expires_at,now()]).map_err(db_error)?;
            if changed != 1 { return Err("STALE_RELAY_DRAFT".into()); }
            Ok(())
        })
    }

    /// Checks the still-unconsumed human approval before a read-only live
    /// control preflight. Unlike `approve_relay_draft`, this does not create a
    /// run, dispatch, ProviderRun, or writer claim.
    pub fn validate_relay_approval_nonce(
        &self,
        owner: &str,
        handoff_id: &str,
        expected_draft_revision: i64,
        nonce_hash: &str,
    ) -> Result<(), String> {
        let owner = nonempty(owner, "Relay owner")?;
        let nonce_hash = sha256(nonce_hash, "Relay review nonce")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            let valid: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM relay_handoff_details d JOIN handoffs h ON h.id=d.handoff_id WHERE d.handoff_id=?1 AND d.owner_principal_key=?2 AND d.draft_revision=?3 AND h.status='READY' AND d.consumed_at IS NULL AND d.approval_expires_at>?4 AND d.review_nonce_hash=?5)",
                    params![handoff_id, owner, expected_draft_revision, now(), nonce_hash],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            if valid { Ok(()) } else { Err("STALE_RELAY_APPROVAL".into()) }
        })
    }

    /// Seals a draft for the later H4 dispatcher. This method deliberately
    /// claims neither a run nor a dispatch; it only proves the human review
    /// still names the exact current endpoints, binding, browser setting and
    /// immutable attachment bytes.
    pub fn approve_relay_draft(
        &self,
        owner: &str,
        handoff_id: &str,
        expected_draft_revision: i64,
        nonce_hash: &str,
    ) -> Result<RelayReview, String> {
        let owner = nonempty(owner, "Relay owner")?;
        let nonce_hash = sha256(nonce_hash, "Relay review nonce")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let row: (String,String,String,i64,i64,String,String,String,String,Option<i64>,Option<String>,Option<i64>) = tx.query_row("SELECT h.workstream_id,h.source_endpoint_id,h.destination_endpoint_id,d.binding_revision,d.root_revision,d.source_message_ref,d.capture_digest,d.destination_policy_json,d.destination_policy_hash,d.approval_expires_at,d.review_nonce_hash,d.consumed_at FROM relay_handoff_details d JOIN handoffs h ON h.id=d.handoff_id WHERE d.handoff_id=?1 AND d.owner_principal_key=?2 AND d.draft_revision=?3 AND h.status='READY'",params![handoff_id,owner,expected_draft_revision],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?,r.get(11)?))).optional().map_err(db_error)?.ok_or("STALE_RELAY_DRAFT")?;
            if row.9.is_none_or(|expires| expires <= now()) || row.10.as_deref() != Some(nonce_hash.as_str()) { return Err("STALE_RELAY_APPROVAL".into()); }
            if row.11.is_some() { return Err("RELAY_APPROVAL_ALREADY_CONSUMED".into()); }
            let current_binding: i64 = tx.query_row("SELECT binding_revision FROM workstreams WHERE id=?1 AND status='ACTIVE' AND trashed_at IS NULL",params![row.0],|r|r.get(0)).map_err(db_error)?;
            if current_binding != row.3 { return Err("RELAY_BINDING_CHANGED".into()); }
            let source: (String,String) = tx.query_row("SELECT provider,status FROM endpoints WHERE id=?1",params![row.1],|r|Ok((r.get(0)?,r.get(1)?))).map_err(db_error)?;
            let destination: (String,String) = tx.query_row("SELECT provider,status FROM endpoints WHERE id=?1",params![row.2],|r|Ok((r.get(0)?,r.get(1)?))).map_err(db_error)?;
            let forward = source == ("CHATGPT".into(), "ACTIVE".into())
                && destination == ("CODEX".into(), "ACTIVE".into());
            let reverse = source == ("CODEX".into(), "ACTIVE".into())
                && destination == ("CHATGPT".into(), "ACTIVE".into());
            if !forward && !reverse { return Err("RELAY_ENDPOINT_CHANGED".into()); }
            let policy: serde_json::Value = serde_json::from_str(&row.7).map_err(|_| "RELAY_POLICY_INVALID")?;
            let expected_settings = policy.get("browserSettingsRevision").and_then(serde_json::Value::as_i64).ok_or("RELAY_POLICY_INVALID")?;
            if policy.get("destinationEndpointId").and_then(serde_json::Value::as_str) != Some(row.2.as_str()) {
                return Err("RELAY_POLICY_CHANGED".into());
            }
            if reverse && (policy.get("version").and_then(serde_json::Value::as_i64) != Some(1)
                || policy.get("strictSingleSubmit").and_then(serde_json::Value::as_bool) != Some(true)
                || policy.get("attachmentLimitProfile").and_then(serde_json::Value::as_str) != Some("hybrid-v1")
                || policy.get("expectedConversationId").and_then(serde_json::Value::as_str) != endpoint_external_id(&tx,&row.2).ok().as_deref()) {
                return Err("RELAY_POLICY_CHANGED".into());
            }
            let (current_root_revision, current_policy_json): (i64, String) = tx.query_row(
                "SELECT r.revision,r.policy_json FROM project_execution_roots r JOIN workstreams w ON w.project_id=r.project_id WHERE w.id=?1",
                params![row.0],
                |record| Ok((record.get(0)?, record.get(1)?)),
            ).optional().map_err(db_error)?.ok_or("RELAY_POLICY_CHANGED")?;
            let current_policy: serde_json::Value = serde_json::from_str(&current_policy_json).map_err(|_| "RELAY_POLICY_CHANGED")?;
            if forward && (policy.get("rootRevision").and_then(serde_json::Value::as_i64) != Some(row.4)
                || policy.get("policy") != Some(&current_policy)) {
                return Err("RELAY_POLICY_CHANGED".into());
            }
            if forward && current_root_revision != row.4 { return Err("RELAY_POLICY_CHANGED".into()); }
            let browser_endpoint = if forward { &row.1 } else { &row.2 };
            let settings: i64 = tx.query_row("SELECT settings_revision FROM browser_endpoint_settings WHERE endpoint_id=?1 AND owner_principal_key=?2",params![browser_endpoint,owner],|r|r.get(0)).optional().map_err(db_error)?.ok_or("BROWSER_SETTINGS_CHANGED")?;
            if settings != expected_settings { return Err("BROWSER_SETTINGS_CHANGED".into()); }
            let mut statement = tx.prepare("SELECT a.id,a.source_endpoint_id,a.source_run_id,a.source_message_ref,a.source_artifact_ref,a.kind,a.filename,a.mime,a.size,a.source_declared_sha256,a.transfer_declared_sha256,a.actual_sha256,a.source_hash_status,a.integrity_status,a.storage_ref,a.state,a.created_at,a.updated_at,x.target_relative_path FROM handoff_attachment_details x JOIN artifact_materializations a ON a.id=x.artifact_id WHERE x.handoff_id=?1 ORDER BY x.ordinal").map_err(db_error)?;
            let artifacts = statement.query_map(params![handoff_id],|record| Ok((relay_artifact_row(record)?,record.get::<_,Option<String>>(18)?.ok_or(rusqlite::Error::InvalidQuery)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            drop(statement);
            let handoff = handoff_by_id(&tx, handoff_id)?;
            let source_external = endpoint_external_id(&tx, &row.1)?;
            let destination_external = endpoint_external_id(&tx, &row.2)?;
            let computed_payload = relay_envelope_hash(&row.1,&source_external,&row.5,&row.6,&row.2,&destination_external,&row.0,row.3,row.4,&row.7,&row.8,expected_draft_revision,&handoff.approved_text,&artifacts);
            let computed_manifest = relay_manifest_hash(&artifacts);
            if artifacts.iter().any(|(a,_)| a.state!="READY" || a.integrity_status!="VERIFIED" || a.actual_sha256.as_deref()!=Some(a.transfer_declared_sha256.as_str())) || computed_manifest != tx.query_row("SELECT manifest_hash FROM relay_handoff_details WHERE handoff_id=?1",params![handoff_id],|r|r.get::<_,String>(0)).map_err(db_error)? || computed_payload != handoff.payload_hash { return Err("RELAY_ARTIFACT_CHANGED".into()); }
            tx.execute("UPDATE relay_handoff_details SET approved_principal_key=?2,consumed_at=?3,updated_at=?3 WHERE handoff_id=?1",params![handoff_id,owner,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;
            Ok(())
        })?;
        self.relay_review(&owner, handoff_id)
    }

    /// Recovers the exact durable outcome of a relay approval when the HTTP
    /// response was lost.  The matching owner, consumed nonce, draft revision,
    /// and payload are all required.  If a process died after consumption but
    /// before the original claim, the same approval may finish *that* claim;
    /// it never creates a second approval or replacement request identity.
    pub fn relay_approval_readback(
        &self,
        owner: &str,
        handoff_id: &str,
        expected_draft_revision: i64,
        nonce_hash: &str,
    ) -> Result<Option<RelayApprovalReadback>, String> {
        let owner = nonempty(owner, "Relay owner")?;
        let nonce_hash = sha256(nonce_hash, "Relay review nonce")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            connection
                .query_row(
                    "SELECT h.id,h.status,p.id,p.status,x.dispatch_id,x.phase,x.last_code FROM relay_handoff_details d JOIN handoffs h ON h.id=d.handoff_id JOIN handoff_dispatches x ON x.handoff_id=h.id JOIN provider_runs p ON p.id=x.run_id WHERE d.handoff_id=?1 AND d.owner_principal_key=?2 AND d.draft_revision=?3 AND d.review_nonce_hash=?4 AND d.consumed_at IS NOT NULL",
                    params![handoff_id, owner, expected_draft_revision, nonce_hash],
                    |row| Ok(RelayApprovalReadback {
                        handoff_id: row.get(0)?, handoff_status: row.get(1)?, run_id: row.get(2)?, run_status: row.get(3)?, dispatch_id: row.get(4)?, dispatch_phase: row.get(5)?, last_code: row.get(6)?,
                    }),
                )
                .optional()
                .map_err(db_error)
        })
    }

    pub fn resume_or_claim_relay_approval(
        &self,
        owner: &str,
        handoff_id: &str,
        expected_draft_revision: i64,
        nonce_hash: &str,
        adapter_epoch: &str,
    ) -> Result<(RelayReview, Option<RelayDispatchSnapshot>, bool), String> {
        Uuid::parse_str(adapter_epoch).map_err(|_| "INVALID_ADAPTER_EPOCH")?;
        let owner = nonempty(owner, "Relay owner")?;
        let nonce_hash = sha256(nonce_hash, "Relay review nonce")?;
        let replay = self.with_connection(|connection| {
            require_relay_schema(connection)?;
            connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM relay_handoff_details d JOIN handoffs h ON h.id=d.handoff_id WHERE d.handoff_id=?1 AND d.owner_principal_key=?2 AND d.draft_revision=?3 AND d.review_nonce_hash=?4 AND d.consumed_at IS NOT NULL AND h.payload_hash IS NOT NULL)",
                    params![handoff_id, owner, expected_draft_revision, nonce_hash],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(db_error)
        })?;
        if replay {
            let existing = self.with_connection(|connection| {
                require_relay_schema(connection)?;
                connection
                    .query_row(
                        "SELECT run_id,dispatch_id FROM handoff_dispatches WHERE handoff_id=?1",
                        params![handoff_id],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                    )
                    .optional()
                    .map_err(db_error)
            })?;
            let (snapshot, newly_claimed) = match existing {
                // Historical response-loss recovery is a durable readback,
                // not a second live preflight.  The Review route returns
                // `relay_approval_readback` for this case.
                Some(_) => (None, false),
                // A legacy/separate approval-and-claim crash can only resume
                // this exact consumed approval.  claim_approved_relay uses the
                // same handoff row and cannot mint a duplicate dispatch.
                None => (
                    Some(self.claim_approved_relay(&owner, handoff_id, adapter_epoch)?),
                    true,
                ),
            };
            return Ok((
                self.relay_review(&owner, handoff_id)?,
                snapshot,
                newly_claimed,
            ));
        }
        let approval =
            self.approve_relay_draft(&owner, handoff_id, expected_draft_revision, &nonce_hash)?;
        let snapshot = self.claim_approved_relay(&owner, handoff_id, adapter_epoch)?;
        Ok((approval, Some(snapshot), true))
    }

    /// Resume the exact approval that was durably consumed before any dispatch
    /// row could be created.  This is crash recovery, not a new approval: the
    /// original immutable handoff must still be READY and have no dispatch.
    pub fn resume_consumed_relay_approval(
        &self,
        owner: &str,
        handoff_id: &str,
        adapter_epoch: &str,
    ) -> Result<RelayDispatchSnapshot, String> {
        Uuid::parse_str(adapter_epoch).map_err(|_| "INVALID_ADAPTER_EPOCH")?;
        let owner = nonempty(owner, "Relay owner")?;
        self.claim_approved_relay_inner(&owner, handoff_id, adapter_epoch, true)
    }

    /// A consumed approval can be resumed only when the first process stopped
    /// before it allocated any run or dispatch. This is exact recovery, never
    /// a second approval or replacement relay.
    pub fn consumed_relay_approval_recovery_available(
        &self,
        owner: &str,
        handoff_id: &str,
    ) -> Result<bool, String> {
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM relay_handoff_details d JOIN handoffs h ON h.id=d.handoff_id WHERE d.handoff_id=?1 AND d.owner_principal_key=?2 AND d.consumed_at IS NOT NULL AND h.status='READY' AND NOT EXISTS(SELECT 1 FROM handoff_dispatches x WHERE x.handoff_id=d.handoff_id))",
                params![handoff_id, owner],
                |row| row.get(0),
            ).map_err(db_error)
        })
    }

    /// Reports whether a different unresolved dispatch owns this handoff's
    /// Workstream. This is a read-only projection for Review: it does not
    /// weaken the durable lock or infer that an UNKNOWN write was not accepted.
    pub fn relay_workstream_writer_busy(
        &self,
        owner: &str,
        handoff_id: &str,
    ) -> Result<bool, String> {
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM relay_handoff_details d JOIN handoffs h ON h.id=d.handoff_id JOIN handoff_dispatches x ON x.workstream_id=h.workstream_id WHERE d.handoff_id=?1 AND d.owner_principal_key=?2 AND x.handoff_id<>h.id AND x.phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN'))",
                    params![handoff_id, owner],
                    |row| row.get(0),
                )
                .map_err(db_error)
        })
    }

    /// Consumes an already sealed Endpoint-source relay exactly once.  This is
    /// deliberately the same `handoff_dispatches`/`provider_runs` claim used
    /// by ControlContext work: there is no browser-specific queue or retry
    /// record.  A caller that loses its process after this succeeds must keep
    /// the returned dispatch and resolve UNKNOWN; it may not call this again
    /// to obtain a new request identity.
    pub fn claim_approved_relay(
        &self,
        owner: &str,
        handoff_id: &str,
        adapter_epoch: &str,
    ) -> Result<RelayDispatchSnapshot, String> {
        Uuid::parse_str(adapter_epoch).map_err(|_| "INVALID_ADAPTER_EPOCH")?;
        let owner = nonempty(owner, "Relay owner")?;
        self.claim_approved_relay_inner(&owner, handoff_id, adapter_epoch, false)
    }

    fn claim_approved_relay_inner(
        &self,
        owner: &str,
        handoff_id: &str,
        adapter_epoch: &str,
        recovery_only: bool,
    ) -> Result<RelayDispatchSnapshot, String> {
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let existing: Option<(String, String)> = tx
                .query_row(
                    "SELECT x.run_id,x.dispatch_id FROM handoff_dispatches x JOIN relay_handoff_details d ON d.handoff_id=x.handoff_id WHERE x.handoff_id=?1 AND d.owner_principal_key=?2 AND d.consumed_at IS NOT NULL",
                    params![handoff_id, owner],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db_error)?;
            let (run_id, dispatch_id) = if let Some(ids) = existing {
                if recovery_only {
                    return Err("RELAY_RECOVERY_UNAVAILABLE".into());
                }
                ids
            } else {
                let row: (String, String, String, String, i64, i64, String, String, String) = tx
                    .query_row(
                        "SELECT h.workstream_id,h.source_endpoint_id,h.destination_endpoint_id,h.payload_hash,d.binding_revision,d.root_revision,d.destination_policy_json,d.destination_policy_hash,h.status FROM handoffs h JOIN relay_handoff_details d ON d.handoff_id=h.id WHERE h.id=?1 AND d.owner_principal_key=?2 AND d.consumed_at IS NOT NULL",
                        params![handoff_id, owner],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
                    )
                    .optional()
                    .map_err(db_error)?
                    .ok_or(if recovery_only {
                        "RELAY_RECOVERY_UNAVAILABLE"
                    } else {
                        "RELAY_APPROVAL_REQUIRED"
                    })?;
                if row.8 != "READY" {
                    return Err(if recovery_only {
                        "RELAY_RECOVERY_UNAVAILABLE".into()
                    } else {
                        "DISPATCH_ALREADY_ATTEMPTED".into()
                    });
                }
                let binding: i64 = tx.query_row("SELECT binding_revision FROM workstreams WHERE id=?1 AND status='ACTIVE' AND trashed_at IS NULL", params![row.0], |r| r.get(0)).map_err(db_error)?;
                if binding != row.4 { return Err("RELAY_BINDING_CHANGED".into()); }
                let destination: (String, String) = tx.query_row("SELECT provider,status FROM endpoints WHERE id=?1", params![row.2], |r| Ok((r.get(0)?,r.get(1)?))).map_err(db_error)?;
                if destination.1 != "ACTIVE" || !matches!(destination.0.as_str(), "CODEX" | "CHATGPT") { return Err("RELAY_ENDPOINT_CHANGED".into()); }
                // The durable Workstream lock is the final backstop, but an
                // explicit preflight keeps this ordinary, recoverable writer
                // conflict out of the generic SQLite
                // error path. Any unresolved dispatch owns the complete
                // Workstream, regardless of destination or observed code.
                let workstream_busy: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM handoff_dispatches WHERE workstream_id=?1 AND phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN'))",
                    params![row.0],
                    |r| r.get(0),
                ).map_err(db_error)?;
                if workstream_busy {
                    return Err("RELAY_WORKSTREAM_BUSY".into());
                }
                let exiting: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM runtime_exit_state WHERE phase!='RUNNING')", [], |r| r.get(0)).map_err(db_error)?;
                if exiting { return Err("EXIT_PENDING".into()); }
                let run = id();
                let dispatch = id();
                let timestamp = now();
                tx.execute("INSERT INTO provider_runs(id,workstream_id,endpoint_id,provider,origin_handoff_id,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,'STARTING',?6,?6)", params![run,row.0,row.2,destination.0,handoff_id,timestamp]).map_err(db_error)?;
                tx.execute("INSERT INTO handoff_dispatches(handoff_id,run_id,workstream_id,endpoint_id,dispatch_id,adapter_epoch,phase,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,'CLAIMED',?7,?7)", params![handoff_id,run,row.0,row.2,dispatch,adapter_epoch,timestamp]).map_err(db_error)?;
                tx.execute("INSERT INTO provider_result_details(run_id,result_state,updated_at) VALUES(?1,'PENDING',?2)", params![run,timestamp]).map_err(db_error)?;
                tx.execute("UPDATE handoffs SET status='SENDING' WHERE id=?1 AND status='READY'", params![handoff_id]).map_err(db_error)?;
                (run, dispatch)
            };
            let snapshot = relay_dispatch_snapshot(&tx, &owner, handoff_id, &run_id, &dispatch_id)?;
            tx.commit().map_err(db_error)?;
            Ok(snapshot)
        })
    }

    /// Rebuilds the current sealed delivery view before every local copy or
    /// Provider write.  It intentionally fails after any stale endpoint,
    /// revision, policy, manifest, or artifact-byte change.
    pub fn relay_dispatch_snapshot(
        &self,
        owner: &str,
        handoff_id: &str,
        run_id: &str,
        dispatch_id: &str,
    ) -> Result<RelayDispatchSnapshot, String> {
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|connection| {
            require_relay_schema(connection)?;
            relay_dispatch_snapshot(connection, &owner, handoff_id, run_id, dispatch_id)
        })
    }

    /// Browser writes use the same durable phases as native writes.  The
    /// request identity is the already allocated dispatch UUID, so a restart
    /// has no avenue to mint a replacement request.
    pub fn begin_relay_browser_write(
        &self,
        owner: &str,
        handoff_id: &str,
        run_id: &str,
        dispatch_id: &str,
    ) -> Result<RelayDispatchSnapshot, String> {
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|c| {
            require_relay_schema(c)?;
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let snapshot = relay_dispatch_snapshot(&tx, &owner, handoff_id, run_id, dispatch_id)?;
            if snapshot.direction != "CODEX_TO_CHATGPT" || snapshot.destination_provider != "CHATGPT" {
                return Err("RELAY_DESTINATION_INVALID".into());
            }
            let changed=tx.execute("UPDATE handoff_dispatches SET phase='PREPARING',revision=revision+1,updated_at=?2 WHERE dispatch_id=?1 AND phase='CLAIMED'",params![dispatch_id,now()]).map_err(db_error)?;
            if changed!=1 { return Err("DISPATCH_ALREADY_ATTEMPTED".into()); }
            tx.commit().map_err(db_error)?;
            Ok(snapshot)
        })
    }

    pub fn begin_relay_native_preparation(
        &self,
        owner: &str,
        handoff_id: &str,
        run_id: &str,
        dispatch_id: &str,
        epoch: &str,
    ) -> Result<RelayDispatchSnapshot, String> {
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|c| {
            require_relay_schema(c)?;
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let snapshot=relay_dispatch_snapshot(&tx,&owner,handoff_id,run_id,dispatch_id)?;
            if snapshot.direction!="CHATGPT_TO_CODEX" || snapshot.destination_provider!="CODEX" { return Err("RELAY_DESTINATION_INVALID".into()); }
            let changed=tx.execute("UPDATE handoff_dispatches SET phase='PREPARING',revision=revision+1,updated_at=?3 WHERE dispatch_id=?1 AND adapter_epoch=?2 AND phase='CLAIMED'",params![dispatch_id,epoch,now()]).map_err(db_error)?;
            if changed!=1 { return Err("DISPATCH_ALREADY_ATTEMPTED".into()); }
            tx.commit().map_err(db_error)?;
            Ok(snapshot)
        })
    }

    pub fn persist_relay_native_write_intent(
        &self,
        owner: &str,
        handoff_id: &str,
        run_id: &str,
        dispatch_id: &str,
        epoch: &str,
        request_id: &Value,
    ) -> Result<(), String> {
        if !(request_id.as_u64().is_some()
            || request_id
                .as_str()
                .is_some_and(|value| !value.is_empty() && value.len() <= 128))
        {
            return Err("INVALID_NATIVE_REQUEST_ID".into());
        }
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|c| {
            require_relay_schema(c)?;
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let _=relay_dispatch_snapshot(&tx,&owner,handoff_id,run_id,dispatch_id)?;
            let changed=tx.execute("UPDATE handoff_dispatches SET phase='WRITE_INTENT',native_request_id_json=?3,write_intent_at=?4,revision=revision+1,updated_at=?4 WHERE dispatch_id=?1 AND adapter_epoch=?2 AND phase='PREPARING' AND native_request_id_json IS NULL",params![dispatch_id,epoch,request_id.to_string(),now()]).map_err(db_error)?;
            if changed!=1 { return Err("DISPATCH_ALREADY_ATTEMPTED".into()); }
            tx.commit().map_err(db_error)
        })
    }

    pub fn persist_relay_browser_write_intent(
        &self,
        owner: &str,
        handoff_id: &str,
        run_id: &str,
        dispatch_id: &str,
    ) -> Result<RelayDispatchSnapshot, String> {
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|c| {
            require_relay_schema(c)?;
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let snapshot = relay_dispatch_snapshot(&tx, &owner, handoff_id, run_id, dispatch_id)?;
            let changed=tx.execute("UPDATE handoff_dispatches SET phase='WRITE_INTENT',native_request_id_json=?2,write_intent_at=?3,revision=revision+1,updated_at=?3 WHERE dispatch_id=?1 AND phase='PREPARING' AND native_request_id_json IS NULL",params![dispatch_id,serde_json::Value::String(dispatch_id.into()).to_string(),now()]).map_err(db_error)?;
            if changed!=1 { return Err("DISPATCH_ALREADY_ATTEMPTED".into()); }
            tx.commit().map_err(db_error)?;
            Ok(snapshot)
        })
    }

    /// Completes an endpoint-source dispatch which has provably not crossed a
    /// physical writer boundary.  It deliberately does not rebuild a live
    /// relay snapshot: the original dispatch is retained as the audit record
    /// even when the current endpoint/policy/binding has moved on.
    pub fn mark_relay_prewrite_not_sent(
        &self,
        owner: &str,
        handoff_id: &str,
        run_id: &str,
        dispatch_id: &str,
        code: &str,
    ) -> Result<(), String> {
        if code.is_empty()
            || code.len() > 100
            || !code
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b == b'_' || b.is_ascii_digit())
        {
            return Err("INVALID_ERROR_CODE".into());
        }
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|c| {
            require_relay_schema(c)?;
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let owns_dispatch: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM handoff_dispatches x JOIN relay_handoff_details d ON d.handoff_id=x.handoff_id WHERE x.dispatch_id=?1 AND x.run_id=?2 AND x.handoff_id=?3 AND d.owner_principal_key=?4 AND d.consumed_at IS NOT NULL)",
                params![dispatch_id, run_id, handoff_id, owner], |row| row.get(0),
            ).map_err(db_error)?;
            if !owns_dispatch {
                return Err("RELAY_APPROVAL_REQUIRED".into());
            }
            let changed = tx.execute(
                "UPDATE handoff_dispatches SET phase='NOT_SENT',revision=revision+1,last_code=?2,updated_at=?3 WHERE dispatch_id=?1 AND phase IN ('CLAIMED','PREPARING','WRITE_INTENT')",
                params![dispatch_id, code, now()],
            ).map_err(db_error)?;
            if changed != 1 { return Err("DISPATCH_ALREADY_ATTEMPTED".into()); }
            tx.execute("UPDATE handoffs SET status='FAILED',failed_at=?2,error_code='PROVEN_NOT_SENT' WHERE id=?1",params![handoff_id,now()]).map_err(db_error)?;
            tx.execute("UPDATE provider_runs SET status='FAILED',terminal_code='PROVEN_NOT_SENT',terminal_at=?2,updated_at=?2 WHERE id=?1",params![run_id,now()]).map_err(db_error)?;
            tx.execute("UPDATE provider_result_details SET result_state='NOT_APPLICABLE',updated_at=?2 WHERE run_id=?1",params![run_id,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)
        })
    }

    /// Finishes an already sealed native prewrite stop for the original relay
    /// dispatch.  `record_prewrite_stop` deliberately moves the dispatch to
    /// UNKNOWN first so a crash between evidence persistence and this final
    /// projection cannot be mistaken for a provider write.  Once the signed
    /// receipt is readable and there is still no write intent, however, the
    /// same original record is provably NOT_SENT and must not keep its
    /// workstream locked awaiting an operator decision.
    pub fn complete_relay_sealed_prewrite_stop(
        &self,
        owner: &str,
        handoff_id: &str,
        run_id: &str,
        dispatch_id: &str,
        key: &[u8; 32],
        code: &str,
    ) -> Result<(), String> {
        if code.is_empty()
            || code.len() > 100
            || !code
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b == b'_' || b.is_ascii_digit())
        {
            return Err("INVALID_ERROR_CODE".into());
        }
        let owner = nonempty(owner, "Relay owner")?;
        let receipts = self.browser_evidence(&owner, dispatch_id, key)?;
        let prewrite = receipts.iter().any(|receipt| {
            receipt.get("kind").and_then(Value::as_str) == Some("PREWRITE_STOP")
                && receipt.get("verified").and_then(Value::as_bool) == Some(true)
        });
        let conflicting = receipts.iter().any(|receipt| {
            matches!(
                receipt.get("kind").and_then(Value::as_str),
                Some("ACK_LINK" | "TERMINAL_LINK" | "CONFLICT")
            )
        });
        if !prewrite || conflicting {
            return Err("PREWRITE_PROOF_UNAVAILABLE".into());
        }
        self.with_connection(|c| {
            require_relay_schema(c)?;
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let owns_dispatch: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM handoff_dispatches x JOIN relay_handoff_details d ON d.handoff_id=x.handoff_id WHERE x.dispatch_id=?1 AND x.run_id=?2 AND x.handoff_id=?3 AND d.owner_principal_key=?4 AND d.consumed_at IS NOT NULL)",
                    params![dispatch_id, run_id, handoff_id, owner],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            if !owns_dispatch {
                return Err("RELAY_APPROVAL_REQUIRED".into());
            }
            let changed = tx
                .execute(
                    "UPDATE handoff_dispatches SET phase='NOT_SENT',revision=revision+1,last_code=?2,updated_at=?3 WHERE dispatch_id=?1 AND phase='UNKNOWN' AND write_intent_at IS NULL",
                    params![dispatch_id, code, now()],
                )
                .map_err(db_error)?;
            if changed != 1 {
                return Err("DISPATCH_ALREADY_ATTEMPTED".into());
            }
            tx.execute(
                "UPDATE handoffs SET status='FAILED',failed_at=?2,error_code='PROVEN_NOT_SENT' WHERE id=?1",
                params![handoff_id, now()],
            )
            .map_err(db_error)?;
            tx.execute(
                "UPDATE provider_runs SET status='FAILED',terminal_code='PROVEN_NOT_SENT',terminal_at=?2,updated_at=?2 WHERE id=?1",
                params![run_id, now()],
            )
            .map_err(db_error)?;
            tx.execute(
                "UPDATE provider_result_details SET result_state='NOT_APPLICABLE',updated_at=?2 WHERE run_id=?1",
                params![run_id, now()],
            )
            .map_err(db_error)?;
            tx.commit().map_err(db_error)
        })
    }

    /// Persists only bounded, MACed bridge facts.  This is intentionally a
    /// different evidence family from native stdio receipts: mixing either
    /// table would allow a browser observation to impersonate a Codex ACK.
    pub fn record_bridge_dispatch_evidence(
        &self,
        owner: &str,
        handoff_id: &str,
        run_id: &str,
        dispatch_id: &str,
        mut evidence: BridgeDispatchEvidence,
        key: &[u8; 32],
    ) -> Result<BridgeDispatchEvidence, String> {
        let owner = nonempty(owner, "Relay owner")?;
        self.with_connection(|c| {
            require_relay_schema(c)?;
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let snapshot=relay_dispatch_snapshot(&tx,&owner,handoff_id,run_id,dispatch_id)?;
            if snapshot.direction!="CODEX_TO_CHATGPT" || snapshot.destination_provider!="CHATGPT" { return Err("RELAY_DESTINATION_INVALID".into()); }
            let epoch: String=tx.query_row("SELECT adapter_epoch FROM handoff_dispatches WHERE dispatch_id=?1",params![dispatch_id],|r|r.get(0)).map_err(db_error)?;
            if !matches!(evidence.kind.as_str(),"USER_ACCEPTED"|"ASSISTANT_TERMINAL"|"PREWRITE_STOP"|"CONFLICT")
                || evidence.dispatch_id!=dispatch_id || evidence.endpoint_id!=snapshot.destination_endpoint_id
                || evidence.conversation_id!=snapshot.destination_external_id || evidence.request_id!=dispatch_id
                || evidence.adapter_epoch!=epoch || evidence.payload_hash!=snapshot.payload_hash
                || evidence.manifest_hash!=snapshot.manifest_hash || evidence.source_message_id.is_empty()
                || evidence.bundle_identity.len()!=64 || !evidence.bundle_identity.bytes().all(|b|b.is_ascii_hexdigit()) { return Err("EVIDENCE_CONFLICT".into()); }
            let prewrite_code = if evidence.kind == "PREWRITE_STOP" {
                if evidence.turn_key.is_some() {
                    return Err("EVIDENCE_CONFLICT".into());
                }
                let generic_marker = format!("prewrite:{dispatch_id}");
                if evidence.source_message_id == generic_marker {
                    "BRIDGE_PREWRITE_STOP".to_owned()
                } else {
                    let prefix = format!("{generic_marker}:");
                    match evidence.source_message_id.strip_prefix(&prefix) {
                        Some(
                            "HUMAN_DRAFT_PRESENT"
                            | "PASSIVE_SUBMIT_PREWRITE_BLOCKED"
                            | "RELAY_ATTACHMENT_NOT_READY",
                        ) => evidence.source_message_id[prefix.len()..].to_owned(),
                        _ => return Err("EVIDENCE_CONFLICT".into()),
                    }
                }
            } else {
                String::new()
            };
            evidence.recorded_at=now();
            if evidence.id.is_empty() { evidence.id=id(); }
            seal_bridge_evidence(&mut evidence,key)?;
            if let Some(existing)=tx.query_row("SELECT id,dispatch_id,kind,endpoint_id,conversation_id,source_message_id,request_id,turn_key,adapter_epoch,bundle_identity,payload_hash,manifest_hash,evidence_digest,integrity_mac,recorded_at FROM bridge_dispatch_evidence WHERE dispatch_id=?1 AND kind=?2 AND source_message_id=?3 AND request_id=?4",params![dispatch_id,evidence.kind,evidence.source_message_id,evidence.request_id],|r|Ok(BridgeDispatchEvidence{id:r.get(0)?,dispatch_id:r.get(1)?,kind:r.get(2)?,endpoint_id:r.get(3)?,conversation_id:r.get(4)?,source_message_id:r.get(5)?,request_id:r.get(6)?,turn_key:r.get(7)?,adapter_epoch:r.get(8)?,bundle_identity:r.get(9)?,payload_hash:r.get(10)?,manifest_hash:r.get(11)?,evidence_digest:r.get(12)?,integrity_mac:r.get(13)?,recorded_at:r.get(14)?})).optional().map_err(db_error)? {
                valid_bridge_evidence(&existing,key)?;
                if existing.kind==evidence.kind && existing.endpoint_id==evidence.endpoint_id && existing.conversation_id==evidence.conversation_id && existing.turn_key==evidence.turn_key && existing.payload_hash==evidence.payload_hash && existing.manifest_hash==evidence.manifest_hash { return Ok(existing); }
                return Err("EVIDENCE_CONFLICT".into());
            }
            tx.execute("INSERT INTO bridge_dispatch_evidence(id,dispatch_id,kind,endpoint_id,conversation_id,source_message_id,request_id,turn_key,adapter_epoch,bundle_identity,payload_hash,manifest_hash,evidence_digest,integrity_mac,recorded_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",params![evidence.id,evidence.dispatch_id,evidence.kind,evidence.endpoint_id,evidence.conversation_id,evidence.source_message_id,evidence.request_id,evidence.turn_key,evidence.adapter_epoch,evidence.bundle_identity,evidence.payload_hash,evidence.manifest_hash,evidence.evidence_digest,evidence.integrity_mac,evidence.recorded_at]).map_err(db_error)?;
            match evidence.kind.as_str() {
                "USER_ACCEPTED" => {
                    let changed=tx.execute("UPDATE handoff_dispatches SET phase='ACCEPTED',revision=revision+1,updated_at=?2 WHERE dispatch_id=?1 AND phase='WRITE_INTENT'",params![dispatch_id,now()]).map_err(db_error)?;
                    if changed!=1 { return Err("EVIDENCE_CONFLICT".into()); }
                    tx.execute("UPDATE handoffs SET status='SENT',sent_at=?2 WHERE id=?1",params![handoff_id,now()]).map_err(db_error)?;
                    tx.execute("UPDATE provider_runs SET status='RUNNING',external_run_id=?2,started_at=?3,updated_at=?3 WHERE id=?1",params![run_id,evidence.turn_key,now()]).map_err(db_error)?;
                }
                "ASSISTANT_TERMINAL" => {
                    let accepted:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bridge_dispatch_evidence WHERE dispatch_id=?1 AND kind='USER_ACCEPTED')",params![dispatch_id],|r|r.get(0)).map_err(db_error)?;
                    if !accepted { return Err("EVIDENCE_CONFLICT".into()); }
                    // A process restart may conservatively mark an already
                    // accepted browser dispatch UNKNOWN before its exact
                    // terminal journal entry is read.  The existing
                    // USER_ACCEPTED evidence plus this separately exact
                    // assistant identity is sufficient to recover only that
                    // same original dispatch; it never authorizes a resend.
                    let changed=tx.execute("UPDATE handoff_dispatches SET phase='TERMINAL',revision=revision+1,updated_at=?2 WHERE dispatch_id=?1 AND phase IN ('ACCEPTED','UNKNOWN')",params![dispatch_id,now()]).map_err(db_error)?;
                    if changed!=1 { return Err("EVIDENCE_CONFLICT".into()); }
                    tx.execute("UPDATE provider_runs SET status='COMPLETED',terminal_at=?2,updated_at=?2 WHERE id=?1",params![run_id,now()]).map_err(db_error)?;
                    tx.execute("UPDATE provider_result_details SET result_state='UNAVAILABLE',bridge_evidence_id=?2,terminal_source='BRIDGE_TERMINAL',updated_at=?3 WHERE run_id=?1",params![run_id,evidence.id,now()]).map_err(db_error)?;
                }
                "PREWRITE_STOP" => {
                    // A typed Bridge prewrite proof means that the physical
                    // browser submit did not begin.  It is therefore not an
                    // ambiguous write: retain the original dispatch as the
                    // audit identity, release the workstream through its
                    // existing NOT_SENT semantics, and never manufacture a
                    // provider turn/message identity.
                    let changed=tx.execute("UPDATE handoff_dispatches SET phase='NOT_SENT',revision=revision+1,last_code=?2,updated_at=?3 WHERE dispatch_id=?1 AND phase IN ('PREPARING','WRITE_INTENT')",params![dispatch_id,prewrite_code,now()]).map_err(db_error)?;
                    if changed!=1 { return Err("EVIDENCE_CONFLICT".into()); }
                    tx.execute("UPDATE handoffs SET status='FAILED',failed_at=?2,error_code='PROVEN_NOT_SENT' WHERE id=?1",params![handoff_id,now()]).map_err(db_error)?;
                    tx.execute("UPDATE provider_runs SET status='FAILED',terminal_code='PROVEN_NOT_SENT',terminal_at=?2,updated_at=?2 WHERE id=?1",params![run_id,now()]).map_err(db_error)?;
                    tx.execute("UPDATE provider_result_details SET result_state='NOT_APPLICABLE',updated_at=?2 WHERE run_id=?1",params![run_id,now()]).map_err(db_error)?;
                }
                _ => {
                    tx.execute("UPDATE handoff_dispatches SET phase='UNKNOWN',revision=revision+1,last_code='BRIDGE_EVIDENCE_CONFLICT',updated_at=?2 WHERE dispatch_id=?1",params![dispatch_id,now()]).map_err(db_error)?;
                    tx.execute("UPDATE provider_runs SET status='UNKNOWN',terminal_code='BRIDGE_EVIDENCE_CONFLICT',updated_at=?2 WHERE id=?1",params![run_id,now()]).map_err(db_error)?;
                }
            }
            tx.commit().map_err(db_error)?;
            Ok(evidence)
        })
    }
}

fn relay_dispatch_snapshot(
    c: &Connection,
    owner: &str,
    handoff_id: &str,
    run_id: &str,
    dispatch_id: &str,
) -> Result<RelayDispatchSnapshot, String> {
    relay_dispatch_snapshot_with_history(c, owner, handoff_id, run_id, dispatch_id, false)
}

/// A completed Codex output is read from the original, sealed forward
/// dispatch.  It never reopens that dispatch or permits a provider write;
/// therefore a newer endpoint binding must not invalidate this local read.
fn historical_output_snapshot(
    c: &Connection,
    owner: &str,
    handoff_id: &str,
    run_id: &str,
    dispatch_id: &str,
) -> Result<RelayDispatchSnapshot, String> {
    relay_dispatch_snapshot_with_history(c, owner, handoff_id, run_id, dispatch_id, true)
}

fn relay_dispatch_snapshot_with_history(
    c: &Connection,
    owner: &str,
    handoff_id: &str,
    run_id: &str,
    dispatch_id: &str,
    historical_output: bool,
) -> Result<RelayDispatchSnapshot, String> {
    let row: (String,String,String,String,String,String,String,i64,i64,String,String,String,String,Option<i64>) = c.query_row(
        "SELECT h.workstream_id,h.source_endpoint_id,h.destination_endpoint_id,h.direction,h.approved_text,h.payload_hash,d.source_message_ref,d.binding_revision,d.root_revision,d.destination_policy_json,d.destination_policy_hash,d.manifest_hash,d.owner_principal_key,d.consumed_at FROM handoffs h JOIN relay_handoff_details d ON d.handoff_id=h.id WHERE h.id=?1",
        params![handoff_id],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?,r.get(11)?,r.get(12)?,r.get(13)?)),
    ).optional().map_err(db_error)?.ok_or("NOT_FOUND")?;
    if row.12 != owner || row.13.is_none() {
        return Err("RELAY_APPROVAL_REQUIRED".into());
    }
    let (stored_run, stored_dispatch, phase): (String, String, String) = c
        .query_row(
            "SELECT run_id,dispatch_id,phase FROM handoff_dispatches WHERE handoff_id=?1",
            params![handoff_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(db_error)?;
    if stored_run != run_id
        || stored_dispatch != dispatch_id
        || !matches!(
            phase.as_str(),
            "CLAIMED"
                | "PREPARING"
                | "WRITE_INTENT"
                | "ACCEPTED"
                | "UNKNOWN"
                | "TERMINAL"
                | "NOT_SENT"
        )
    {
        return Err("DISPATCH_ALREADY_ATTEMPTED".into());
    }
    let binding: i64 = c.query_row("SELECT binding_revision FROM workstreams WHERE id=?1 AND status='ACTIVE' AND trashed_at IS NULL",params![row.0],|r|r.get(0)).map_err(db_error)?;
    if !historical_output && binding != row.7 {
        return Err("RELAY_BINDING_CHANGED".into());
    }
    let destination = endpoint_by_id(c, &row.2)?;
    if (!historical_output && destination.status != "ACTIVE")
        || !matches!(destination.provider.as_str(), "CODEX" | "CHATGPT")
    {
        return Err("RELAY_ENDPOINT_CHANGED".into());
    }
    let source = endpoint_by_id(c, &row.1)?;
    if !historical_output && source.status != "ACTIVE" {
        return Err("RELAY_ENDPOINT_CHANGED".into());
    }
    let policy: Value = serde_json::from_str(&row.9).map_err(|_| "RELAY_POLICY_INVALID")?;
    if policy.get("destinationEndpointId").and_then(Value::as_str) != Some(destination.id.as_str())
    {
        return Err("RELAY_POLICY_CHANGED".into());
    }
    let (canonical_path, current_root_revision, current_policy_json): (String, i64, String) = c
        .query_row(
            "SELECT r.canonical_path,r.revision,r.policy_json FROM project_execution_roots r JOIN workstreams w ON w.project_id=r.project_id WHERE w.id=?1",
            params![row.0],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(db_error)?
        .ok_or("RELAY_POLICY_CHANGED")?;
    let native_policy: super::control::PolicySnapshot =
        serde_json::from_str(&current_policy_json).map_err(|_| "RELAY_POLICY_CHANGED")?;
    let codex_delivery = row.3 == "CHATGPT_TO_CODEX" && destination.provider == "CODEX";
    let browser_delivery = row.3 == "CODEX_TO_CHATGPT" && destination.provider == "CHATGPT";
    if !codex_delivery && !browser_delivery {
        return Err("RELAY_DESTINATION_INVALID".into());
    }
    if codex_delivery
        && !historical_output
        && (current_root_revision != row.8
            || policy.get("policy")
                != Some(&serde_json::to_value(&native_policy).map_err(|_| "RELAY_POLICY_CHANGED")?))
    {
        return Err("RELAY_POLICY_CHANGED".into());
    }
    if browser_delivery
        && (policy.get("version").and_then(Value::as_i64) != Some(1)
            || policy.get("strictSingleSubmit").and_then(Value::as_bool) != Some(true)
            || policy.get("attachmentLimitProfile").and_then(Value::as_str) != Some("hybrid-v1")
            || policy.get("expectedConversationId").and_then(Value::as_str)
                != Some(destination.external_id.as_str()))
    {
        return Err("RELAY_POLICY_CHANGED".into());
    }
    let mut stmt=c.prepare("SELECT a.id,a.source_endpoint_id,a.source_run_id,a.source_message_ref,a.source_artifact_ref,a.kind,a.filename,a.mime,a.size,a.source_declared_sha256,a.transfer_declared_sha256,a.actual_sha256,a.source_hash_status,a.integrity_status,a.storage_ref,a.state,a.created_at,a.updated_at,x.target_relative_path FROM handoff_attachment_details x JOIN artifact_materializations a ON a.id=x.artifact_id WHERE x.handoff_id=?1 ORDER BY x.ordinal").map_err(db_error)?;
    let attachments = stmt
        .query_map(params![handoff_id], |r| {
            Ok((relay_artifact_row(r)?, r.get::<_, String>(18)?))
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    let recomputed_manifest = relay_manifest_hash(&attachments);
    // capture_digest is deliberately included in the persisted payload. Load it
    // separately rather than accepting a browser supplied value.
    let capture: String = c
        .query_row(
            "SELECT capture_digest FROM relay_handoff_details WHERE handoff_id=?1",
            params![handoff_id],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    let draft: i64 = c
        .query_row(
            "SELECT draft_revision FROM relay_handoff_details WHERE handoff_id=?1",
            params![handoff_id],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    let payload = relay_envelope_hash(
        &source.id,
        &source.external_id,
        &row.6,
        &capture,
        &destination.id,
        &destination.external_id,
        &row.0,
        row.7,
        row.8,
        &row.9,
        &row.10,
        draft,
        &row.4,
        &attachments,
    );
    if recomputed_manifest != row.11
        || payload != row.5
        || attachments.iter().any(|(a, _)| {
            a.state != "READY"
                || a.integrity_status != "VERIFIED"
                || a.actual_sha256.as_deref() != Some(a.transfer_declared_sha256.as_str())
        })
    {
        return Err("RELAY_ARTIFACT_CHANGED".into());
    }
    Ok(RelayDispatchSnapshot {
        owner_principal_key: row.12,
        handoff_id: handoff_id.into(),
        run_id: run_id.into(),
        dispatch_id: dispatch_id.into(),
        workstream_id: row.0,
        source_endpoint_id: row.1,
        destination_endpoint_id: destination.id,
        destination_provider: destination.provider,
        destination_external_id: destination.external_id,
        direction: row.3,
        approved_text: row.4,
        payload_hash: row.5,
        manifest_hash: row.11,
        binding_revision: row.7,
        root_revision: row.8,
        canonical_path,
        policy: native_policy,
        destination_policy_json: row.9,
        destination_policy_hash: row.10,
        attachments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_packet_has_no_generated_outer_newline_before_approval() {
        let text = relay_input_text("approved Unicode 数据\nsecond line", &[], None);
        assert!(text.starts_with("approved Unicode 数据\nsecond line"));
        assert!(text.ends_with("No attachments."));
        assert!(!text.ends_with('\n'));
        let codex = relay_input_text("same body", &[], Some(".ai-work-router/handoffs/exact/out"));
        assert!(codex.ends_with('\n')); // preserve the native output contract
    }
    use crate::store::dispatch::PendingReverseRelay;

    #[test]
    fn forward_output_contract_is_sealed_outside_the_editable_body() {
        let text = relay_input_text(
            "read the selected inputs",
            &[],
            Some(".ai-work-router/handoffs/exact-handoff/out"),
        );
        assert!(text.starts_with("read the selected inputs"));
        assert!(text.contains(RELAY_MANIFEST_MARKER));
        assert!(text.contains(RELAY_OUTPUT_MARKER));
        assert!(text.contains(".ai-work-router/handoffs/exact-handoff/out/"));
        assert_eq!(editable_relay_body(&text), "read the selected inputs");
    }

    fn preview_store() -> (tempfile::TempDir, RouterStore) {
        let dir = tempfile::tempdir().unwrap();
        let profile =
            crate::runtime::PreviewProfile::acquire(dir.path().join("mcp-preview")).unwrap();
        RouterStore::initialize_preview(&profile).unwrap();
        (dir, RouterStore::open_preview(profile).unwrap())
    }

    fn endpoints(store: &RouterStore) -> (String, Endpoint, Endpoint) {
        let project = store.create_project("relay".into(), None).unwrap();
        let workstream = store
            .create_workstream(&project.id, "relay".into())
            .unwrap();
        let chat = store
            .bind_endpoint(
                &workstream.id,
                Provider::Chatgpt,
                "conversation".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let codex = store
            .bind_endpoint(
                &workstream.id,
                Provider::Codex,
                "thread".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        (workstream.id, chat, codex)
    }

    #[test]
    fn relay_artifact_only_handoff_is_owner_scoped_relative_and_ready() {
        let (_dir, store) = preview_store();
        let (workstream, chat, codex) = endpoints(&store);
        let settings = store
            .configure_browser_endpoint(BrowserEndpointSettingsInput {
                endpoint_id: chat.id.clone(),
                owner_principal_key: "owner".into(),
                observe_enabled: false,
                paired_by: "owner".into(),
            })
            .unwrap();
        let enabled = store
            .set_browser_observation(&chat.id, "owner", settings.settings_revision, true)
            .unwrap();
        assert!(enabled.observe_enabled);
        let digest = "a".repeat(64);
        let artifact = store
            .persist_relay_artifact(RelayArtifactInput {
                id: "artifact".into(),
                source_endpoint_id: chat.id.clone(),
                source_run_id: None,
                source_message_ref: "message".into(),
                source_artifact_ref: "file".into(),
                kind: "DOCUMENT".into(),
                filename: "proof.txt".into(),
                mime: "text/plain".into(),
                size: 4,
                source_declared_sha256: Some(digest.clone()),
                transfer_declared_sha256: digest.clone(),
                actual_sha256: Some(digest.clone()),
                source_hash_status: "DECLARED".into(),
                integrity_status: "VERIFIED".into(),
                storage_ref: "artifacts/opaque/proof.txt".into(),
                state: "READY".into(),
            })
            .unwrap();
        let handoff = store
            .create_ready_relay_handoff(NewRelayHandoff {
                workstream_id: workstream,
                source_endpoint_id: chat.id,
                source_run_id: None,
                source_message_ref: "message".into(),
                capture_digest: "capture".into(),
                source_completeness: "COMPLETE".into(),
                destination_endpoint_id: codex.id.clone(),
                direction: "CHATGPT_TO_CODEX".into(),
                original_text: "".into(),
                approved_text: "".into(),
                owner_principal_key: "owner".into(),
                binding_revision: 2,
                root_revision: 1,
                destination_policy_json: "{}".into(),
                destination_policy_hash: "policy".into(),
                draft_revision: 1,
                client_request_id: "request".into(),
                attachments: vec![RelayAttachmentSelection {
                    attachment_id: "attachment".into(),
                    artifact_id: artifact.id,
                    mime: "text/plain".into(),
                    approved_sha256: digest,
                    approved_size: 4,
                    manifest_entry_json: "{}".into(),
                    target_relative_path: Some(
                        ".ai-work-router/handoffs/relay/in/proof.txt".into(),
                    ),
                }],
            })
            .unwrap();
        assert_eq!(handoff.status, "READY");
        assert!(handoff.approved_text.contains(RELAY_MANIFEST_MARKER));
        assert_eq!(handoff.attachments[0].original_path, "artifact://artifact");
    }

    #[test]
    fn relay_still_rejects_a_contentless_handoff_without_attachments() {
        let (_dir, store) = preview_store();
        assert_eq!(
            store
                .create_ready_relay_handoff(NewRelayHandoff {
                    workstream_id: "workstream".into(),
                    source_endpoint_id: "source".into(),
                    source_run_id: None,
                    source_message_ref: "message".into(),
                    capture_digest: "capture".into(),
                    source_completeness: "COMPLETE".into(),
                    destination_endpoint_id: "destination".into(),
                    direction: "CHATGPT_TO_CODEX".into(),
                    original_text: "".into(),
                    approved_text: "".into(),
                    owner_principal_key: "owner".into(),
                    binding_revision: 1,
                    root_revision: 1,
                    destination_policy_json: "{}".into(),
                    destination_policy_hash: "policy".into(),
                    draft_revision: 1,
                    client_request_id: "request".into(),
                    attachments: vec![],
                })
                .unwrap_err(),
            "INVALID_RELAY_HANDOFF"
        );
    }

    #[test]
    fn relay_rejects_absolute_or_changed_artifacts_before_creating_a_handoff() {
        let (_dir, store) = preview_store();
        let (_workstream, chat, _codex) = endpoints(&store);
        let invalid = RelayArtifactInput {
            id: "artifact".into(),
            source_endpoint_id: chat.id,
            source_run_id: None,
            source_message_ref: "message".into(),
            source_artifact_ref: "file".into(),
            kind: "DOCUMENT".into(),
            filename: "proof.txt".into(),
            mime: "text/plain".into(),
            size: 1,
            source_declared_sha256: None,
            transfer_declared_sha256: "a".repeat(64),
            actual_sha256: Some("a".repeat(64)),
            source_hash_status: "UNAVAILABLE".into(),
            integrity_status: "VERIFIED".into(),
            storage_ref: "C:/outside".into(),
            state: "READY".into(),
        };
        assert_eq!(
            store.persist_relay_artifact(invalid).unwrap_err(),
            "Artifact storage reference must be a Router-relative reference"
        );
    }

    #[test]
    fn relay_approval_is_single_use_and_review_state_cannot_be_reopened() {
        let (_dir, store) = preview_store();
        let (workstream, chat, codex) = endpoints(&store);
        let source_endpoint_id = chat.id.clone();
        let root_policy = crate::store::control::PolicySnapshot {
            version: 1,
            backend_id: "00000000-0000-0000-0000-000000000001".into(),
            native_version: "0.154.0".into(),
            model: "fixture".into(),
            permission_mode: crate::store::control::PermissionMode::ReadOnly,
            approval_mode: crate::store::control::ApprovalMode::OnRequest,
            network_access: false,
            root_identity_hash: "d".repeat(64),
            config_digest: "e".repeat(64),
            native_launch_digest: None,
        };
        store
            .with_connection(|connection| {
                let project_id: String = connection.query_row(
                    "SELECT project_id FROM workstreams WHERE id=?1",
                    params![workstream],
                    |row| row.get(0),
                ).map_err(db_error)?;
                connection.execute(
                    "INSERT INTO project_execution_roots(project_id,canonical_path,path_identity_hash,revision,policy_json,policy_hash,updated_at) VALUES(?1,'C:/fixture',?2,1,?3,?4,1)",
                    params![project_id, root_policy.root_identity_hash, serde_json::to_string(&root_policy).map_err(|_| "INTERNAL")?, root_policy.hash()?],
                ).map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let settings = store
            .configure_browser_endpoint(BrowserEndpointSettingsInput {
                endpoint_id: chat.id.clone(),
                owner_principal_key: "owner".into(),
                observe_enabled: false,
                paired_by: "owner".into(),
            })
            .unwrap();
        let settings = store
            .set_browser_observation(&chat.id, "owner", settings.settings_revision, true)
            .unwrap();
        let digest = "b".repeat(64);
        let artifact = store
            .persist_relay_artifact(RelayArtifactInput {
                id: "artifact-approved".into(),
                source_endpoint_id: chat.id.clone(),
                source_run_id: None,
                source_message_ref: "message".into(),
                source_artifact_ref: "image".into(),
                kind: "IMAGE".into(),
                filename: "proof.png".into(),
                mime: "image/png".into(),
                size: 4,
                source_declared_sha256: Some(digest.clone()),
                transfer_declared_sha256: digest.clone(),
                actual_sha256: Some(digest.clone()),
                source_hash_status: "DECLARED".into(),
                integrity_status: "VERIFIED".into(),
                storage_ref: "artifacts/opaque/blob".into(),
                state: "READY".into(),
            })
            .unwrap();
        let handoff = store
            .create_ready_relay_handoff(NewRelayHandoff {
                workstream_id: workstream,
                source_endpoint_id: source_endpoint_id.clone(),
                source_run_id: None,
                source_message_ref: "message".into(),
                capture_digest: "capture".into(),
                source_completeness: "COMPLETE".into(),
                destination_endpoint_id: codex.id.clone(),
                direction: "CHATGPT_TO_CODEX".into(),
                original_text: "proof".into(),
                approved_text: "proof".into(),
                owner_principal_key: "owner".into(),
                binding_revision: 2,
                root_revision: 1,
                destination_policy_json: serde_json::json!({
                    "version": 1,
                    "destinationEndpointId": codex.id,
                    "rootRevision": 1,
                    "policy": root_policy,
                    "browserSettingsRevision": settings.settings_revision,
                })
                .to_string(),
                destination_policy_hash: "policy".into(),
                draft_revision: 1,
                client_request_id: "request-approved".into(),
                attachments: vec![RelayAttachmentSelection {
                    attachment_id: "attachment-approved".into(),
                    artifact_id: artifact.id,
                    mime: "image/png".into(),
                    approved_sha256: digest,
                    approved_size: 4,
                    manifest_entry_json: "{}".into(),
                    target_relative_path: None,
                }],
            })
            .unwrap();
        let nonce = "c".repeat(64);
        store
            .issue_relay_approval_nonce("owner", &handoff.id, 1, &nonce, now() + 10_000)
            .unwrap();
        assert_eq!(
            store
                .relay_review("owner", &handoff.id)
                .unwrap()
                .approval_state,
            "PENDING"
        );
        store
            .with_connection(|connection| {
                connection
                    .execute("UPDATE project_execution_roots SET revision=2", [])
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            store
                .approve_relay_draft("owner", &handoff.id, 1, &nonce)
                .unwrap_err(),
            "RELAY_POLICY_CHANGED"
        );
        store
            .with_connection(|connection| {
                connection
                    .execute("UPDATE project_execution_roots SET revision=1", [])
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            store
                .approve_relay_draft("owner", &handoff.id, 1, &nonce)
                .unwrap()
                .approval_state,
            "APPROVED"
        );
        assert!(store
            .consumed_relay_approval_recovery_available("owner", &handoff.id)
            .unwrap());
        assert!(!store
            .consumed_relay_approval_recovery_available("wrong-owner", &handoff.id)
            .unwrap());
        assert_eq!(
            store
                .issue_relay_approval_nonce(
                    "owner",
                    &handoff.id,
                    1,
                    &"d".repeat(64),
                    now() + 10_000
                )
                .unwrap_err(),
            "STALE_RELAY_DRAFT"
        );
        assert_eq!(
            store
                .approve_relay_draft("owner", &handoff.id, 1, &nonce)
                .unwrap_err(),
            "RELAY_APPROVAL_ALREADY_CONSUMED"
        );
        // Simulate the old crash window after approval consumption but before
        // a dispatch row existed.  The same nonce can resume only this one
        // claim, then a response-loss retry becomes a pure durable readback.
        let epoch = "00000000-0000-0000-0000-000000000001";
        let claimed = store
            .resume_consumed_relay_approval("owner", &handoff.id, epoch)
            .unwrap();
        assert!(!store
            .consumed_relay_approval_recovery_available("owner", &handoff.id)
            .unwrap());
        assert_eq!(
            store
                .resume_consumed_relay_approval("owner", &handoff.id, epoch)
                .unwrap_err(),
            "RELAY_RECOVERY_UNAVAILABLE"
        );
        assert_eq!(claimed.destination_provider, "CODEX");
        assert_eq!(claimed.attachments.len(), 1);
        let native_input = crate::application::dispatch::relay_native_input(&claimed).unwrap();
        assert_eq!(native_input[0]["type"], "text");
        assert_eq!(native_input[0]["text"], claimed.approved_text);
        assert_eq!(native_input[1]["type"], "localImage");
        assert!(native_input[1]["path"]
            .as_str()
            .unwrap()
            .replace('\\', "/")
            .ends_with(&format!(
                "/.ai-work-router/handoffs/{}/in/00-artifact-approved-proof.png",
                handoff.id
            )));
        let (_approval, replay, newly_claimed) = store
            .resume_or_claim_relay_approval("owner", &handoff.id, 1, &nonce, epoch)
            .unwrap();
        assert!(!newly_claimed);
        assert!(replay.is_none());
        let replay = store
            .relay_approval_readback("owner", &handoff.id, 1, &nonce)
            .unwrap()
            .unwrap();
        assert_eq!(replay.dispatch_id, claimed.dispatch_id);
        assert_eq!(
            store
                .resume_or_claim_relay_approval("wrong-owner", &handoff.id, 1, &nonce, epoch)
                .unwrap_err(),
            "STALE_RELAY_DRAFT"
        );
        assert_eq!(
            store
                .resume_or_claim_relay_approval("owner", &handoff.id, 1, &"d".repeat(64), epoch)
                .unwrap_err(),
            "STALE_RELAY_DRAFT"
        );
        assert_eq!(
            store
                .resume_or_claim_relay_approval("owner", &handoff.id, 2, &nonce, epoch)
                .unwrap_err(),
            "STALE_RELAY_DRAFT"
        );
        assert_eq!(
            store
                .relay_dispatch_snapshot(
                    "owner",
                    &handoff.id,
                    &claimed.run_id,
                    &claimed.dispatch_id,
                )
                .unwrap()
                .payload_hash,
            claimed.payload_hash
        );
        store
            .mark_dispatch_unknown(&claimed.dispatch_id, epoch, "RELAY_HOST_START_FAILED")
            .unwrap();
        let unknown = store
            .relay_approval_readback("owner", &handoff.id, 1, &nonce)
            .unwrap()
            .unwrap();
        assert_eq!(unknown.handoff_status, "SENDING");
        assert_eq!(unknown.run_status, "UNKNOWN");
        assert_eq!(unknown.dispatch_phase, "UNKNOWN");
        assert_eq!(
            unknown.last_code.as_deref(),
            Some("RELAY_HOST_START_FAILED")
        );
        // Endpoint-source Relay runs do not own a synthetic Control Context.
        // A signed, exact native prewrite stop is enough to close this one
        // original dispatch as NOT_SENT; a different owner cannot unlock it.
        let evidence_key = [11; 32];
        store
            .record_prewrite_stop(
                &claimed.dispatch_id,
                &crate::codex::adapter::SealedPrewrite {
                    adapter_epoch: epoch.into(),
                },
                &evidence_key,
            )
            .unwrap();
        assert!(store
            .complete_relay_sealed_prewrite_stop(
                "wrong-owner",
                &handoff.id,
                &claimed.run_id,
                &claimed.dispatch_id,
                &evidence_key,
                "RELAY_NATIVE_PREWRITE_FAILED",
            )
            .is_err());
        store
            .complete_relay_sealed_prewrite_stop(
                "owner",
                &handoff.id,
                &claimed.run_id,
                &claimed.dispatch_id,
                &evidence_key,
                "RELAY_NATIVE_PREWRITE_FAILED",
            )
            .unwrap();
        assert_eq!(
            store.dispatch_record(&claimed.dispatch_id).unwrap().phase,
            "NOT_SENT"
        );
        assert_eq!(
            store
                .browser_owned_run("owner", &claimed.run_id)
                .unwrap()
                .status,
            "FAILED"
        );

        // A reverse Review is allowed to collect output from this exact
        // completed forward run even after a replacement ChatGPT conversation
        // has superseded the historical source endpoint.  This is a local
        // sealed-output read, not a retry or a provider write.
        store
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE provider_runs SET status='COMPLETED' WHERE id=?1",
                        params![claimed.run_id],
                    )
                    .map_err(db_error)?;
                connection
                    .execute(
                        "UPDATE provider_result_details SET result_state='AVAILABLE' WHERE run_id=?1",
                        params![claimed.run_id],
                    )
                    .map_err(db_error)?;
                connection
                    .execute(
                        "UPDATE handoff_dispatches SET phase='TERMINAL' WHERE dispatch_id=?1",
                        params![claimed.dispatch_id],
                    )
                    .map_err(db_error)?;
                connection
                    .execute(
                        "UPDATE workstreams SET binding_revision=3 WHERE id=?1",
                        params![handoff.workstream_id],
                    )
                    .map_err(db_error)?;
                connection
                    .execute(
                        "UPDATE endpoints SET status='SUPERSEDED' WHERE id=?1",
                        params![source_endpoint_id],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            store
                .relay_dispatch_snapshot(
                    "owner",
                    &handoff.id,
                    &claimed.run_id,
                    &claimed.dispatch_id,
                )
                .unwrap_err(),
            "RELAY_BINDING_CHANGED"
        );
        let historical = store
            .completed_codex_output_snapshot("owner", &handoff.workstream_id, &claimed.run_id)
            .unwrap();
        assert_eq!(historical.handoff_id, handoff.id);
        assert_eq!(historical.run_id, claimed.run_id);
        assert_eq!(historical.dispatch_id, claimed.dispatch_id);
    }

    #[test]
    fn reverse_relay_acceptance_and_terminal_are_bridge_evidence_only() {
        let (_dir, store) = preview_store();
        let (workstream, chat, codex) = endpoints(&store);
        let root_policy = crate::store::control::PolicySnapshot {
            version: 1,
            backend_id: "00000000-0000-0000-0000-000000000001".into(),
            native_version: "0.154.0".into(),
            model: "fixture".into(),
            permission_mode: crate::store::control::PermissionMode::ReadOnly,
            approval_mode: crate::store::control::ApprovalMode::OnRequest,
            network_access: false,
            root_identity_hash: "a".repeat(64),
            config_digest: "b".repeat(64),
            native_launch_digest: None,
        };
        store.with_connection(|c| {
            let project: String=c.query_row("SELECT project_id FROM workstreams WHERE id=?1",params![workstream],|r|r.get(0)).map_err(db_error)?;
            c.execute("INSERT INTO project_execution_roots(project_id,canonical_path,path_identity_hash,revision,policy_json,policy_hash,updated_at) VALUES(?1,'C:/fixture',?2,1,?3,?4,1)",params![project,root_policy.root_identity_hash,serde_json::to_string(&root_policy).map_err(|_|"INTERNAL")?,root_policy.hash()?]).map_err(db_error)?;
            Ok(())
        }).unwrap();
        let settings = store
            .configure_browser_endpoint(BrowserEndpointSettingsInput {
                endpoint_id: chat.id.clone(),
                owner_principal_key: "owner".into(),
                observe_enabled: false,
                paired_by: "owner".into(),
            })
            .unwrap();
        let handoff=store.create_ready_relay_handoff(NewRelayHandoff{
            workstream_id:workstream,source_endpoint_id:codex.id.clone(),source_run_id:None,source_message_ref:"turn-1".into(),capture_digest:"result".into(),source_completeness:"COMPLETE".into(),destination_endpoint_id:chat.id.clone(),direction:"CODEX_TO_CHATGPT".into(),original_text:"result".into(),approved_text:"result".into(),owner_principal_key:"owner".into(),binding_revision:2,root_revision:1,
            destination_policy_json:serde_json::json!({"version":1,"destinationEndpointId":chat.id,"strictSingleSubmit":true,"attachmentLimitProfile":"hybrid-v1","expectedConversationId":"conversation","browserSettingsRevision":settings.settings_revision}).to_string(),destination_policy_hash:"c".repeat(64),draft_revision:1,client_request_id:"reverse".into(),attachments:vec![]
        }).unwrap();
        let nonce = "d".repeat(64);
        store
            .issue_relay_approval_nonce("owner", &handoff.id, 1, &nonce, now() + 10_000)
            .unwrap();
        store
            .approve_relay_draft("owner", &handoff.id, 1, &nonce)
            .unwrap();
        let snapshot = store
            .claim_approved_relay("owner", &handoff.id, "00000000-0000-0000-0000-000000000001")
            .unwrap();
        let prepared = store
            .begin_relay_browser_write(
                "owner",
                &handoff.id,
                &snapshot.run_id,
                &snapshot.dispatch_id,
            )
            .unwrap();
        store
            .persist_relay_browser_write_intent(
                "owner",
                &handoff.id,
                &snapshot.run_id,
                &snapshot.dispatch_id,
            )
            .unwrap();
        let accepted = store
            .record_bridge_dispatch_evidence(
                "owner",
                &handoff.id,
                &snapshot.run_id,
                &snapshot.dispatch_id,
                BridgeDispatchEvidence {
                    id: String::new(),
                    dispatch_id: snapshot.dispatch_id.clone(),
                    kind: "USER_ACCEPTED".into(),
                    endpoint_id: chat.id.clone(),
                    conversation_id: "conversation".into(),
                    source_message_id: "user-message".into(),
                    request_id: snapshot.dispatch_id.clone(),
                    turn_key: Some("turn-key".into()),
                    adapter_epoch: "00000000-0000-0000-0000-000000000001".into(),
                    bundle_identity: "e".repeat(64),
                    payload_hash: prepared.payload_hash,
                    manifest_hash: prepared.manifest_hash,
                    evidence_digest: String::new(),
                    integrity_mac: String::new(),
                    recorded_at: 0,
                },
                &[7; 32],
            )
            .unwrap();
        assert_eq!(accepted.kind, "USER_ACCEPTED");
        let accepted_readback = store
            .relay_approval_readback("owner", &handoff.id, 1, &nonce)
            .unwrap()
            .unwrap();
        assert_eq!(accepted_readback.handoff_status, "SENT");
        assert_eq!(accepted_readback.run_status, "RUNNING");
        assert_eq!(accepted_readback.dispatch_phase, "ACCEPTED");
        assert_eq!(accepted_readback.dispatch_id, snapshot.dispatch_id);
        assert_eq!(
            store.pending_reverse_relays(64).unwrap(),
            vec![PendingReverseRelay {
                owner: "owner".into(),
                handoff_id: handoff.id.clone(),
            }]
        );
        // A restart must initially fail closed, but an independently exact
        // terminal observation may recover this same accepted dispatch.
        store
            .mark_dispatch_unknown(
                &snapshot.dispatch_id,
                "00000000-0000-0000-0000-000000000001",
                "RESTART_UNCERTAIN",
            )
            .unwrap();
        assert_eq!(
            store.dispatch_record(&snapshot.dispatch_id).unwrap().phase,
            "UNKNOWN"
        );
        let terminal = store
            .record_bridge_dispatch_evidence(
                "owner",
                &handoff.id,
                &snapshot.run_id,
                &snapshot.dispatch_id,
                BridgeDispatchEvidence {
                    id: String::new(),
                    dispatch_id: snapshot.dispatch_id.clone(),
                    kind: "ASSISTANT_TERMINAL".into(),
                    endpoint_id: chat.id.clone(),
                    conversation_id: "conversation".into(),
                    source_message_id: "assistant-message".into(),
                    request_id: snapshot.dispatch_id.clone(),
                    turn_key: Some("turn-key".into()),
                    adapter_epoch: "00000000-0000-0000-0000-000000000001".into(),
                    bundle_identity: "e".repeat(64),
                    payload_hash: snapshot.payload_hash,
                    manifest_hash: snapshot.manifest_hash,
                    evidence_digest: String::new(),
                    integrity_mac: String::new(),
                    recorded_at: 0,
                },
                &[7; 32],
            )
            .unwrap();
        assert_eq!(terminal.kind, "ASSISTANT_TERMINAL");
        assert_eq!(
            store.dispatch_record(&snapshot.dispatch_id).unwrap().phase,
            "TERMINAL"
        );
        assert!(store.pending_reverse_relays(64).unwrap().is_empty());

        // A typed draft conflict is an explicit non-submit proof, not an ACK
        // loss.  Its durable projection is immediately recoverable NOT_SENT
        // and cannot be promoted to USER_ACCEPTED later.
        let prewrite = store.create_ready_relay_handoff(NewRelayHandoff {
            workstream_id: snapshot.workstream_id.clone(), source_endpoint_id: codex.id,
            source_run_id: None, source_message_ref: "turn-prewrite".into(),
            capture_digest: "prewrite".into(), source_completeness: "COMPLETE".into(),
            destination_endpoint_id: chat.id.clone(), direction: "CODEX_TO_CHATGPT".into(),
            original_text: "result".into(), approved_text: "result".into(),
            owner_principal_key: "owner".into(), binding_revision: 2, root_revision: 1,
            destination_policy_json: serde_json::json!({"version":1,"destinationEndpointId":chat.id,"strictSingleSubmit":true,"attachmentLimitProfile":"hybrid-v1","expectedConversationId":"conversation","browserSettingsRevision":settings.settings_revision}).to_string(),
            destination_policy_hash: "f".repeat(64), draft_revision: 1,
            client_request_id: "reverse-prewrite".into(), attachments: vec![],
        }).unwrap();
        let nonce = "f".repeat(64);
        store
            .issue_relay_approval_nonce("owner", &prewrite.id, 1, &nonce, now() + 10_000)
            .unwrap();
        store
            .approve_relay_draft("owner", &prewrite.id, 1, &nonce)
            .unwrap();
        let prewrite_snapshot = store
            .claim_approved_relay(
                "owner",
                &prewrite.id,
                "00000000-0000-0000-0000-000000000002",
            )
            .unwrap();
        store
            .begin_relay_browser_write(
                "owner",
                &prewrite.id,
                &prewrite_snapshot.run_id,
                &prewrite_snapshot.dispatch_id,
            )
            .unwrap();
        store
            .persist_relay_browser_write_intent(
                "owner",
                &prewrite.id,
                &prewrite_snapshot.run_id,
                &prewrite_snapshot.dispatch_id,
            )
            .unwrap();
        store
            .record_bridge_dispatch_evidence(
                "owner",
                &prewrite.id,
                &prewrite_snapshot.run_id,
                &prewrite_snapshot.dispatch_id,
                BridgeDispatchEvidence {
                    id: String::new(),
                    dispatch_id: prewrite_snapshot.dispatch_id.clone(),
                    kind: "PREWRITE_STOP".into(),
                    endpoint_id: chat.id,
                    conversation_id: "conversation".into(),
                    source_message_id: format!("prewrite:{}", prewrite_snapshot.dispatch_id),
                    request_id: prewrite_snapshot.dispatch_id.clone(),
                    turn_key: None,
                    adapter_epoch: "00000000-0000-0000-0000-000000000002".into(),
                    bundle_identity: "e".repeat(64),
                    payload_hash: prewrite_snapshot.payload_hash,
                    manifest_hash: prewrite_snapshot.manifest_hash,
                    evidence_digest: String::new(),
                    integrity_mac: String::new(),
                    recorded_at: 0,
                },
                &[7; 32],
            )
            .unwrap();
        assert_eq!(
            store
                .dispatch_record(&prewrite_snapshot.dispatch_id)
                .unwrap()
                .phase,
            "NOT_SENT"
        );
        assert_eq!(
            store
                .with_connection(|c| c
                    .query_row(
                        "SELECT status FROM provider_runs WHERE id=?1",
                        params![prewrite_snapshot.run_id],
                        |r| r.get::<_, String>(0)
                    )
                    .map_err(db_error))
                .unwrap(),
            "FAILED"
        );
        assert!(store
            .record_bridge_dispatch_evidence(
                "owner",
                &prewrite.id,
                &prewrite_snapshot.run_id,
                &prewrite_snapshot.dispatch_id,
                BridgeDispatchEvidence {
                    id: String::new(),
                    dispatch_id: prewrite_snapshot.dispatch_id.clone(),
                    kind: "USER_ACCEPTED".into(),
                    endpoint_id: "chat".into(),
                    conversation_id: "conversation".into(),
                    source_message_id: "forbidden".into(),
                    request_id: prewrite_snapshot.dispatch_id.clone(),
                    turn_key: Some("forbidden".into()),
                    adapter_epoch: "00000000-0000-0000-0000-000000000002".into(),
                    bundle_identity: "e".repeat(64),
                    payload_hash: "x".repeat(64),
                    manifest_hash: "x".repeat(64),
                    evidence_digest: String::new(),
                    integrity_mac: String::new(),
                    recorded_at: 0,
                },
                &[7; 32],
            )
            .is_err());
    }
}
