//! Durable receipts for reviewed native Goal controls; never chat-message sends.
use super::*;
#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct GoalControlReceipt{pub id:String,pub thread_id:String,pub payload_hash:String,pub status:String,pub result:Option<Value>,pub error_code:Option<String>}
pub(super) fn migrate(c:&Connection)->Result<(),String>{
 c.execute_batch("CREATE TABLE IF NOT EXISTS goal_control_actions(id TEXT PRIMARY KEY,thread_id TEXT NOT NULL,payload_hash TEXT NOT NULL,status TEXT NOT NULL CHECK(status IN ('READY','SENDING','APPLIED','REJECTED','UNKNOWN')),result_json TEXT,error_code TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS goal_control_thread ON goal_control_actions(thread_id,created_at);").map_err(db_error)
}
fn receipt(c:&Connection,thread:&str,id:&str)->Result<Option<GoalControlReceipt>,String>{
 c.query_row("SELECT id,thread_id,payload_hash,status,result_json,error_code FROM goal_control_actions WHERE id=?1 AND thread_id=?2",params![id,thread],|r|{let raw:Option<String>=r.get(4)?;Ok(GoalControlReceipt{id:r.get(0)?,thread_id:r.get(1)?,payload_hash:r.get(2)?,status:r.get(3)?,result:raw.and_then(|v|serde_json::from_str(&v).ok()),error_code:r.get(5)?})}).optional().map_err(db_error)
}
impl RouterStore {
 pub fn goal_control_receipt(&self,thread:&str,id:&str)->Result<Option<GoalControlReceipt>,String>{self.with_connection(|c|receipt(c,thread,id))}
 pub fn prepare_goal_control(&self,thread:&str,id:&str,hash:&str)->Result<GoalControlReceipt,String>{
  if Uuid::parse_str(id).is_err()||hash.len()!=64{return Err("GOAL_CONTROL_ID_INVALID".into());}
  self.with_connection(|c|{let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   if let Some(old)=receipt(&tx,thread,id)?{if old.payload_hash!=hash{return Err("GOAL_CONTROL_ARGUMENTS_CHANGED".into());}return Ok(old);}
   tx.execute("INSERT INTO goal_control_actions(id,thread_id,payload_hash,status,created_at,updated_at) VALUES(?1,?2,?3,'READY',?4,?4)",params![id,thread,hash,now()]).map_err(db_error)?;
   let r=receipt(&tx,thread,id)?.ok_or("GOAL_CONTROL_RECEIPT_UNAVAILABLE")?;tx.commit().map_err(db_error)?;Ok(r)
  })
 }
 pub fn claim_goal_control(&self,thread:&str,id:&str)->Result<(),String>{self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
  let busy:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM goal_control_actions WHERE thread_id=?1 AND id<>?2 AND status IN ('SENDING','UNKNOWN'))",params![thread,id],|r|r.get(0)).map_err(db_error)?;
  if busy{return Err("GOAL_CONTROL_PREVIOUS_UNCERTAIN".into());}
  if tx.execute("UPDATE goal_control_actions SET status='SENDING',updated_at=?3 WHERE id=?1 AND thread_id=?2 AND status='READY'",params![id,thread,now()]).map_err(db_error)?!=1{return Err("GOAL_CONTROL_ALREADY_ATTEMPTED".into());}
  tx.commit().map_err(db_error)
 })}
 pub fn finish_goal_control(&self,thread:&str,id:&str,status:&str,result:Option<&Value>,code:Option<&str>)->Result<(),String>{
  if !matches!(status,"APPLIED"|"UNKNOWN"|"REJECTED"){return Err("GOAL_CONTROL_STATUS_INVALID".into());}
  self.with_connection(|c|{if c.execute("UPDATE goal_control_actions SET status=?3,result_json=?4,error_code=?5,updated_at=?6 WHERE id=?1 AND thread_id=?2 AND status=?7",params![id,thread,status,result.map(Value::to_string),code,now(),if status=="REJECTED"{"READY"}else{"SENDING"}]).map_err(db_error)?!=1{return Err("GOAL_CONTROL_ALREADY_ATTEMPTED".into());}Ok(())})
 }
 pub fn require_goal_control_clear(&self,thread:&str)->Result<(),String>{
  self.require_no_watch_reply_writer(thread)?;
  self.with_connection(|c|{
   let unresolved:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM handoffs h JOIN endpoints e ON e.id=h.destination_endpoint_id WHERE e.provider='CODEX' AND e.external_id=?1 AND h.status IN ('SENDING','UNKNOWN')) OR EXISTS(SELECT 1 FROM provider_runs p JOIN endpoints e ON e.id=p.endpoint_id WHERE e.provider='CODEX' AND e.external_id=?1 AND p.status IN ('UNKNOWN','STARTING')) OR EXISTS(SELECT 1 FROM assistant_decisions d JOIN handoffs h ON h.id=d.handoff_id JOIN endpoints e ON e.id IN (h.source_endpoint_id,h.destination_endpoint_id) WHERE e.provider='CODEX' AND e.external_id=?1 AND d.answer IS NULL AND h.status IN ('READY','APPROVED','SENDING','UNKNOWN')) OR EXISTS(SELECT 1 FROM assistant_actions a WHERE COALESCE(json_extract(a.input_json,'$.threadId'),json_extract(a.input_json,'$.target.threadId'))=?1 AND a.status='READY' AND a.question IS NOT NULL AND a.answer IS NULL)",[thread],|r|r.get(0)).map_err(db_error)?;
   if unresolved{return Err("GOAL_CONTROL_PENDING_OR_UNCERTAIN".into());}Ok(())
  })
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn receipt_identity_and_uncertainty_survive_restart_and_block_another_request(){
  let d=tempfile::tempdir().unwrap();let p=d.path().join("goal.db");let s=RouterStore::open_at(&p).unwrap();let id=Uuid::new_v4().to_string();let hash="a".repeat(64);
  s.prepare_goal_control("exact",&id,&hash).unwrap();s.claim_goal_control("exact",&id).unwrap();s.finish_goal_control("exact",&id,"UNKNOWN",None,Some("GOAL_CONTROL_UNCERTAIN")).unwrap();drop(s);
  let s=RouterStore::open_at(&p).unwrap();assert_eq!(s.prepare_goal_control("exact",&id,&hash).unwrap().status,"UNKNOWN");assert!(s.prepare_goal_control("exact",&id,&"b".repeat(64)).is_err());assert!(s.claim_goal_control("exact",&id).is_err());
  let next=Uuid::new_v4().to_string();s.prepare_goal_control("exact",&next,&hash).unwrap();assert!(s.claim_goal_control("exact",&next).is_err());assert!(s.goal_control_receipt("other",&id).unwrap().is_none());
 }
}
