//! Bounded metadata-only projections from exact native transcript identities.
use serde_json::{json,Value};
use std::{path::Path,io::{Read,Seek,SeekFrom}};
fn tail(path:&Path)->Option<Vec<Value>>{
 let mut f=std::fs::File::open(path).ok()?;let length=f.metadata().ok()?.len();let start=length.saturating_sub(2*1024*1024);f.seek(SeekFrom::Start(start)).ok()?;let mut bytes=Vec::new();f.take(2*1024*1024).read_to_end(&mut bytes).ok()?;
 Some(bytes.split(|b|*b==b'\n').skip(if start>0{1}else{0}).collect::<Vec<_>>().into_iter().rev().filter(|line|line.len()<1024*1024).filter_map(|line|serde_json::from_slice(line).ok()).collect())
}
fn at()->i64{std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64}
pub(crate) fn claude(session:&str)->Option<Value>{
 let path=crate::claude_transcript::find(&crate::claude_transcript::projects_root()?,session)?;
 let rows=tail(&path)?;
 let row=rows.iter().find(|r|r["type"]=="assistant"&&r["sessionId"]==session&&r["isSidechain"]!=true&&r["message"]["model"].as_str().is_some_and(|s|!s.starts_with('<')&&!s.is_empty()))?;
 Some(json!({"model":row["message"]["model"],"modelLabel":null,"effort":null,"source":"NATIVE","observedAt":at()}))
}
pub(crate) fn codex(thread:&Value)->Option<Value>{
 let context=thread["path"].as_str().and_then(|p|tail(Path::new(p))).and_then(|rows|rows.into_iter().find(|v|v["type"]=="turn_context"));
 let native=context.as_ref().map(|v|&v["payload"]);
 let model=thread["model"].as_str().or_else(||native.and_then(|v|v["model"].as_str()));
 let effort=thread["reasoningEffort"].as_str().or_else(||native.and_then(|v|v["effort"].as_str().or_else(||v["reasoning_effort"].as_str())));
 if model.is_none()&&effort.is_none(){return None;}
 Some(json!({"model":model,"modelLabel":null,"effort":effort,"source":"NATIVE","observedAt":at()}))
}
#[cfg(test)]mod tests{use super::*;#[test]fn exact_native_context_only_never_title_or_prompt(){let t=json!({"id":"t","name":"gpt-fiction ultra","preview":"model made-up"});assert!(codex(&t).is_none());let t=json!({"model":"native-model","reasoningEffort":"high"});let r=codex(&t).unwrap();assert_eq!(r["model"],"native-model");assert_eq!(r["effort"],"high");}}
