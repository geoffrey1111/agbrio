use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Constructed only after the ingress has authenticated and mapped an owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeIdentity {
    pub principal_key: String,
    pub scope_key: String,
    pub key_version: i64,
}

pub fn length_prefixed_hash(fields: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for field in fields {
        hash.update((field.len() as u64).to_be_bytes());
        hash.update(field);
    }
    format!("{:x}", hash.finalize())
}

fn keyed(key: &[u8; 32], fields: &[&[u8]]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("fixed key length");
    for field in fields {
        mac.update(&(field.len() as u64).to_be_bytes());
        mac.update(field);
    }
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Owner mapping has already unified separately verified application subjects.
pub fn principal_key(key: &[u8; 32], issuer: &str, configured_owner_subject: &str) -> String {
    keyed(
        key,
        &[
            b"principal-v1",
            issuer.as_bytes(),
            configured_owner_subject.as_bytes(),
        ],
    )
}

pub fn request_scope(
    key: &[u8; 32],
    key_version: i64,
    principal_key: &str,
    meta: &Value,
) -> Result<ScopeIdentity, String> {
    if key_version <= 0 || principal_key.len() != 64 {
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
    let org = if meta.get("openai/organization").is_some() {
        Some(field("openai/organization")?)
    } else {
        None
    };
    Ok(ScopeIdentity {
        principal_key: principal_key.into(),
        scope_key: keyed(
            key,
            &[
                b"scope-v1",
                b"chatgpt",
                principal_key.as_bytes(),
                subject.as_bytes(),
                session.as_bytes(),
                if org.is_some() { b"PRESENT" } else { b"ABSENT" },
                org.unwrap_or("").as_bytes(),
            ],
        ),
        key_version,
    })
}

/// Creates a scope for the independently authenticated Review surface when it
/// starts a new, direct-paused Codex task.  This is deliberately distinct from
/// a ChatGPT host scope: the Review page is the actor that requested creation,
/// and no browser conversation identity is inferred or fabricated.
pub fn review_creation_scope(
    key: &[u8; 32],
    key_version: i64,
    principal_key: &str,
    client_request_id: &str,
) -> Result<ScopeIdentity, String> {
    if key_version <= 0
        || principal_key.len() != 64
        || client_request_id.len() != 36
        || !client_request_id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b'-')
    {
        return Err("SCOPE_INVALID".into());
    }
    Ok(ScopeIdentity {
        principal_key: principal_key.into(),
        scope_key: keyed(
            key,
            &[
                b"scope-v1",
                b"review-create-v1",
                principal_key.as_bytes(),
                client_request_id.as_bytes(),
            ],
        ),
        key_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn scope_is_per_request_principal_and_exact_host_not_conversation() {
        let key = [17; 32];
        let owner = principal_key(&key, "issuer", "owner");
        let a = json!({"openai/subject":"s","openai/session":"a"});
        let first = request_scope(&key, 1, &owner, &a).unwrap();
        assert_eq!(first, request_scope(&key, 1, &owner, &a).unwrap());
        assert_ne!(
            first,
            request_scope(
                &key,
                1,
                &owner,
                &json!({"openai/subject":"s","openai/session":"b"})
            )
            .unwrap()
        );
        assert_ne!(
            first,
            request_scope(&key, 1, &principal_key(&key, "issuer", "other"), &a).unwrap()
        );
        assert_eq!(
            request_scope(&key, 1, &owner, &json!({})).unwrap_err(),
            "SCOPE_REQUIRED"
        );
        for bad in [
            json!(null),
            json!([]),
            json!({}),
            json!(""),
            json!(" "),
            json!("x".repeat(513)),
        ] {
            assert!(request_scope(
                &key,
                1,
                &owner,
                &json!({"openai/subject":"s","openai/session":bad})
            )
            .is_err());
        }
        assert_ne!(
            length_prefixed_hash(&[b"ab", b"c"]),
            length_prefixed_hash(&[b"a", b"bc"])
        );
    }

    #[test]
    fn review_creation_scope_is_owner_and_request_scoped_not_a_chat_scope() {
        let key = [23; 32];
        let owner = principal_key(&key, "issuer", "owner");
        let first =
            review_creation_scope(&key, 1, &owner, "11111111-1111-4111-8111-111111111111").unwrap();
        assert_eq!(first.principal_key, owner);
        assert_ne!(
            first,
            request_scope(
                &key,
                1,
                &owner,
                &json!({"openai/subject":"s","openai/session":"a"})
            )
            .unwrap()
        );
        assert_ne!(
            first,
            review_creation_scope(&key, 1, &owner, "22222222-2222-4222-8222-222222222222",)
                .unwrap()
        );
    }
}
