//! Bounded, locally authenticated receipts. MACs are not upstream signatures.
use super::*;
use crate::{
    codex::{
        adapter::{NativeTurnAck, SealedPrewrite},
        result::ExactTerminal,
    },
    identity::length_prefixed_hash,
};
use hmac::{Hmac, Mac};

#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NativeReceipt {
    pub id: String,
    pub dispatch_id: String,
    pub kind: String,
    pub backend_id: String,
    pub native_version: String,
    pub proof_source: String,
    pub adapter_epoch: String,
    pub request_id: Value,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub item_id: Option<String>,
    pub payload_hash: String,
    pub native_status: Option<String>,
    pub event_sequence: Option<i64>,
    pub evidence_digest: String,
    pub integrity_mac: String,
    pub recorded_at: i64,
}
fn unsigned(r: &NativeReceipt) -> Result<Vec<u8>, String> {
    let mut r = r.clone();
    r.integrity_mac.clear();
    r.evidence_digest.clear();
    let data = serde_json::to_vec(&r).map_err(|_| "INTERNAL")?;
    if data.len() > 4096 {
        return Err("EVIDENCE_LIMIT".into());
    }
    Ok(data)
}
fn authenticate(r: &mut NativeReceipt, key: &[u8; 32]) -> Result<(), String> {
    let data = unsigned(r)?;
    if r.evidence_digest.is_empty() {
        r.evidence_digest = length_prefixed_hash(&[b"native-receipt-v1", &data]);
    }
    if r.evidence_digest.len() != 64 || !r.evidence_digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("EVIDENCE_INVALID".into());
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| "INTERNAL")?;
    mac.update(&data);
    mac.update(r.evidence_digest.as_bytes());
    r.integrity_mac = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect();
    if serde_json::to_vec(r).map_err(|_| "INTERNAL")?.len() > 4096 {
        return Err("EVIDENCE_LIMIT".into());
    }
    Ok(())
}
pub(super) fn validate(r: &NativeReceipt, key: &[u8; 32]) -> Result<(), String> {
    if r.integrity_mac.len() != 64
        || !r.integrity_mac.bytes().all(|b| b.is_ascii_hexdigit())
        || r.evidence_digest.len() != 64
        || !r.evidence_digest.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("EVIDENCE_INVALID".into());
    }
    let bytes: Result<Vec<u8>, _> = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&r.integrity_mac[i..i + 2], 16))
        .collect();
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| "INTERNAL")?;
    mac.update(&unsigned(r)?);
    mac.update(r.evidence_digest.as_bytes());
    mac.verify_slice(&bytes.map_err(|_| "EVIDENCE_INVALID")?)
        .map_err(|_| "EVIDENCE_INVALID".into())
}
pub(super) fn load(
    c: &Connection,
    dispatch: &str,
    kind: &str,
) -> Result<Option<NativeReceipt>, String> {
    c.query_row("SELECT id,dispatch_id,kind,backend_id,native_version,proof_source,adapter_epoch,request_id_json,thread_id,turn_id,item_id,payload_hash,native_status,event_sequence,evidence_digest,integrity_mac,recorded_at FROM dispatch_native_evidence WHERE dispatch_id=?1 AND kind=?2",params![dispatch,kind],|r|{
        let raw:String=r.get(7)?;
        let request_id=serde_json::from_str(&raw).map_err(|e|rusqlite::Error::FromSqlConversionFailure(7,rusqlite::types::Type::Text,Box::new(e)))?;
        Ok(NativeReceipt{id:r.get(0)?,dispatch_id:r.get(1)?,kind:r.get(2)?,backend_id:r.get(3)?,native_version:r.get(4)?,proof_source:r.get(5)?,adapter_epoch:r.get(6)?,request_id,thread_id:r.get(8)?,turn_id:r.get(9)?,item_id:r.get(10)?,payload_hash:r.get(11)?,native_status:r.get(12)?,event_sequence:r.get(13)?,evidence_digest:r.get(14)?,integrity_mac:r.get(15)?,recorded_at:r.get(16)?})
    }).optional().map_err(db_error)
}
fn insert(c: &Connection, r: &NativeReceipt) -> Result<(), String> {
    c.execute("INSERT INTO dispatch_native_evidence(id,dispatch_id,kind,backend_id,native_version,proof_source,adapter_epoch,request_id_json,thread_id,turn_id,item_id,payload_hash,native_status,event_sequence,evidence_digest,integrity_mac,recorded_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",params![r.id,r.dispatch_id,r.kind,r.backend_id,r.native_version,r.proof_source,r.adapter_epoch,r.request_id.to_string(),r.thread_id,r.turn_id,r.item_id,r.payload_hash,r.native_status,r.event_sequence,r.evidence_digest,r.integrity_mac,r.recorded_at]).map_err(db_error)?;
    Ok(())
}
#[derive(Clone)]
pub(super) struct EvidenceSnapshot {
    pub workstream_id: String,
    pub thread_id: String,
    pub policy: super::control::PolicySnapshot,
    pub payload_hash: String,
}
pub(super) fn approved(c: &Connection, d: &DispatchRecord) -> Result<EvidenceSnapshot, String> {
    if let Some(principal)=c.query_row("SELECT approved_principal_key FROM control_handoff_details WHERE handoff_id=?1 AND consumed_at IS NOT NULL",params![d.handoff_id],|r|r.get::<_,String>(0)).optional().map_err(db_error)? {
        let s=control::snapshot(c, &principal, &d.handoff_id)?;
        return Ok(EvidenceSnapshot{workstream_id:s.workstream_id,thread_id:s.thread_id,policy:s.policy,payload_hash:s.draft.payload_hash});
    }
    let row:(String,String,String,String,String)=c.query_row("SELECT h.workstream_id,e.external_id,d.destination_policy_json,h.payload_hash,e.provider FROM handoffs h JOIN relay_handoff_details d ON d.handoff_id=h.id JOIN endpoints e ON e.id=h.destination_endpoint_id WHERE h.id=?1 AND d.consumed_at IS NOT NULL",params![d.handoff_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(db_error)?.ok_or("RELAY_APPROVAL_REQUIRED")?;
    if row.4 != "CODEX" {
        return Err("RELAY_DESTINATION_INVALID".into());
    }
    let policy = serde_json::from_str::<serde_json::Value>(&row.2)
        .ok()
        .and_then(|v| v.get("policy").cloned())
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or("RELAY_POLICY_CHANGED")?;
    Ok(EvidenceSnapshot {
        workstream_id: row.0,
        thread_id: row.1,
        policy,
        payload_hash: row.3,
    })
}
pub(super) fn bound(
    r: &NativeReceipt,
    d: &DispatchRecord,
    s: &EvidenceSnapshot,
) -> Result<(), String> {
    if r.dispatch_id != d.dispatch_id
        || r.adapter_epoch != d.adapter_epoch
        || (Some(&r.request_id) != d.request_id.as_ref()
            && !(r.kind == "PREWRITE_STOP" && r.request_id.is_null() && d.request_id.is_none()))
        || r.thread_id != s.thread_id
        || r.payload_hash != s.payload_hash
        || r.backend_id != s.policy.backend_id
        || r.native_version != s.policy.native_version
    {
        return Err("EVIDENCE_CONFLICT".into());
    }
    Ok(())
}
fn conflict<T>(
    tx: rusqlite::Transaction<'_>,
    d: &DispatchRecord,
    s: &EvidenceSnapshot,
    key: &[u8; 32],
    reason: &str,
) -> Result<T, String> {
    if load(&tx, &d.dispatch_id, "CONFLICT")?.is_none() {
        let mut receipt = NativeReceipt {
            id: id(),
            dispatch_id: d.dispatch_id.clone(),
            kind: "CONFLICT".into(),
            backend_id: s.policy.backend_id.clone(),
            native_version: s.policy.native_version.clone(),
            proof_source: "CONFLICT".into(),
            adapter_epoch: d.adapter_epoch.clone(),
            request_id: d.request_id.clone().unwrap_or(Value::Null),
            thread_id: s.thread_id.clone(),
            turn_id: None,
            item_id: None,
            payload_hash: s.payload_hash.clone(),
            native_status: None,
            event_sequence: None,
            evidence_digest: length_prefixed_hash(&[b"conflict-v1", reason.as_bytes()]),
            integrity_mac: String::new(),
            recorded_at: now(),
        };
        authenticate(&mut receipt, key)?;
        insert(&tx, &receipt)?;
    }
    tx.execute("UPDATE handoff_dispatches SET phase='UNKNOWN',revision=revision+1,last_code='EVIDENCE_CONFLICT',updated_at=?2 WHERE dispatch_id=?1",params![d.dispatch_id,now()]).map_err(db_error)?;
    tx.execute("UPDATE provider_runs SET status='UNKNOWN',terminal_at=NULL,terminal_code='EVIDENCE_CONFLICT',updated_at=?2 WHERE id=?1",params![d.run_id,now()]).map_err(db_error)?;
    tx.commit().map_err(db_error)?;
    Err("EVIDENCE_CONFLICT".into())
}
impl RouterStore {
    pub fn browser_evidence(
        &self,
        principal: &str,
        dispatch: &str,
        key: &[u8; 32],
    ) -> Result<Vec<serde_json::Value>, String> {
        let d = self.dispatch_record(dispatch)?;
        self.browser_owned_run(principal, &d.run_id)?;
        self.with_connection(|c| {
            let mut rows=Vec::new();for kind in ["ACK_LINK","PREWRITE_STOP","TERMINAL_LINK","CONFLICT"] {
                if let Some(r)=load(c,dispatch,kind)? {rows.push(serde_json::json!({"id":r.id,"kind":r.kind,"thread_id":r.thread_id,"turn_id":r.turn_id,"native_status":r.native_status,"digest":r.evidence_digest,"verified":validate(&r,key).is_ok(),"recorded_at":r.recorded_at}));}
            }Ok(rows)
        })
    }
    pub fn record_prewrite_stop(
        &self,
        dispatch: &str,
        proof: &SealedPrewrite,
        key: &[u8; 32],
    ) -> Result<NativeReceipt, String> {
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let d=record(&tx,dispatch)?;check_epoch(&d,&proof.adapter_epoch)?;let s=approved(&tx,&d)?;
            let written:bool=tx.query_row("SELECT write_intent_at IS NOT NULL FROM handoff_dispatches WHERE dispatch_id=?1",params![dispatch],|r|r.get(0)).map_err(db_error)?;
            if written || !matches!(d.phase.as_str(),"CLAIMED"|"PREPARING"|"UNKNOWN") || load(&tx,dispatch,"ACK_LINK")?.is_some() || load(&tx,dispatch,"CONFLICT")?.is_some() {return Err("PREWRITE_PROOF_UNAVAILABLE".into());}
            if let Some(old)=load(&tx,dispatch,"PREWRITE_STOP")? {validate(&old,key)?;bound(&old,&d,&s)?;return Ok(old);}
            let mut receipt=NativeReceipt{id:id(),dispatch_id:dispatch.into(),kind:"PREWRITE_STOP".into(),backend_id:s.policy.backend_id,native_version:s.policy.native_version,proof_source:"PREWRITE_GATE".into(),adapter_epoch:proof.adapter_epoch.clone(),request_id:d.request_id.clone().unwrap_or(Value::Null),thread_id:s.thread_id,turn_id:None,item_id:None,payload_hash:s.payload_hash,native_status:None,event_sequence:None,evidence_digest:String::new(),integrity_mac:String::new(),recorded_at:now()};
            authenticate(&mut receipt,key)?;insert(&tx,&receipt)?;
            tx.execute("UPDATE handoff_dispatches SET epoch_sealed_at=?2,phase='UNKNOWN',revision=revision+1,updated_at=?2 WHERE dispatch_id=?1",params![dispatch,now()]).map_err(db_error)?;
            tx.execute("UPDATE provider_runs SET status='UNKNOWN',updated_at=?2 WHERE id=?1",params![d.run_id,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;Ok(receipt)
        })
    }
    /// Receipt transaction is deliberately separate from acceptance state. A
    /// crash between them leaves verifiable identity for browser resolution.
    pub fn record_native_ack(
        &self,
        dispatch: &str,
        epoch: &str,
        ack: &NativeTurnAck,
        key: &[u8; 32],
    ) -> Result<NativeReceipt, String> {
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let d=record(&tx,dispatch)?;check_epoch(&d,epoch)?;let s=approved(&tx,&d)?;
            if !matches!(d.phase.as_str(),"WRITE_INTENT"|"UNKNOWN"|"ACCEPTED"|"TERMINAL") || d.request_id.as_ref()!=Some(&ack.request_id) || ack.thread_id!=s.thread_id || ack.adapter_epoch!=epoch {return Err("EVIDENCE_CONFLICT".into());}
            if let Some(old)=load(&tx,dispatch,"ACK_LINK")? {
                validate(&old,key)?;bound(&old,&d,&s)?;
                if old.turn_id.as_deref()!=Some(ack.turn_id.as_str()) {return conflict(tx,&d,&s,key,"ACK_CHANGED");}
                return Ok(old);
            }
            let conflicting:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM dispatch_native_evidence WHERE kind='ACK_LINK' AND backend_id=?1 AND thread_id=?2 AND turn_id=?3 AND dispatch_id!=?4)",params![s.policy.backend_id,ack.thread_id,ack.turn_id,dispatch],|r|r.get(0)).map_err(db_error)?;
            if conflicting {return conflict(tx,&d,&s,key,"TURN_ALREADY_ATTACHED");}
            let mut receipt=NativeReceipt{id:id(),dispatch_id:dispatch.into(),kind:"ACK_LINK".into(),backend_id:s.policy.backend_id,native_version:s.policy.native_version,proof_source:"STDIO_ACK".into(),adapter_epoch:epoch.into(),request_id:ack.request_id.clone(),thread_id:ack.thread_id.clone(),turn_id:Some(ack.turn_id.clone()),item_id:None,payload_hash:s.payload_hash,native_status:None,event_sequence:None,evidence_digest:String::new(),integrity_mac:String::new(),recorded_at:now()};
            authenticate(&mut receipt,key)?;insert(&tx,&receipt)?;tx.execute("UPDATE handoff_dispatches SET revision=revision+1,updated_at=?2 WHERE dispatch_id=?1",params![dispatch,now()]).map_err(db_error)?;tx.commit().map_err(db_error)?;Ok(receipt)
        })
    }
    pub fn accept_native_ack(
        &self,
        dispatch: &str,
        epoch: &str,
        key: &[u8; 32],
    ) -> Result<(), String> {
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let d=record(&tx,dispatch)?;check_epoch(&d,epoch)?;let s=approved(&tx,&d)?;
            let ack=load(&tx,dispatch,"ACK_LINK")?.ok_or("EVIDENCE_REQUIRED")?;validate(&ack,key)?;bound(&ack,&d,&s)?;
            if load(&tx,dispatch,"CONFLICT")?.is_some() {return Err("EVIDENCE_CONFLICT".into());}
            if matches!(d.phase.as_str(),"ACCEPTED"|"TERMINAL") {return Ok(());}
            // UNKNOWN is intentionally browser-only, even if a late ACK exists.
            if d.phase!="WRITE_INTENT" {return Err("OPERATOR_RESOLUTION_REQUIRED".into());}
            tx.execute("UPDATE provider_runs SET external_run_id=?2,status='RUNNING',started_at=?3,updated_at=?3 WHERE id=?1",params![d.run_id,ack.turn_id,now()]).map_err(db_error)?;
            tx.execute("UPDATE handoffs SET status='SENT',sent_at=?2 WHERE id=?1",params![d.handoff_id,now()]).map_err(db_error)?;
            tx.execute("UPDATE handoff_dispatches SET phase='ACCEPTED',revision=revision+1,updated_at=?2 WHERE dispatch_id=?1",params![dispatch,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)
        })
    }
    /// Exact live terminal only. The caller's accumulator was associated using
    /// the matching ACK; this transaction independently verifies that linkage.
    pub fn persist_live_terminal(
        &self,
        dispatch: &str,
        epoch: &str,
        terminal: &ExactTerminal,
        sequence: i64,
        key: &[u8; 32],
    ) -> Result<(), String> {
        if sequence < 0
            || !matches!(
                terminal.status.as_str(),
                "COMPLETED" | "FAILED" | "CANCELLED"
            )
        {
            return Err("CAPABILITY_UNVERIFIED".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let d=record(&tx,dispatch)?;check_epoch(&d,epoch)?;let s=approved(&tx,&d)?;
            let ack=load(&tx,dispatch,"ACK_LINK")?.ok_or("EVIDENCE_REQUIRED")?;validate(&ack,key)?;bound(&ack,&d,&s)?;
            if terminal.thread_id!=ack.thread_id || Some(&terminal.turn_id)!=ack.turn_id.as_ref() {return conflict(tx,&d,&s,key,"TERMINAL_ID_CHANGED");}
            if let Some(old)=load(&tx,dispatch,"TERMINAL_LINK")? {
                validate(&old,key)?;
                if old.native_status.as_deref()!=Some(&terminal.status) || old.item_id!=terminal.item_id || old.evidence_digest!=length_prefixed_hash(&[b"terminal-v1",serde_json::to_vec(terminal).map_err(|_|"RESULT_INVALID")?.as_slice()]) {return conflict(tx,&d,&s,key,"TERMINAL_CHANGED");}
                return Ok(());
            }
            if !matches!(d.phase.as_str(),"ACCEPTED"|"WRITE_INTENT"|"UNKNOWN") {return Err("EVIDENCE_CONFLICT".into());}
            let mut receipt=ack.clone();receipt.id=id();receipt.kind="TERMINAL_LINK".into();receipt.proof_source="STDIO_TERMINAL".into();receipt.native_status=Some(terminal.status.clone());receipt.item_id=terminal.item_id.clone();receipt.event_sequence=Some(sequence);receipt.recorded_at=now();receipt.evidence_digest=length_prefixed_hash(&[b"terminal-v1",serde_json::to_vec(terminal).map_err(|_|"RESULT_INVALID")?.as_slice()]);authenticate(&mut receipt,key)?;insert(&tx,&receipt)?;
            let state=serde_json::to_value(&terminal.result_state).map_err(|_|"INTERNAL")?.as_str().ok_or("INTERNAL")?.to_owned();
            if state=="AVAILABLE" {
                let text=terminal.text.as_ref().ok_or("RESULT_INVALID")?;
                if text.len()>1024*1024 || Some(text.len())!=terminal.bytes || terminal.hash.as_deref()!=Some(format!("{:x}",Sha256::digest(text.as_bytes())).as_str()) {return Err("RESULT_INVALID".into());}
            } else if terminal.text.is_some() {return Err("RESULT_INVALID".into());}
            // One retained final result per Workstream/provider. Identity and
            // outcome of earlier runs remain readable with NOT_RETAINED state.
            tx.execute("UPDATE provider_result_details SET result_state='NOT_RETAINED',updated_at=?3 WHERE run_id IN (SELECT id FROM provider_runs WHERE workstream_id=?1 AND provider='CODEX' AND id!=?2 AND result_text IS NOT NULL)",params![s.workstream_id,d.run_id,now()]).map_err(db_error)?;
            tx.execute("UPDATE provider_runs SET result_text=NULL WHERE workstream_id=?1 AND provider='CODEX' AND id!=?2 AND result_text IS NOT NULL",params![s.workstream_id,d.run_id]).map_err(db_error)?;
            tx.execute("UPDATE provider_runs SET external_run_id=COALESCE(?2,external_run_id),status=?3,result_text=?4,result_identity=?5,terminal_code=?6,terminal_at=?7,started_at=COALESCE(started_at,?7),updated_at=?8 WHERE id=?1",params![d.run_id,if d.phase=="UNKNOWN" { None } else { Some(terminal.turn_id.as_str()) },if d.phase=="UNKNOWN" { "UNKNOWN" } else { terminal.status.as_str() },terminal.text,terminal.item_id,terminal.code,if d.phase=="UNKNOWN" { None } else { Some(now()) },now()]).map_err(db_error)?;
            tx.execute("UPDATE provider_result_details SET result_state=?2,source=?3,final_item_id=?4,final_item_phase=?5,result_hash=?6,result_bytes=?7,terminal_evidence_id=?8,result_error_code=?9,updated_at=?10 WHERE run_id=?1",params![d.run_id,state,terminal.source,terminal.item_id,terminal.phase,terminal.hash,terminal.bytes.map(|b|b as i64),receipt.id,terminal.code,now()]).map_err(db_error)?;
            if d.phase=="UNKNOWN" {tx.execute("UPDATE handoff_dispatches SET revision=revision+1,updated_at=?2 WHERE dispatch_id=?1",params![dispatch,now()]).map_err(db_error)?;tx.commit().map_err(db_error)?;return Ok(());}
            tx.execute("UPDATE handoffs SET status='SENT',sent_at=COALESCE(sent_at,?2) WHERE id=?1",params![d.handoff_id,now()]).map_err(db_error)?;
            tx.execute("UPDATE handoff_dispatches SET phase='TERMINAL',revision=revision+1,updated_at=?2 WHERE dispatch_id=?1",params![dispatch,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)
        })
    }
}
