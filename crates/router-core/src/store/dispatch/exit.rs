//! Browser maintenance exit, independent from UNKNOWN resolution.
use super::*;
use crate::identity::length_prefixed_hash;

#[derive(Clone, Debug, Serialize)]
pub struct ExitPreview {
    pub revision: i64,
    pub unresolved_count: usize,
    pub unresolved_set_hash: String,
    pub adapter_epoch: String,
}
pub struct ExitGrant {
    pub principal_key: String,
    pub preview: ExitPreview,
    pub nonce_hash: String,
    pub expires_at: i64,
}
fn preview(c: &Connection) -> Result<ExitPreview, String> {
    let (revision, epoch): (i64, String) = c
        .query_row(
            "SELECT revision,adapter_epoch FROM runtime_exit_state WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(db_error)?;
    let mut statement=c.prepare("SELECT dispatch_id,revision,phase FROM handoff_dispatches WHERE phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN') ORDER BY dispatch_id").map_err(db_error)?;
    let records = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    let mut creation=c.prepare("SELECT id,state FROM binding_requests WHERE operation='CREATE' AND state IN ('CREATING','VERIFYING','READY','UNKNOWN') ORDER BY id").map_err(db_error)?;
    let creating = creation
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    let count = records.len() + creating.len();
    let encoded = serde_json::to_vec(&(records, creating)).map_err(|_| "INTERNAL")?;
    Ok(ExitPreview {
        revision,
        adapter_epoch: epoch,
        unresolved_count: count,
        unresolved_set_hash: length_prefixed_hash(&[b"exit-unresolved-v1", &encoded]),
    })
}
impl RouterStore {
    pub fn verify_runtime_running(&self, epoch: &str) -> Result<(), String> {
        self.with_connection(|c|{let running:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM runtime_exit_state WHERE singleton=1 AND adapter_epoch=?1 AND phase='RUNNING')",params![epoch],|r|r.get(0)).map_err(db_error)?;if running{Ok(())}else{Err("EXIT_PENDING".into())}})
    }

    pub fn exit_preview(&self) -> Result<ExitPreview, String> {
        self.with_connection(|c| preview(c))
    }
    /// The HTTP layer supplies owner authority and a session-bound nonce. This
    /// method has no MCP route and never treats exit as proof of non-delivery.
    pub fn quiesce_preview_runtime(&self, grant: &ExitGrant) -> Result<String, String> {
        if grant.principal_key.len() != 64
            || grant.nonce_hash.len() != 64
            || !grant.nonce_hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("FORBIDDEN".into());
        }
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let (phase,nonce,principal):(String,Option<String>,Option<String>)=tx.query_row("SELECT phase,nonce_hash,operator_principal_key FROM runtime_exit_state WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(db_error)?;
            if nonce.as_deref()==Some(&grant.nonce_hash) && principal.as_deref()==Some(&grant.principal_key) && phase!="RUNNING" {return Ok(phase);}
            if grant.expires_at<now() {return Err("EXPIRED".into());}
            let current=preview(&tx)?;
            if current.revision!=grant.preview.revision || current.adapter_epoch!=grant.preview.adapter_epoch || current.unresolved_set_hash!=grant.preview.unresolved_set_hash {return Err("STALE_RESOLUTION".into());}
            if phase!="RUNNING" {return Err("EXIT_PENDING".into());}
            tx.execute("UPDATE runtime_exit_state SET phase='QUIESCED',revision=revision+1,unresolved_set_hash=?1,operator_principal_key=?2,nonce_hash=?3,quiesced_at=?4 WHERE singleton=1",params![current.unresolved_set_hash,grant.principal_key,grant.nonce_hash,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;Ok("QUIESCED".into())
        })
    }
    /// A true value is supplied only after all owned children have normally
    /// exited and their streams drained. DB failure means the host stays alive.
    pub fn finish_preview_exit(
        &self,
        runtime_epoch: &str,
        children_drained: bool,
    ) -> Result<String, String> {
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let (phase,epoch):(String,String)=tx.query_row("SELECT phase,adapter_epoch FROM runtime_exit_state WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?))).map_err(db_error)?;
            if epoch!=runtime_epoch {return Err("STALE_ADAPTER_EPOCH".into());}
            if !matches!(phase.as_str(),"QUIESCED"|"EXIT_PENDING") {return Err("EXIT_NOT_QUIESCED".into());}
            let next=if children_drained {"SEALED"}else{"EXIT_PENDING"};
            if children_drained {
                tx.execute("UPDATE provider_runs SET status='UNKNOWN',updated_at=?1,terminal_code='EXIT_UNRESOLVED' WHERE id IN (SELECT run_id FROM handoff_dispatches WHERE phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED'))",params![now()]).map_err(db_error)?;
                tx.execute("UPDATE handoff_dispatches SET phase='UNKNOWN',revision=revision+1,updated_at=?1,last_code='EXIT_UNRESOLVED' WHERE phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED')",params![now()]).map_err(db_error)?;
                tx.execute("UPDATE binding_requests SET state='UNKNOWN' WHERE operation='CREATE' AND state IN ('CREATING','VERIFYING','READY')",[]).map_err(db_error)?;
            }
            tx.execute("UPDATE runtime_exit_state SET phase=?1,revision=revision+1,sealed_at=?2 WHERE singleton=1",params![next,if children_drained{Some(now())}else{None}]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;Ok(next.into())
        })
    }
}
