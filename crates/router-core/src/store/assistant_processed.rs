//! Read-only attribution of an actual, acknowledged assistant handoff.
use super::*;

pub const ASSISTANT_PROCESSED_TTL_MS:i64=5*60*1000;
#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct AssistantProcessed {
 pub handoff_id:String,
 pub observation_id:String,
 pub source_role:String,
 pub destination_endpoint_id:String,
 pub destination_turn_id:Option<String>,
 pub sent_at:i64,
 pub expires_at:i64,
}
pub(super) fn migrate(c:&Connection)->Result<(),String>{
 c.execute_batch("CREATE TABLE IF NOT EXISTS assistant_sent_handoffs(handoff_id TEXT PRIMARY KEY REFERENCES handoffs(id) ON DELETE CASCADE,grant_id TEXT NOT NULL REFERENCES assistant_grants(id),recorded_at INTEGER NOT NULL);").map_err(db_error)
}
impl RouterStore {
 /// Called only after the authenticated assistant physical-send path succeeds
 /// (possibly awaiting a ChatGPT terminal receipt). Projection still requires SENT.
 /// Preparing/reviewing alone, a user send, or a guessed historical actor never
 /// creates an assistant-processed badge.
 pub fn note_assistant_sent(&self,grant:&str,handoff:&str)->Result<(),String>{self.with_connection(|c|{
  let eligible:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM handoffs h JOIN assistant_approvals a ON a.handoff_id=h.id AND a.payload_hash=h.payload_hash WHERE h.id=?1 AND h.status IN ('SENT','SENDING') AND a.grant_id=?2)",params![handoff,grant],|r|r.get(0)).map_err(db_error)?;
  if !eligible{return Err("ASSISTANT_SEND_NOT_CONFIRMED".into());}
  c.execute("INSERT OR IGNORE INTO assistant_sent_handoffs(handoff_id,grant_id,recorded_at) VALUES(?1,?2,?3)",params![handoff,grant,now()]).map_err(db_error)?;Ok(())
 })}

 pub fn assistant_processed(&self,workstream:&str,at:i64)->Result<Option<AssistantProcessed>,String>{
  self.with_connection(|c|c.query_row(
   "SELECT h.id,r.id,d.source_role,h.destination_endpoint_id,(SELECT p.external_run_id FROM provider_runs p WHERE p.origin_handoff_id=h.id AND p.endpoint_id=h.destination_endpoint_id ORDER BY p.created_at DESC LIMIT 1),h.sent_at FROM handoffs h JOIN assistant_sent_handoffs a ON a.handoff_id=h.id JOIN role_handoff_details d ON d.handoff_id=h.id JOIN workstreams w ON w.id=h.workstream_id JOIN endpoints src ON src.id=h.source_endpoint_id JOIN endpoints dst ON dst.id=h.destination_endpoint_id JOIN reply_observations r ON r.workstream_id=h.workstream_id AND r.endpoint_id=h.source_endpoint_id AND r.assistant_identity=h.source_response_identity WHERE h.workstream_id=?1 AND h.status='SENT' AND h.sent_at<=?2 AND h.sent_at>?3 AND d.binding_revision=w.binding_revision AND w.trashed_at IS NULL AND w.archived_at IS NULL AND src.status='ACTIVE' AND dst.status='ACTIVE' AND NOT EXISTS(SELECT 1 FROM reply_observations newer JOIN endpoints e ON e.id=newer.endpoint_id WHERE newer.workstream_id=w.id AND e.status='ACTIVE' AND newer.observed_at>h.sent_at AND newer.handled_at IS NULL) ORDER BY h.sent_at DESC,h.id LIMIT 1",
   params![workstream,at,at-ASSISTANT_PROCESSED_TTL_MS],|r|{let sent_at=r.get(5)?;Ok(AssistantProcessed{handoff_id:r.get(0)?,observation_id:r.get(1)?,source_role:r.get(2)?,destination_endpoint_id:r.get(3)?,destination_turn_id:r.get(4)?,sent_at,expires_at:sent_at+ASSISTANT_PROCESSED_TTL_MS})}
  ).optional().map_err(db_error))
 }
}
