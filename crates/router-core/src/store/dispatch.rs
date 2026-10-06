//! Durable dispatch gates. Transactions never wait on a native process.
pub mod evidence;
pub mod exit;
pub mod resolution;
use super::control::ReviewSnapshot;
use super::*;
use serde_json::Value;

#[derive(Clone, Debug, Serialize)]
pub struct DispatchRecord {
    pub dispatch_id: String,
    pub run_id: String,
    pub handoff_id: String,
    pub adapter_epoch: String,
    pub revision: i64,
    pub phase: String,
    /// Bounded Router classification for the original dispatch.  This is
    /// status evidence only; it never authorizes a retry or replacement.
    pub last_code: Option<String>,
    pub request_id: Option<Value>,
}

/// A durable, already-dispatched reverse relay which may be completed only by
/// reading exact Bridge terminal evidence.  This is deliberately not a claim
/// or recovery grant: callers cannot use it to send, retry, or replace a
/// dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingReverseRelay {
    pub owner: String,
    pub handoff_id: String,
}
fn record(c: &Connection, dispatch: &str) -> Result<DispatchRecord, String> {
    c.query_row("SELECT dispatch_id,run_id,handoff_id,adapter_epoch,revision,phase,last_code,native_request_id_json FROM handoff_dispatches WHERE dispatch_id=?1",params![dispatch],|r| {
        let raw: Option<String> = r.get(7)?;
        let request_id = raw.map(|s| serde_json::from_str(&s)).transpose().map_err(|e|rusqlite::Error::FromSqlConversionFailure(7,rusqlite::types::Type::Text,Box::new(e)))?;
        Ok(DispatchRecord{dispatch_id:r.get(0)?,run_id:r.get(1)?,handoff_id:r.get(2)?,adapter_epoch:r.get(3)?,revision:r.get(4)?,phase:r.get(5)?,last_code:r.get(6)?,request_id})
    }).optional().map_err(db_error)?.ok_or("NOT_FOUND".into())
}
fn check_epoch(d: &DispatchRecord, epoch: &str) -> Result<(), String> {
    if d.adapter_epoch != epoch {
        return Err("STALE_ADAPTER_EPOCH".into());
    }
    Ok(())
}
impl RouterStore {
    pub fn dispatch_record(&self, dispatch: &str) -> Result<DispatchRecord, String> {
        self.with_connection(|c| record(c, dispatch))
    }
    /// Returns the single durable dispatch already claimed for one relay
    /// handoff.  This is a readback helper only: it never claims, resumes, or
    /// creates a replacement dispatch.
    pub fn relay_dispatch_record(
        &self,
        owner: &str,
        handoff_id: &str,
    ) -> Result<Option<DispatchRecord>, String> {
        self.with_connection(|c| {
            let dispatch: Option<String> = c
                .query_row(
                    "SELECT x.dispatch_id FROM handoff_dispatches x JOIN relay_handoff_details d ON d.handoff_id=x.handoff_id WHERE x.handoff_id=?1 AND d.owner_principal_key=?2",
                    params![handoff_id, owner],
                    |r| r.get(0),
                )
                .optional()
                .map_err(db_error)?;
            dispatch.map(|id| record(c, &id)).transpose()
        })
    }

    /// Lists a bounded set of existing CODEX_TO_CHATGPT dispatches which have
    /// crossed the user-acceptance boundary but have no terminal evidence yet.
    /// It is used solely to resume exact, read-only terminal reconciliation
    /// after a Review-page session expires or the Router restarts.
    pub fn pending_reverse_relays(&self, limit: usize) -> Result<Vec<PendingReverseRelay>, String> {
        if !(1..=64).contains(&limit) {
            return Err("INVALID_RELAY_RECONCILIATION_LIMIT".into());
        }
        self.with_connection(|c| {
            let mut statement = c
                .prepare(
                    "SELECT d.owner_principal_key,x.handoff_id \
                     FROM handoff_dispatches x \
                     JOIN relay_handoff_details d ON d.handoff_id=x.handoff_id \
                     JOIN handoffs h ON h.id=x.handoff_id \
                     JOIN provider_runs p ON p.id=x.run_id \
                     WHERE h.direction='CODEX_TO_CHATGPT' \
                       AND x.phase IN ('ACCEPTED','UNKNOWN') \
                       AND p.external_run_id IS NOT NULL \
                     ORDER BY x.updated_at ASC \
                     LIMIT ?1",
                )
                .map_err(db_error)?;
            let rows = statement
                .query_map(params![limit as i64], |row| {
                    Ok(PendingReverseRelay {
                        owner: row.get(0)?,
                        handoff_id: row.get(1)?,
                    })
                })
                .map_err(db_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
        })
    }
    /// Takes a previously claimed dispatch once. An error never grants a retry.
    pub fn begin_dispatch_preparation(
        &self,
        dispatch: &str,
        epoch: &str,
    ) -> Result<ReviewSnapshot, String> {
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let d=record(&tx,dispatch)?;check_epoch(&d,epoch)?;
            if d.phase!="CLAIMED" {return Err("DISPATCH_ALREADY_ATTEMPTED".into());}
            let principal:String=tx.query_row("SELECT approved_principal_key FROM control_handoff_details WHERE handoff_id=?1 AND consumed_at IS NOT NULL",params![d.handoff_id],|r|r.get(0)).map_err(db_error)?;
            let snapshot=control::snapshot(&tx,&principal,&d.handoff_id)?;
            let exiting:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM runtime_exit_state WHERE phase!='RUNNING')",[],|r|r.get(0)).map_err(db_error)?;
            if exiting {return Err("EXIT_PENDING".into());}
            tx.execute("UPDATE handoff_dispatches SET phase='PREPARING',revision=revision+1,updated_at=?2 WHERE dispatch_id=?1",params![dispatch,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;Ok(snapshot)
        })
    }
    /// Called synchronously with the adapter's exact reserved JSON-RPC identity,
    /// immediately before its first stdin write. No native I/O occurs in this TX.
    pub fn persist_write_intent(
        &self,
        dispatch: &str,
        epoch: &str,
        request_id: &Value,
        approved: &ReviewSnapshot,
    ) -> Result<(), String> {
        if !(request_id.as_u64().is_some()
            || request_id
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 128))
        {
            return Err("INVALID_NATIVE_REQUEST_ID".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let d=record(&tx,dispatch)?;check_epoch(&d,epoch)?;
            if d.phase!="PREPARING" || d.request_id.is_some() || d.handoff_id!=approved.draft.handoff_id {return Err("DISPATCH_ALREADY_ATTEMPTED".into());}
            let principal:String=tx.query_row("SELECT approved_principal_key FROM control_handoff_details WHERE handoff_id=?1 AND consumed_at IS NOT NULL",params![d.handoff_id],|r|r.get(0)).map_err(db_error)?;
            let current=control::snapshot(&tx,&principal,&d.handoff_id)?;
            if current.draft.payload_hash!=approved.draft.payload_hash || current.thread_id!=approved.thread_id || current.policy!=approved.policy || current.text!=approved.text || current.canonical_path!=approved.canonical_path {return Err("STALE_APPROVAL".into());}
            let exiting:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM runtime_exit_state WHERE phase!='RUNNING')",[],|r|r.get(0)).map_err(db_error)?;
            if exiting {return Err("EXIT_PENDING".into());}
            tx.execute("UPDATE handoff_dispatches SET phase='WRITE_INTENT',native_request_id_json=?2,write_intent_at=?3,revision=revision+1,updated_at=?3 WHERE dispatch_id=?1",params![dispatch,request_id.to_string(),now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)
        })
    }
    /// Conservative failure preserves the original approval, run, and lock.
    pub fn mark_dispatch_unknown(
        &self,
        dispatch: &str,
        epoch: &str,
        code: &str,
    ) -> Result<(), String> {
        if code.is_empty()
            || code.len() > 100
            || !code
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b == b'_' || b.is_ascii_digit())
        {
            return Err("INVALID_ERROR_CODE".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let d=record(&tx,dispatch)?;check_epoch(&d,epoch)?;
            if matches!(d.phase.as_str(),"TERMINAL"|"NOT_SENT") {return Err("ALREADY_TERMINAL".into());}
            if d.phase!="UNKNOWN" {
                tx.execute("UPDATE handoff_dispatches SET phase='UNKNOWN',revision=revision+1,last_code=?2,updated_at=?3 WHERE dispatch_id=?1",params![dispatch,code,now()]).map_err(db_error)?;
                tx.execute("UPDATE provider_runs SET status='UNKNOWN',terminal_code=?2,updated_at=?3 WHERE id=?1",params![d.run_id,code,now()]).map_err(db_error)?;
            }
            tx.commit().map_err(db_error)
        })
    }
    /// Call exactly once after acquiring the exclusive profile lock, before any
    /// serving or native startup. A new epoch never clears an unresolved lock.
    pub fn recover_preview_startup(&self, new_epoch: &str) -> Result<usize, String> {
        Uuid::parse_str(new_epoch).map_err(|_| "INVALID_ADAPTER_EPOCH")?;
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let previous:Option<String>=tx.query_row("SELECT adapter_epoch FROM runtime_exit_state WHERE singleton=1",[],|r|r.get(0)).optional().map_err(db_error)?;
            if previous.as_deref()==Some(new_epoch) {return Err("EPOCH_ALREADY_STARTED".into());}
            tx.execute("UPDATE provider_runs SET status='UNKNOWN',terminal_code='RESTART_UNCERTAIN',updated_at=?1 WHERE id IN (SELECT run_id FROM handoff_dispatches WHERE phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED'))",params![now()]).map_err(db_error)?;
            let count=tx.execute("UPDATE handoff_dispatches SET phase='UNKNOWN',revision=revision+1,last_code='RESTART_UNCERTAIN',updated_at=?1 WHERE phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED')",params![now()]).map_err(db_error)?;
            tx.execute("UPDATE binding_requests SET state='UNKNOWN' WHERE state IN ('CREATING','VERIFYING','READY') AND operation='CREATE'",[]).map_err(db_error)?;
            // No old browser challenge survives a process restart. Unresolved
            // dispatch records retain their original adapter epoch and evidence.
            tx.execute("INSERT INTO runtime_exit_state(singleton,revision,adapter_epoch,phase,unresolved_set_hash) VALUES(1,0,?1,'RUNNING','') ON CONFLICT(singleton) DO UPDATE SET revision=revision+1,adapter_epoch=excluded.adapter_epoch,phase='RUNNING',unresolved_set_hash='',operator_principal_key=NULL,nonce_hash=NULL,quiesced_at=NULL,sealed_at=NULL",params![new_epoch]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;Ok(count)
        })
    }
}
