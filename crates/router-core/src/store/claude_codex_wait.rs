//! Metadata-only completion notices for explicitly selected, already watched Codex turns.
//! No provider read, body, send, watch creation or permission mutation occurs here.
use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WaitProbe {
    pub session_id: String,
    pub instance_id: String,
    pub cwd: String,
    pub thread_id: String,
    pub generation: i64,
    pub turn_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_code::Event;
    #[test]
    fn wait_probe_is_metadata_only_and_fenced_by_publisher_and_exact_source() {
        let d = tempfile::tempdir().unwrap();
        let s = RouterStore::open_at(d.path().join("router.db")).unwrap();
        let pair = s.create_claude_connection().unwrap();
        let sid = id();
        let instance = id();
        let tid = id();
        let turn = id();
        let e = Event {
            event_id: id(),
            session_id: sid.clone(),
            instance_id: instance.clone(),
            sequence: 1,
            cwd: d.path().to_str().unwrap().into(),
            version: "2.1.308".into(),
            kind: "SESSION_START".into(),
            turn_id: None,
            text: None,
            completed_at: None,
        };
        s.ingest_claude_event(&pair.token, &e).unwrap();
        let snapshot = codex_watch::WatchSnapshot {
            state: "RUNNING".into(),
            turn_id: Some(turn.clone()),
            item_id: None,
            text: "PRIVATE BODY MUST NEVER LEAVE".into(),
        };
        s.enable_codex_watch(&tid, "private label", &e.cwd, &snapshot)
            .unwrap();
        let mut p = WaitProbe {
            session_id: sid,
            instance_id: instance,
            cwd: e.cwd.clone(),
            thread_id: tid.clone(),
            generation: 1,
            turn_id: turn.clone(),
        };
        let v = s.claude_codex_wait_probe(&pair.token, &p).unwrap();
        assert_eq!(v["state"], "RUNNING");
        assert_eq!(v.as_object().unwrap().len(), 4);
        assert!(!v.to_string().contains("PRIVATE"));
        assert!(s.claude_codex_wait_probe("invalid", &p).is_err());
        p.instance_id = id();
        assert!(s.claude_codex_wait_probe(&pair.token, &p).is_err());
        p.instance_id = e.instance_id.clone();
        p.cwd = d.path().join("wrong").to_string_lossy().into();
        assert!(s.claude_codex_wait_probe(&pair.token, &p).is_err());
        p.cwd = e.cwd.clone();
        p.turn_id = id();
        assert!(s.claude_codex_wait_probe(&pair.token, &p).is_err());
        p.turn_id = turn;
        p.generation = 2;
        assert!(s.claude_codex_wait_probe(&pair.token, &p).is_err());
        p.generation = 1;
        s.record_codex_watch(
            &tid,
            1,
            &codex_watch::WatchSnapshot {
                state: "RESULT_READY".into(),
                ..snapshot
            },
        )
        .unwrap();
        assert_eq!(
            s.claude_codex_wait_probe(&pair.token, &p).unwrap()["state"],
            "RESULT_READY"
        );
        s.record_codex_watch(&tid, 1, &codex_watch::WatchSnapshot{state:"RUNNING".into(),turn_id:Some(id()),item_id:None,text:String::new()}).unwrap();
        assert_eq!(s.claude_codex_wait_probe(&pair.token,&p).unwrap()["state"],"RESULT_READY");
        s.set_watch_item_removed("WATCH", &tid, true).unwrap();
        assert!(s.claude_codex_wait_probe(&pair.token, &p).is_err());
        s.revoke_claude_connection(&pair.connection_id).unwrap();
        assert!(s.claude_codex_wait_probe(&pair.token, &p).is_err());
    }
}

impl RouterStore {
    pub fn claude_codex_wait_probe(&self, token: &str, input: &WaitProbe) -> Result<Value, String> {
        if !Uuid::parse_str(&input.thread_id).is_ok_and(|v| v.to_string() == input.thread_id)
            || input.turn_id.is_empty()
            || input.turn_id.len() > 128
            || input.generation < 1
        {
            return Err("CLAUDE_WAIT_TARGET_INVALID".into());
        }
        self.with_connection(|c| {
            super::claude_manual::verify_publisher(c,token,&input.session_id,&input.instance_id,&input.cwd)?;
            let root:String=c.query_row("SELECT cwd FROM claude_sessions WHERE session_id=?1",[&input.session_id],|r|r.get(0)).map_err(db_error)?;
            if !crate::claude_code::same_root(&root,&input.cwd) {return Err("CLAUDE_ROOT_CHANGED".into());}
            let row:Option<(bool,i64,String,Option<String>)>=c.query_row("SELECT enabled,generation,snapshot_json,error_code FROM codex_watches WHERE thread_id=?1 AND NOT EXISTS(SELECT 1 FROM watch_removed_items WHERE kind='WATCH' AND id=?1)",[&input.thread_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(db_error)?;
            let Some((true,generation,raw,error))=row else {return Err("CLAUDE_WAIT_WATCH_UNAVAILABLE".into());};
            if generation!=input.generation {return Err("CLAUDE_WAIT_GENERATION_CHANGED".into());}
            if error.is_some(){return Err("CLAUDE_WAIT_SOURCE_UNAVAILABLE".into());}
            let mut snapshot:codex_watch::WatchSnapshot=serde_json::from_str(&raw).map_err(|_|"CLAUDE_WAIT_SOURCE_INVALID")?;
            // A later turn must never impersonate completion of the selected turn.
            // A completed selected turn can still be found in the existing bounded
            // event journal if the source started another turn during disconnection.
            if snapshot.turn_id.as_deref()!=Some(input.turn_id.as_str()){
                let retained:Option<String>=c.query_row("SELECT snapshot_json FROM codex_watch_events WHERE thread_id=?1 AND json_extract(snapshot_json,'$.turnId')=?2 AND json_extract(snapshot_json,'$.state') IN ('RESULT_READY','ACTION_REQUIRED','FAILED','INTERRUPTED') AND NOT EXISTS(SELECT 1 FROM watch_removed_items WHERE kind='EVENT' AND id=CAST(codex_watch_events.sequence AS TEXT)) ORDER BY sequence DESC LIMIT 1",params![input.thread_id,input.turn_id],|r|r.get(0)).optional().map_err(db_error)?;
                snapshot=serde_json::from_str(&retained.ok_or("CLAUDE_WAIT_TURN_CHANGED")?).map_err(|_|"CLAUDE_WAIT_SOURCE_INVALID")?;
            }
            Ok(serde_json::json!({"threadId":input.thread_id,"generation":generation,"turnId":input.turn_id,"state":snapshot.state}))
        })
    }
}
