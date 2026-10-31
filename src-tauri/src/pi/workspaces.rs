//! Desktop owner ceilings; reviewed management actions select immutable subsets.
use super::{permissions,session::plain};
use crate::host_application::RouterCore;
use router_core::store::{pi::{Task,DirectoryScope},pi_policy::{PiTaskAccess,PiWorkspacePolicy}};
use serde::{Deserialize,Serialize};
use std::path::{Path,PathBuf};

#[derive(Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub(crate) struct WorkspaceInput {
 pub roots:Vec<String>,pub machine_scope:bool,pub writable:bool,
 pub terminal:bool,pub administrator:bool,pub inherit_context:bool,pub grant_ids:Vec<String>,
}
pub(crate) fn owner_authorize(core:&RouterCore,input:WorkspaceInput)->Result<serde_json::Value,String>{
 let mut roots=if input.machine_scope{local_drives()?}else{input.roots.iter().map(|p|permissions::authorize_root(p)).collect::<Result<Vec<_>,_>>()?};
 roots.sort();roots.dedup();if roots.is_empty()||roots.len()>16{return Err("PI_ACCESS_INVALID".into());}
 if input.terminal&&!input.writable{return Err("PI_TERMINAL_REQUIRES_WRITE_SCOPE".into());}
 let grants=core.store.assistant_grants()?;
 if input.grant_ids.is_empty()||input.grant_ids.iter().any(|id|!grants.iter().any(|g|&g.id==id&&g.revoked_at.is_none()&&g.scope=="INSTANCE"&&g.expires_at>now())){return Err("PI_POLICY_ASSISTANT_REQUIRED".into());}
 let service=core.session.lock().map_err(|_|"PI_SERVICE_UNAVAILABLE")?.pi.clone();
 if let Some(old)=core.store.pi_scopes()?.into_iter().find(|s|s.root==roots[0]){
  if service.scope_in_use(&core.store,&old.id)?{return Err("PI_SCOPE_IN_USE_STOP_TASKS_FIRST".into());}
 }
 let policy=PiWorkspacePolicy{roots,writable:input.writable,terminal:input.terminal,administrator:input.administrator,inherit_context:input.inherit_context,grant_ids:input.grant_ids,machine_scope:input.machine_scope};
 core.store.pi_authorize_workspace(&policy)?;super::application::owner_settings(core)
}
fn now()->i64{std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64}

#[cfg(windows)]
fn local_drives()->Result<Vec<String>,String>{
 use windows::{core::PCWSTR,Win32::Storage::FileSystem::{GetDriveTypeW,GetLogicalDrives}};
 let drives=unsafe{GetLogicalDrives()};let mut roots=Vec::new();
 for bit in 0..26{if drives&(1<<bit)==0{continue;}let root=format!("{}:\\",(b'A'+bit)as char);let wide:Vec<u16>=root.encode_utf16().chain(Some(0)).collect();
  if unsafe{GetDriveTypeW(PCWSTR(wide.as_ptr()))}==3 {roots.push(plain(&permissions::canonical_directory(&root)?));}
 }Ok(roots)
}
#[cfg(not(windows))]
fn local_drives()->Result<Vec<String>,String>{Err("PI_MACHINE_SCOPE_WINDOWS_ONLY".into())}

fn within(path:&Path,root:&str)->Result<bool,String>{Ok(path.starts_with(permissions::canonical_directory(root)?))}
pub(crate) fn subset(core:&RouterCore,scope:&DirectoryScope,directory:&str,roots:Option<&[String]>,writable:Option<bool>,grant:Option<&str>)->Result<PiTaskAccess,String>{
 let directory=PathBuf::from(permissions::authorize_root(directory)?);
 let policy=core.store.pi_workspace_policy(&scope.id)?;
 let Some(policy)=policy else {
  permissions::admitted_directory(&scope.root,&plain(&directory))?;
  if roots.is_some()||writable==Some(true)&&!scope.writable{return Err("PI_LEGACY_SCOPE_REQUIRES_WORKSPACE_POLICY".into());}
  return Ok(PiTaskAccess{roots:vec![plain(&directory)],writable:scope.writable,terminal:false,administrator:false,inherit_context:false});
 };
 if grant.is_some_and(|g|!policy.grant_ids.iter().any(|id|id==g)){return Err("PI_SCOPE_NOT_DELEGATED_TO_ASSISTANT".into());}
 let selected=roots.map(|v|v.to_vec()).unwrap_or_else(||policy.roots.clone());
 if selected.is_empty()||selected.len()>16{return Err("PI_ACCESS_INVALID".into());}
 let mut canonical=Vec::new();
 for root in selected{
  let candidate=if policy.machine_scope&&policy.roots.iter().any(|r|r==&root){permissions::canonical_directory(&root)?}else{PathBuf::from(permissions::authorize_root(&root)?)};
  if !policy.roots.iter().any(|r|within(&candidate,r).unwrap_or(false)){return Err("PI_DIRECTORY_OUT_OF_SCOPE".into());}
  canonical.push(plain(&candidate));
 }
 canonical.sort();canonical.dedup();
 if !canonical.iter().any(|r|within(&directory,r).unwrap_or(false)){return Err("PI_WORKING_DIRECTORY_NOT_GRANTED".into());}
 if writable==Some(true)&&!policy.writable{return Err("PI_WRITE_NOT_GRANTED".into());}
 Ok(PiTaskAccess{roots:canonical,writable:writable.unwrap_or(policy.writable),terminal:policy.terminal&&writable.unwrap_or(policy.writable),administrator:policy.administrator,inherit_context:policy.inherit_context})
}
pub(crate) fn effective(core:&RouterCore,task:&Task)->Result<PiTaskAccess,String>{
 if let Some(access)=core.store.pi_task_access(&task.id)?{return Ok(access);}
 let scope=core.store.pi_scope(&task.scope_id)?;subset(core,&scope,&task.directory,None,None,None)
}
