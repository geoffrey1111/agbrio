//! Provider-neutral durable projection for an explicitly watched native session.
//! Display message IDs are adapter observation IDs, never native routing IDs.
use super::*;
use serde::Deserialize;
use serde_json::json;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedConversation {
    pub thread_id: String,
    pub provider: String,
    pub native_id: String,
    pub cwd: String,
    pub epoch: String,
    pub configuration: Value,
}
pub(super) fn migrate(c: &mut Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS antigravity_owner_settings(singleton INTEGER PRIMARY KEY CHECK(singleton=1),permission_mode TEXT NOT NULL CHECK(permission_mode IN ('ASK','FULL_ACCESS')),updated_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS managed_conversations(thread_id TEXT PRIMARY KEY,provider TEXT NOT NULL,native_id TEXT NOT NULL,cwd TEXT NOT NULL,epoch TEXT NOT NULL,configuration TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS managed_public_messages(ordinal INTEGER PRIMARY KEY AUTOINCREMENT,thread_id TEXT NOT NULL,id TEXT NOT NULL,role TEXT NOT NULL,text TEXT NOT NULL,at INTEGER NOT NULL,UNIQUE(thread_id,id));
 CREATE INDEX IF NOT EXISTS managed_public_thread ON managed_public_messages(thread_id,ordinal);
 CREATE TABLE IF NOT EXISTS managed_assistant_origins(request_id TEXT PRIMARY KEY,grant_id TEXT NOT NULL,action_hash TEXT NOT NULL);
 ").map_err(db_error)
}
impl RouterStore {
    pub fn managed_next_queued(
        &self,
        key: &str,
    ) -> Result<Option<watch_reply::WatchReply>, String> {
        let id=self.with_connection(|c|c.query_row("SELECT id FROM watch_replies WHERE thread_id=?1 AND status='QUEUED' ORDER BY rowid LIMIT 1",[key],|r|r.get::<_,String>(0)).optional().map_err(db_error))?;
        id.map(|id| {
            self.watch_reply(&id)
                .and_then(|r| r.ok_or("REPLY_NOT_FOUND".into()))
        })
        .transpose()
    }
    pub fn prepare_managed_owner_reply(
        &self,
        input: &watch_reply::WatchReply,
    ) -> Result<watch_reply::WatchReply, String> {
        self.with_connection(|c| {
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let r = watch_reply::prepare_watch_reply_queue_at(&tx, input, true)?;
            tx.commit().map_err(db_error)?;
            Ok(r)
        })
    }
    pub fn prepare_managed_assistant_reply(
        &self,
        input: &watch_reply::WatchReply,
    ) -> Result<watch_reply::WatchReply, String> {
        self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
  let(gid,hash,raw):(String,String,String)=tx.query_row("SELECT grant_id,payload_hash,input_json FROM assistant_actions WHERE id=?1 AND operation='SEND_CHAT' AND status='EXECUTING' AND basis IS NOT NULL",[&input.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_|"MANAGED_ASSISTANT_REVIEW_REQUIRED")?;
  let approved:Value=serde_json::from_str(&raw).map_err(|_|"MANAGED_ASSISTANT_REVIEW_REQUIRED")?;
  if assistant::active(&tx,&gid)?.scope!="INSTANCE"||approved["threadId"]!=input.thread_id||approved["generation"]!=input.generation||approved["text"]!=input.text||approved["queueRequested"].as_bool().unwrap_or(false)!=(input.mode=="QUEUE"){return Err("MANAGED_ASSISTANT_PAYLOAD_CHANGED".into());}
  let mode=approved["mode"].as_str().unwrap_or("AUTO");if input.mode!=if mode=="AUTO"{"SEND"}else{mode}{return Err("MANAGED_ASSISTANT_PAYLOAD_CHANGED".into());}
  for field in ["model","effort"]{if approved["options"].get(field).unwrap_or(&Value::Null)!=input.options.get(field).unwrap_or(&Value::Null){return Err("MANAGED_ASSISTANT_PAYLOAD_CHANGED".into());}}
  if approved["options"]["attachments"].as_array().cloned().unwrap_or_default()!=input.options["attachments"].as_array().cloned().unwrap_or_default(){return Err("MANAGED_ASSISTANT_PAYLOAD_CHANGED".into());}
  let r=watch_reply::prepare_watch_reply_queue_at(&tx,input,true)?;tx.execute("INSERT INTO managed_assistant_origins(request_id,grant_id,action_hash) VALUES(?1,?2,?3)",params![input.id,gid,hash]).map_err(db_error)?;tx.commit().map_err(db_error)?;Ok(r)
 })
    }
    pub fn authorize_managed_reply(&self, id: &str, key: &str) -> Result<(), String> {
        self.with_connection(|c|{
  let origin:Option<(String,String)>=c.query_row("SELECT grant_id,action_hash FROM managed_assistant_origins WHERE request_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
  if let Some((gid,hash))=origin{if assistant::active(c,&gid)?.scope!="INSTANCE"||!c.query_row("SELECT EXISTS(SELECT 1 FROM assistant_actions WHERE id=?1 AND grant_id=?2 AND payload_hash=?3 AND status IN ('EXECUTING','APPLIED') AND basis IS NOT NULL AND json_extract(input_json,'$.threadId')=?4)",params![id,gid,hash,key],|r|r.get::<_,bool>(0)).map_err(db_error)?{return Err("MANAGED_ASSISTANT_AUTHORIZATION_CHANGED".into());}}Ok(())
 })
    }
    pub fn register_managed_conversation(&self, v: &ManagedConversation) -> Result<(), String> {
        if v.thread_id.is_empty()
            || v.thread_id.len() > 512
            || v.native_id.is_empty()
            || v.native_id.len() > 256
            || v.epoch.is_empty()
        {
            return Err("MANAGED_CONVERSATION_IDENTITY_INVALID".into());
        }
        let w = self.codex_chat_context(&v.thread_id)?;
        if w.cwd != v.cwd {
            return Err("MANAGED_CONVERSATION_ROOT_CHANGED".into());
        }
        let raw = serde_json::to_string(&v.configuration)
            .map_err(|_| "MANAGED_CONVERSATION_CONFIGURATION_INVALID")?;
        if raw.len() > 65536 {
            return Err("MANAGED_CONVERSATION_CONFIGURATION_INVALID".into());
        }
        self.with_connection(|c|{
   let old:Option<(String,String,String)>=c.query_row("SELECT provider,native_id,cwd FROM managed_conversations WHERE thread_id=?1",[&v.thread_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db_error)?;
   if old.is_some_and(|(p,n,d)|p!=v.provider||n!=v.native_id||d!=v.cwd){return Err("MANAGED_CONVERSATION_IDENTITY_CHANGED".into());}
   c.execute("INSERT INTO managed_conversations(thread_id,provider,native_id,cwd,epoch,configuration) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(thread_id) DO UPDATE SET epoch=excluded.epoch,configuration=excluded.configuration",params![v.thread_id,v.provider,v.native_id,v.cwd,v.epoch,raw]).map_err(db_error)?;Ok(())
  })
    }
    pub fn managed_conversation(&self, key: &str) -> Result<ManagedConversation, String> {
        self.with_connection(|c|{
  c.query_row("SELECT thread_id,provider,native_id,cwd,epoch,configuration FROM managed_conversations WHERE thread_id=?1",[key],|r|{let raw:String=r.get(5)?;Ok(ManagedConversation{thread_id:r.get(0)?,provider:r.get(1)?,native_id:r.get(2)?,cwd:r.get(3)?,epoch:r.get(4)?,configuration:serde_json::from_str(&raw).map_err(|e|rusqlite::Error::FromSqlConversionFailure(5,rusqlite::types::Type::Text,Box::new(e)))?})}).map_err(|_|"MANAGED_CONVERSATION_UNAVAILABLE".into())
 })
    }
    pub fn managed_append_text(
        &self,
        key: &str,
        epoch: &str,
        id: &str,
        role: &str,
        text: &str,
    ) -> Result<(), String> {
        if !matches!(role, "user" | "assistant")
            || id.is_empty()
            || id.len() > 256
            || text.len() > 100000
        {
            return Err("MANAGED_MESSAGE_INVALID".into());
        }
        self.with_connection(|c|{
   let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM managed_conversations m JOIN codex_watches w ON w.thread_id=m.thread_id WHERE m.thread_id=?1 AND m.epoch=?2 AND w.enabled=1 AND w.cwd=m.cwd)",params![key,epoch],|r|r.get(0)).map_err(db_error)?;
   if !valid{return Err("MANAGED_CONVERSATION_EPOCH_CHANGED".into());}
   let old:Option<(String,usize)>=tx.query_row("SELECT role,length(CAST(text AS BLOB)) FROM managed_public_messages WHERE thread_id=?1 AND id=?2",params![key,id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
   if old.as_ref().is_some_and(|(r,n)|r!=role||*n+text.len()>500000){return Err("MANAGED_MESSAGE_LIMIT".into());}
   tx.execute("INSERT INTO managed_public_messages(thread_id,id,role,text,at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(thread_id,id) DO UPDATE SET text=managed_public_messages.text||excluded.text",params![key,id,role,text,now()]).map_err(db_error)?;
   tx.commit().map_err(db_error)?;Ok(())
  })
    }
    pub fn replace_managed_history(
        &self,
        key: &str,
        epoch: &str,
        rows: &[(String, String)],
    ) -> Result<(), String> {
        if rows.len() > 10000
            || rows
                .iter()
                .any(|(r, t)| !matches!(r.as_str(), "user" | "assistant") || t.len() > 500000)
            || rows.iter().map(|(_, t)| t.len()).sum::<usize>() > 8 * 1024 * 1024
        {
            return Err("MANAGED_HISTORY_LIMIT".into());
        }
        self.with_connection(|c|{let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM managed_conversations m JOIN codex_watches w ON w.thread_id=m.thread_id WHERE m.thread_id=?1 AND m.epoch=?2 AND w.enabled=1 AND w.cwd=m.cwd)",params![key,epoch],|r|r.get(0)).map_err(db_error)?;
   if !valid{return Err("MANAGED_CONVERSATION_EPOCH_CHANGED".into());}
   tx.execute("DELETE FROM managed_public_messages WHERE thread_id=?1",[key]).map_err(db_error)?;
   for (i,(role,text)) in rows.iter().enumerate(){tx.execute("INSERT INTO managed_public_messages(thread_id,id,role,text,at) VALUES(?1,?2,?3,?4,?5)",params![key,format!("acp-history:{i}"),role,text,now()]).map_err(db_error)?;}
   tx.commit().map_err(db_error)?;Ok(())})
    }
    pub fn managed_public_history(&self, key: &str, before: Option<i64>) -> Result<Value, String> {
        self.with_connection(|c|{
  let mut stmt=c.prepare("SELECT ordinal,id,role,text,at FROM managed_public_messages WHERE thread_id=?1 AND (?2 IS NULL OR ordinal<?2) ORDER BY ordinal DESC LIMIT 51").map_err(db_error)?;
  let mut rows=stmt.query_map(params![key,before],|r|Ok(json!({"order":r.get::<_,i64>(0)?,"id":r.get::<_,String>(1)?,"turnId":format!("managed:{}",r.get::<_,String>(1)?),"role":r.get::<_,String>(2)?,"text":r.get::<_,String>(3)?,"timestamp":r.get::<_,i64>(4)?,"identityKind":"ADAPTER_OBSERVATION","kind":"PUBLIC"}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
  let mut bytes=0;let keep=rows.iter().take(50).take_while(|v|{let n=v["text"].as_str().unwrap_or_default().len();if bytes+n>1_000_000&&bytes>0{return false;}bytes+=n;true}).count();let more=rows.len()>keep;rows.truncate(keep);
  // Exact adapter request identity joins the admitted receipt to its public echo,
  // including while the native prompt is still running. Text is never identity.
  for row in &mut rows { if let Some(id)=row["id"].as_str().and_then(|id|id.strip_prefix("acp-user:")) { let id=id.to_owned();row["clientId"]=json!(id); } }
let next=if more{rows.last().and_then(|r|r["order"].as_i64())}else{None};rows.reverse();
  Ok(json!({"messages":rows,"nextCursor":next.map(|n|format!("managed:{n}")),"coverage":"NATIVE_ACP_OBSERVATIONS","order":"OLDEST_FIRST"}))
 })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observations_keep_order_after_reopen_and_old_epoch_cannot_append() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("router.sqlite");
        let s = RouterStore::open_at(&path).unwrap();
        let root = d.path().to_string_lossy().into_owned();
        s.enable_codex_watch(
            "native:fixture",
            "Fixture",
            &root,
            &super::super::codex_watch::WatchSnapshot {
                state: "IDLE".into(),
                turn_id: None,
                item_id: None,
                text: String::new(),
            },
        )
        .unwrap();
        let mut meta = ManagedConversation {
            thread_id: "native:fixture".into(),
            native_id: "fixture".into(),
            provider: "FIXTURE".into(),
            cwd: root,
            epoch: "first".into(),
            configuration: json!({}),
        };
        s.register_managed_conversation(&meta).unwrap();
        s.managed_append_text(&meta.thread_id, "first", "user-1", "user", "one")
            .unwrap();
        s.managed_append_text(&meta.thread_id, "first", "assistant-1", "assistant", "two")
            .unwrap();
        s.managed_append_text(
            &meta.thread_id,
            "first",
            "assistant-1",
            "assistant",
            "\nthree",
        )
        .unwrap();
        drop(s);
        let s = RouterStore::open_at(&path).unwrap();
        let h = s.managed_public_history(&meta.thread_id, None).unwrap();
        assert_eq!(h["messages"][0]["text"], "one");
        assert_eq!(h["messages"][1]["text"], "two\nthree");
        meta.epoch = "second".into();
        s.register_managed_conversation(&meta).unwrap();
        assert!(s
            .managed_append_text(&meta.thread_id, "first", "late", "assistant", "late")
            .is_err());
    }
}

impl RouterStore {
 pub fn antigravity_permission_mode(&self)->Result<String,String>{self.with_connection(|c|{let mode=c.query_row("SELECT permission_mode FROM antigravity_owner_settings WHERE singleton=1",[],|r|r.get::<_,String>(0)).optional().map_err(db_error)?.unwrap_or_else(||"ASK".into());if !matches!(mode.as_str(),"ASK"|"FULL_ACCESS"){return Err("ANTIGRAVITY_PERMISSION_SETTING_INVALID".into())}Ok(mode)})}
 /// Called only by the current-user owner control surface, never MCP operations.
 pub fn set_antigravity_permission_mode(&self,mode:&str)->Result<(),String>{if !matches!(mode,"ASK"|"FULL_ACCESS"){return Err("ANTIGRAVITY_PERMISSION_SETTING_INVALID".into())}self.with_connection(|c|{c.execute("INSERT INTO antigravity_owner_settings(singleton,permission_mode,updated_at) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET permission_mode=excluded.permission_mode,updated_at=excluded.updated_at",params![mode,now()]).map_err(db_error)?;Ok(())})}
}
