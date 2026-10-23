//! Cached native public-message time, independent of polling, sending and unread state.
use super::*;
impl RouterStore {
 pub fn conversation_message_at(&self,thread:&str,cwd:&str)->Result<Option<i64>,String>{
  self.with_connection(|c|c.query_row("SELECT message_at FROM conversation_message_times WHERE thread_id=?1 AND cwd=?2",params![thread,cwd],|r|r.get(0)).optional().map_err(db_error))
 }
 pub fn note_conversation_message_at(&self,thread:&str,cwd:&str,at:i64)->Result<(),String>{
  if at<=0{return Err("CONVERSATION_MESSAGE_TIME_INVALID".into());}
  self.with_connection(|c|{
   let registered:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM codex_watches WHERE thread_id=?1 AND cwd=?2 AND enabled=1 UNION ALL SELECT 1 FROM endpoints e JOIN endpoint_role_details d ON d.endpoint_id=e.id JOIN workstreams w ON w.id=e.workstream_id WHERE ((e.provider='CODEX' AND e.external_id=?1) OR (e.provider='CLAUDE_CODE' AND 'claude_code:'||e.external_id=?1)) AND d.cwd=?2 AND e.status='ACTIVE' AND w.trashed_at IS NULL AND w.archived_at IS NULL)",params![thread,cwd],|r|r.get(0)).map_err(db_error)?;
   if !registered{return Err("CONVERSATION_MESSAGE_TARGET_CHANGED".into());}
   c.execute("INSERT INTO conversation_message_times(thread_id,cwd,message_at) VALUES(?1,?2,?3) ON CONFLICT(thread_id,cwd) DO UPDATE SET message_at=excluded.message_at WHERE message_at!=excluded.message_at",params![thread,cwd,at]).map_err(db_error)?;Ok(())
  })
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn native_time_survives_polling_and_restart_but_rejects_wrong_target(){
  let d=tempfile::tempdir().unwrap();let path=d.path().join("router.db");let cwd=d.path().to_str().unwrap();let s=RouterStore::open_at(&path).unwrap();let snap=codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()};
  s.enable_codex_watch("a","A",cwd,&snap).unwrap();s.note_conversation_message_at("a",cwd,1234).unwrap();assert!(s.note_conversation_message_at("a","/other",9999).is_err());assert!(s.note_conversation_message_at("missing",cwd,9999).is_err());s.record_codex_watch("a",1,&snap).unwrap();assert_eq!(s.conversation_message_at("a",cwd).unwrap(),Some(1234));drop(s);
  let s=RouterStore::open_at(&path).unwrap();assert_eq!(s.conversation_message_at("a",cwd).unwrap(),Some(1234));s.pause_codex_watch("a").unwrap();assert!(s.note_conversation_message_at("a",cwd,9999).is_err());assert_eq!(s.conversation_message_at("a",cwd).unwrap(),Some(1234));
 }
}
