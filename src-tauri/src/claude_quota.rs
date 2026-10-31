//! Claude Code subscription usage, read from the small file Agbrio's owner-enabled
//! status line script writes (`rate_limits` from Claude Code's status line input).
//! Read-only: no independent CLI login, model calls, credential reads or live-task control.
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
    if let Some(native)=native_snapshot(){return native;}
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

#[derive(serde::Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]
pub(crate) struct NativeUsage{session_id:String,instance_id:String,cwd:String,observed_at:i64,windows:Vec<NativeWindow>}
#[derive(serde::Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]
struct NativeWindow{kind:String,percent_used:f64,resets_at:Option<i64>}
#[cfg(test)]pub(crate) static TEST_NATIVE_PATH:std::sync::Mutex<Option<PathBuf>>=std::sync::Mutex::new(None);
fn native_path()->Option<PathBuf>{
 #[cfg(test)]if let Some(path)=TEST_NATIVE_PATH.lock().ok().and_then(|p|p.clone()){return Some(path);}
 usage_path().map(|p|p.with_file_name("claude-desktop-usage.json"))}
fn native_value(input:&NativeUsage,at:i64)->Result<Value,String>{
 if input.observed_at<=0||input.observed_at>at+30_000||at-input.observed_at>120_000||input.windows.is_empty()||input.windows.len()>2{return Err("CLAUDE_USAGE_INVALID".into());}
 let mut value=serde_json::json!({"observedAt":input.observed_at});
 for row in &input.windows{let key=match row.kind.as_str(){"five_hour"=>"fiveHour","seven_day"=>"sevenDay",_=>return Err("CLAUDE_USAGE_INVALID".into())};
  if value.get(key).is_some()||!row.percent_used.is_finite()||!(0.0..=100.0).contains(&row.percent_used)||row.resets_at.is_some_and(|v|v<=0||v>253402300799){return Err("CLAUDE_USAGE_INVALID".into());}
  value[key]=serde_json::json!({"usedPercentage":row.percent_used,"resetsAt":row.resets_at});
 }Ok(value)
}
pub(crate) fn record_native(core:&crate::host_application::RouterCore,token:&str,input:NativeUsage)->Result<(),String>{
 static WRITE:std::sync::Mutex<()>=std::sync::Mutex::new(());let _guard=WRITE.lock().map_err(|_|"CLAUDE_USAGE_UNAVAILABLE")?;
 core.store.authorize_claude_owner_publisher(token,&input.session_id,&input.instance_id,&input.cwd)?;
 let value=native_value(&input,now())?;let path=native_path().ok_or("CLAUDE_USAGE_UNAVAILABLE")?;let parent=path.parent().ok_or("CLAUDE_USAGE_UNAVAILABLE")?;
 if std::fs::metadata(&path).ok().is_some_and(|m|m.len()<=MAX_BYTES){if let Ok(bytes)=std::fs::read(&path){if serde_json::from_slice::<Value>(&bytes).ok().and_then(|v|v["observedAt"].as_i64()).is_some_and(|at|at>=input.observed_at){return Ok(());}}}
 std::fs::create_dir_all(parent).map_err(|_|"CLAUDE_USAGE_UNAVAILABLE")?;
 let temp=parent.join(format!(".claude-desktop-usage-{}.tmp",uuid::Uuid::new_v4()));let bytes=serde_json::to_vec(&value).map_err(|_|"CLAUDE_USAGE_INVALID")?;
 let result=(||{use std::io::Write;let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|_|"CLAUDE_USAGE_UNAVAILABLE")?;file.write_all(&bytes).map_err(|_|"CLAUDE_USAGE_UNAVAILABLE")?;drop(file);std::fs::rename(&temp,&path).map_err(|_|"CLAUDE_USAGE_UNAVAILABLE")})();let _=std::fs::remove_file(&temp);result.map_err(str::to_owned)
}
fn native_snapshot()->Option<Quota>{
 use std::io::Read;let path=native_path()?;let file=std::fs::File::open(path).ok()?;if file.metadata().ok()?.len()>MAX_BYTES{return None;}let mut bytes=Vec::new();file.take(MAX_BYTES+1).read_to_end(&mut bytes).ok()?;if bytes.len() as u64>MAX_BYTES{return None;}let v:Value=serde_json::from_slice(&bytes).ok()?;let mut q=project(&v);let at=now();if q.status!="AVAILABLE"||q.observed_at>at+30_000||at-q.observed_at>300_000{return None;}
 // Never keep a native rate-limit window alive beyond its own reset.
 for bucket in &mut q.buckets{bucket.windows.retain(|w|w.resets_at.is_none_or(|r|r*1000>at));}if q.buckets.iter().all(|b|b.windows.is_empty()){return None;}q.reason=Some("NATIVE_DESKTOP");Some(q)
}
#[cfg(test)]mod native_usage_tests{use super::*;#[test]fn native_data_is_only_two_bounded_windows_without_identity(){let input:NativeUsage=serde_json::from_value(serde_json::json!({"sessionId":"private-session","instanceId":"private-instance","cwd":"private-root","observedAt":1000,"windows":[{"kind":"five_hour","percentUsed":27,"resetsAt":2000},{"kind":"seven_day","percentUsed":21}]})).unwrap();let value=native_value(&input,1001).unwrap();let q=project(&value);assert_eq!(q.buckets[0].windows[0].remaining_percent,73.0);assert_eq!(q.buckets[0].windows[1].remaining_percent,79.0);assert!(!value.to_string().contains("private"));assert!(native_value(&input,200000).is_err());}}

#[cfg(test)]mod native_http_tests{
 use super::*;use std::sync::{Arc,Mutex};use serde_json::{json,Value};
 #[test]#[ignore="isolated native quota publisher/authenticated phone reader; run alone, no provider"]
 fn original_desktop_account_quota_reaches_phone_without_cli_login(){
 let d=tempfile::tempdir().unwrap();let root=d.path().to_string_lossy().into_owned();let store=Arc::new(crate::RouterStore::open_at(d.path().join("router.db")).unwrap());let core=crate::host_application::RouterCore{chatgpt:Arc::default(),session:Arc::new(Mutex::new(crate::host_application::Session::default())),completed_chatgpt_responses:Arc::default(),store:store.clone()};
 let sid="10000000-0000-4000-8000-000000000001";let instance="20000000-0000-4000-8000-000000000001";let pair=store.create_claude_connection().unwrap();let event=router_core::claude_code::Event{event_id:uuid::Uuid::new_v4().to_string(),session_id:sid.into(),instance_id:instance.into(),sequence:1,cwd:root.clone(),version:"2.1.309".into(),kind:"SESSION_START".into(),turn_id:None,text:None,completed_at:None};store.ingest_claude_event(&pair.token,&event).unwrap();
 struct Reset;impl Drop for Reset{fn drop(&mut self){*TEST_NATIVE_PATH.lock().unwrap()=None;}}let path=d.path().join("desktop-usage.json");*TEST_NATIVE_PATH.lock().unwrap()=Some(path.clone());let _reset=Reset;
 let host=crate::HostRuntime::default();let auth=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());let code=auth.issue().unwrap();let session=auth.exchange(&code.code,"phone",false).unwrap();*host.test_web_auth.lock().unwrap()=Some(auth);
 host.ensure_mobile(core.clone(),crate::mobile_http::MobileHttpConfig{port:0,allowed_host:"quota.fixture.invalid".into(),allowed_origin:"https://quota.fixture.invalid".into(),access_issuer:String::new(),access_audience:String::new(),access_jwks_url:String::new(),static_dir:d.path().into()});let base=format!("http://{}",host.mobile_address().unwrap());let client=reqwest::blocking::Client::new();
 let body=json!({"sessionId":sid,"instanceId":instance,"cwd":root,"observedAt":now(),"windows":[{"kind":"five_hour","percentUsed":27,"resetsAt":now()/1000+3600},{"kind":"seven_day","percentUsed":21}]});let url=format!("{base}/v1/claude-code/usage");
 assert_eq!(client.post(&url).header("host","quota.fixture.invalid").json(&body).send().unwrap().status(),reqwest::StatusCode::UNAUTHORIZED);
 assert_eq!(client.post(&url).header("host","quota.fixture.invalid").header("origin","https://quota.fixture.invalid").bearer_auth(&pair.token).json(&body).send().unwrap().status(),reqwest::StatusCode::FORBIDDEN);
 let post=|v:&Value|client.post(&url).header("host","quota.fixture.invalid").bearer_auth(&pair.token).json(v).send().unwrap();let mut wrong=body.clone();wrong["instanceId"]=json!("30000000-0000-4000-8000-000000000001");assert!(!post(&wrong).status().is_success());assert!(post(&body).status().is_success());
 let quota:Value=client.get(format!("{base}/v1/mobile/claude-quota")).header("host","quota.fixture.invalid").header("cookie",format!("{}={session}",crate::web_auth::COOKIE)).send().unwrap().json().unwrap();assert_eq!(quota["reason"],"NATIVE_DESKTOP");assert_eq!(quota["buckets"][0]["windows"][0]["remainingPercent"],73.0);assert_eq!(quota["buckets"][0]["windows"][1]["remainingPercent"],79.0);
 let bytes=std::fs::read(&path).unwrap();let mut old=body.clone();old["observedAt"]=json!(now()-60000);assert!(post(&old).status().is_success());assert_eq!(std::fs::read(&path).unwrap(),bytes);assert!(!String::from_utf8(bytes).unwrap().contains(sid));assert!(core.session.lock().unwrap().adapter.is_none());host.shutdown(&core);
 }
}

#[cfg(test)]mod owner_native_cache_acceptance{use super::*;#[test]#[ignore="explicit owner read-only current native quota cache; no auth/model or writes"]fn current_desktop_session_metadata_projects_as_available_quota(){let q=native_snapshot().expect("fresh actual Desktop native quota sample");assert_eq!(q.status,"AVAILABLE");assert_eq!(q.reason,Some("NATIVE_DESKTOP"));assert!(!q.buckets[0].windows.is_empty());eprintln!("{}",serde_json::to_string(&q).unwrap());}}
