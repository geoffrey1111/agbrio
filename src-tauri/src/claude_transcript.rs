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
pub(crate) struct Message { pub id: String, pub role: &'static str, pub text: String, pub kind: &'static str, pub card:Option<Value>, pub at: i64, pub order: u64 }
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Turn { pub id: String, pub messages: Vec<Message> }
#[derive(Debug, Default, Clone)]
pub(crate) struct Page { pub turns: Vec<Turn>, pub next: Option<u64>, pub cwd: Option<String>, pub version:u64 }

/// Shared unchanged-file projection for list preview and the live reader.
pub(crate) fn latest(session:&str)->Option<Page>{
    use std::sync::{Mutex,OnceLock};
    type Cache=std::collections::HashMap<PathBuf,((u64,std::time::SystemTime),Page)>;
    static CACHE:OnceLock<Mutex<Cache>>=OnceLock::new();
    let path=find(&projects_root()?,session)?;let meta=std::fs::metadata(&path).ok()?;
    let stamp=(meta.len(),meta.modified().ok()?);let mut cache=CACHE.get_or_init(||Mutex::new(Default::default())).lock().ok()?;
    if let Some((old,p))=cache.get(&path){if *old==stamp{return Some(p.clone());}}
    let p=page(&path,session,None).ok()?;if cache.len()>32{cache.clear();}cache.insert(path,(stamp,p.clone()));Some(p)
}

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
fn local_command(text:&str)->bool {
    ["<command-", "<local-command-", "Caveat: The messages below"].iter().any(|p|text.trim_start().starts_with(p))
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
    if trimmed.is_empty() || local_command(trimmed) { return None; }
    Some(text)
}

/// One line, newest-first walk. Returns the parsed role for the caller's turn builder.
enum Line { Prompt { turn: String, id: String, text: String, cwd: Option<String>, kind: &'static str, at: i64 }, Reply { id: String, text: String, card:Option<Value>, at: i64 }, Skip }
#[cfg(test)]
pub(crate) fn reference_ids(path:&Path,session:&str)->std::collections::HashSet<String>{
    assert!(std::fs::metadata(path).unwrap().len()<=128*1024*1024,"bounded read-only acceptance input");
    let bytes=std::fs::read(path).unwrap();bytes.split(|b|*b==b'\n').filter_map(|line|match classify(line,session){Line::Prompt{id,..}|Line::Reply{id,..}=>Some(id),Line::Skip=>None}).collect()
}
fn collaboration(v: &Value, text: &str) -> bool {
    v["origin"]["kind"] == "peer" || v["turnOrigin"] == "peer" ||
    (v["promptSource"] == "sdk" && v["turnOrigin"] == "sdk" && text.starts_with("<system-reminder>\nThe session "))
}
fn classify(line: &[u8], session: &str) -> Line {
    if line.len() > MAX_LINE { return Line::Skip; }
    let Ok(text) = std::str::from_utf8(line) else { return Line::Skip };
    let user = text.contains("\"type\":\"user\"");
    if !user && !text.contains("\"type\":\"assistant\"") { return Line::Skip; }
    if user && text.contains("\"tool_result\"") { return Line::Skip; }
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Line::Skip };
    if v["sessionId"].as_str() != Some(session) || v["isSidechain"] == true || v["isCompactSummary"]==true { return Line::Skip; }
    let uuid = v["uuid"].as_str().unwrap_or_default().to_owned();
    let at = v["timestamp"].as_str().and_then(crate::claude_quota::parse_utc_ms).unwrap_or(0);
    match v["type"].as_str() {
        Some("user") => match prompt_text(&v["message"]["content"]) {
            Some(text) => { let internal=collaboration(&v,&text);if v["isMeta"]==true&&!internal{return Line::Skip;} let kind=if internal{"COLLABORATION"}else{"PUBLIC"}; Line::Prompt { turn: format!("claude_turn:{}", v["promptId"].as_str().filter(|p| !p.is_empty()).unwrap_or(&uuid)), id: uuid, text, cwd: v["cwd"].as_str().map(str::to_owned), kind, at } },
            None => Line::Skip,
        },
        Some("assistant") => {
            if v["isMeta"]==true{return Line::Skip;}
            let text = v["message"]["content"].as_array().map(|items| items.iter().filter(|i| i["type"] == "text").filter_map(|i| i["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
            // Desktop's wrap_up is native user-facing presentation, not tool chatter.
            // Read only the exact built-in name and documented visible fields.
            let wrap=v["message"]["content"].as_array().and_then(|items|items.iter().find(|b|b["type"]=="tool_use"&&b["name"]=="mcp__ccd_turn__wrap_up"));
            let card=wrap.map(|b|{let input=&b["input"];let actions=input["actions"].as_array().map(|a|a.iter().take(12).filter_map(|a|Some(serde_json::json!({"label":a["label"].as_str()?.chars().take(2000).collect::<String>(),"message":a["message"].as_str()?.chars().take(100000).collect::<String>()}))).collect::<Vec<_>>()).unwrap_or_default();serde_json::json!({"id":b["id"],"question":input["ask"].as_str(),"actions":actions})});
            let recap=wrap.and_then(|b|b["input"]["recap"].as_str()).unwrap_or("");
            let text=if text.trim().is_empty(){recap.to_owned()}else{text};
            if text.trim().is_empty()&&card.is_none() { return Line::Skip; }
            let id = v["message"]["id"].as_str().map(|m| format!("claude_msg:{m}")).unwrap_or(uuid);
            Line::Reply { id, text, card, at }
        }
        _ => Line::Skip,
    }
}

/// What the session's own transcript says about its latest turn.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Status { pub turn: String, pub state: &'static str, pub reply: Option<String>, pub completed_at: i64, pub cwd: Option<String> }
const STATUS_SCAN: u64 = 8 * 1024 * 1024;
// Desktop ends a next-step turn with this exact successful built-in result,
// without an assistant end_turn row. Pair it with its own wrap_up call first.
const CARD_TURN_END: &str = "Shown in the next-step box. The box ends your turn: write no text after this call, since the recap and question already say what the user needs.";
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
    let mut local_turn:Option<String>=None;
    let mut skipped_large=false;
    let mut card_end:Option<(String,String,i64)>=None;
    for line in bytes.split(|b| *b == b'\n').rev() {
        if line.len() > MAX_LINE {skipped_large=true;continue;}
        if line.is_empty() { continue; }
        let Ok(text) = std::str::from_utf8(line) else { continue };
        if !text.contains(session) { continue; }
        let Ok(v) = serde_json::from_str::<Value>(text) else { continue };
        if v["sessionId"].as_str() != Some(session) || v["isSidechain"] == true || v["isCompactSummary"]==true { continue; }
        if cwd.is_none() { cwd = v["cwd"].as_str().map(str::to_owned); }
        if let Some((tool,turn,at))=&card_end {
            if matches!(v["type"].as_str(),Some("assistant"|"user"|"api-request")) {
                let wrap=v["message"]["content"].as_array().filter(|_|v["isMeta"]!=true&&v["isApiErrorMessage"]!=true).filter(|items|items.iter().filter(|b|b["type"]=="tool_use").count()==1).and_then(|items|items.iter().find(|b|b["type"]=="tool_use"&&b["name"]=="mcp__ccd_turn__wrap_up"&&b["id"].as_str()==Some(tool)));
                let input=wrap.map(|b|&b["input"]);
                let recap=input.and_then(|i|i["recap"].as_str()).filter(|s|!s.trim().is_empty());
                let pending=input.is_some_and(|i|i["ask"].as_str().is_some_and(|s|!s.trim().is_empty())||i["actions"].as_array().is_some_and(|a|a.iter().any(|a|a["label"].as_str().is_some_and(|s|!s.trim().is_empty())&&a["message"].as_str().is_some_and(|s|!s.trim().is_empty()))));
                return Some(Status{turn:turn.clone(),state:if pending&&recap.is_some(){"COMPLETE"}else{"RUNNING"},reply:recap.filter(|_|pending).map(str::to_owned),completed_at:if pending&&recap.is_some(){*at}else{0},cwd});
            }
        }
        match v["type"].as_str() {
            Some("api-request") => { if state.is_none(){state=Some("RUNNING");} }
            Some("system") if v["subtype"] == "stop_hook_summary" => { if state.is_none() { state = Some("COMPLETE"); } }
            Some("assistant") => {
                let stop = v["message"]["stop_reason"].as_str();
                // A turn also ends on an error the client writes itself (sign-in, rate
                // limit, API failure) or when the output is cut at a stop sequence/limit.
                let finished = matches!(stop, Some("end_turn" | "stop_sequence" | "max_tokens"));
                if state.is_none() { state = Some(if v["isApiErrorMessage"] == true { "FAILED" } else if finished { "COMPLETE" } else { "RUNNING" }); }
                let recap=v["message"]["content"].as_array().and_then(|items|items.iter().find(|b|b["type"]=="tool_use"&&b["name"]=="mcp__ccd_turn__wrap_up")).and_then(|b|b["input"]["recap"].as_str());
                if state == Some("COMPLETE") && (finished||recap.is_some()) {
                    let id = v["message"]["id"].as_str().map(str::to_owned);
                    let part = v["message"]["content"].as_array().map(|items| items.iter().filter(|i| i["type"] == "text").filter_map(|i| i["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
                    let part=if part.trim().is_empty(){recap.unwrap_or("").to_owned()}else{part};
                    match (&reply_id, &reply) {
                        (None, _) if !part.trim().is_empty() => { reply = Some(part); reply_id = id; completed_at = v["timestamp"].as_str().and_then(crate::claude_quota::parse_utc_ms).unwrap_or(0); }
                        (Some(known), Some(existing)) if Some(known) == id.as_ref() && !part.trim().is_empty() => { reply = Some(format!("{part}\n{existing}")); }
                        _ => {}
                    }
                }
            }
            Some("user") => {
                let content = &v["message"]["content"];
                if state.is_none()&&v["isMeta"]!=true {
                    if let (Some(items),Some(turn))=(content.as_array().filter(|items|items.len()==1),v["promptId"].as_str().filter(|p|!p.is_empty())) {
                        let result=&items[0];
                        let text=result["content"].as_str().or_else(||result["content"].as_array().filter(|a|a.len()==1&&a[0]["type"]=="text").and_then(|a|a[0]["text"].as_str()));
                        if result["type"]=="tool_result"&&result["is_error"]!=true&&text==Some(CARD_TURN_END) {
                            if let Some(tool)=result["tool_use_id"].as_str().filter(|s|!s.is_empty()) {card_end=Some((tool.into(),format!("claude_turn:{turn}"),v["timestamp"].as_str().and_then(crate::claude_quota::parse_utc_ms).unwrap_or(0)));continue;}
                        }
                    }
                }
                let tool = content.as_array().is_some_and(|items| items.iter().any(|i| i["type"] == "tool_result"));
                let plain = match content { Value::String(s) => s.clone(), Value::Array(items) => items.iter().filter(|i| i["type"] == "text").filter_map(|i| i["text"].as_str()).collect::<Vec<_>>().join(""), _ => String::new() };
                if v["isMeta"]==true&&!collaboration(&v,&plain){continue;}
                // Slash-command rows are context, not model turns. Continue to
                // the preceding real turn so a command issued during a task
                // cannot falsely make it idle. A command-only transcript is idle.
                if !tool && local_command(&plain) {
                    if local_turn.is_none(){local_turn=v["promptId"].as_str().map(|p|format!("claude_turn:{p}"));}
                    continue;
                }
                if state.is_none() { state = Some(if !tool && plain.starts_with("[Request interrupted by user") { "INTERRUPTED" } else { "RUNNING" }); }
                let Some(turn) = v["promptId"].as_str().filter(|p| !p.is_empty()) else { continue };
                let state = match state { Some("COMPLETE") if reply.is_none() => "EMPTY", Some(s) => s, None => "RUNNING" };
                return Some(Status { turn: format!("claude_turn:{turn}"), state, reply: if state == "COMPLETE" { reply } else { None }, completed_at, cwd });
            }
            _ => {}
        }
    }
    if let Some((_,turn,_))=card_end {return Some(Status{turn,state:"RUNNING",reply:None,completed_at:0,cwd});}
    if start==0 && state.is_none() && !skipped_large {local_turn.map(|turn|Status{turn,state:"EMPTY",reply:None,completed_at:0,cwd})}else{None}
}
const RECEIPT_SCAN: u64 = 16 * 1024 * 1024;
pub(crate) fn awaiting_reply(session:&str,turn:&str)->bool{
 latest(session).is_some_and(|p|p.turns.iter().any(|t|t.id==turn)&&page_awaiting_reply(&p))
}
fn page_awaiting_reply(page:&Page)->bool{
 let mut messages=page.turns.iter().flat_map(|t|&t.messages).collect::<Vec<_>>();messages.sort_by_key(|m|std::cmp::Reverse(m.order));
 for m in messages {if m.role=="user"&&m.kind=="PUBLIC"{return false;}if m.card.as_ref().is_some_and(|c|c["question"].as_str().is_some_and(|s|!s.trim().is_empty())||c["actions"].as_array().is_some_and(|a|!a.is_empty())){return true;}}
 false
}
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

/// Native stored result receipt; no synthetic hook ACK can establish this row.
pub(crate) fn find_tool_result(path:&Path,session:&str,tool:&str)->Option<String>{
 type Stamp=(u64,std::time::SystemTime);
 type Cache=std::collections::HashMap<(PathBuf,String,String),(Stamp,Option<String>)>;
 static CACHE:std::sync::OnceLock<std::sync::Mutex<Cache>>=std::sync::OnceLock::new();
 let mut file=File::open(path).ok()?;let meta=file.metadata().ok()?;let len=meta.len();let stamp=(len,meta.modified().ok()?);let key=(path.to_owned(),session.to_owned(),tool.to_owned());
 if let Some((old,result))=CACHE.get_or_init(Default::default).lock().ok()?.get(&key){if *old==stamp{return result.clone()}}
file.seek(SeekFrom::Start(len.saturating_sub(RECEIPT_SCAN))).ok()?;let mut bytes=Vec::new();file.take(RECEIPT_SCAN).read_to_end(&mut bytes).ok()?;
 let result=bytes.split(|b|*b==b'\n').rev().filter(|line|line.len()<=MAX_LINE).filter_map(|line|serde_json::from_slice::<Value>(line).ok()).find_map(|v|{
  if v["sessionId"].as_str()!=Some(session)||v["isSidechain"]==true||v["type"]!="user"{return None}
  v["message"]["content"].as_array()?.iter().any(|b|b["type"]=="tool_result"&&b["tool_use_id"].as_str()==Some(tool)).then(||v["uuid"].as_str().filter(|id|native(id)).map(str::to_owned)).flatten()
 });
 if let Ok(mut cache)=CACHE.get_or_init(Default::default).lock(){if cache.len()>=256{cache.clear()}cache.insert(key,(stamp,result.clone()));}
 result
}

/// Reads backwards from `before` (or the end) until `PAGE_TURNS` complete turns.
pub(crate) fn page(path: &Path, session: &str, before: Option<u64>) -> std::io::Result<Page> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let end = before.unwrap_or(len).min(len);
    let mut result = Page{version:len,..Default::default()};
    // Assistant messages seen (newest first) since the last prompt.
    let mut pending: Vec<Message> = Vec::new();
    let mut tail: Vec<u8> = Vec::new(); // bytes after the last newline seen, belonging to the line in progress
    let mut pos = end;
    let mut read = 0u64;
    let flush = |line: &[u8], line_start: u64, result: &mut Page, pending: &mut Vec<Message>| -> bool {
        match classify(line, session) {
            Line::Reply { id, text, card, at } => {
                // Streamed content blocks of one API message share its id: join them.
                match pending.last_mut() { Some(m) if m.id == id => {m.text = format!("{text}\n{}", m.text);m.order=line_start;if card.is_some(){m.card=card;}}, _ => pending.push(Message { id, role: "assistant", text, kind:"PUBLIC", card, at, order:line_start }) }
            }
            Line::Prompt { turn, id, text, cwd, kind, at } => {
                if result.cwd.is_none() { result.cwd = cwd; }
                let mut messages = vec![Message { id, role: "user", text, kind, card:None, at, order:line_start }];
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
    fn internal_notifications_require_provenance_and_keep_native_offsets(){
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("chat.jsonl");
        let human=user("human","u","<system-reminder>\nThe session quoted by a human");
        let peer=json!({"type":"user","sessionId":S,"promptId":"peer","uuid":"p","cwd":"D:\\p","isMeta":true,"turnOrigin":"peer","origin":{"kind":"peer"},"message":{"content":"child result"}}).to_string();
        let sdk=json!({"type":"user","sessionId":S,"promptId":"sdk","uuid":"s","cwd":"D:\\p","promptSource":"sdk","turnOrigin":"sdk","message":{"content":"<system-reminder>\nThe session child finished a turn."}}).to_string();
        std::fs::write(&path,format!("{human}\n{peer}\n{sdk}\n")).unwrap();
        let p=page(&path,S,None).unwrap();let rows:Vec<_>=p.turns.iter().rev().flat_map(|t|&t.messages).collect();
        assert_eq!(rows.iter().map(|m|m.kind).collect::<Vec<_>>(),vec!["PUBLIC","COLLABORATION","COLLABORATION"]);
        assert!(rows.windows(2).all(|w|w[0].order<w[1].order));
    }
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
        assert_eq!(first.turns[0].messages.iter().map(|m|(m.id.as_str(),m.role,m.text.as_str())).collect::<Vec<_>>(), vec![("u6","user","问题 6"),("claude_msg:m6","assistant","第一段\n第二段")]);
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
    fn local_commands_do_not_start_or_finish_model_turns(){
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("t.jsonl");
        let local=json!({"type":"user","sessionId":S,"cwd":"D:\\p","promptId":"local","message":{"role":"user","content":"<command-name>/agbrio-status</command-name>"}}).to_string();
        let output=json!({"type":"system","subtype":"local_command","sessionId":S,"content":"<local-command-stdout>connected</local-command-stdout>"}).to_string();
        let write=|rows:Vec<String>|{std::fs::write(&path,rows.join("\n")).unwrap();status(&path,S).unwrap()};
        let empty=write(vec![local.clone(),output.clone()]);assert_eq!((empty.state,empty.reply), ("EMPTY",None));
        assert!(page(&path,S,None).unwrap().turns.is_empty());
        let prompt=json!({"type":"user","sessionId":S,"promptId":"real","message":{"role":"user","content":"real work"}}).to_string();
        let busy=write(vec![prompt.clone(),local.clone(),output.clone()]);assert_eq!((busy.state,busy.turn.as_str()),("RUNNING","claude_turn:real"));
        let end=json!({"type":"assistant","sessionId":S,"message":{"id":"answer","stop_reason":"end_turn","content":[{"type":"text","text":"done"}]}}).to_string();
        let done=write(vec![prompt,end,local,output]);assert_eq!((done.state,done.reply.as_deref(),done.turn.as_str()),("COMPLETE",Some("done"),"claude_turn:real"));
        let huge=json!({"type":"user","sessionId":S,"promptId":"huge","message":{"content":"x".repeat(MAX_LINE+1)}}).to_string();
        let command=json!({"type":"user","sessionId":S,"promptId":"local","message":{"content":"<command-name>/status</command-name>"}}).to_string();
        std::fs::write(&path,[huge,command].join("\n")).unwrap();assert!(status(&path,S).is_none()); // bounded scan cannot prove idle
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
    #[test]
    fn peer_turns_and_native_compaction_do_not_answer_an_owner_card(){
      let dir=tempfile::tempdir().unwrap();let path=dir.path().join("pending-card.jsonl");
      let call=json!({"type":"assistant","sessionId":S,"message":{"id":"question","stop_reason":"tool_use","content":[{"type":"tool_use","name":"mcp__ccd_turn__wrap_up","id":"card-call","input":{"recap":"结果","ask":"怎么处理？","actions":[{"label":"A","message":"执行 A"}]}}]}});
      let mut peer:Value=serde_json::from_str(&user("peer-turn","peer-user","子对话回报")).unwrap();peer["origin"]=json!({"kind":"peer"});peer["isMeta"]=json!(true);
      let mut summary:Value=serde_json::from_str(&user("peer-turn","context-summary","internal context only")).unwrap();summary["isCompactSummary"]=json!(true);
      let mut rows=vec![user("owner","owner-user","开始"),call.to_string(),peer.to_string(),reply("peer-reply","收到进展"),summary.to_string()];
      std::fs::write(&path,rows.join("\n")).unwrap();let p=page(&path,S,None).unwrap();assert!(page_awaiting_reply(&p));assert!(!p.turns.iter().flat_map(|t|&t.messages).any(|m|m.id=="context-summary"));
      rows.push(user("answer","owner-answer","执行 A"));std::fs::write(&path,rows.join("\n")).unwrap();assert!(!page_awaiting_reply(&page(&path,S,None).unwrap()));
    }
    #[test]
    fn next_step_box_requires_its_exact_successful_native_terminal_pair(){
      let dir=tempfile::tempdir().unwrap();let path=dir.path().join("card-end.jsonl");
      let call=json!({"type":"assistant","sessionId":S,"message":{"id":"wrap","stop_reason":"tool_use","content":[{"type":"tool_use","id":"call","name":"mcp__ccd_turn__wrap_up","input":{"recap":"阶段结果","ask":"选择下一步","actions":[{"label":"继续","message":"继续工作"}]}}]}});
      let ack=json!({"type":"user","sessionId":S,"promptId":"p","cwd":"D:\\p","timestamp":"2026-10-22T07:49:50.917Z","message":{"content":[{"type":"tool_result","tool_use_id":"call","content":[{"type":"text","text":CARD_TURN_END}]}]}});
      let check=|rows:Vec<Value>|{std::fs::write(&path,rows.into_iter().map(|v|v.to_string()).collect::<Vec<_>>().join("\n")).unwrap();status(&path,S).unwrap()};
      let prompt:Value=serde_json::from_str(&user("p","u","开始")).unwrap();
      let done=check(vec![prompt.clone(),call.clone(),ack.clone()]);assert_eq!((done.state,done.turn.as_str(),done.reply.as_deref()),("COMPLETE","claude_turn:p",Some("阶段结果")));assert!(done.completed_at>0);
      let mut wrong=ack.clone();wrong["message"]["content"][0]["tool_use_id"]=json!("another");assert_eq!(check(vec![prompt.clone(),call.clone(),wrong]).state,"RUNNING");
      let mut error=ack.clone();error["message"]["content"][0]["is_error"]=json!(true);assert_eq!(check(vec![prompt.clone(),call.clone(),error]).state,"RUNNING");
      let mut other=call.clone();other["message"]["content"][0]["name"]=json!("Bash");assert_eq!(check(vec![prompt.clone(),other,ack.clone()]).state,"RUNNING");
      let mut recap=call.clone();recap["message"]["content"][0]["input"]["ask"]=Value::Null;recap["message"]["content"][0]["input"]["actions"]=json!([]);assert_eq!(check(vec![prompt.clone(),recap,ack.clone()]).state,"RUNNING");
      assert_eq!(check(vec![prompt.clone(),call.clone()]).state,"RUNNING");
      let newer:Value=serde_json::from_str(&user("new","new-user","新要求")).unwrap();let next=check(vec![prompt.clone(),call.clone(),ack.clone(),newer]);assert_eq!((next.state,next.turn.as_str()),("RUNNING","claude_turn:new"));
      let request=json!({"type":"api-request","sessionId":S});assert_eq!(check(vec![prompt.clone(),call.clone(),ack.clone(),request]).state,"RUNNING");
      assert_eq!(check(vec![prompt,ack]).state,"RUNNING");
    }
    #[test]
    #[ignore="explicit exact owner session/card; bounded native prefix copy, read only, no provider calls"]
    fn real_peer_card_prefix_read_only(){
      let sid=std::env::var("AGBRIO_READ_ONLY_CLAUDE_SESSION").unwrap();let tool=std::env::var("AGBRIO_READ_ONLY_CARD_ID").unwrap();assert!(native(&sid));
      let root=PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join(".claude/projects");let source=find(&root,&sid).unwrap();let mut f=File::open(source).unwrap();let start=f.metadata().unwrap().len().saturating_sub(STATUS_SCAN);f.seek(SeekFrom::Start(start)).unwrap();let mut data=Vec::new();f.take(STATUS_SCAN).read_to_end(&mut data).unwrap();
      let rows=data.split(|b|*b==b'\n').filter_map(|l|serde_json::from_slice::<Value>(l).ok()).filter(|v|v["sessionId"]==sid&&v["isSidechain"]!=true).collect::<Vec<_>>();
      let card=rows.iter().rposition(|v|v["type"]=="assistant"&&v["message"]["content"].as_array().is_some_and(|a|a.iter().any(|b|b["name"]=="mcp__ccd_turn__wrap_up"&&b["id"]==tool))).unwrap();
      let owner=(card+1..rows.len()).find(|&i|matches!(classify(rows[i].to_string().as_bytes(),&sid),Line::Prompt{kind:"PUBLIC",..})).unwrap();
      assert!(rows[card+1..owner].iter().any(|v|v["origin"]["kind"]=="peer"));
      let dir=tempfile::tempdir().unwrap();let path=dir.path().join("native-prefix.jsonl");let prefix=rows[..owner].iter().map(Value::to_string).collect::<Vec<_>>().join("\n");std::fs::write(&path,&prefix).unwrap();let p=page(&path,&sid,None).unwrap();assert!(page_awaiting_reply(&p));let question=p.turns.iter().find(|t|t.messages.iter().any(|m|m.card.as_ref().is_some_and(|c|c["id"]==tool))).unwrap();assert_ne!(p.turns[0].id,question.id);
      std::fs::write(&path,format!("{prefix}\n{}",rows[owner])).unwrap();assert!(!page_awaiting_reply(&page(&path,&sid,None).unwrap()));
      println!("exactNativeCard=true newerPeerTurn=true pendingBeforeOwner=true retiredAfterOwner=true modelCalls=0 ownerWrites=0");
    }
    #[test]
    #[ignore="explicit exact owner session, transcript read only; no provider calls"]
    fn real_native_next_step_terminal_read_only(){
      let sid=std::env::var("AGBRIO_READ_ONLY_CLAUDE_SESSION").unwrap();assert!(native(&sid));
      let root=PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join(".claude/projects");let path=find(&root,&sid).unwrap();
      let s=status(&path,&sid).unwrap();let p=page(&path,&sid,None).unwrap();let turn=p.turns.iter().find(|t|t.id==s.turn).unwrap();
      assert_eq!(s.state,"COMPLETE");assert!(s.reply.as_ref().is_some_and(|s|!s.trim().is_empty()));assert!(turn.messages.iter().any(|m|m.card.is_some()));
      println!("exactSession=true state={} nativeCards={} publicMessages={} modelCalls=0 ownerWrites=0",s.state,turn.messages.iter().filter(|m|m.card.is_some()).count(),turn.messages.len());
    }
    #[test]
    fn native_desktop_wrap_up_preserves_recap_question_actions_and_completion(){
      let dir=tempfile::tempdir().unwrap();let path=dir.path().join("card.jsonl");let wrap=json!({"type":"assistant","sessionId":S,"uuid":"row","message":{"id":"wrap-native","stop_reason":"tool_use","content":[{"type":"tool_use","name":"mcp__ccd_turn__wrap_up","id":"native-call","input":{"recap":"阶段结果","ask":"继续哪一项？","actions":[{"label":"方案 A","message":"执行方案 A"},{"label":"方案 B","message":"执行方案 B"}]}}]}});
      let end=json!({"type":"assistant","sessionId":S,"message":{"id":"empty-end","stop_reason":"end_turn","content":[]}});
      std::fs::write(&path,[user("p","u","开始"),wrap.to_string(),end.to_string()].join("\n")).unwrap();let page=page(&path,S,None).unwrap();let card=&page.turns[0].messages[1];assert_eq!(card.id,"claude_msg:wrap-native");assert_eq!(card.text,"阶段结果");assert_eq!(card.card.as_ref().unwrap()["actions"][0]["message"],"执行方案 A");let status=status(&path,S).unwrap();assert_eq!(status.state,"COMPLETE");assert_eq!(status.reply.as_deref(),Some("阶段结果"));
    }
    #[test]
    fn owner_decision_receipts_require_an_exact_native_tool_result_row(){
      let dir=tempfile::tempdir().unwrap();let path=dir.path().join("receipt.jsonl");let uuid="40000000-0000-4000-8000-000000000001";
      let row=json!({"type":"user","sessionId":S,"uuid":uuid,"message":{"content":[{"type":"tool_result","tool_use_id":"exact-call","content":"done"}]}});
      for field in ["sessionId","uuid"]{let mut wrong=row.clone();wrong[field]="wrong".into();std::fs::write(&path,wrong.to_string()).unwrap();assert!(find_tool_result(&path,S,"exact-call").is_none());}
      let mut side=row.clone();side["isSidechain"]=true.into();std::fs::write(&path,side.to_string()).unwrap();assert!(find_tool_result(&path,S,"exact-call").is_none());
      std::fs::write(&path,row.to_string()).unwrap();assert_eq!(find_tool_result(&path,S,"exact-call").as_deref(),Some(uuid));assert!(find_tool_result(&path,S,"other-call").is_none());
    }
}
