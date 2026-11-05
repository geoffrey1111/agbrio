//! Read only native asynchronous owner questions, never reasoning/tool output.
use serde_json::{Value,json};
use std::{fs::File,io::{BufRead,BufReader,Read,Seek,SeekFrom},path::Path};
pub fn answer_id(thread:&str,request:&str)->String{
 use sha2::{Digest,Sha256};let hash=format!("{:x}",Sha256::digest(format!("agbrio-async-answer-v1:{}:{thread}:{request}",thread.len()).as_bytes()));format!("{}-{}-{}-{}-{}",&hash[..8],&hash[8..12],&hash[12..16],&hash[16..20],&hash[20..32])
}
const WINDOW:u64=16*1024*1024;
const MAX_LINE:usize=2*1024*1024;
#[derive(Clone)]
struct CachedQuestions{stamp:(u64,std::time::SystemTime),created:Option<std::time::SystemTime>,rows:Vec<Value>,boundaries:Vec<(u64,String)>}
fn context(line:&[u8])->Option<String>{
 if line.len()>MAX_LINE||!line.windows(12).any(|w|w==b"turn_context"){return None}
 let v:Value=serde_json::from_slice(line).ok()?;if v["type"]!="turn_context"{return None}
 Some(v["payload"]["turn_id"].as_str().filter(|s|s.len()<=256).unwrap_or_default().into())
}
/// Recover the original boundary before a tail window, never guess the latest turn.
/// Backward fixed-size reads retain at most one bounded metadata line.
fn preceding_context(file:&mut File,end:u64)->std::io::Result<Option<(u64,String)>>{
 let mut cursor=end;let mut carry=Vec::new();let mut oversized=false;
 while cursor>0{
  let start=cursor.saturating_sub(1024*1024);let mut chunk=vec![0;(cursor-start)as usize];file.seek(SeekFrom::Start(start))?;file.read_exact(&mut chunk)?;
  let mut part_end=chunk.len();
  for i in (0..chunk.len()).rev(){if chunk[i]!=b'\n'{continue}
   if !oversized{let part=&chunk[i+1..part_end];if part.len()+carry.len()>MAX_LINE{oversized=true}else{let mut line=part.to_vec();line.extend_from_slice(&carry);carry=line;}}
   if !oversized{if let Some(turn)=context(&carry){return Ok(Some((start+i as u64+1,turn)))}}
   carry.clear();oversized=false;part_end=i;
  }
  if !oversized{if part_end+carry.len()>MAX_LINE{carry.clear();oversized=true}else{let mut line=chunk[..part_end].to_vec();line.extend_from_slice(&carry);carry=line;}}
  cursor=start;
 }
 Ok(if oversized{None}else{context(&carry).map(|turn|(0,turn))})
}
pub fn read(path:&Path,root:&Path,thread:&str)->Vec<Value>{
 let Ok(canonical)=path.canonicalize()else{return vec![]};let Ok(root)=root.canonicalize()else{return vec![]};if !canonical.starts_with(root)||canonical.extension().is_none_or(|x|x!="jsonl"){return vec![]}
 type Cache=std::collections::HashMap<(std::path::PathBuf,String),CachedQuestions>;
 static CACHE:std::sync::OnceLock<std::sync::Mutex<Cache>>=std::sync::OnceLock::new();
 let Ok(meta)=std::fs::metadata(&canonical)else{return vec![]};let Ok(modified)=meta.modified()else{return vec![]};let stamp=(meta.len(),modified);let created=meta.created().ok();let key=(canonical.clone(),thread.to_owned());
 let prior=CACHE.get_or_init(Default::default).lock().ok().and_then(|cache|cache.get(&key).cloned());
 if let Some(old)=prior.as_ref(){if old.stamp==stamp&&old.created==created{return old.rows.clone()}}
 let Ok(mut file)=File::open(canonical)else{return vec![]};let mut first=String::new();if BufReader::new((&mut file).take(MAX_LINE as u64)).read_line(&mut first).is_err(){return vec![]}
 let Ok(header)=serde_json::from_str::<Value>(&first)else{return vec![]};if header["type"]!="session_meta"||header["payload"]["id"].as_str()!=Some(thread){return vec![]}
 let len=stamp.0;let start=len.saturating_sub(WINDOW);if file.seek(SeekFrom::Start(start)).is_err(){return vec![]};let mut tail=Vec::new();if (&mut file).take(WINDOW).read_to_end(&mut tail).is_err(){return vec![]}
 // The first fragment may be the middle of any native JSON record.
 let skip=if start>0{tail.iter().position(|b|*b==b'\n').map(|i|i+1).unwrap_or(tail.len())}else{0};let aligned=start+skip as u64;let bytes=&tail[skip..];
 let cached_seed=prior.as_ref().filter(|old|created.is_some()&&old.created==created&&old.stamp.0<len&&old.stamp.0>=aligned).and_then(|old|old.boundaries.iter().rev().find(|(offset,_)|*offset<aligned).cloned());
 let seed=if aligned==0{None}else{cached_seed.or_else(||preceding_context(&mut file,aligned).ok().flatten())};
 let mut rows=project_seeded(bytes,seed.as_ref().map(|(_,s)|s.as_str()).unwrap_or_default());
 let mut boundaries=seed.into_iter().collect::<Vec<_>>();let mut offset=aligned;for line in bytes.split_inclusive(|b|*b==b'\n'){if let Some(turn)=context(line){boundaries.push((offset,turn));}offset+=line.len()as u64;}
 for row in &mut rows{row["answerId"]=json!(answer_id(thread,row["requestId"].as_str().unwrap_or_default()));}
 if let Ok(mut cache)=CACHE.get_or_init(Default::default).lock(){if cache.len()>=64{cache.clear()}cache.insert(key,CachedQuestions{stamp,created,rows:rows.clone(),boundaries});}rows
}
pub(crate) fn response_hash(text:&str)->Option<String>{
 use sha2::{Digest,Sha256};
 let raw=text.trim().strip_prefix("<send_user_message_question_reply>")?.strip_suffix("</send_user_message_question_reply>")?;
 let rows:Vec<Value>=serde_json::from_str(raw).ok()?;
 let mut pairs=std::collections::BTreeMap::new();
 for r in rows{let id=r["questionItemId"].as_str()?;let answer=r["answer"].as_str()?;if pairs.insert(id.to_owned(),answer.to_owned()).is_some(){return None}}
 if pairs.is_empty(){return None}Some(format!("{:x}",Sha256::digest(serde_json::to_vec(&pairs).ok()?)))
}
#[cfg(test)]
fn project(bytes:&[u8])->Vec<Value>{project_seeded(bytes,"")}
fn project_seeded(bytes:&[u8],initial_turn:&str)->Vec<Value>{
 let mut turn=initial_turn.to_owned();let mut cards:Vec<Value>=vec![];let mut answered=std::collections::HashSet::new();let mut receipts=std::collections::HashMap::new();
 // Latest display state must not erase evidence for an earlier exact send.
 // All receipt sets remain bounded by the native read window, not persisted prompts.
 let mut receipt_history:std::collections::HashMap<String,std::collections::BTreeSet<String>>=std::collections::HashMap::new();
 for line in bytes.split(|b|*b==b'\n').filter(|l|l.len()<=2*1024*1024){let Ok(text)=std::str::from_utf8(line)else{continue};if !text.contains("turn_context")&&!text.contains("request_user_input_async")&&!text.contains("send_user_message_question_reply"){continue;}let Ok(v)=serde_json::from_slice::<Value>(line)else{continue};let p=&v["payload"];
  if v["type"]=="turn_context"{turn=p["turn_id"].as_str().unwrap_or_default().into();continue}
  if v["type"]!="response_item"{continue}
  if p["type"]=="function_call"&&p["name"]=="request_user_input_async"{
   let Some(call)=p["call_id"].as_str().filter(|s|!s.is_empty()&&s.len()<=256)else{continue};let Some(args)=p["arguments"].as_str().filter(|s|s.len()<=131072).and_then(|s|serde_json::from_str::<Value>(s).ok())else{continue};let Some(questions)=args["questions"].as_array().filter(|qs|!qs.is_empty()&&qs.len()<=12)else{continue};
   let questions=questions.iter().enumerate().filter_map(|(i,q)|{let question=q["title"].as_str()?.chars().take(16000).collect::<String>();let options=q["options"].as_array().map(|options|options.iter().take(12).filter_map(|o|o.as_str().map(|s|json!({"label":s,"description":""}))).collect::<Vec<_>>()).unwrap_or_default();Some(json!({"id":json!(["request_user_input_async",call,i]).to_string(),"label":question,"placeholder":question,"required":true,"isOther":true,"options":options}))}).collect::<Vec<_>>();
   if questions.is_empty(){continue}cards.push(json!({"requestId":call,"revision":1,"method":"request_user_input_async","kind":"USER_INPUT","isBlocking":false,"responseSent":false,"turnId":turn,"questions":questions}));
  }else if p["type"]=="message"&&p["role"]=="user"{if let Some(content)=p["content"].as_array(){for item in content{let Some(text)=item["text"].as_str()else{continue};let hash=response_hash(text);let Some(text)=text.trim().strip_prefix("<send_user_message_question_reply>").and_then(|t|t.strip_suffix("</send_user_message_question_reply>"))else{continue};let Ok(rows)=serde_json::from_str::<Vec<Value>>(text)else{continue};for r in rows{if r["answer"].as_str().is_some(){if let Some(id)=r["questionItemId"].as_str(){answered.insert(id.to_owned());if let Some(hash)=&hash{receipts.insert(id.to_owned(),hash.clone());receipt_history.entry(id.to_owned()).or_default().insert(hash.clone());}}}}}}
  }
 }
 for card in &mut cards{card["responseSent"]=json!(card["questions"].as_array().is_some_and(|qs|qs.iter().all(|q|q["id"].as_str().is_some_and(|id|answered.contains(id)))));let hashes=card["questions"].as_array().map(|qs|qs.iter().filter_map(|q|q["id"].as_str().and_then(|id|receipts.get(id))).collect::<Vec<_>>()).unwrap_or_default();if card["responseSent"]==true&&!hashes.is_empty()&&hashes.iter().all(|h|*h==hashes[0]){card["responseHash"]=json!(hashes[0]);}}
 for card in &mut cards{
  if card["responseSent"]!=true{continue}
  let ids=card["questions"].as_array().map(|qs|qs.iter().filter_map(|q|q["id"].as_str()).collect::<Vec<_>>()).unwrap_or_default();
  let mut complete=ids.first().and_then(|id|receipt_history.get(*id)).cloned().unwrap_or_default();
  for id in ids.iter().skip(1){complete.retain(|hash|receipt_history.get(*id).is_some_and(|set|set.contains(hash)));}
  card["responseHashes"]=json!(complete);
 }
 cards.into_iter().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect()
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn earlier_complete_receipts_survive_later_and_partial_answers(){
  let request="same-question";let ids=[json!(["request_user_input_async",request,0]).to_string(),json!(["request_user_input_async",request,1]).to_string()];
  let q=json!({"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","call_id":request,"arguments":json!({"questions":[{"title":"One"},{"title":"Two"}]}).to_string()}}).to_string();
  let envelope=|values:Value|format!("<send_user_message_question_reply>{values}</send_user_message_question_reply>");
  let a=envelope(json!([{"questionItemId":ids[0],"answer":"wait"},{"questionItemId":ids[1],"answer":"keep"}]));
  let partial=envelope(json!([{"questionItemId":ids[0],"answer":"changed"}]));
  let b=envelope(json!([{"questionItemId":ids[0],"answer":"allow"},{"questionItemId":ids[1],"answer":"keep"}]));
  let row=|text:&str|json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":text}]}}).to_string();
  let earlier=project(format!("{q}\n{}\n{}",row(&a),row(&partial)).as_bytes());
  assert_eq!(earlier[0]["responseSent"],true);assert_eq!(earlier[0]["responseHashes"],json!([response_hash(&a).unwrap()]));
  assert!(earlier[0]["responseHash"].is_null(),"partial envelopes must not be combined into a fake complete receipt");
  let latest=project(format!("{q}\n{}\n{}\n{}",row(&a),row(&partial),row(&b)).as_bytes());
  let hashes=latest[0]["responseHashes"].as_array().unwrap();assert_eq!(hashes.len(),2);assert!(hashes.contains(&json!(response_hash(&a).unwrap())));assert!(hashes.contains(&json!(response_hash(&b).unwrap())));assert!(!hashes.contains(&json!(response_hash(&partial).unwrap())));
  assert_eq!(latest[0]["responseHash"],response_hash(&b).unwrap());
 }
 #[test]fn exact_question_ids_resolve_only_explicit_native_owner_answers(){
 let question=json!({"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","call_id":"exact","arguments":json!({"questions":[{"title":"Choose","options":["A","B"]}]}).to_string()}}).to_string();
 let mut rows=vec![question.clone()];let result=project(rows.join("\n").as_bytes());assert_eq!(result[0]["questions"][0]["options"][0]["label"],"A");assert_eq!(result[0]["responseSent"],false);
 let id=result[0]["questions"][0]["id"].as_str().unwrap();let reply=format!("<send_user_message_question_reply>{}</send_user_message_question_reply>",json!([{"questionItemId":id,"answer":"B"}]));rows.push(json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":reply}]}}).to_string());assert_eq!(project(rows.join("\n").as_bytes())[0]["responseSent"],true);
 assert!(project(question.replace("request_user_input_async","unrelated_private_tool").as_bytes()).is_empty());
 }
 #[test]fn identity_boundary_and_answer_ids_survive_reopen(){
 let d=tempfile::tempdir().unwrap();let p=d.path().join("native.jsonl");let header=json!({"type":"session_meta","payload":{"id":"exact","base_instructions":"x".repeat(30000)}});let question=json!({"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","call_id":"call","arguments":json!({"questions":[{"title":"Choose"}]}).to_string()}});std::fs::write(&p,format!("{header}\n{question}")).unwrap();
 assert!(read(&p,d.path(),"other").is_empty());assert!(read(&p,&d.path().join("unrelated"),"exact").is_empty());let first=read(&p,d.path(),"exact");let second=read(&p,d.path(),"exact");assert_eq!(first,second);assert_eq!(first[0]["answerId"],answer_id("exact","call"));assert_ne!(answer_id("other","call"),answer_id("exact","call"));
 }
 #[test]fn native_answer_receipt_is_exact_and_does_not_follow_a_transport_ack(){
 let q=json!({"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","call_id":"exact","arguments":json!({"questions":[{"title":"Choose"}]}).to_string()}}).to_string();
 let text=format!("<send_user_message_question_reply>{}</send_user_message_question_reply>",json!([{"questionItemId":"[\"request_user_input_async\",\"exact\",0]","answer":"A"}]));
 let ack=json!({"type":"response_item","payload":{"type":"function_call_output","output":{"turnId":"active"}}}).to_string();
 let pending=project(format!("{q}\n{ack}").as_bytes());assert_eq!(pending[0]["responseSent"],false);assert!(pending[0]["responseHash"].is_null());
 let row=json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":text}]}}).to_string();let sent=project(format!("{q}\n{row}").as_bytes());assert_eq!(sent[0]["responseSent"],true);assert_eq!(sent[0]["responseHash"],response_hash(&text).unwrap());assert_ne!(response_hash(&text),response_hash(&text.replace("\"A\"","\"B\"")));
 }
 #[test]fn tail_window_recovers_original_turn_without_loading_or_guessing_from_a_later_turn(){
 let d=tempfile::tempdir().unwrap();let p=d.path().join("tail.jsonl");let header=json!({"type":"session_meta","payload":{"id":"exact"}});let ctx=json!({"type":"turn_context","payload":{"turn_id":"original"}});let q=json!({"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","call_id":"q","arguments":json!({"questions":[{"title":"Choose"}]}).to_string()}});
 let mut file=File::create(&p).unwrap();use std::io::Write;writeln!(file,"{header}\n{ctx}").unwrap();for _ in 0..18{file.write_all(&vec![b'x';1024*1024]).unwrap();}file.write_all(b"\n").unwrap();writeln!(file,"{q}").unwrap();drop(file);
 let first=read(&p,d.path(),"exact");assert_eq!(first[0]["turnId"],"original");assert_eq!(first[0]["responseSent"],false);
 let mut file=std::fs::OpenOptions::new().append(true).open(&p).unwrap();writeln!(file,"{}",json!({"type":"turn_context","payload":{"turn_id":"later"}})).unwrap();writeln!(file,"{}",q.to_string().replace("\"q\"","\"q-later\"")).unwrap();drop(file);let next=read(&p,d.path(),"exact");assert_eq!(next.iter().find(|q|q["requestId"]=="q").unwrap()["turnId"],"original");assert_eq!(next.iter().find(|q|q["requestId"]=="q-later").unwrap()["turnId"],"later");
 assert!(read(&p,d.path(),"foreign").is_empty());
 }
 #[test]fn truly_missing_native_context_remains_explicitly_unconfirmed(){let q=json!({"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","call_id":"q","arguments":json!({"questions":[{"title":"Choose"}]}).to_string()}});assert_eq!(project(q.to_string().as_bytes())[0]["turnId"],"");}
 #[test]#[ignore="explicit exact owner transcript, read-only question metadata; no model or session operations"]fn real_native_questions_read_only(){
 let thread=std::env::var("AGBRIO_QUESTION_THREAD").unwrap();let path=std::path::PathBuf::from(std::env::var_os("AGBRIO_QUESTION_ROLLOUT").unwrap());let root=std::path::PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join(".codex/sessions");let request=std::env::var("AGBRIO_QUESTION_REQUEST").unwrap_or_else(|_|"call_9c930989a3b9430e85450dfbbbd06c09".into());let rows=read(&path,&root,&thread);assert!(rows.iter().any(|r|r["requestId"]==request&&r["responseSent"]==true&&r["responseHash"].as_str().is_some_and(|h|h.len()==64)));if let Ok(turn)=std::env::var("AGBRIO_QUESTION_TURN"){assert_eq!(rows.iter().find(|r|r["requestId"]==request).unwrap()["turnId"],turn);}
 println!("nativeQuestions={} resolved={} pending={}",rows.len(),rows.iter().filter(|r|r["responseSent"]==true).count(),rows.iter().filter(|r|r["responseSent"]==false).count());
 }

}
