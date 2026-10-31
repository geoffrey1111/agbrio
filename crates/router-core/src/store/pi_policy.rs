//! Owner-created permission ceilings and immutable per-task subsets.
use super::*;
use serde::{Deserialize,Serialize};
use rusqlite::{params,OptionalExtension};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct PiWorkspacePolicy {
 pub roots:Vec<String>, pub writable:bool, pub terminal:bool,
 pub administrator:bool, pub inherit_context:bool, pub grant_ids:Vec<String>,
 pub machine_scope:bool,
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn owner_policy_revisions_and_task_access_are_fenced(){
  let base=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime");let d=tempfile::tempdir_in(base).unwrap();let s=RouterStore::open_at(d.path().join("router.db")).unwrap();let root=std::fs::canonicalize(d.path()).unwrap().to_string_lossy().into_owned();
  let mut policy=PiWorkspacePolicy{roots:vec![root.clone()],writable:true,terminal:true,administrator:true,inherit_context:true,grant_ids:vec!["manager-connection".into()],machine_scope:false};
  let scope=s.pi_authorize_workspace(&policy).unwrap();assert_eq!(s.pi_authorize_workspace(&policy).unwrap().generation,scope.generation);
  let task=s.pi_begin_create("create","manager-connection",&scope.id,scope.generation,&root,"task",super::super::pi::Parent{provider:"CODEX".into(),external_id:"native-parent".into(),cwd:root.clone()},None).unwrap();
  let access=PiTaskAccess{roots:vec![root],writable:true,terminal:true,administrator:true,inherit_context:true};s.pi_bind_task_access(&task.id,scope.generation,&access).unwrap();s.pi_bind_task_access(&task.id,scope.generation,&access).unwrap();
  let mut changed=access.clone();changed.writable=false;assert!(s.pi_bind_task_access(&task.id,scope.generation,&changed).is_err());
  policy.grant_ids.push("second-manager".into());let newer=s.pi_authorize_workspace(&policy).unwrap();assert_eq!(newer.generation,scope.generation+1);assert!(s.pi_bind_task_access(&task.id,scope.generation,&access).is_err());
  assert_eq!(s.pi_task_access(&task.id).unwrap(),Some(access));
 }
 #[test]
 fn creation_uses_every_authorized_root_and_keeps_legacy_and_principal_fences(){
  let base=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime");let d=tempfile::tempdir_in(base).unwrap();let s=RouterStore::open_at(d.path().join("cross-volume.db")).unwrap();
  let mut policy=PiWorkspacePolicy{roots:vec![r"C:\".into(),r"D:\".into(),r"E:\".into()],writable:true,terminal:true,administrator:true,inherit_context:true,grant_ids:vec!["manager".into()],machine_scope:true};
  let scope=s.pi_authorize_workspace(&policy).unwrap();assert!(scope.root.starts_with("C:"));
  let parent=super::super::pi::Parent{provider:"CODEX".into(),external_id:"native-parent".into(),cwd:r"D:\manager".into()};
  for (id,directory) in [("d-drive",r"D:\work\task"),("e-drive",r"E:\assets\task")]{let task=s.pi_begin_create(id,"manager",&scope.id,scope.generation,directory,"fixture",parent.clone(),None).unwrap();assert_eq!(task.directory,directory);assert!(task.native_session_id.is_none());}
  assert_eq!(s.pi_begin_create("f-denied","manager",&scope.id,scope.generation,r"F:\task","fixture",parent.clone(),None).unwrap_err(),"PI_DIRECTORY_OUT_OF_SCOPE");
  assert_eq!(s.pi_begin_create("principal-denied","other",&scope.id,scope.generation,r"D:\task","fixture",parent.clone(),None).unwrap_err(),"PI_SCOPE_NOT_DELEGATED_TO_ASSISTANT");
  assert_eq!(s.pi_begin_create("stale","manager",&scope.id,scope.generation+1,r"D:\task","fixture",parent.clone(),None).unwrap_err(),"PI_SCOPE_CHANGED");
  policy.roots=vec![r"D:\allowed".into()];policy.machine_scope=false;let narrow=s.pi_authorize_workspace(&policy).unwrap();
  assert!(s.pi_begin_create("nested","manager",&narrow.id,narrow.generation,r"d:/allowed/child","fixture",parent.clone(),None).is_ok());
  for (id,directory) in [("prefix-denied",r"D:\allowed-other\task"),("traversal-denied",r"D:\allowed\..\outside")]{assert_eq!(s.pi_begin_create(id,"manager",&narrow.id,narrow.generation,directory,"fixture",parent.clone(),None).unwrap_err(),"PI_DIRECTORY_OUT_OF_SCOPE");}
  let legacy=s.pi_authorize_directory(r"D:\legacy",true,false).unwrap();assert!(s.pi_begin_create("legacy-child","manager",&legacy.id,legacy.generation,r"D:\legacy\child","fixture",parent.clone(),None).is_ok());assert_eq!(s.pi_begin_create("legacy-other","manager",&legacy.id,legacy.generation,r"E:\child","fixture",parent,None).unwrap_err(),"PI_DIRECTORY_OUT_OF_SCOPE");
  assert_eq!(s.pi_tasks().unwrap().len(),4,"rejected requests must not create task records");
 }

}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct PiTaskAccess {
 pub roots:Vec<String>, pub writable:bool, pub terminal:bool,
 pub administrator:bool, pub inherit_context:bool,
}
fn bounded(roots:&[String])->Result<(),String>{
 if roots.is_empty()||roots.len()>16||roots.iter().any(|p|p.is_empty()||p.len()>4096||p.chars().any(char::is_control)){return Err("PI_ACCESS_INVALID".into());}Ok(())
}
impl RouterStore {
 pub fn pi_reviewed_action_grant(&self,action:&str,operation:&str)->Result<String,String>{
  self.with_connection(|c|{let grant:String=c.query_row("SELECT grant_id FROM assistant_actions WHERE id=?1 AND operation=?2 AND status='EXECUTING'",params![action,operation],|r|r.get(0)).map_err(|_|"PI_REVIEWED_ACTION_REQUIRED")?;let g=super::assistant::active(c,&grant)?;if g.scope!="INSTANCE"{return Err("ASSISTANT_INSTANCE_AUTHORITY_REQUIRED".into());}Ok(grant)})
 }
 pub fn pi_reconfigure_task_access(&self,task:&str,expected:i64,scope:&str,scope_generation:i64,access:&PiTaskAccess)->Result<(),String>{
  bounded(&access.roots)?;let raw=serde_json::to_string(access).map_err(|_|"PI_ACCESS_INVALID")?;
  self.with_connection(|c|{
   let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   let valid:bool=tx.query_row("SELECT enabled AND generation=?2 FROM pi_directory_scopes WHERE id=?1",params![scope,scope_generation],|r|r.get(0)).map_err(db_error)?;
   if !valid{return Err("PI_SCOPE_CHANGED".into());}
   if tx.execute("UPDATE pi_tasks SET scope_id=?3,scope_generation=?4,generation=generation+1,error_code=NULL,updated_at=?5 WHERE id=?1 AND generation=?2 AND native_session_id IS NOT NULL",params![task,expected,scope,scope_generation,now()]).map_err(db_error)?!=1{return Err("PI_TASK_GENERATION_CHANGED".into());}
   tx.execute("INSERT INTO pi_task_access(task_id,scope_generation,access_json,audit_json) VALUES(?1,?2,?3,NULL) ON CONFLICT(task_id) DO UPDATE SET scope_generation=excluded.scope_generation,access_json=excluded.access_json,audit_json=NULL",params![task,scope_generation,raw]).map_err(db_error)?;
   tx.commit().map_err(db_error)
  })
 }
 pub fn pi_execution_audit(&self,task:&str)->Result<Option<Value>,String>{
  self.with_connection(|c|{let raw:Option<String>=c.query_row("SELECT audit_json FROM pi_task_access WHERE task_id=?1",[task],|r|r.get(0)).optional().map_err(db_error)?.flatten();raw.map(|s|serde_json::from_str(&s).map_err(|_|"PI_AUDIT_INVALID".into())).transpose()})
 }
 pub fn pi_put_execution_audit(&self,task:&str,audit:&Value)->Result<(),String>{
  self.with_connection(|c|{if c.execute("UPDATE pi_task_access SET audit_json=?2 WHERE task_id=?1",params![task,audit.to_string()]).map_err(db_error)?!=1{return Err("PI_TASK_ACCESS_UNCONFIRMED".into());}Ok(())})
 }
 pub fn pi_workspace_policy(&self,id:&str)->Result<Option<PiWorkspacePolicy>,String>{
  self.with_connection(|c|{let raw:Option<String>=c.query_row("SELECT policy_json FROM pi_scope_policies WHERE scope_id=?1",[id],|r|r.get(0)).optional().map_err(db_error)?;raw.map(|s|serde_json::from_str(&s).map_err(|_|"PI_POLICY_INVALID".into())).transpose()})
 }
 /// Only the authenticated desktop owner calls this; MCP may consume subsets.
 pub fn pi_authorize_workspace(&self,policy:&PiWorkspacePolicy)->Result<super::pi::DirectoryScope,String>{
  bounded(&policy.roots)?;
  if policy.grant_ids.is_empty()||policy.grant_ids.len()>64{return Err("PI_POLICY_ASSISTANT_REQUIRED".into());}
  let raw=serde_json::to_string(policy).map_err(|_|"PI_POLICY_INVALID")?;
  self.with_connection(|c|{
   let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   let existing:Option<(String,bool,String)>=tx.query_row("SELECT s.id,s.enabled,COALESCE(p.policy_json,'') FROM pi_directory_scopes s LEFT JOIN pi_scope_policies p ON p.scope_id=s.id WHERE s.root=?1",[&policy.roots[0]],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db_error)?;
   if let Some((key,true,old))=&existing{if old==&raw{let key=key.clone();tx.commit().map_err(db_error)?;return c.query_row("SELECT id,root,writable,terminal_confirmation,enabled,generation FROM pi_directory_scopes WHERE id=?1",[key],|r|Ok(super::pi::DirectoryScope{id:r.get(0)?,root:r.get(1)?,writable:r.get(2)?,terminal_confirmation:r.get(3)?,enabled:r.get(4)?,generation:r.get(5)?})).map_err(db_error);}}
   let key=existing.map(|v|v.0).unwrap_or_else(id);let at=now();
   tx.execute("INSERT INTO pi_directory_scopes(id,root,writable,terminal_confirmation,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5) ON CONFLICT(root) DO UPDATE SET writable=excluded.writable,terminal_confirmation=excluded.terminal_confirmation,enabled=1,generation=generation+1,updated_at=excluded.updated_at",params![key,policy.roots[0],policy.writable,policy.terminal,at]).map_err(db_error)?;
   tx.execute("INSERT INTO pi_scope_policies(scope_id,policy_json,updated_at) VALUES(?1,?2,?3) ON CONFLICT(scope_id) DO UPDATE SET policy_json=excluded.policy_json,updated_at=excluded.updated_at",params![key,raw,at]).map_err(db_error)?;
   tx.commit().map_err(db_error)?;
   // Same connection: avoid re-entering the store mutex.
   c.query_row("SELECT id,root,writable,terminal_confirmation,enabled,generation FROM pi_directory_scopes WHERE id=?1",[key],|r|Ok(super::pi::DirectoryScope{id:r.get(0)?,root:r.get(1)?,writable:r.get(2)?,terminal_confirmation:r.get(3)?,enabled:r.get(4)?,generation:r.get(5)?})).map_err(db_error)
  })
 }
 pub fn pi_bind_task_access(&self,task:&str,generation:i64,access:&PiTaskAccess)->Result<(),String>{
  bounded(&access.roots)?;let raw=serde_json::to_string(access).map_err(|_|"PI_ACCESS_INVALID")?;
  self.with_connection(|c|{
   let context:(i64,bool)=c.query_row("SELECT t.scope_generation,s.enabled AND s.generation=t.scope_generation FROM pi_tasks t JOIN pi_directory_scopes s ON s.id=t.scope_id WHERE t.id=?1",[task],|r|Ok((r.get(0)?,r.get(1)?))).map_err(db_error)?;
   if context!=(generation,true){return Err("PI_SCOPE_CHANGED".into());}
   let old:Option<(i64,String)>=c.query_row("SELECT scope_generation,access_json FROM pi_task_access WHERE task_id=?1",[task],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
   if let Some(old)=old{if old!=(generation,raw){return Err("PI_TASK_ACCESS_CHANGED".into());}return Ok(());}
   c.execute("INSERT INTO pi_task_access(task_id,scope_generation,access_json) VALUES(?1,?2,?3)",params![task,generation,raw]).map_err(db_error)?;Ok(())
  })
 }
 pub fn pi_task_access(&self,task:&str)->Result<Option<PiTaskAccess>,String>{
  self.with_connection(|c|{let raw:Option<String>=c.query_row("SELECT access_json FROM pi_task_access WHERE task_id=?1",[task],|r|r.get(0)).optional().map_err(db_error)?;raw.map(|s|serde_json::from_str(&s).map_err(|_|"PI_ACCESS_INVALID".into())).transpose()})
 }
}
