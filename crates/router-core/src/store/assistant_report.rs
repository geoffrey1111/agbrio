//! Stored receipt accounting only: no provider reads, scheduling or writes.
use super::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ReportQuery {
 pub workstream_ids:Vec<String>, pub window_start:i64, pub window_end:i64,
 pub cursor:Option<String>, pub limit:Option<usize>,
}
#[derive(Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Cursor {query_hash:String,ledger_hash:String,offset:usize}

impl RouterStore {
 pub fn assistant_report(&self,gid:&str,mut q:ReportQuery)->Result<Value,String>{
  let limit=q.limit.unwrap_or(100);
  if q.workstream_ids.is_empty()||q.workstream_ids.len()>100||q.workstream_ids.iter().any(|s|s.is_empty()||s.len()>100)||q.window_start<0||q.window_end<=q.window_start||q.window_end>now()||!(1..=200).contains(&limit){return Err("REPORT_ARGUMENTS_INVALID".into());}
  q.workstream_ids.sort();q.workstream_ids.dedup();
  let query_hash=length_prefixed_hash(&[gid.as_bytes(),serde_json::to_string(&q.workstream_ids).unwrap().as_bytes(),q.window_start.to_string().as_bytes(),q.window_end.to_string().as_bytes()]);
  self.with_connection(|c|{
   if assistant::active(c,gid)?.scope!="INSTANCE"{return Err("ASSISTANT_INSTANCE_AUTHORITY_REQUIRED".into());}
   // Digest-fenced pages of one fixed window; changes require a read-only restart.
   let tx=c.transaction().map_err(db_error)?;
   let mut rows:Vec<Value>=Vec::new();
   for wid in &q.workstream_ids {
    let project:(String,String,String)=tx.query_row("SELECT p.id,p.name,w.name FROM workstreams w JOIN projects p ON p.id=w.project_id WHERE w.id=?1",[wid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_|"REPORT_BRIDGE_UNAVAILABLE")?;
    let mut statement=tx.prepare("SELECT h.id,d.request_id,h.status,h.sent_at,h.created_at,h.payload_hash,r.source_role,r.binding_revision,a.grant_id,h.error_code,h.source_endpoint_id,h.destination_endpoint_id,d.observation_id FROM handoffs h LEFT JOIN role_handoff_details r ON r.handoff_id=h.id LEFT JOIN assistant_drafts d ON d.handoff_id=h.id LEFT JOIN assistant_sent_handoffs a ON a.handoff_id=h.id WHERE h.workstream_id=?1 AND ((h.created_at>=?2 AND h.created_at<?3) OR (h.sent_at>=?2 AND h.sent_at<?3)) ORDER BY h.id").map_err(db_error)?;
    let records=statement.query_map(params![wid,q.window_start,q.window_end],|r|{
     let actor:Option<String>=r.get(8)?;let status:String=r.get(2)?;let sent:Option<i64>=r.get(3)?;
     Ok(json!({"kind":"HANDOFF","handoffId":r.get::<_,String>(0)?,"requestId":r.get::<_,Option<String>>(1)?,"status":status,"sentAt":sent,"createdAt":r.get::<_,i64>(4)?,"payloadHash":r.get::<_,String>(5)?,"sourceRole":r.get::<_,Option<String>>(6)?,"bindingRevision":r.get::<_,Option<i64>>(7)?,"actorGrantId":actor,"attribution":match actor.as_deref(){Some(id) if id==gid=>"THIS_GRANT",Some(_)=>"OTHER_ASSISTANT_GRANT",None=>"UNATTRIBUTED"},"errorCode":r.get::<_,Option<String>>(9)?,"sourceEndpointId":r.get::<_,Option<String>>(10)?,"destinationEndpointId":r.get::<_,String>(11)?,"observationId":r.get::<_,Option<String>>(12)?,"confirmedSentInWindow":status=="SENT"&&sent.is_some_and(|t|t>=q.window_start&&t<q.window_end)}))
    }).map_err(db_error)?;
    for row in records {let mut row=row.map_err(db_error)?;row["workstreamId"]=json!(wid);row["projectId"]=json!(project.0);row["projectName"]=json!(project.1);row["bridgeName"]=json!(project.2);rows.push(row);}
    // SEND_CHAT is separate. Current exact generation prevents silently assigning
    // an old original-chat write to a newly bound Bridge. History gaps are explicit.
    let mut endpoints=tx.prepare("SELECT id,external_id FROM endpoints WHERE workstream_id=?1 AND provider='CODEX' AND status='ACTIVE'").map_err(db_error)?;
    let endpoints=endpoints.query_map([wid],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    for (endpoint,thread) in endpoints {
     let Ok(context)=codex_watch::chat_context(&tx,&thread) else{continue};
     let mut s=tx.prepare("SELECT a.id,a.request_id,a.grant_id,r.status,r.updated_at,r.created_at,r.payload_hash,r.error_code,a.status FROM assistant_actions a JOIN watch_replies r ON r.id=a.id WHERE a.operation='SEND_CHAT' AND r.thread_id=?1 AND r.generation=?2 AND ((r.created_at>=?3 AND r.created_at<?4) OR (r.status='SENT' AND r.updated_at>=?3 AND r.updated_at<?4)) ORDER BY a.id").map_err(db_error)?;
     let records=s.query_map(params![thread,context.generation,q.window_start,q.window_end],|r|{
      let actor:String=r.get(2)?;let delivery:String=r.get(3)?;let action_status:String=r.get(8)?;let status=if action_status=="APPLIED"{delivery}else{action_status};let sent=if status=="SENT"{Some(r.get::<_,i64>(4)?)}else{None};
      Ok(json!({"kind":"SEND_CHAT","actionId":r.get::<_,String>(0)?,"requestId":r.get::<_,String>(1)?,"actorGrantId":actor,"attribution":if actor==gid{"THIS_GRANT"}else{"OTHER_ASSISTANT_GRANT"},"status":status,"sentAt":sent,"createdAt":r.get::<_,i64>(5)?,"payloadHash":r.get::<_,String>(6)?,"errorCode":r.get::<_,Option<String>>(7)?,"destinationEndpointId":endpoint,"confirmedSentInWindow":status=="SENT"&&sent.is_some_and(|t|t>=q.window_start&&t<q.window_end)}))
     }).map_err(db_error)?;
     for row in records {let mut row=row.map_err(db_error)?;row["workstreamId"]=json!(wid);row["projectId"]=json!(project.0);row["projectName"]=json!(project.1);row["bridgeName"]=json!(project.2);rows.push(row);}
    }
   }
   rows.sort_by_key(|r|(r["kind"].as_str().unwrap().to_owned(),r["handoffId"].as_str().or(r["actionId"].as_str()).unwrap().to_owned()));
   let ledger_hash=length_prefixed_hash(&[serde_json::to_vec(&rows).map_err(|_|"REPORT_UNAVAILABLE")?.as_slice()]);
   let offset=if let Some(raw)=q.cursor {
    if raw.len()>1024{return Err("REPORT_CURSOR_INVALID".into());}
    let cursor:Cursor=serde_json::from_str(&raw).map_err(|_|"REPORT_CURSOR_INVALID")?;
    if cursor.query_hash!=query_hash{return Err("REPORT_CURSOR_SCOPE_CHANGED".into());}
    if cursor.ledger_hash!=ledger_hash{return Err("REPORT_LEDGER_CHANGED_RESTART_READ".into());}
    if cursor.offset>rows.len(){return Err("REPORT_CURSOR_INVALID".into());}cursor.offset
   }else{0};
   let end=(offset+limit).min(rows.len());
   let next=if end<rows.len(){Some(serde_json::to_string(&Cursor{query_hash,ledger_hash:ledger_hash.clone(),offset:end}).unwrap())}else{None};
   let count=|kind:&str,actor:&str|rows.iter().filter(|r|r["kind"]==kind&&r["attribution"]==actor&&r["confirmedSentInWindow"]==true).count();
   let result=json!({"windowStart":q.window_start,"windowEnd":q.window_end,"windowSemantics":"[start,end)","workstreamIds":q.workstream_ids,"ledgerHash":ledger_hash,"records":&rows[offset..end],"nextCursor":next,"totalRecords":rows.len(),"pageComplete":next.is_none(),"coverage":"AGBRIO_STORED_RECEIPTS_ONLY","attributionComplete":false,"assistantIdentity":"GRANT_ONLY_NOT_UNIQUE_DOT","warnings":["Match this assistant's retained handoffId/requestId evidence; a shared grant alone does not identify a dot. Unattributed history must not be called manual or this assistant.","SEND_CHAT covers saved receipts matching current exact bindings/generations only; historical rebindings or manual original-chat writes may be unavailable.","Read all pages with the same window/scope. On changed-ledger error restart read-only; partial pages or absent native results mean partial verification. SENT is delivery, not task completion."],"totals":{"handoffsThisGrant":count("HANDOFF","THIS_GRANT"),"handoffsOtherGrant":count("HANDOFF","OTHER_ASSISTANT_GRANT"),"handoffsUnattributed":count("HANDOFF","UNATTRIBUTED"),"coordinationThisGrant":count("SEND_CHAT","THIS_GRANT"),"coordinationOtherGrant":count("SEND_CHAT","OTHER_ASSISTANT_GRANT")}});
   tx.commit().map_err(db_error)?;Ok(result)
  })
 }
}

#[cfg(test)]mod tests;
