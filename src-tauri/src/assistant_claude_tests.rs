use super::*;
use crate::host_application::Session;
use router_core::claude_code::Event;
use std::sync::{Arc,Mutex};

#[test]
fn reviewed_claude_text_roundtrip_uses_one_native_offer_and_independent_receipt(){
 let temp=tempfile::tempdir().unwrap();let store=Arc::new(router_core::store::RouterStore::open_at(temp.path().join("router.db")).unwrap());
 let core=RouterCore{store:store.clone(),session:Arc::new(Mutex::new(Session::default()))};
 let pair=store.create_claude_connection().unwrap();let sid=uuid::Uuid::new_v4().to_string();let instance=uuid::Uuid::new_v4().to_string();let turn=uuid::Uuid::new_v4().to_string();let thread=format!("claude_code:{sid}");
 let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
 let event=|kind:&str,n|Event{event_id:uuid::Uuid::new_v4().to_string(),session_id:sid.clone(),instance_id:instance.clone(),sequence:n,cwd:temp.path().to_string_lossy().into(),version:"2.1.295".into(),kind:kind.into(),turn_id:Some(turn.clone()),text:(kind=="COMPLETE").then(||"Fixture reply".into()),completed_at:(kind=="COMPLETE").then_some(now)};
 for (n,kind) in ["SESSION_START","RUNNING","COMPLETE"].iter().enumerate(){store.ingest_claude_event(&pair.token,&event(kind,n as i64+1)).unwrap();}
 store.enable_codex_watch(&thread,"Fixture",&temp.path().to_string_lossy(),&router_core::store::codex_watch::WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some(turn.clone()),item_id:None,text:"Fixture reply".into()}).unwrap();let watch=store.codex_chat_context(&thread).unwrap();
 let beat=event("HEARTBEAT",4);store.ingest_claude_event(&pair.token,&beat).unwrap();store.offer_claude_manual(&pair.token,&beat).unwrap();
 let grant=store.connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput{label:"Fixture".into(),expires_at:now+600000}).unwrap();
 let call=|name:&str,args|crate::assistant_mcp::call(&core,&grant.id,name,args);
 let read=call("agbrio_read_chat",json!({"threadId":thread})).unwrap();assert_eq!(read["sendCapabilities"]["text"],true);assert_eq!(read["sendContext"]["generation"],watch.generation);assert!(read["history"]["messages"].is_array());
 let input=json!({"threadId":thread,"generation":watch.generation,"text":"Authorized fixture message","mode":"AUTO"});
 let action=call("agbrio_prepare_action",json!({"requestId":"fixture-send","operation":"SEND_CHAT","input":input})).unwrap();
 let review=json!({"actionId":action["id"],"expectedHash":action["payloadHash"],"ruleId":null,"useOwnerAnswer":false,"assessment":"Authorized fixture message to the exact manager"});
 assert_eq!(call("agbrio_execute_action",review.clone()).unwrap()["status"],"APPLIED");assert_eq!(call("agbrio_execute_action",review).unwrap()["status"],"APPLIED");
 let reference=json!({"actionId":action["id"],"expectedHash":action["payloadHash"]});
 assert_eq!(call("agbrio_action_receipt",reference.clone()).unwrap()["delivery"]["status"],"QUEUED");
 let offer=store.offer_claude_manual(&pair.token,&beat).unwrap().unwrap();assert_eq!(offer.text,"Authorized fixture message");assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_none());
 let native=uuid::Uuid::new_v4().to_string();store.accept_claude_manual_receipt(&pair.token,&router_core::store::claude_manual::ManualReceipt{request_id:offer.request_id,session_id:sid.clone(),instance_id:instance.clone(),cwd:temp.path().to_string_lossy().into(),status:"SENT".into(),message_id:Some(native.clone()),text_sha256:offer.text_sha256}).unwrap();
 let receipt=call("agbrio_action_receipt",reference).unwrap();assert_eq!(receipt["delivery"]["status"],"SENT");assert_eq!(receipt["nativeMessageId"],native);
 store.ingest_claude_event(&pair.token,&event("RUNNING",5)).unwrap();
 let options=ReplyOptions::default();assert_eq!(validate(&core,&thread,watch.generation,"AUTO",false,Some(&turn),&options).unwrap_err(),"CLAUDE_BUSY_USE_EXPLICIT_QUEUE");assert!(validate(&core,&thread,watch.generation,"QUEUE",true,None,&options).is_ok());
 store.ingest_claude_event(&pair.token,&event("WAITING_DECISION",6)).unwrap();assert_eq!(validate(&core,&thread,watch.generation,"QUEUE",true,None,&options).unwrap_err(),"CLAUDE_NATIVE_APPROVAL_PENDING");
}
