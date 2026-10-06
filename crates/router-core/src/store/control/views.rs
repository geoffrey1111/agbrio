//! Scope-constrained read models. Historical endpoint IDs remain exact.
use super::*;

#[derive(Clone, Debug)]
pub struct RegisteredRoot {
    pub canonical_path: String,
    pub root_revision: i64,
    pub policy: PolicySnapshot,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeBaselineRootMigration {
    pub revisions: Vec<i64>,
    pub changed: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct ProjectView {
    pub id: String,
    pub label: String,
    pub root_label: String,
    pub root_revision: i64,
}
#[derive(Clone, Debug, Serialize)]
pub struct NamedView {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct TargetView {
    pub endpoint_id: String,
    pub thread_id: String,
    pub label: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct ContextView {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_revision: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workstream: Option<NamedView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_revision: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetView>,
}
/// Exact Browser-Bridge management association.  The caller never supplies a
/// Workstream: owner authorization and the ACTIVE ChatGPT endpoint resolve it
/// together in one Store transaction.
#[derive(Clone, Debug, Serialize)]
pub struct ManagementContextView {
    pub context: ContextView,
    pub chatgpt_endpoint: Endpoint,
    pub codex_endpoint: Endpoint,
}
#[derive(Clone, Debug)]
pub struct ManagementObservedCall<'a> {
    pub owner: &'a str,
    pub workstream_id: &'a str,
    pub endpoint_id: &'a str,
    pub conversation_id: &'a str,
    pub source_client_id: &'a str,
    pub source_message_id: &'a str,
    pub source_turn_key: &'a str,
    pub source_user_turn_key: &'a str,
    pub request_id: &'a str,
    pub operation: &'a str,
    pub request_hash: &'a str,
}
#[derive(Clone, Debug)]
pub struct ManagementCallRecord {
    pub id: String,
    pub request_id: String,
    pub state: String,
    pub result_json: Option<String>,
    pub result_user_turn_key: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct RunView {
    pub run_id: String,
    pub handoff_id: String,
    pub endpoint_id: String,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub handoff_status: String,
    pub status: String,
    pub pending_native_request: bool,
    pub result_available: bool,
    pub result_state: String,
    pub dispatch_revision: i64,
    pub code: Option<String>,
}
#[derive(Debug)]
pub struct ResultChunk {
    pub run_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub result_identity: String,
    pub result_hash: String,
    pub text: String,
    pub next_offset: Option<usize>,
    pub receipt_id: String,
}
fn context_projection(c: &Connection, ctx: &ContextRecord) -> Result<ContextView, String> {
    let mut result = ContextView {
        state: ctx.state.clone(),
        context_id: Some(ctx.id.clone()),
        context_revision: Some(ctx.revision),
        workstream: None,
        binding_revision: None,
        target: None,
    };
    if ctx.state == "BOUND" {
        let s = binding(c, ctx)?;
        let (work_label,target_label):(String,String)=c.query_row("SELECT w.name,e.label FROM workstreams w JOIN endpoints e ON e.id=?2 WHERE w.id=?1",params![s.workstream_id,s.endpoint_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(db_error)?;
        result.workstream = Some(NamedView {
            id: s.workstream_id,
            label: work_label.chars().take(200).collect(),
        });
        result.binding_revision = Some(s.binding_revision);
        result.target = Some(TargetView {
            endpoint_id: s.endpoint_id,
            thread_id: s.thread_id,
            label: target_label.chars().take(200).collect(),
        });
    }
    Ok(result)
}
fn owned_context(c: &Connection, identity: &ScopeIdentity) -> Result<ContextRecord, String> {
    let context = context(c, identity)?.ok_or("NOT_FOUND")?;
    if context.state != "BOUND" {
        return Err("NOT_FOUND".into());
    }
    Ok(context)
}
fn run(c: &Connection, identity: &ScopeIdentity, run_id: &str) -> Result<RunView, String> {
    let ctx = owned_context(c, identity)?;
    c.query_row("SELECT p.id,h.id,e.id,e.external_id,p.external_run_id,h.status,p.status,r.result_state,d.revision,p.terminal_code FROM provider_runs p JOIN handoffs h ON h.id=p.origin_handoff_id JOIN endpoints e ON e.id=p.endpoint_id JOIN handoff_dispatches d ON d.run_id=p.id JOIN provider_result_details r ON r.run_id=p.id WHERE p.id=?1 AND h.source_context_id=?2 AND h.workstream_id=?3",params![run_id,ctx.id,ctx.workstream_id],|r|{
        let result_state:String=r.get(7)?;
        Ok(RunView{run_id:r.get(0)?,handoff_id:r.get(1)?,endpoint_id:r.get(2)?,thread_id:r.get(3)?,turn_id:r.get(4)?,handoff_status:r.get(5)?,status:r.get(6)?,pending_native_request:false,result_available:result_state=="AVAILABLE",result_state,dispatch_revision:r.get(8)?,code:r.get(9)?})
    }).optional().map_err(db_error)?.ok_or("NOT_FOUND".into())
}
fn owned_run(c: &Connection, principal: &str, run_id: &str) -> Result<RunView, String> {
    // A historical run is either owned by its still-bound Control Context or
    // by the independently authenticated owner recorded on an endpoint-source
    // Relay handoff. Do not turn a Relay owner into a synthetic host scope:
    // a scope key represents a real authenticated browser session.
    c.query_row(
        "SELECT p.id,h.id,e.id,e.external_id,p.external_run_id,h.status,p.status,r.result_state,d.revision,p.terminal_code
         FROM provider_runs p
         JOIN handoffs h ON h.id=p.origin_handoff_id
         JOIN endpoints e ON e.id=p.endpoint_id
         JOIN handoff_dispatches d ON d.run_id=p.id
         JOIN provider_result_details r ON r.run_id=p.id
         LEFT JOIN control_contexts c ON c.id=h.source_context_id
         LEFT JOIN relay_handoff_details relay ON relay.handoff_id=h.id
         WHERE p.id=?1 AND (
            (h.source_kind='CONTROL_CONTEXT' AND c.principal_key=?2 AND c.state='BOUND' AND c.workstream_id=h.workstream_id)
            OR (h.source_kind='ENDPOINT' AND relay.owner_principal_key=?2 AND relay.consumed_at IS NOT NULL)
         )",
        params![run_id, principal],
        |r| {
            let result_state: String = r.get(7)?;
            Ok(RunView {
                run_id: r.get(0)?, handoff_id: r.get(1)?, endpoint_id: r.get(2)?,
                thread_id: r.get(3)?, turn_id: r.get(4)?, handoff_status: r.get(5)?,
                status: r.get(6)?, pending_native_request: false,
                result_available: result_state == "AVAILABLE", result_state,
                dispatch_revision: r.get(8)?, code: r.get(9)?,
            })
        },
    )
    .optional()
    .map_err(db_error)?
    .ok_or("NOT_FOUND".into())
}
impl RouterStore {
    /// One approved source-input migration updates both immutable source and
    /// Router-owned launch identities in one root-policy transaction.
    pub fn migrate_native_source_input_roots(
        &self,
        expected_config_digest: &str,
        expected_launch_digest: Option<&str>,
        next_config_digest: &str,
        next_launch_digest: Option<&str>,
    ) -> Result<NativeBaselineRootMigration, String> {
        if !valid_digest(expected_config_digest)
            || !valid_digest(next_config_digest)
            || expected_launch_digest.is_some_and(|digest| !valid_digest(digest))
            || next_launch_digest.is_some_and(|digest| !valid_digest(digest))
        {
            return Err("POLICY_CHANGED".into());
        }
        self.with_connection(|c| {
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let busy: bool = tx.query_row(
                // A root migration changes only future launch policy. It
                // cannot resume, unlock, or rewrite any durable UNKNOWN, so
                // historical UNKNOWN evidence must not make local Review
                // recovery permanently impossible. Live native processes and
                // incomplete binding creation still stop the migration.
                "SELECT EXISTS(SELECT 1 FROM provider_runs WHERE status IN ('STARTING','RUNNING')) OR EXISTS(SELECT 1 FROM binding_requests WHERE state IN ('CREATING','VERIFYING','UNKNOWN'))",
                [], |r| r.get(0),
            ).map_err(db_error)?;
            if busy { return Err("CREATION_BUSY".into()); }
            let rows: Vec<(String, i64, String, String)> = tx.prepare(
                "SELECT project_id,revision,policy_json,policy_hash FROM project_execution_roots ORDER BY project_id"
            ).map_err(db_error)?.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .map_err(db_error)?.collect::<Result<_, _>>().map_err(db_error)?;
            if rows.is_empty() { return Err("NOT_FOUND".into()); }
            let mut parsed = Vec::with_capacity(rows.len());
            let mut all_expected = true;
            let mut all_next = true;
            for (project, revision, raw, stored_hash) in rows {
                let policy: PolicySnapshot = serde_json::from_str(&raw).map_err(|_| "POLICY_CHANGED")?;
                if policy.hash()? != stored_hash { return Err("POLICY_CHANGED".into()); }
                all_expected &= policy.config_digest == expected_config_digest
                    && policy.native_launch_digest.as_deref() == expected_launch_digest;
                all_next &= policy.config_digest == next_config_digest
                    && policy.native_launch_digest.as_deref() == next_launch_digest;
                parsed.push((project, revision, policy, stored_hash));
            }
            if all_next {
                tx.commit().map_err(db_error)?;
                return Ok(NativeBaselineRootMigration { revisions: parsed.into_iter().map(|(_, revision, _, _)| revision).collect(), changed: false });
            }
            if !all_expected { return Err("POLICY_CHANGED".into()); }
            let mut revisions = Vec::with_capacity(parsed.len());
            for (project, revision, mut policy, stored_hash) in parsed {
                policy.config_digest = next_config_digest.into();
                policy.native_launch_digest = next_launch_digest.map(str::to_owned);
                let hash = policy.hash()?;
                let changed = tx.execute(
                    "UPDATE project_execution_roots SET revision=revision+1,policy_json=?2,policy_hash=?3,updated_at=?4 WHERE project_id=?1 AND revision=?5 AND policy_hash=?6",
                    params![project, serde_json::to_string(&policy).map_err(|_| "INTERNAL")?, hash, now(), revision, stored_hash],
                ).map_err(db_error)?;
                if changed != 1 { return Err("ROOT_CHANGED".into()); }
                revisions.push(revision + 1);
            }
            tx.commit().map_err(db_error)?;
            Ok(NativeBaselineRootMigration { revisions, changed: true })
        })
    }
    /// Narrow follow-up to the approved native-baseline migration. It binds
    /// current roots to the exact Router launch overlay without rewriting any
    /// historic binding, draft, approval, or run evidence.
    pub fn amend_native_launch_policy_roots(
        &self,
        expected_config_digest: &str,
        expected_launch_digest: Option<&str>,
        next_launch_digest: Option<&str>,
    ) -> Result<NativeBaselineRootMigration, String> {
        if !valid_digest(expected_config_digest)
            || expected_launch_digest.is_some_and(|digest| !valid_digest(digest))
            || next_launch_digest.is_some_and(|digest| !valid_digest(digest))
        {
            return Err("POLICY_CHANGED".into());
        }
        self.with_connection(|c| {
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            // Every UNKNOWN remains unresolved evidence.  Policy migration
            // cannot bypass it without a separately approved, evidence-bound
            // resolution path.
            let busy: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM provider_runs WHERE status IN ('STARTING','RUNNING','UNKNOWN')) OR EXISTS(SELECT 1 FROM binding_requests WHERE state IN ('CREATING','VERIFYING','UNKNOWN'))",
                    [],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            if busy {
                return Err("CREATION_BUSY".into());
            }
            let rows: Vec<(String, i64, String, String)> = tx
                .prepare("SELECT project_id,revision,policy_json,policy_hash FROM project_execution_roots ORDER BY project_id")
                .map_err(db_error)?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .map_err(db_error)?
                .collect::<Result<_, _>>()
                .map_err(db_error)?;
            if rows.is_empty() {
                return Err("NOT_FOUND".into());
            }
            let mut parsed = Vec::with_capacity(rows.len());
            let mut all_expected = true;
            let mut all_next = true;
            for (project, revision, raw, stored_hash) in rows {
                let policy: PolicySnapshot = serde_json::from_str(&raw).map_err(|_| "POLICY_CHANGED")?;
                if policy.hash()? != stored_hash {
                    return Err("POLICY_CHANGED".into());
                }
                all_expected &= policy.config_digest == expected_config_digest
                    && policy.native_launch_digest.as_deref() == expected_launch_digest
                    && matches!(
                        policy.permission_mode,
                        PermissionMode::ReadOnly | PermissionMode::WorkspaceWrite
                    )
                    && matches!(
                        policy.approval_mode,
                        ApprovalMode::OnRequest | ApprovalMode::Never
                    );
                all_next &= policy.config_digest == expected_config_digest
                    && policy.native_launch_digest.as_deref() == next_launch_digest
                    && policy.permission_mode == PermissionMode::WorkspaceWrite
                    && policy.approval_mode == ApprovalMode::Never;
                parsed.push((project, revision, policy, stored_hash));
            }
            if all_next {
                tx.commit().map_err(db_error)?;
                return Ok(NativeBaselineRootMigration {
                    revisions: parsed.into_iter().map(|(_, revision, _, _)| revision).collect(),
                    changed: false,
                });
            }
            if !all_expected {
                return Err("POLICY_CHANGED".into());
            }
            let mut revisions = Vec::with_capacity(parsed.len());
            for (project, revision, mut policy, stored_hash) in parsed {
                policy.native_launch_digest = next_launch_digest.map(str::to_owned);
                policy.permission_mode = PermissionMode::WorkspaceWrite;
                policy.approval_mode = ApprovalMode::Never;
                let hash = policy.hash()?;
                let changed = tx.execute(
                    "UPDATE project_execution_roots SET revision=revision+1,policy_json=?2,policy_hash=?3,updated_at=?4 WHERE project_id=?1 AND revision=?5 AND policy_hash=?6",
                    params![project, serde_json::to_string(&policy).map_err(|_| "INTERNAL")?, hash, now(), revision, stored_hash],
                ).map_err(db_error)?;
                if changed != 1 {
                    return Err("ROOT_CHANGED".into());
                }
                revisions.push(revision + 1);
            }
            tx.commit().map_err(db_error)?;
            Ok(NativeBaselineRootMigration { revisions, changed: true })
        })
    }
    /// Narrow offline maintenance path for a previously approved preview.
    /// It changes only current root policy snapshots; historic binding/draft/
    /// approval rows remain evidence of their former policy.
    pub fn migrate_native_baseline_roots(
        &self,
        expected_digest: &str,
        next_digest: &str,
    ) -> Result<NativeBaselineRootMigration, String> {
        if expected_digest.len() != 64
            || next_digest.len() != 64
            || !expected_digest.bytes().all(|b| b.is_ascii_hexdigit())
            || !next_digest.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("POLICY_CHANGED".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let busy:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM provider_runs WHERE status IN ('STARTING','RUNNING','UNKNOWN')) OR EXISTS(SELECT 1 FROM binding_requests WHERE state IN ('CREATING','VERIFYING','UNKNOWN'))",[],|r|r.get(0)).map_err(db_error)?;
            if busy { return Err("CREATION_BUSY".into()); }
            let rows:Vec<(String,i64,String,String)>=tx.prepare("SELECT project_id,revision,policy_json,policy_hash FROM project_execution_roots ORDER BY project_id").map_err(db_error)?.query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(db_error)?.collect::<Result<_,_>>().map_err(db_error)?;
            if rows.is_empty() { return Err("NOT_FOUND".into()); }
            let mut parsed = Vec::with_capacity(rows.len());
            let mut all_expected = true;
            let mut all_next = true;
            for (project, revision, raw, stored_hash) in rows {
                let policy:PolicySnapshot=serde_json::from_str(&raw).map_err(|_|"POLICY_CHANGED")?;
                if policy.hash()? != stored_hash { return Err("POLICY_CHANGED".into()); }
                all_expected &= policy.config_digest == expected_digest;
                all_next &= policy.config_digest == next_digest;
                parsed.push((project, revision, policy, stored_hash));
            }
            if all_next {
                tx.commit().map_err(db_error)?;
                return Ok(NativeBaselineRootMigration {
                    revisions: parsed.into_iter().map(|(_, revision, _, _)| revision).collect(),
                    changed: false,
                });
            }
            if !all_expected { return Err("POLICY_CHANGED".into()); }
            let mut revisions=Vec::with_capacity(parsed.len());
            for (project,revision,mut policy,stored_hash) in parsed {
                policy.config_digest=next_digest.into(); let hash=policy.hash()?; let changed=tx.execute("UPDATE project_execution_roots SET revision=revision+1,policy_json=?2,policy_hash=?3,updated_at=?4 WHERE project_id=?1 AND revision=?5 AND policy_hash=?6",params![project,serde_json::to_string(&policy).map_err(|_|"INTERNAL")?,hash,now(),revision,stored_hash]).map_err(db_error)?;
                if changed!=1 { return Err("ROOT_CHANGED".into()); } revisions.push(revision+1);
            }
            tx.commit().map_err(db_error)?; Ok(NativeBaselineRootMigration { revisions, changed: true })
        })
    }
    /// Owner maintenance projection; no paths or database download are exposed.
    pub fn preview_database_bytes(&self) -> Result<u64, String> {
        self.with_connection(|c| {
            let path = c.path().ok_or("INTERNAL")?;
            let mut total = std::fs::metadata(path).map_err(|_| "INTERNAL")?.len();
            for suffix in ["-wal", "-shm"] {
                match std::fs::metadata(format!("{path}{suffix}")) {
                    Ok(m) => total = total.saturating_add(m.len()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Err("INTERNAL".into()),
                }
            }
            Ok(total)
        })
    }

    /// Internal worker identity read. Never mapped to an unscoped HTTP/tool route.
    pub fn with_run_identity(&self, run: &str) -> Result<(String, Option<String>), String> {
        self.with_connection(|c| c.query_row("SELECT e.external_id,p.external_run_id FROM provider_runs p JOIN endpoints e ON e.id=p.endpoint_id WHERE p.id=?1",params![run],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?.ok_or("NOT_FOUND".into()))
    }

    pub fn browser_handoff_scope(
        &self,
        principal: &str,
        handoff: &str,
    ) -> Result<ScopeIdentity, String> {
        self.with_connection(|c|c.query_row("SELECT c.principal_key,c.scope_key,c.key_version,c.host_scope_id FROM control_contexts c JOIN handoffs h ON h.source_context_id=c.id WHERE h.id=?1 AND c.principal_key=?2 AND c.state='BOUND' AND c.workstream_id=h.workstream_id",params![handoff,principal],|r| {let legacy:String=r.get(1)?;let host:Option<String>=r.get(3)?;Ok(ScopeIdentity{principal_key:r.get(0)?,scope_key:host.unwrap_or(legacy),key_version:r.get(2)?})}).optional().map_err(db_error)?.ok_or("NOT_FOUND".into()))
    }
    pub fn browser_run_scope(
        &self,
        principal: &str,
        run_id: &str,
    ) -> Result<ScopeIdentity, String> {
        self.with_connection(|c|c.query_row("SELECT c.principal_key,c.scope_key,c.key_version,c.host_scope_id FROM control_contexts c JOIN handoffs h ON h.source_context_id=c.id JOIN provider_runs p ON p.origin_handoff_id=h.id WHERE p.id=?1 AND c.principal_key=?2 AND c.state='BOUND' AND c.workstream_id=h.workstream_id",params![run_id,principal],|r| {let legacy:String=r.get(1)?;let host:Option<String>=r.get(3)?;Ok(ScopeIdentity{principal_key:r.get(0)?,scope_key:host.unwrap_or(legacy),key_version:r.get(2)?})}).optional().map_err(db_error)?.ok_or("NOT_FOUND".into()))
    }
    pub fn run_dispatch_id(&self, principal: &str, run_id: &str) -> Result<String, String> {
        self.with_connection(|c| {
            owned_run(c, principal, run_id)?;
            c.query_row(
                "SELECT dispatch_id FROM handoff_dispatches WHERE run_id=?1",
                params![run_id],
                |r| r.get(0),
            )
            .map_err(db_error)
        })
    }
    /// Read a historical run from either legitimate Review ownership source.
    /// This is deliberately separate from `scoped_run`, whose ScopeIdentity
    /// remains a real browser-host capability for control-context requests.
    pub fn browser_owned_run(&self, principal: &str, run_id: &str) -> Result<RunView, String> {
        self.with_connection(|c| owned_run(c, principal, run_id))
    }
    /// Durable idempotent lookup also works after browser memory was replaced.
    /// It cannot create a grant or perform a second dispatch.
    pub fn consumed_approval(
        &self,
        principal: &str,
        handoff: &str,
        nonce_hash: &str,
        revision: i64,
        payload_hash: &str,
    ) -> Result<Option<ClaimedRun>, String> {
        self.browser_handoff_scope(principal, handoff)?;
        self.with_connection(|c|c.query_row("SELECT x.run_id,x.handoff_id,x.dispatch_id FROM handoff_dispatches x JOIN control_handoff_details d ON d.handoff_id=x.handoff_id JOIN handoffs h ON h.id=x.handoff_id WHERE x.handoff_id=?1 AND d.approved_principal_key=?2 AND d.review_nonce_hash=?3 AND d.draft_revision=?4 AND h.payload_hash=?5 AND d.consumed_at IS NOT NULL",params![handoff,principal,nonce_hash,revision,payload_hash],|r|Ok(ClaimedRun{run_id:r.get(0)?,handoff_id:r.get(1)?,dispatch_id:r.get(2)?,newly_claimed:false})).optional().map_err(db_error))
    }
    pub fn mark_browser_result_read(
        &self,
        principal: &str,
        run_id: &str,
        hash: &str,
    ) -> Result<(), String> {
        let identity = self.browser_run_scope(principal, run_id)?;
        self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let view=run(&tx,&identity,run_id)?;
            if view.result_state!="AVAILABLE" {return Err("RESULT_UNAVAILABLE".into());}
            let affected=tx.execute("UPDATE provider_runs SET reviewed_at=COALESCE(reviewed_at,?3) WHERE id=?1 AND id IN (SELECT run_id FROM provider_result_details WHERE result_hash=?2 AND result_state='AVAILABLE')",params![run_id,hash,now()]).map_err(db_error)?;
            if affected!=1 {return Err("HISTORY_CHANGED".into());}tx.commit().map_err(db_error)
        })
    }
    pub fn mark_browser_owned_result_read(
        &self,
        principal: &str,
        run_id: &str,
        hash: &str,
    ) -> Result<(), String> {
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let view=owned_run(&tx,principal,run_id)?;
            if view.result_state!="AVAILABLE" {return Err("RESULT_UNAVAILABLE".into());}
            let affected=tx.execute("UPDATE provider_runs SET reviewed_at=COALESCE(reviewed_at,?3) WHERE id=?1 AND id IN (SELECT run_id FROM provider_result_details WHERE result_hash=?2 AND result_state='AVAILABLE')",params![run_id,hash,now()]).map_err(db_error)?;
            if affected!=1 {return Err("HISTORY_CHANGED".into());}
            tx.commit().map_err(db_error)
        })
    }
    /// Trusted local configuration import only; there is no public registration
    /// tool. Existing roots cannot silently change under outstanding approvals.
    pub fn import_registered_root(
        &self,
        project_id: &str,
        label: &str,
        canonical_path: &str,
        policy: &PolicySnapshot,
    ) -> Result<(), String> {
        Uuid::parse_str(project_id).map_err(|_| "INVALID_CONFIG")?;
        policy.validate()?;
        if label.trim().is_empty()
            || label.chars().count() > 200
            || !Path::new(canonical_path).is_absolute()
        {
            return Err("INVALID_CONFIG".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let old:Option<(String,String)>=tx.query_row("SELECT canonical_path,policy_hash FROM project_execution_roots WHERE project_id=?1",params![project_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
            if let Some((path,hash))=old {if path!=canonical_path || hash!=policy.hash()? {return Err("ROOT_CHANGED".into());}return Ok(());}
            tx.execute("INSERT INTO projects(id,name,created_at,updated_at) VALUES(?1,?2,?3,?3)",params![project_id,label,now()]).map_err(db_error)?;
            tx.execute("INSERT INTO project_execution_roots(project_id,canonical_path,path_identity_hash,revision,policy_json,policy_hash,updated_at) VALUES(?1,?2,?3,1,?4,?5,?6)",params![project_id,canonical_path,policy.root_identity_hash,serde_json::to_string(policy).map_err(|_|"INTERNAL")?,policy.hash()?,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)
        })
    }
    pub fn owner_workstream_revision(
        &self,
        principal: &str,
        workstream: &str,
    ) -> Result<i64, String> {
        self.with_connection(|c|c.query_row("SELECT w.binding_revision FROM workstreams w JOIN control_contexts c ON c.workstream_id=w.id AND c.state='BOUND' WHERE w.id=?1 AND c.principal_key=?2 AND w.status='ACTIVE' AND w.trashed_at IS NULL",params![workstream,principal],|r|r.get(0)).optional().map_err(db_error)?.ok_or("NOT_FOUND".into()))
    }
    /// Browser review may enumerate only Workstreams whose currently bound
    /// control context belongs to this principal. This is presentation
    /// authorization, not a routing selector: callers still read each exact
    /// Endpoint from the Workstream snapshot.
    pub fn owner_workstream_ids(&self, principal: &str) -> Result<Vec<String>, String> {
        self.with_connection(|c| {
            let mut statement = c.prepare("SELECT w.id FROM workstreams w WHERE w.status='ACTIVE' AND w.trashed_at IS NULL AND EXISTS(SELECT 1 FROM control_contexts x WHERE x.workstream_id=w.id AND x.principal_key=?1 AND x.state='BOUND') AND NOT EXISTS(SELECT 1 FROM control_contexts x WHERE x.workstream_id=w.id AND x.principal_key!=?1 AND x.state='BOUND') ORDER BY w.updated_at DESC,w.id")
                .map_err(db_error)?;
            let result = statement
                .query_map(params![principal], |row| row.get(0))
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error);
            result
        })
    }
    /// Read-only catalog guard for first-use binding.  An archived record
    /// deliberately retains its exact endpoint for audit, so that thread is
    /// not a safe candidate for a second Workstream either.
    pub fn codex_candidate_binding_reason(
        &self,
        project_id: &str,
        thread_id: &str,
    ) -> Result<Option<String>, String> {
        self.with_connection(|c| {
            c.query_row(
                "SELECT w.status,w.trashed_at FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id WHERE e.provider='CODEX' AND e.status='ACTIVE' AND e.external_id=?1 AND w.project_id=?2 ORDER BY w.updated_at DESC LIMIT 1",
                params![thread_id, project_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
            )
            .optional()
            .map_err(db_error)
            .map(|existing| existing.map(|(status, trashed)| {
                if trashed.is_some() { "RECYCLED_WORKSTREAM".into() }
                else if status == "ARCHIVED" { "ARCHIVED_WORKSTREAM".into() }
                else { "ACTIVE_WORKSTREAM".into() }
            }))
        })
    }
    pub fn context_view(&self, identity: &ScopeIdentity) -> Result<ContextView, String> {
        self.with_connection(|c| {
            let Some(ctx) = context(c, identity)? else {
                return Ok(ContextView {
                    state: "UNBOUND".into(),
                    context_id: None,
                    context_revision: Some(0),
                    workstream: None,
                    binding_revision: None,
                    target: None,
                });
            };
            context_projection(c, &ctx)
        })
    }
    pub fn management_context_for_chatgpt_endpoint(
        &self,
        principal: &str,
        endpoint_id: &str,
        conversation_id: &str,
    ) -> Result<ManagementContextView, String> {
        self.with_connection(|c| {
            let (workstream_id, endpoint): (String, Endpoint) = c.query_row(
                "SELECT e.workstream_id,e.id,e.workstream_id,e.provider,e.external_id,e.label,e.status,e.replaces_endpoint_id,e.created_at,e.superseded_at
                 FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id
                 WHERE e.id=?1 AND e.provider='CHATGPT' AND e.status='ACTIVE' AND e.external_id=?2
                   AND w.status='ACTIVE' AND w.trashed_at IS NULL
                   AND EXISTS(SELECT 1 FROM control_contexts x WHERE x.workstream_id=w.id AND x.principal_key=?3 AND x.state='BOUND')
                   AND NOT EXISTS(SELECT 1 FROM control_contexts x WHERE x.workstream_id=w.id AND x.principal_key!=?3 AND x.state='BOUND')",
                params![endpoint_id,conversation_id,principal],
                |r| Ok((r.get(0)?, Endpoint{id:r.get(1)?,workstream_id:r.get(2)?,provider:r.get(3)?,external_id:r.get(4)?,label:r.get(5)?,status:r.get(6)?,replaces_endpoint_id:r.get(7)?,created_at:r.get(8)?,superseded_at:r.get(9)?})),
            ).optional().map_err(db_error)?.ok_or("FORBIDDEN")?;
            let ctx = c.query_row(
                "SELECT id,principal_key,state,revision,workstream_id FROM control_contexts WHERE workstream_id=?1 AND principal_key=?2 AND state='BOUND'",
                params![workstream_id, principal],
                |r| Ok(ContextRecord{id:r.get(0)?,principal_key:r.get(1)?,state:r.get(2)?,revision:r.get(3)?,workstream_id:r.get(4)?}),
            ).optional().map_err(db_error)?.ok_or("FORBIDDEN")?;
            let codex = active_endpoint(c, &workstream_id, "CODEX")?.ok_or("FORBIDDEN")?;
            Ok(ManagementContextView { context: context_projection(c,&ctx)?, chatgpt_endpoint: endpoint, codex_endpoint: codex })
        })
    }
    pub fn management_active_chatgpt_endpoints(
        &self,
        principal: &str,
    ) -> Result<Vec<Endpoint>, String> {
        self.with_connection(|c| {
            c.prepare("SELECT e.id,e.workstream_id,e.provider,e.external_id,e.label,e.status,e.replaces_endpoint_id,e.created_at,e.superseded_at FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id WHERE e.provider='CHATGPT' AND e.status='ACTIVE' AND w.status='ACTIVE' AND w.trashed_at IS NULL AND EXISTS(SELECT 1 FROM control_contexts x WHERE x.workstream_id=w.id AND x.principal_key=?1 AND x.state='BOUND') AND NOT EXISTS(SELECT 1 FROM control_contexts x WHERE x.workstream_id=w.id AND x.principal_key!=?1 AND x.state='BOUND') ORDER BY e.created_at")
                .map_err(db_error)?
                .query_map(params![principal], |r| Ok(Endpoint{id:r.get(0)?,workstream_id:r.get(1)?,provider:r.get(2)?,external_id:r.get(3)?,label:r.get(4)?,status:r.get(5)?,replaces_endpoint_id:r.get(6)?,created_at:r.get(7)?,superseded_at:r.get(8)?}))
                .map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)
        })
    }
    pub fn management_cursor(&self, endpoint_id: &str) -> Result<Option<(String, u64)>, String> {
        self.with_connection(|c| c.query_row("SELECT stream_epoch,sequence FROM management_bridge_cursors WHERE chatgpt_endpoint_id=?1",params![endpoint_id],|r|Ok((r.get(0)?,r.get::<_,i64>(1)? as u64))).optional().map_err(db_error))
    }
    pub fn save_management_cursor(
        &self,
        endpoint_id: &str,
        epoch: &str,
        sequence: u64,
    ) -> Result<(), String> {
        self.with_connection(|c| { c.execute("INSERT INTO management_bridge_cursors(chatgpt_endpoint_id,stream_epoch,sequence,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(chatgpt_endpoint_id) DO UPDATE SET stream_epoch=excluded.stream_epoch,sequence=excluded.sequence,updated_at=excluded.updated_at",params![endpoint_id,epoch,sequence as i64,now()]).map_err(db_error)?; Ok(()) })
    }
    pub fn observe_management_call(
        &self,
        call: &ManagementObservedCall<'_>,
    ) -> Result<ManagementCallRecord, String> {
        self.with_connection(|c| {
            let id=Uuid::new_v4().to_string();
            let changed=c.execute("INSERT INTO management_bridge_calls(id,owner_principal_key,workstream_id,chatgpt_endpoint_id,conversation_id,source_client_id,source_message_id,source_turn_key,source_user_turn_key,request_id,operation,request_hash,state,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'OBSERVED',?13,?13) ON CONFLICT(chatgpt_endpoint_id,source_message_id) DO NOTHING",params![id,call.owner,call.workstream_id,call.endpoint_id,call.conversation_id,call.source_client_id,call.source_message_id,call.source_turn_key,call.source_user_turn_key,call.request_id,call.operation,call.request_hash,now()]).map_err(db_error)?;
            if changed == 1 { return Ok(ManagementCallRecord{id,request_id:call.request_id.into(),state:"OBSERVED".into(),result_json:None,result_user_turn_key:None}); }
            c.query_row("SELECT id,request_id,state,result_json,result_user_turn_key FROM management_bridge_calls WHERE chatgpt_endpoint_id=?1 AND source_message_id=?2",params![call.endpoint_id,call.source_message_id],|r|Ok(ManagementCallRecord{id:r.get(0)?,request_id:r.get(1)?,state:r.get(2)?,result_json:r.get(3)?,result_user_turn_key:r.get(4)?})).map_err(db_error)
        })
    }
    pub fn management_request_is_stale(
        &self,
        endpoint_id: &str,
        request_id: &str,
    ) -> Result<bool, String> {
        self.with_connection(|c| c.query_row("SELECT EXISTS(SELECT 1 FROM management_bridge_stale_requests WHERE chatgpt_endpoint_id=?1 AND request_id=?2)",params![endpoint_id,request_id],|r|r.get(0)).map_err(db_error))
    }
    pub fn record_management_stale_request(
        &self,
        endpoint_id: &str,
        conversation_id: &str,
        request_id: &str,
    ) -> Result<(), String> {
        uuid::Uuid::parse_str(request_id).map_err(|_| "MANAGEMENT_STALE_REQUEST_INVALID")?;
        self.with_connection(|c| { c.execute("INSERT INTO management_bridge_stale_requests(chatgpt_endpoint_id,conversation_id,request_id,reason,recorded_at) VALUES(?1,?2,?3,'HISTORY_UNOBSERVABLE',?4) ON CONFLICT(chatgpt_endpoint_id,request_id) DO NOTHING",params![endpoint_id,conversation_id,request_id,now()]).map_err(db_error)?; Ok(()) })
    }
    pub fn save_management_result(&self, id: &str, result_json: &str) -> Result<(), String> {
        self.with_connection(|c| { if c.execute("UPDATE management_bridge_calls SET state='EXECUTED',result_json=?2,updated_at=?3 WHERE id=?1 AND state='OBSERVED'",params![id,result_json,now()]).map_err(db_error)? != 1 { return Err("MANAGEMENT_CALL_STATE_INVALID".into()); } Ok(()) })
    }
    pub fn mark_management_returning(&self, id: &str) -> Result<(), String> {
        self.with_connection(|c| { if c.execute("UPDATE management_bridge_calls SET state='RETURNING',updated_at=?2 WHERE id=?1 AND state='EXECUTED'",params![id,now()]).map_err(db_error)? != 1 { return Err("MANAGEMENT_CALL_STATE_INVALID".into()); } Ok(()) })
    }
    pub fn finish_management_return(
        &self,
        id: &str,
        state: &str,
        user_turn_key: Option<&str>,
    ) -> Result<(), String> {
        if !matches!(state, "RETURNED" | "WAITING_SAFE_RETURN" | "UNKNOWN") {
            return Err("MANAGEMENT_CALL_STATE_INVALID".into());
        }
        self.with_connection(|c| { if c.execute("UPDATE management_bridge_calls SET state=?2,result_user_turn_key=?3,updated_at=?4 WHERE id=?1 AND state='RETURNING'",params![id,state,user_turn_key,now()]).map_err(db_error)? != 1 { return Err("MANAGEMENT_CALL_STATE_INVALID".into()); } Ok(()) })
    }
    pub fn management_return_user_turn_is_known(
        &self,
        endpoint_id: &str,
        user_turn_key: &str,
    ) -> Result<bool, String> {
        self.with_connection(|c| c.query_row("SELECT EXISTS(SELECT 1 FROM management_bridge_calls WHERE chatgpt_endpoint_id=?1 AND result_user_turn_key=?2)",params![endpoint_id,user_turn_key],|r|r.get(0)).map_err(db_error))
    }
    pub fn mark_management_consumed(
        &self,
        endpoint_id: &str,
        user_turn_key: &str,
        message_id: &str,
    ) -> Result<(), String> {
        self.with_connection(|c| { c.execute("UPDATE management_bridge_calls SET state='CONSUMED',consumed_message_id=?3,updated_at=?4 WHERE chatgpt_endpoint_id=?1 AND result_user_turn_key=?2 AND state='RETURNED'",params![endpoint_id,user_turn_key,message_id,now()]).map_err(db_error)?; Ok(()) })
    }
    /// Registry only, never scans the filesystem. Cursor integrity is checked by
    /// the presentation layer before passing its last registered project ID.
    pub fn registered_projects(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ProjectView>, String> {
        if !(1..=51).contains(&limit) {
            return Err("INVALID_LIMIT".into());
        }
        self.with_connection(|c| {
            let mut statement=c.prepare("SELECT p.id,p.name,r.canonical_path,r.revision FROM projects p JOIN project_execution_roots r ON r.project_id=p.id WHERE (?1 IS NULL OR p.id>?1) ORDER BY p.id LIMIT ?2").map_err(db_error)?;
            let result=statement.query_map(params![after,limit as i64],|r| {
                let path:String=r.get(2)?;let name:String=r.get(1)?;
                Ok(ProjectView{id:r.get(0)?,label:name.chars().take(200).collect(),root_label:Path::new(&path).file_name().and_then(|s|s.to_str()).unwrap_or("Registered root").chars().take(200).collect(),root_revision:r.get(3)?})
            }).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;Ok(result)
        })
    }
    pub fn registered_root(&self, project: &str) -> Result<RegisteredRoot, String> {
        self.with_connection(|c|c.query_row("SELECT canonical_path,revision,policy_json FROM project_execution_roots WHERE project_id=?1",params![project],|r| {
            let raw:String=r.get(2)?;let policy=serde_json::from_str(&raw).map_err(|e|rusqlite::Error::FromSqlConversionFailure(2,rusqlite::types::Type::Text,Box::new(e)))?;
            Ok(RegisteredRoot{canonical_path:r.get(0)?,root_revision:r.get(1)?,policy})
        }).optional().map_err(db_error)?.ok_or("NOT_FOUND".into()))
    }
    pub fn scoped_run(&self, identity: &ScopeIdentity, run_id: &str) -> Result<RunView, String> {
        self.with_connection(|c| run(c, identity, run_id))
    }
    pub fn scoped_handoff_run(
        &self,
        identity: &ScopeIdentity,
        handoff: &str,
    ) -> Result<Option<RunView>, String> {
        self.with_connection(|c| {
            let ctx=owned_context(c,identity)?;
            let id:Option<String>=c.query_row("SELECT p.id FROM handoffs h LEFT JOIN provider_runs p ON p.origin_handoff_id=h.id WHERE h.id=?1 AND h.source_context_id=?2 AND h.workstream_id=?3",params![handoff,ctx.id,ctx.workstream_id],|r|r.get(0)).optional().map_err(db_error)?.ok_or("NOT_FOUND")?;
            id.map(|id|run(c,identity,&id)).transpose()
        })
    }
    pub fn scoped_result_chunk(
        &self,
        identity: &ScopeIdentity,
        run_id: &str,
        offset: usize,
        max_bytes: usize,
        expected_hash: Option<&str>,
    ) -> Result<ResultChunk, String> {
        if !(1024..=16384).contains(&max_bytes) || offset > 1024 * 1024 {
            return Err("CURSOR_INVALID".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Deferred).map_err(db_error)?;
            let r=run(&tx,identity,run_id)?;
            let code=match r.result_state.as_str(){"PENDING"=>Some("RESULT_NOT_READY"),"NOT_RETAINED"=>Some("RESULT_NOT_RETAINED"),"TOO_LARGE"=>Some("RESULT_TOO_LARGE"),"NOT_APPLICABLE"=>Some("RESULT_NOT_APPLICABLE"),"UNAVAILABLE"=>Some("RESULT_UNAVAILABLE"),"AVAILABLE"=>None,_=>Some("INTERNAL")};
            if let Some(code)=code {return Err(code.into());}
            let (hash,item,receipt,total,bytes):(String,String,String,i64,Vec<u8>)=tx.query_row("SELECT d.result_hash,d.final_item_id,d.terminal_evidence_id,d.result_bytes,substr(CAST(p.result_text AS BLOB),?2,?3) FROM provider_runs p JOIN provider_result_details d ON d.run_id=p.id WHERE p.id=?1",params![run_id,offset as i64+1,max_bytes as i64],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(db_error)?;
            if total<=0 || total>1024*1024 || offset>=total as usize || expected_hash.is_some_and(|h|h!=hash) {return Err("CURSOR_INVALID".into());}
            let text=match std::str::from_utf8(&bytes) {Ok(s)=>s.to_owned(),Err(e) if e.error_len().is_none() && e.valid_up_to()>0=>std::str::from_utf8(&bytes[..e.valid_up_to()]).map_err(|_|"RESULT_UNAVAILABLE")?.to_owned(),Err(_)=>return Err("CURSOR_INVALID".into())};
            let next=offset+text.len();let turn=r.turn_id.ok_or("RESULT_UNAVAILABLE")?;
            tx.commit().map_err(db_error)?;
            Ok(ResultChunk{run_id:run_id.into(),thread_id:r.thread_id,turn_id:turn,result_identity:item,result_hash:hash,text,next_offset:if next<total as usize{Some(next)}else{None},receipt_id:receipt})
        })
    }
    /// Read a retained result owned by an endpoint-source Relay approval.
    /// This deliberately authenticates through `owned_run` rather than
    /// manufacturing a Control Context scope for an endpoint handoff.
    pub fn browser_owned_result_chunk(
        &self,
        principal: &str,
        run_id: &str,
        offset: usize,
        max_bytes: usize,
        expected_hash: Option<&str>,
    ) -> Result<ResultChunk, String> {
        if !(1024..=16384).contains(&max_bytes) || offset > 1024 * 1024 {
            return Err("CURSOR_INVALID".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Deferred).map_err(db_error)?;
            let r=owned_run(&tx,principal,run_id)?;
            let code=match r.result_state.as_str(){"PENDING"=>Some("RESULT_NOT_READY"),"NOT_RETAINED"=>Some("RESULT_NOT_RETAINED"),"TOO_LARGE"=>Some("RESULT_TOO_LARGE"),"NOT_APPLICABLE"=>Some("RESULT_NOT_APPLICABLE"),"UNAVAILABLE"=>Some("RESULT_UNAVAILABLE"),"AVAILABLE"=>None,_=>Some("INTERNAL")};
            if let Some(code)=code {return Err(code.into());}
            let (hash,item,receipt,total,bytes):(String,String,String,i64,Vec<u8>)=tx.query_row("SELECT d.result_hash,d.final_item_id,d.terminal_evidence_id,d.result_bytes,substr(CAST(p.result_text AS BLOB),?2,?3) FROM provider_runs p JOIN provider_result_details d ON d.run_id=p.id WHERE p.id=?1",params![run_id,offset as i64+1,max_bytes as i64],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(db_error)?;
            if total<=0 || total>1024*1024 || offset>=total as usize || expected_hash.is_some_and(|h|h!=hash) {return Err("CURSOR_INVALID".into());}
            let text=match std::str::from_utf8(&bytes) {Ok(s)=>s.to_owned(),Err(e) if e.error_len().is_none() && e.valid_up_to()>0=>std::str::from_utf8(&bytes[..e.valid_up_to()]).map_err(|_|"RESULT_UNAVAILABLE")?.to_owned(),Err(_)=>return Err("CURSOR_INVALID".into())};
            let next=offset+text.len(); let turn=r.turn_id.ok_or("RESULT_UNAVAILABLE")?;
            tx.commit().map_err(db_error)?;
            Ok(ResultChunk{run_id:run_id.into(),thread_id:r.thread_id,turn_id:turn,result_identity:item,result_hash:hash,text,next_offset:if next<total as usize{Some(next)}else{None},receipt_id:receipt})
        })
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
