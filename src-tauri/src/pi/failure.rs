//! Bounded provider failure facts. Never forward an entire HTTP response/header.
use serde_json::{json, Value};
pub(super) fn owner_action(code: &str) -> bool {
    matches!(
        code,
        "PI_PROVIDER_BILLING_REQUIRED"
            | "PI_PROVIDER_QUOTA_EXHAUSTED"
            | "PI_PROVIDER_AUTHENTICATION_ERROR"
    )
}
pub(super) fn code(message: &str) -> &'static str {
    let s = message.to_ascii_lowercase();
    let status = http_status(message);
    if s.contains("credit balance")
        || s.contains("insufficient_credit")
        || status == Some(402)
        || [
            "billing limit",
            "billing_limit",
            "billing hard",
            "billing_hard",
            "billing error",
            "billing_error",
            "billing required",
            "billing_required",
            "billing disabled",
            "insufficient funds",
        ]
        .iter()
        .any(|p| s.contains(p))
        || s.contains("payment required")
        || s.contains("purchase credits")
        || s == "pi_provider_billing_required"
    {
        return "PI_PROVIDER_BILLING_REQUIRED";
    }
    if (s.contains("quota")
        || s.contains("daily limit")
        || s.contains("monthly limit")
        || s.contains("spend limit")
        || s.contains("usage limit")
        || s.contains("usage_limit")
        || s.contains("usagelimit")
        || s.contains("out of budget")
        || s.contains("available balance"))
        && !s.contains("tokens per min")
        && !s.contains("requests per min")
        || s == "pi_provider_quota_exhausted"
    {
        return "PI_PROVIDER_QUOTA_EXHAUSTED";
    }
    if matches!(status, Some(401 | 403))
        || [
            "unauthori",
            "authentication",
            "permission",
            "invalid api key",
        ]
        .iter()
        .any(|v| s.contains(v))
    {
        return "PI_PROVIDER_AUTHENTICATION_ERROR";
    }
    if s.contains("context") || s.contains("too large") || status == Some(413) {
        return "PI_PROVIDER_CONTEXT_LIMIT";
    }
    if s.contains("abort") || s.contains("cancel") {
        return "PI_PROVIDER_CANCELLED";
    }
    if status == Some(429)
        || s.contains("rate limit")
        || s.contains("tokens per min")
        || s.contains("requests per min")
        || s == "pi_provider_rate_limit"
    {
        return "PI_PROVIDER_RATE_LIMIT";
    }
    if [
        "connection error",
        "econnreset",
        "etimedout",
        "timed out",
        "socket hang up",
        "fetch failed",
        "tls",
        "enotfound",
    ]
    .iter()
    .any(|v| s.contains(v))
        || s == "pi_provider_connection_error"
    {
        return "PI_PROVIDER_CONNECTION_ERROR";
    }
    if matches!(status, Some(500 | 502 | 503 | 504))
        || [
            "temporar",
            "overload",
            "capacity",
            "internal server error",
            "api_error",
        ]
        .iter()
        .any(|v| s.contains(v))
    {
        return "PI_PROVIDER_TEMPORARY_UNAVAILABLE";
    }
    "PI_PROVIDER_ERROR"
}
fn http_status(raw: &str) -> Option<u16> {
    let tokens = raw
        .split(|c: char| c.is_whitespace() || matches!(c, ':' | '=' | '"' | '\''))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    for (i, t) in tokens.iter().enumerate() {
        if i == 0
            || tokens[i - 1].eq_ignore_ascii_case("http")
            || tokens[i - 1].eq_ignore_ascii_case("status")
            || (i >= 2
                && tokens[i - 1].eq_ignore_ascii_case("code")
                && tokens[i - 2].eq_ignore_ascii_case("status"))
        {
            if t.len() == 3 {
                if let Ok(n) = t.parse::<u16>() {
                    if (400..600).contains(&n) {
                        return Some(n);
                    }
                }
            }
        }
    }
    None
}
pub(super) fn detail(raw: &str) -> Value {
    let raw = raw.chars().take(16384).collect::<String>();
    let code = code(&raw);
    let status = http_status(&raw);
    let body = raw
        .find('{')
        .and_then(|at| serde_json::from_str::<Value>(&raw[at..]).ok());
    let reason = body
        .as_ref()
        .and_then(|v| v.pointer("/error/message").or(v.get("message")))
        .and_then(Value::as_str)
        .unwrap_or(&raw);
    let lower = reason.to_ascii_lowercase();
    // Authentication material can appear in arbitrary provider diagnostics. Do
    // not attempt to preserve its surrounding sentence when a header/key is seen.
    let sensitive = [
        "authorization",
        "bearer ",
        "api_key",
        "apikey",
        "api-key",
        "api key",
        "password",
        "cookie:",
        "access_token",
        "refresh_token",
        "client_secret",
        "sk-",
    ]
    .iter()
    .any(|s| lower.contains(s));
    let mut redacted = sensitive || reason.chars().count() > 2048;
    let safe = if sensitive {
        "[provider response redacted: contains credential material]".to_string()
    } else {
        reason
            .split_whitespace()
            .map(|word| {
                if word.contains("://")
                    || word.starts_with("org-")
                    || word.contains(":\\")
                    || word.starts_with('/')
                    || word.chars().filter(|c| c.is_ascii_alphanumeric()).count() > 32
                    || word.starts_with("eyJ")
                {
                    redacted = true;
                    "[redacted]"
                } else {
                    word
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .filter(|c| !c.is_control())
            .take(2048)
            .collect()
    };
    let retryable = matches!(
        code,
        "PI_PROVIDER_RATE_LIMIT"
            | "PI_PROVIDER_CONNECTION_ERROR"
            | "PI_PROVIDER_TEMPORARY_UNAVAILABLE"
    ) && !lower.contains("certificate")
        && !lower.contains("cert_");
    json!({"code":code,"reason":safe,"reasonRedacted":redacted,"httpStatus":status,"retryable":retryable,"requiresOwnerAction":owner_action(code),"attentionLabel":if owner_action(code){Some("需要你处理")}else{None},"recovery":match code{"PI_PROVIDER_BILLING_REQUIRED"=>"CHECK_API_BILLING","PI_PROVIDER_QUOTA_EXHAUSTED"=>"CHECK_API_QUOTA","PI_PROVIDER_AUTHENTICATION_ERROR"=>"CHECK_API_AUTHENTICATION","PI_PROVIDER_RATE_LIMIT"=>"WAIT_THEN_CONTINUE_SAME_SESSION",_=>"CONTINUE_SAME_SESSION_AFTER_RECOVERY"}})
}
pub(super) fn message_detail(message: &Value) -> Option<Value> {
    let raw = message["errorMessage"].as_str().filter(|s| !s.is_empty())?;
    let mut result = detail(raw);
    if let Some(stored) = message["errorCode"]
        .as_str()
        .filter(|s| s.starts_with("PI_PROVIDER_"))
    {
        let code = code(stored);
        result["code"] = json!(code);
        result["requiresOwnerAction"] = json!(owner_action(code));
    }
    if let Some(status) = message["providerFailure"]["httpStatus"]
        .as_u64()
        .filter(|s| (400..600).contains(s))
    {
        result["httpStatus"] = json!(status);
    }
    if message["providerFailure"]["reasonRedacted"] == true {
        result["reasonRedacted"] = json!(true);
    }
    Some(result)
}
pub(super) fn task_detail(task: &router_core::store::pi::Task) -> Option<Value> {
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::{Mutex, OnceLock},
        time::SystemTime,
    };
    type Key = (PathBuf, String, String, i64, u64, SystemTime);
    static CACHE: OnceLock<Mutex<HashMap<Key, Value>>> = OnceLock::new();
    if task.state != "FAILED"
        || !task.file_verified
        || !task
            .error_code
            .as_deref()
            .is_some_and(|s| s.starts_with("PI_PROVIDER_"))
    {
        return None;
    }
    let file = Path::new(task.native_session_file.as_deref()?);
    let native = task.native_session_id.as_deref()?;
    let entry = task.result.as_ref()?["lastAssistant"]["id"].as_str()?;
    let metadata = std::fs::metadata(file).ok()?;
    let key = (
        file.to_path_buf(),
        native.to_string(),
        entry.to_string(),
        task.generation,
        metadata.len(),
        metadata.modified().ok()?,
    );
    if let Ok(cache) = CACHE.get_or_init(Default::default).lock() {
        if let Some(v) = cache.get(&key) {
            return Some(v.clone());
        }
    }
    super::session::verify_session_header(file, native, Path::new(&task.directory)).ok()?;
    let detail = super::history::failure_metadata(file, entry)?["providerFailure"].clone();
    if let Ok(mut cache) = CACHE.get_or_init(Default::default).lock() {
        if cache.len() >= 128 {
            cache.clear()
        }
        cache.insert(key, detail.clone());
    }
    Some(detail)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn billing_and_exhausted_quota_are_permanent_not_rate_limits() {
        let f = detail(
            r#"400 {"error":{"type":"invalid_request_error","message":"Your credit balance is too low to access the Anthropic API."}}"#,
        );
        assert_eq!(f["code"], "PI_PROVIDER_BILLING_REQUIRED");
        assert_eq!(f["httpStatus"], 400);
        assert_eq!(f["retryable"], false);
        assert_eq!(f["requiresOwnerAction"], true);
        assert!(f["reason"]
            .as_str()
            .unwrap()
            .contains("credit balance is too low"));
        assert_eq!(
            code("429 insufficient_quota"),
            "PI_PROVIDER_QUOTA_EXHAUSTED"
        );
        assert_eq!(code("Rate limit reached on tokens per min (TPM): Limit 1000000, Used 920014, Requested 141393. You can increase your rate limit by adding a payment method at https://platform.openai.com/account/billing."),"PI_PROVIDER_RATE_LIMIT");
    }
    #[test]
    fn provider_reason_is_bounded_and_drops_credentials() {
        for error in [
            "500 Authorization: Bearer PRIVATE_SECRET",
            "502 api_key=short-private",
            "400 sk-example-private-value",
            "503 cookie: session=secret",
        ] {
            let f = detail(error);
            assert_eq!(f["reasonRedacted"], true);
            assert!(!f["reason"].as_str().unwrap().contains("private"));
            assert!(!f["reason"].as_str().unwrap().contains("secret"));
        }
        let f=detail("Rate limit reached for model in organization org-private_account on tokens per min. Visit https://host.invalid/?token=private");
        assert!(!f["reason"].as_str().unwrap().contains("private"));
        assert!(f["reason"].as_str().unwrap().contains("tokens per min"));
    }
}
