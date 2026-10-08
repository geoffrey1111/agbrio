//! Owner-issued delegation. Transport authentication never creates a grant.
use super::*;
use serde::Deserialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BriefRule { pub id: String, pub text: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantInput {
    pub workstream_id: String, pub source_role: String, pub binding_revision: i64,
    pub label: String, pub rules: Vec<BriefRule>, pub expires_at: i64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantGrant {
    pub id: String, pub workstream_id: String, pub source_role: String, pub scope:String,
    pub binding_revision: i64, pub label: String, pub rules: Vec<BriefRule>,
    pub created_at: i64, pub expires_at: i64, pub revoked_at: Option<i64>,
    pub approval_mode: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalInput {
    pub handoff_id: String, pub expected_hash: String,
    pub rule_id: Option<String>, pub decision_id: Option<String>, pub assessment: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDecision {
    pub id: String, pub handoff_id: String, pub payload_hash: String, pub question: String,
    pub answer: Option<String>, pub answer_reference: Option<String>,
}
pub(super) fn bounded(value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max { Err("ASSISTANT_INPUT_INVALID".into()) } else { Ok(()) }
}
fn grant(c: &Connection, id: &str) -> Result<AssistantGrant, String> {
    let mut g: AssistantGrant = c.query_row(
        "SELECT id,workstream_id,source_role,binding_revision,label,brief_json,created_at,expires_at,revoked_at,scope,approval_mode FROM assistant_grants WHERE id=?1", [id], |r| {
            let rules: String = r.get(5)?;
            Ok(AssistantGrant { id:r.get(0)?,workstream_id:r.get::<_,Option<String>>(1)?.unwrap_or_default(),source_role:r.get(2)?,binding_revision:r.get(3)?,label:r.get(4)?,
                rules: serde_json::from_str(&rules).map_err(|e|rusqlite::Error::FromSqlConversionFailure(5,rusqlite::types::Type::Text,Box::new(e)))?,
                created_at:r.get(6)?,expires_at:r.get(7)?,revoked_at:r.get(8)?,scope:r.get(9)?,approval_mode:r.get(10)? })
        }).map_err(|_| "ASSISTANT_GRANT_UNAVAILABLE".to_string())?;
    // The serialized version is the immutable grant ID, not an editable label.
    g.rules.shrink_to_fit(); Ok(g)
}
pub(super) fn active(c: &Connection, id: &str) -> Result<AssistantGrant, String> {
    let g = grant(c,id)?;
    if g.revoked_at.is_some() || g.expires_at <= now() { return Err("ASSISTANT_GRANT_REVOKED_OR_EXPIRED".into()); }
    if g.scope=="INSTANCE"{return Ok(g);}
    let w = workstream_by_id(c,&g.workstream_id)?;
    if w.trashed_at.is_some() || w.archived_at.is_some() || w.binding_revision != g.binding_revision {
        return Err("ASSISTANT_BRIDGE_CHANGED_OR_REMOVED".into());
    }
    Ok(g)
}
fn draft(c: &Connection, gid: &str, hid: &str, hash: &str) -> Result<(AssistantGrant,HandoffHistoryItem),String> {
    let g = active(c,gid)?;
    let (owner,obs,head):(String,String,Option<String>) = c.query_row("SELECT grant_id,observation_id,review_head_id FROM assistant_drafts WHERE handoff_id=?1",[hid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))
        .map_err(|_|"ASSISTANT_DRAFT_UNAVAILABLE".to_string())?;
    let h = handoff_by_id(c,hid)?;
    if owner != gid || (g.scope!="INSTANCE"&&h.workstream_id != g.workstream_id) || h.payload_hash != hash { return Err("ASSISTANT_DRAFT_CHANGED_OR_OUT_OF_SCOPE".into()); }
    role_bridge::validate_handoff(c,&h)?;
    let latest:String = c.query_row("SELECT id FROM reply_observations WHERE endpoint_id=?1 ORDER BY observed_at DESC,rowid DESC LIMIT 1",
        [h.endpoint_source()?.id.as_str()],|r|r.get(0)).map_err(db_error)?;
    if latest != head.as_deref().unwrap_or(&obs) { return Err("ASSISTANT_SOURCE_CHANGED".into()); }
    let role:String = c.query_row("SELECT source_role FROM role_handoff_details WHERE handoff_id=?1",[hid],|r|r.get(0)).map_err(db_error)?;
    if g.source_role!="BOTH" && role != g.source_role { return Err("ASSISTANT_DIRECTION_OUT_OF_SCOPE".into()); }
    Ok((g,h))
}
/// Called inside the same transaction as the existing single-attempt writer claim.
pub(super) fn validate_claim(c: &Connection,h: &HandoffHistoryItem) -> Result<(),String> {
    let record:Option<(String,String)> = c.query_row("SELECT grant_id,payload_hash FROM assistant_approvals WHERE handoff_id=?1",[&h.id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
    if let Some((gid,hash)) = record { draft(c,&gid,&h.id,&hash)?; }
    Ok(())
}
impl RouterStore {
    /// Owner-only entry point. MCP does not expose this operation.
    pub fn create_assistant_grant(&self,input:GrantInput)->Result<AssistantGrant,String> {
        bounded(&input.label,80)?;
        if input.rules.is_empty() || input.rules.len()>30 || input.expires_at<=now() || input.expires_at>now()+90*86_400_000 {
            return Err("ASSISTANT_GRANT_INVALID".into());
        }
        let mut ids=std::collections::HashSet::new();
        for rule in &input.rules { bounded(&rule.id,80)?;bounded(&rule.text,8000)?;if !ids.insert(&rule.id){return Err("ASSISTANT_RULE_DUPLICATE".into());} }
        let rules=serde_json::to_string(&input.rules).map_err(|_|"ASSISTANT_BRIEF_INVALID")?;
        if rules.len()>32000 {return Err("ASSISTANT_BRIEF_INVALID".into());}
        self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let b=role_bridge::bridge(&tx,&input.workstream_id)?;
            let w=workstream_by_id(&tx,&input.workstream_id)?;
            if b.binding_revision!=input.binding_revision || w.trashed_at.is_some() || w.archived_at.is_some(){return Err("ASSISTANT_BRIDGE_CHANGED_OR_REMOVED".into());}
            if !matches!(input.source_role.as_str(),"DECISION"|"EXECUTION"|"BOTH") || b.decision.is_none() || b.execution.is_none(){return Err("ASSISTANT_BRIDGE_NOT_BOUND".into());}
            let gid=id();
            tx.execute("INSERT INTO assistant_grants(id,workstream_id,source_role,binding_revision,label,brief_json,created_at,expires_at,revoked_at,scope) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,NULL,'BRIDGE')",params![gid,input.workstream_id,input.source_role,input.binding_revision,input.label,rules,now(),input.expires_at]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;grant(c,&gid)
        })
    }
    pub fn assistant_grants(&self)->Result<Vec<AssistantGrant>,String>{self.with_connection(|c|{
        let ids=c.prepare("SELECT id FROM assistant_grants ORDER BY created_at DESC,rowid DESC").map_err(db_error)?.query_map([],|r|r.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
        ids.iter().map(|id|grant(c,id)).collect()
    })}
    pub fn assistant_grant(&self,gid:&str)->Result<AssistantGrant,String>{self.with_connection(|c|active(c,gid))}
    pub fn revoke_assistant_grant(&self,gid:&str)->Result<(),String>{self.with_connection(|c|{
        if c.execute("UPDATE assistant_grants SET revoked_at=COALESCE(revoked_at,?2) WHERE id=?1",params![gid,now()]).map_err(db_error)?!=1{return Err("ASSISTANT_GRANT_UNAVAILABLE".into());}Ok(())
    })}
    pub fn issue_assistant_access(&self,gid:&str,resource:&str,expires:i64)->Result<String,String>{self.with_connection(|c|{
        let g=active(c,gid)?;bounded(resource,1024)?;
        if expires<=now()||expires>g.expires_at{return Err("ASSISTANT_TOKEN_EXPIRY_INVALID".into());}
        let token=format!("{}{}",Uuid::new_v4().simple(),Uuid::new_v4().simple());
        c.execute("INSERT INTO assistant_access VALUES(?1,?2,?3,?4)",params![format!("{:x}",Sha256::digest(token.as_bytes())),gid,resource,expires]).map_err(db_error)?;Ok(token)
    })}
    pub fn authenticate_assistant(&self,token:&str,resource:&str)->Result<AssistantGrant,String>{self.with_connection(|c|{
        if token.len()!=64{return Err("ASSISTANT_UNAUTHORIZED".into());}
        let gid:String=c.query_row("SELECT grant_id FROM assistant_access WHERE token_hash=?1 AND resource=?2 AND expires_at>?3",
            params![format!("{:x}",Sha256::digest(token.as_bytes())),resource,now()],|r|r.get(0)).map_err(|_|"ASSISTANT_UNAUTHORIZED".to_string())?;
        active(c,&gid)
    })}
    pub fn register_assistant_draft(&self,gid:&str,hid:&str,observation:&str)->Result<(),String>{self.register_assistant_draft_request(gid,hid,observation,None)}
    /// Ownership and retry identity commit together; a losing concurrent prepare
    /// cannot leave a second draft authorized for assistant approval.
    pub fn register_assistant_prepare(&self,gid:&str,hid:&str,observation:&str,request:&str,hash:&str)->Result<(),String>{
        bounded(request,100)?;bounded(hash,64)?;self.register_assistant_draft_request(gid,hid,observation,Some((request,hash)))
    }
    fn register_assistant_draft_request(&self,gid:&str,hid:&str,observation:&str,request:Option<(&str,&str)>)->Result<(),String>{self.with_connection(|c|{
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        let g=active(&tx,gid)?;let h=handoff_by_id(&tx,hid)?;
        let role:String=tx.query_row("SELECT source_role FROM role_handoff_details WHERE handoff_id=?1",[hid],|r|r.get(0)).map_err(db_error)?;
        if h.status!="READY"||(g.scope!="INSTANCE"&&h.workstream_id!=g.workstream_id)||(g.source_role!="BOTH"&&role!=g.source_role){return Err("ASSISTANT_DRAFT_OUT_OF_SCOPE".into());}
        let source=h.endpoint_source()?;
        let original_identity:String=tx.query_row("SELECT assistant_identity FROM reply_observations WHERE id=?1 AND workstream_id=?2 AND endpoint_id=?3",params![observation,h.workstream_id,source.id],|r|r.get(0)).map_err(db_error)?;
        if h.source_response_identity.as_deref()!=Some(original_identity.as_str()){return Err("ASSISTANT_SOURCE_CHANGED".into());}
        let head:String=if g.approval_mode=="CONVERSATION_REVIEW"{tx.query_row("SELECT id FROM reply_observations WHERE endpoint_id=?1 ORDER BY observed_at DESC,rowid DESC LIMIT 1",[source.id.as_str()],|r|r.get(0)).map_err(db_error)?}else{observation.to_string()};
        tx.execute("INSERT INTO assistant_drafts(handoff_id,grant_id,observation_id,request_id,request_hash,review_head_id) VALUES(?1,?2,?3,?4,?5,?6)",params![hid,gid,observation,request.map(|v|v.0),request.map(|v|v.1),head]).map_err(db_error)?;
        draft(&tx,gid,hid,&h.payload_hash)?;tx.commit().map_err(db_error)
    })}
    pub fn ask_assistant_decision(&self,gid:&str,hid:&str,hash:&str,question:&str)->Result<PendingDecision,String>{
        bounded(question,8000)?;self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let(_,h)=draft(&tx,gid,hid,hash)?;if h.status!="READY"{return Err("ASSISTANT_DRAFT_NOT_READY".into());}
            let existing:Option<String>=tx.query_row("SELECT id FROM assistant_decisions WHERE handoff_id=?1 AND answered_at IS NULL ORDER BY created_at DESC,rowid DESC LIMIT 1",params![hid],|r|r.get(0)).optional().map_err(db_error)?;
            let did=existing.unwrap_or_else(id);
            tx.execute("INSERT OR IGNORE INTO assistant_decisions(id,handoff_id,payload_hash,question,created_at) VALUES(?1,?2,?3,?4,?5)",params![did,hid,hash,question,now()]).map_err(db_error)?;
            tx.execute("UPDATE assistant_decisions SET payload_hash=?2,question=?3,created_at=?4 WHERE id=?1 AND payload_hash!=?2 AND answered_at IS NULL",params![did,hash,question,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;decision(c,&did)
        })
    }
    /// Records a user reply attested by the authorized assistant, not an independent
    /// Agbrio observation of the assistant's conversation. It does not change a brief.
    pub fn answer_assistant_decision(&self,gid:&str,did:&str,hash:&str,answer:&str,reference:&str)->Result<PendingDecision,String>{
        bounded(answer,16000)?;bounded(reference,1024)?;self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let d=decision(&tx,did)?;draft(&tx,gid,&d.handoff_id,hash)?;
            if d.payload_hash!=hash{return Err("ASSISTANT_DECISION_CHANGED".into());}
            if d.answer.is_some(){return Err("ASSISTANT_DECISION_ALREADY_ANSWERED".into());}
            tx.execute("UPDATE assistant_decisions SET answer=?2,answer_reference=?3,answered_at=?4 WHERE id=?1",params![did,answer,reference,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;decision(c,did)
        })
    }
    pub fn assistant_decisions(&self,gid:&str)->Result<Vec<PendingDecision>,String>{self.with_connection(|c|{
        active(c,gid)?;
        let ids=c.prepare("SELECT d.id FROM assistant_decisions d JOIN assistant_drafts h ON h.handoff_id=d.handoff_id WHERE h.grant_id=?1 ORDER BY d.created_at,d.rowid").map_err(db_error)?.query_map([gid],|r|r.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
        ids.iter().map(|id|decision(c,id)).collect()
    })}
    pub fn approve_assistant_handoff(&self,gid:&str,input:ApprovalInput)->Result<HandoffHistoryItem,String>{
        bounded(&input.assessment,8000)?;self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let(g,h)=draft(&tx,gid,&input.handoff_id,&input.expected_hash)?;
            let unanswered:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM assistant_decisions WHERE handoff_id=?1 AND answered_at IS NULL)",params![h.id],|r|r.get(0)).map_err(db_error)?;
            if unanswered{return Err("ASSISTANT_OWNER_DECISION_PENDING".into());}
            let asked:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM assistant_decisions WHERE handoff_id=?1)",params![h.id],|r|r.get(0)).map_err(db_error)?;
            if asked&&input.decision_id.is_none(){return Err("ASSISTANT_OWNER_ANSWER_REQUIRED".into());}
            let(basis,reference)=match(input.rule_id,input.decision_id){
                (None,None) if g.scope=="INSTANCE" && g.approval_mode=="CONVERSATION_REVIEW"=>("ASSISTANT_REVIEW",g.id.clone()),
                (Some(r),None) if g.rules.iter().any(|v|v.id==r)=>("BRIEF_RULE",r),
                (None,Some(did))=>{let d=decision(&tx,&did)?;if d.handoff_id!=h.id||d.payload_hash!=h.payload_hash||d.answer.is_none(){return Err("ASSISTANT_OWNER_ANSWER_REQUIRED".into());}("ASSISTANT_ATTESTED_OWNER_ANSWER",did)},
                _=>return Err("ASSISTANT_APPROVAL_BASIS_REQUIRED".into())};
            if tx.execute("UPDATE handoffs SET status='APPROVED',approved_at=?2 WHERE id=?1 AND status='READY' AND payload_hash=?3",params![h.id,now(),input.expected_hash]).map_err(db_error)?!=1{return Err("BRIDGE_REVIEW_CHANGED_OR_NOT_READY".into());}
            tx.execute("INSERT INTO assistant_approvals VALUES(?1,?2,?3,?4,?5,?6,?7)",params![h.id,gid,h.payload_hash,basis,reference,input.assessment,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;handoff_by_id(c,&h.id)
        })
    }
    pub fn require_assistant_send(&self,gid:&str,hid:&str,hash:&str)->Result<(),String>{self.with_connection(|c|{
        let(_,h)=draft(c,gid,hid,hash)?;
        let approved:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM assistant_approvals WHERE handoff_id=?1 AND grant_id=?2 AND payload_hash=?3)",params![hid,gid,hash],|r|r.get(0)).map_err(db_error)?;
        if !approved||h.status!="APPROVED"{return Err("ASSISTANT_NOT_APPROVED_OR_ALREADY_ATTEMPTED".into());}Ok(())
    })}
    pub fn require_assistant_draft_scope(&self,gid:&str,hid:&str,hash:&str)->Result<(),String>{self.with_connection(|c|{
        let g=active(c,gid)?;let h=handoff_by_id(c,hid)?;
        let owner:String=c.query_row("SELECT grant_id FROM assistant_drafts WHERE handoff_id=?1",[hid],|r|r.get(0)).map_err(|_|"ASSISTANT_DRAFT_UNAVAILABLE")?;
        if owner!=gid||(g.scope!="INSTANCE"&&h.workstream_id!=g.workstream_id)||h.payload_hash!=hash{return Err("ASSISTANT_DRAFT_CHANGED_OR_OUT_OF_SCOPE".into());}Ok(())
    })}
    pub fn assistant_prepare_receipt(&self,gid:&str,request:&str,hash:&str)->Result<Option<HandoffHistoryItem>,String>{
        bounded(request,100)?;self.with_connection(|c|{
            active(c,gid)?;
            let record:Option<(String,String)>=c.query_row("SELECT handoff_id,request_hash FROM assistant_drafts WHERE grant_id=?1 AND request_id=?2",params![gid,request],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
            match record{Some((hid,stored))=>{if stored!=hash{return Err("ASSISTANT_REQUEST_REUSED_WITH_DIFFERENT_BYTES".into());}Ok(Some(handoff_by_id(c,&hid)?))},None=>Ok(None)}
        })
    }
    pub fn assistant_approval(&self,hid:&str)->Result<Option<Value>,String>{self.with_connection(|c|{
        c.query_row("SELECT a.grant_id,g.label,a.payload_hash,a.basis,a.decision_reference,a.assessment,a.created_at FROM assistant_approvals a JOIN assistant_grants g ON g.id=a.grant_id WHERE a.handoff_id=?1",[hid],|r|Ok(serde_json::json!({"kind":"USER_DELEGATED_ASSISTANT","grantId":r.get::<_,String>(0)?,"assistantLabel":r.get::<_,String>(1)?,"payloadHash":r.get::<_,String>(2)?,"basis":r.get::<_,String>(3)?,"decisionReference":r.get::<_,String>(4)?,"assessment":r.get::<_,String>(5)?,"approvedAt":r.get::<_,i64>(6)?}))).optional().map_err(db_error)
    })}
}
fn decision(c:&Connection,did:&str)->Result<PendingDecision,String>{c.query_row("SELECT id,handoff_id,payload_hash,question,answer,answer_reference FROM assistant_decisions WHERE id=?1",[did],|r|Ok(PendingDecision{id:r.get(0)?,handoff_id:r.get(1)?,payload_hash:r.get(2)?,question:r.get(3)?,answer:r.get(4)?,answer_reference:r.get(5)?})).map_err(|_|"ASSISTANT_DECISION_UNAVAILABLE".into())}

#[cfg(test)]
#[path="assistant_tests.rs"]
mod tests;

#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct InstanceGrantInput{pub label:String,pub rules:Vec<BriefRule>,pub expires_at:i64}
impl RouterStore{
 pub fn create_assistant_instance_grant(&self,input:InstanceGrantInput)->Result<AssistantGrant,String>{
  bounded(&input.label,80)?;
  if input.rules.is_empty()||input.rules.len()>30||input.expires_at<=now()||input.expires_at>now()+90*86_400_000{return Err("ASSISTANT_GRANT_INVALID".into());}
  let mut ids=std::collections::HashSet::new();for r in &input.rules{bounded(&r.id,80)?;bounded(&r.text,8000)?;if !ids.insert(&r.id){return Err("ASSISTANT_RULE_DUPLICATE".into());}}
  let rules=serde_json::to_string(&input.rules).map_err(|_|"ASSISTANT_BRIEF_INVALID")?;if rules.len()>32000{return Err("ASSISTANT_BRIEF_INVALID".into());}
  self.with_connection(|c|{let gid=id();c.execute("INSERT INTO assistant_grants(id,workstream_id,source_role,binding_revision,label,brief_json,created_at,expires_at,revoked_at,scope) VALUES(?1,NULL,'BOTH',0,?2,?3,?4,?5,NULL,'INSTANCE')",params![gid,input.label,rules,now(),input.expires_at]).map_err(db_error)?;grant(c,&gid)})
 }
 pub fn require_assistant_bridge(&self,gid:&str,wid:&str,revision:Option<i64>)->Result<Workstream,String>{self.with_connection(|c|{
  let g=active(c,gid)?;let w=workstream_by_id(c,wid)?;
  if(g.scope!="INSTANCE"&&g.workstream_id!=wid)||w.trashed_at.is_some()||w.archived_at.is_some()||revision.is_some_and(|r|r!=w.binding_revision){return Err("ASSISTANT_BRIDGE_CHANGED_OR_REMOVED".into());}Ok(w)
 })}
}

/// Owner connects one assistant to the instance. Business decisions live in the
/// owner's assistant conversation; never convert legacy grants automatically.
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct AssistantConnectionInput { pub label:String, pub expires_at:i64 }
impl RouterStore {
 pub fn connect_assistant_instance(&self,input:AssistantConnectionInput)->Result<AssistantGrant,String>{
  bounded(&input.label,80)?;
  if input.expires_at<=now()||input.expires_at>now()+90*86_400_000{return Err("ASSISTANT_GRANT_INVALID".into());}
  self.with_connection(|c|{let gid=id();c.execute("INSERT INTO assistant_grants(id,workstream_id,source_role,binding_revision,label,brief_json,created_at,expires_at,revoked_at,scope,approval_mode) VALUES(?1,NULL,'BOTH',0,?2,'[]',?3,?4,NULL,'INSTANCE','CONVERSATION_REVIEW')",params![gid,input.label,now(),input.expires_at]).map_err(db_error)?;grant(c,&gid)})
 }
}
