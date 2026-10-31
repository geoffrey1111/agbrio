//! Allowlisted public history. Never serialize native messages wholesale.
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

pub(super) fn public_entry(entry: &Value) -> Option<Value> {
    if entry["type"] != "message" {
        return None;
    }
    let id = entry["id"].as_str()?;
    let message = &entry["message"];
    let role = message["role"].as_str()?;
    if !matches!(role, "user" | "assistant" | "toolResult") {
        return None;
    }
    let mut blocks = Vec::new();
    if let Some(text) = message["content"].as_str() {
        blocks.push(json!({"type":"text","text":text}));
    }
    if let Some(content) = message["content"].as_array() {
        for block in content {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(text) = block["text"].as_str() {
                        blocks.push(json!({"type":"text","text":text}));
                    }
                }
                Some("image") => {
                    // Bytes stay native/private until the media importer validates
                    // and creates a scoped attachment. Never relay base64 to MCP.
                    if matches!(block["mimeType"].as_str(), Some("image/png" | "image/jpeg")) {
                        blocks.push(json!({"type":"image","mimeType":block["mimeType"],"nativeEntryId":id,"availability":"NATIVE_REFERENCE"}));
                    }
                }
                Some("toolCall") => {
                    // Tool arguments and results may be private task data, but
                    // signatures and provider replay/thinking fields never are.
                    blocks.push(json!({"type":"toolCall","id":block["id"].as_str(),"name":block["name"].as_str()}));
                }
                _ => {}
            }
        }
    }
    Some(
        json!({"id":id,"parentId":entry["parentId"].as_str(),"timestamp":entry["timestamp"].as_str(),"role":role,"content":blocks,"provider":message["provider"].as_str(),"model":message["model"].as_str(),"responseModel":message["responseModel"].as_str(),"providerThinkingLevel":message["providerThinkingLevel"].as_str(),"toolCallId":message["toolCallId"].as_str(),"toolName":message["toolName"].as_str(),"isError":message["isError"].as_bool(),"errorCode":message["errorMessage"].as_str().filter(|s|!s.is_empty()).map(|s|if s.starts_with("PI_PROVIDER_"){s}else{super::history::provider_error(s)}),"stopReason":message["stopReason"].as_str()}),
    )
}

/// Exact active leaf traversal; abandoned branches remain distinct. Missing or
/// cyclic native parents are a protocol error, never repaired by text matching.
pub(super) fn active_branch<'a>(
    entries: &'a [Value],
    leaf: Option<&str>,
) -> Result<Vec<&'a Value>, String> {
    let mut index = HashMap::new();
    for entry in entries {
        let id = entry["id"].as_str().ok_or("PI_HISTORY_ID_INVALID")?;
        if index.insert(id, entry).is_some() {
            return Err("PI_HISTORY_DUPLICATE_ID".into());
        }
    }
    let mut current = leaf;
    let mut visited = HashSet::new();
    let mut branch = Vec::new();
    while let Some(id) = current {
        if !visited.insert(id) {
            return Err("PI_HISTORY_CYCLE".into());
        }
        let entry = *index.get(id).ok_or("PI_HISTORY_PARENT_MISSING")?;
        branch.push(entry);
        current = entry["parentId"].as_str();
    }
    branch.reverse();
    Ok(branch)
}

/// Invoke only after agent_settled. A prompt ACK/agent_end cannot call this.
pub(super) fn settled_state(
    entries: &[Value],
    leaf: Option<&str>,
    aborted: bool,
) -> Result<&'static str, String> {
    let branch = active_branch(entries, leaf)?;
    if aborted {
        return Ok("CANCELLED");
    }
    let latest_user = branch
        .iter()
        .rposition(|e| e["type"] == "message" && e["message"]["role"] == "user")
        .ok_or("PI_RESULT_WITHOUT_INPUT")?;
    let current = &branch[latest_user + 1..];
    let last = current
        .iter()
        .rev()
        .find(|e| e["type"] == "message" && e["message"]["role"] == "assistant")
        .ok_or("PI_RESULT_UNCONFIRMED")?;
    let message = &last["message"];
    if message["stopReason"] == "aborted" {
        return Ok("CANCELLED");
    }
    if message["stopReason"] == "error"
        || message["errorMessage"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    {
        return Ok("FAILED");
    }
    if !matches!(message["stopReason"].as_str(), Some("stop" | "length")) {
        return Err("PI_RESULT_UNCONFIRMED".into());
    }
    if message["stopReason"] == "length" {
        return Ok("PARTIAL");
    }
    Ok("RESULT_READY")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_projection_drops_thinking_signatures_and_image_bytes() {
        let entry = json!({"type":"message","id":"a","message":{"role":"assistant","content":[{"type":"thinking","thinking":"PRIVATE_THOUGHT","thinkingSignature":"PRIVATE_SIGNATURE"},{"type":"text","text":"answer","textSignature":"PRIVATE_SIGNATURE"},{"type":"image","mimeType":"image/png","data":"PRIVATE_IMAGE"},{"type":"toolCall","id":"t","name":"read","arguments":{"path":"x"},"thoughtSignature":"PRIVATE_SIGNATURE"}]}});
        let output = public_entry(&entry).unwrap().to_string();
        assert!(output.contains("answer"));
        assert!(!output.contains("PRIVATE_"));
    }
    fn entry(id: &str, parent: Option<&str>, role: &str, stop: &str) -> Value {
        json!({"type":"message","id":id,"parentId":parent,"message":{"role":role,"stopReason":stop}})
    }
    #[test]
    fn settled_false_does_not_override_assistant_error_or_abandoned_branch() {
        let entries = vec![
            entry("u", None, "user", ""),
            entry("good", Some("u"), "assistant", "stop"),
            entry("bad", Some("u"), "assistant", "error"),
        ];
        assert_eq!(
            settled_state(&entries, Some("bad"), false).unwrap(),
            "FAILED"
        );
        assert_eq!(
            settled_state(&entries, Some("good"), false).unwrap(),
            "RESULT_READY"
        );
    }
    #[test]
    fn settled_without_current_answer_never_reuses_previous_result() {
        let entries = vec![
            entry("u", None, "user", ""),
            entry("a", Some("u"), "assistant", "stop"),
            entry("next", Some("a"), "user", ""),
        ];
        assert!(settled_state(&entries, Some("next"), false).is_err());
        assert_eq!(
            settled_state(&entries, Some("next"), true).unwrap(),
            "CANCELLED"
        );
    }
}
