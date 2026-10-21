//! Owner-requested full message flow for one exact Claude Code session: reads only
//! that session's own transcript (`~/.claude/projects/*/<session UUID>.jsonl`),
//! backwards from the end, a few turns per page. Keeps user prompts and assistant
//! text only; tool calls, tool output, reasoning, side-agent and meta lines are
//! skipped, and huge lines (inline images, tool output) are never parsed.
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const CHUNK: u64 = 1024 * 1024;
const PAGE_READ_LIMIT: u64 = 24 * 1024 * 1024;
const MAX_LINE: usize = 2 * 1024 * 1024;
const PAGE_TURNS: usize = 5;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Message { pub id: String, pub role: &'static str, pub text: String }
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Turn { pub id: String, pub messages: Vec<Message> }
#[derive(Debug, Default)]
pub(crate) struct Page { pub turns: Vec<Turn>, pub next: Option<u64>, pub cwd: Option<String> }

fn native(v: &str) -> bool { v.len() == 36 && uuid::Uuid::parse_str(v).is_ok() }

#[cfg(test)]
pub(crate) static TEST_PROJECTS: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
pub(crate) fn projects_root() -> Option<PathBuf> {
    #[cfg(test)] { return TEST_PROJECTS.lock().ok().and_then(|p| p.clone()); }
    #[allow(unreachable_code)]
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| PathBuf::from(h).join(".claude").join("projects"))
}
/// The transcript named by the exact native session id, in any project folder.
pub(crate) fn find(root: &Path, session: &str) -> Option<PathBuf> {
    if !native(session) { return None; }
    let name = format!("{session}.jsonl");
    std::fs::read_dir(root).ok()?.flatten().take(4096)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path().join(&name))
        .find(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file()))
}

/// Newer Claude Code stores an Agbrio-relayed prompt inside its own framing; show
/// the owner's original text.
fn unwrap_plugin_prompt(text: &str) -> String {
    const PREFIX: &str = "The agbrio-bridge plugin sent a message:\n";
    const SUFFIX: &str = "\n\nThis is how Claude Code surfaces a prompt a plugin submits";
    let Some(body) = text.strip_prefix(PREFIX) else { return text.to_owned() };
    body.rfind(SUFFIX).map_or(body, |end| &body[..end]).to_owned()
}
fn prompt_text(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            if items.iter().any(|i| i["type"] == "tool_result") { return None; }
            let texts: Vec<&str> = items.iter().filter(|i| i["type"] == "text").filter_map(|i| i["text"].as_str()).collect();
            let images = items.iter().filter(|i| i["type"] == "image").count();
            let mut text = texts.join("\n");
            if images > 0 { text = format!("{}{}[图片 ×{images}]", text, if text.is_empty() { "" } else { "\n" }); }
            text
        }
        _ => return None,
    };
    let text = unwrap_plugin_prompt(&text);
    let trimmed = text.trim_start();
    // Local slash commands and their output are not conversation messages.
    if trimmed.is_empty() || ["<command-", "<local-command-", "Caveat: The messages below"].iter().any(|p| trimmed.starts_with(p)) { return None; }
    Some(text)
}

/// One line, newest-first walk. Returns the parsed role for the caller's turn builder.
enum Line { Prompt { turn: String, id: String, text: String, cwd: Option<String> }, Reply { id: String, text: String }, Skip }
fn classify(line: &[u8], session: &str) -> Line {
    if line.len() > MAX_LINE { return Line::Skip; }
    let Ok(text) = std::str::from_utf8(line) else { return Line::Skip };
    let user = text.contains("\"type\":\"user\"");
    if !user && !text.contains("\"type\":\"assistant\"") { return Line::Skip; }
    if user && text.contains("\"tool_result\"") { return Line::Skip; }
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Line::Skip };
    if v["sessionId"].as_str() != Some(session) || v["isSidechain"] == true || v["isMeta"] == true { return Line::Skip; }
    let uuid = v["uuid"].as_str().unwrap_or_default().to_owned();
    match v["type"].as_str() {
        Some("user") => match prompt_text(&v["message"]["content"]) {
            Some(text) => Line::Prompt { turn: format!("claude_turn:{}", v["promptId"].as_str().filter(|p| !p.is_empty()).unwrap_or(&uuid)), id: uuid, text, cwd: v["cwd"].as_str().map(str::to_owned) },
            None => Line::Skip,
        },
        Some("assistant") => {
            let text = v["message"]["content"].as_array().map(|items| items.iter().filter(|i| i["type"] == "text").filter_map(|i| i["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
            if text.trim().is_empty() { return Line::Skip; }
            let id = v["message"]["id"].as_str().map(|m| format!("claude_msg:{m}")).unwrap_or(uuid);
            Line::Reply { id, text }
        }
        _ => Line::Skip,
    }
}

/// What the session's own transcript says about its latest turn.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Status { pub turn: String, pub state: &'static str, pub reply: Option<String>, pub completed_at: i64, pub cwd: Option<String> }
const STATUS_SCAN: u64 = 8 * 1024 * 1024;
/// Newest turn, read backwards: a turn ends with an `end_turn` assistant message (or
/// the stop-hook summary after it); every main-chain user row carries the turn's
/// `promptId`. The reply is the final `end_turn` message's text.
pub(crate) fn status(path: &Path, session: &str) -> Option<Status> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(STATUS_SCAN);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new(); file.take(STATUS_SCAN).read_to_end(&mut bytes).ok()?;
    let mut state: Option<&'static str> = None;
    let (mut reply, mut reply_id, mut completed_at): (Option<String>, Option<String>, i64) = (None, None, 0);
    let mut cwd: Option<String> = None;
    for line in bytes.split(|b| *b == b'\n').rev() {
        if line.len() > MAX_LINE || line.is_empty() { continue; }
        let Ok(text) = std::str::from_utf8(line) else { continue };
        if !text.contains(session) { continue; }
        let Ok(v) = serde_json::from_str::<Value>(text) else { continue };
        if v["sessionId"].as_str() != Some(session) || v["isSidechain"] == true { continue; }
        if cwd.is_none() { cwd = v["cwd"].as_str().map(str::to_owned); }
        match v["type"].as_str() {
            Some("system") if v["subtype"] == "stop_hook_summary" => { if state.is_none() { state = Some("COMPLETE"); } }
            Some("assistant") => {
                let stop = v["message"]["stop_reason"].as_str();
                // A turn also ends on an error the client writes itself (sign-in, rate
                // limit, API failure) or when the output is cut at a stop sequence/limit.
                let finished = matches!(stop, Some("end_turn" | "stop_sequence" | "max_tokens"));
                if state.is_none() { state = Some(if v["isApiErrorMessage"] == true { "FAILED" } else if finished { "COMPLETE" } else { "RUNNING" }); }
                if state == Some("COMPLETE") && finished {
                    let id = v["message"]["id"].as_str().map(str::to_owned);
                    let part = v["message"]["content"].as_array().map(|items| items.iter().filter(|i| i["type"] == "text").filter_map(|i| i["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
                    match (&reply_id, &reply) {
                        (None, _) if !part.trim().is_empty() => { reply = Some(part); reply_id = id; completed_at = v["timestamp"].as_str().and_then(crate::claude_quota::parse_utc_ms).unwrap_or(0); }
                        (Some(known), Some(existing)) if Some(known) == id.as_ref() && !part.trim().is_empty() => { reply = Some(format!("{part}\n{existing}")); }
                        _ => {}
                    }
                }
            }
            Some("user") if v["isMeta"] != true => {
                let content = &v["message"]["content"];
                let tool = content.as_array().is_some_and(|items| items.iter().any(|i| i["type"] == "tool_result"));
                let plain = match content { Value::String(s) => s.clone(), Value::Array(items) => items.iter().filter(|i| i["type"] == "text").filter_map(|i| i["text"].as_str()).collect::<Vec<_>>().join(""), _ => String::new() };
                if state.is_none() { state = Some(if !tool && plain.starts_with("[Request interrupted by user") { "INTERRUPTED" } else { "RUNNING" }); }
                let Some(turn) = v["promptId"].as_str().filter(|p| !p.is_empty()) else { continue };
                let state = match state { Some("COMPLETE") if reply.is_none() => "EMPTY", Some(s) => s, None => "RUNNING" };
                return Some(Status { turn: format!("claude_turn:{turn}"), state, reply: if state == "COMPLETE" { reply } else { None }, completed_at, cwd });
            }
            _ => {}
        }
    }
    None
}
const RECEIPT_SCAN: u64 = 16 * 1024 * 1024;
/// The native row a plugin submission produced: this session, origin
/// `agbrio-bridge`, written at or after the offer, unwrapped text exactly `text`.
pub(crate) fn find_plugin_prompt(path: &Path, session: &str, text: &str, since_ms: i64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(RECEIPT_SCAN);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new(); file.take(RECEIPT_SCAN).read_to_end(&mut bytes).ok()?;
    for line in bytes.split(|b| *b == b'\n').rev() {
        if line.len() > MAX_LINE { continue; }
        let Ok(line) = std::str::from_utf8(line) else { continue };
        if !line.contains("\"agbrio-bridge\"") || !line.contains("\"type\":\"user\"") { continue; }
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v["sessionId"].as_str() != Some(session) || v["origin"]["kind"] != "plugin" || v["origin"]["name"] != "agbrio-bridge" || v["isSidechain"] == true { continue; }
        let at = v["timestamp"].as_str().and_then(crate::claude_quota::parse_utc_ms).unwrap_or(0);
        if at + 60_000 < since_ms { continue; }
        let content = &v["message"]["content"];
        let raw = match content { Value::String(s) => s.clone(), Value::Array(items) => items.iter().filter(|i| i["type"] == "text").filter_map(|i| i["text"].as_str()).collect::<Vec<_>>().join(""), _ => continue };
        if unwrap_plugin_prompt(&raw) == text { return v["uuid"].as_str().filter(|u| native(u)).map(str::to_owned); }
    }
    None
}

/// Reads backwards from `before` (or the end) until `PAGE_TURNS` complete turns.
pub(crate) fn page(path: &Path, session: &str, before: Option<u64>) -> std::io::Result<Page> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let end = before.unwrap_or(len).min(len);
    let mut result = Page::default();
    // Assistant messages seen (newest first) since the last prompt.
    let mut pending: Vec<Message> = Vec::new();
    let mut tail: Vec<u8> = Vec::new(); // bytes after the last newline seen, belonging to the line in progress
    let mut pos = end;
    let mut read = 0u64;
    let flush = |line: &[u8], line_start: u64, result: &mut Page, pending: &mut Vec<Message>| -> bool {
        match classify(line, session) {
            Line::Reply { id, text } => {
                // Streamed content blocks of one API message share its id: join them.
                match pending.last_mut() { Some(m) if m.id == id => m.text = format!("{text}\n{}", m.text), _ => pending.push(Message { id, role: "assistant", text }) }
            }
            Line::Prompt { turn, id, text, cwd } => {
                if result.cwd.is_none() { result.cwd = cwd; }
                let mut messages = vec![Message { id, role: "user", text }];
                messages.extend(pending.drain(..).rev());
                result.turns.push(Turn { id: turn, messages });
                if result.turns.len() >= PAGE_TURNS { result.next = (line_start > 0).then_some(line_start); return true; }
            }
            Line::Skip => {}
        }
        false
    };
    'outer: while pos > 0 {
        if read >= PAGE_READ_LIMIT { result.next = Some(pos); break; }
        let start = pos.saturating_sub(CHUNK);
        let mut chunk = vec![0u8; (pos - start) as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut chunk)?;
        read += pos - start;
        chunk.extend_from_slice(&tail);
        let mut cut = chunk.len();
        while let Some(nl) = chunk[..cut].iter().rposition(|b| *b == b'\n') {
            let line = &chunk[nl + 1..cut];
            if !line.is_empty() && flush(line, start + nl as u64 + 1, &mut result, &mut pending) { break 'outer; }
            cut = nl;
        }
        tail = chunk[..cut].to_vec();
        if tail.len() > MAX_LINE * 2 { tail.clear(); } // an oversized line is skipped, not buffered
        pos = start;
    }
    if pos == 0 && result.turns.len() < PAGE_TURNS && result.next.is_none() {
        if !tail.is_empty() { let line = tail.clone(); flush(&line, 0, &mut result, &mut pending); }
        if !pending.is_empty() && result.turns.len() < PAGE_TURNS {
            result.turns.push(Turn { id: "claude_turn:start".into(), messages: pending.drain(..).rev().collect() });
        }
    } else if !pending.is_empty() && result.turns.is_empty() {
        // A single turn longer than the read limit: show what was read.
        result.turns.push(Turn { id: "claude_turn:partial".into(), messages: pending.drain(..).rev().collect() });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const S: &str = "10000000-0000-4000-8000-000000000001";
    fn user(prompt: &str, id: &str, text: &str) -> String { json!({"type":"user","sessionId":S,"promptId":prompt,"uuid":id,"cwd":"D:\\p","message":{"role":"user","content":text}}).to_string() }
    fn reply(msg: &str, text: &str) -> String { json!({"type":"assistant","sessionId":S,"uuid":format!("u-{msg}-{}",text.len()),"message":{"id":msg,"content":[{"type":"text","text":text}]}}).to_string() }
    #[test]
    fn pages_turns_newest_first_with_user_prompts_and_assistant_text_only() {
        let dir = tempfile::tempdir().unwrap();
        let mut lines = vec![];
        for n in 0..7 {
            lines.push(user(&format!("p{n}"), &format!("u{n}"), &format!("问题 {n}")));
            lines.push(json!({"type":"assistant","sessionId":S,"message":{"id":format!("t{n}"),"content":[{"type":"tool_use","name":"Bash","input":{"command":"secret"}},{"type":"thinking","thinking":"hidden"}]}}).to_string());
            lines.push(json!({"type":"user","sessionId":S,"message":{"content":[{"type":"tool_result","content":"tool output"}]}}).to_string());
            lines.push(reply(&format!("m{n}"), "第一段"));
            lines.push(reply(&format!("m{n}"), "第二段"));
            lines.push(json!({"type":"assistant","sessionId":S,"isSidechain":true,"message":{"id":"side","content":[{"type":"text","text":"side agent"}]}}).to_string());
            lines.push(user("cmd", &format!("c{n}"), "<command-name>/agbrio-status</command-name>"));
        }
        lines.push(json!({"type":"user","sessionId":"other","message":{"content":"other session"}}).to_string());
        let path = dir.path().join(format!("{S}.jsonl"));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let first = page(&path, S, None).unwrap();
        assert_eq!(first.turns.len(), 5);
        assert_eq!(first.turns[0].id, "claude_turn:p6");
        assert_eq!(first.turns[0].messages, vec![Message{id:"u6".into(),role:"user",text:"问题 6".into()}, Message{id:"claude_msg:m6".into(),role:"assistant",text:"第一段\n第二段".into()}]);
        assert_eq!(first.cwd.as_deref(), Some("D:\\p"));
        let all = serde_json::to_string(&format!("{:?}", first.turns)).unwrap();
        for hidden in ["secret", "hidden", "tool output", "side agent", "agbrio-status", "other session"] { assert!(!all.contains(hidden), "{hidden}"); }
        let second = page(&path, S, first.next).unwrap();
        assert_eq!(second.turns.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["claude_turn:p1", "claude_turn:p0"]);
        assert!(second.next.is_none());
        assert_eq!(find(dir.path(), S), None); // only project subfolders are searched
        let project = dir.path().join("D--p"); std::fs::create_dir(&project).unwrap(); std::fs::copy(&path, project.join(format!("{S}.jsonl"))).unwrap();
        assert_eq!(find(dir.path(), S), Some(project.join(format!("{S}.jsonl"))));
        assert_eq!(find(dir.path(), "../escape"), None);
    }
    #[test]
    fn finds_only_the_exact_plugin_submission_after_the_offer() {
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("t.jsonl");
        let row = |uuid: &str, ts: &str, origin: Value, text: &str| json!({"type":"user","sessionId":S,"uuid":uuid,"timestamp":ts,"origin":origin,"message":{"role":"user","content":format!("The agbrio-bridge plugin sent a message:\n{text}\n\nThis is how Claude Code surfaces a prompt a plugin submits between turns.")}}).to_string();
        let plugin = json!({"kind":"plugin","name":"agbrio-bridge"});
        let a = "aaaaaaaa-0000-4000-8000-000000000001"; let b = "bbbbbbbb-0000-4000-8000-000000000002"; let c = "cccccccc-0000-4000-8000-000000000003";
        std::fs::write(&path, [row(a, "2026-10-21T07:00:00.000Z", plugin.clone(), "测试发送"), row(b, "2026-10-21T07:12:57.837Z", json!({"kind":"human"}), "测试发送"), row(c, "2026-10-21T07:12:58.000Z", plugin, "测试发送")].join("\n")).unwrap();
        let offered = crate::claude_quota::parse_utc_ms("2026-10-21T07:12:57.000Z").unwrap();
        assert_eq!(find_plugin_prompt(&path, S, "测试发送", offered).as_deref(), Some(c));
        assert_eq!(find_plugin_prompt(&path, S, "别的内容", offered), None);
        assert_eq!(find_plugin_prompt(&path, S, "测试发送", offered + 3_600_000), None);
    }
    #[test]
    fn status_follows_the_newest_turn_from_prompt_to_end_turn() {
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("t.jsonl");
        let prompt = |p: &str| json!({"type":"user","sessionId":S,"promptId":p,"cwd":"D:\\p","message":{"role":"user","content":"做点事"}}).to_string();
        let tool_result = |p: &str| json!({"type":"user","sessionId":S,"promptId":p,"cwd":"D:\\p","message":{"content":[{"type":"tool_result","content":"x"}]}}).to_string();
        let step = |stop: &str, id: &str, text: &str| json!({"type":"assistant","sessionId":S,"timestamp":"2026-10-21T08:31:33.000Z","message":{"id":id,"stop_reason":stop,"content":[{"type":"text","text":text}]}}).to_string();
        let stop_hook = json!({"type":"system","subtype":"stop_hook_summary","sessionId":S}).to_string();
        let write = |lines: Vec<String>| { std::fs::write(&path, lines.join("\n")).unwrap(); status(&path, S).unwrap() };
        let running = write(vec![prompt("a"), step("tool_use", "m1", "看一下"), tool_result("a")]);
        assert_eq!((running.turn.as_str(), running.state, running.reply.clone()), ("claude_turn:a", "RUNNING", None));
        let done = write(vec![prompt("a"), step("tool_use", "m1", "看一下"), tool_result("a"), step("end_turn", "m2", "第一段"), step("end_turn", "m2", "第二段"), stop_hook.clone()]);
        assert_eq!((done.state, done.reply.as_deref(), done.cwd.as_deref()), ("COMPLETE", Some("第一段\n第二段"), Some("D:\\p")));
        assert!(done.completed_at > 0);
        let next = write(vec![prompt("a"), step("end_turn", "m2", "完成"), stop_hook, prompt("b")]);
        assert_eq!((next.turn.as_str(), next.state), ("claude_turn:b", "RUNNING"));
        let stopped = write(vec![prompt("c"), json!({"type":"user","sessionId":S,"promptId":"c","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}}).to_string()]);
        assert_eq!(stopped.state, "INTERRUPTED");
        let side = write(vec![prompt("d"), step("end_turn", "m3", "主回复"), json!({"type":"assistant","sessionId":S,"isSidechain":true,"message":{"id":"x","stop_reason":"tool_use","content":[{"type":"text","text":"子代理"}]}}).to_string()]);
        assert_eq!((side.state, side.reply.as_deref()), ("COMPLETE", Some("主回复")));
        let error = write(vec![prompt("e"), json!({"type":"assistant","sessionId":S,"isApiErrorMessage":true,"message":{"id":"x","model":"<synthetic>","stop_reason":"stop_sequence","content":[{"type":"text","text":"Not logged in"}]}}).to_string()]);
        assert_eq!((error.turn.as_str(), error.state, error.reply), ("claude_turn:e", "FAILED", None));
    }
    #[test]
    fn a_relayed_prompt_shows_the_owner_text_not_the_framing() {
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("t.jsonl");
        let framed = "The agbrio-bridge plugin sent a message:
测试发送

This is how Claude Code surfaces a prompt a plugin submits between turns.";
        std::fs::write(&path, [user("a","ua",framed), reply("ma","收到")].join("
")).unwrap();
        assert_eq!(page(&path, S, None).unwrap().turns[0].messages[0].text, "测试发送");
    }
    #[test]
    fn oversized_lines_are_skipped_without_losing_neighbours() {
        let dir = tempfile::tempdir().unwrap();
        let huge = json!({"type":"user","sessionId":S,"message":{"content":[{"type":"text","text":"x".repeat(3 * 1024 * 1024)}]}}).to_string();
        let path = dir.path().join("t.jsonl");
        std::fs::write(&path, [user("a","ua","前"), reply("ma","答"), huge, user("b","ub","后"), reply("mb","答2")].join("\n")).unwrap();
        let p = page(&path, S, None).unwrap();
        assert_eq!(p.turns.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["claude_turn:b", "claude_turn:a"]);
        assert_eq!(p.turns[1].messages[1].text, "答");
    }
}
