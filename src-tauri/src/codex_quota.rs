//! Read-only account usage from the existing adapter. Never start a provider,
//! read credentials, return account identity, or consume reset credits.
use crate::RouterCore;
use serde::Serialize;
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Window {
    pub slot: String,
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub window_duration_mins: Option<i64>,
    pub resets_at: Option<i64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Bucket { pub id: String, pub windows: Vec<Window> }
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Quota {
    pub status: &'static str,
    pub reason: Option<&'static str>,
    pub observed_at: i64,
    pub buckets: Vec<Bucket>,
}
fn now() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64 }
fn unavailable(reason: &'static str) -> Quota { Quota { status: "UNAVAILABLE", reason: Some(reason), observed_at: now(), buckets: vec![] } }
pub(crate) fn project(value: &Value) -> Quota {
    let mut buckets = vec![];
    let fallback = value.get("rateLimits").filter(|v| v.is_object());
    let mut rows: Vec<(String, &Value)> = value.get("rateLimitsByLimitId").and_then(Value::as_object)
        .map(|rows| rows.iter().map(|(id, v)| (id.clone(), v)).collect()).unwrap_or_default();
    if let Some(v) = fallback {
        let id = v.get("limitId").and_then(Value::as_str).unwrap_or("codex");
        if !rows.iter().any(|(key, _)| key == id) { rows.push((id.into(), v)); }
    }
    for (id, row) in rows {
        // Limit identifiers only; upstream arbitrary names/plan/account fields
        // and the complete response must never leak through this projection.
        if id.is_empty() || id.len() > 100 || !id.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c)) { continue; }
        let mut windows = vec![];
        for slot in ["primary", "secondary"] {
            let Some(v) = row.get(slot).filter(|v| v.is_object()) else { continue; };
            let Some(used) = v.get("usedPercent").and_then(Value::as_f64).filter(|n| n.is_finite() && (0.0..=100.0).contains(n)) else { continue; };
            windows.push(Window {
                slot: slot.into(), used_percent: used, remaining_percent: 100.0 - used,
                window_duration_mins: v.get("windowDurationMins").and_then(Value::as_i64).filter(|n| *n > 0),
                resets_at: v.get("resetsAt").and_then(Value::as_i64).filter(|n| *n > 0 && *n <= 253402300799),
            });
        }
        if !windows.is_empty() { buckets.push(Bucket { id, windows }); }
    }
    if buckets.is_empty() { return unavailable("NO_USAGE_WINDOWS"); }
    Quota { status: "AVAILABLE", reason: None, observed_at: now(), buckets }
}
pub(crate) fn read(core: &RouterCore) -> Quota {
    let Ok(mut session) = core.session.try_lock() else { return unavailable("BACKEND_BUSY"); };
    let Some(adapter) = session.adapter.as_mut().filter(|a| !a.is_closed()) else { return unavailable("BACKEND_OFFLINE"); };
    let budget = Duration::from_secs(4);
    let Ok(before) = adapter.request_with_timeout("account/read", json!({"refreshToken":false}), budget) else { return unavailable("ACCOUNT_UNAVAILABLE"); };
    if before.pointer("/account/type").and_then(Value::as_str) != Some("chatgpt") { return unavailable("ACCOUNT_UNSUPPORTED"); }
    let Ok(usage) = adapter.request_with_timeout("account/rateLimits/read", json!({}), budget) else { return unavailable("USAGE_UNAVAILABLE"); };
    let Ok(after) = adapter.request_with_timeout("account/read", json!({"refreshToken":false}), budget) else { return unavailable("ACCOUNT_UNAVAILABLE"); };
    if before.get("account") != after.get("account") { return unavailable("ACCOUNT_CHANGED"); }
    project(&usage)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_native_windows_keep_their_real_period_and_hide_private_fields() {
        let quota = project(&json!({"accountId":"private-account", "rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":36.0,"windowDurationMins":10080,"resetsAt":1791540000},"secondary":null,"credits":{"balance":"private-balance"}}},"rateLimits":{"limitId":"codex","primary":{"usedPercent":36,"windowDurationMins":10080}}}));
        assert_eq!(quota.buckets.len(),1);assert_eq!(quota.buckets[0].windows.len(),1);
        let window=&quota.buckets[0].windows[0];assert_eq!(window.remaining_percent,64.0);assert_eq!(window.window_duration_mins,Some(10080));assert_eq!(window.resets_at,Some(1791540000));
        let serialized=serde_json::to_string(&quota).unwrap();assert!(!serialized.contains("private"));assert!(!serialized.contains("credits"));
    }
    #[test]
    fn multiple_buckets_and_zero_are_not_missing_and_invalid_is_not_zero() {
        let q=project(&json!({"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":100,"windowDurationMins":300},"secondary":{"usedPercent":0,"windowDurationMins":10080}},"codex-other":{"primary":{"usedPercent":8}},"bad":{"primary":{"usedPercent":101}}}}));
        assert_eq!(q.buckets.len(),2);assert_eq!(q.buckets[0].windows[0].remaining_percent,0.0);assert_eq!(q.buckets[0].windows[1].remaining_percent,100.0);
        assert_eq!(project(&json!({"rateLimits":{"primary":{"usedPercent":null}}})).status,"UNAVAILABLE");
    }
    #[test]
    #[ignore = "explicit read-only check against the resident shared Codex; no provider launch"]
    fn resident_adapter_read_only_quota() {
        use std::sync::{Arc,Mutex};
        assert_eq!(std::env::var("AGBRIO_READONLY_QUOTA_ACCEPTANCE").as_deref(),Ok("1"));
        let command=crate::shared_codex::transport_command().unwrap().expect("Existing resident shared Codex is required");
        let mut adapter=crate::codex::adapter::CodexAdapter::start_shared_validation(command,Arc::new(|_|{})).unwrap();
        adapter.initialize().unwrap();
        let directory=tempfile::tempdir().unwrap();
        let core=RouterCore {store:Arc::new(crate::RouterStore::open_at(directory.path().join("quota-only.db")).unwrap()),chatgpt:Arc::default(),session:Arc::new(Mutex::new(crate::Session{adapter:Some(adapter),..Default::default()})),completed_chatgpt_responses:Arc::default()};
        let q=read(&core);assert_eq!(q.status,"AVAILABLE");assert!(!q.buckets.is_empty());
        let counts=core.session.lock().unwrap().adapter.as_ref().unwrap().request_counts();
        assert_eq!(counts.get("account/rateLimits/read"),Some(&1));assert_eq!(counts.get("account/read"),Some(&2));
        assert!(!counts.keys().any(|k|k.starts_with("turn/")||k.starts_with("thread/")));
        println!("QUOTA_RESIDENT_ADAPTER_READ_ONLY_PASS buckets={} private_identity_exported=false send_calls=0",q.buckets.len());
    }
}
