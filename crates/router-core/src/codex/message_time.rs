//! Bounded metadata projection of the last visible native message. No body escapes.
use serde_json::Value;
use std::{fs::File,io::{BufRead,BufReader,Read,Seek,SeekFrom},path::Path};
pub(super) fn read(path:&Path,thread:&str,cwd:&str)->Option<String>{
 // Only the exact native thread/read metadata supplies this path. Histories
 // can legitimately live outside the current default CODEX_HOME after moves.
 if !path.is_absolute()||path.extension().is_none_or(|e|e!="jsonl"){return None;}
 let path=path.canonicalize().ok()?;
 type Cache=std::collections::HashMap<(std::path::PathBuf,String,String),((u64,std::time::SystemTime),Option<String>)>;
 static CACHE:std::sync::OnceLock<std::sync::Mutex<Cache>>=std::sync::OnceLock::new();
 let meta=std::fs::metadata(&path).ok()?;let stamp=(meta.len(),meta.modified().ok()?);let key=(path.clone(),thread.to_owned(),cwd.to_owned());
 if let Ok(c)=CACHE.get_or_init(Default::default).lock(){if let Some((old,result))=c.get(&key){if *old==stamp{return result.clone();}}}
 let mut file=File::open(&path).ok()?;let mut first=String::new();BufReader::new((&mut file).take(2*1024*1024)).read_line(&mut first).ok()?;
 let header:Value=serde_json::from_str(&first).ok()?;
 if header["type"]!="session_meta"||header["payload"]["id"].as_str()!=Some(thread)||!Path::new(header["payload"]["cwd"].as_str()?).canonicalize().ok()?.eq(&Path::new(cwd).canonicalize().ok()?){return None;}
 let start=stamp.0.saturating_sub(16*1024*1024);file.seek(SeekFrom::Start(start)).ok()?;let mut bytes=Vec::new();file.take(16*1024*1024).read_to_end(&mut bytes).ok()?;
 let result=project(&bytes);
 if let Ok(mut c)=CACHE.get_or_init(Default::default).lock(){if c.len()>=64{c.clear();}c.insert(key,(stamp,result.clone()));}result
}
fn project(bytes:&[u8])->Option<String>{
 // Skip a partial appended row. Native order owns "last", not clock maximum.
 for line in bytes.split_inclusive(|b|*b==b'\n').rev(){
  if !line.ends_with(b"\n")||line.len()>2*1024*1024{continue;}
  let Ok(v)=serde_json::from_slice::<Value>(line)else{continue};let p=&v["payload"];
  if v["type"]!="response_item"{continue;}
  // The native asynchronous question is rendered as a public decision card.
  let question=p["type"]=="function_call"&&p["name"]=="request_user_input_async"&&p["arguments"].as_str().filter(|s|s.len()<=131072).and_then(|s|serde_json::from_str::<Value>(s).ok()).is_some_and(|a|a["questions"].as_array().is_some_and(|q|!q.is_empty()&&q.len()<=12&&q.iter().any(|q|q["title"].as_str().is_some_and(|s|!s.trim().is_empty()))));
  if question {if let Some(at)=v["timestamp"].as_str().filter(|s|s.len()<=40){return Some(at.into());}continue;}
  if p["type"]!="message"{continue;}
  let public=match p["role"].as_str(){Some("user")=>true,Some("assistant")=>matches!(p["phase"].as_str(),None|Some("commentary"|"final_answer"))&&!matches!(p["channel"].as_str(),Some("analysis")),_=>false};
  if !public||!p["content"].as_array().is_some_and(|rows|rows.iter().any(|r|matches!(r["type"].as_str(),Some("input_text"|"output_text"|"input_image")))){continue;}
  return v["timestamp"].as_str().filter(|s|s.len()<=40).map(str::to_owned);
 }None
}
#[cfg(test)]mod tests{
 use super::*;use serde_json::json;
 #[test]fn only_native_public_messages_move_time(){
  let row=|role:&str,phase:&str,at:&str|json!({"type":"response_item","timestamp":at,"payload":{"type":"message","role":role,"phase":phase,"content":[{"type":"output_text","text":"fixture"}]}}).to_string()+"\n";
  let input=row("user","","2026-10-23T00:01:00Z")+&row("assistant","commentary","2026-10-23T00:02:00Z")+&row("assistant","analysis","2026-10-23T00:09:00Z")+"{\"type\":\"event_msg\",\"timestamp\":\"2026-10-23T00:10:00Z\"}\n";
  assert_eq!(project(input.as_bytes()).as_deref(),Some("2026-10-23T00:02:00Z"));assert_eq!(project((input+&row("assistant","final_answer","2026-10-23T00:03:00Z")).as_bytes()).as_deref(),Some("2026-10-23T00:03:00Z"));
 }
 #[test]fn a_visible_async_question_counts_but_an_ordinary_tool_does_not(){
  let row=|name|json!({"type":"response_item","timestamp":"2026-10-23T00:05:00Z","payload":{"type":"function_call","name":name,"arguments":"{\"questions\":[{\"title\":\"Choose a direction\"}]}"}}).to_string()+"\n";
  assert!(project(row("exec_command").as_bytes()).is_none());assert_eq!(project(row("request_user_input_async").as_bytes()).as_deref(),Some("2026-10-23T00:05:00Z"));
 }
 #[test]fn native_metadata_path_identity_project_and_file_change_are_checked(){
  let d=tempfile::tempdir().unwrap();let p=d.path().join("session.jsonl");let header=json!({"type":"session_meta","payload":{"id":"exact","cwd":d.path()}}).to_string()+"\n";std::fs::write(&p,&header).unwrap();assert_eq!(read(&p,"wrong",d.path().to_str().unwrap()),None);
  let row=json!({"type":"response_item","timestamp":"2026-10-23T00:03:00Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fixture"}]}}).to_string()+"\n";std::fs::write(&p,header+&row).unwrap();assert_eq!(read(&p,"exact",d.path().to_str().unwrap()).as_deref(),Some("2026-10-23T00:03:00Z"));assert!(read(&p,"exact",d.path().join("missing").to_str().unwrap()).is_none());
 }
}
