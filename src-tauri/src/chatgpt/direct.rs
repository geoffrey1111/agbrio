//! Normal-product ChatGPT carrier contracts with no retired provider dependency.
use super::model::{HandoffAttachment, PreDispatchAttachmentEvidence, RelayCandidate};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use url::Url;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedChatGptResponse {
    pub conversation_id: String,
    pub response_identity: Option<String>,
    pub final_text: String,
    pub artifacts: Option<serde_json::Value>,
    pub relay_candidates: Vec<RelayCandidate>,
}

/// Strict normal-product input parser for explicit ChatGPT Endpoint binding.
/// Project context is accepted only as a provider-native URL segment and is
/// deliberately discarded before routing.
pub fn parse_explicit_conversation_url(input: &str) -> Result<String, String> {
    let url = Url::parse(input.trim())
        .map_err(|_| "Paste a complete ChatGPT conversation URL".to_string())?;
    if url.scheme() != "https"
        || url.host_str() != Some("chatgpt.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("ChatGPT conversation URL must be an exact https://chatgpt.com URL without credentials, port, query, or fragment".into());
    }
    let segments = url
        .path_segments()
        .ok_or("ChatGPT conversation URL path is invalid")?
        .collect::<Vec<_>>();
    let conversation_id = match segments.as_slice() {
        ["c", conversation_id] => *conversation_id,
        ["g", project_segment, "c", conversation_id] if !project_segment.is_empty() => {
            *conversation_id
        }
        _ => return Err("ChatGPT conversation URL must be /c/<conversation_id> or /g/<project>/c/<conversation_id>".into()),
    };
    validate_id(conversation_id)
}

pub(crate) fn validate_attachments(
    attachments: &[HandoffAttachment],
) -> Result<Vec<PreDispatchAttachmentEvidence>, String> {
    attachments
        .iter()
        .map(|attachment| {
            Ok(PreDispatchAttachmentEvidence {
                attachment_id: attachment.id.clone(),
                review_sha256: attachment.actual_sha256.clone(),
                integrity_status: attachment.integrity_status.clone(),
                send_sha256: validate_attachment(attachment)?,
            })
        })
        .collect()
}

fn validate_attachment(attachment: &HandoffAttachment) -> Result<String, String> {
    let metadata = fs::metadata(&attachment.path)
        .map_err(|_| format!("Attachment no longer exists: {}", attachment.filename))?;
    if !metadata.is_file() {
        return Err(format!(
            "Attachment is not a regular file: {}",
            attachment.filename
        ));
    }
    let actual = sha256_file(Path::new(&attachment.path))?;
    if let Some(expected) = &attachment.actual_sha256 {
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!(
                "Attachment changed after review: {}",
                attachment.filename
            ));
        }
    }
    Ok(actual)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut reader =
        BufReader::new(File::open(path).map_err(|_| "Could not open attachment for verification")?);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|_| "Could not read attachment for verification")?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_id(id: &str) -> Result<String, String> {
    if id.len() < 8
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err("Enter a concrete ChatGPT conversation ID or /c/<id> URL; title matching is not supported".to_string());
    }
    Ok(id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_binding_urls_are_strict_and_project_context_is_not_identity() {
        assert_eq!(
            parse_explicit_conversation_url("https://chatgpt.com/g/project/c/12345678-abcd")
                .unwrap(),
            "12345678-abcd"
        );
        for invalid in [
            "12345678-abcd",
            "http://chatgpt.com/c/12345678-abcd",
            "https://chatgpt.com/c/12345678-abcd?x=1",
            "https://chat.openai.com/c/12345678-abcd",
        ] {
            assert!(
                parse_explicit_conversation_url(invalid).is_err(),
                "{invalid}"
            );
        }
    }
}

/// Normalized terminal evidence only; no browser implementation or Tauri types.
#[derive(Debug, PartialEq, Eq)]
pub struct TerminalReply {
    pub message_id: String,
    pub text: String,
}
