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
            used_tokens: None,
        });
    }
    if windows.is_empty() || observed_at == 0 { return unavailable("NO_USAGE_WINDOWS"); }
    Quota { status: "AVAILABLE", reason: None, observed_at, buckets: vec![Bucket { id: "claude".into(), windows }] }
}

fn status_line() -> Option<Quota> {
    let path = usage_path()?;
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > MAX_BYTES { return None; }
    let value = serde_json::from_slice::<Value>(&std::fs::read(&path).ok()?).ok()?;
    let quota = project(&value);
    // Exact percentages win only while the 5-hour window they describe is still current.
    let fresh = quota.buckets.first().and_then(|b| b.windows.first()).and_then(|w| w.resets_at).is_some_and(|reset| reset * 1000 > now());
    (quota.status == "AVAILABLE" && fresh).then_some(quota)
}

/// Exact status-line percentages when fresh; otherwise an estimate from Claude Code's own logs.
pub(crate) fn read() -> Quota {
    if let Some(exact) = status_line() { return exact; }
    let Some(root) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| PathBuf::from(h).join(".claude").join("projects")) else { return unavailable("NOT_ENABLED") };
    estimate(&root, now())
}

const HOUR_MS: i64 = 3_600_000;
const BLOCK_MS: i64 = 5 * HOUR_MS;
const WEEK_MS: i64 = 7 * 24 * HOUR_MS;
const MAX_LOG_BYTES: u64 = 256 * 1024 * 1024;

/// Days-from-civil (Howard Hinnant) for "YYYY-MM-DDTHH:MM:SS(.fff)Z" timestamps.
fn parse_utc_ms(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' { return None; }
    let n = |r: std::ops::Range<usize>| text.get(r)?.parse::<i64>().ok();
    let (y, m, d, hh, mm, ss) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    let ms = if b.get(19) == Some(&b'.') { text[20..].chars().take_while(|c| c.is_ascii_digit()).take(3).collect::<String>().parse::<i64>().unwrap_or(0) } else { 0 };
    let (y2, m2) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y2.div_euclid(400); let yoe = y2 - era * 400; let doy = (153 * m2 + 2) / 5 + d - 1; let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(((era * 146_097 + doe - 719_468) * 86_400 + hh * 3600 + mm * 60 + ss) * 1000 + ms)
}

/// Reads only the usage numbers and timestamps of assistant entries; message text is never kept.
fn entries(root: &std::path::Path, since: i64) -> Vec<(i64, u64)> {
    let mut seen = std::collections::HashSet::new(); let mut out = vec![]; let mut budget = MAX_LOG_BYTES;
    let Ok(projects) = std::fs::read_dir(root) else { return out };
    for project in projects.flatten() {
        let Ok(files) = std::fs::read_dir(project.path()) else { continue };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") { continue; }
            let Ok(meta) = file.metadata() else { continue };
            let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0);
            if modified < since || meta.len() > budget { continue; }
            budget -= meta.len();
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            for line in text.lines().filter(|l| l.contains("\"usage\"")) {
                let Ok(value) = serde_json::from_str::<Value>(line) else { continue };
                let Some(at) = value.get("timestamp").and_then(Value::as_str).and_then(parse_utc_ms) else { continue };
                if at < since { continue; }
                let Some(usage) = value.pointer("/message/usage") else { continue };
                // The same API response can be logged more than once; count it once.
                let key = format!("{}:{}", value.pointer("/message/id").and_then(Value::as_str).unwrap_or(""), value.get("requestId").and_then(Value::as_str).unwrap_or(""));
                if key != ":" && !seen.insert(key) { continue; }
                let tokens = ["input_tokens", "output_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"].iter().filter_map(|k| usage.get(*k).and_then(Value::as_u64)).sum::<u64>();
                if tokens > 0 { out.push((at, tokens)); }
            }
        }
    }
    out.sort_by_key(|(at, _)| *at); out
}

pub(crate) fn estimate(root: &std::path::Path, now_ms: i64) -> Quota {
    let rows = entries(root, now_ms - WEEK_MS - BLOCK_MS);
    if rows.is_empty() { return unavailable("NO_USAGE_WINDOWS"); }
    // 5-hour blocks start at the hour of their first entry, as Claude's session limits do.
    let (mut start, mut block_tokens) = (i64::MIN, 0u64);
    for (at, tokens) in &rows { if *at >= start.saturating_add(BLOCK_MS) { start = at - at.rem_euclid(HOUR_MS); block_tokens = 0; } block_tokens += tokens; }
    let current = now_ms < start + BLOCK_MS;
    let week = rows.iter().filter(|(at, _)| *at >= now_ms - WEEK_MS).map(|(_, t)| *t).sum::<u64>();
    let windows = vec![
        Window { slot: "primary".into(), used_percent: 0.0, remaining_percent: 0.0, window_duration_mins: Some(300), resets_at: current.then_some((start + BLOCK_MS) / 1000), used_tokens: Some(if current { block_tokens } else { 0 }) },
        Window { slot: "secondary".into(), used_percent: 0.0, remaining_percent: 0.0, window_duration_mins: Some(10_080), resets_at: None, used_tokens: Some(week) },
    ];
    Quota { status: "AVAILABLE", reason: Some("LOCAL_ESTIMATE"), observed_at: now(), buckets: vec![Bucket { id: "claude".into(), windows }] }
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
    fn estimates_the_current_block_and_week_from_local_logs_without_content() {
        let dir = tempfile::tempdir().unwrap(); let project = dir.path().join("p"); std::fs::create_dir(&project).unwrap();
        let now = parse_utc_ms("2026-10-19T12:30:00.000Z").unwrap();
        let line = |at: &str, id: &str, out: u64| format!("{{\"timestamp\":\"{at}\",\"requestId\":\"r{id}\",\"message\":{{\"id\":\"m{id}\",\"content\":\"secret text\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":{out}}}}}}}");
        let log = [line("2026-10-15T09:00:00.000Z","1",90), line("2026-10-19T10:20:00.000Z","2",190), line("2026-10-19T10:20:00.000Z","2",190), line("2026-10-19T12:10:00.000Z","3",290)].join("\n");
        std::fs::write(project.join("s.jsonl"), log).unwrap();
        let q = estimate(dir.path(), now);
        let w = &q.buckets[0].windows;
        assert_eq!(w[0].used_tokens, Some(500)); // block from 10:00: 200 + 300, duplicate counted once
        assert_eq!(w[0].resets_at, Some(parse_utc_ms("2026-10-19T15:00:00Z").unwrap() / 1000));
        assert_eq!(w[1].used_tokens, Some(600));
        assert!(!serde_json::to_string(&q).unwrap().contains("secret"));
    }
    #[test]
    fn rejects_out_of_range_or_missing_data() {
        assert_eq!(project(&json!({"observedAt":1,"fiveHour":{"usedPercentage":140}})).status, "UNAVAILABLE");
        assert_eq!(project(&json!({"fiveHour":{"usedPercentage":10}})).status, "UNAVAILABLE");
        assert_eq!(project(&json!({})).reason, Some("NO_USAGE_WINDOWS"));
    }
}
