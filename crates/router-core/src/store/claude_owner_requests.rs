//! Individual owner decisions, separate from prompts and permission settings.
use super::*;
use serde::{Deserialize,Serialize};
#[derive(Clone,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct NativeRequest {pub id:String,pub session_id:String,pub instance_id:String,pub cwd:String,pub tool_use_id:String,pub kind:String,pub payload:Value}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct OwnerRequest {pub id:String,pub session_id:String,pub kind:String,pub payload:Value,pub status:String,pub revision:i64}
fn valid_context(c:&Connection,root:&str,generation:i64)->bool{codex_watch::chat_context(c,root).is_ok_and(|w|w.enabled&&w.generation==generation)}
fn validate_payload(kind:&str,payload:&Value)->Result<(),String>{
 if payload.to_string().len()>131072{return Err("CLAUDE_REQUEST_TOO_LARGE".into())}
 if kind=="QUESTION" {
  let qs=payload["questions"].as_array().ok_or("CLAUDE_QUESTION_INVALID")?;
  if qs.is_empty()||qs.len()>4{return Err("CLAUDE_QUESTION_INVALID".into())}
  let mut keys=std::collections::HashSet::new();
  for q in qs{let text=q["question"].as_str().filter(|s|!s.trim().is_empty()&&s.len()<=16000).ok_or("CLAUDE_QUESTION_INVALID")?;if !keys.insert(text){return Err("CLAUDE_QUESTION_INVALID".into())}let options=q["options"].as_array().ok_or("CLAUDE_QUESTION_INVALID")?;if options.len()>12||options.iter().any(|o|o["label"].as_str().is_none_or(|s|s.is_empty()||s.len()>2000)){return Err("CLAUDE_QUESTION_INVALID".into())}}
 }else if kind=="PERMISSION"{if payload["tool"].as_str().is_none_or(|s|s.is_empty()||s.len()>200){return Err("CLAUDE_PERMISSION_INVALID".into())}}
 else{return Err("CLAUDE_REQUEST_KIND_INVALID".into())}Ok(())
}
impl RouterStore {
 pub fn capture_claude_owner_request(&self,token:&str,root:&str,generation:i64,input:&NativeRequest)->Result<bool,String>{
  if Uuid::parse_str(&input.id).is_err()||input.tool_use_id.is_empty()||input.tool_use_id.len()>200{return Err("CLAUDE_REQUEST_INVALID".into())}validate_payload(&input.kind,&input.payload)?;
  self.with_connection(|c|{let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   claude_manual::verify_publisher(&tx,token,&input.session_id,&input.instance_id,&input.cwd)?;
   let cwd:String=tx.query_row("SELECT cwd FROM claude_sessions WHERE session_id=?1",[&input.session_id],|r|r.get(0)).map_err(db_error)?;
   if !crate::claude_code::same_root(&cwd,&input.cwd)||!valid_context(&tx,root,generation){return Ok(false)}
   let raw=input.payload.to_string();let hash=length_prefixed_hash(&[input.session_id.as_bytes(),input.instance_id.as_bytes(),input.cwd.as_bytes(),input.tool_use_id.as_bytes(),input.kind.as_bytes(),raw.as_bytes()]);
   let old:Option<String>=tx.query_row("SELECT payload_hash FROM claude_owner_requests WHERE id=?1",[&input.id],|r|r.get(0)).optional().map_err(db_error)?;
   if let Some(old)=old{if old!=hash{return Err("CLAUDE_REQUEST_REPLAY_CONFLICT".into())}return Ok(true)}
   tx.execute("INSERT INTO claude_owner_requests VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'PENDING',NULL,NULL,?11,?11)",params![input.id,input.session_id,input.instance_id,input.cwd,root,generation,input.tool_use_id,input.kind,raw,hash,now()]).map_err(db_error)?;tx.commit().map_err(db_error)?;Ok(true)
  })
 }
 pub fn claude_owner_requests(&self,root:&str,generation:i64,sid:&str)->Result<Vec<OwnerRequest>,String>{self.with_connection(|c|{
  if !valid_context(c,root,generation){return Err("REPLY_TARGET_CHANGED_REFRESH".into())}
  let mut q=c.prepare("SELECT id,session_id,kind,payload_json,status,root,generation FROM claude_owner_requests r WHERE session_id=?3 AND ((status IN ('PENDING','READY','OFFERED') AND EXISTS(SELECT 1 FROM claude_sessions s WHERE s.session_id=r.session_id AND s.instance_id=r.instance_id AND s.lease_until>?4 AND s.state!='SESSION_END')) OR status='UNKNOWN' OR (status IN ('APPLIED','NATIVE') AND updated_at>?4-60000)) ORDER BY CASE WHEN status IN ('PENDING','READY','OFFERED') THEN 0 WHEN status='UNKNOWN' THEN 1 ELSE 2 END,created_at DESC LIMIT 12").map_err(db_error)?;
  let rows=q.query_map(params![root,generation,sid,now()],|r|{let raw:String=r.get(3)?;Ok((OwnerRequest{id:r.get(0)?,session_id:r.get(1)?,kind:r.get(2)?,payload:serde_json::from_str(&raw).unwrap_or(Value::Null),status:r.get(4)?,revision:1},r.get::<_,String>(5)?,r.get::<_,i64>(6)?))}).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;Ok(rows.into_iter().filter(|(_,root,revision)|valid_context(c,root,*revision)).map(|(request,_,_)|request).collect())
 })}
 pub fn answer_claude_owner_request(&self,root:&str,generation:i64,sid:&str,id:&str,revision:i64,answer:&Value)->Result<(),String>{self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;if revision!=1||!valid_context(&tx,root,generation){return Err("REPLY_TARGET_CHANGED_REFRESH".into())}
  let row:Option<(String,String,String,Option<String>,String,i64)>=tx.query_row("SELECT kind,payload_json,status,answer_json,root,generation FROM claude_owner_requests r WHERE id=?1 AND session_id=?4 AND EXISTS(SELECT 1 FROM claude_sessions s WHERE s.session_id=r.session_id AND s.instance_id=r.instance_id AND s.lease_until>?5 AND s.state!='SESSION_END')",params![id,root,generation,sid,now()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional().map_err(db_error)?;
  let Some((kind,raw,status,previous,captured_root,captured_revision))=row else{return Err("CLAUDE_REQUEST_EXPIRED".into())};if !valid_context(&tx,&captured_root,captured_revision){return Err("CLAUDE_REQUEST_EXPIRED".into())}let encoded=answer.to_string();
  if status!="PENDING"{if previous.as_deref()==Some(&encoded){return Ok(())}return Err("CLAUDE_REQUEST_ALREADY_ANSWERED".into())}
  let action=answer["action"].as_str().ok_or("CLAUDE_ANSWER_INVALID")?;let fields=answer.as_object().ok_or("CLAUDE_ANSWER_INVALID")?;if fields.keys().any(|k|k!="action"&&!(kind=="QUESTION"&&action=="ANSWER"&&k=="answers")){return Err("CLAUDE_ANSWER_INVALID".into())}
  if action!="NATIVE"{if kind=="QUESTION"{
   if action!="ANSWER"{return Err("CLAUDE_ANSWER_INVALID".into())}let payload:Value=serde_json::from_str(&raw).map_err(|_|"CLAUDE_REQUEST_INVALID")?;let qs=payload["questions"].as_array().ok_or("CLAUDE_REQUEST_INVALID")?;let answers=answer["answers"].as_object().ok_or("CLAUDE_ANSWER_INVALID")?;
   if qs.len()!=answers.len()||qs.iter().any(|q|q["question"].as_str().and_then(|key|answers.get(key)).and_then(Value::as_str).is_none_or(|v|v.trim().is_empty()||v.len()>16000)){return Err("CLAUDE_ANSWER_INVALID".into())}
  }else if !matches!(action,"ALLOW_ONCE"|"DENY"){return Err("CLAUDE_ANSWER_INVALID".into())}}
  tx.execute("UPDATE claude_owner_requests SET status=?2,answer_json=?3,updated_at=?4 WHERE id=?1 AND status='PENDING'",params![id,if action=="NATIVE"{"NATIVE"}else{"READY"},encoded,now()]).map_err(db_error)?;tx.commit().map_err(db_error)
 })}
 /// A decision is claimed before the HTTP response. An uncertain offer is never replayed.
 pub fn poll_claude_owner_request(&self,token:&str,sid:&str,instance:&str,cwd:&str,id:&str)->Result<Value,String>{self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;claude_manual::verify_publisher(&tx,token,sid,instance,cwd)?;
  let row:Option<(String,i64,String,Option<String>)>=tx.query_row("SELECT root,generation,status,answer_json FROM claude_owner_requests WHERE id=?1 AND session_id=?2 AND instance_id=?3",params![id,sid,instance],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(db_error)?;
  let Some((root,generation,status,answer))=row else{return Err("CLAUDE_REQUEST_EXPIRED".into())};
  if !valid_context(&tx,&root,generation){tx.execute("UPDATE claude_owner_requests SET status=CASE WHEN status='OFFERED' THEN 'UNKNOWN' ELSE 'EXPIRED' END,updated_at=?2 WHERE id=?1",params![id,now()]).map_err(db_error)?;tx.commit().map_err(db_error)?;return Ok(serde_json::json!({"status":"EXPIRED"}))}
  if status=="READY"{tx.execute("UPDATE claude_owner_requests SET status='OFFERED',updated_at=?2 WHERE id=?1",params![id,now()]).map_err(db_error)?;tx.commit().map_err(db_error)?;return Ok(serde_json::json!({"status":"ANSWER","answer":answer.and_then(|a|serde_json::from_str::<Value>(&a).ok())}))}
  tx.commit().map_err(db_error)?;Ok(serde_json::json!({"status":if status=="OFFERED"{"UNKNOWN"}else{&status}}))
 })}
 pub fn claude_owner_request_tool(&self,id:&str,sid:&str,instance:&str)->Result<String,String>{self.with_connection(|c|c.query_row("SELECT tool_use_id FROM claude_owner_requests WHERE id=?1 AND session_id=?2 AND instance_id=?3 AND status IN ('OFFERED','UNKNOWN')",params![id,sid,instance],|r|r.get(0)).optional().map_err(db_error)?.ok_or("CLAUDE_REQUEST_EXPIRED".into()))}

 pub fn unconfirmed_claude_owner_requests(&self,sid:&str)->Result<Vec<(String,String,String,String)>,String>{self.with_connection(|c|{let mut q=c.prepare("SELECT id,tool_use_id,status,cwd FROM claude_owner_requests WHERE session_id=?1 AND status IN ('PENDING','READY','OFFERED','UNKNOWN') ORDER BY updated_at DESC LIMIT 12").map_err(db_error)?;let rows=q.query_map([sid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;Ok(rows)})}
 pub fn confirm_claude_owner_request_native(&self,id:&str,sid:&str,tool:&str,row:&str)->Result<(),String>{self.with_connection(|c|{c.execute("UPDATE claude_owner_requests SET status=CASE WHEN status IN ('PENDING','READY') THEN 'NATIVE' ELSE 'APPLIED' END,receipt_message_id=?4,updated_at=?5 WHERE id=?1 AND session_id=?2 AND tool_use_id=?3 AND status IN ('PENDING','READY','OFFERED','UNKNOWN')",params![id,sid,tool,row,now()]).map_err(db_error)?;Ok(())})}
 pub fn claude_owner_request_pending(&self,sid:&str,cwd:&str)->Result<bool,String>{self.with_connection(|c|{
  let mut q=c.prepare("SELECT r.root,r.generation FROM claude_owner_requests r JOIN claude_sessions s ON s.session_id=r.session_id AND s.instance_id=r.instance_id WHERE r.session_id=?1 AND r.cwd=?2 AND r.status IN ('PENDING','READY','OFFERED') AND s.lease_until>?3 AND s.state!='SESSION_END' LIMIT 12").map_err(db_error)?;let rows=q.query_map(params![sid,cwd,now()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;Ok(rows.into_iter().any(|(root,generation)|valid_context(c,&root,generation)))
 })}
 pub fn release_claude_owner_request(&self,token:&str,sid:&str,instance:&str,cwd:&str,id:&str)->Result<(),String>{self.with_connection(|c|{claude_manual::verify_publisher(c,token,sid,instance,cwd)?;c.execute("UPDATE claude_owner_requests SET status=CASE WHEN status IN ('OFFERED','UNKNOWN') THEN 'UNKNOWN' ELSE 'NATIVE' END,updated_at=?4 WHERE id=?1 AND session_id=?2 AND instance_id=?3 AND status NOT IN ('APPLIED','EXPIRED')",params![id,sid,instance,now()]).map_err(db_error)?;Ok(())})}

 pub fn authorize_claude_owner_publisher(&self,token:&str,sid:&str,instance:&str,cwd:&str)->Result<(),String>{self.with_connection(|c|claude_manual::verify_publisher(c,token,sid,instance,cwd))}
 pub fn claude_owner_request_confirmed(&self,id:&str,sid:&str,instance:&str)->Result<bool,String>{self.with_connection(|c|c.query_row("SELECT EXISTS(SELECT 1 FROM claude_owner_requests WHERE id=?1 AND session_id=?2 AND instance_id=?3 AND status='APPLIED' AND receipt_message_id IS NOT NULL)",params![id,sid,instance],|r|r.get(0)).map_err(db_error))}

 pub fn claude_owner_request_targets(&self)->Result<Vec<String>,String>{self.with_connection(|c|{
 c.execute("UPDATE claude_owner_requests SET status='EXPIRED',updated_at=?1 WHERE status IN ('PENDING','READY') AND created_at<?1-900000",[now()]).map_err(db_error)?;
 let mut q=c.prepare("SELECT DISTINCT session_id FROM claude_owner_requests WHERE status IN ('PENDING','READY','OFFERED','UNKNOWN') LIMIT 128").map_err(db_error)?;let rows=q.query_map([],|r|r.get(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;Ok(rows)
 })}

}

#[cfg(test)]mod tests{
 use super::*;use crate::claude_code::Event;
 fn fixture()->(tempfile::TempDir,RouterStore,String,NativeRequest,String){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("router.db")).unwrap();let pair=s.create_claude_connection().unwrap();let sid="10000000-0000-4000-8000-000000000001";let instance="20000000-0000-4000-8000-000000000001";let cwd=d.path().to_string_lossy().to_string();
 s.ingest_claude_event(&pair.token,&Event{event_id:Uuid::new_v4().to_string(),session_id:sid.into(),instance_id:instance.into(),sequence:1,cwd:cwd.clone(),version:"2.1.304".into(),kind:"SESSION_START".into(),turn_id:None,text:None,completed_at:None}).unwrap();let root=format!("claude_code:{sid}");s.enable_codex_watch(&root,"fixture",&cwd,&codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()}).unwrap();
 let request=NativeRequest{id:Uuid::new_v4().to_string(),session_id:sid.into(),instance_id:instance.into(),cwd,tool_use_id:"native-question".into(),kind:"QUESTION".into(),payload:serde_json::json!({"questions":[{"question":"Choose transport","header":"Transport","options":[{"label":"A"},{"label":"B"}],"multiSelect":false}]})};(d,s,pair.token,request,root)
 }
 #[test]fn owner_answers_are_fenced_exactly_and_offered_once_without_a_prompt(){
 let(_d,s,token,r,root)=fixture();assert!(s.capture_claude_owner_request("wrong",&root,1,&r).is_err());assert!(s.capture_claude_owner_request(&token,&root,1,&r).unwrap());assert!(s.capture_claude_owner_request(&token,&root,1,&r).unwrap());
 let mut changed=r.clone();changed.payload["questions"][0]["question"]="Changed".into();assert!(s.capture_claude_owner_request(&token,&root,1,&changed).is_err());
 assert_eq!(s.claude_owner_requests(&root,1,&r.session_id).unwrap().len(),1);assert!(s.claude_owner_request_pending(&r.session_id,&r.cwd).unwrap());
 let answer=serde_json::json!({"action":"ANSWER","answers":{"Choose transport":"A"}});assert!(s.answer_claude_owner_request(&root,2,&r.session_id,&r.id,1,&answer).is_err());assert!(s.answer_claude_owner_request(&root,1,"other",&r.id,1,&answer).is_err());assert!(s.answer_claude_owner_request(&root,1,&r.session_id,&r.id,2,&answer).is_err());
 assert!(s.answer_claude_owner_request(&root,1,&r.session_id,&r.id,1,&serde_json::json!({"action":"ANSWER","answers":{"wrong":"A"}})).is_err());
 s.answer_claude_owner_request(&root,1,&r.session_id,&r.id,1,&answer).unwrap();s.answer_claude_owner_request(&root,1,&r.session_id,&r.id,1,&answer).unwrap();assert!(s.answer_claude_owner_request(&root,1,&r.session_id,&r.id,1,&serde_json::json!({"action":"ANSWER","answers":{"Choose transport":"B"}})).is_err());
 let first=s.poll_claude_owner_request(&token,&r.session_id,&r.instance_id,&r.cwd,&r.id).unwrap();assert_eq!(first["status"],"ANSWER");assert_eq!(first["answer"],answer);let again=s.poll_claude_owner_request(&token,&r.session_id,&r.instance_id,&r.cwd,&r.id).unwrap();assert_eq!(again["status"],"UNKNOWN");assert!(again.get("answer").is_none());assert!(!s.claude_owner_request_confirmed(&r.id,&r.session_id,&r.instance_id).unwrap());
 s.confirm_claude_owner_request_native(&r.id,&r.session_id,&r.tool_use_id,"40000000-0000-4000-8000-000000000001").unwrap();assert!(s.claude_owner_request_confirmed(&r.id,&r.session_id,&r.instance_id).unwrap());assert!(!s.claude_owner_request_pending(&r.session_id,&r.cwd).unwrap());assert!(s.watch_replies(&root).unwrap().is_empty());
 }
 #[test]fn restart_and_pause_never_replay_a_decision(){
 let(d,s,token,r,root)=fixture();s.capture_claude_owner_request(&token,&root,1,&r).unwrap();let answer=serde_json::json!({"action":"ANSWER","answers":{"Choose transport":"A"}});s.answer_claude_owner_request(&root,1,&r.session_id,&r.id,1,&answer).unwrap();s.poll_claude_owner_request(&token,&r.session_id,&r.instance_id,&r.cwd,&r.id).unwrap();drop(s);let s=RouterStore::open_at(d.path().join("router.db")).unwrap();assert_eq!(s.poll_claude_owner_request(&token,&r.session_id,&r.instance_id,&r.cwd,&r.id).unwrap()["status"],"UNKNOWN");s.pause_codex_watch(&root).unwrap();assert_eq!(s.poll_claude_owner_request(&token,&r.session_id,&r.instance_id,&r.cwd,&r.id).unwrap()["status"],"EXPIRED");assert!(s.answer_claude_owner_request(&root,1,&r.session_id,&r.id,1,&answer).is_err());
 }
 #[test]fn permission_card_cannot_change_native_permission_settings(){
 let(_d,s,token,mut r,root)=fixture();r.kind="PERMISSION".into();r.payload=serde_json::json!({"tool":"Bash","input":{"command":"echo fictional"}});s.capture_claude_owner_request(&token,&root,1,&r).unwrap();assert!(s.answer_claude_owner_request(&root,1,&r.session_id,&r.id,1,&serde_json::json!({"action":"ALLOW_ONCE","updatedPermissions":[{"type":"setMode","mode":"bypassPermissions"}]})).is_err());s.answer_claude_owner_request(&root,1,&r.session_id,&r.id,1,&serde_json::json!({"action":"ALLOW_ONCE"})).unwrap();assert_eq!(s.poll_claude_owner_request(&token,&r.session_id,&r.instance_id,&r.cwd,&r.id).unwrap()["answer"]["action"],"ALLOW_ONCE");
 }
 #[test]fn native_completion_retires_a_card_and_pending_cards_expire(){
 let(_d,s,token,r,root)=fixture();s.capture_claude_owner_request(&token,&root,1,&r).unwrap();s.confirm_claude_owner_request_native(&r.id,&r.session_id,&r.tool_use_id,"native-row").unwrap();assert_eq!(s.claude_owner_requests(&root,1,&r.session_id).unwrap()[0].status,"NATIVE");assert!(!s.claude_owner_request_confirmed(&r.id,&r.session_id,&r.instance_id).unwrap());
 let mut second=r.clone();second.id=Uuid::new_v4().to_string();second.tool_use_id="next-tool".into();s.capture_claude_owner_request(&token,&root,1,&second).unwrap();s.with_connection(|c|{c.execute("UPDATE claude_owner_requests SET created_at=0 WHERE id=?1",[&second.id]).map_err(db_error)?;Ok(())}).unwrap();s.claude_owner_request_targets().unwrap();assert!(!s.claude_owner_request_pending(&r.session_id,&r.cwd).unwrap());
 }
 #[test]fn a_child_card_can_be_answered_from_its_parent_or_derived_conversation(){
 let(_d,s,token,mut r,root)=fixture();let child="claude_code:30000000-0000-4000-8000-000000000001";r.session_id=claude_manual::native_id(child).unwrap().into();
 s.ingest_claude_event(&token,&Event{event_id:Uuid::new_v4().to_string(),session_id:r.session_id.clone(),instance_id:r.instance_id.clone(),sequence:1,cwd:r.cwd.clone(),version:"2.1.304".into(),kind:"SESSION_START".into(),turn_id:None,text:None,completed_at:None}).unwrap();s.capture_claude_owner_request(&token,&root,1,&r).unwrap();s.open_claude_child_context(&root,1,child,&r.cwd,"child").unwrap();let context=s.codex_chat_context(child).unwrap();assert_eq!(s.claude_owner_requests(child,context.generation,&r.session_id).unwrap().len(),1);
 s.answer_claude_owner_request(child,context.generation,&r.session_id,&r.id,1,&serde_json::json!({"action":"ANSWER","answers":{"Choose transport":"B"}})).unwrap();s.pause_codex_watch(&root).unwrap();assert_eq!(s.poll_claude_owner_request(&token,&r.session_id,&r.instance_id,&r.cwd,&r.id).unwrap()["status"],"EXPIRED");
 }

}
