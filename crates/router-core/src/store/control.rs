//! ControlContext application transactions. No native I/O is allowed in this module.
use super::*;
use crate::identity::{length_prefixed_hash, ScopeIdentity};
use serde::Deserialize;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PolicySnapshot {
    pub version: u32,
    pub backend_id: String,
    pub native_version: String,
    pub model: String,
    pub permission_mode: PermissionMode,
    pub approval_mode: ApprovalMode,
    pub network_access: bool,
    pub root_identity_hash: String,
    pub config_digest: String,
    /// Binds a root to the exact Router-owned launch overlay as well as the
    /// immutable shared config input. `None` is retained only for historic
    /// snapshots written before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_launch_digest: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PermissionMode {
    ReadOnly,
    WorkspaceWrite,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalMode {
    OnRequest,
    Never,
}
impl PolicySnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || Uuid::parse_str(&self.backend_id).is_err()
            || self.native_version != "0.154.0"
            || self.model.trim().is_empty()
            || self.model.len() > 100
            || self.network_access
            || !valid_hash(&self.root_identity_hash)
            || !valid_hash(&self.config_digest)
            || self
                .native_launch_digest
                .as_deref()
                .is_some_and(|digest| !valid_hash(digest))
        {
            return Err("POLICY_CHANGED".into());
        }
        Ok(())
    }
    pub fn hash(&self) -> Result<String, String> {
        self.validate()?;
        Ok(length_prefixed_hash(&[
            b"policy-v1",
            serde_json::to_string(self)
                .map_err(|_| "INTERNAL")?
                .as_bytes(),
        ]))
    }
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Clone, Debug, Serialize)]
pub struct ContextRecord {
    pub id: String,
    pub principal_key: String,
    pub state: String,
    pub revision: i64,
    pub workstream_id: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct DraftRecord {
    pub handoff_id: String,
    pub draft_revision: i64,
    pub payload_hash: String,
    pub state: String,
    pub run_id: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ReviewSnapshot {
    pub draft: DraftRecord,
    pub context_id: String,
    pub context_revision: i64,
    pub binding_revision: i64,
    pub root_revision: i64,
    pub workstream_id: String,
    pub endpoint_id: String,
    pub thread_id: String,
    pub canonical_path: String,
    pub policy: PolicySnapshot,
    pub text: String,
}
/// Created by the browser session/challenge verifier, never deserialized from
/// a model or browser JSON body. Scope/revisions/hash come from its server snapshot.
#[derive(Clone, Debug)]
pub struct ApprovalGrant {
    pub(crate) principal_key: String,
    pub(crate) snapshot: ReviewSnapshot,
    pub(crate) nonce_hash: String,
    pub(crate) expires_at: i64,
}
impl ApprovalGrant {
    pub fn snapshot(&self) -> &ReviewSnapshot {
        &self.snapshot
    }
    pub fn from_verified_browser(
        principal_key: String,
        snapshot: ReviewSnapshot,
        nonce_hash: String,
        expires_at: i64,
    ) -> Result<Self, String> {
        if !valid_hash(&principal_key) || !valid_hash(&nonce_hash) {
            return Err("FORBIDDEN".into());
        }
        Ok(Self {
            principal_key,
            snapshot,
            nonce_hash,
            expires_at,
        })
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ClaimedRun {
    pub run_id: String,
    pub handoff_id: String,
    pub dispatch_id: String,
    pub newly_claimed: bool,
}

fn context(conn: &Connection, identity: &ScopeIdentity) -> Result<Option<ContextRecord>, String> {
    conn.query_row("SELECT id,principal_key,state,revision,workstream_id FROM control_contexts WHERE (host_scope_id=?1 AND ?2>=2) OR (host_scope_id IS NULL AND principal_key=?3 AND scope_key=?4 AND key_version=?2)",params![identity.scope_key,identity.key_version,identity.principal_key,identity.scope_key],|r|Ok(ContextRecord{id:r.get(0)?,principal_key:r.get(1)?,state:r.get(2)?,revision:r.get(3)?,workstream_id:r.get(4)?})).optional().map_err(db_error)
}
pub(super) fn ensure_no_dispatch(conn: &Connection, ws: &str) -> Result<(), String> {
    let busy:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM handoff_dispatches WHERE workstream_id=?1 AND phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN'))",params![ws],|r|r.get(0)).map_err(db_error)?;
    if busy {
        return Err("RUN_IN_FLIGHT".into());
    }
    let legacy:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM provider_runs WHERE workstream_id=?1 AND status IN ('STARTING','RUNNING','UNKNOWN'))",params![ws],|r|r.get(0)).map_err(db_error)?;
    if legacy {
        return Err("RUN_IN_FLIGHT".into());
    }
    Ok(())
}
fn normalized_text(text: &str) -> Result<String, String> {
    let text = text.replace("\r\n", "\n");
    if text.trim().is_empty() || text.len() > 32768 {
        return Err("Invalid instruction text".into());
    }
    Ok(text)
}
fn binding(conn: &Connection, ctx: &ContextRecord) -> Result<ReviewSnapshot, String> {
    if ctx.state != "BOUND" {
        return Err(if ctx.state == "REVOKED" {
            "NOT_FOUND"
        } else {
            "UNBOUND"
        }
        .into());
    }
    conn.query_row("SELECT w.id,w.binding_revision,e.id,e.external_id,r.canonical_path,r.revision,r.policy_json FROM workstreams w JOIN endpoints e ON e.workstream_id=w.id AND e.provider='CODEX' AND e.status='ACTIVE' JOIN project_execution_roots r ON r.project_id=w.project_id WHERE w.id=?1 AND w.status='ACTIVE' AND w.trashed_at IS NULL",params![ctx.workstream_id],|r| {
        let raw:String=r.get(6)?;
        let policy:PolicySnapshot=serde_json::from_str(&raw).map_err(|e|rusqlite::Error::FromSqlConversionFailure(6,rusqlite::types::Type::Text,Box::new(e)))?;
        Ok(ReviewSnapshot{draft:DraftRecord{handoff_id:String::new(),draft_revision:1,payload_hash:String::new(),state:"READY".into(),run_id:None},context_id:ctx.id.clone(),context_revision:ctx.revision,binding_revision:r.get(1)?,root_revision:r.get(5)?,workstream_id:r.get(0)?,endpoint_id:r.get(2)?,thread_id:r.get(3)?,canonical_path:r.get(4)?,policy,text:String::new()})
    }).optional().map_err(db_error)?.ok_or("UNBOUND".into())
}
fn requires_binding_revalidation(conn: &Connection, s: &ReviewSnapshot) -> Result<bool, String> {
    let current = s.policy.hash()?;
    let latest:Option<(i64,String)>=conn.query_row("SELECT root_revision,candidate_policy_hash FROM binding_requests WHERE context_id=?1 AND workstream_id=?2 AND state='APPLIED' ORDER BY created_at DESC LIMIT 1",params![s.context_id,s.workstream_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
    Ok(matches!(latest, Some((revision, hash)) if revision != s.root_revision || hash != current))
}
fn payload_hash(principal: &str, s: &ReviewSnapshot) -> Result<String, String> {
    Ok(length_prefixed_hash(&[
        b"instruction-v1",
        s.text.as_bytes(),
        principal.as_bytes(),
        s.context_id.as_bytes(),
        &s.context_revision.to_be_bytes(),
        s.workstream_id.as_bytes(),
        &s.binding_revision.to_be_bytes(),
        s.endpoint_id.as_bytes(),
        s.thread_id.as_bytes(),
        &s.root_revision.to_be_bytes(),
        s.policy.hash()?.as_bytes(),
    ]))
}
pub(super) fn snapshot(
    conn: &Connection,
    principal: &str,
    handoff_id: &str,
) -> Result<ReviewSnapshot, String> {
    let ctx=conn.query_row("SELECT c.id,c.principal_key,c.state,c.revision,c.workstream_id FROM control_contexts c JOIN handoffs h ON h.source_context_id=c.id WHERE h.id=?1 AND c.principal_key=?2 AND c.state='BOUND'",params![handoff_id,principal],|r|Ok(ContextRecord{id:r.get(0)?,principal_key:r.get(1)?,state:r.get(2)?,revision:r.get(3)?,workstream_id:r.get(4)?})).optional().map_err(db_error)?.ok_or("NOT_FOUND")?;
    let mut s = binding(conn, &ctx)?;
    let (draft,body,cr,br,rr,policy_hash,ws,ep):(DraftRecord,String,i64,i64,i64,String,String,String)=conn.query_row("SELECT h.id,d.draft_revision,h.payload_hash,h.status,x.run_id,h.approved_text,d.context_revision,d.binding_revision,d.root_revision,d.policy_hash,h.workstream_id,h.destination_endpoint_id FROM handoffs h JOIN control_handoff_details d ON d.handoff_id=h.id AND d.context_id=h.source_context_id LEFT JOIN handoff_dispatches x ON x.handoff_id=h.id WHERE h.id=?1",params![handoff_id],|r|Ok((DraftRecord{handoff_id:r.get(0)?,draft_revision:r.get(1)?,payload_hash:r.get(2)?,state:r.get(3)?,run_id:r.get(4)?},r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?,r.get(11)?))).map_err(db_error)?;
    if cr != s.context_revision {
        return Err("STALE_CONTEXT".into());
    }
    if br != s.binding_revision || ws != s.workstream_id || ep != s.endpoint_id {
        return Err("STALE_BINDING".into());
    }
    if rr != s.root_revision {
        return Err("ROOT_CHANGED".into());
    }
    if policy_hash != s.policy.hash()? {
        return Err("POLICY_CHANGED".into());
    }
    s.draft = draft;
    s.text = body;
    if payload_hash(principal, &s)? != s.draft.payload_hash {
        // A credential rotation changes the private principal seed. Preserve
        // the immutable original payload hash only when offline maintenance
        // recorded an exact continuity marker for this replacement owner.
        let continuity: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM private_owner_rotation_continuity WHERE handoff_id=?1 AND payload_hash=?2 AND principal_key=?3)",
            params![handoff_id, s.draft.payload_hash, principal],
            |row| row.get(0),
        ).map_err(db_error)?;
        if !continuity {
            return Err("INTERNAL".into());
        }
    }
    Ok(s)
}

impl RouterStore {
    pub fn control_context(
        &self,
        identity: &ScopeIdentity,
    ) -> Result<Option<ContextRecord>, String> {
        self.with_connection(|c| context(c, identity))
    }
    pub fn ensure_unbound_context(
        &self,
        identity: &ScopeIdentity,
    ) -> Result<ContextRecord, String> {
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            if let Some(existing)=context(&tx,identity)? {return Ok(existing);}
            let identifier=id();let timestamp=now();
            let host_scope = (identity.key_version >= 2).then_some(identity.scope_key.as_str());
            tx.execute("INSERT INTO control_contexts(id,principal_key,scope_key,key_version,host_scope_id,state,revision,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,'UNBOUND',0,?6,?6)",params![identifier,identity.principal_key,identity.scope_key,identity.key_version,host_scope,timestamp]).map_err(db_error)?;
            let created=context(&tx,identity)?.ok_or("INTERNAL")?;tx.commit().map_err(db_error)?;Ok(created)
        })
    }
    pub fn prepare_instruction(
        &self,
        identity: &ScopeIdentity,
        client_request_id: &str,
        expected_context_revision: i64,
        expected_binding_revision: i64,
        text: &str,
    ) -> Result<DraftRecord, String> {
        Uuid::parse_str(client_request_id).map_err(|_| "Invalid request ID")?;
        let text = normalized_text(text)?;
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let ctx=context(&tx,identity)?.ok_or("UNBOUND")?;
            if ctx.revision!=expected_context_revision {return Err("STALE_CONTEXT".into());}
            let mut s=binding(&tx,&ctx)?;
            if s.binding_revision!=expected_binding_revision {return Err("STALE_BINDING".into());}
            if requires_binding_revalidation(&tx,&s)? { return Err("BINDING_REVALIDATION_REQUIRED".into()); }
            s.text=text.clone();let request_hash=payload_hash(&identity.principal_key,&s)?;
            if let Some((existing,hash))=tx.query_row("SELECT handoff_id,request_hash FROM control_handoff_details WHERE context_id=?1 AND client_request_id=?2",params![ctx.id,client_request_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional().map_err(db_error)? {
                if hash!=request_hash {return Err("IDEMPOTENCY_CONFLICT".into());}
                return Ok(snapshot(&tx,&identity.principal_key,&existing)?.draft);
            }
            ensure_no_dispatch(&tx,&s.workstream_id)?;
            let (per_context,per_owner):(i64,i64)=tx.query_row("SELECT sum(CASE WHEN h.source_context_id=?1 THEN 1 ELSE 0 END),count(*) FROM handoffs h JOIN control_contexts c ON c.id=h.source_context_id WHERE h.source_kind='CONTROL_CONTEXT' AND h.status='READY' AND c.principal_key=?2",params![ctx.id,identity.principal_key],|r|Ok((r.get::<_,Option<i64>>(0)?.unwrap_or(0),r.get(1)?))).map_err(db_error)?;
            if per_context>=20 || per_owner>=100 {return Err("RATE_LIMITED".into());}
            s.draft.handoff_id=id();s.draft.payload_hash=request_hash.clone();let timestamp=now();
            tx.execute("INSERT INTO handoffs(id,workstream_id,source_kind,source_context_id,destination_endpoint_id,direction,original_text,approved_text,status,payload_hash,created_at) VALUES(?1,?2,'CONTROL_CONTEXT',?3,?4,'CONTROL_TO_CODEX',?5,?5,'READY',?6,?7)",params![s.draft.handoff_id,s.workstream_id,ctx.id,s.endpoint_id,s.text,request_hash,timestamp]).map_err(db_error)?;
            tx.execute("INSERT INTO control_handoff_details(handoff_id,context_id,client_request_id,request_hash,context_revision,binding_revision,root_revision,draft_revision,policy_json,policy_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,1,?8,?9)",params![s.draft.handoff_id,ctx.id,client_request_id,request_hash,ctx.revision,s.binding_revision,s.root_revision,serde_json::to_string(&s.policy).map_err(|_|"INTERNAL")?,s.policy.hash()?]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;Ok(s.draft)
        })
    }
    pub fn review_snapshot(
        &self,
        principal: &str,
        handoff_id: &str,
    ) -> Result<ReviewSnapshot, String> {
        self.with_connection(|c| snapshot(c, principal, handoff_id))
    }
    pub fn scoped_draft(
        &self,
        identity: &ScopeIdentity,
        handoff_id: &str,
    ) -> Result<DraftRecord, String> {
        self.with_connection(|c| {
            let ctx = context(c, identity)?.ok_or("NOT_FOUND")?;
            let s = snapshot(c, &identity.principal_key, handoff_id)?;
            if s.context_id != ctx.id {
                return Err("NOT_FOUND".into());
            }
            Ok(s.draft)
        })
    }
    pub fn edit_instruction(
        &self,
        identity: &ScopeIdentity,
        handoff_id: &str,
        expected_draft_revision: i64,
        text: &str,
    ) -> Result<DraftRecord, String> {
        let text = normalized_text(text)?;
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let ctx=context(&tx,identity)?.ok_or("NOT_FOUND")?;
            let mut s=snapshot(&tx,&identity.principal_key,handoff_id)?;
            if ctx.id!=s.context_id {return Err("NOT_FOUND".into());}
            if s.draft.state!="READY" || s.draft.draft_revision!=expected_draft_revision {return Err("STALE_DRAFT".into());}
            s.text=text.clone();s.draft.payload_hash=payload_hash(&identity.principal_key,&s)?;s.draft.draft_revision+=1;
            tx.execute("UPDATE handoffs SET approved_text=?2,payload_hash=?3 WHERE id=?1",params![handoff_id,s.text,s.draft.payload_hash]).map_err(db_error)?;
            tx.execute("UPDATE control_handoff_details SET draft_revision=?2,review_nonce_hash=NULL,approval_expires_at=NULL,approved_principal_key=NULL,approved_channel=NULL WHERE handoff_id=?1",params![handoff_id,s.draft.draft_revision]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;Ok(s.draft)
        })
    }
    pub fn approve_and_claim(
        &self,
        grant: &ApprovalGrant,
        adapter_epoch: &str,
    ) -> Result<ClaimedRun, String> {
        Uuid::parse_str(adapter_epoch).map_err(|_| "INTERNAL")?;
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let s=snapshot(&tx,&grant.principal_key,&grant.snapshot.draft.handoff_id)?;
            let nonce:Option<String>=tx.query_row("SELECT review_nonce_hash FROM control_handoff_details WHERE handoff_id=?1",params![s.draft.handoff_id],|r|r.get(0)).map_err(db_error)?;
            if nonce.as_deref()==Some(&grant.nonce_hash) && s.draft.run_id.is_some() && s.draft.payload_hash==grant.snapshot.draft.payload_hash {
                return tx.query_row("SELECT run_id,handoff_id,dispatch_id FROM handoff_dispatches WHERE handoff_id=?1",params![s.draft.handoff_id],|r|Ok(ClaimedRun{run_id:r.get(0)?,handoff_id:r.get(1)?,dispatch_id:r.get(2)?,newly_claimed:false})).map_err(db_error);
            }
            if grant.expires_at<now() {return Err("EXPIRED".into());}
            if s.context_id!=grant.snapshot.context_id || s.context_revision!=grant.snapshot.context_revision {return Err("STALE_CONTEXT".into());}
            if s.draft.state!="READY" || s.draft.draft_revision!=grant.snapshot.draft.draft_revision || s.draft.payload_hash!=grant.snapshot.draft.payload_hash {return Err("STALE_DRAFT".into());}
            ensure_no_dispatch(&tx,&s.workstream_id)?;
            let exiting:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM runtime_exit_state WHERE phase!='RUNNING')",[],|r|r.get(0)).map_err(db_error)?;
            if exiting {return Err("EXIT_PENDING".into());}
            let timestamp=now();let run=id();let dispatch=id();
            tx.execute("UPDATE handoffs SET status='APPROVED',approved_at=?2 WHERE id=?1",params![s.draft.handoff_id,timestamp]).map_err(db_error)?;
            tx.execute("UPDATE control_handoff_details SET approved_principal_key=?2,approved_channel='BROWSER_REVIEW',review_nonce_hash=?3,approval_expires_at=?4,consumed_at=?5 WHERE handoff_id=?1",params![s.draft.handoff_id,grant.principal_key,grant.nonce_hash,grant.expires_at,timestamp]).map_err(db_error)?;
            tx.execute("INSERT INTO provider_runs(id,workstream_id,endpoint_id,provider,origin_handoff_id,status,created_at,updated_at) VALUES(?1,?2,?3,'CODEX',?4,'STARTING',?5,?5)",params![run,s.workstream_id,s.endpoint_id,s.draft.handoff_id,timestamp]).map_err(db_error)?;
            tx.execute("INSERT INTO handoff_dispatches(handoff_id,run_id,workstream_id,endpoint_id,dispatch_id,adapter_epoch,phase,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,'CLAIMED',?7,?7)",params![s.draft.handoff_id,run,s.workstream_id,s.endpoint_id,dispatch,adapter_epoch,timestamp]).map_err(db_error)?;
            tx.execute("INSERT INTO provider_result_details(run_id,result_state,updated_at) VALUES(?1,'PENDING',?2)",params![run,timestamp]).map_err(db_error)?;
            tx.execute("UPDATE handoffs SET status='SENDING' WHERE id=?1",params![s.draft.handoff_id]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;
            Ok(ClaimedRun{run_id:run,handoff_id:s.draft.handoff_id,dispatch_id:dispatch,newly_claimed:true})
        })
    }
}
pub mod binding;
pub mod views;
