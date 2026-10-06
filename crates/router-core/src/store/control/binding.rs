use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BindingOperation {
    Select,
    Create,
    Replace,
    Takeover,
    Unbind,
}
impl BindingOperation {
    fn wire(&self) -> &'static str {
        match self {
            Self::Select => "SELECT",
            Self::Create => "CREATE",
            Self::Replace => "REPLACE",
            Self::Takeover => "TAKEOVER",
            Self::Unbind => "UNBIND",
        }
    }
}
/// Native IDs here are decoded server candidate tickets, never model path/ID fields.
pub struct BindingProposal {
    pub client_request_id: String,
    pub operation: BindingOperation,
    pub expected_context_revision: i64,
    pub project_id: Option<String>,
    pub candidate_thread_id: Option<String>,
    pub workstream_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct BindingReview {
    pub id: String,
    pub context_id: String,
    pub operation: BindingOperation,
    pub state: String,
    pub project_id: Option<String>,
    pub workstream_id: Option<String>,
    pub expected_context_revision: i64,
    pub expected_binding_revision: Option<i64>,
    pub root_revision: Option<i64>,
    pub candidate_thread_id: Option<String>,
    pub policy_hash: Option<String>,
    pub expires_at: i64,
}
#[derive(Clone)]
pub struct BindingGrant {
    pub principal_key: String,
    pub review: BindingReview,
    pub nonce_hash: String,
}
/// Produced by exact native validation, not deserializable from HTTP input.
pub struct VerifiedBindingTarget {
    pub thread_id: String,
    pub policy_hash: String,
    pub root_revision: i64,
    pub durable: bool,
}
fn review(conn: &Connection, principal: &str, request_id: &str) -> Result<BindingReview, String> {
    conn.query_row("SELECT b.id,b.context_id,b.operation,b.state,b.project_id,b.workstream_id,b.expected_context_revision,b.expected_binding_revision,b.root_revision,b.candidate_thread_id,b.candidate_policy_hash,b.expires_at FROM binding_requests b JOIN control_contexts c ON c.id=b.context_id WHERE b.id=?1 AND c.principal_key=?2 AND c.state!='REVOKED'",params![request_id,principal],|r| {
        let op:String=r.get(2)?;let operation=match op.as_str(){"SELECT"=>BindingOperation::Select,"CREATE"=>BindingOperation::Create,"REPLACE"=>BindingOperation::Replace,"TAKEOVER"=>BindingOperation::Takeover,_=>BindingOperation::Unbind};
        Ok(BindingReview{id:r.get(0)?,context_id:r.get(1)?,operation,state:r.get(3)?,project_id:r.get(4)?,workstream_id:r.get(5)?,expected_context_revision:r.get(6)?,expected_binding_revision:r.get(7)?,root_revision:r.get(8)?,candidate_thread_id:r.get(9)?,policy_hash:r.get(10)?,expires_at:r.get(11)?})
    }).optional().map_err(db_error)?.ok_or("NOT_FOUND".into())
}
/// Pure preview: no binding/Goal/native change on GET. POST recomputes this
/// exact selection inside its existing atomic binding transaction.
fn selected_review(
    c: &Connection,
    principal: &str,
    mut r: BindingReview,
    candidate: Option<&str>,
    workstream: Option<&str>,
) -> Result<BindingReview, String> {
    if candidate.is_none() && workstream.is_none() {
        return Ok(r);
    }
    if r.state != "REVIEW"
        || r.expires_at < now()
        || !matches!(
            r.operation,
            BindingOperation::Select | BindingOperation::Replace | BindingOperation::Takeover
        )
        || (candidate.is_some() && workstream.is_some())
    {
        return Err("STALE_BINDING".into());
    }
    if let Some(ws) = workstream {
        if !matches!(
            r.operation,
            BindingOperation::Select | BindingOperation::Takeover
        ) {
            return Err("STALE_BINDING".into());
        }
        let (project, rev, thread): (String,i64,String) = c.query_row(
            "SELECT w.project_id,w.binding_revision,e.external_id FROM workstreams w JOIN endpoints e ON e.workstream_id=w.id AND e.provider='CODEX' AND e.status='ACTIVE' WHERE w.id=?1 AND w.status='ACTIVE' AND w.trashed_at IS NULL AND EXISTS(SELECT 1 FROM control_contexts x WHERE x.workstream_id=w.id AND x.principal_key=?2 AND x.state='BOUND') AND NOT EXISTS(SELECT 1 FROM control_contexts x WHERE x.workstream_id=w.id AND x.principal_key!=?2 AND x.state='BOUND')",
            params![ws,principal], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(db_error)?.ok_or("NOT_FOUND")?;
        if r.project_id.as_deref() != Some(&project) {
            return Err("STALE_BINDING".into());
        }
        ensure_no_dispatch(c, ws)?;
        r.operation = BindingOperation::Takeover;
        r.workstream_id = Some(ws.into());
        r.expected_binding_revision = Some(rev);
        r.candidate_thread_id = Some(thread);
    } else if let Some(thread) = candidate {
        if !matches!(
            r.operation,
            BindingOperation::Select | BindingOperation::Replace
        ) {
            return Err("STALE_BINDING".into());
        }
        Uuid::parse_str(thread).map_err(|_| "INVALID_PARAMS")?;
        r.candidate_thread_id = Some(thread.into());
    }
    Ok(r)
}
fn owned_workstream_reconfirmation_required(
    c: &Connection,
    principal: &str,
    project: &str,
    thread: &str,
) -> Result<bool, String> {
    c.query_row(
        "SELECT EXISTS(SELECT 1 FROM workstreams w JOIN endpoints e ON e.workstream_id=w.id AND e.provider='CODEX' AND e.status='ACTIVE' WHERE w.project_id=?1 AND w.status='ACTIVE' AND w.trashed_at IS NULL AND e.external_id=?2 AND EXISTS(SELECT 1 FROM control_contexts x WHERE x.workstream_id=w.id AND x.principal_key=?3 AND x.state='BOUND') AND NOT EXISTS(SELECT 1 FROM control_contexts x WHERE x.workstream_id=w.id AND x.principal_key!=?3 AND x.state='BOUND'))",
        params![project,thread,principal],
        |row| row.get(0),
    )
    .map_err(db_error)
}
impl RouterStore {
    /// Durable one-shot metadata authorization. A repeat only reads back state;
    /// it never authorizes another native thread/start after an ambiguous ACK.
    pub fn begin_creation(&self, grant: &BindingGrant) -> Result<(BindingReview, bool), String> {
        if !valid_hash(&grant.nonce_hash) {
            return Err("FORBIDDEN".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let exiting:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM runtime_exit_state WHERE phase!='RUNNING')",[],|r|r.get(0)).map_err(db_error)?;
            if exiting {return Err("EXIT_PENDING".into());}
            let r=review(&tx,&grant.principal_key,&grant.review.id)?;
            if r.operation!=BindingOperation::Create {return Err("FORBIDDEN".into());}
            if r.state!="REVIEW" {
                let nonce:Option<String>=tx.query_row("SELECT review_nonce_hash FROM binding_requests WHERE id=?1",params![r.id],|row|row.get(0)).map_err(db_error)?;
                if nonce.as_deref()!=Some(&grant.nonce_hash) {return Err("FORBIDDEN".into());}
                return Ok((r,false));
            }
            if r.expires_at<now() {return Err("EXPIRED".into());}
            if r.project_id!=grant.review.project_id || r.root_revision!=grant.review.root_revision || r.policy_hash!=grant.review.policy_hash {return Err("ROOT_CHANGED".into());}
            let (state,revision):(String,i64)=tx.query_row("SELECT state,revision FROM control_contexts WHERE id=?1",params![r.context_id],|row|Ok((row.get(0)?,row.get(1)?))).map_err(db_error)?;
            if state!="UNBOUND" || revision!=r.expected_context_revision || revision!=grant.review.expected_context_revision {return Err("STALE_CONTEXT".into());}
            let (root,policy):(i64,String)=tx.query_row("SELECT revision,policy_hash FROM project_execution_roots WHERE project_id=?1",params![r.project_id],|row|Ok((row.get(0)?,row.get(1)?))).map_err(db_error)?;
            if Some(root)!=r.root_revision || Some(policy)!=r.policy_hash {return Err("ROOT_CHANGED".into());}
            // Creation is one-shot for this exact Review-created control
            // context.  An unresolved dispatch or a different creation must
            // remain locked in its own Workstream/context, but it cannot make
            // every other isolated Workstream impossible to create.  The
            // process-local Backend reservation still serializes live native
            // metadata work in this Router instance.
            let busy:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM binding_requests WHERE context_id=?1 AND state IN ('CREATING','VERIFYING','UNKNOWN'))",params![r.context_id],|row|row.get(0)).map_err(db_error)?;
            if busy {return Err("CREATION_BUSY".into());}
            tx.execute("UPDATE binding_requests SET state='CREATING',approved_principal_key=?2,review_nonce_hash=?3 WHERE id=?1",params![r.id,grant.principal_key,grant.nonce_hash]).map_err(db_error)?;
            let r=review(&tx,&grant.principal_key,&r.id)?;tx.commit().map_err(db_error)?;Ok((r,true))
        })
    }
    pub fn record_created_candidate(
        &self,
        grant: &BindingGrant,
        thread_id: &str,
    ) -> Result<(), String> {
        if thread_id.is_empty() || thread_id.len() > 256 {
            return Err("CAPABILITY_UNVERIFIED".into());
        }
        self.with_connection(|c| {
            let changed=c.execute("UPDATE binding_requests SET candidate_thread_id=?2,state='VERIFYING' WHERE id=?1 AND state='CREATING' AND operation='CREATE' AND approved_principal_key=?3 AND review_nonce_hash=?4 AND candidate_thread_id IS NULL",params![grant.review.id,thread_id,grant.principal_key,grant.nonce_hash]).map_err(db_error)?;
            if changed!=1 {return Err("CREATION_UNKNOWN".into());}Ok(())
        })
    }
    pub fn finish_creation_verification(
        &self,
        grant: &BindingGrant,
        target: &VerifiedBindingTarget,
    ) -> Result<(), String> {
        if !target.durable {
            return Err("NEW_THREAD_DURABILITY_UNSUPPORTED".into());
        }
        self.with_connection(|c| {
            let changed=c.execute("UPDATE binding_requests SET state='READY' WHERE id=?1 AND state='VERIFYING' AND operation='CREATE' AND approved_principal_key=?2 AND review_nonce_hash=?3 AND candidate_thread_id=?4 AND root_revision=?5 AND candidate_policy_hash=?6",params![grant.review.id,grant.principal_key,grant.nonce_hash,target.thread_id,target.root_revision,target.policy_hash]).map_err(db_error)?;
            if changed!=1 {return Err("CREATION_UNKNOWN".into());}Ok(())
        })
    }
    pub fn mark_creation_unknown(&self, grant: &BindingGrant) -> Result<(), String> {
        self.with_connection(|c| {
            c.execute("UPDATE binding_requests SET state='UNKNOWN' WHERE id=?1 AND state IN ('CREATING','VERIFYING') AND approved_principal_key=?2 AND review_nonce_hash=?3",params![grant.review.id,grant.principal_key,grant.nonce_hash]).map_err(db_error)?;Ok(())
        })
    }
    /// Reopens verification of the one durable candidate created by this
    /// exact Review.  It deliberately cannot return to CREATING and therefore
    /// cannot authorize another `thread/start` after an ambiguous attempt.
    pub fn begin_creation_recovery(&self, grant: &BindingGrant) -> Result<BindingReview, String> {
        if !valid_hash(&grant.nonce_hash) {
            return Err("FORBIDDEN".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let r=review(&tx,&grant.principal_key,&grant.review.id)?;
            if r.operation!=BindingOperation::Create
                || r.state!="UNKNOWN"
                || r.candidate_thread_id.as_deref().filter(|v|!v.is_empty()).is_none()
                || r.project_id!=grant.review.project_id
                || r.root_revision!=grant.review.root_revision
                || r.policy_hash!=grant.review.policy_hash
            { return Err("CREATION_UNKNOWN".into()); }
            let approved:Option<String>=tx.query_row("SELECT approved_principal_key FROM binding_requests WHERE id=?1",params![r.id],|row|row.get(0)).map_err(db_error)?;
            if approved.as_deref()!=Some(&grant.principal_key) { return Err("FORBIDDEN".into()); }
            // Recovery has a newly authenticated browser action, so its
            // nonce cannot equal the nonce that authorized the original
            // one-shot CREATE.  Roll it forward atomically while leaving the
            // durable candidate identity untouched; finish verification then
            // remains bound to this recovery action only.
            let changed=tx.execute("UPDATE binding_requests SET state='VERIFYING',review_nonce_hash=?3 WHERE id=?1 AND state='UNKNOWN' AND operation='CREATE' AND approved_principal_key=?2 AND candidate_thread_id IS NOT NULL",params![r.id,grant.principal_key,grant.nonce_hash]).map_err(db_error)?;
            if changed!=1 { return Err("CREATION_UNKNOWN".into()); }
            let next=review(&tx,&grant.principal_key,&r.id)?;
            tx.commit().map_err(db_error)?;
            Ok(next)
        })
    }
    /// Records the narrowly provable absence of the candidate returned by a
    /// prior `thread/start`.  This is not expiry or a guessed retry: the
    /// pinned server must have rejected both exact read/resume of that same
    /// durable id as having no rollout.  The historical request and id stay
    /// retained for audit, while the unbound context may prepare a new Review.
    pub fn confirm_creation_not_created(
        &self,
        grant: &BindingGrant,
        _thread: &str,
    ) -> Result<(), String> {
        self.with_connection(|c| {
            // `begin_creation_recovery` has already atomically consumed the
            // browser action and moved this exact record to VERIFYING.  Its
            // persisted nonce belongs to that transition; comparing the
            // caller's pre-transition review snapshot again here incorrectly
            // rejects the same in-flight recovery.  VERIFYING plus owner and
            // original candidate identity was checked before this function
            // was entered; the persisted column may have been normalized by
            // the original interrupted CREATE. A second action cannot enter
            // while this record is VERIFYING.
            let changed = c.execute(
                "UPDATE binding_requests SET state='NOT_CREATED' WHERE id=?1 AND state='VERIFYING' AND operation='CREATE' AND approved_principal_key=?2",
                params![grant.review.id, grant.principal_key],
            ).map_err(db_error)?;
            if changed != 1 { return Err("CREATION_UNKNOWN".into()); }
            Ok(())
        })
    }
    pub fn prepare_binding(
        &self,
        identity: &ScopeIdentity,
        proposal: &BindingProposal,
    ) -> Result<BindingReview, String> {
        Uuid::parse_str(&proposal.client_request_id).map_err(|_| "Invalid request ID")?;
        match proposal.operation {
            BindingOperation::Select | BindingOperation::Replace
                if proposal.project_id.is_some() && proposal.candidate_thread_id.is_some() => {}
            BindingOperation::Create
                if proposal.project_id.is_some() && proposal.candidate_thread_id.is_none() => {}
            BindingOperation::Takeover
                if proposal.workstream_id.is_some() && proposal.candidate_thread_id.is_none() => {}
            BindingOperation::Unbind
                if proposal.project_id.is_none()
                    && proposal.candidate_thread_id.is_none()
                    && proposal.workstream_id.is_none() => {}
            _ => return Err("Invalid binding proposal".into()),
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let ctx=if let Some(ctx)=context(&tx,identity)? {ctx} else {
                let identifier=id();let host_scope=(identity.key_version>=2).then_some(identity.scope_key.as_str());tx.execute("INSERT INTO control_contexts(id,principal_key,scope_key,key_version,host_scope_id,state,revision,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,'UNBOUND',0,?6,?6)",params![identifier,identity.principal_key,identity.scope_key,identity.key_version,host_scope,now()]).map_err(db_error)?;context(&tx,identity)?.ok_or("INTERNAL")?
            };
            if ctx.state=="REVOKED" {return Err("NOT_FOUND".into());}
            let hash=length_prefixed_hash(&[b"binding-v1",proposal.operation.wire().as_bytes(),&proposal.expected_context_revision.to_be_bytes(),proposal.project_id.as_deref().unwrap_or("").as_bytes(),proposal.candidate_thread_id.as_deref().unwrap_or("").as_bytes(),proposal.workstream_id.as_deref().unwrap_or("").as_bytes()]);
            if let Some((old,old_hash))=tx.query_row("SELECT id,request_hash FROM binding_requests WHERE context_id=?1 AND client_request_id=?2",params![ctx.id,proposal.client_request_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional().map_err(db_error)? {
                if old_hash!=hash {return Err("IDEMPOTENCY_CONFLICT".into());}return review(&tx,&identity.principal_key,&old);
            }
            if ctx.revision!=proposal.expected_context_revision {return Err("STALE_CONTEXT".into());}
            let pending:i64=tx.query_row("SELECT count(*) FROM binding_requests b JOIN control_contexts c ON c.id=b.context_id WHERE c.principal_key=?1 AND b.state='REVIEW' AND b.expires_at>?2",params![identity.principal_key,now()],|r|r.get(0)).map_err(db_error)?;
            if pending>=20 {return Err("RATE_LIMITED".into());}
            let busy:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM binding_requests WHERE context_id=?1 AND state IN ('CREATING','VERIFYING','UNKNOWN'))",params![ctx.id],|r|r.get(0)).map_err(db_error)?;
            if busy {return Err("CREATION_UNKNOWN".into());}
            if matches!(proposal.operation,BindingOperation::Select|BindingOperation::Create) && ctx.state=="BOUND" {return Err("STALE_BINDING".into());}
            if matches!(proposal.operation,BindingOperation::Replace|BindingOperation::Unbind) && ctx.state!="BOUND" {return Err("UNBOUND".into());}
            let ws=proposal.workstream_id.clone().or(ctx.workstream_id.clone());
            let mut project=proposal.project_id.clone();let mut binding_revision=None;
            if let Some(ws)=&ws {
                ensure_no_dispatch(&tx,ws)?;
                let (p,rev):(String,i64)=tx.query_row("SELECT project_id,binding_revision FROM workstreams WHERE id=?1 AND status='ACTIVE' AND trashed_at IS NULL",params![ws],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?.ok_or("NOT_FOUND")?;
                if project.as_deref().is_some_and(|old|old!=p) {return Err("STALE_BINDING".into());}project=Some(p);binding_revision=Some(rev);
            }
            let (root_revision,policy_hash)=if let Some(project)=&project {
                let (r,h):(i64,String)=tx.query_row("SELECT revision,policy_hash FROM project_execution_roots WHERE project_id=?1",params![project],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?.ok_or("NOT_FOUND")?;(Some(r),Some(h))
            } else {(None,None)};
            let candidate=if proposal.operation==BindingOperation::Takeover {
                tx.query_row("SELECT external_id FROM endpoints WHERE workstream_id=?1 AND provider='CODEX' AND status='ACTIVE'",params![ws],|r|r.get(0)).optional().map_err(db_error)?
            } else {proposal.candidate_thread_id.clone()};
            let identifier=id();tx.execute("INSERT INTO binding_requests(id,context_id,client_request_id,request_hash,operation,project_id,workstream_id,expected_context_revision,expected_binding_revision,root_revision,candidate_thread_id,candidate_policy_hash,state,created_at,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'REVIEW',?13,?14)",params![identifier,ctx.id,proposal.client_request_id,hash,proposal.operation.wire(),project,ws,ctx.revision,binding_revision,root_revision,candidate,policy_hash,now(),now()+600_000]).map_err(db_error)?;
            let value=review(&tx,&identity.principal_key,&identifier)?;tx.commit().map_err(db_error)?;Ok(value)
        })
    }
    pub fn binding_review(
        &self,
        principal: &str,
        request_id: &str,
    ) -> Result<BindingReview, String> {
        self.with_connection(|c| review(c, principal, request_id))
    }
    pub fn binding_scope(&self, principal: &str, request: &str) -> Result<ScopeIdentity, String> {
        self.with_connection(|c|c.query_row("SELECT x.principal_key,x.scope_key,x.key_version,x.host_scope_id FROM control_contexts x JOIN binding_requests b ON b.context_id=x.id WHERE b.id=?1 AND x.principal_key=?2 AND x.state!='REVOKED'",params![request,principal],|row| {let legacy:String=row.get(1)?;let host:Option<String>=row.get(3)?;Ok(ScopeIdentity{principal_key:row.get(0)?,scope_key:host.unwrap_or(legacy),key_version:row.get(2)?})}).optional().map_err(db_error)?.ok_or("NOT_FOUND".into()))
    }
    pub fn binding_selection_preview(
        &self,
        principal: &str,
        request: &str,
        candidate: Option<&str>,
        workstream: Option<&str>,
    ) -> Result<BindingReview, String> {
        self.with_connection(|c| {
            selected_review(
                c,
                principal,
                review(c, principal, request)?,
                candidate,
                workstream,
            )
        })
    }
    pub fn binding_owner_workstreams(
        &self,
        principal: &str,
        request: &str,
        after: &str,
    ) -> Result<Vec<(String, String, i64, String)>, String> {
        self.with_connection(|c| {
            let r=review(c,principal,request)?;
            let mut q=c.prepare("SELECT w.id,w.name,w.binding_revision,e.external_id FROM workstreams w JOIN endpoints e ON e.workstream_id=w.id AND e.provider='CODEX' AND e.status='ACTIVE' WHERE w.project_id=?1 AND w.id>?3 AND w.status='ACTIVE' AND w.trashed_at IS NULL AND EXISTS(SELECT 1 FROM control_contexts x WHERE x.workstream_id=w.id AND x.principal_key=?2 AND x.state='BOUND') AND NOT EXISTS(SELECT 1 FROM control_contexts x WHERE x.workstream_id=w.id AND x.principal_key!=?2 AND x.state='BOUND') ORDER BY w.id LIMIT 21").map_err(db_error)?;
            let rows=q.query_map(params![r.project_id,principal,after],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).map_err(db_error)?;
            rows.collect::<Result<Vec<_>,_>>().map_err(db_error)
        })
    }
    /// A second ChatGPT scope must re-confirm the owner's existing Workstream
    /// instead of trying to insert the same durable Codex endpoint again.
    /// This is only a preflight for the browser flow; `apply_binding` repeats
    /// the check in its transaction so a concurrent change cannot bypass it.
    pub fn binding_requires_owned_workstream_reconfirmation(
        &self,
        principal: &str,
        review: &BindingReview,
    ) -> Result<bool, String> {
        if review.operation != BindingOperation::Select {
            return Ok(false);
        }
        let (Some(project), Some(thread)) = (
            review.project_id.as_deref(),
            review.candidate_thread_id.as_deref(),
        ) else {
            return Ok(false);
        };
        self.with_connection(|c| {
            owned_workstream_reconfirmation_required(c, principal, project, thread)
        })
    }
    /// Called only with a browser challenge and freshly verified native target.
    /// CREATE uses a separate durable CREATING/VERIFYING coordinator before this.
    pub fn apply_binding(
        &self,
        grant: &BindingGrant,
        target: Option<&VerifiedBindingTarget>,
    ) -> Result<ContextRecord, String> {
        if !valid_hash(&grant.nonce_hash) {
            return Err("FORBIDDEN".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let exiting:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM runtime_exit_state WHERE phase!='RUNNING')",[],|r|r.get(0)).map_err(db_error)?;
            if exiting {return Err("EXIT_PENDING".into());}
            let mut r=review(&tx,&grant.principal_key,&grant.review.id)?;
            if r.state=="REVIEW" && (r.operation!=grant.review.operation || r.candidate_thread_id!=grant.review.candidate_thread_id || r.workstream_id!=grant.review.workstream_id) {
                let (candidate,ws)=if grant.review.operation==BindingOperation::Takeover {(None,grant.review.workstream_id.as_deref())}else{(grant.review.candidate_thread_id.as_deref(),None)};
                let selected=selected_review(&tx,&grant.principal_key,r.clone(),candidate,ws)?;
                if selected!=grant.review {return Err("STALE_BINDING".into());}
                tx.execute("UPDATE binding_requests SET operation=?2,workstream_id=?3,expected_binding_revision=?4,candidate_thread_id=?5 WHERE id=?1 AND state='REVIEW'",params![r.id,selected.operation.wire(),selected.workstream_id,selected.expected_binding_revision,selected.candidate_thread_id]).map_err(db_error)?;
                r=selected;
            }
            let (state,revision,old_ws,current_host_scope):(String,i64,Option<String>,Option<String>)=tx.query_row("SELECT state,revision,workstream_id,host_scope_id FROM control_contexts WHERE id=?1 AND principal_key=?2",params![r.context_id,grant.principal_key],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).map_err(db_error)?;
            if state=="REVOKED" {return Err("NOT_FOUND".into());}
            if r.state=="APPLIED" {
                let nonce:Option<String>=tx.query_row("SELECT review_nonce_hash FROM binding_requests WHERE id=?1",params![r.id],|row|row.get(0)).map_err(db_error)?;
                if nonce.as_deref()!=Some(&grant.nonce_hash) {return Err("FORBIDDEN".into());}
                return Ok(ContextRecord{id:r.context_id,principal_key:grant.principal_key.clone(),state,revision,workstream_id:old_ws});
            }
            if r.expires_at<now() {return Err("EXPIRED".into());}
            if revision!=r.expected_context_revision || r.expected_context_revision!=grant.review.expected_context_revision {return Err("STALE_CONTEXT".into());}
            if r.operation!=grant.review.operation || (r.operation!=BindingOperation::Create && r.candidate_thread_id!=grant.review.candidate_thread_id) || r.workstream_id!=grant.review.workstream_id {return Err("STALE_BINDING".into());}
            if r.project_id!=grant.review.project_id || r.root_revision!=grant.review.root_revision || r.policy_hash!=grant.review.policy_hash {return Err("ROOT_CHANGED".into());}
            if !matches!(r.state.as_str(),"REVIEW"|"READY") {return Err("CREATION_UNKNOWN".into());}
            if let Some(ws)=&old_ws {ensure_no_dispatch(&tx,ws)?;}
            let mut ws=r.workstream_id.clone();
            if let Some(ws)=&ws {
                ensure_no_dispatch(&tx,ws)?;
                let revision:i64=tx.query_row("SELECT binding_revision FROM workstreams WHERE id=?1",params![ws],|r|r.get(0)).map_err(db_error)?;
                if Some(revision)!=r.expected_binding_revision {return Err("STALE_BINDING".into());}
            }
            if r.operation!=BindingOperation::Unbind {
                let target=target.ok_or("CAPABILITY_UNVERIFIED")?;
                if !target.durable || Some(&target.policy_hash)!=r.policy_hash.as_ref() || Some(target.root_revision)!=r.root_revision {return Err("POLICY_CHANGED".into());}
                if r.candidate_thread_id.as_deref()!=Some(&target.thread_id) {return Err("STALE_BINDING".into());}
                let (rr,ph):(i64,String)=tx.query_row("SELECT revision,policy_hash FROM project_execution_roots WHERE project_id=?1",params![r.project_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(db_error)?;
                if rr!=target.root_revision || ph!=target.policy_hash {return Err("ROOT_CHANGED".into());}
                if r.operation==BindingOperation::Create && r.state!="READY" {return Err("NEW_THREAD_DURABILITY_UNSUPPORTED".into());}
                if r.operation==BindingOperation::Select {
                    if let (Some(project),Some(thread))=(r.project_id.as_deref(),r.candidate_thread_id.as_deref()) {
                        if owned_workstream_reconfirmation_required(&tx,&grant.principal_key,project,thread)? {
                            return Err("OWNED_WORKSTREAM_RECONFIRMATION_REQUIRED".into());
                        }
                    }
                }
                if ws.is_none() {
                    let new_ws=id();tx.execute("INSERT INTO workstreams(id,project_id,name,status,created_at,updated_at) VALUES(?1,?2,'Codex task','ACTIVE',?3,?3)",params![new_ws,r.project_id,now()]).map_err(db_error)?;ws=Some(new_ws);
                }
                let ws_id=ws.as_ref().ok_or("INTERNAL")?;
                if r.operation==BindingOperation::Takeover {
                    let stable_host_reconfirmation = current_host_scope.is_some() as i64;
                    let foreign:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM control_contexts WHERE workstream_id=?1 AND state='BOUND' AND principal_key!=?2 AND (?3=0 OR host_scope_id IS NOT NULL))",params![ws_id,grant.principal_key,stable_host_reconfirmation],|r|r.get(0)).map_err(db_error)?;
                    if foreign {return Err("FORBIDDEN".into());}
                    tx.execute("UPDATE control_contexts SET state='REVOKED',revision=revision+1,updated_at=?2 WHERE workstream_id=?1 AND state='BOUND' AND id!=?3 AND (?4=0 OR host_scope_id IS NOT NULL)",params![ws_id,now(),r.context_id,stable_host_reconfirmation]).map_err(db_error)?;
                } else {
                    let previous:Option<String>=tx.query_row("SELECT id FROM endpoints WHERE workstream_id=?1 AND provider='CODEX' AND status='ACTIVE'",params![ws_id],|r|r.get(0)).optional().map_err(db_error)?;
                    if let Some(previous)=&previous {tx.execute("UPDATE endpoints SET status='SUPERSEDED',superseded_at=?2 WHERE id=?1",params![previous,now()]).map_err(db_error)?;}
                    tx.execute("INSERT INTO endpoints(id,workstream_id,provider,external_id,label,status,replaces_endpoint_id,created_at) VALUES(?1,?2,'CODEX',?3,'Codex','ACTIVE',?4,?5)",params![id(),ws_id,target.thread_id,previous,now()]).map_err(db_error)?;
                }
            }
            for ws_id in old_ws.iter().chain(ws.iter()).collect::<std::collections::HashSet<_>>() {
                tx.execute("UPDATE handoffs SET status='CANCELLED' WHERE workstream_id=?1 AND source_kind='CONTROL_CONTEXT' AND status='READY'",params![ws_id]).map_err(db_error)?;
                tx.execute("UPDATE workstreams SET binding_revision=binding_revision+1,updated_at=?2 WHERE id=?1",params![ws_id,now()]).map_err(db_error)?;
            }
            let bound=if r.operation==BindingOperation::Unbind {None}else{ws};let state=if bound.is_some(){"BOUND"}else{"UNBOUND"};
            tx.execute("UPDATE control_contexts SET state=?2,workstream_id=?3,revision=revision+1,updated_at=?4 WHERE id=?1",params![r.context_id,state,bound,now()]).map_err(db_error)?;
            tx.execute("UPDATE binding_requests SET state='APPLIED',approved_principal_key=?2,review_nonce_hash=?3 WHERE id=?1",params![r.id,grant.principal_key,grant.nonce_hash]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;Ok(ContextRecord{id:r.context_id,principal_key:grant.principal_key.clone(),state:state.into(),revision:revision+1,workstream_id:bound})
        })
    }
}
