use crate::host_application::RouterCore;
use router_core::store::claude_manual::native_id;
use serde_json::Value;
pub(crate) fn open(core:&RouterCore,key:&str,revision:i64,child:&str)->Result<Value,String>{
 let parent=core.store.codex_chat_context(key)?;if !parent.enabled||parent.generation!=revision{return Err("REPLY_TARGET_CHANGED_REFRESH".into())}
 let tree=crate::claude_task_tree::read(core,native_id(key)?,&parent.cwd);let node=tree.nodes.iter().find(|n|n.session_id==child).ok_or("CLAUDE_CHILD_NOT_IN_TREE")?;
 let root=core.store.claude_child_context_origin(key)?;let context=core.store.codex_chat_context(&root)?;let child_key=format!("claude_code:{child}");
 core.store.open_claude_child_context(&root,context.generation,&child_key,&node.cwd,&node.title)?;
 crate::claude_chat::state(core,&child_key)
}
pub(crate) fn renew(core:&RouterCore){
 let Ok(rows)=core.store.claude_child_contexts()else{return};
 for (key,root,generation,cwd) in rows {
  let valid=core.store.codex_chat_context(&root).ok().filter(|w|w.enabled&&w.generation==generation).is_some_and(|w|native_id(&root).ok().is_some_and(|sid|native_id(&key).ok().is_some_and(|child|crate::claude_task_tree::read(core,sid,&w.cwd).nodes.iter().any(|n|n.session_id==child&&router_core::claude_code::same_root(&n.cwd,&cwd)))));
  let _=core.store.renew_claude_child_context(&key,valid);
 }
}
