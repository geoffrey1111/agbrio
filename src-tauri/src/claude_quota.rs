//! Claude Code subscription usage, read from the small file Agbrio's owner-enabled
//! status line script writes (`rate_limits` from Claude Code's status line input).
//! Read-only: never starts Claude Code, reads credentials or calls a provider.
use crate::codex_quota::{Bucket, Quota, Window};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 8 * 1024;

pub(crate) fn usage_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join("AIWorkRouter").join("data").join("claude-usage.json"))
}
fn now() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64 }
fn unavailable(reason: &'static str) -> Quota { Quota { status: "UNAVAILABLE", reason: Some(reason), observed_at: now(), buckets: vec![] } }

/// Projects only the two known windows; any other content in the file is ignored.
pub(crate) fn project(value: &Value) -> Quota {
    let observed_at = value.get("observedAt").and_then(Value::as_i64).filter(|n| *n > 0).unwrap_or(0);
    let mut windows = vec![];
    for (key, slot, minutes) in [("fiveHour", "primary", 300_i64), ("sevenDay", "secondary", 10_080)] {
        let Some(row) = value.get(key).filter(|v| v.is_object()) else { continue };
        let Some(used) = row.get("usedPercentage").and_then(Value::as_f64).filter(|n| n.is_finite() && (0.0..=100.0).contains(n)) else { continue };
        windows.push(Window {
            slot: slot.into(), used_percent: used, remaining_percent: 100.0 - used, window_duration_mins: Some(minutes),
            resets_at: row.get("resetsAt").and_then(Value::as_i64).filter(|n| *n > 0 && *n <= 253_402_300_799),
        });
    }
    if windows.is_empty() || observed_at == 0 { return unavailable("NO_USAGE_WINDOWS"); }
    Quota { status: "AVAILABLE", reason: None, observed_at, buckets: vec![Bucket { id: "claude".into(), windows }] }
}

pub(crate) fn read() -> Quota {
    let Some(path) = usage_path() else { return unavailable("NOT_ENABLED") };
    let Ok(meta) = std::fs::metadata(&path) else { return unavailable("NOT_ENABLED") };
    if !meta.is_file() || meta.len() > MAX_BYTES { return unavailable("NO_USAGE_WINDOWS"); }
    match std::fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()) {
        Some(value) => project(&value),
        None => unavailable("NO_USAGE_WINDOWS"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn projects_both_windows_with_their_real_durations() {
        let q = project(&json!({"observedAt":1_700_000_000_000_i64,"fiveHour":{"usedPercentage":23.5,"resetsAt":1_738_425_600},"sevenDay":{"usedPercentage":41.2,"resetsAt":1_738_857_600},"account":"never-shown"}));
        assert_eq!(q.status, "AVAILABLE");
        let w = &q.buckets[0].windows;
        assert_eq!((w[0].window_duration_mins, w[0].remaining_percent), (Some(300), 76.5));
        assert_eq!(w[1].window_duration_mins, Some(10_080));
        assert!(!serde_json::to_string(&q).unwrap().contains("never-shown"));
    }
    #[test]
    fn rejects_out_of_range_or_missing_data() {
        assert_eq!(project(&json!({"observedAt":1,"fiveHour":{"usedPercentage":140}})).status, "UNAVAILABLE");
        assert_eq!(project(&json!({"fiveHour":{"usedPercentage":10}})).status, "UNAVAILABLE");
        assert_eq!(project(&json!({})).reason, Some("NO_USAGE_WINDOWS"));
    }
}
