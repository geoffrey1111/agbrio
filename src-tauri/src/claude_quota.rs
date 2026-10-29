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

/// Exact status-line percentages when fresh; then the percentages the Claude desktop app
/// itself records; otherwise an estimate from Claude Code's own logs.
pub(crate) fn read() -> Quota {
    if let Some(exact) = status_line() { return exact; }
    // Use the adapter's verified Desktop roots, including the Store app's
    // redirected LocalCache. Read only its bounded usage file, never credentials.
    let roots = crate::claude_code_desktop::data_roots();
    if let Some(desktop) = desktop_at(&roots, None, now()) { return desktop; }
    let logs = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| PathBuf::from(h).join(".claude").join("projects"));
    logs.as_deref().map(|root| estimate(root, now())).unwrap_or_else(|| unavailable("NOT_ENABLED"))
}

fn desktop_at(roots: &[PathBuf], local: Option<&Quota>, now_ms: i64) -> Option<Quota> {
    roots.iter().filter_map(|root| {
        let path = root.join("plan-usage-history.json");
        let meta = std::fs::metadata(&path).ok()?;
        if !meta.is_file() || meta.len() > MAX_HISTORY_BYTES { return None; }
        let file = std::fs::File::open(path).ok()?;
        use std::io::Read;
        let mut bytes = Vec::new();
        file.take(MAX_HISTORY_BYTES + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 > MAX_HISTORY_BYTES { return None; }
        desktop_history(&serde_json::from_slice::<Value>(&bytes).ok()?, local, now_ms)
    }).max_by_key(|quota| quota.observed_at)
}

const MAX_HISTORY_BYTES: u64 = 4 * 1024 * 1024;
const DESKTOP_FRESH_MS: i64 = 6 * HOUR_MS;

/// The Claude desktop app samples plan utilization (`fh` five-hour, `sd` seven-day percent)
/// into its own history file. Only the newest sample's two numbers are used; the
/// organization id is never returned. The five-hour reset comes from the local log block.
pub(crate) fn desktop_history(history: &Value, local: Option<&Quota>, now_ms: i64) -> Option<Quota> {
    let sample = history.get("samples")?.as_array()?.iter().filter(|s| s.get("t").and_then(Value::as_i64).is_some()).max_by_key(|s| s["t"].as_i64())?;
    let at = sample["t"].as_i64()?;
    if at > now_ms + 60_000 || now_ms - at > DESKTOP_FRESH_MS { return None; }
    let percent = |key: &str| sample.pointer(&format!("/u/{key}")).and_then(Value::as_f64).filter(|n| n.is_finite() && (0.0..=100.0).contains(n));
    let reset = local.and_then(|q| q.buckets.first()).and_then(|b| b.windows.first()).and_then(|w| w.resets_at);
    let mut windows = vec![];
    for (key, slot, minutes, resets_at) in [("fh", "primary", 300_i64, reset), ("sd", "secondary", 10_080, None)] {
        let Some(used) = percent(key) else { continue };
        windows.push(Window { slot: slot.into(), used_percent: used, remaining_percent: 100.0 - used, window_duration_mins: Some(minutes), resets_at, used_tokens: None });
    }
    (!windows.is_empty()).then(|| Quota { status: "AVAILABLE", reason: Some("DESKTOP_APP"), observed_at: at, buckets: vec![Bucket { id: "claude".into(), windows }] })
}

const HOUR_MS: i64 = 3_600_000;
const BLOCK_MS: i64 = 5 * HOUR_MS;
const WEEK_MS: i64 = 7 * 24 * HOUR_MS;
const MAX_LOG_BYTES: u64 = 256 * 1024 * 1024;

/// Days-from-civil (Howard Hinnant) for "YYYY-MM-DDTHH:MM:SS(.fff)Z" timestamps.
pub(crate) fn parse_utc_ms(text: &str) -> Option<i64> {
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
    fn uses_the_newest_desktop_sample_without_the_organization() {
        let now = 1_792_427_500_000_i64;
        let history = json!({"version":2,"samples":[{"t":now-7_200_000,"org":"org-secret","u":{"fh":10,"sd":20}},{"t":now-40_000,"org":"org-secret","u":{"fh":78,"sd":83}}]});
        let q = desktop_history(&history, None, now).unwrap();
        let w = &q.buckets[0].windows;
        assert_eq!((w[0].used_percent, w[1].used_percent, q.observed_at), (78.0, 83.0, now - 40_000));
        assert!(!serde_json::to_string(&q).unwrap().contains("org-secret"));
        assert!(desktop_history(&history, None, now + DESKTOP_FRESH_MS).is_none()); // stale
        assert!(desktop_history(&json!({"samples":[{"t":now,"u":{"fh":140}}]}), None, now).is_none());
    }
    #[test]
    fn rejects_out_of_range_or_missing_data() {
        assert_eq!(project(&json!({"observedAt":1,"fiveHour":{"usedPercentage":140}})).status, "UNAVAILABLE");
        assert_eq!(project(&json!({"fiveHour":{"usedPercentage":10}})).status, "UNAVAILABLE");
        assert_eq!(project(&json!({})).reason, Some("NO_USAGE_WINDOWS"));
    }
}

#[cfg(test)]
mod desktop_path_tests {
    use super::*;
    #[test]
    fn store_history_is_used_when_legacy_history_is_absent_or_older() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("Roaming/Claude");
        let store = dir.path().join("Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude");
        std::fs::create_dir_all(&store).unwrap();
        let now = 1_793_244_008_338_i64;
        let history = |at| serde_json::json!({"samples":[{"t":at,"u":{"fh":5,"sd":15}}]}).to_string();
        std::fs::write(store.join("plan-usage-history.json"), history(now)).unwrap();
        let roots = vec![legacy.clone(), store.clone()];
        let actual = desktop_at(&roots, None, now).unwrap();
        assert_eq!(actual.buckets[0].windows[0].remaining_percent, 95.0);
        assert_eq!(actual.buckets[0].windows[1].remaining_percent, 85.0);
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("plan-usage-history.json"), history(now - 1000)).unwrap();
        assert_eq!(desktop_at(&roots, None, now).unwrap().observed_at, now);
        assert!(desktop_at(&roots, None, now + DESKTOP_FRESH_MS + 1).is_none());
        std::fs::write(store.join("plan-usage-history.json"), "invalid").unwrap();
        assert_eq!(desktop_at(&roots, None, now).unwrap().observed_at, now - 1000);
    }
}
