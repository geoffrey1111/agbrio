//! Owner-initiated creation journal. A lost response never authorizes a second
//! native creation. Provider execution and first-message receipts stay separate.
use super::*;

pub(super) fn migrate(c:&mut Connection)->Result<(),String>{
 c.execute_batch("CREATE TABLE IF NOT EXISTS owner_conversation_creations (
 request_id TEXT PRIMARY KEY,payload_hash TEXT NOT NULL,status TEXT NOT NULL,
 native_id TEXT,result_json TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL
 )").map_err(db_error)
}
#[derive(Debug,Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct CreationReceipt {
 pub request_id:String,pub payload_hash:String,pub status:String,
 pub native_id:Option<String>,pub result:Option<Value>,
}
fn receipt(c:&Connection,request:&str)->Result<CreationReceipt,String>{
 c.query_row("SELECT request_id,payload_hash,status,native_id,result_json FROM owner_conversation_creations WHERE request_id=?1",[request],|r|{
  let raw:Option<String>=r.get(4)?;
  Ok(CreationReceipt{request_id:r.get(0)?,payload_hash:r.get(1)?,status:r.get(2)?,native_id:r.get(3)?,result:raw.map(|v|serde_json::from_str(&v)).transpose().map_err(|e|rusqlite::Error::FromSqlConversionFailure(4,rusqlite::types::Type::Text,Box::new(e)))?})
 }).map_err(|_|"CONVERSATION_CREATION_RECEIPT_UNAVAILABLE".into())
}
fn input_hash(request:&str,input:&Value)->Result<String,String>{
 uuid::Uuid::parse_str(request).map_err(|_|"CONVERSATION_CREATION_REQUEST_ID_INVALID")?;
 if !input.is_object(){return Err("CONVERSATION_CREATION_INPUT_INVALID".into());}
 let raw=serde_json::to_vec(input).map_err(|_|"CONVERSATION_CREATION_INPUT_INVALID")?;
 if raw.len()>150000{return Err("CONVERSATION_CREATION_INPUT_TOO_LARGE".into());}
 Ok(length_prefixed_hash(&[b"OWNER_CREATE_CONVERSATION_V1",&raw]))
}
impl RouterStore {
 /// Check the original input before any filesystem or provider preflight.
 /// A database failure is not evidence that no native attempt was admitted.
 pub fn matching_owner_conversation_creation(&self,request:&str,input:&Value)->Result<Option<CreationReceipt>,String>{
  let hash=input_hash(request,input)?;
  self.with_connection(|c|{
   let exists:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM owner_conversation_creations WHERE request_id=?1)",[request],|r|r.get(0)).map_err(db_error)?;
   if !exists{return Ok(None);}
   let r=receipt(c,request)?;
   if r.payload_hash!=hash{return Err("CONVERSATION_CREATION_REQUEST_CHANGED".into());}
   Ok(Some(r))
  })
 }
 /// `true` is returned to exactly one caller. Existing EXECUTING/UNKNOWN,
 /// CREATED and completed attempts are always readback, never another claim.
 /// Authentication and provider/input validation must precede this method.
 pub fn claim_owner_conversation_creation(&self,request:&str,input:&Value)->Result<(CreationReceipt,bool),String>{
  let hash=input_hash(request,input)?;
  self.with_connection(|c|{
   let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   let inserted=tx.execute("INSERT OR IGNORE INTO owner_conversation_creations(request_id,payload_hash,status,created_at,updated_at) VALUES(?1,?2,'EXECUTING',?3,?3)",params![request,hash,now()]).map_err(db_error)?==1;
   let r=receipt(&tx,request)?;
   if r.payload_hash!=hash{return Err("CONVERSATION_CREATION_REQUEST_CHANGED".into());}
   tx.commit().map_err(db_error)?;Ok((r,inserted))
  })
 }
 pub fn owner_conversation_creation(&self,request:&str)->Result<CreationReceipt,String>{self.with_connection(|c|receipt(c,request))}
 /// Persist the original native identity before enabling watches or sending
 /// the first instruction. Losing those later responses cannot create a twin.
 pub fn checkpoint_owner_conversation_created(&self,request:&str,native_id:&str)->Result<(),String>{
  if native_id.is_empty()||native_id.len()>256||native_id.chars().any(char::is_control){return Err("CONVERSATION_CREATION_IDENTITY_INVALID".into());}
  self.with_connection(|c|{
   if c.execute("UPDATE owner_conversation_creations SET native_id=?2,status='CREATED',updated_at=?3 WHERE request_id=?1 AND status='EXECUTING' AND native_id IS NULL",params![request,native_id,now()]).map_err(db_error)?!=1{return Err("CONVERSATION_CREATION_ALREADY_ATTEMPTED".into());}Ok(())
  })
 }
 pub fn finish_owner_conversation_creation(&self,request:&str,result:&Value,unknown:bool)->Result<CreationReceipt,String>{
  let raw=serde_json::to_string(result).map_err(|_|"CONVERSATION_CREATION_RESULT_INVALID")?;
  if raw.len()>150000{return Err("CONVERSATION_CREATION_RESULT_TOO_LARGE".into());}
  self.with_connection(|c|{
   let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   // Completion requires a checkpointed native ID. A failure without one is
   // still UNKNOWN: an unobserved native creation may already have happened.
   let changed=tx.execute("UPDATE owner_conversation_creations SET status=?2,result_json=?3,updated_at=?4 WHERE request_id=?1 AND status IN ('EXECUTING','CREATED') AND (?2='UNKNOWN' OR native_id IS NOT NULL)",params![request,if unknown{"UNKNOWN"}else{"COMPLETE"},raw,now()]).map_err(db_error)?;
   if changed!=1{return Err("CONVERSATION_CREATION_ALREADY_ATTEMPTED".into());}
   let r=receipt(&tx,request)?;tx.commit().map_err(db_error)?;Ok(r)
  })
 }
}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn lost_creation_response_is_not_reexecuted_after_reopen_and_changed_payload_is_rejected(){
  let d=tempfile::tempdir().unwrap();let path=d.path().join("router.sqlite");let request=uuid::Uuid::new_v4().to_string();let input=serde_json::json!({"provider":"CODEX","directory":"D:/fixture","text":"first instruction"});
  {let store=RouterStore::open_at(&path).unwrap();assert!(store.claim_owner_conversation_creation(&request,&input).unwrap().1);}
  let store=RouterStore::open_at(&path).unwrap();let(r,claimed)=store.claim_owner_conversation_creation(&request,&input).unwrap();assert!(!claimed);assert_eq!(r.status,"EXECUTING");
  assert!(store.claim_owner_conversation_creation(&request,&serde_json::json!({"provider":"CLAUDE_CODE"})).unwrap_err().contains("CHANGED"));
  store.checkpoint_owner_conversation_created(&request,"native-exact").unwrap();
  let r=store.finish_owner_conversation_creation(&request,&serde_json::json!({"firstMessageStatus":"UNKNOWN"}),true).unwrap();assert_eq!(r.native_id.as_deref(),Some("native-exact"));
  assert!(!store.claim_owner_conversation_creation(&request,&input).unwrap().1);
  assert!(store.finish_owner_conversation_creation(&request,&Value::Null,false).is_err());
 }
 #[test]fn completion_cannot_invent_native_identity_or_claim_first_message_was_sent(){
  let d=tempfile::tempdir().unwrap();let store=RouterStore::open_at(d.path().join("router.sqlite")).unwrap();let request=uuid::Uuid::new_v4().to_string();store.claim_owner_conversation_creation(&request,&serde_json::json!({})).unwrap();
  assert!(store.finish_owner_conversation_creation(&request,&Value::Null,false).is_err());
  store.checkpoint_owner_conversation_created(&request,"native-original").unwrap();
  let r=store.finish_owner_conversation_creation(&request,&serde_json::json!({"firstMessageSent":false}),false).unwrap();assert_eq!(r.status,"COMPLETE");assert_eq!(r.result.unwrap()["firstMessageSent"],false);
 }
}
