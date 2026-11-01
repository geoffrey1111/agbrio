//! Owner presentation projections. No provider calls or new task/approval state.
use super::*;
use serde_json::json;

impl RouterStore {
    /// Read metadata only under the store lock, against one exact binding.
    /// No full source/handoff body or transcript is loaded by directory polling.
    pub fn todo_ready_handoffs(
        &self,
        workstream: &str,
        revision: i64,
    ) -> Result<Vec<Value>, String> {
        self.with_connection(|c|{
            let w=workstream_by_id(c,workstream)?;
            if w.trashed_at.is_some()||w.archived_at.is_some(){return Err("BRIDGE_UNAVAILABLE".into())}
            let b=role_bridge::bridge(c,workstream)?;
            if b.binding_revision!=revision{return Err("BRIDGE_BINDING_CHANGED_REFRESH".into())}
            let mut out=vec![];
            let(Some(decision),Some(execution))=(&b.decision,&b.execution)else{return Ok(out)};
            if !b.explicit_roles{return Ok(out)}
            for(src,dst)in [(decision,execution),(execution,decision)]{
                let mut q=c.prepare("SELECT h.id,o.id,h.status FROM handoffs h JOIN role_handoff_details d ON d.handoff_id=h.id JOIN reply_observations o ON o.workstream_id=h.workstream_id AND o.endpoint_id=h.source_endpoint_id AND o.assistant_identity=h.source_response_identity WHERE h.workstream_id=?1 AND d.binding_revision=?2 AND d.source_role=?3 AND h.source_endpoint_id=?4 AND h.destination_endpoint_id=?5 AND h.status IN ('READY','APPROVED') AND NOT EXISTS(SELECT 1 FROM handoffs other JOIN role_handoff_details od ON od.handoff_id=other.id WHERE other.workstream_id=h.workstream_id AND other.source_endpoint_id=h.source_endpoint_id AND other.source_response_identity=h.source_response_identity AND other.destination_endpoint_id=h.destination_endpoint_id AND od.binding_revision=?2 AND other.status IN ('SENDING','SENT','UNKNOWN')) ORDER BY h.created_at,h.id").map_err(db_error)?;
                let rows=q.query_map(params![workstream,revision,src.role,src.endpoint.id,dst.endpoint.id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"sourceObservationId":r.get::<_,String>(1)?,"sourceRole":src.role,"status":r.get::<_,String>(2)?,"bindingRevision":revision}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;out.extend(rows);
                let mut q=c.prepare("SELECT o.id FROM reply_observations o WHERE o.workstream_id=?1 AND o.endpoint_id=?2 AND o.assistant_identity IS NOT NULL AND length(trim(o.text))>0 AND o.handled_at IS NULL AND NOT EXISTS(SELECT 1 FROM handoffs h WHERE h.workstream_id=o.workstream_id AND h.source_endpoint_id=o.endpoint_id AND h.source_response_identity=o.assistant_identity) ORDER BY o.observed_at,o.id").map_err(db_error)?;
                let ids=q.query_map(params![workstream,src.endpoint.id],|r|r.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
                out.extend(ids.into_iter().map(|id|json!({"id":format!("source:{revision}:{id}"),"sourceObservationId":id,"sourceRole":src.role,"status":"UNPREPARED","bindingRevision":revision})));
            }
            Ok(out)
        })
    }
    pub fn todo_claude_requests(
        &self,
        root: &str,
        generation: i64,
        sid: &str,
    ) -> Result<Vec<Value>, String> {
        self.with_connection(|c|{
            let w=codex_watch::chat_context(c,root)?;
            if !w.enabled||w.generation!=generation{return Ok(vec![])}
            let mut q=c.prepare("SELECT r.id,r.kind,r.payload_json,r.created_at,r.tool_use_id,r.session_id FROM claude_owner_requests r JOIN claude_sessions s ON s.session_id=r.session_id AND s.instance_id=r.instance_id AND s.cwd=r.cwd WHERE r.root=?1 AND r.generation=?2 AND r.session_id=?3 AND r.status='PENDING' AND r.created_at>?4-900000 AND s.lease_until>?4 AND s.state!='SESSION_END' ORDER BY r.created_at,r.id").map_err(db_error)?;
            let rows=q.query_map(params![root,generation,sid,now()],|r|Ok(json!({"id":r.get::<_,String>(0)?,"kind":r.get::<_,String>(1)?,"payload":serde_json::from_str::<Value>(&r.get::<_,String>(2)?).unwrap_or(Value::Null),"since":r.get::<_,i64>(3)?,"nativeRequestId":r.get::<_,String>(4)?,"nativeSessionId":r.get::<_,String>(5)?}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            Ok(rows)
        })
    }
    /// A successful explicit close only; an unloaded or failed runtime is not closed.
    pub fn todo_pi_closed_at(&self, task: &str, generation: i64) -> Result<Option<i64>, String> {
        self.with_connection(|c|c.query_row("SELECT MAX(updated_at) FROM pi_commands WHERE task_id=?1 AND generation=?2 AND operation='CANCEL' AND status='APPLIED' AND json_extract(input_json,'$.operation')='CLOSE_PI_TASK'",params![task,generation],|r|r.get(0)).map_err(db_error))
    }
    pub fn todo_unread_result(&self, thread: &str) -> Result<bool, String> {
        self.with_connection(|c| unread(c, thread))
    }
    /// Snapshot a per-thread cutoff inside one transaction. A pruned or foreign
    /// caller cursor cannot accidentally acknowledge a different conversation.
    pub fn todo_mark_thread_read(
        &self,
        thread: &str,
        cutoff: Option<i64>,
    ) -> Result<Value, String> {
        if thread.is_empty() || thread.len() > 256 || cutoff.is_some_and(|v| v < 0) {
            return Err("WATCH_THREAD_CURSOR_INVALID".into());
        }
        self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            // Only an existing watched/provider-backed conversation is admissible.
            let known:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM codex_watches WHERE thread_id=?1) OR EXISTS(SELECT 1 FROM pi_tasks WHERE 'pi:'||native_session_id=?1)",[thread],|r|r.get(0)).map_err(db_error)?;
            if !known{return Err("WATCH_CONVERSATION_NOT_REGISTERED".into())}
            let captured:Option<i64>=tx.query_row("SELECT MAX(sequence) FROM codex_watch_events WHERE thread_id=?1 AND (?2 IS NULL OR sequence<=?2)",params![thread,cutoff],|r|r.get(0)).map_err(db_error)?;
            if let Some(sequence)=captured{tx.execute("INSERT OR IGNORE INTO watch_removed_items(kind,id,removed_at) SELECT 'EVENT',CAST(sequence AS TEXT),?3 FROM codex_watch_events WHERE thread_id=?1 AND sequence<=?2",params![thread,sequence,now()]).map_err(db_error)?;}
            let pending=unread(&tx,thread)?;
            tx.commit().map_err(db_error)?;
            Ok(json!({"threadId":thread,"upToSequence":captured,"unreadResult":pending}))
        })
    }
}
fn unread(c: &Connection, thread: &str) -> Result<bool, String> {
    c.query_row("SELECT EXISTS(SELECT 1 FROM codex_watch_events e WHERE e.thread_id=?1 AND json_extract(e.snapshot_json,'$.state')='RESULT_READY' AND NOT EXISTS(SELECT 1 FROM watch_removed_items r WHERE r.kind='EVENT' AND r.id=CAST(e.sequence AS TEXT)))",[thread],|r|r.get(0)).map_err(db_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, RouterStore) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/t-items-fixtures");
        fs::create_dir_all(&root).unwrap();
        let d = tempfile::tempdir_in(root).unwrap();
        let s = RouterStore::open_at(d.path().join("router.db")).unwrap();
        (d, s)
    }
    fn snap(state: &str, turn: &str) -> codex_watch::WatchSnapshot {
        codex_watch::WatchSnapshot {
            state: state.into(),
            turn_id: Some(turn.into()),
            item_id: Some(turn.into()),
            text: "public".into(),
        }
    }
    #[test]
    fn thread_read_preserves_newer_other_thread_and_native_requests() {
        let (d, s) = fixture();
        let cwd = d.path().to_str().unwrap();
        for id in ["a", "b"] {
            s.enable_codex_watch(id, id, cwd, &snap("IDLE", "0"))
                .unwrap();
            s.record_codex_watch(id, 1, &snap("RESULT_READY", "1"))
                .unwrap();
        }
        let old = s
            .codex_watch_feed(0)
            .unwrap()
            .events
            .into_iter()
            .find(|e| e.thread_id == "a")
            .unwrap()
            .sequence;
        s.record_codex_watch("a", 1, &snap("RUNNING", "2")).unwrap();
        s.record_codex_watch("a", 1, &snap("RESULT_READY", "2"))
            .unwrap();
        assert_eq!(
            s.todo_mark_thread_read("a", Some(old)).unwrap()["unreadResult"],
            true
        );
        assert!(s.todo_unread_result("b").unwrap());
        let done = s.todo_mark_thread_read("a", None).unwrap();
        assert_eq!(done["unreadResult"], false);
        assert!(s.todo_unread_result("b").unwrap());
        assert!(s.registered_codex_watch("a").unwrap().enabled);
        assert!(s.todo_mark_thread_read("unknown", None).is_err());
        assert!(s.todo_mark_thread_read("a", Some(-1)).is_err());
    }
    #[test]
    fn failed_or_old_cohort_is_not_closed() {
        let (_d, s) = fixture();
        s.with_connection(|c|{c.execute_batch("PRAGMA foreign_keys=OFF; INSERT INTO pi_commands(id,task_id,request_id,generation,operation,payload_hash,input_json,status,created_at,updated_at) VALUES('c','t','r',1,'CANCEL','h','{\"operation\":\"CLOSE_PI_TASK\"}','UNKNOWN',1,20);").map_err(db_error)?;Ok(())}).unwrap();
        assert_eq!(s.todo_pi_closed_at("t", 1).unwrap(), None);
        s.with_connection(|c| {
            c.execute("UPDATE pi_commands SET status='APPLIED'", [])
                .map_err(db_error)?;
            Ok(())
        })
        .unwrap();
        assert_eq!(s.todo_pi_closed_at("t", 1).unwrap(), Some(20));
        assert_eq!(s.todo_pi_closed_at("t", 2).unwrap(), None);
    }
    #[test]
    fn t2_only_exact_unanswered_leased_current_generation_requests() {
        use super::super::claude_owner_requests::NativeRequest;
        use crate::claude_code::Event;
        let (d, s) = fixture();
        let pair = s.create_claude_connection().unwrap();
        let sid = "10000000-0000-4000-8000-000000000001";
        let instance = "20000000-0000-4000-8000-000000000001";
        let cwd = d.path().to_string_lossy().to_string();
        s.ingest_claude_event(
            &pair.token,
            &Event {
                event_id: Uuid::new_v4().to_string(),
                session_id: sid.into(),
                instance_id: instance.into(),
                sequence: 1,
                cwd: cwd.clone(),
                version: "2.1.304".into(),
                kind: "SESSION_START".into(),
                turn_id: None,
                text: None,
                completed_at: None,
            },
        )
        .unwrap();
        let root = format!("claude_code:{sid}");
        s.enable_codex_watch(&root, "fixture", &cwd, &snap("IDLE", "0"))
            .unwrap();
        let r = NativeRequest {
            id: Uuid::new_v4().to_string(),
            session_id: sid.into(),
            instance_id: instance.into(),
            cwd,
            tool_use_id: "native-question".into(),
            kind: "QUESTION".into(),
            payload: json!({"questions":[{"question":"原文","options":[{"label":"A"}]}]}),
        };
        s.capture_claude_owner_request(&pair.token, &root, 1, &r)
            .unwrap();
        assert_eq!(s.todo_claude_requests(&root, 1, sid).unwrap().len(), 1);
        assert!(s.todo_claude_requests(&root, 2, sid).unwrap().is_empty());
        assert!(s
            .todo_claude_requests(&root, 1, "different")
            .unwrap()
            .is_empty());
        for status in [
            "READY", "OFFERED", "UNKNOWN", "EXPIRED", "APPLIED", "NATIVE",
        ] {
            s.with_connection(|c| {
                c.execute("UPDATE claude_owner_requests SET status=?1", [status])
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
            assert!(
                s.todo_claude_requests(&root, 1, sid).unwrap().is_empty(),
                "{status}"
            );
        }
        s.with_connection(|c| {
            c.execute(
                "UPDATE claude_owner_requests SET status='PENDING',created_at=0",
                [],
            )
            .map_err(db_error)?;
            Ok(())
        })
        .unwrap();
        assert!(s.todo_claude_requests(&root, 1, sid).unwrap().is_empty());
    }
}
