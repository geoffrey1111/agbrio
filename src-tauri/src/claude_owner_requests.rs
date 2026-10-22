//! Authenticated owner decisions for a native suspended hook, never text prompts.
use crate::host_application::RouterCore;
use router_core::store::{claude_manual::native_id,claude_owner_requests::NativeRequest};
use serde::Deserialize;
use serde_json::{Value,json};
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub(crate) struct Poll {pub id:String,pub session_id:String,pub instance_id:String,pub cwd:String}
pub(crate) fn capture(core:&RouterCore,token:&str,input:&NativeRequest)->Result<Value,String>{
 core.store.authorize_claude_owner_publisher(token,&input.session_id,&input.instance_id,&input.cwd)?;
 let own=format!("claude_code:{}",input.session_id);
 let direct=core.store.codex_chat_context(&own).ok().filter(|w|w.enabled&&router_core::claude_code::same_root(&w.cwd,&input.cwd));
 let context=if let Some(w)=direct {Some((own,w.generation))}else{
  let mut targets:Vec<_>=crate::claude_watch::targets(core)?.into_iter().collect();targets.sort_by(|a,b|a.0.cmp(&b.0));
  targets.into_iter().find_map(|(sid,cwd)|{let cwd=cwd?;let key=format!("claude_code:{sid}");let w=core.store.codex_chat_context(&key).ok()?;
   (w.enabled&&crate::claude_task_tree::read(core,&sid,&cwd).nodes.iter().any(|n|n.session_id==input.session_id&&router_core::claude_code::same_root(&n.cwd,&input.cwd))).then_some((key,w.generation))})
 };
 let Some((root,generation))=context else{return Ok(json!({"captured":false}))};
 Ok(json!({"captured":core.store.capture_claude_owner_request(token,&root,generation,input)?}))
}
pub(crate) fn answer(core:&RouterCore,key:&str,generation:i64,child:Option<&str>,id:&str,revision:i64,answer:&Value)->Result<Value,String>{
 let w=core.store.codex_chat_context(key)?;if !w.enabled||w.generation!=generation{return Err("REPLY_TARGET_CHANGED_REFRESH".into())}
 let root=native_id(key)?;let sid=child.unwrap_or(root);
 if sid!=root&&!crate::claude_task_tree::read(core,root,&w.cwd).nodes.iter().any(|n|n.session_id==sid){return Err("CLAUDE_CHILD_NOT_IN_TREE".into())}
 core.store.answer_claude_owner_request(key,generation,sid,id,revision,answer)?;Ok(json!({"accepted":true}))
}
pub(crate) fn reconcile(core:&RouterCore,sid:&str){
 let Ok(rows)=core.store.unconfirmed_claude_owner_requests(sid)else{return};if rows.is_empty(){return}
 let Some(path)=crate::claude_transcript::projects_root().and_then(|r|crate::claude_transcript::find(&r,sid))else{return};
 for (id,tool,_,cwd) in rows {if !crate::claude_transcript::latest(sid).is_some_and(|p|p.cwd.as_ref().is_some_and(|v|router_core::claude_code::same_root(v,&cwd))){continue}if let Some(row)=crate::claude_transcript::find_tool_result(&path,sid,&tool){let _=core.store.confirm_claude_owner_request_native(&id,sid,&tool,&row);}}
}

pub(crate) fn tick(core:&RouterCore){if let Ok(targets)=core.store.claude_owner_request_targets(){for sid in targets{reconcile(core,&sid);}}}
