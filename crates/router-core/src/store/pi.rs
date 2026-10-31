//! Persistent cross-provider delegation. Never a Claude/Codex native child tree.
use super::*;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Parent {
    pub provider: String,
    pub external_id: String,
    pub cwd: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub provider: String,
    pub model: String,
    pub thinking: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandReceipt {
    pub id: String,
    pub task_id: String,
    pub request_id: String,
    pub generation: i64,
    pub operation: String,
    pub payload_hash: String,
    pub status: String,
    pub native_entry_id: Option<String>,
    pub error_code: Option<String>,
}
fn command(c: &Connection, task: &str, request: &str) -> Result<CommandReceipt, String> {
    c.query_row("SELECT id,task_id,request_id,generation,operation,payload_hash,status,native_entry_id,error_code FROM pi_commands WHERE task_id=?1 AND request_id=?2",params![task,request],|r|Ok(CommandReceipt{id:r.get(0)?,task_id:r.get(1)?,request_id:r.get(2)?,generation:r.get(3)?,operation:r.get(4)?,payload_hash:r.get(5)?,status:r.get(6)?,native_entry_id:r.get(7)?,error_code:r.get(8)?})).map_err(|_|"PI_COMMAND_NOT_FOUND".into())
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryScope {
    pub id: String,
    pub root: String,
    pub writable: bool,
    pub terminal_confirmation: bool,
    pub enabled: bool,
    pub generation: i64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub create_action_id: String,
    pub creator_grant_id: String,
    pub scope_id: String,
    pub scope_generation: i64,
    pub directory: String,
    pub label: String,
    pub parent: Parent,
    pub native_session_id: Option<String>,
    pub native_session_file: Option<String>,
    pub file_verified: bool,
    pub generation: i64,
    pub state: String,
    pub selected: Option<Selection>,
    pub selection_source: String,
    pub runtime_epoch: Option<String>,
    pub result: Option<Value>,
    pub error_code: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
fn task(c: &Connection, key: &str) -> Result<Task, String> {
    c.query_row("SELECT id,create_action_id,creator_grant_id,scope_id,scope_generation,directory,label,parent_provider,parent_external_id,parent_cwd,native_session_id,native_session_file,file_verified,generation,state,selected_json,selection_source,runtime_epoch,result_json,error_code,created_at,updated_at FROM pi_tasks WHERE id=?1",[key],|r|{
  let selected:Option<String>=r.get(15)?;let result:Option<String>=r.get(18)?;
  Ok(Task{id:r.get(0)?,create_action_id:r.get(1)?,creator_grant_id:r.get(2)?,scope_id:r.get(3)?,scope_generation:r.get(4)?,directory:r.get(5)?,label:r.get(6)?,parent:Parent{provider:r.get(7)?,external_id:r.get(8)?,cwd:r.get(9)?},native_session_id:r.get(10)?,native_session_file:r.get(11)?,file_verified:r.get(12)?,generation:r.get(13)?,state:r.get(14)?,selected:selected.map(|v|serde_json::from_str(&v)).transpose().map_err(|e|rusqlite::Error::FromSqlConversionFailure(15,rusqlite::types::Type::Text,Box::new(e)))?,selection_source:r.get(16)?,runtime_epoch:r.get(17)?,result:result.map(|v|serde_json::from_str(&v)).transpose().map_err(|e|rusqlite::Error::FromSqlConversionFailure(18,rusqlite::types::Type::Text,Box::new(e)))?,error_code:r.get(19)?,created_at:r.get(20)?,updated_at:r.get(21)?})
 }).map_err(|_|"PI_TASK_NOT_FOUND".into())
}
fn scope(c: &Connection, key: &str) -> Result<DirectoryScope, String> {
    c.query_row("SELECT id,root,writable,terminal_confirmation,enabled,generation FROM pi_directory_scopes WHERE id=?1",[key],|r|Ok(DirectoryScope{id:r.get(0)?,root:r.get(1)?,writable:r.get(2)?,terminal_confirmation:r.get(3)?,enabled:r.get(4)?,generation:r.get(5)?})).map_err(|_|"PI_SCOPE_NOT_FOUND".into())
}
impl RouterStore {
    pub fn pi_record_terminal(
        &self,
        key: &str,
        native_receipt: &str,
        requests: &[Value],
        state: &str,
        result: &Value,
        active: bool,
    ) -> Result<bool, String> {
        if !matches!(state, "RESULT_READY" | "PARTIAL" | "FAILED" | "CANCELLED")
            || native_receipt.is_empty()
        {
            return Err("PI_TERMINAL_RECEIPT_INVALID".into());
        }
        self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;task(&tx,key)?;let mut newest=0i64;
            for request in requests{
                let id=request["requestId"].as_str().ok_or("PI_TERMINAL_RECEIPT_INVALID")?;let hash=request["payloadHash"].as_str().ok_or("PI_TERMINAL_RECEIPT_INVALID")?;
                let old=command(&tx,key,id)?;if old.payload_hash!=hash{return Err("PI_TERMINAL_BINDING_MISMATCH".into());}
                let row:i64=tx.query_row("SELECT rowid FROM pi_commands WHERE task_id=?1 AND request_id=?2",params![key,id],|r|r.get(0)).map_err(db_error)?;newest=newest.max(row);
                tx.execute("UPDATE pi_commands SET result_json=json_set(COALESCE(result_json,'{}'),'$.terminalReceiptId',?3,'$.terminalState',?4,'$.terminalResult',json(?5)),updated_at=?6 WHERE task_id=?1 AND request_id=?2 AND json_extract(result_json,'$.terminalReceiptId') IS NULL",params![key,id,native_receipt,state,result.to_string(),now()]).map_err(db_error)?;
            }
            let later:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pi_commands WHERE task_id=?1 AND rowid>?2 AND operation IN('AUTO','SEND','STEER','QUEUE') AND status NOT IN('NOT_SENT','CANCELLED'))",params![key,newest],|r|r.get(0)).map_err(db_error)?;
            let apply=active&&!later&&newest>0;
            if apply{tx.execute("UPDATE pi_tasks SET state=?2,result_json=?3,error_code=CASE WHEN ?2='FAILED' THEN json_extract(?3,'$.lastAssistant.errorCode') ELSE NULL END,updated_at=?4 WHERE id=?1 AND (state!=?2 OR COALESCE(json_extract(result_json,'$.nativeTerminalReceiptId'),'')!=?5 OR error_code IS NOT NULL)",params![key,state,result.to_string(),now(),native_receipt]).map_err(db_error)?;}
            tx.commit().map_err(db_error)?;Ok(apply)
        })
    }
    pub fn pi_command_input(&self, key: &str, request: &str) -> Result<Value, String> {
        self.with_connection(|c| {
            let raw: String = c
                .query_row(
                    "SELECT input_json FROM pi_commands WHERE task_id=?1 AND request_id=?2",
                    params![key, request],
                    |r| r.get(0),
                )
                .map_err(|_| "PI_COMMAND_NOT_FOUND")?;
            serde_json::from_str(&raw).map_err(|_| "PI_COMMAND_INVALID".into())
        })
    }
    pub fn pi_recover_previous_runtime(&self, key: &str, generation: i64) -> Result<(), String> {
        self.with_connection(|c|{
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        tx.execute("INSERT OR IGNORE INTO pi_command_tombstones(task_id,request_id,created_at) SELECT task_id,request_id,?3 FROM pi_commands WHERE task_id=?1 AND generation<?2 AND status='READY'",params![key,generation,now()]).map_err(db_error)?;
        tx.execute("UPDATE pi_commands SET status='NOT_SENT',error_code='PI_OLD_RUNTIME_NEVER_ATTEMPTED',updated_at=?3 WHERE task_id=?1 AND generation<?2 AND status='READY'",params![key,generation,now()]).map_err(db_error)?;
        tx.execute("UPDATE pi_commands SET status='UNKNOWN',error_code='PI_PREVIOUS_RUNTIME_DELIVERY_UNCONFIRMED',updated_at=?3 WHERE task_id=?1 AND generation<?2 AND status IN('SENDING','ACCEPTED')",params![key,generation,now()]).map_err(db_error)?;
        tx.commit().map_err(db_error)
    })
    }
    pub fn pi_note_effective_mode(
        &self,
        key: &str,
        request: &str,
        mode: &str,
    ) -> Result<(), String> {
        if !matches!(mode, "SEND" | "STEER" | "QUEUE") {
            return Err("PI_NATIVE_MODE_INVALID".into());
        }
        self.with_connection(|c|{c.execute("UPDATE pi_commands SET result_json=json_set(COALESCE(result_json,'{}'),'$.nativeMode',?3),updated_at=?4 WHERE task_id=?1 AND request_id=?2 AND status='SENDING'",params![key,request,mode,now()]).map_err(db_error)?;Ok(())})
    }
    pub fn pi_cancel_unattempted_inputs(&self, key: &str) -> Result<(), String> {
        self.with_connection(|c|{
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        tx.execute("INSERT OR IGNORE INTO pi_command_tombstones(task_id,request_id,created_at) SELECT task_id,request_id,?2 FROM pi_commands WHERE task_id=?1 AND status='READY' AND operation!='CANCEL'",params![key,now()]).map_err(db_error)?;
        tx.execute("UPDATE pi_commands SET status='NOT_SENT',error_code='PI_CANCELLED_BEFORE_NATIVE_SUBMISSION',updated_at=?2 WHERE task_id=?1 AND status='READY' AND operation!='CANCEL'",params![key,now()]).map_err(db_error)?;
        tx.commit().map_err(db_error)
    })
    }
    pub fn pi_branch_info(&self, key: &str) -> Result<Value, String> {
        self.with_connection(|c|{c.query_row("SELECT native_leaf_id,branch_revision,(SELECT COUNT(*) FROM pi_public_items WHERE task_id=?1 AND active=1),(SELECT COUNT(*) FROM pi_public_items WHERE task_id=?1 AND active=0) FROM pi_tasks WHERE id=?1",[key],|r|Ok(serde_json::json!({"leafId":r.get::<_,Option<String>>(0)?,"revision":r.get::<_,i64>(1)?,"activeMessageCount":r.get::<_,i64>(2)?,"otherBranchMessageCount":r.get::<_,i64>(3)?,"view":"ACTIVE_NATIVE_BRANCH"}))).map_err(db_error)})
    }
    pub fn pi_checkpoint_native_identity(
        &self,
        key: &str,
        native: &str,
        file: &str,
        observed: Option<&Selection>,
    ) -> Result<(), String> {
        self.with_connection(|c|{
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let old=task(&tx,key)?;
        if old.native_session_id.as_ref().is_some_and(|v|v!=native)||old.native_session_file.as_ref().is_some_and(|v|v!=file){return Err("PI_SESSION_IDENTITY_CHANGED".into());}
        tx.execute("UPDATE pi_tasks SET native_session_id=?2,native_session_file=?3,updated_at=?4 WHERE id=?1",params![key,native,file,now()]).map_err(db_error)?;
        let metadata=serde_json::to_string(&observed).map_err(|_|"PI_SETTINGS_INVALID")?;
        tx.execute("UPDATE assistant_actions SET result_json=json_set(result_json,'$.nativeSessionId',?3,'$.threadId',?4,'$.observedSettings',json(?5),'$.registration','SELECTING'),updated_at=?6 WHERE id=?1 AND status='EXECUTING' AND operation='CREATE_CHAT' AND json_extract(result_json,'$.taskId')=?2",params![old.create_action_id,key,native,format!("pi:{native}"),metadata,now()]).map_err(db_error)?;
        tx.commit().map_err(db_error)
    })
    }
    pub fn pi_acknowledge_unknown(
        &self,
        key: &str,
        request: &str,
        confirmed: bool,
    ) -> Result<(), String> {
        if !confirmed {
            return Err("PI_OWNER_CONFIRMATION_REQUIRED".into());
        }
        self.with_connection(|c|{if c.execute("UPDATE pi_commands SET status='ACKNOWLEDGED',updated_at=?3 WHERE task_id=?1 AND request_id=?2 AND status='UNKNOWN'",params![key,request,now()]).map_err(db_error)?!=1{return Err("PI_UNKNOWN_RECEIPT_REQUIRED".into());}Ok(())})
    }
    pub fn pi_begin_native_command(
        &self,
        key: &str,
        request: &str,
        hash: &str,
        generation: i64,
        mode: &str,
    ) -> Result<bool, String> {
        self.with_connection(|c|{
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let t=task(&tx,key)?;let scope=scope(&tx,&t.scope_id)?;if t.generation!=generation||!scope.enabled||scope.generation!=t.scope_generation{return Err("PI_COMMAND_CONTEXT_CHANGED".into());}
        let old=command(&tx,key,request)?;if old.payload_hash!=hash||old.generation!=generation||!((old.operation=="AUTO"&&matches!(mode,"SEND"|"STEER"))||old.operation==mode){return Err("PI_COMMAND_INTENT_CHANGED".into());}
        let authority:Option<(String,String)>=tx.query_row("SELECT grant_id,json_extract(input_json,'$.threadId') FROM assistant_actions WHERE id=?1 AND operation IN('SEND_CHAT','STOP_CHAT')",[request],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
        if let Some((grant,target))=authority{let g=super::assistant::active(&tx,&grant)?;if g.scope!="INSTANCE"||Some(target.as_str())!=t.native_session_id.as_ref().map(|id|format!("pi:{id}")).as_deref(){return Err("PI_COMMAND_GRANT_CONTEXT_CHANGED".into());}
        let policy:Option<String>=tx.query_row("SELECT policy_json FROM pi_scope_policies WHERE scope_id=?1",[&t.scope_id],|r|r.get(0)).optional().map_err(db_error)?;
        if let Some(raw)=policy{let policy:super::pi_policy::PiWorkspacePolicy=serde_json::from_str(&raw).map_err(|_|"PI_POLICY_INVALID")?;if !policy.grant_ids.contains(&grant){return Err("PI_SCOPE_NOT_DELEGATED_TO_ASSISTANT".into());}}}
        let changed=tx.execute("UPDATE pi_commands SET status='SENDING',result_json=json_object('nativeMode',?4),updated_at=?5 WHERE task_id=?1 AND request_id=?2 AND payload_hash=?3 AND status='READY'",params![key,request,hash,mode,now()]).map_err(db_error)?;
        tx.commit().map_err(db_error)?;Ok(changed==1)
    })
    }
    pub fn pi_abandon_unattempted(&self, key: &str, request: &str) -> Result<bool, String> {
        self.with_connection(|c|{
        if request.is_empty()||request.len()>128||!request.bytes().all(|c|c.is_ascii_alphanumeric()||b"_-".contains(&c)){return Err("PI_COMMAND_INVALID".into());}
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;task(&tx,key)?;
        let status:Option<String>=tx.query_row("SELECT status FROM pi_commands WHERE task_id=?1 AND request_id=?2",params![key,request],|r|r.get(0)).optional().map_err(db_error)?;
        if status.as_deref().is_some_and(|s|!matches!(s,"READY"|"NOT_SENT")){return Ok(false);}
        tx.execute("INSERT OR IGNORE INTO pi_command_tombstones(task_id,request_id,created_at) VALUES(?1,?2,?3)",params![key,request,now()]).map_err(db_error)?;
        tx.execute("UPDATE pi_commands SET status='NOT_SENT',error_code='PI_ABANDONED_BEFORE_NATIVE_SUBMISSION',updated_at=?3 WHERE task_id=?1 AND request_id=?2 AND status='READY'",params![key,request,now()]).map_err(db_error)?;
        tx.commit().map_err(db_error)?;Ok(true)
    })
    }
    pub fn pi_advance_runtime_generation(
        &self,
        key: &str,
        generation: i64,
    ) -> Result<Task, String> {
        self.with_connection(|c|{if c.execute("UPDATE pi_tasks SET generation=generation+1,runtime_epoch=NULL,updated_at=?3 WHERE id=?1 AND generation=?2",params![key,generation,now()]).map_err(db_error)?!=1{return Err("PI_TASK_GENERATION_CHANGED".into());}task(c,key)})
    }
    pub fn pi_record_owner_question(&self,key:&str,epoch:&str,request:&Value)->Result<(),String>{self.with_connection(|c|{
      let t=task(c,key)?;if t.runtime_epoch.as_deref()!=Some(epoch){return Err("PI_RUNTIME_CHANGED_REFRESH".into());}
      let id=request["id"].as_str().filter(|s|!s.is_empty()&&s.len()<=256&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_:".contains(&b))).ok_or("PI_QUESTION_INVALID")?;
      let method=request["method"].as_str().filter(|s|matches!(*s,"confirm"|"select"|"input"|"editor")).ok_or("PI_QUESTION_INVALID")?;
      let title=request["title"].as_str().unwrap_or("");let message=request["message"].as_str().unwrap_or("");if title.len()+message.len()>32768{return Err("PI_QUESTION_INVALID".into());}
      c.execute("INSERT OR IGNORE INTO pi_owner_questions(task_id,generation,runtime_epoch,request_id,method,title,message,observed_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![t.id,t.generation,epoch,id,method,title,message,now()]).map_err(db_error)?;Ok(())
    })}
    pub fn pi_owner_questions(&self,key:&str)->Result<Vec<Value>,String>{self.with_connection(|c|{let t=task(c,key)?;let mut q=c.prepare("SELECT request_id,method,title,message,observed_at,disposition FROM pi_owner_questions WHERE task_id=?1 AND generation=?2 AND runtime_epoch=?3 ORDER BY observed_at DESC LIMIT 20").map_err(db_error)?;let rows=q.query_map(params![t.id,t.generation,t.runtime_epoch],|r|Ok(serde_json::json!({"requestId":r.get::<_,String>(0)?,"method":r.get::<_,String>(1)?,"title":r.get::<_,String>(2)?,"message":r.get::<_,String>(3)?,"observedAt":r.get::<_,i64>(4)?,"disposition":r.get::<_,String>(5)?}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;Ok(rows)})}
    /// Latest assistant text only, with pagination before filtering and a bounded payload.
    pub fn pi_assistant_history_page(&self,key:&str,before:Option<&str>,limit:usize)->Result<Vec<Value>,String>{
        self.with_connection(|c|{
            task(c,key)?;
            let cursor=if let Some(id)=before{c.query_row("SELECT ordinal FROM pi_public_items WHERE task_id=?1 AND id=?2",params![key,id],|r|r.get::<_,i64>(0)).map_err(|_|"PI_HISTORY_CURSOR_INVALID")?}else{i64::MAX};
            let mut q=c.prepare("SELECT content_json,ordinal FROM pi_public_items WHERE task_id=?1 AND active=1 AND ordinal<?2 AND json_extract(content_json,'$.role')='assistant' AND EXISTS (SELECT 1 FROM json_each(content_json,'$.content') b WHERE json_extract(b.value,'$.type')='text' AND trim(json_extract(b.value,'$.text'))!='') ORDER BY ordinal DESC LIMIT ?3").map_err(db_error)?;
            let raw=q.query_map(params![key,cursor,limit.clamp(1,50) as i64],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            let mut rows=Vec::new();let mut remaining=32000usize;
            for (raw,order) in raw{let v:Value=serde_json::from_str(&raw).map_err(|_|"PI_HISTORY_INVALID")?;let text=v["content"].as_array().into_iter().flatten().filter(|b|b["type"]=="text").filter_map(|b|b["text"].as_str()).collect::<Vec<_>>().join("\n\n");if text.trim().is_empty(){continue;}if text.chars().count()>remaining&&!rows.is_empty(){break;}let clipped:String=text.chars().take(remaining).collect();remaining=remaining.saturating_sub(clipped.chars().count());rows.push(serde_json::json!({"id":v["id"],"role":"assistant","text":clipped,"timestamp":v["timestamp"],"order":order,"truncated":clipped.len()<text.len()}));if remaining==0{break;}}
            rows.reverse();Ok(rows)
        })
    }
    pub fn pi_public_history_page(
        &self,
        key: &str,
        before: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Value>, String> {
        self.with_connection(|c|{
        task(c,key)?;let cursor=if let Some(id)=before{c.query_row("SELECT ordinal FROM pi_public_items WHERE task_id=?1 AND id=?2",params![key,id],|r|r.get::<_,i64>(0)).map_err(|_|"PI_HISTORY_CURSOR_INVALID")?}else{i64::MAX};
        let mut statement=c.prepare("SELECT content_json,ordinal FROM pi_public_items WHERE task_id=?1 AND active=1 AND ordinal<?2 ORDER BY ordinal DESC LIMIT ?3").map_err(db_error)?;
        let rows=statement.query_map(params![key,cursor,limit.clamp(1,100)as i64],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
        let mut values=rows.into_iter().map(|(v,ordinal)|{let mut value:Value=serde_json::from_str(&v).map_err(|_|"PI_HISTORY_INVALID")?;value["order"]=ordinal.into();Ok(value)}).collect::<Result<Vec<_>,String>>()?;values.reverse();Ok(values)
    })
    }
    pub fn pi_public_item(&self, key: &str, native: &str) -> Result<Value, String> {
        self.with_connection(|c| {
            let raw: String = c
                .query_row(
                    "SELECT content_json FROM pi_public_items WHERE task_id=?1 AND id=?2",
                    params![key, native],
                    |r| r.get(0),
                )
                .map_err(|_| "PI_PUBLIC_ITEM_NOT_FOUND")?;
            serde_json::from_str(&raw).map_err(|_| "PI_HISTORY_INVALID".into())
        })
    }
    pub fn pi_reply_views(&self, key: &str) -> Result<Vec<Value>, String> {
        self.with_connection(|c|{
        let t=task(c,key)?;let native=t.native_session_id.ok_or("PI_SESSION_ID_UNCONFIRMED")?;let mut statement=c.prepare("SELECT p.id,p.request_id,COALESCE(json_extract(p.result_json,'$.nativeMode'),p.operation),p.input_json,p.status,p.error_code,p.created_at,p.updated_at,(SELECT i.id FROM pi_public_items i WHERE i.task_id=p.task_id AND i.role='user' AND json_extract(i.content_json,'$.clientId')=p.request_id LIMIT 1) FROM pi_commands p WHERE task_id=?1 AND operation IN('AUTO','SEND','STEER','QUEUE') ORDER BY created_at DESC LIMIT 100").map_err(db_error)?;
        let rows=statement.query_map([key],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,i64>(6)?,r.get::<_,i64>(7)?,r.get::<_,Option<String>>(8)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
        rows.into_iter().map(|(_,request,mode,input,status,error,created,updated,message)|{let input:Value=serde_json::from_str(&input).map_err(|_|"PI_COMMAND_INVALID")?;let mapped=match status.as_str(){"READY"|"SENDING"=>"SENDING","ACCEPTED" if mode=="QUEUE"=>"QUEUED","ACCEPTED"=>"SENDING","CONSUMED"=>"SENT","ACKNOWLEDGED"=>"ACKNOWLEDGED","UNKNOWN"=>"UNKNOWN","NOT_SENT"=>"FAILED","CANCELLED"=>"CANCELLED",_=>"UNKNOWN"};Ok(serde_json::json!({"id":request,"threadId":format!("pi:{native}"),"cwd":t.directory,"payloadHash":command(c,key,&request)?.payload_hash,"generation":command(c,key,&request)?.generation,"sourceSequence":null,"expectedTurnId":null,"mode":if mode=="AUTO"{"SEND"}else{&mode},"text":input["text"].as_str().unwrap_or_default(),"status":mapped,"nativeMessageId":message,"turnId":format!("pi-session:{native}"),"errorCode":error,"createdAt":created,"updatedAt":updated,"options":{"attachments":input["attachments"].as_array().cloned().unwrap_or_default()}}))}).collect()
    })
    }
    pub fn pi_revoke_scope(&self, key: &str, generation: i64) -> Result<(), String> {
        self.with_connection(|c|{if c.execute("UPDATE pi_directory_scopes SET enabled=0,generation=generation+1,updated_at=?3 WHERE id=?1 AND generation=?2 AND enabled=1",params![key,generation,now()]).map_err(db_error)?!=1{return Err("PI_SCOPE_CHANGED".into());}Ok(())})
    }
    pub fn pi_cancel_native_queue_receipts(&self, key: &str) -> Result<(), String> {
        self.with_connection(|c|{c.execute("UPDATE pi_commands SET status='CANCELLED',updated_at=?2 WHERE task_id=?1 AND operation IN('QUEUE','STEER') AND status='ACCEPTED'",params![key,now()]).map_err(db_error)?;Ok(())})
    }
    pub fn pi_checkpoint_created_action(
        &self,
        action: &str,
        key: &str,
        result: &Value,
    ) -> Result<(), String> {
        self.with_connection(|c|{
        let current:String=c.query_row("SELECT result_json FROM assistant_actions WHERE id=?1 AND operation='CREATE_CHAT' AND status='EXECUTING' AND json_extract(result_json,'$.provider')='PI' AND json_extract(result_json,'$.taskId')=?2",params![action,key],|r|r.get(0)).map_err(|_|"PI_CREATE_RECEIPT_IDENTITY_CHANGED")?;
        let old:Value=serde_json::from_str(&current).map_err(|_|"PI_CREATE_RECEIPT_INVALID")?;
        if result["provider"]!="PI"||result["taskId"]!=key||result["firstMessageSent"]!=false||old["nativeSessionId"].as_str().is_some_and(|s|result["nativeSessionId"].as_str()!=Some(s)){return Err("PI_CREATE_RECEIPT_IDENTITY_CHANGED".into());}
        c.execute("UPDATE assistant_actions SET result_json=?2,updated_at=?3 WHERE id=?1 AND status='EXECUTING'",params![action,result.to_string(),now()]).map_err(db_error)?;Ok(())
    })
    }
    pub fn pi_create_action_grant(&self, action: &str) -> Result<String, String> {
        self.with_connection(|c|{
        let grant:String=c.query_row("SELECT grant_id FROM assistant_actions WHERE id=?1 AND operation='CREATE_CHAT' AND status='EXECUTING' AND json_extract(input_json,'$.provider')='PI'",[action],|r|r.get(0)).map_err(|_|"PI_REVIEWED_CREATE_REQUIRED")?;
        let active=super::assistant::active(c,&grant)?;if active.scope!="INSTANCE"{return Err("ASSISTANT_INSTANCE_AUTHORITY_REQUIRED".into());}Ok(grant)
    })
    }
    pub fn pi_storage_root(&self) -> Result<PathBuf, String> {
        self.with_connection(|c| {
            let filename: String = c
                .query_row(
                    "SELECT file FROM pragma_database_list WHERE name='main'",
                    [],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            let path = PathBuf::from(filename);
            if !path.is_absolute() {
                return Err("PI_PERSISTENT_STORAGE_REQUIRED".into());
            }
            Ok(path
                .parent()
                .ok_or("PI_PERSISTENT_STORAGE_REQUIRED")?
                .join("pi-executor"))
        })
    }
    pub fn pi_store_public_history(
        &self,
        key: &str,
        generation: i64,
        items: &[Value],
        active: &[String],
        leaf: Option<&str>,
    ) -> Result<(), String> {
        self.with_connection(|c|{
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let t=task(&tx,key)?;if t.generation!=generation{return Err("PI_TASK_GENERATION_CHANGED".into());}
        let old_leaf:Option<String>=tx.query_row("SELECT native_leaf_id FROM pi_tasks WHERE id=?1",[key],|r|r.get(0)).map_err(db_error)?;
        let changed=old_leaf.as_ref().is_some_and(|id|!active.contains(id));
        tx.execute("UPDATE pi_tasks SET native_leaf_id=?2,branch_revision=branch_revision+?3,state=CASE WHEN ?3 THEN 'UNKNOWN' ELSE state END,error_code=CASE WHEN ?3 THEN 'PI_NATIVE_BRANCH_CHANGED' ELSE error_code END WHERE id=?1",params![key,leaf,changed]).map_err(db_error)?;
        tx.execute("UPDATE pi_public_items SET active=0 WHERE task_id=?1",[key]).map_err(db_error)?;
        for (ordinal,item) in items.iter().enumerate(){let native=item["id"].as_str().filter(|s|!s.is_empty()).ok_or("PI_HISTORY_ID_INVALID")?;let role=item["role"].as_str().filter(|s|matches!(*s,"user"|"assistant"|"toolResult")).ok_or("PI_HISTORY_ROLE_INVALID")?;
            let content=item.to_string();let old:Option<String>=tx.query_row("SELECT content_json FROM pi_public_items WHERE task_id=?1 AND id=?2",params![key,native],|r|r.get(0)).optional().map_err(db_error)?;
            if let Some(old)=old.filter(|v|*v!=content){
                let mut before:Value=serde_json::from_str(&old).map_err(|_|"PI_HISTORY_ENTRY_CHANGED")?;let mut after=item.clone();
                let known=|v:Option<&Value>|v.is_none_or(|v|v.is_null()||matches!(v.as_str(),Some("PI_PROVIDER_CONNECTION_ERROR"|"PI_PROVIDER_CONTEXT_LIMIT"|"PI_PROVIDER_RATE_LIMIT"|"PI_PROVIDER_TEMPORARY_UNAVAILABLE"|"PI_PROVIDER_ERROR")));
                if !known(before.get("errorCode"))||!known(after.get("errorCode")){return Err("PI_HISTORY_ENTRY_CHANGED".into());}
                before.as_object_mut().ok_or("PI_HISTORY_ENTRY_CHANGED")?.remove("errorCode");after.as_object_mut().ok_or("PI_HISTORY_ENTRY_CHANGED")?.remove("errorCode");
                if before!=after{return Err("PI_HISTORY_ENTRY_CHANGED".into());}
                tx.execute("UPDATE pi_public_items SET content_json=?3 WHERE task_id=?1 AND id=?2",params![key,native,content]).map_err(db_error)?;
            }
            tx.execute("INSERT OR IGNORE INTO pi_public_items(task_id,id,native_parent_id,ordinal,role,kind,content_json,at) VALUES(?1,?2,?3,?4,?5,'MESSAGE',?6,?7)",params![key,native,item["parentId"].as_str(),ordinal as i64,role,content,now()]).map_err(db_error)?;
        }
        for id in active{tx.execute("UPDATE pi_public_items SET active=1 WHERE task_id=?1 AND id=?2",params![key,id]).map_err(db_error)?;}
        tx.commit().map_err(db_error)
    })
    }
    pub fn pi_public_history(
        &self,
        key: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Value>, String> {
        self.with_connection(|c|{
        task(c,key)?;let cursor=if let Some(id)=after{c.query_row("SELECT ordinal FROM pi_public_items WHERE task_id=?1 AND id=?2",params![key,id],|r|r.get::<_,i64>(0)).map_err(|_|"PI_HISTORY_CURSOR_INVALID")?}else{-1};
        let mut statement=c.prepare("SELECT content_json FROM pi_public_items WHERE task_id=?1 AND active=1 AND ordinal>?2 ORDER BY ordinal LIMIT ?3").map_err(db_error)?;let rows=statement.query_map(params![key,cursor,limit.clamp(1,100)as i64],|r|r.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;rows.into_iter().map(|v|serde_json::from_str(&v).map_err(|_|"PI_HISTORY_INVALID".into())).collect()
    })
    }
    pub fn pi_requested_selection(&self, key: &str) -> Result<Option<Selection>, String> {
        self.with_connection(|c| {
            let value: Option<String> = c
                .query_row(
                    "SELECT desired_json FROM pi_tasks WHERE id=?1",
                    [key],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            value
                .map(|v| serde_json::from_str(&v))
                .transpose()
                .map_err(|_| "PI_SETTINGS_INVALID".into())
        })
    }
    /// Claim exactly once before touching native stdin. A duplicate returns its
    /// existing receipt; callers must not submit again, including SENDING/UNKNOWN.
    pub fn pi_claim_command(
        &self,
        key: &str,
        request: &str,
        generation: i64,
        operation: &str,
        input: &Value,
    ) -> Result<(CommandReceipt, bool), String> {
        if request.is_empty()
            || request.len() > 128
            || !request
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
            || !matches!(operation, "AUTO" | "SEND" | "STEER" | "QUEUE" | "CANCEL")
        {
            return Err("PI_COMMAND_INVALID".into());
        }
        let input_json = serde_json::to_string(input).map_err(|_| "PI_COMMAND_INVALID")?;
        if input_json.len() > 60 * 1024 * 1024 {
            return Err("PI_COMMAND_TOO_LARGE".into());
        }
        let hash = {
            use sha2::{Digest, Sha256};
            format!("{:x}", Sha256::digest(input_json.as_bytes()))
        };
        self.with_connection(|c|{let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   let cancelled:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pi_command_tombstones WHERE task_id=?1 AND request_id=?2)",params![key,request],|r|r.get(0)).map_err(db_error)?;if cancelled{return Err("PI_COMMAND_ABANDONED".into());}
   let existing:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pi_commands WHERE task_id=?1 AND request_id=?2)",params![key,request],|r|r.get(0)).map_err(db_error)?;
   if existing{let receipt=command(&tx,key,request)?;if receipt.generation!=generation||receipt.operation!=operation||receipt.payload_hash!=hash{return Err("PI_COMMAND_INTENT_CHANGED".into());}return Ok((receipt,false));}
   let t=task(&tx,key)?;let scope=scope(&tx,&t.scope_id)?;if t.generation!=generation{return Err("PI_TASK_GENERATION_CHANGED".into());}if !scope.enabled||scope.generation!=t.scope_generation{return Err("PI_SCOPE_CHANGED".into());}
   let uncertain:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pi_commands WHERE task_id=?1 AND status IN('READY','SENDING','UNKNOWN'))",[key],|r|r.get(0)).map_err(db_error)?;if uncertain&&operation!="CANCEL"{return Err("PI_PREVIOUS_COMMAND_UNCONFIRMED".into());}
   if input["images"].as_array().is_some_and(|v|!v.is_empty()){let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pi_commands WHERE task_id=?1 AND status='ACCEPTED' AND json_array_length(input_json,'$.images')>0)",[key],|r|r.get(0)).map_err(db_error)?;if pending{return Err("PI_IMAGE_INPUT_STILL_PENDING".into());}}
   if operation=="QUEUE"{let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pi_commands WHERE task_id=?1 AND operation='QUEUE' AND status='ACCEPTED')",[key],|r|r.get(0)).map_err(db_error)?;if pending{return Err("PI_QUEUE_PENDING_CHECK_FIRST".into());}}
   let at=now();tx.execute("INSERT INTO pi_commands(id,task_id,request_id,generation,operation,payload_hash,input_json,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,'READY',?8,?8)",params![id(),key,request,generation,operation,hash,input_json,at]).map_err(db_error)?;let receipt=command(&tx,key,request)?;tx.commit().map_err(db_error)?;Ok((receipt,true))
  })
    }
    pub fn pi_command_receipt(&self, key: &str, request: &str) -> Result<CommandReceipt, String> {
        self.with_connection(|c| command(c, key, request))
    }
    pub fn pi_finish_command(
        &self,
        key: &str,
        request: &str,
        hash: &str,
        status: &str,
        native_entry: Option<&str>,
        error: Option<&str>,
    ) -> Result<CommandReceipt, String> {
        if !matches!(
            status,
            "ACCEPTED" | "CONSUMED" | "UNKNOWN" | "NOT_SENT" | "CANCELLED" | "APPLIED"
        ) {
            return Err("PI_COMMAND_STATUS_INVALID".into());
        }
        self.with_connection(|c|{let old=command(c,key,request)?;if old.payload_hash!=hash{return Err("PI_COMMAND_INTENT_CHANGED".into());}if status=="APPLIED"&&old.operation!="CANCEL"{return Err("PI_COMMAND_STATUS_INVALID".into());}if old.status=="READY"&&matches!(status,"ACCEPTED"|"CONSUMED"|"UNKNOWN"){return Err("PI_NATIVE_ATTEMPT_NOT_CLAIMED".into());}if matches!(old.status.as_str(),"CONSUMED"|"NOT_SENT"|"CANCELLED")&&old.status!=status{return Err("PI_COMMAND_ALREADY_TERMINAL".into());}
   if matches!(status,"ACCEPTED"|"CONSUMED")&&native_entry.is_none(){return Err("PI_NATIVE_RECEIPT_REQUIRED".into());}
   c.execute("UPDATE pi_commands SET status=?3,native_entry_id=COALESCE(?4,native_entry_id),error_code=?5,updated_at=?6 WHERE task_id=?1 AND request_id=?2",params![key,request,status,native_entry,error,now()]).map_err(db_error)?;command(c,key,request)
  })
    }
    /// Owner-UI operation only. The application layer validates the canonical local
    /// root before this call; no MCP tool is allowed to call this method.
    pub fn pi_authorize_directory(
        &self,
        root: &str,
        writable: bool,
        terminal: bool,
    ) -> Result<DirectoryScope, String> {
        self.with_connection(|c|{
  let existing:Option<String>=c.query_row("SELECT id FROM pi_directory_scopes WHERE root=?1",[root],|r|r.get(0)).optional().map_err(db_error)?;
  if let Some(key)=&existing{let old=scope(c,key)?;if old.enabled&&old.writable==writable&&old.terminal_confirmation==terminal{return Ok(old);}}
  let key=existing.unwrap_or_else(id);let at=now();c.execute("INSERT INTO pi_directory_scopes(id,root,writable,terminal_confirmation,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5) ON CONFLICT(root) DO UPDATE SET writable=excluded.writable,terminal_confirmation=excluded.terminal_confirmation,enabled=1,generation=generation+1,updated_at=excluded.updated_at",params![key,root,writable,terminal,at]).map_err(db_error)?;scope(c,&key)
 })
    }
    pub fn pi_scopes(&self) -> Result<Vec<DirectoryScope>, String> {
        self.with_connection(|c| {
            let mut q = c
                .prepare("SELECT id FROM pi_directory_scopes ORDER BY created_at")
                .map_err(db_error)?;
            let keys = q
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?;
            keys.into_iter().map(|key| scope(c, &key)).collect()
        })
    }
    pub fn pi_scope(&self, key: &str) -> Result<DirectoryScope, String> {
        self.with_connection(|c| scope(c, key))
    }
    pub fn pi_task(&self, key: &str) -> Result<Task, String> {
        self.with_connection(|c| {
            if let Some(native) = key.strip_prefix("pi:") {
                let id: String = c
                    .query_row(
                        "SELECT id FROM pi_tasks WHERE native_session_id=?1",
                        [native],
                        |r| r.get(0),
                    )
                    .map_err(|_| "PI_TASK_NOT_FOUND")?;
                task(c, &id)
            } else {
                task(c, key)
            }
        })
    }
    pub fn pi_notice_tasks(&self)->Result<Vec<Task>,String>{self.with_connection(|c|{let mut q=c.prepare("SELECT id FROM pi_tasks WHERE state IN ('RUNNING','WAITING_DECISION') OR id IN (SELECT id FROM pi_tasks ORDER BY updated_at DESC LIMIT 200) ORDER BY updated_at DESC").map_err(db_error)?;let keys=q.query_map([],|r|r.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;keys.into_iter().map(|key|task(c,&key)).collect()})}
    pub fn pi_tasks(&self) -> Result<Vec<Task>, String> {
        self.with_connection(|c| {
            let mut q = c
                .prepare("SELECT id FROM pi_tasks ORDER BY created_at DESC LIMIT 200")
                .map_err(db_error)?;
            let keys = q
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?;
            keys.into_iter().map(|key| task(c, &key)).collect()
        })
    }
    pub fn pi_begin_create(
        &self,
        action_id: &str,
        grant: &str,
        scope_id: &str,
        generation: i64,
        directory: &str,
        label: &str,
        parent: Parent,
        requested: Option<&Selection>,
    ) -> Result<Task, String> {
        self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
  let desired=requested.map(serde_json::to_string).transpose().map_err(|_|"PI_SETTINGS_INVALID")?;
  if let Some(key)=tx.query_row("SELECT id FROM pi_tasks WHERE create_action_id=?1",[action_id],|r|r.get::<_,String>(0)).optional().map_err(db_error)?{let old=task(&tx,&key)?;let saved:Option<String>=tx.query_row("SELECT desired_json FROM pi_tasks WHERE id=?1",[&key],|r|r.get(0)).map_err(db_error)?;if saved!=desired||old.creator_grant_id!=grant||old.scope_id!=scope_id||old.scope_generation!=generation||old.directory!=directory||old.parent.provider!=parent.provider||old.parent.external_id!=parent.external_id||old.parent.cwd!=parent.cwd{return Err("PI_CREATE_INTENT_CHANGED".into());}return Ok(old);}
  let s=scope(&tx,scope_id)?;if !s.enabled||s.generation!=generation{return Err("PI_SCOPE_CHANGED".into());}
  let normalized=|v:&str|{let slash=v.replace('\\',"/");let mut path=PathBuf::new();for part in std::path::Path::new(&slash).components(){match part{std::path::Component::ParentDir=>{path.pop();},std::path::Component::CurDir=>{},other=>path.push(other.as_os_str())}}path.to_string_lossy().replace('\\',"/").trim_end_matches('/').to_ascii_lowercase()};
  let policy:Option<String>=tx.query_row("SELECT policy_json FROM pi_scope_policies WHERE scope_id=?1",[scope_id],|r|r.get(0)).optional().map_err(db_error)?;
  let roots=if let Some(raw)=policy{let policy:super::pi_policy::PiWorkspacePolicy=serde_json::from_str(&raw).map_err(|_|"PI_POLICY_INVALID")?;if !policy.grant_ids.iter().any(|id|id==grant){return Err("PI_SCOPE_NOT_DELEGATED_TO_ASSISTANT".into());}policy.roots}else{vec![s.root.clone()]};
  let dir=normalized(directory);if !roots.iter().any(|root|{let root=normalized(root);dir==root||dir.starts_with(&(root+"/"))}){return Err("PI_DIRECTORY_OUT_OF_SCOPE".into());}
  if !matches!(parent.provider.as_str(),"CODEX"|"CLAUDE_CODE")||parent.external_id.is_empty(){return Err("PI_PARENT_INVALID".into());}
  let key=id();let at=now();tx.execute("INSERT INTO pi_tasks(id,create_action_id,creator_grant_id,scope_id,scope_generation,directory,label,parent_provider,parent_external_id,parent_cwd,created_at,updated_at,desired_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?12)",params![key,action_id,grant,scope_id,generation,directory,label,parent.provider,parent.external_id,parent.cwd,at,desired]).map_err(db_error)?;let row=task(&tx,&key)?;tx.commit().map_err(db_error)?;Ok(row)
 })
    }
    pub fn pi_checkpoint_session(
        &self,
        key: &str,
        session_id: &str,
        file: &str,
        verified: bool,
        selection: &Selection,
        source: &str,
        epoch: &str,
    ) -> Result<Task, String> {
        self.with_connection(|c|{
  let old=task(c,key)?;if old.native_session_id.as_ref().is_some_and(|v|v!=session_id)||old.native_session_file.as_ref().is_some_and(|v|v!=file){return Err("PI_SESSION_IDENTITY_CHANGED".into());}
  c.execute("UPDATE pi_tasks SET native_session_id=?2,native_session_file=?3,file_verified=?4,selected_json=?5,selection_source=?6,runtime_epoch=?7,state=CASE WHEN state='CREATING' THEN 'IDLE' ELSE state END,updated_at=?8 WHERE id=?1",params![old.id,session_id,file,verified,serde_json::to_string(selection).map_err(|_|"PI_SETTINGS_INVALID")?,source,epoch,now()]).map_err(db_error)?;task(c,key)
 })
    }
    pub fn pi_set_state(
        &self,
        key: &str,
        state: &str,
        result: Option<&Value>,
        error: Option<&str>,
    ) -> Result<Task, String> {
        if !matches!(
            state,
            "CREATING"
                | "IDLE"
                | "RUNNING"
                | "WAITING_DECISION"
                | "RESULT_READY"
                | "PARTIAL"
                | "FAILED"
                | "CANCELLED"
                | "UNKNOWN"
                | "INTERRUPTED"
                | "EMPTY_LOST"
        ) {
            return Err("PI_STATE_INVALID".into());
        }
        self.with_connection(|c|{let old=task(c,key)?;c.execute("UPDATE pi_tasks SET state=?2,result_json=?3,error_code=?4,updated_at=?5 WHERE id=?1",params![old.id,state,result.map(Value::to_string),error,now()]).map_err(db_error)?;task(c,key)})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_scope_is_stable_and_create_intent_cannot_rebind_parent_or_native_session() {
        let d = tempfile::tempdir().unwrap();
        let s = RouterStore::open_at(d.path().join("router.db")).unwrap();
        let root = d.path().to_string_lossy().into_owned();
        let scope = s.pi_authorize_directory(&root, true, false).unwrap();
        assert_eq!(
            s.pi_authorize_directory(&root, true, false)
                .unwrap()
                .generation,
            scope.generation
        );
        let parent = Parent {
            provider: "CODEX".into(),
            external_id: "native-parent".into(),
            cwd: root.clone(),
        };
        let t = s
            .pi_begin_create(
                "create-id",
                "grant",
                &scope.id,
                scope.generation,
                &root,
                "child",
                parent.clone(),
                None,
            )
            .unwrap();
        assert_eq!(
            s.pi_begin_create(
                "create-id",
                "grant",
                &scope.id,
                scope.generation,
                &root,
                "child",
                parent.clone(),
                None
            )
            .unwrap()
            .id,
            t.id
        );
        let mut wrong = parent;
        wrong.external_id = "other-parent".into();
        assert!(s
            .pi_begin_create(
                "create-id",
                "grant",
                &scope.id,
                scope.generation,
                &root,
                "child",
                wrong,
                None
            )
            .is_err());
        let selection = Selection {
            provider: "openai".into(),
            model: "native-model".into(),
            thinking: "low".into(),
        };
        s.pi_checkpoint_session(
            &t.id,
            "native-pi",
            "owned.jsonl",
            false,
            &selection,
            "DEFAULT",
            "epoch",
        )
        .unwrap();
        assert!(s
            .pi_checkpoint_session(
                &t.id,
                "replacement",
                "owned.jsonl",
                false,
                &selection,
                "DEFAULT",
                "epoch"
            )
            .is_err());
        assert_eq!(s.pi_task("pi:native-pi").unwrap().id, t.id);
        assert!(!s.pi_task("pi:native-pi").unwrap().file_verified);
        assert!(s.pi_set_state(&t.id, "FAKE_SUCCESS", None, None).is_err());
    }
    #[test]
    fn command_claims_survive_restart_and_never_replay_uncertain_delivery() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("router.db");
        let s = RouterStore::open_at(&db).unwrap();
        let root = d.path().to_string_lossy().into_owned();
        let scope = s.pi_authorize_directory(&root, false, false).unwrap();
        let parent = Parent {
            provider: "CLAUDE_CODE".into(),
            external_id: "native-manager".into(),
            cwd: root.clone(),
        };
        let selected = Selection {
            provider: "openai".into(),
            model: "m".into(),
            thinking: "low".into(),
        };
        let task = s
            .pi_begin_create(
                "create-command-test",
                "grant",
                &scope.id,
                scope.generation,
                &root,
                "child",
                parent.clone(),
                Some(&selected),
            )
            .unwrap();
        assert!(s
            .pi_begin_create(
                "create-command-test",
                "grant",
                &scope.id,
                scope.generation,
                &root,
                "child",
                parent,
                None
            )
            .is_err());
        let input = serde_json::json!({"text":"public task","attachments":[]});
        let (receipt, claimed) = s
            .pi_claim_command(&task.id, "send-1", task.generation, "SEND", &input)
            .unwrap();
        assert!(claimed);
        assert_eq!(receipt.status, "READY");
        assert!(
            !s.pi_claim_command(&task.id, "send-1", task.generation, "SEND", &input)
                .unwrap()
                .1
        );
        assert!(s
            .pi_claim_command(
                &task.id,
                "send-1",
                task.generation,
                "SEND",
                &serde_json::json!({"text":"changed"})
            )
            .is_err());
        assert!(s
            .pi_claim_command(&task.id, "send-2", task.generation, "SEND", &input)
            .is_err());
        assert!(s
            .pi_begin_native_command(
                &task.id,
                "send-1",
                &receipt.payload_hash,
                task.generation,
                "SEND"
            )
            .unwrap());
        assert!(!s.pi_abandon_unattempted(&task.id, "send-1").unwrap());
        s.pi_finish_command(
            &task.id,
            "send-1",
            &receipt.payload_hash,
            "UNKNOWN",
            None,
            Some("PI_COMMAND_RESPONSE_UNCONFIRMED"),
        )
        .unwrap();
        drop(s);
        let reopened = RouterStore::open_at(&db).unwrap();
        assert_eq!(
            reopened
                .pi_command_receipt(&task.id, "send-1")
                .unwrap()
                .status,
            "UNKNOWN"
        );
        assert!(
            !reopened
                .pi_claim_command(&task.id, "send-1", task.generation, "SEND", &input)
                .unwrap()
                .1
        );
        assert!(reopened
            .pi_claim_command(&task.id, "send-2", task.generation, "SEND", &input)
            .is_err());
        assert!(reopened
            .pi_finish_command(
                &task.id,
                "send-1",
                &receipt.payload_hash,
                "ACCEPTED",
                None,
                None
            )
            .is_err());
        reopened
            .pi_finish_command(
                &task.id,
                "send-1",
                &receipt.payload_hash,
                "CONSUMED",
                Some("native-input-entry"),
                None,
            )
            .unwrap();
        assert!(reopened
            .pi_finish_command(
                &task.id,
                "send-1",
                &receipt.payload_hash,
                "NOT_SENT",
                None,
                None
            )
            .is_err());
        assert!(reopened
            .pi_claim_command(&task.id, "send-2", task.generation + 1, "SEND", &input)
            .is_err());
    }
    fn fixture() -> (tempfile::TempDir, RouterStore, Task) {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime");
        std::fs::create_dir_all(&base).unwrap();
        let d = tempfile::tempdir_in(base).unwrap();
        let store = RouterStore::open_at(d.path().join("router.db")).unwrap();
        let root = d.path().to_string_lossy().into_owned();
        let scope = store.pi_authorize_directory(&root, false, false).unwrap();
        let t = store
            .pi_begin_create(
                "fixture-create",
                "fixture-grant",
                &scope.id,
                scope.generation,
                &root,
                "fixture",
                Parent {
                    provider: "CODEX".into(),
                    external_id: "fixture-manager".into(),
                    cwd: root.clone(),
                },
                None,
            )
            .unwrap();
        (d, store, t)
    }
    #[test]
    fn derived_error_classification_upgrade_preserves_native_history_immutability(){
        let(_d,s,t)=fixture();let old=serde_json::json!({"id":"native-one","parentId":null,"role":"assistant","content":[{"type":"text","text":"immutable original"}],"stopReason":"stop"});
        s.pi_store_public_history(&t.id,t.generation,&[old.clone()],&["native-one".into()],Some("native-one")).unwrap();let mut upgraded=old.clone();upgraded["errorCode"]=Value::Null;s.pi_store_public_history(&t.id,t.generation,&[upgraded.clone()],&["native-one".into()],Some("native-one")).unwrap();assert_eq!(s.pi_public_item(&t.id,"native-one").unwrap(),upgraded);
        let mut changed=upgraded.clone();changed["content"][0]["text"]="tampered original".into();assert_eq!(s.pi_store_public_history(&t.id,t.generation,&[changed],&["native-one".into()],Some("native-one")).unwrap_err(),"PI_HISTORY_ENTRY_CHANGED");let mut unknown=upgraded.clone();unknown["errorCode"]="ARBITRARY_DERIVED_TEXT".into();assert_eq!(s.pi_store_public_history(&t.id,t.generation,&[unknown],&["native-one".into()],Some("native-one")).unwrap_err(),"PI_HISTORY_ENTRY_CHANGED");assert_eq!(s.pi_public_item(&t.id,"native-one").unwrap(),upgraded);
        let mut failed=upgraded.clone();failed["stopReason"]="error".into();assert!(s.pi_store_public_history(&t.id,t.generation,&[failed],&["native-one".into()],Some("native-one")).is_err());
    }
    #[test]
    fn compact_reader_starts_at_latest_assistant_text_without_tool_payloads(){
        let(_d,s,t)=fixture();s.with_connection(|c|{for i in 0..80{let role=if i%2==0{"assistant"}else{"toolResult"};let content=serde_json::json!({"id":format!("m{i}"),"role":role,"content":[{"type":"text","text":if role=="assistant"{format!("answer-{i}")}else{"PRIVATE_TOOL_OUTPUT".repeat(5000)}}]});c.execute("INSERT INTO pi_public_items(task_id,id,ordinal,role,kind,content_json,at,active) VALUES(?1,?2,?3,?4,'message',?5,0,1)",params![t.id,format!("m{i}"),i,role,content.to_string()]).map_err(db_error)?;}Ok(())}).unwrap();
        let rows=s.pi_assistant_history_page(&t.id,None,3).unwrap();assert_eq!(rows.len(),3);assert_eq!(rows[2]["text"],"answer-78");assert!(!serde_json::to_string(&rows).unwrap().contains("PRIVATE_TOOL_OUTPUT"));let earlier=s.pi_assistant_history_page(&t.id,rows[0]["id"].as_str(),3).unwrap();assert_eq!(earlier[2]["text"],"answer-72");
    }
    #[test]
    fn abandonment_and_stop_fence_late_local_mailbox_writes() {
        let (_d, s, t) = fixture();
        let input = serde_json::json!({"text":"late body","images":[]});
        assert!(s.pi_abandon_unattempted(&t.id, "not-yet-claimed").unwrap());
        assert!(s
            .pi_claim_command(&t.id, "not-yet-claimed", t.generation, "AUTO", &input)
            .is_err());
        let (first, _) = s
            .pi_claim_command(&t.id, "late-sender", t.generation, "AUTO", &input)
            .unwrap();
        let (cancel, _) = s
            .pi_claim_command(
                &t.id,
                "stop",
                t.generation,
                "CANCEL",
                &serde_json::json!({"operation":"CANCEL"}),
            )
            .unwrap();
        assert!(s
            .pi_begin_native_command(&t.id, "stop", &cancel.payload_hash, t.generation, "CANCEL")
            .unwrap());
        s.pi_cancel_unattempted_inputs(&t.id).unwrap();
        assert!(!s
            .pi_begin_native_command(
                &t.id,
                "late-sender",
                &first.payload_hash,
                t.generation,
                "SEND"
            )
            .unwrap());
        assert_eq!(
            s.pi_command_receipt(&t.id, "late-sender").unwrap().status,
            "NOT_SENT"
        );
        assert!(s
            .pi_claim_command(&t.id, "late-sender", t.generation, "AUTO", &input)
            .is_err());
    }
    #[test]
    fn active_native_branch_is_not_flattened_with_abandoned_messages() {
        let (_d, s, t) = fixture();
        let message = |id: &str, parent: Option<&str>, role: &str| serde_json::json!({"id":id,"parentId":parent,"role":role,"content":[{"type":"text","text":id}],"timestamp":"2026-10-30T00:00:00Z"});
        let mut entries = vec![
            message("u", None, "user"),
            message("old", Some("u"), "assistant"),
            message("new", Some("u"), "assistant"),
        ];
        s.pi_store_public_history(
            &t.id,
            t.generation,
            &entries,
            &["u".into(), "old".into()],
            Some("old"),
        )
        .unwrap();
        assert_eq!(
            s.pi_public_history_page(&t.id, None, 50)
                .unwrap()
                .iter()
                .map(|v| v["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["u", "old"]
        );
        s.pi_store_public_history(
            &t.id,
            t.generation,
            &entries,
            &["u".into(), "new".into()],
            Some("new"),
        )
        .unwrap();
        assert_eq!(s.pi_branch_info(&t.id).unwrap()["revision"], 2);
        assert_eq!(
            s.pi_public_history_page(&t.id, None, 50)
                .unwrap()
                .iter()
                .map(|v| v["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["u", "new"]
        );
        assert_eq!(s.pi_public_item(&t.id, "old").unwrap()["id"], "old");
        entries.push(message("more", Some("new"), "assistant"));
        s.pi_store_public_history(
            &t.id,
            t.generation,
            &entries,
            &["u".into(), "new".into(), "more".into()],
            Some("more"),
        )
        .unwrap();
        assert_eq!(s.pi_branch_info(&t.id).unwrap()["revision"], 2);
    }
    #[test]
    fn an_older_settled_event_cannot_complete_a_newer_input() {
        let (_d, s, t) = fixture();
        let input = serde_json::json!({"text":"A","images":[]});
        let (a, _) = s
            .pi_claim_command(&t.id, "input-a", t.generation, "AUTO", &input)
            .unwrap();
        s.pi_begin_native_command(&t.id, "input-a", &a.payload_hash, t.generation, "SEND")
            .unwrap();
        s.pi_finish_command(
            &t.id,
            "input-a",
            &a.payload_hash,
            "CONSUMED",
            Some("native-a"),
            None,
        )
        .unwrap();
        let (b, _) = s
            .pi_claim_command(
                &t.id,
                "input-b",
                t.generation,
                "AUTO",
                &serde_json::json!({"text":"B","images":[]}),
            )
            .unwrap();
        s.pi_begin_native_command(&t.id, "input-b", &b.payload_hash, t.generation, "SEND")
            .unwrap();
        let ra = serde_json::json!({"lastAssistant":{"id":"answer-a"},"nativeTerminalReceiptId":"settled-a"});
        let rb = serde_json::json!({"lastAssistant":{"id":"answer-b"},"nativeTerminalReceiptId":"settled-b"});
        assert!(!s
            .pi_record_terminal(
                &t.id,
                "settled-a",
                &[serde_json::json!({"requestId":"input-a","payloadHash":a.payload_hash})],
                "RESULT_READY",
                &ra,
                true
            )
            .unwrap());
        assert!(s.pi_task(&t.id).unwrap().result.is_none());
        s.pi_finish_command(
            &t.id,
            "input-b",
            &b.payload_hash,
            "CONSUMED",
            Some("native-b"),
            None,
        )
        .unwrap();
        assert!(s
            .pi_record_terminal(
                &t.id,
                "settled-b",
                &[serde_json::json!({"requestId":"input-b","payloadHash":b.payload_hash})],
                "RESULT_READY",
                &rb,
                true
            )
            .unwrap());
        assert!(!s
            .pi_record_terminal(
                &t.id,
                "settled-a",
                &[serde_json::json!({"requestId":"input-a","payloadHash":a.payload_hash})],
                "RESULT_READY",
                &ra,
                true
            )
            .unwrap());
        assert_eq!(
            s.pi_task(&t.id).unwrap().result.unwrap()["lastAssistant"]["id"],
            "answer-b"
        );
    }
}
