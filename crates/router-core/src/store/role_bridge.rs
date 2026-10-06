//! Decision/Execution are roles, not provider names. Native identity stays on Endpoint.
use super::*;
use serde::Deserialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleBindingInput {
    pub provider: String,
    pub external_id: String,
    pub label: String,
    pub cwd: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleEndpoint {
    pub role: String,
    pub endpoint: Endpoint,
    pub cwd: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleBridge {
    pub workstream_id: String,
    pub binding_revision: i64,
    pub decision: Option<RoleEndpoint>,
    pub execution: Option<RoleEndpoint>,
    pub explicit_roles: bool,
}
pub(super) fn ensure_legacy_binding(c: &Connection, workstream: &str) -> Result<(), String> {
    let present: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('endpoints') WHERE name='bridge_role')",
            [],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    if present {
        let configured:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM endpoints WHERE workstream_id=?1 AND status='ACTIVE' AND bridge_role!='LEGACY')",params![workstream],|r|r.get(0)).map_err(db_error)?;
        if configured {
            return Err("BRIDGE_USE_ROLE_BINDING".into());
        }
    }
    Ok(())
}

pub(super) fn migrate(connection: &mut Connection, path: &Path) -> Result<(), String> {
    let applied: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM router_feature_migrations WHERE key='bridge-roles-v1')",
            [],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    if applied {
        return Ok(());
    }
    // A backup is SQLite-native and consistent, not a filesystem copy of a live DB.
    let backup = path.with_extension(format!("before-bridge-roles-{}.db", id()));
    connection
        .execute("VACUUM INTO ?1", params![backup.to_string_lossy().as_ref()])
        .map_err(db_error)?;
    let saved = Connection::open(&backup).map_err(db_error)?;
    if saved
        .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
        .map_err(db_error)?
        != "ok"
    {
        return Err("BRIDGE_MIGRATION_BACKUP_INVALID".into());
    }
    drop(saved);
    let ddl: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='handoffs'",
            [],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    let old = "'CHATGPT_TO_CODEX', 'CODEX_TO_CHATGPT'";
    if !ddl.contains(old) {
        return Err("BRIDGE_MIGRATION_SCHEMA_UNEXPECTED".into());
    }
    let objects = {
        let mut statement = connection.prepare("SELECT sql FROM sqlite_master WHERE tbl_name='handoffs' AND type IN ('index','trigger') AND sql IS NOT NULL").map_err(db_error)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        rows
    };
    connection
        .execute_batch("PRAGMA foreign_keys=OFF;")
        .map_err(db_error)?;
    let result = (|| {
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let replacement = ddl
            .replacen(
                "CREATE TABLE handoffs",
                "CREATE TABLE handoffs_role_upgrade",
                1,
            )
            .replace(
                old,
                "'CHATGPT_TO_CODEX', 'CODEX_TO_CHATGPT', 'CODEX_TO_CODEX'",
            );
        tx.execute_batch(&replacement).map_err(db_error)?;
        let count: i64 = tx
            .query_row("SELECT count(*) FROM handoffs", [], |r| r.get(0))
            .map_err(db_error)?;
        tx.execute_batch("INSERT INTO handoffs_role_upgrade SELECT * FROM handoffs; DROP TABLE handoffs; ALTER TABLE handoffs_role_upgrade RENAME TO handoffs;").map_err(db_error)?;
        for sql in objects {
            tx.execute_batch(&sql).map_err(db_error)?;
        }
        tx.execute_batch("ALTER TABLE endpoints ADD COLUMN bridge_role TEXT NOT NULL DEFAULT 'LEGACY' CHECK(bridge_role IN ('LEGACY','DECISION','EXECUTION'));
            DROP INDEX endpoints_one_active_provider_per_workstream;
            CREATE UNIQUE INDEX endpoints_one_active_role_per_workstream ON endpoints(workstream_id, CASE WHEN bridge_role='LEGACY' THEN CASE provider WHEN 'CHATGPT' THEN 'DECISION' ELSE 'EXECUTION' END ELSE bridge_role END) WHERE status='ACTIVE';
            CREATE TABLE endpoint_role_details(endpoint_id TEXT PRIMARY KEY REFERENCES endpoints(id), cwd TEXT);
            CREATE TABLE role_handoff_details(handoff_id TEXT PRIMARY KEY REFERENCES handoffs(id), binding_revision INTEGER NOT NULL, source_role TEXT NOT NULL CHECK(source_role IN ('DECISION','EXECUTION')));").map_err(db_error)?;
        if tx
            .query_row("SELECT count(*) FROM handoffs", [], |r| r.get::<_, i64>(0))
            .map_err(db_error)?
            != count
        {
            return Err("BRIDGE_MIGRATION_ROW_COUNT_CHANGED".into());
        }
        let violation = tx
            .prepare("PRAGMA foreign_key_check")
            .map_err(db_error)?
            .exists([])
            .map_err(db_error)?;
        if violation {
            return Err("BRIDGE_MIGRATION_FOREIGN_KEY_INVALID".into());
        }
        tx.execute(
            "INSERT INTO router_feature_migrations(key,applied_at) VALUES('bridge-roles-v1',?1)",
            params![now()],
        )
        .map_err(db_error)?;
        tx.commit().map_err(db_error)
    })();
    let restore = connection
        .execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(db_error);
    restore?;
    result
}

fn role_endpoint(
    c: &Connection,
    workstream: &str,
    role: &str,
) -> Result<Option<RoleEndpoint>, String> {
    if !matches!(role, "DECISION" | "EXECUTION") {
        return Err("BRIDGE_ROLE_INVALID".into());
    }
    let key: Option<(String, Option<String>)> = c.query_row(
        "SELECT e.id,d.cwd FROM endpoints e LEFT JOIN endpoint_role_details d ON d.endpoint_id=e.id WHERE e.workstream_id=?1 AND e.status='ACTIVE' AND (CASE WHEN e.bridge_role='LEGACY' THEN CASE e.provider WHEN 'CHATGPT' THEN 'DECISION' ELSE 'EXECUTION' END ELSE e.bridge_role END)=?2", params![workstream,role], |r| Ok((r.get(0)?,r.get(1)?))
    ).optional().map_err(db_error)?;
    key.map(|(id, cwd)| {
        Ok(RoleEndpoint {
            role: role.into(),
            endpoint: endpoint_by_id(c, &id)?,
            cwd,
        })
    })
    .transpose()
}
pub(super) fn projection_endpoint(
    c: &Connection,
    w: &str,
    provider: &str,
) -> Result<Option<Endpoint>, String> {
    let count:i64=c.query_row("SELECT count(*) FROM endpoints WHERE workstream_id=?1 AND provider=?2 AND status='ACTIVE'",params![w,provider],|r|r.get(0)).map_err(db_error)?;
    if count < 2 {
        return active_endpoint(c, w, provider);
    }
    // Presentation can show the execution thread; dispatch must use exact role.
    Ok(role_endpoint(c, w, "EXECUTION")?
        .filter(|s| s.endpoint.provider == provider)
        .map(|s| s.endpoint))
}
pub(super) fn complete_roles(c: &Connection, w: &str) -> Result<bool, String> {
    let present: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('endpoints') WHERE name='bridge_role')",
            [],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    if !present {
        return Ok(false);
    }
    let b = bridge(c, w)?;
    Ok(b.explicit_roles && b.decision.is_some() && b.execution.is_some())
}
fn bridge(c: &Connection, workstream: &str) -> Result<RoleBridge, String> {
    let w = workstream_by_id(c, workstream)?;
    Ok(RoleBridge { workstream_id: workstream.into(), binding_revision: w.binding_revision,
        decision: role_endpoint(c,workstream,"DECISION")?, execution: role_endpoint(c,workstream,"EXECUTION")?,
        explicit_roles: c.query_row("SELECT EXISTS(SELECT 1 FROM endpoints WHERE workstream_id=?1 AND status='ACTIVE' AND bridge_role!='LEGACY')",params![workstream],|r|r.get(0)).map_err(db_error)? })
}
fn validate_handoff(c: &Connection, handoff: &HandoffHistoryItem) -> Result<(), String> {
    let details: (i64, String) = c
        .query_row(
            "SELECT binding_revision,source_role FROM role_handoff_details WHERE handoff_id=?1",
            params![handoff.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(db_error)?;
    let b = bridge(c, &handoff.workstream_id)?;
    let (src, dst) = if details.1 == "DECISION" {
        (b.decision, b.execution)
    } else {
        (b.execution, b.decision)
    };
    if b.binding_revision != details.0
        || src.as_ref().map(|e| e.endpoint.id.as_str())
            != Some(handoff.endpoint_source()?.id.as_str())
        || dst.as_ref().map(|e| e.endpoint.id.as_str())
            != Some(handoff.destination_endpoint.id.as_str())
    {
        return Err("BRIDGE_BINDING_CHANGED".into());
    }
    Ok(())
}
impl RouterStore {
    /// Immutable role/revision provenance for history, independent of whether
    /// the old approval is still eligible for a write under current bindings.
    pub fn role_handoff_source(&self,id:&str)->Result<(i64,String),String>{
        self.with_connection(|c|c.query_row("SELECT binding_revision,source_role FROM role_handoff_details WHERE handoff_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(db_error))
    }
    pub fn role_bridge(&self, workstream: &str) -> Result<RoleBridge, String> {
        self.with_connection(|c| bridge(c, workstream))
    }
    pub fn bind_role_bridge(
        &self,
        workstream: &str,
        revision: i64,
        decision: RoleBindingInput,
        execution: RoleBindingInput,
    ) -> Result<RoleBridge, String> {
        for input in [&decision, &execution] {
            if !matches!(input.provider.as_str(), "CODEX" | "CHATGPT")
                || input.external_id.len() < 8
                || input.label.trim().is_empty()
            {
                return Err("BRIDGE_BINDING_INVALID".into());
            }
            if input.provider == "CODEX"
                && input
                    .cwd
                    .as_deref()
                    .is_none_or(|v| !Path::new(v).is_absolute())
            {
                return Err("BRIDGE_CODEX_ROOT_REQUIRED".into());
            }
        }
        if decision.provider == "CHATGPT" && execution.provider == "CHATGPT" {
            return Err("BRIDGE_PAIR_UNSUPPORTED".into());
        }
        if decision.provider == execution.provider && decision.external_id == execution.external_id
        {
            return Err("BRIDGE_SAME_NATIVE_TARGET".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let w=workstream_by_id(&tx,workstream)?; ensure_workstream_not_trashed(&w)?;
            if w.binding_revision!=revision { return Err("BRIDGE_BINDING_CHANGED".into()); }
            let busy:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM handoffs WHERE workstream_id=?1 AND status='SENDING') OR EXISTS(SELECT 1 FROM provider_runs WHERE workstream_id=?1 AND status IN ('STARTING','RUNNING','UNKNOWN'))",params![workstream],|r|r.get(0)).map_err(db_error)?;
            if busy { return Err("BRIDGE_WRITER_UNRESOLVED".into()); }
            // Rebinding a role never moves or steals a native identity from another Workstream.
            for input in [&decision,&execution] {
                let other:Option<String>=tx.query_row("SELECT workstream_id FROM endpoints WHERE provider=?1 AND external_id=?2",params![input.provider,input.external_id],|r|r.get(0)).optional().map_err(db_error)?;
                if other.as_deref().is_some_and(|v|v!=workstream) { return Err("BRIDGE_NATIVE_TARGET_ALREADY_BOUND".into()); }
            }
            tx.execute("UPDATE endpoints SET status='SUPERSEDED',superseded_at=?2 WHERE workstream_id=?1 AND status='ACTIVE'",params![workstream,now()]).map_err(db_error)?;
            for (role,input) in [("DECISION",decision),("EXECUTION",execution)] {
                let existing:Option<String>=tx.query_row("SELECT id FROM endpoints WHERE provider=?1 AND external_id=?2",params![input.provider,input.external_id],|r|r.get(0)).optional().map_err(db_error)?;
                let endpoint=if let Some(existing)=existing {
                    tx.execute("UPDATE endpoints SET status='ACTIVE',superseded_at=NULL,bridge_role=?2,label=?3 WHERE id=?1",params![existing,role,input.label]).map_err(db_error)?; existing
                } else {
                    let endpoint=id();tx.execute("INSERT INTO endpoints(id,workstream_id,provider,external_id,label,status,created_at,bridge_role) VALUES(?1,?2,?3,?4,?5,'ACTIVE',?6,?7)",params![endpoint,workstream,input.provider,input.external_id,input.label,now(),role]).map_err(db_error)?;endpoint
                };
                tx.execute("INSERT INTO endpoint_role_details(endpoint_id,cwd) VALUES(?1,?2) ON CONFLICT(endpoint_id) DO UPDATE SET cwd=excluded.cwd",params![endpoint,input.cwd]).map_err(db_error)?;
            }
            tx.execute("UPDATE workstreams SET binding_revision=binding_revision+1,updated_at=?2 WHERE id=?1",params![workstream,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;bridge(c,workstream)
        })
    }
    pub fn prepare_role_handoff(
        &self,
        workstream: &str,
        role: &str,
        observation: &str,
        edited: &str,
    ) -> Result<HandoffHistoryItem, String> {
        self.prepare_role_handoff_with_attachments(
            workstream,
            role,
            observation,
            edited,
            Vec::new(),
        )
    }
    pub fn prepare_role_handoff_with_attachments(
        &self,
        workstream: &str,
        role: &str,
        observation: &str,
        edited: &str,
        attachments: Vec<NewAttachment>,
    ) -> Result<HandoffHistoryItem, String> {
        if edited.trim().is_empty() || edited.len() > MAX_WORKSTREAM_DRAFT_BYTES {
            return Err("BRIDGE_TEXT_INVALID".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let b=bridge(&tx,workstream)?;
            let (src,dst)=match role {"DECISION"=>(b.decision,b.execution),"EXECUTION"=>(b.execution,b.decision),_=>return Err("BRIDGE_ROLE_INVALID".into())};
            let src=src.ok_or("BRIDGE_SOURCE_MISSING")?.endpoint;let dst=dst.ok_or("BRIDGE_TARGET_MISSING")?.endpoint;
            let original:(String,String)=tx.query_row("SELECT text,assistant_identity FROM reply_observations WHERE id=?1 AND endpoint_id=?2 AND workstream_id=?3",params![observation,src.id,workstream],|r|Ok((r.get(0)?,r.get(1)?))).map_err(db_error)?;
            let record=id();let direction=format!("{}_TO_{}",src.provider,dst.provider);
            let mut reviewed_text=edited.to_string();
            if dst.provider=="CODEX"&&!attachments.is_empty(){
                reviewed_text.push_str("\n\nSelected attachments in the target conversation's project:\n");
                for a in &attachments {reviewed_text.push_str(&format!("- {}: .aiwr/incoming/{}/{} (SHA-256: {})\n",a.filename,record,a.filename,a.sha256.as_deref().unwrap_or("unavailable")));}
            }
            let mut envelope=vec![original.1.clone(),dst.id.clone(),reviewed_text.clone()];
            for a in &attachments {
                if a.sha256.as_deref().is_none_or(|s|s.len()!=64||!s.bytes().all(|b|b.is_ascii_hexdigit())) || Path::new(&a.filename).components().count()!=1 || a.size.is_none_or(|s|s<0) {return Err("BRIDGE_ATTACHMENT_INVALID".into());}
                envelope.extend([a.filename.clone(),a.sha256.clone().unwrap(),a.size.unwrap().to_string()]);
            }
            let fields:Vec<&[u8]>=envelope.iter().map(|s|s.as_bytes()).collect();let hash=length_prefixed_hash(&fields);
            tx.execute("INSERT INTO handoffs(id,workstream_id,source_endpoint_id,destination_endpoint_id,direction,source_response_identity,original_text,approved_text,status,payload_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'READY',?9,?10)",params![record,workstream,src.id,dst.id,direction,original.1,original.0,reviewed_text,hash,now()]).map_err(db_error)?;
            tx.execute("INSERT INTO role_handoff_details(handoff_id,binding_revision,source_role) VALUES(?1,?2,?3)",params![record,b.binding_revision,role]).map_err(db_error)?;
            for a in attachments {tx.execute("INSERT INTO handoff_attachments(id,handoff_id,filename,original_path,size,sha256,integrity_status,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![a.id,record,a.filename,a.original_path,a.size,a.sha256,a.integrity_status,now()]).map_err(db_error)?;}
            tx.commit().map_err(db_error)?;handoff_by_id(c,&record)
        })
    }
    pub fn approve_role_handoff(&self, handoff: &str) -> Result<HandoffHistoryItem, String> {
        let hash = self.role_handoff(handoff)?.payload_hash;
        self.approve_role_handoff_checked(handoff, &hash)
    }
    pub fn approve_role_handoff_checked(
        &self,
        handoff: &str,
        expected_hash: &str,
    ) -> Result<HandoffHistoryItem, String> {
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let h=handoff_by_id(&tx,handoff)?;validate_handoff(&tx,&h)?;
            if tx.execute("UPDATE handoffs SET status='APPROVED',approved_at=?2 WHERE id=?1 AND status='READY' AND payload_hash=?3",params![handoff,now(),expected_hash]).map_err(db_error)?!=1 {return Err("BRIDGE_REVIEW_CHANGED_OR_NOT_READY".into());}
            tx.commit().map_err(db_error)?;handoff_by_id(c,handoff)
        })
    }
    pub fn edit_role_handoff(
        &self,
        handoff: &str,
        expected_hash: &str,
        text: &str,
    ) -> Result<HandoffHistoryItem, String> {
        if text.trim().is_empty() || text.len() > MAX_WORKSTREAM_DRAFT_BYTES {
            return Err("BRIDGE_TEXT_INVALID".into());
        }
        self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let h=handoff_by_id(&tx,handoff)?;validate_handoff(&tx,&h)?;
            let mut envelope=vec![h.source_response_identity.clone().ok_or("BRIDGE_SOURCE_IDENTITY_MISSING")?,h.destination_endpoint.id.clone(),text.into()];
            for a in &h.attachments {envelope.extend([a.filename.clone(),a.sha256.clone().ok_or("BRIDGE_ATTACHMENT_INVALID")?,a.size.ok_or("BRIDGE_ATTACHMENT_INVALID")?.to_string()]);}
            let fields:Vec<&[u8]>=envelope.iter().map(|s|s.as_bytes()).collect();let hash=length_prefixed_hash(&fields);
            if tx.execute("UPDATE handoffs SET approved_text=?2,payload_hash=?3 WHERE id=?1 AND status='READY' AND payload_hash=?4",params![handoff,text,hash,expected_hash]).map_err(db_error)?!=1{return Err("BRIDGE_DRAFT_CHANGED".into());}
            tx.commit().map_err(db_error)?;handoff_by_id(c,handoff)
        })
    }
    pub fn claim_role_handoff(&self, handoff: &str) -> Result<HandoffHistoryItem, String> {
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let h=handoff_by_id(&tx,handoff)?;validate_handoff(&tx,&h)?;
            let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM handoffs WHERE status='SENT' AND source_response_identity=?1 AND destination_endpoint_id=?2 AND payload_hash=?3)",params![h.source_response_identity,h.destination_endpoint.id,h.payload_hash],|r|r.get(0)).map_err(db_error)?;
            if duplicate {return Err("BRIDGE_ALREADY_SENT".into());}
            let busy:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM handoffs WHERE workstream_id=?1 AND status='SENDING') OR EXISTS(SELECT 1 FROM provider_runs WHERE workstream_id=?1 AND status IN ('STARTING','RUNNING','UNKNOWN'))",params![h.workstream_id],|r|r.get(0)).map_err(db_error)?;
            if busy {return Err("BRIDGE_WRITER_UNRESOLVED".into());}
            if tx.execute("UPDATE handoffs SET status='SENDING' WHERE id=?1 AND status='APPROVED'",params![handoff]).map_err(db_error)?!=1 {return Err("BRIDGE_ALREADY_ATTEMPTED_OR_NOT_APPROVED".into());}
            tx.commit().map_err(db_error)?;handoff_by_id(c,handoff)
        })
    }
    pub fn role_handoff(&self, handoff: &str) -> Result<HandoffHistoryItem, String> {
        self.with_connection(|c| {
            c.query_row(
                "SELECT handoff_id FROM role_handoff_details WHERE handoff_id=?1",
                params![handoff],
                |r| r.get::<_, String>(0),
            )
            .map_err(db_error)?;
            handoff_by_id(c, handoff)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(provider: &str, id: &str) -> RoleBindingInput {
        RoleBindingInput {
            provider: provider.into(),
            external_id: id.into(),
            label: id.into(),
            cwd: (provider == "CODEX").then(|| "D:\\role-fixture".into()),
        }
    }
    fn fixture() -> (tempfile::TempDir, RouterStore, String) {
        let dir = tempfile::tempdir().unwrap();
        let s = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        let p = s.create_project("roles".into(), None).unwrap();
        let w = s.create_workstream(&p.id, "roles".into()).unwrap();
        (dir, s, w.id)
    }
    #[test]
    fn two_codex_roles_use_exact_distinct_ids_and_provider_lookup_fails_closed() {
        let (_dir, s, w) = fixture();
        let rev = s.role_bridge(&w).unwrap().binding_revision;
        let b = s
            .bind_role_bridge(
                &w,
                rev,
                input("CODEX", "thread-decision"),
                input("CODEX", "thread-execution"),
            )
            .unwrap();
        assert_eq!(b.decision.unwrap().endpoint.external_id, "thread-decision");
        assert_eq!(
            b.execution.unwrap().endpoint.external_id,
            "thread-execution"
        );
        assert!(s
            .active_endpoint_for_workstream(&w, Provider::Codex)
            .unwrap_err()
            .contains("AMBIGUOUS"));
        assert!(s
            .bind_endpoint(
                &w,
                Provider::Chatgpt,
                "conversation-new".into(),
                "new".into(),
                false
            )
            .is_err());
        assert!(s
            .bind_role_bridge(
                &w,
                b.binding_revision,
                input("CODEX", "thread-same"),
                input("CODEX", "thread-same")
            )
            .is_err());
    }
    #[test]
    fn both_mixed_assignments_and_legacy_fallback_keep_provider_truth() {
        let (_dir, s, w) = fixture();
        s.bind_endpoint(
            &w,
            Provider::Chatgpt,
            "conversation-old".into(),
            "old".into(),
            false,
        )
        .unwrap();
        s.bind_endpoint(
            &w,
            Provider::Codex,
            "thread-old".into(),
            "old".into(),
            false,
        )
        .unwrap();
        let old = s.role_bridge(&w).unwrap();
        assert!(!old.explicit_roles);
        let chat = old.decision.unwrap().endpoint;
        let codex = old.execution.unwrap().endpoint;
        let swapped = s
            .bind_role_bridge(
                &w,
                old.binding_revision,
                input("CODEX", "thread-old"),
                input("CHATGPT", "conversation-old"),
            )
            .unwrap();
        assert_eq!(swapped.decision.unwrap().endpoint.id, codex.id);
        assert_eq!(swapped.execution.unwrap().endpoint.id, chat.id);
        let b = s
            .bind_role_bridge(
                &w,
                swapped.binding_revision,
                input("CHATGPT", "conversation-old"),
                input("CODEX", "thread-old"),
            )
            .unwrap();
        assert_eq!(b.decision.unwrap().endpoint.id, chat.id);
        assert_eq!(b.execution.unwrap().endpoint.id, codex.id);
    }
    #[test]
    fn observed_source_edited_approval_separate_atomic_claim_and_restart_never_resend() {
        let (dir, s, w) = fixture();
        let b = s
            .bind_role_bridge(
                &w,
                s.role_bridge(&w).unwrap().binding_revision,
                input("CODEX", "thread-source"),
                input("CODEX", "thread-target"),
            )
            .unwrap();
        let src = b.decision.unwrap().endpoint;
        s.record_reply_observation(&w, &src.id, Some("native-result-1"), "native body", None)
            .unwrap();
        let obs = s.reply_observations_for_workstream(&w).unwrap().remove(0);
        let h = s
            .prepare_role_handoff(&w, "DECISION", &obs.id, "edited bytes\n")
            .unwrap();
        assert_eq!(h.direction, "CODEX_TO_CODEX");
        assert_eq!(h.status, "READY");
        assert_eq!(h.original_text, "native body");
        assert!(s.claim_role_handoff(&h.id).is_err());
        assert!(s.provider_runs_for_workstream(&w).unwrap().is_empty());
        s.approve_role_handoff(&h.id).unwrap();
        let claimed = s.claim_role_handoff(&h.id).unwrap();
        assert_eq!(claimed.approved_text, "edited bytes\n");
        assert!(s.claim_role_handoff(&h.id).is_err());
        drop(s);
        let reopened = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        assert!(reopened.claim_role_handoff(&h.id).is_err());
        assert_eq!(reopened.role_handoff(&h.id).unwrap().status, "SENDING");
    }
    #[test]
    fn role_change_invalidates_review_and_foreign_observation_cannot_be_imported() {
        let (_dir, s, w) = fixture();
        let b = s
            .bind_role_bridge(
                &w,
                s.role_bridge(&w).unwrap().binding_revision,
                input("CODEX", "thread-source"),
                input("CODEX", "thread-target"),
            )
            .unwrap();
        let src = b.decision.unwrap().endpoint;
        s.record_reply_observation(&w, &src.id, Some("native-1"), "source", None)
            .unwrap();
        let obs = s.reply_observations_for_workstream(&w).unwrap().remove(0);
        assert!(s
            .prepare_role_handoff(&w, "EXECUTION", &obs.id, "edited")
            .is_err());
        let h = s
            .prepare_role_handoff(&w, "DECISION", &obs.id, "edited")
            .unwrap();
        s.approve_role_handoff(&h.id).unwrap();
        s.bind_role_bridge(
            &w,
            b.binding_revision,
            input("CODEX", "thread-source"),
            input("CODEX", "thread-target-new"),
        )
        .unwrap();
        assert!(s.claim_role_handoff(&h.id).is_err());
    }
    #[test]
    fn migration_preserves_existing_history_attachments_settings_triggers_and_verified_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("router.db");
        let mut c = Connection::open(&path).unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        run_migrations(&mut c).unwrap();
        run_normal_feature_migrations(&mut c).unwrap();
        c.execute_batch("INSERT INTO projects(id,name,created_at,updated_at) VALUES('p','original',1,1);
            INSERT INTO workstreams(id,project_id,name,status,created_at,updated_at) VALUES('w','p','original','ACTIVE',1,1);
            INSERT INTO endpoints(id,workstream_id,provider,external_id,label,status,created_at) VALUES('a','w','CHATGPT','conversation-original','decision','ACTIVE',1),('b','w','CODEX','thread-original','execution','ACTIVE',1);
            INSERT INTO handoffs(id,workstream_id,source_endpoint_id,destination_endpoint_id,direction,original_text,approved_text,status,payload_hash,created_at) VALUES('h','w','a','b','CHATGPT_TO_CODEX','original bytes','approved bytes','APPROVED','original hash',1);
            INSERT INTO handoff_attachments(id,handoff_id,filename,original_path,size,sha256,created_at) VALUES('f','h','original.txt','D:/original.txt',53,'original file hash',1);
            INSERT INTO app_settings(key,value,updated_at) VALUES('last_project_id','p',1),('last_workstream_id','w',1);
            CREATE TABLE original_audit(value TEXT);
            CREATE TRIGGER original_handoff_trigger AFTER UPDATE ON handoffs BEGIN INSERT INTO original_audit(value) VALUES(new.status);END;").unwrap();
        let original: (String, String, String) = c
            .query_row(
                "SELECT original_text,approved_text,payload_hash FROM handoffs WHERE id='h'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        drop(c);
        let s = RouterStore::open_at(&path).unwrap();
        let h = s.handoff_by_id("h").unwrap();
        assert_eq!((h.original_text, h.approved_text, h.payload_hash), original);
        assert_eq!(h.attachments[0].id, "f");
        assert_eq!(
            s.snapshot().unwrap().selected_workstream_id.as_deref(),
            Some("w")
        );
        s.transition_handoff("h", "SENDING", None).unwrap();
        s.with_connection(|c| {
            assert_eq!(
                c.query_row("SELECT count(*) FROM original_audit", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert!(!c
                .prepare("PRAGMA foreign_key_check")
                .unwrap()
                .exists([])
                .unwrap());
            Ok(())
        })
        .unwrap();
        let backups = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|p| {
                p.file_name()
                    .to_string_lossy()
                    .contains("before-bridge-roles")
            })
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        let backup = Connection::open(backups[0].path()).unwrap();
        assert_eq!(
            backup
                .query_row("SELECT approved_text FROM handoffs WHERE id='h'", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "approved bytes"
        );
        assert_eq!(
            backup
                .query_row("SELECT status FROM handoffs WHERE id='h'", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "APPROVED"
        );
    }
    #[test]
    fn approval_is_bound_to_the_exact_edited_hash_and_attachment_manifest() {
        let (_dir, s, w) = fixture();
        let b = s
            .bind_role_bridge(
                &w,
                s.role_bridge(&w).unwrap().binding_revision,
                input("CODEX", "thread-source"),
                input("CODEX", "thread-target"),
            )
            .unwrap();
        let src = b.decision.unwrap().endpoint;
        s.record_reply_observation(&w, &src.id, Some("native-1"), "source", None)
            .unwrap();
        let obs = s.reply_observations_for_workstream(&w).unwrap().remove(0);
        let h = s
            .prepare_role_handoff_with_attachments(
                &w,
                "DECISION",
                &obs.id,
                "first",
                vec![NewAttachment {
                    id: "file-1".into(),
                    filename: "selected.txt".into(),
                    original_path: "D:/selected.txt".into(),
                    size: Some(53),
                    sha256: Some("a".repeat(64)),
                    integrity_status: Some("VERIFIED".into()),
                }],
            )
            .unwrap();
        let edited = s
            .edit_role_handoff(&h.id, &h.payload_hash, "second")
            .unwrap();
        assert_ne!(edited.payload_hash, h.payload_hash);
        assert_eq!(edited.attachments[0].sha256, h.attachments[0].sha256);
        assert!(s
            .approve_role_handoff_checked(&h.id, &h.payload_hash)
            .is_err());
        assert!(s
            .edit_role_handoff(&h.id, &h.payload_hash, "third")
            .is_err());
        s.approve_role_handoff_checked(&h.id, &edited.payload_hash)
            .unwrap();
        assert!(s
            .edit_role_handoff(&h.id, &edited.payload_hash, "after approval")
            .is_err());
    }
    #[test]
    fn two_codex_roles_keep_independent_terminal_results(){
        let (_dir,s,w)=fixture();let b=s.bind_role_bridge(&w,s.role_bridge(&w).unwrap().binding_revision,input("CODEX","thread-decision"),input("CODEX","thread-execution")).unwrap();
        let mut ids=Vec::new();
        for (side,turn,text) in [(b.decision.unwrap(),"turn-decision","Decision result"),(b.execution.unwrap(),"turn-execution","Execution result")] {
            let r=s.create_provider_run(&w,&side.endpoint.id,"CODEX",None,None,"STARTING").unwrap();s.attach_provider_run_external_identity(&r.id,"CODEX",turn).unwrap();s.accept_completed_provider_result(&r.id,turn,&format!("item-{turn}"),text.into()).unwrap();ids.push(r.id);
        }
        let runs=s.provider_runs_for_workstream(&w).unwrap();assert_eq!(runs.iter().find(|r|r.id==ids[0]).unwrap().result_text.as_deref(),Some("Decision result"));assert_eq!(runs.iter().find(|r|r.id==ids[1]).unwrap().result_text.as_deref(),Some("Execution result"));
    }
}
