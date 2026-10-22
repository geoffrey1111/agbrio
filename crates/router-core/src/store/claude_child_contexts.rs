//! Parent-scoped child readers/owner replies, without visible independent watches.
use super::*;
fn generation(root:&str,revision:i64,key:&str,cwd:&str)->Result<i64,String>{let hash=length_prefixed_hash(&[b"claude-child-v1",root.as_bytes(),revision.to_string().as_bytes(),key.as_bytes(),cwd.as_bytes()]);Ok(-(i64::from_str_radix(&hash[..13],16).map_err(|_|"REPLY_CONTEXT_INVALID")?+1))}
pub(super) fn context(c:&Connection,key:&str)->Result<codex_watch::CodexWatch,String>{
 let row:Option<(String,i64,String,String,i64)>=c.query_row("SELECT root,generation,cwd,label,valid_until FROM claude_child_contexts WHERE key=?1",[key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(db_error)?;
 let Some((root,revision,cwd,label,until))=row else{return Err("WATCH_NOT_FOUND".into())};
 let parent=codex_watch::primary_chat_context(c,&root)?;
 if root==key||!parent.enabled||parent.generation!=revision||until<now(){return Err("REPLY_TARGET_CHANGED_REFRESH".into())}
 Ok(codex_watch::CodexWatch{thread_id:key.into(),label,cwd:cwd.clone(),enabled:true,generation:generation(&root,revision,key,&cwd)?,snapshot:codex_watch::WatchSnapshot{state:"UNKNOWN".into(),turn_id:None,item_id:None,text:String::new()},checked_at:0,error_code:None,retry_after:0,send_count:0})
}
impl RouterStore {
 pub fn open_claude_child_context(&self,root:&str,revision:i64,key:&str,cwd:&str,label:&str)->Result<(),String>{
  claude_manual::native_id(root)?;claude_manual::native_id(key)?;if root==key||!Path::new(cwd).is_absolute(){return Err("CLAUDE_CHILD_NOT_IN_TREE".into())}
  self.with_connection(|c|{let parent=codex_watch::primary_chat_context(c,root)?;if !parent.enabled||parent.generation!=revision{return Err("REPLY_TARGET_CHANGED_REFRESH".into())}
   c.execute("INSERT INTO claude_child_contexts VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(key) DO UPDATE SET root=excluded.root,generation=excluded.generation,cwd=excluded.cwd,label=excluded.label,valid_until=excluded.valid_until",params![key,root,revision,cwd,label,now()+15000]).map_err(db_error)?;Ok(())
  })
 }
 pub fn claude_child_context_origin(&self,key:&str)->Result<String,String>{self.with_connection(|c|{
  if codex_watch::primary_chat_context(c,key).is_ok(){return Ok(key.into())}
  let child=context(c,key)?;let _=child;c.query_row("SELECT root FROM claude_child_contexts WHERE key=?1",[key],|r|r.get(0)).map_err(db_error)
 })}
 pub fn claude_child_contexts(&self)->Result<Vec<(String,String,i64,String)>,String>{self.with_connection(|c|{let mut q=c.prepare("SELECT key,root,generation,cwd FROM claude_child_contexts ORDER BY valid_until DESC LIMIT 256").map_err(db_error)?;let rows=q.query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;Ok(rows)})}
 pub fn renew_claude_child_context(&self,key:&str,valid:bool)->Result<(),String>{self.with_connection(|c|{c.execute("UPDATE claude_child_contexts SET valid_until=?2 WHERE key=?1",params![key,if valid{now()+15000}else{0}]).map_err(db_error)?;Ok(())})}
}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn child_reply_context_inherits_parent_fences_without_creating_a_watch(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("router.db")).unwrap();let root="claude_code:10000000-0000-4000-8000-000000000001";let child="claude_code:20000000-0000-4000-8000-000000000001";let cwd=d.path().to_str().unwrap();
 let snapshot=codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()};s.enable_codex_watch(root,"manager",cwd,&snapshot).unwrap();
 assert!(s.open_claude_child_context(root,2,child,cwd,"child").is_err());s.open_claude_child_context(root,1,child,cwd,"child").unwrap();let first=s.codex_chat_context(child).unwrap();assert!(first.enabled&&first.generation<0);assert_eq!(s.codex_watches().unwrap().len(),1);assert_eq!(s.claude_child_context_origin(child).unwrap(),root);
 s.renew_claude_child_context(child,false).unwrap();assert!(s.codex_chat_context(child).is_err());s.renew_claude_child_context(child,true).unwrap();assert_eq!(s.codex_chat_context(child).unwrap().generation,first.generation);
 s.pause_codex_watch(root).unwrap();assert!(s.codex_chat_context(child).is_err());s.enable_codex_watch(root,"manager",cwd,&snapshot).unwrap();assert!(s.codex_chat_context(child).is_err());let parent=s.codex_chat_context(root).unwrap();s.open_claude_child_context(root,parent.generation,child,cwd,"child").unwrap();assert_ne!(s.codex_chat_context(child).unwrap().generation,first.generation);
 s.enable_codex_watch(child,"independent",cwd,&snapshot).unwrap();assert_eq!(s.codex_chat_context(child).unwrap().generation,1);assert_eq!(s.claude_child_context_origin(child).unwrap(),child);
 }
}
