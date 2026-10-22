//! Read only native asynchronous owner questions, never reasoning/tool output.
use serde_json::{Value,json};
use std::{fs::File,io::{BufRead,BufReader,Read,Seek,SeekFrom},path::Path};
pub fn answer_id(thread:&str,request:&str)->String{
 use sha2::{Digest,Sha256};let hash=format!("{:x}",Sha256::digest(format!("agbrio-async-answer-v1:{}:{thread}:{request}",thread.len()).as_bytes()));format!("{}-{}-{}-{}-{}",&hash[..8],&hash[8..12],&hash[12..16],&hash[16..20],&hash[20..32])
}
pub fn read(path:&Path,root:&Path,thread:&str)->Vec<Value>{
 let Ok(canonical)=path.canonicalize()else{return vec![]};let Ok(root)=root.canonicalize()else{return vec![]};if !canonical.starts_with(root)||canonical.extension().is_none_or(|x|x!="jsonl"){return vec![]}
 type Stamp=(u64,std::time::SystemTime);
 type Cache=std::collections::HashMap<(std::path::PathBuf,String),(Stamp,Vec<Value>)>;
 static CACHE:std::sync::OnceLock<std::sync::Mutex<Cache>>=std::sync::OnceLock::new();
 let Ok(meta)=std::fs::metadata(&canonical)else{return vec![]};let Ok(modified)=meta.modified()else{return vec![]};let stamp=(meta.len(),modified);let key=(canonical.clone(),thread.to_owned());
 if let Ok(cache)=CACHE.get_or_init(Default::default).lock(){if let Some((old,rows))=cache.get(&key){if *old==stamp{return rows.clone()}}}
 let Ok(mut file)=File::open(canonical)else{return vec![]};let mut first=String::new();if BufReader::new((&mut file).take(2*1024*1024)).read_line(&mut first).is_err(){return vec![]}
 let Ok(meta)=serde_json::from_str::<Value>(&first)else{return vec![]};if meta["type"]!="session_meta"||meta["payload"]["id"].as_str()!=Some(thread){return vec![]}
 let Ok(len)=file.metadata().map(|m|m.len())else{return vec![]};let start=len.saturating_sub(16*1024*1024);if file.seek(SeekFrom::Start(start)).is_err(){return vec![]};let mut bytes=Vec::new();if file.take(16*1024*1024).read_to_end(&mut bytes).is_err(){return vec![]}
 let mut rows=project(&bytes);for row in &mut rows{row["answerId"]=json!(answer_id(thread,row["requestId"].as_str().unwrap_or_default()));}
 if let Ok(mut cache)=CACHE.get_or_init(Default::default).lock(){if cache.len()>=64{cache.clear()}cache.insert(key,(stamp,rows.clone()));}rows
}
fn project(bytes:&[u8])->Vec<Value>{
 let mut turn=String::new();let mut cards:Vec<Value>=vec![];let mut answered=std::collections::HashSet::new();
 for line in bytes.split(|b|*b==b'\n').filter(|l|l.len()<=2*1024*1024){let Ok(v)=serde_json::from_slice::<Value>(line)else{continue};let p=&v["payload"];
  if v["type"]=="turn_context"{turn=p["turn_id"].as_str().unwrap_or_default().into();continue}
  if v["type"]!="response_item"{continue}
  if p["type"]=="function_call"&&p["name"]=="request_user_input_async"{
   let Some(call)=p["call_id"].as_str().filter(|s|!s.is_empty()&&s.len()<=256)else{continue};let Some(args)=p["arguments"].as_str().filter(|s|s.len()<=131072).and_then(|s|serde_json::from_str::<Value>(s).ok())else{continue};let Some(questions)=args["questions"].as_array().filter(|qs|!qs.is_empty()&&qs.len()<=12)else{continue};
   let questions=questions.iter().enumerate().filter_map(|(i,q)|{let question=q["title"].as_str()?.chars().take(16000).collect::<String>();let options=q["options"].as_array().map(|options|options.iter().take(12).filter_map(|o|o.as_str().map(|s|json!({"label":s,"description":""}))).collect::<Vec<_>>()).unwrap_or_default();Some(json!({"id":json!(["request_user_input_async",call,i]).to_string(),"label":question,"placeholder":question,"required":true,"isOther":true,"options":options}))}).collect::<Vec<_>>();
   if questions.is_empty(){continue}cards.push(json!({"requestId":call,"revision":1,"method":"request_user_input_async","kind":"USER_INPUT","isBlocking":false,"responseSent":false,"turnId":turn,"questions":questions}));
  }else if p["type"]=="message"&&p["role"]=="user"{if let Some(content)=p["content"].as_array(){for item in content{let Some(text)=item["text"].as_str()else{continue};let Some(text)=text.trim().strip_prefix("<send_user_message_question_reply>").and_then(|t|t.strip_suffix("</send_user_message_question_reply>"))else{continue};let Ok(rows)=serde_json::from_str::<Vec<Value>>(text)else{continue};for r in rows{if r["answer"].as_str().is_some(){if let Some(id)=r["questionItemId"].as_str(){answered.insert(id.to_owned());}}}}}
  }
 }
 for card in &mut cards{card["responseSent"]=json!(card["questions"].as_array().is_some_and(|qs|qs.iter().all(|q|q["id"].as_str().is_some_and(|id|answered.contains(id)))));}
 cards.into_iter().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect()
}
#[cfg(test)]mod tests{
 use super::*;
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
 #[test]#[ignore="explicit exact owner transcript, read-only question metadata; no model or session operations"]fn real_native_questions_read_only(){
 let thread=std::env::var("AGBRIO_QUESTION_THREAD").unwrap();let path=std::path::PathBuf::from(std::env::var_os("AGBRIO_QUESTION_ROLLOUT").unwrap());let root=std::path::PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join(".codex/sessions");let rows=read(&path,&root,&thread);assert!(rows.iter().any(|r|r["requestId"]=="call_9c930989a3b9430e85450dfbbbd06c09"&&r["responseSent"]==true));println!("nativeQuestions={} resolved={} pending={}",rows.len(),rows.iter().filter(|r|r["responseSent"]==true).count(),rows.iter().filter(|r|r["responseSent"]==false).count());
 }

}
