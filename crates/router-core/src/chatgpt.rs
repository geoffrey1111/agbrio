//! Narrow ChatGPTAdapter for the isolated Browser Executor Spike. The existing
//! Store remains route/approval/dispatch authority. No DOM/CDP/browser lifecycle
//! enters application/domain code, and no legacy transport is a fallback.
use crate::browser_executor::ExecutorProcess;
use crate::store::{Provider, RouterStore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::PathBuf, process::Command, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactReply {
    pub conversation_id: String,
    pub message_id: String,
    pub text: String,
    pub text_sha256: String,
    pub preceding_user_message_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Receipt {
    Sent {
        dispatch_id: String,
        conversation_id: String,
        accepted_message_id: String,
        payload_sha256: String,
    },
    Unknown {
        dispatch_id: String,
    },
    NotFound,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireResult {
    status: String,
    code: Option<String>,
    conversation_id: Option<String>,
    dispatch_id: Option<String>,
    message_id: Option<String>,
    text: Option<String>,
    text_sha256: Option<String>,
    payload_sha256: Option<String>,
    accepted_message_id: Option<String>,
    preceding_user_message_id: Option<String>,
}

pub struct ChatGPTAdapter {
    node: PathBuf,
    cli: PathBuf,
    manifests: PathBuf,
    process: Option<Arc<ExecutorProcess>>,
}

impl ChatGPTAdapter {
    pub fn new(node: PathBuf, cli: PathBuf, manifests: PathBuf) -> Self {
        Self {
            node,
            cli,
            manifests,
            process: None,
        }
    }

    pub fn resident(process: Arc<ExecutorProcess>, manifests: PathBuf) -> Self {
        Self {
            node: PathBuf::new(),
            cli: PathBuf::new(),
            manifests,
            process: Some(process),
        }
    }

    pub fn observe_exact(&self, conversation_id: &str) -> Result<ExactReply, String> {
        uuid::Uuid::parse_str(conversation_id).map_err(|_| "EXACT_CONVERSATION_ID_REQUIRED")?;
        normalize_reply(
            self.call("observe", Some(conversation_id))?,
            conversation_id,
        )
    }
    pub fn observe_passive(&self, conversation_id: &str) -> Result<ExactReply, String> {
        uuid::Uuid::parse_str(conversation_id).map_err(|_| "EXACT_CONVERSATION_ID_REQUIRED")?;
        let value = self
            .process
            .as_ref()
            .ok_or("RESIDENT_EXECUTOR_REQUIRED")?
            .request(
                "observe",
                serde_json::json!({"conversationId": conversation_id, "passive": true}),
            )?;
        normalize_reply(
            serde_json::from_value(value).map_err(|_| "BROWSER_EXECUTOR_EVIDENCE_INVALID")?,
            conversation_id,
        )
    }

    /// Application must first persist its existing human approval/write intent.
    /// This adapter cannot select a destination, approve, edit or retry it.
    pub fn submit_approved_exact(
        &self,
        conversation_id: &str,
        dispatch_id: &str,
        immutable_payload: &str,
    ) -> Result<Receipt, String> {
        self.submit_approved_with_attachments(
            conversation_id,
            dispatch_id,
            immutable_payload,
            serde_json::json!([]),
        )
    }

    pub fn submit_approved_with_attachments(
        &self,
        conversation_id: &str,
        dispatch_id: &str,
        immutable_payload: &str,
        attachments: serde_json::Value,
    ) -> Result<Receipt, String> {
        uuid::Uuid::parse_str(conversation_id).map_err(|_| "EXACT_CONVERSATION_ID_REQUIRED")?;
        let payload_sha256 = digest(immutable_payload);
        let fields = serde_json::json!({"conversationId": conversation_id,
            "dispatchId": dispatch_id, "immutablePayload": immutable_payload,
            "payloadSha256": payload_sha256, "attachments": attachments});
        let process = self.process.as_ref().ok_or("RESIDENT_EXECUTOR_REQUIRED")?;
        let result = process.request("send", fields);
        match result {
            Ok(value) => normalize_receipt(
                serde_json::from_value(value).map_err(|_| "BROWSER_EXECUTOR_EVIDENCE_INVALID")?,
                dispatch_id,
                Some((conversation_id, &payload_sha256)),
            ),
            Err(_) => Ok(Receipt::Unknown {
                dispatch_id: dispatch_id.into(),
            }),
        }
    }

    fn call(&self, operation: &str, argument: Option<&str>) -> Result<WireResult, String> {
        if !matches!(operation, "health" | "observe" | "send" | "receipt") {
            return Err("BROWSER_EXECUTOR_OPERATION_INVALID".into());
        }
        if let Some(process) = &self.process {
            let fields = match operation {
                "observe" => serde_json::json!({"conversationId": argument}),
                "receipt" => serde_json::json!({"dispatchId": argument}),
                "send" => serde_json::from_slice(
                    &fs::read(argument.ok_or("SEND_MANIFEST_REQUIRED")?)
                        .map_err(|_| "SEND_MANIFEST_UNAVAILABLE")?,
                )
                .map_err(|_| "SEND_MANIFEST_INVALID")?,
                _ => serde_json::json!({}),
            };
            return serde_json::from_value(process.request(operation, fields)?)
                .map_err(|_| "BROWSER_EXECUTOR_EVIDENCE_INVALID".into());
        }
        let mut command = Command::new(&self.node);
        command.arg(node_path(&self.cli)?).arg(operation);
        if let Some(argument) = argument {
            command.arg(argument);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // helper console only; browser remains headed
        }
        let output = command
            .output()
            .map_err(|_| "BROWSER_EXECUTOR_UNAVAILABLE")?;
        if !output.status.success() || output.stdout.len() > 1_000_000 {
            return Err("BROWSER_EXECUTOR_PROCESS_FAILED".into());
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|_| "BROWSER_EXECUTOR_EVIDENCE_INVALID".into())
    }

    pub fn health(&self) -> Result<(), String> {
        let result = self.call("health", None)?;
        if result.status == "AVAILABLE" {
            Ok(())
        } else {
            Err(result.status)
        }
    }

    pub fn observe_active(
        &self,
        store: &RouterStore,
        workstream_id: &str,
    ) -> Result<ExactReply, String> {
        let active = store
            .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
            .ok_or("NO_ACTIVE_CHATGPT_ENDPOINT")?;
        uuid::Uuid::parse_str(&active.external_id).map_err(|_| "EXACT_CONVERSATION_ID_REQUIRED")?;
        let reply = normalize_reply(
            self.call("observe", Some(&active.external_id))?,
            &active.external_id,
        )?;
        let current = store
            .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
            .ok_or("ACTIVE_ENDPOINT_CHANGED")?;
        if current.id != active.id || current.external_id != active.external_id {
            return Err("ACTIVE_ENDPOINT_CHANGED".into());
        }
        Ok(reply)
    }

    /// Caller must already have consumed Router approval and persisted the
    /// existing WRITE_INTENT. This method cannot approve/claim/create a dispatch.
    /// Reconcile uses receipt(), never send(), for retained UNKNOWN/accepted rows.
    pub fn send_claimed_relay(
        &self,
        store: &RouterStore,
        owner: &str,
        handoff_id: &str,
        run_id: &str,
        dispatch_id: &str,
    ) -> Result<Receipt, String> {
        let record = store.dispatch_record(dispatch_id)?;
        if record.phase != "WRITE_INTENT"
            || record.handoff_id != handoff_id
            || record.run_id != run_id
        {
            return Err("ROUTER_WRITE_INTENT_REQUIRED".into());
        }
        let snapshot = store.relay_dispatch_snapshot(owner, handoff_id, run_id, dispatch_id)?;
        if snapshot.destination_provider != "CHATGPT" || snapshot.direction != "CODEX_TO_CHATGPT" {
            return Err("RELAY_DESTINATION_INVALID".into());
        }
        if !snapshot.attachments.is_empty() {
            return Err("SPIKE_TEXT_ONLY_ATTACHMENTS_UNSUPPORTED".into());
        }
        let active = store
            .active_endpoint_for_workstream(&snapshot.workstream_id, Provider::Chatgpt)?
            .ok_or("NO_ACTIVE_CHATGPT_ENDPOINT")?;
        if active.id != snapshot.destination_endpoint_id
            || active.external_id != snapshot.destination_external_id
        {
            return Err("ACTIVE_ENDPOINT_CHANGED".into());
        }
        uuid::Uuid::parse_str(&active.external_id).map_err(|_| "EXACT_CONVERSATION_ID_REQUIRED")?;
        let payload_sha256 = digest(&snapshot.approved_text);
        let manifest = serde_json::json!({"conversationId": active.external_id, "dispatchId": dispatch_id,
            "immutablePayload": snapshot.approved_text, "payloadSha256": payload_sha256});
        fs::create_dir_all(&self.manifests).map_err(|_| "SEND_MANIFEST_UNAVAILABLE")?;
        let path = self
            .manifests
            .join(format!("{}.json", uuid::Uuid::new_v4()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| "SEND_MANIFEST_UNAVAILABLE")?;
        file.write_all(manifest.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "SEND_MANIFEST_UNAVAILABLE")?;
        drop(file);
        let result = self.call("send", Some(&node_path(&path)?));
        // Delete only the exact uniquely-created manifest. Transport loss after
        // durable Router intent is UNKNOWN; it never grants a retry.
        let _ = fs::remove_file(path);
        match result {
            Ok(wire) => normalize_receipt(
                wire,
                dispatch_id,
                Some((&active.external_id, &payload_sha256)),
            ),
            Err(_) => Ok(Receipt::Unknown {
                dispatch_id: dispatch_id.into(),
            }),
        }
    }

    pub fn receipt(&self, dispatch_id: &str) -> Result<Receipt, String> {
        normalize_receipt(self.call("receipt", Some(dispatch_id))?, dispatch_id, None)
    }
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
// Node's entry-module resolver does not accept Windows verbatim DOS paths.
// Preserve the same physical path while using ordinary native argument syntax.
pub(crate) fn node_path(path: &std::path::Path) -> Result<String, String> {
    let value = path.to_str().ok_or("EXECUTOR_PATH_INVALID")?;
    #[cfg(windows)]
    {
        if let Some(rest) = value.strip_prefix("\\\\?\\UNC\\") {
            return Ok(format!("\\\\{rest}"));
        }
        if let Some(rest) = value.strip_prefix("\\\\?\\") {
            return Ok(rest.to_string());
        }
    }
    Ok(value.to_string())
}
fn normalize_reply(wire: WireResult, expected: &str) -> Result<ExactReply, String> {
    if wire.status != "OBSERVED" {
        return Err(wire.status);
    }
    let reply = ExactReply {
        conversation_id: wire.conversation_id.ok_or("EXACT_IDENTITY_MISSING")?,
        message_id: wire.message_id.ok_or("EXACT_IDENTITY_MISSING")?,
        text: wire.text.ok_or("TERMINAL_TEXT_MISSING")?,
        text_sha256: wire.text_sha256.ok_or("TERMINAL_HASH_MISSING")?,
        preceding_user_message_id: wire.preceding_user_message_id,
    };
    if reply.conversation_id != expected
        || reply.message_id.is_empty()
        || reply.text.is_empty()
        || digest(&reply.text) != reply.text_sha256
    {
        return Err("EXACT_REPLY_EVIDENCE_INVALID".into());
    }
    Ok(reply)
}
fn normalize_receipt(
    wire: WireResult,
    dispatch: &str,
    expected: Option<(&str, &str)>,
) -> Result<Receipt, String> {
    if wire.status == "NOT_FOUND" {
        return Ok(Receipt::NotFound);
    }
    if !matches!(wire.status.as_str(), "SENT" | "UNKNOWN") {
        return Err(wire.code.unwrap_or(wire.status));
    }
    if wire.dispatch_id.as_deref() != Some(dispatch) {
        return Err("DISPATCH_IDENTITY_MISMATCH".into());
    }
    if wire.status == "UNKNOWN" {
        return Ok(Receipt::Unknown {
            dispatch_id: dispatch.into(),
        });
    }
    if wire.status != "SENT" {
        return Err(wire.status);
    }
    let conversation = wire.conversation_id.ok_or("EXACT_IDENTITY_MISSING")?;
    let accepted = wire
        .accepted_message_id
        .ok_or("ACCEPTED_IDENTITY_MISSING")?;
    let hash = wire.payload_sha256.ok_or("PAYLOAD_HASH_MISSING")?;
    if accepted.is_empty()
        || uuid::Uuid::parse_str(&conversation).is_err()
        || hash.len() != 64
        || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        || expected.is_some_and(|(id, digest)| id != conversation || digest != hash)
    {
        return Err("ACCEPTANCE_EVIDENCE_INVALID".into());
    }
    Ok(Receipt::Sent {
        dispatch_id: dispatch.into(),
        conversation_id: conversation,
        accepted_message_id: accepted,
        payload_sha256: hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "00000000-0000-4000-8000-000000000001";
    fn wire(value: serde_json::Value) -> WireResult {
        serde_json::from_value(value).unwrap()
    }
    #[cfg(windows)]
    #[test]
    fn node_arguments_preserve_dos_and_unc_paths_without_verbatim_prefix() {
        assert_eq!(
            node_path(std::path::Path::new(r"\\?\D:\project with spaces\cli.mjs")).unwrap(),
            r"D:\project with spaces\cli.mjs"
        );
        assert_eq!(
            node_path(std::path::Path::new(r"\\?\UNC\server\share\cli.mjs")).unwrap(),
            r"\\server\share\cli.mjs"
        );
    }
    #[test]
    fn wrong_conversation_or_modified_terminal_hash_is_rejected() {
        for (id, hash) in [
            ("wrong-conversation", digest("terminal")),
            (ID, digest("other text")),
        ] {
            let result = wire(
                serde_json::json!({"status":"OBSERVED","conversationId":id,"messageId":"native-message","text":"terminal","textSha256":hash}),
            );
            assert_eq!(
                normalize_reply(result, ID).unwrap_err(),
                "EXACT_REPLY_EVIDENCE_INVALID"
            );
        }
    }
    #[test]
    fn mismatched_accepted_dispatch_or_payload_is_not_sent() {
        let result = wire(
            serde_json::json!({"status":"SENT","dispatchId":"other","conversationId":ID,"acceptedMessageId":"native-user","payloadSha256":digest("body")}),
        );
        assert!(normalize_receipt(result, "dispatch", Some((ID, &digest("body")))).is_err());
        let result = wire(
            serde_json::json!({"status":"SENT","dispatchId":"dispatch","conversationId":ID,"acceptedMessageId":"native-user","payloadSha256":digest("other")}),
        );
        assert!(normalize_receipt(result, "dispatch", Some((ID, &digest("body")))).is_err());
    }
}
