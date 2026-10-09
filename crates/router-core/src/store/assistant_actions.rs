//! Durable instance-operation intent. Same-ID retries never repeat a mutation.
use super::*;
use super::assistant::{active,bounded};
use serde::Deserialize;
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ActionInput{pub request_id:String,pub operation:String,pub input:Value}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ActionApproval{pub action_id:String,pub expected_hash:String,pub rule_id:Option<String>,pub use_owner_answer:bool,pub assessment:String}
#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct AssistantAction{
 pub id:String,pub grant_id:String,pub payload_hash:String,pub operation:String,pub input:Value,pub status:String,
 pub question:Option<String>,pub answer:Option<String>,pub answer_reference:Option<String>,pub basis:Option<String>,pub assessment:Option<String>,pub result:Option<Value>,pub error_code:Option<String>
}
fn action(c:&Connection,gid:&str,aid:&str,hash:Option<&str>)->Result<AssistantAction,String>{
 let g=active(c,gid)?;if g.scope!="INSTANCE"{return Err("ASSISTANT_INSTANCE_AUTHORITY_REQUIRED".into());}
 let a=c.query_row("SELECT id,grant_id,payload_hash,operation,input_json,status,question,answer,answer_reference,basis,assessment,result_json,error_code FROM assistant_actions WHERE id=?1 AND grant_id=?2",params![aid,gid],|r|{
  let input:String=r.get(4)?;let result:Option<String>=r.get(11)?;
  Ok(AssistantAction{id:r.get(0)?,grant_id:r.get(1)?,payload_hash:r.get(2)?,operation:r.get(3)?,input:serde_json::from_str(&input).map_err(|e|rusqlite::Error::FromSqlConversionFailure(4,rusqlite::types::Type::Text,Box::new(e)))?,status:r.get(5)?,question:r.get(6)?,answer:r.get(7)?,answer_reference:r.get(8)?,basis:r.get(9)?,assessment:r.get(10)?,result:result.map(|s|serde_json::from_str(&s)).transpose().map_err(|e|rusqlite::Error::FromSqlConversionFailure(11,rusqlite::types::Type::Text,Box::new(e)))?,error_code:r.get(12)?})
 }).map_err(|_|"ASSISTANT_ACTION_UNAVAILABLE".to_string())?;
 if hash.is_some_and(|h|h!=a.payload_hash){return Err("ASSISTANT_ACTION_CHANGED".into());}Ok(a)
}
impl RouterStore{
 pub fn prepare_assistant_action(&self,gid:&str,input:ActionInput)->Result<AssistantAction,String>{
  bounded(&input.request_id,100)?;bounded(&input.operation,80)?;if !input.input.is_object(){return Err("ASSISTANT_ARGUMENTS_INVALID".into());}
  let raw=serde_json::to_string(&input.input).map_err(|_|"ASSISTANT_ARGUMENTS_INVALID")?;if raw.len()>150000{return Err("ASSISTANT_ACTION_TOO_LARGE".into());}
  let hash=length_prefixed_hash(&[input.operation.as_bytes(),raw.as_bytes()]);
  self.with_connection(|c|{
   let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   let g=active(&tx,gid)?;if g.scope!="INSTANCE"{return Err("ASSISTANT_INSTANCE_AUTHORITY_REQUIRED".into());}
   let previous:Option<String>=tx.query_row("SELECT id FROM assistant_actions WHERE grant_id=?1 AND request_id=?2",params![gid,input.request_id],|r|r.get(0)).optional().map_err(db_error)?;
   if let Some(aid)=previous{return action(&tx,gid,&aid,Some(&hash));}
   let aid=id();tx.execute("INSERT INTO assistant_actions(id,grant_id,request_id,payload_hash,operation,input_json,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,'READY',?7,?7)",params![aid,gid,input.request_id,hash,input.operation,raw,now()]).map_err(db_error)?;
   let a=action(&tx,gid,&aid,None)?;tx.commit().map_err(db_error)?;Ok(a)
  })
 }
 pub fn assistant_action(&self,gid:&str,aid:&str,hash:Option<&str>)->Result<AssistantAction,String>{self.with_connection(|c|action(c,gid,aid,hash))}
 pub fn assistant_actions(&self,gid:&str)->Result<Vec<AssistantAction>,String>{self.with_connection(|c|{
  let g=active(c,gid)?;if g.scope!="INSTANCE"{return Err("ASSISTANT_INSTANCE_AUTHORITY_REQUIRED".into());}
  let ids=c.prepare("SELECT id FROM assistant_actions WHERE grant_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 50").map_err(db_error)?.query_map([gid],|r|r.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
  ids.iter().map(|id|action(c,gid,id,None)).collect()
 })}
 pub fn ask_assistant_action(&self,gid:&str,aid:&str,hash:&str,question:&str)->Result<AssistantAction,String>{bounded(question,8000)?;self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let a=action(&tx,gid,aid,Some(hash))?;
  if a.status!="READY"||a.answer.is_some(){return Err("ASSISTANT_ACTION_ALREADY_ATTEMPTED_OR_ANSWERED".into());}
  if a.question.as_deref().is_some_and(|q|q!=question){return Err("ASSISTANT_ACTION_QUESTION_CHANGED".into());}
  tx.execute("UPDATE assistant_actions SET question=?2,updated_at=?3 WHERE id=?1",params![aid,question,now()]).map_err(db_error)?;let a=action(&tx,gid,aid,Some(hash))?;tx.commit().map_err(db_error)?;Ok(a)
 })}
 pub fn answer_assistant_action(&self,gid:&str,aid:&str,hash:&str,answer:&str,reference:&str)->Result<AssistantAction,String>{bounded(answer,16000)?;bounded(reference,1024)?;self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let a=action(&tx,gid,aid,Some(hash))?;
  if a.status!="READY"||a.question.is_none()||a.answer.is_some(){return Err("ASSISTANT_ACTION_NOT_WAITING_FOR_ANSWER".into());}
  tx.execute("UPDATE assistant_actions SET answer=?2,answer_reference=?3,updated_at=?4 WHERE id=?1",params![aid,answer,reference,now()]).map_err(db_error)?;let a=action(&tx,gid,aid,Some(hash))?;tx.commit().map_err(db_error)?;Ok(a)
 })}
 pub fn claim_assistant_action(&self,gid:&str,input:ActionApproval)->Result<AssistantAction,String>{bounded(&input.assessment,8000)?;self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let a=action(&tx,gid,&input.action_id,Some(&input.expected_hash))?;let g=active(&tx,gid)?;
  if a.status!="READY"{return Err("ASSISTANT_ACTION_ALREADY_ATTEMPTED".into());}
  if a.operation=="ACKNOWLEDGE_UNKNOWN_CHAT"&&(a.question.is_none()||a.answer.is_none()||!input.use_owner_answer||input.rule_id.is_some()){return Err("ASSISTANT_OWNER_ANSWER_REQUIRED".into());}
  let basis=if a.question.is_some(){if a.answer.is_none()||!input.use_owner_answer||input.rule_id.is_some(){return Err("ASSISTANT_OWNER_ANSWER_REQUIRED".into());}"ASSISTANT_ATTESTED_OWNER_ANSWER".to_string()}
   else if !input.use_owner_answer&&input.rule_id.as_ref().is_some_and(|id|g.rules.iter().any(|r|&r.id==id)){format!("BRIEF_RULE:{}",input.rule_id.unwrap())}
   else if !input.use_owner_answer&&input.rule_id.is_none()&&g.approval_mode=="CONVERSATION_REVIEW"{"ASSISTANT_REVIEW".to_string()}
   else{return Err("ASSISTANT_APPROVAL_BASIS_REQUIRED".into());};
  tx.execute("UPDATE assistant_actions SET status='EXECUTING',basis=?2,assessment=?3,updated_at=?4 WHERE id=?1 AND status='READY'",params![a.id,basis,input.assessment,now()]).map_err(db_error)?;
  let out=action(&tx,gid,&a.id,None)?;tx.commit().map_err(db_error)?;Ok(out)
 })}
 pub fn finish_assistant_action(&self,aid:&str,result:Result<Value,String>)->Result<(),String>{self.with_connection(|c|{
  let(status,value,error)=match result{Ok(v)=>("APPLIED",Some(v.to_string()),None),Err(e)=>("UNKNOWN",None,Some(e))};
  if c.execute("UPDATE assistant_actions SET status=?2,result_json=?3,error_code=?4,updated_at=?5 WHERE id=?1 AND status='EXECUTING'",params![aid,status,value,error,now()]).map_err(db_error)?!=1{return Err("ASSISTANT_ACTION_ALREADY_ATTEMPTED".into());}Ok(())
 })}
}
