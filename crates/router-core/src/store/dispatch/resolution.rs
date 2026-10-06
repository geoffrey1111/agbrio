//! UNKNOWN actions accept only a server preview verified by the browser layer.
use super::evidence::{approved, bound, load, validate, NativeReceipt};
use super::*;
use crate::identity::length_prefixed_hash;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResolutionAction {
    AttachRun,
    ConfirmNotSent,
    KeepUnknown,
}
impl ResolutionAction {
    fn wire(&self) -> &'static str {
        match self {
            Self::AttachRun => "ATTACH_RUN",
            Self::ConfirmNotSent => "CONFIRM_NOT_SENT",
            Self::KeepUnknown => "KEEP_UNKNOWN",
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ResolutionPreview {
    pub dispatch_id: String,
    pub source_kind: String,
    pub context_id: Option<String>,
    pub source_endpoint_id: Option<String>,
    pub context_revision: i64,
    pub binding_revision: i64,
    pub dispatch_revision: i64,
    pub action: ResolutionAction,
    pub evidence_digest: String,
    pub ack_or_stop_evidence_id: Option<String>,
    pub terminal_evidence_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolutionSource {
    source_kind: String,
    context_id: Option<String>,
    source_endpoint_id: Option<String>,
    context_revision: i64,
    binding_revision: i64,
}
fn source(c: &Connection, principal: &str, handoff_id: &str) -> Result<ResolutionSource, String> {
    // Control and Relay dispatches carry different durable authorization
    // records. The latter has no host ScopeIdentity, so resolving it must use
    // the Relay owner row directly rather than manufacturing a browser scope.
    if let Ok(snapshot) = control::snapshot(c, principal, handoff_id) {
        return Ok(ResolutionSource {
            source_kind: "CONTROL_CONTEXT".into(),
            context_id: Some(snapshot.context_id),
            source_endpoint_id: None,
            context_revision: snapshot.context_revision,
            binding_revision: snapshot.binding_revision,
        });
    }
    c.query_row(
        "SELECT h.source_endpoint_id,d.binding_revision
         FROM handoffs h JOIN relay_handoff_details d ON d.handoff_id=h.id
         WHERE h.id=?1 AND h.source_kind='ENDPOINT' AND d.owner_principal_key=?2
           AND d.consumed_at IS NOT NULL",
        params![handoff_id, principal],
        |r| {
            Ok(ResolutionSource {
                source_kind: "ENDPOINT".into(),
                context_id: None,
                source_endpoint_id: Some(r.get(0)?),
                // Endpoint-source Relay handoffs have no Control Context. Zero is
                // the explicit schema-compatible sentinel, never a forged context
                // revision; source_kind/source_endpoint_id preserve the identity.
                context_revision: 0,
                binding_revision: r.get(1)?,
            })
        },
    )
    .optional()
    .map_err(db_error)?
    .ok_or("NOT_FOUND".into())
}
/// Not deserializable; constructed only from a session-bound server preview.
pub struct ResolutionGrant {
    pub principal_key: String,
    pub preview: ResolutionPreview,
    pub nonce_hash: String,
    pub expires_at: i64,
}
#[derive(Clone, Debug, Serialize)]
pub struct ResolutionOutcome {
    pub resolution_id: String,
    pub run_id: String,
    pub outcome: String,
    pub lock_held: bool,
}
#[derive(Serialize)]
struct ResolutionState {
    handoff_status: String,
    run_status: String,
    dispatch_phase: String,
    turn_id: Option<String>,
    lock_held: bool,
}
fn state(c: &Connection, d: &DispatchRecord) -> Result<ResolutionState, String> {
    c.query_row("SELECT h.status,p.status,p.external_run_id FROM handoffs h JOIN provider_runs p ON p.origin_handoff_id=h.id WHERE h.id=?1 AND p.id=?2",params![d.handoff_id,d.run_id],|r|Ok(ResolutionState{handoff_status:r.get(0)?,run_status:r.get(1)?,turn_id:r.get(2)?,dispatch_phase:d.phase.clone(),lock_held:!matches!(d.phase.as_str(),"TERMINAL"|"NOT_SENT")})).map_err(db_error)
}
fn preview(
    c: &Connection,
    principal: &str,
    dispatch: &str,
    action: ResolutionAction,
    key: &[u8; 32],
) -> Result<
    (
        ResolutionPreview,
        Option<NativeReceipt>,
        Option<NativeReceipt>,
    ),
    String,
> {
    let d = record(c, dispatch)?;
    let source = source(c, principal, &d.handoff_id)?;
    let evidence = approved(c, &d)?;
    if d.phase != "UNKNOWN" {
        return Err("RESOLUTION_NOT_REQUIRED".into());
    }
    let mut receipts = Vec::new();
    let mut ack = None;
    let mut stop = None;
    let mut terminal = None;
    let mut conflict = false;
    for kind in ["ACK_LINK", "PREWRITE_STOP", "TERMINAL_LINK", "CONFLICT"] {
        if let Some(r) = load(c, dispatch, kind)? {
            // Corrupt receipts never unlock. KeepUnknown remains available for
            // a valid owner; its digest still binds the observed corrupt bytes.
            let verified = validate(&r, key)
                .and_then(|_| bound(&r, &d, &evidence))
                .is_ok();
            receipts.push(serde_json::to_string(&r).map_err(|_| "INTERNAL")?);
            if !verified || kind == "CONFLICT" {
                conflict = true;
                continue;
            }
            match kind {
                "ACK_LINK" => ack = Some(r),
                "PREWRITE_STOP" => stop = Some(r),
                "TERMINAL_LINK" => terminal = Some(r),
                _ => {}
            }
        }
    }
    if let (Some(a), Some(t)) = (&ack, &terminal) {
        if a.turn_id != t.turn_id {
            conflict = true;
        }
    }
    if terminal.is_some() && ack.is_none() {
        conflict = true;
    }
    let digest = length_prefixed_hash(&receipts.iter().map(|s| s.as_bytes()).collect::<Vec<_>>());
    let chosen = match action {
        ResolutionAction::KeepUnknown => None,
        ResolutionAction::AttachRun => {
            if conflict || stop.is_some() || ack.is_none() {
                return Err("INSUFFICIENT_EVIDENCE".into());
            }
            ack.clone()
        }
        ResolutionAction::ConfirmNotSent => {
            let written:bool=c.query_row("SELECT write_intent_at IS NOT NULL OR epoch_sealed_at IS NULL FROM handoff_dispatches WHERE dispatch_id=?1",params![dispatch],|r|r.get(0)).map_err(db_error)?;
            if conflict || written || ack.is_some() || terminal.is_some() || stop.is_none() {
                return Err("INSUFFICIENT_EVIDENCE".into());
            }
            stop.clone()
        }
    };
    Ok((
        ResolutionPreview {
            dispatch_id: dispatch.into(),
            source_kind: source.source_kind,
            context_id: source.context_id,
            source_endpoint_id: source.source_endpoint_id,
            context_revision: source.context_revision,
            binding_revision: source.binding_revision,
            dispatch_revision: d.revision,
            action,
            evidence_digest: digest,
            ack_or_stop_evidence_id: chosen.as_ref().map(|r| r.id.clone()),
            terminal_evidence_id: terminal.as_ref().map(|r| r.id.clone()),
        },
        chosen,
        terminal,
    ))
}
impl RouterStore {
    /// Re-authenticated duplicate lookup after in-memory previews were lost.
    /// The public commit body consists only of the derived preview ID, nonce,
    /// client ID and expected revision, all checked before returning old audit.
    pub fn resolution_replay(
        &self,
        principal: &str,
        dispatch: &str,
        client_id: &str,
        nonce_hash: &str,
        expected_revision: i64,
    ) -> Result<Option<ResolutionOutcome>, String> {
        let d = self.dispatch_record(dispatch)?;
        self.with_connection(|c| {
            source(c, principal, &d.handoff_id)?;
            let audit:Option<(String,String,i64,String)>=c.query_row("SELECT id,nonce_hash,expected_dispatch_revision,action FROM operator_resolutions WHERE dispatch_id=?1 AND client_resolution_id=?2 AND principal_key=?3",params![dispatch,client_id,principal],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(db_error)?;
            let Some((id,nonce,revision,action))=audit else{return Ok(None)};
            if nonce!=nonce_hash || revision!=expected_revision {return Err("IDEMPOTENCY_CONFLICT".into());}
            Ok(Some(ResolutionOutcome{resolution_id:id,run_id:d.run_id,outcome:match action.as_str(){"ATTACH_RUN"=>"ATTACHED","CONFIRM_NOT_SENT"=>"NOT_SENT",_=>"KEPT_UNKNOWN"}.into(),lock_held:!matches!(d.phase.as_str(),"TERMINAL"|"NOT_SENT")}))
        })
    }
    pub fn resolution_preview(
        &self,
        principal: &str,
        dispatch: &str,
        action: ResolutionAction,
        key: &[u8; 32],
    ) -> Result<ResolutionPreview, String> {
        self.with_connection(|c| preview(c, principal, dispatch, action, key).map(|v| v.0))
    }
    pub fn commit_resolution(
        &self,
        grant: &ResolutionGrant,
        client_id: &str,
        key: &[u8; 32],
    ) -> Result<ResolutionOutcome, String> {
        Uuid::parse_str(client_id).map_err(|_| "INVALID_REQUEST_ID")?;
        if grant.nonce_hash.len() != 64 || !grant.nonce_hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("FORBIDDEN".into());
        }
        let request_hash = length_prefixed_hash(&[
            b"resolution-v1",
            grant.principal_key.as_bytes(),
            serde_json::to_string(&grant.preview)
                .map_err(|_| "INTERNAL")?
                .as_bytes(),
            grant.nonce_hash.as_bytes(),
        ]);
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let d=record(&tx,&grant.preview.dispatch_id)?;
            let current_source = source(&tx, &grant.principal_key, &d.handoff_id)?;
            if current_source.source_kind != grant.preview.source_kind
                || current_source.context_id != grant.preview.context_id
                || current_source.source_endpoint_id != grant.preview.source_endpoint_id
                || current_source.context_revision != grant.preview.context_revision
                || current_source.binding_revision != grant.preview.binding_revision
            { return Err("STALE_RESOLUTION".into()); }
            let old:Option<(String,String,String)>=tx.query_row("SELECT id,request_hash,action FROM operator_resolutions WHERE dispatch_id=?1 AND client_resolution_id=?2",params![d.dispatch_id,client_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db_error)?;
            if let Some((id,hash,action))=old {
                if hash!=request_hash {return Err("IDEMPOTENCY_CONFLICT".into());}
                return Ok(ResolutionOutcome{resolution_id:id,run_id:d.run_id,outcome:match action.as_str(){"ATTACH_RUN"=>"ATTACHED","CONFIRM_NOT_SENT"=>"NOT_SENT",_=>"KEPT_UNKNOWN"}.into(),lock_held:!matches!(d.phase.as_str(),"TERMINAL"|"NOT_SENT")});
            }
            if grant.expires_at<now() {return Err("EXPIRED".into());}
            let (current,proof,terminal)=preview(&tx,&grant.principal_key,&d.dispatch_id,grant.preview.action.clone(),key)?;
            if serde_json::to_value(&current).map_err(|_|"INTERNAL")?!=serde_json::to_value(&grant.preview).map_err(|_|"INTERNAL")? {return Err("STALE_RESOLUTION".into());}
            let old_state=state(&tx,&d)?;
            let mut next=ResolutionState{handoff_status:old_state.handoff_status.clone(),run_status:old_state.run_status.clone(),dispatch_phase:"UNKNOWN".into(),turn_id:old_state.turn_id.clone(),lock_held:true};
            let (reason,outcome)=match current.action {
                ResolutionAction::KeepUnknown=>("INSUFFICIENT","KEPT_UNKNOWN"),
                ResolutionAction::ConfirmNotSent=>{next.handoff_status="FAILED".into();next.run_status="FAILED".into();next.dispatch_phase="NOT_SENT".into();next.lock_held=false;("PREWRITE_PROVEN","NOT_SENT")},
                ResolutionAction::AttachRun=>{
                    let ack=proof.as_ref().ok_or("EVIDENCE_REQUIRED")?;
                    if old_state.turn_id.is_some() && old_state.turn_id!=ack.turn_id {return Err("EVIDENCE_CONFLICT".into());}
                    next.turn_id=ack.turn_id.clone();next.handoff_status="SENT".into();
                    if let Some(t)=terminal.as_ref() {
                        let status=t.native_status.as_deref().ok_or("EVIDENCE_INVALID")?;
                        if !matches!(status,"COMPLETED"|"FAILED"|"CANCELLED") {return Err("EVIDENCE_INVALID".into());}
                        next.run_status=status.into();next.dispatch_phase="TERMINAL".into();next.lock_held=false;("TERMINAL_PROVEN","ATTACHED")
                    } else {next.run_status="UNKNOWN".into();("ACK_PROVEN","ATTACHED")}
                }
            };
            tx.execute("UPDATE handoff_dispatches SET phase=?2,revision=revision+1,updated_at=?3 WHERE dispatch_id=?1",params![d.dispatch_id,next.dispatch_phase,now()]).map_err(db_error)?;
            tx.execute("UPDATE provider_runs SET status=?2,external_run_id=?3,terminal_code=CASE WHEN ?4='PREWRITE_PROVEN' THEN 'PROVEN_NOT_SENT' ELSE terminal_code END,terminal_at=CASE WHEN ?5 THEN NULL ELSE ?6 END,updated_at=?6 WHERE id=?1",params![d.run_id,next.run_status,next.turn_id,reason,next.lock_held,now()]).map_err(db_error)?;
            tx.execute("UPDATE handoffs SET status=?2,sent_at=CASE WHEN ?2='SENT' THEN COALESCE(sent_at,?3) ELSE sent_at END,failed_at=CASE WHEN ?2='FAILED' THEN ?3 ELSE failed_at END,error_code=CASE WHEN ?2='FAILED' THEN 'PROVEN_NOT_SENT' ELSE error_code END WHERE id=?1",params![d.handoff_id,next.handoff_status,now()]).map_err(db_error)?;
            if reason=="PREWRITE_PROVEN" {tx.execute("UPDATE provider_result_details SET result_state='NOT_APPLICABLE',updated_at=?2 WHERE run_id=?1",params![d.run_id,now()]).map_err(db_error)?;}
            let resolution=id();
            tx.execute("INSERT INTO operator_resolutions(id,dispatch_id,client_resolution_id,request_hash,principal_key,context_id,source_endpoint_id,source_kind,context_revision,binding_revision,expected_dispatch_revision,resulting_dispatch_revision,action,ack_or_stop_evidence_id,terminal_evidence_id,evidence_digest,nonce_hash,old_state_json,new_state_json,reason_code,channel,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,'BROWSER_REVIEW',?21)",params![resolution,d.dispatch_id,client_id,request_hash,grant.principal_key,current.context_id,current.source_endpoint_id,current.source_kind,current.context_revision,current.binding_revision,current.dispatch_revision,current.dispatch_revision+1,current.action.wire(),current.ack_or_stop_evidence_id,current.terminal_evidence_id,current.evidence_digest,grant.nonce_hash,serde_json::to_string(&old_state).map_err(|_|"INTERNAL")?,serde_json::to_string(&next).map_err(|_|"INTERNAL")?,reason,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;
            Ok(ResolutionOutcome{resolution_id:resolution,run_id:d.run_id,outcome:outcome.into(),lock_held:next.lock_held})
        })
    }
}
