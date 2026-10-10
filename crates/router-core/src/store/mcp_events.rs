//! Durable, exact-source event metadata. No HTTP, secrets, provider input or agent loop.
use super::*;
use serde::{Deserialize, Serialize};

pub const REPLY_READY: &str = "agbrio.bridge.reply_ready";
pub const DECISION_REQUIRED: &str = "agbrio.bridge.decision_required";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventFilter {
    pub workstream_id: String,
    pub binding_revision: i64,
    pub source_role: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRecord {
    pub event_id: String,
    pub name: String,
    pub workstream_id: String,
    pub binding_revision: i64,
    pub observation_id: String,
    pub source_role: String,
    pub occurred_at: i64,
    pub root_event_id: String,
    pub hop_count: i64,
    pub reason: Option<String>,
}
#[derive(Clone)]
pub struct EventSubscription {
    pub id: String,
    pub grant_id: String,
    pub name: String,
    pub filter: EventFilter,
    pub callback_url: String,
    pub secret: Vec<u8>,
    pub previous_secret: Option<Vec<u8>>,
    pub rotation_until: Option<i64>,
    pub verified_at: i64,
    pub expires_at: i64,
    pub status: String,
}
pub struct EventDelivery {
    pub subscription: EventSubscription,
    pub event: EventRecord,
    pub attempts: i64,
}
fn event_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<EventRecord> {
    Ok(EventRecord {
        event_id: r.get(0)?,
        name: r.get(1)?,
        workstream_id: r.get(2)?,
        binding_revision: r.get(3)?,
        observation_id: r.get(4)?,
        source_role: r.get(5)?,
        occurred_at: r.get(6)?,
        root_event_id: r.get(7)?,
        hop_count: r.get(8)?,
        reason: r.get(9)?,
    })
}
const EVENT_COLUMNS:&str="event_id,name,workstream_id,binding_revision,observation_id,source_role,occurred_at,root_event_id,hop_count,reason";
fn event_hash(fields: &[&str]) -> String {
    length_prefixed_hash(&fields.iter().map(|v| v.as_bytes()).collect::<Vec<_>>())
}
fn subscription(c: &Connection, sid: &str) -> Result<EventSubscription, String> {
    c.query_row("SELECT id,grant_id,name,workstream_id,binding_revision,source_role,callback_url,secret,previous_secret,rotation_until,verified_at,expires_at,status FROM mcp_event_subscriptions WHERE id=?1",[sid],|r|Ok(EventSubscription{id:r.get(0)?,grant_id:r.get(1)?,name:r.get(2)?,filter:EventFilter{workstream_id:r.get(3)?,binding_revision:r.get(4)?,source_role:r.get(5)?},callback_url:r.get(6)?,secret:r.get(7)?,previous_secret:r.get(8)?,rotation_until:r.get(9)?,verified_at:r.get(10)?,expires_at:r.get(11)?,status:r.get(12)?})).map_err(|_|"MCP_EVENT_SUBSCRIPTION_UNAVAILABLE".into())
}
fn authority(c: &Connection, gid: &str, f: &EventFilter) -> Result<(), String> {
    let g = assistant::active(c, gid)?;
    let w = workstream_by_id(c, &f.workstream_id)?;
    if w.trashed_at.is_some()
        || w.archived_at.is_some()
        || w.status == "ARCHIVED"
        || w.binding_revision != f.binding_revision
        || !matches!(f.source_role.as_str(), "DECISION" | "EXECUTION")
        || (g.scope != "INSTANCE"
            && (g.workstream_id != w.id || g.binding_revision != w.binding_revision))
        || (g.source_role != "BOTH" && g.source_role != f.source_role)
    {
        return Err("MCP_EVENT_FORBIDDEN".into());
    }
    let bound:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM endpoints WHERE workstream_id=?1 AND status='ACTIVE' AND bridge_role=?2)",params![w.id,f.source_role],|r|r.get(0)).map_err(db_error)?;
    if !bound {
        return Err("MCP_EVENT_FORBIDDEN".into());
    }
    Ok(())
}
fn enqueue(c: &Connection, event: &EventRecord, actor: Option<&str>) -> Result<(), String> {
    c.execute("INSERT OR IGNORE INTO mcp_events(event_id,name,workstream_id,binding_revision,endpoint_id,observation_id,source_role,occurred_at,root_event_id,hop_count,reason,actor_grant_id) SELECT ?1,?2,?3,?4,endpoint_id,?5,?6,?7,?8,?9,?10,?11 FROM (SELECT id,endpoint_id FROM reply_observations UNION ALL SELECT id,endpoint_id FROM mcp_event_request_sources) WHERE id=?5",params![event.event_id,event.name,event.workstream_id,event.binding_revision,event.observation_id,event.source_role,event.occurred_at,event.root_event_id,event.hop_count,event.reason,actor]).map_err(db_error)?;
    // No body or synthetic old-history replay. An event and eligible deliveries
    // commit together with the actual source completion/decision transaction.
    c.execute("INSERT OR IGNORE INTO mcp_event_deliveries(subscription_id,event_id,status,next_attempt_at) SELECT s.id,e.event_id,'QUEUED',?2 FROM mcp_event_subscriptions s JOIN mcp_events e ON e.event_id=?1 JOIN assistant_grants g ON g.id=s.grant_id WHERE s.status='ACTIVE' AND s.expires_at>?2 AND g.revoked_at IS NULL AND g.expires_at>?2 AND s.workstream_id=e.workstream_id AND s.binding_revision=e.binding_revision AND s.source_role=e.source_role AND s.name=e.name AND e.sequence>s.start_sequence AND e.occurred_at>=s.created_at-1000 AND (?3 IS NULL OR s.grant_id!=?3)",params![event.event_id,now(),actor]).map_err(db_error)?;
    Ok(())
}
pub(super) fn completed(
    c: &Connection,
    wid: &str,
    endpoint: &str,
    identity: &str,
) -> Result<(), String> {
    let row:Option<(String,String,i64,Option<String>,String)>=c.query_row("SELECT o.id,e.bridge_role,COALESCE(o.completed_at,o.observed_at),o.source_provider_run_id,e.external_id FROM reply_observations o JOIN endpoints e ON e.id=o.endpoint_id JOIN workstreams w ON w.id=o.workstream_id WHERE o.workstream_id=?1 AND o.endpoint_id=?2 AND o.assistant_identity=?3 AND o.completion_checked=1 AND length(o.text)>0 AND e.status='ACTIVE' AND e.bridge_role IN ('DECISION','EXECUTION') AND w.trashed_at IS NULL AND w.archived_at IS NULL AND w.status!='ARCHIVED'",params![wid,endpoint,identity],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(db_error)?;
    let Some((observation, role, at, _run, thread)) = row else {
        return Ok(());
    };
    let initial: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM mcp_event_initial_observations WHERE observation_id=?1)",
            [&observation],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    if initial {
        return Ok(());
    };
    // Passive ChatGPT history has no trustworthy completion timestamp. Its
    // first exact read establishes a baseline unless an exact completed run
    // proves a new result. Never relabel cold history as a newly finished reply.
    if endpoint_by_id(c, endpoint)?.provider == "CHATGPT" {
        let proven:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM reply_observations WHERE endpoint_id=?1 AND id!=?2 AND completion_checked=1) OR EXISTS(SELECT 1 FROM provider_runs WHERE endpoint_id=?1 AND status='COMPLETED' AND (id=?3 OR result_identity=?4))",params![endpoint,observation,_run,identity],|r|r.get(0)).map_err(db_error)?;
        if !proven {
            c.execute(
                "INSERT OR IGNORE INTO mcp_event_initial_observations VALUES(?1)",
                [&observation],
            )
            .map_err(db_error)?;
            return Ok(());
        }
    }
    let revision = workstream_by_id(c, wid)?.binding_revision;
    let turn = identity
        .strip_prefix("codex:")
        .and_then(|s| s.split_once(':'))
        .map(|(turn, _)| turn);
    let cause: Option<(String, i64, bool)> = if let Some(turn) = turn {
        c.query_row("SELECT root_event_id,hop_count,ambiguous FROM mcp_event_native_causes WHERE thread_id=?1 AND turn_id=?2",params![thread,turn],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db_error)?
    } else {
        None
    };
    let cause = if cause.is_some() {
        cause
    } else {
        c.query_row("SELECT ev.root_event_id,ev.hop_count+1,0 FROM provider_runs p JOIN mcp_event_actions a ON a.handoff_id=p.origin_handoff_id JOIN mcp_events ev ON ev.event_id=a.event_id WHERE p.workstream_id=?1 AND p.endpoint_id=?2 AND (p.id=?3 OR p.result_identity=?4) ORDER BY p.created_at DESC LIMIT 1",params![wid,endpoint,_run,identity],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db_error)?
    };
    let uncertain:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM handoffs WHERE workstream_id=?1 AND destination_endpoint_id=?2 AND status IN ('SENDING','UNKNOWN'))",params![wid,endpoint],|r|r.get(0)).map_err(db_error)?;
    let hops = cause.as_ref().map_or(0, |v| v.1);
    let reason = if uncertain {
        Some("RECEIPT_UNRESOLVED")
    } else if cause.as_ref().is_some_and(|v| v.2) {
        Some("CAUSAL_LINEAGE_AMBIGUOUS")
    } else if hops >= 2 {
        Some("LOOP_BOUNDARY")
    } else {
        None
    };
    let name = if reason.is_some() {
        DECISION_REQUIRED
    } else {
        REPLY_READY
    };
    let eid = format!(
        "evt_{}",
        event_hash(&[name, wid, &revision.to_string(), &observation, &role])
    );
    let event = EventRecord {
        root_event_id: cause.map_or_else(|| eid.clone(), |v| v.0),
        event_id: eid,
        name: name.into(),
        workstream_id: wid.into(),
        binding_revision: revision,
        observation_id: observation,
        source_role: role,
        occurred_at: at,
        hop_count: hops,
        reason: reason.map(str::to_string),
    };
    enqueue(c, &event, None)
}
pub(super) fn owner_decision(c: &Connection, gid: &str, hid: &str) -> Result<(), String> {
    let row:Option<(String,String,i64,String)>=c.query_row("SELECT d.observation_id,h.workstream_id,r.binding_revision,r.source_role FROM assistant_drafts d JOIN handoffs h ON h.id=d.handoff_id JOIN role_handoff_details r ON r.handoff_id=h.id WHERE d.handoff_id=?1",[hid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(db_error)?;
    if let Some((obs, wid, revision, role)) = row {
        let eid = format!(
            "evt_{}",
            event_hash(&[DECISION_REQUIRED, &wid, &revision.to_string(), &obs, &role])
        );
        enqueue(
            c,
            &EventRecord {
                event_id: eid.clone(),
                name: DECISION_REQUIRED.into(),
                workstream_id: wid,
                binding_revision: revision,
                observation_id: obs,
                source_role: role,
                occurred_at: now(),
                root_event_id: eid,
                hop_count: 0,
                reason: Some("OWNER_DECISION".into()),
            },
            Some(gid),
        )?;
    }
    Ok(())
}
impl RouterStore {
    pub fn require_event_filter(&self, gid: &str, f: &EventFilter) -> Result<(), String> {
        self.with_connection(|c| authority(c, gid, f))
    }
    pub fn event_subscription(&self, sid: &str) -> Result<Option<EventSubscription>, String> {
        self.with_connection(|c| {
            let exists: bool = c
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM mcp_event_subscriptions WHERE id=?1)",
                    [sid],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            if exists {
                subscription(c, sid).map(Some)
            } else {
                Ok(None)
            }
        })
    }
    pub fn save_event_subscription(&self, s: &EventSubscription) -> Result<(), String> {
        self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;authority(&tx,&s.grant_id,&s.filter)?;
  let time=now();
  if let Ok(old)=subscription(&tx,&s.id){if old.grant_id!=s.grant_id||old.name!=s.name||old.filter.workstream_id!=s.filter.workstream_id||old.filter.binding_revision!=s.filter.binding_revision||old.filter.source_role!=s.filter.source_role||old.callback_url!=s.callback_url{return Err("MCP_EVENT_SUBSCRIPTION_IDENTITY_CHANGED".into());}}
  let max=assistant::active(&tx,&s.grant_id)?.expires_at;if s.expires_at<=now()||s.expires_at>max||s.secret.is_empty(){return Err("MCP_EVENT_SUBSCRIPTION_INVALID".into());}
  let count:i64=tx.query_row("SELECT COUNT(*) FROM mcp_event_subscriptions WHERE grant_id=?1 AND status='ACTIVE' AND expires_at>?2",params![s.grant_id,now()],|r|r.get(0)).map_err(db_error)?;
  let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM mcp_event_subscriptions WHERE id=?1 AND grant_id=?2 AND status='ACTIVE' AND expires_at>?3)",params![s.id,s.grant_id,now()],|r|r.get(0)).map_err(db_error)?;if count>=32&&!exists{return Err("MCP_EVENT_SUBSCRIPTION_LIMIT".into());}
  tx.execute("INSERT INTO mcp_event_subscriptions(id,grant_id,name,workstream_id,binding_revision,source_role,callback_url,secret,previous_secret,rotation_until,verified_at,expires_at,status,created_at,updated_at,start_sequence) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'ACTIVE',?13,?13,COALESCE((SELECT MAX(sequence) FROM mcp_events),0)) ON CONFLICT(id) DO UPDATE SET secret=excluded.secret,previous_secret=excluded.previous_secret,rotation_until=excluded.rotation_until,verified_at=excluded.verified_at,expires_at=excluded.expires_at,start_sequence=CASE WHEN mcp_event_subscriptions.status!='ACTIVE' OR mcp_event_subscriptions.expires_at<=excluded.updated_at THEN excluded.start_sequence ELSE mcp_event_subscriptions.start_sequence END,status='ACTIVE',updated_at=excluded.updated_at",params![s.id,s.grant_id,s.name,s.filter.workstream_id,s.filter.binding_revision,s.filter.source_role,s.callback_url,s.secret,s.previous_secret,s.rotation_until,s.verified_at,s.expires_at,time]).map_err(db_error)?;
  tx.commit().map_err(db_error)
 })
    }
    /// Verification is cached only for the same principal and exact callback URL.
    pub fn recently_verified_event_callbacks(
        &self,
        gid: &str,
        url: &str,
    ) -> Result<Vec<EventSubscription>, String> {
        self.with_connection(|c|{assistant::active(c,gid)?;let mut q=c.prepare("SELECT id FROM mcp_event_subscriptions WHERE grant_id=?1 AND callback_url=?2 AND status='ACTIVE' AND expires_at>?3 AND verified_at>?4 ORDER BY verified_at DESC LIMIT 32").map_err(db_error)?;let ids=q.query_map(params![gid,url,now(),now()-300000],|r|r.get::<_,String>(0)).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;ids.into_iter().map(|id|subscription(c,&id)).collect()})
    }
    pub fn unsubscribe_event(&self, gid: &str, sid: &str) -> Result<(), String> {
        self.with_connection(|c|{
  assistant::active(c,gid)?;let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
  tx.execute("UPDATE mcp_event_subscriptions SET status='UNSUBSCRIBED',secret=x'',previous_secret=NULL,rotation_until=NULL,expires_at=?3,updated_at=?3 WHERE id=?1 AND grant_id=?2",params![sid,gid,now()]).map_err(db_error)?;
  tx.execute("UPDATE mcp_event_deliveries SET status='STOPPED' WHERE subscription_id=?1 AND status IN ('QUEUED','SENDING') AND EXISTS(SELECT 1 FROM mcp_event_subscriptions WHERE id=?1 AND grant_id=?2)",params![sid,gid]).map_err(db_error)?;tx.commit().map_err(db_error)
 })
    }
    pub fn read_mcp_event(&self, gid: &str, eid: &str) -> Result<EventRecord, String> {
        self.with_connection(|c| {
            let e = c
                .query_row(
                    &format!("SELECT {EVENT_COLUMNS} FROM mcp_events WHERE event_id=?1"),
                    [eid],
                    event_row,
                )
                .map_err(|_| "MCP_EVENT_UNAVAILABLE")?;
            authority(
                c,
                gid,
                &EventFilter {
                    workstream_id: e.workstream_id.clone(),
                    binding_revision: e.binding_revision,
                    source_role: e.source_role.clone(),
                },
            )?;
            Ok(e)
        })
    }
    pub fn claim_event_delivery(&self) -> Result<Option<EventDelivery>, String> {
        self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let time=now();
  tx.execute("UPDATE mcp_event_subscriptions SET previous_secret=NULL,rotation_until=NULL WHERE rotation_until<=?1",[time]).map_err(db_error)?;
  tx.execute("UPDATE mcp_event_subscriptions SET status='EXPIRED',secret=x'',previous_secret=NULL,rotation_until=NULL WHERE status='ACTIVE' AND (expires_at<=?1 OR EXISTS(SELECT 1 FROM assistant_grants g WHERE g.id=mcp_event_subscriptions.grant_id AND (g.revoked_at IS NOT NULL OR g.expires_at<=?1)))",[time]).map_err(db_error)?;
  tx.execute("UPDATE mcp_event_deliveries SET status='STOPPED' WHERE status IN ('QUEUED','SENDING') AND event_id IN (SELECT e.event_id FROM mcp_events e JOIN mcp_event_request_sources r ON r.id=e.observation_id WHERE r.resolved_at IS NOT NULL)",[]).map_err(db_error)?;
  tx.execute("UPDATE mcp_event_deliveries SET status='QUEUED',lease_until=NULL WHERE status='SENDING' AND lease_until<=?1",[time]).map_err(db_error)?;
  tx.execute("UPDATE mcp_event_deliveries SET status='STOPPED' WHERE status IN ('QUEUED','SENDING') AND subscription_id IN (SELECT s.id FROM mcp_event_subscriptions s JOIN assistant_grants g ON g.id=s.grant_id JOIN workstreams w ON w.id=s.workstream_id WHERE s.status!='ACTIVE' OR s.expires_at<=?1 OR g.revoked_at IS NOT NULL OR g.expires_at<=?1 OR w.trashed_at IS NOT NULL OR w.archived_at IS NOT NULL OR w.binding_revision!=s.binding_revision)",[time]).map_err(db_error)?;
  let selected:Option<(String,String,i64)>=tx.query_row("SELECT subscription_id,event_id,attempts FROM mcp_event_deliveries WHERE status='QUEUED' AND next_attempt_at<=?1 ORDER BY next_attempt_at,event_id LIMIT 1",[time],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db_error)?;
  let Some((sid,eid,attempts))=selected else{tx.commit().map_err(db_error)?;return Ok(None)};let s=subscription(&tx,&sid)?;
  if authority(&tx,&s.grant_id,&s.filter).is_err(){tx.execute("UPDATE mcp_event_deliveries SET status='STOPPED' WHERE subscription_id=?1 AND event_id=?2",params![sid,eid]).map_err(db_error)?;tx.commit().map_err(db_error)?;return Ok(None);}
  let e=tx.query_row(&format!("SELECT {EVENT_COLUMNS} FROM mcp_events WHERE event_id=?1"),[&eid],event_row).map_err(db_error)?;
  tx.execute("UPDATE mcp_event_deliveries SET status='SENDING',attempts=attempts+1,lease_until=?3 WHERE subscription_id=?1 AND event_id=?2",params![sid,eid,time+30000]).map_err(db_error)?;tx.commit().map_err(db_error)?;Ok(Some(EventDelivery{subscription:s,event:e,attempts:attempts+1}))
 })
    }
    pub fn event_delivery_still_allowed(&self, sid: &str) -> Result<bool, String> {
        self.with_connection(|c| {
            let s = subscription(c, sid)?;
            Ok(s.status == "ACTIVE"
                && s.expires_at > now()
                && authority(c, &s.grant_id, &s.filter).is_ok())
        })
    }
    pub fn event_message_still_allowed(&self, sid: &str, eid: &str) -> Result<bool, String> {
        self.with_connection(|c|{
  let s=subscription(c,sid)?;if s.status!="ACTIVE"||s.expires_at<=now()||authority(c,&s.grant_id,&s.filter).is_err(){return Ok(false);}
  let valid:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM mcp_events e WHERE e.event_id=?1 AND e.workstream_id=?2 AND e.binding_revision=?3 AND e.source_role=?4 AND (EXISTS(SELECT 1 FROM reply_observations o WHERE o.id=e.observation_id) OR EXISTS(SELECT 1 FROM mcp_event_request_sources r WHERE r.id=e.observation_id AND r.resolved_at IS NULL)))",params![eid,s.filter.workstream_id,s.filter.binding_revision,s.filter.source_role],|r|r.get(0)).map_err(db_error)?;Ok(valid)
 })
    }
    pub fn finish_event_delivery(
        &self,
        sid: &str,
        eid: &str,
        attempt: i64,
        status: Option<u16>,
    ) -> Result<(), String> {
        self.with_connection(|c|{
  let accepted=status.is_some_and(|s|(200..300).contains(&s));let stopped=status.is_some_and(|s|matches!(s,410|413))||attempt>=8;
  let state=if accepted{"DELIVERED"}else if stopped{"FAILED"}else{"QUEUED"};let delay=1000_i64.checked_shl(attempt.min(8)as u32).unwrap_or(256000);
  c.execute("UPDATE mcp_event_deliveries SET status=?4,next_attempt_at=?5,lease_until=NULL,delivered_at=?6,http_status=?7 WHERE subscription_id=?1 AND event_id=?2 AND status='SENDING' AND attempts=?3",params![sid,eid,attempt,state,now()+delay,accepted.then(now),status]).map_err(db_error)?;Ok(())
 })
    }
    pub fn event_status_for_grant(&self, gid: &str) -> Result<Value, String> {
        self.with_connection(|c|{
  assistant::active(c,gid)?;let subscriptions:i64=c.query_row("SELECT COUNT(*) FROM mcp_event_subscriptions WHERE grant_id=?1 AND status='ACTIVE' AND expires_at>?2",params![gid,now()],|r|r.get(0)).map_err(db_error)?;
  let delivered:i64=c.query_row("SELECT COUNT(*) FROM mcp_event_deliveries d JOIN mcp_event_subscriptions s ON s.id=d.subscription_id WHERE s.grant_id=?1 AND d.status='DELIVERED'",[gid],|r|r.get(0)).map_err(db_error)?;
  Ok(serde_json::json!({"activeSubscriptions":subscriptions,"webhookReceipts":delivered,"businessCompletionProven":false}))
 })
    }
}

fn event_for_prepare(
    c: &Connection,
    gid: &str,
    wid: &str,
    revision: i64,
    obs: &str,
    role: &str,
    explicit: Option<&str>,
) -> Result<Option<EventRecord>, String> {
    authority(
        c,
        gid,
        &EventFilter {
            workstream_id: wid.into(),
            binding_revision: revision,
            source_role: role.into(),
        },
    )?;
    if obs.starts_with("req_") {
        return Err("ASSISTANT_REQUEST_IS_NOT_COMPLETE_REPLY".into());
    }
    let eid:Option<String>=c.query_row("SELECT e.event_id FROM mcp_events e WHERE e.workstream_id=?1 AND e.binding_revision=?2 AND e.observation_id=?3 AND e.source_role=?4 AND (?5 IS NULL OR e.event_id=?5) AND EXISTS(SELECT 1 FROM mcp_event_subscriptions s JOIN mcp_event_deliveries d ON d.subscription_id=s.id AND d.event_id=e.event_id WHERE s.grant_id=?6 AND s.status='ACTIVE' AND s.expires_at>?7 AND e.sequence>s.start_sequence) ORDER BY e.sequence DESC LIMIT 1",params![wid,revision,obs,role,explicit,gid,now()],|r|r.get(0)).optional().map_err(db_error)?;
    if eid.is_none() {
        let previously_notified:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM mcp_events e JOIN mcp_event_deliveries d ON d.event_id=e.event_id JOIN mcp_event_subscriptions s ON s.id=d.subscription_id WHERE s.grant_id=?1 AND e.workstream_id=?2 AND e.binding_revision=?3 AND e.observation_id=?4 AND e.source_role=?5)",params![gid,wid,revision,obs,role],|r|r.get(0)).map_err(db_error)?;
        if explicit.is_some() || previously_notified {
            return Err("ASSISTANT_EVENT_SUBSCRIPTION_ENDED_OR_SOURCE_CHANGED".into());
        }
    }
    eid.map(|id| {
        c.query_row(
            &format!("SELECT {EVENT_COLUMNS} FROM mcp_events WHERE event_id=?1"),
            [id],
            event_row,
        )
        .map_err(db_error)
    })
    .transpose()
}
fn source_key(e: &EventRecord) -> String {
    event_hash(&[
        &e.workstream_id,
        &e.binding_revision.to_string(),
        &e.observation_id,
        &e.source_role,
    ])
}
pub(super) fn register_event_action(
    c: &Connection,
    gid: &str,
    hid: &str,
    obs: &str,
    event_hash_value: &str,
    explicit: Option<&str>,
) -> Result<(), String> {
    let(wid,revision,role):(String,i64,String)=c.query_row("SELECT h.workstream_id,r.binding_revision,r.source_role FROM handoffs h JOIN role_handoff_details r ON r.handoff_id=h.id WHERE h.id=?1",[hid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(db_error)?;
    if let Some(e) = event_for_prepare(c, gid, &wid, revision, obs, &role, explicit)? {
        c.execute("INSERT INTO mcp_event_actions(grant_id,event_id,handoff_id,request_hash,source_key) VALUES(?1,?2,?3,?4,?5)",params![gid,e.event_id,hid,event_hash_value,source_key(&e)]).map_err(|_|"ASSISTANT_EVENT_PREPARE_ALREADY_EXISTS")?;
        if e.reason.is_some() {
            let hash = handoff_by_id(c, hid)?.payload_hash;
            c.execute("INSERT INTO assistant_decisions(id,handoff_id,payload_hash,question,created_at) VALUES(?1,?2,?3,?4,?5)",params![id(),hid,hash,"事件已到达循环或未确认回执边界。请向实际所有者核对是否继续；不能自动重复转发。",now()]).map_err(db_error)?;
        }
    }
    Ok(())
}
pub(super) fn require_event_send(c: &Connection, gid: &str, hid: &str) -> Result<(), String> {
    let eid: Option<String> = c
        .query_row(
            "SELECT event_id FROM mcp_event_actions WHERE grant_id=?1 AND handoff_id=?2",
            params![gid, hid],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?;
    if let Some(eid) = eid {
        let e = c
            .query_row(
                &format!("SELECT {EVENT_COLUMNS} FROM mcp_events WHERE event_id=?1"),
                [eid],
                event_row,
            )
            .map_err(db_error)?;
        event_for_prepare(
            c,
            gid,
            &e.workstream_id,
            e.binding_revision,
            &e.observation_id,
            &e.source_role,
            Some(&e.event_id),
        )?;
    }
    Ok(())
}
impl RouterStore {
    pub fn event_prepare_receipt(
        &self,
        gid: &str,
        wid: &str,
        revision: i64,
        obs: &str,
        role: &str,
        hash: &str,
        explicit: Option<&str>,
    ) -> Result<Option<HandoffHistoryItem>, String> {
        self.with_connection(|c|{
  let Some(e)=event_for_prepare(c,gid,wid,revision,obs,role,explicit)?else{return Ok(None)};
  let found:Option<(String,String)>=c.query_row("SELECT handoff_id,request_hash FROM mcp_event_actions WHERE grant_id=?1 AND source_key=?2",params![gid,source_key(&e)],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
  if let Some((hid,stored))=found{if stored!=hash{return Err("ASSISTANT_EVENT_ARGUMENTS_CHANGED".into());}handoff_by_id(c,&hid).map(Some)}else{Ok(None)}
 })
    }
    pub fn note_mcp_native_cause(&self, hid: &str, thread: &str, turn: &str) -> Result<(), String> {
        self.with_connection(|c|{
  let found:Option<(String,String,i64)>=c.query_row("SELECT a.grant_id,e.root_event_id,e.hop_count FROM mcp_event_actions a JOIN mcp_events e ON e.event_id=a.event_id WHERE a.handoff_id=?1",[hid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db_error)?;
  if let Some((gid,root,hops))=found{c.execute("INSERT INTO mcp_event_native_causes(thread_id,turn_id,handoff_id,root_event_id,hop_count,actor_grant_id) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(thread_id,turn_id) DO UPDATE SET ambiguous=CASE WHEN mcp_event_native_causes.handoff_id=excluded.handoff_id THEN mcp_event_native_causes.ambiguous ELSE 1 END,hop_count=MAX(mcp_event_native_causes.hop_count,excluded.hop_count)",params![thread,turn,hid,root,hops+1,gid]).map_err(db_error)?;}Ok(())
 })
    }
}

impl RouterStore {
    pub fn mcp_request_identity(
        &self,
        obs: &str,
    ) -> Result<Option<(String, String, String)>, String> {
        self.with_connection(|c|c.query_row("SELECT thread_id,turn_id,raw_request_id FROM mcp_event_request_sources WHERE id=?1 AND resolved_at IS NULL",[obs],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db_error))
    }
    /// Persist only the existing public request projection; callback data remains IDs.
    pub fn observe_mcp_native_request(
        &self,
        thread: &str,
        turn: &str,
        raw_id: &str,
        public_request: &Value,
    ) -> Result<(), String> {
        if thread.is_empty()
            || thread.len() > 256
            || turn.is_empty()
            || turn.len() > 256
            || raw_id.len() > 512
        {
            return Err("MCP_EVENT_REQUEST_INVALID".into());
        }
        let body =
            serde_json::to_string(public_request).map_err(|_| "MCP_EVENT_REQUEST_INVALID")?;
        if body.len() > 65536 {
            return Err("MCP_EVENT_REQUEST_INVALID".into());
        }
        self.with_connection(|c|{let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   let endpoints={let mut q=tx.prepare("SELECT e.id,e.workstream_id,e.bridge_role,w.binding_revision FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id WHERE e.provider='CODEX' AND e.external_id=?1 AND e.status='ACTIVE' AND e.bridge_role IN ('DECISION','EXECUTION') AND w.status='ACTIVE' AND w.trashed_at IS NULL AND w.archived_at IS NULL").map_err(db_error)?;let rows=q.query_map([thread],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;rows};
   let resolved:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM mcp_event_request_tombstones WHERE thread_id=?1 AND turn_id IN (?2,'*') AND raw_request_id IN (?3,'*'))",params![thread,turn,raw_id],|r|r.get(0)).map_err(db_error)?;
   if resolved{tx.commit().map_err(db_error)?;return Ok(());}
   for(ep,wid,role,revision)in endpoints{
    let obs=format!("req_{}",event_hash(&[&wid,&revision.to_string(),&ep,turn,raw_id]));
    tx.execute("INSERT INTO mcp_event_request_sources(id,workstream_id,binding_revision,endpoint_id,source_role,thread_id,turn_id,raw_request_id,public_request,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(id) DO UPDATE SET public_request=excluded.public_request WHERE mcp_event_request_sources.resolved_at IS NULL",params![obs,wid,revision,ep,role,thread,turn,raw_id,body,now()]).map_err(db_error)?;
    let resolved:bool=tx.query_row("SELECT resolved_at IS NOT NULL FROM mcp_event_request_sources WHERE id=?1",[&obs],|r|r.get(0)).map_err(db_error)?;if resolved{continue;}
    let cause:Option<(String,i64)>=tx.query_row("SELECT root_event_id,hop_count FROM mcp_event_native_causes WHERE thread_id=?1 AND turn_id=?2",params![thread,turn],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
    let eid=format!("evt_{}",event_hash(&[DECISION_REQUIRED,&wid,&revision.to_string(),&obs,&role]));let(root,hops)=cause.unwrap_or_else(||(eid.clone(),0));
    enqueue(&tx,&EventRecord{event_id:eid,name:DECISION_REQUIRED.into(),workstream_id:wid,binding_revision:revision,observation_id:obs,source_role:role,occurred_at:now(),root_event_id:root,hop_count:hops,reason:Some("NATIVE_REQUEST".into())},None)?;
   }tx.commit().map_err(db_error)
  })
    }
    pub fn resolve_mcp_native_request(
        &self,
        thread: &str,
        turn: Option<&str>,
        raw_id: Option<&str>,
    ) -> Result<(), String> {
        self.with_connection(|c|{
  if turn.is_none()&&raw_id.is_none(){return Err("MCP_EVENT_REQUEST_RESOLUTION_INVALID".into());}
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
  tx.execute("INSERT OR IGNORE INTO mcp_event_request_tombstones VALUES(?1,?2,?3,?4)",params![thread,turn.unwrap_or("*"),raw_id.unwrap_or("*"),now()]).map_err(db_error)?;
  tx.execute("UPDATE mcp_event_request_sources SET resolved_at=COALESCE(resolved_at,?4) WHERE thread_id=?1 AND (?2 IS NULL OR turn_id=?2) AND (?3 IS NULL OR raw_request_id=?3)",params![thread,turn,raw_id,now()]).map_err(db_error)?;
  tx.execute("UPDATE mcp_event_deliveries SET status='STOPPED' WHERE status IN ('QUEUED','SENDING') AND event_id IN (SELECT e.event_id FROM mcp_events e JOIN mcp_event_request_sources r ON r.id=e.observation_id WHERE r.resolved_at IS NOT NULL)",[]).map_err(db_error)?;
  tx.commit().map_err(db_error)
 })
    }
    pub fn read_mcp_request_source(
        &self,
        gid: &str,
        wid: &str,
        role: &str,
        obs: &str,
    ) -> Result<Value, String> {
        self.with_connection(|c|{
  let(revision,request,resolved):(i64,String,Option<i64>)=c.query_row("SELECT binding_revision,public_request,resolved_at FROM mcp_event_request_sources WHERE id=?1 AND workstream_id=?2 AND source_role=?3",params![obs,wid,role],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_|"MCP_EVENT_SOURCE_UNAVAILABLE")?;
  authority(c,gid,&EventFilter{workstream_id:wid.into(),binding_revision:revision,source_role:role.into()})?;
  Ok(serde_json::json!({"sourceKind":"NATIVE_REQUEST","observationId":obs,"workstreamId":wid,"bindingRevision":revision,"sourceRole":role,"resolved":resolved.is_some(),"request":serde_json::from_str::<Value>(&request).map_err(|_|"MCP_EVENT_SOURCE_INVALID")?,"canPrepareHandoff":false}))
 })
    }
}

#[cfg(test)]
mod tests {
    use super::super::assistant::{BriefRule, GrantInput};
    use super::*;
    fn fixture() -> (
        tempfile::TempDir,
        RouterStore,
        EventSubscription,
        String,
        String,
    ) {
        let d = tempfile::tempdir().unwrap();
        let s = RouterStore::open_at(d.path().join("events.db")).unwrap();
        let p = s.create_project("fixture".into(), None).unwrap();
        let w = s.create_workstream(&p.id, "Bridge".into()).unwrap();
        let side = |id: &str| role_bridge::RoleBindingInput {
            provider: "CODEX".into(),
            external_id: id.into(),
            label: id.into(),
            cwd: Some(d.path().to_string_lossy().into()),
        };
        let b = s
            .bind_role_bridge(
                &w.id,
                w.binding_revision,
                side("fixture-control"),
                side("fixture-execution"),
            )
            .unwrap();
        let g = s
            .create_assistant_grant(GrantInput {
                workstream_id: w.id.clone(),
                source_role: "DECISION".into(),
                binding_revision: b.binding_revision,
                label: "fixture Dot".into(),
                rules: vec![BriefRule {
                    id: "continue".into(),
                    text: "Continue exact agreed instruction".into(),
                }],
                expires_at: now() + 86400000,
            })
            .unwrap();
        let sub = EventSubscription {
            id: "fixture-sub".into(),
            grant_id: g.id,
            name: REPLY_READY.into(),
            filter: EventFilter {
                workstream_id: w.id,
                binding_revision: b.binding_revision,
                source_role: "DECISION".into(),
            },
            callback_url: "https://example.com/callback".into(),
            secret: vec![1],
            previous_secret: None,
            rotation_until: None,
            verified_at: now(),
            expires_at: now() + 600000,
            status: "ACTIVE".into(),
        };
        (
            d,
            s,
            sub,
            b.decision.unwrap().endpoint.id,
            b.execution.unwrap().endpoint.id,
        )
    }
    fn complete(s: &RouterStore, sub: &EventSubscription, ep: &str, id: &str) -> ReplyObservation {
        let obs = s
            .record_reply_observation(
                &sub.filter.workstream_id,
                ep,
                Some(id),
                "fixture complete body",
                None,
            )
            .unwrap()
            .unwrap();
        s.note_reply_completion(&sub.filter.workstream_id, ep, id, Some(now()))
            .unwrap();
        obs
    }
    fn sql(s: &RouterStore, statement: &str) {
        s.with_connection(|c| {
            c.execute(statement, []).map_err(db_error)?;
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn events_only_after_complete_filtered_and_idempotent() {
        let (_d, s, sub, decision, execution) = fixture();
        s.save_event_subscription(&sub).unwrap();
        s.record_reply_observation(
            &sub.filter.workstream_id,
            &decision,
            Some("partial"),
            "fixture partial",
            None,
        )
        .unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
        complete(&s, &sub, &execution, "other-role");
        assert!(s.claim_event_delivery().unwrap().is_none());
        let obs = complete(&s, &sub, &decision, "exact-complete");
        let item = s.claim_event_delivery().unwrap().unwrap();
        assert_eq!(item.event.observation_id, obs.id);
        assert_eq!(item.event.source_role, "DECISION");
        assert_eq!(item.event.binding_revision, sub.filter.binding_revision);
        s.note_reply_completion(&sub.filter.workstream_id, &decision, "exact-complete", None)
            .unwrap();
        s.finish_event_delivery(&sub.id, &item.event.event_id, item.attempts, Some(204))
            .unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
        assert_eq!(
            s.event_status_for_grant(&sub.grant_id).unwrap()["businessCompletionProven"],
            false
        );
    }
    #[test]
    fn durable_claim_lease_retry_keeps_exact_event_and_rejects_late_ack() {
        let (d, s, sub, decision, _) = fixture();
        s.save_event_subscription(&sub).unwrap();
        complete(&s, &sub, &decision, "retry");
        let first = s.claim_event_delivery().unwrap().unwrap();
        drop(s);
        let s = RouterStore::open_at(d.path().join("events.db")).unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
        sql(&s, "UPDATE mcp_event_deliveries SET lease_until=0");
        let second = s.claim_event_delivery().unwrap().unwrap();
        assert_eq!(first.event.event_id, second.event.event_id);
        assert_eq!(second.attempts, 2);
        s.finish_event_delivery(&sub.id, &first.event.event_id, first.attempts, Some(200))
            .unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
        s.finish_event_delivery(&sub.id, &second.event.event_id, second.attempts, Some(503))
            .unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
        sql(&s, "UPDATE mcp_event_deliveries SET next_attempt_at=0");
        let third = s.claim_event_delivery().unwrap().unwrap();
        assert_eq!(third.event.event_id, first.event.event_id);
        s.finish_event_delivery(&sub.id, &third.event.event_id, third.attempts, Some(410))
            .unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
    }
    #[test]
    fn unsubscribe_idempotent_and_revocation_stops_queued_work() {
        let (_d, s, sub, decision, _) = fixture();
        s.save_event_subscription(&sub).unwrap();
        complete(&s, &sub, &decision, "cancel");
        s.unsubscribe_event(&sub.grant_id, &sub.id).unwrap();
        s.unsubscribe_event(&sub.grant_id, &sub.id).unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
        assert!(!s.event_delivery_still_allowed(&sub.id).unwrap());
        s.save_event_subscription(&sub).unwrap();
        complete(&s, &sub, &decision, "after-resubscribe");
        s.revoke_assistant_grant(&sub.grant_id).unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
        assert!(s.read_mcp_event(&sub.grant_id, "unknown").is_err());
    }
    #[test]
    fn exact_filter_and_expiry_are_enforced() {
        let (_d, s, sub, decision, _) = fixture();
        let mut wrong = sub.filter.clone();
        wrong.source_role = "EXECUTION".into();
        assert!(s.require_event_filter(&sub.grant_id, &wrong).is_err());
        wrong = sub.filter.clone();
        wrong.binding_revision += 1;
        assert!(s.require_event_filter(&sub.grant_id, &wrong).is_err());
        s.save_event_subscription(&sub).unwrap();
        complete(&s, &sub, &decision, "expired");
        sql(&s, "UPDATE mcp_event_subscriptions SET expires_at=1");
        assert!(s.claim_event_delivery().unwrap().is_none());
        assert!(!s.event_delivery_still_allowed(&sub.id).unwrap());
        s.save_event_subscription(&sub).unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
    }
    #[test]
    fn rebind_and_trash_stop_delivery_without_redirecting_to_another_bridge() {
        for statement in [
            "UPDATE workstreams SET binding_revision=binding_revision+1",
            "UPDATE workstreams SET trashed_at=1",
        ] {
            let (_d, s, sub, decision, _) = fixture();
            s.save_event_subscription(&sub).unwrap();
            complete(&s, &sub, &decision, "retired");
            sql(&s, statement);
            assert!(s.claim_event_delivery().unwrap().is_none());
            assert!(!s.event_delivery_still_allowed(&sub.id).unwrap());
        }
    }
    #[test]
    fn event_prepare_receipt_deduplicates_request_ids_and_expired_resubscription_does_not_revive_source(
    ) {
        let (_d, s, sub, decision, _) = fixture();
        s.save_event_subscription(&sub).unwrap();
        let obs = complete(&s, &sub, &decision, "prepare");
        let event = s.claim_event_delivery().unwrap().unwrap().event;
        let handoff = s
            .prepare_role_handoff(
                &sub.filter.workstream_id,
                "DECISION",
                &obs.id,
                "authorized fixture bytes",
            )
            .unwrap();
        let hash = "a".repeat(64);
        s.register_assistant_prepare_event(
            &sub.grant_id,
            &handoff.id,
            &obs.id,
            "request-one",
            &hash,
            &hash,
            Some(&event.event_id),
        )
        .unwrap();
        assert_eq!(
            s.event_prepare_receipt(
                &sub.grant_id,
                &sub.filter.workstream_id,
                sub.filter.binding_revision,
                &obs.id,
                "DECISION",
                &hash,
                Some(&event.event_id)
            )
            .unwrap()
            .unwrap()
            .id,
            handoff.id
        );
        assert!(s
            .event_prepare_receipt(
                &sub.grant_id,
                &sub.filter.workstream_id,
                sub.filter.binding_revision,
                &obs.id,
                "DECISION",
                &"b".repeat(64),
                Some(&event.event_id)
            )
            .is_err());
        s.unsubscribe_event(&sub.grant_id, &sub.id).unwrap();
        s.save_event_subscription(&sub).unwrap();
        assert!(s
            .event_prepare_receipt(
                &sub.grant_id,
                &sub.filter.workstream_id,
                sub.filter.binding_revision,
                &obs.id,
                "DECISION",
                &hash,
                Some(&event.event_id)
            )
            .is_err());
        assert!(s
            .event_prepare_receipt(
                &sub.grant_id,
                &sub.filter.workstream_id,
                sub.filter.binding_revision,
                &obs.id,
                "DECISION",
                &hash,
                None
            )
            .is_err());
    }
    #[test]
    fn native_questions_have_typed_sources_without_fabricating_complete_replies() {
        let (_d, s, mut sub, _, _) = fixture();
        sub.name = DECISION_REQUIRED.into();
        s.save_event_subscription(&sub).unwrap();
        let question = serde_json::json!({"id":"runtime-action","kind":"USER_INPUT","questions":[{"id":"choice","prompt":"private fixture question"}]});
        s.observe_mcp_native_request("fixture-control", "native-turn", "42", &question)
            .unwrap();
        s.observe_mcp_native_request("fixture-control", "native-turn", "42", &question)
            .unwrap();
        let d = s.claim_event_delivery().unwrap().unwrap();
        assert!(d.event.observation_id.starts_with("req_"));
        assert_eq!(d.event.name, DECISION_REQUIRED);
        let source = s
            .read_mcp_request_source(
                &sub.grant_id,
                &sub.filter.workstream_id,
                "DECISION",
                &d.event.observation_id,
            )
            .unwrap();
        assert_eq!(source["request"], question);
        assert_eq!(source["canPrepareHandoff"], false);
        assert!(s
            .reply_observations_for_workstream(&sub.filter.workstream_id)
            .unwrap()
            .is_empty());
        assert!(s
            .prepare_role_handoff(
                &sub.filter.workstream_id,
                "DECISION",
                &d.event.observation_id,
                "must not forward a question as a complete result"
            )
            .is_err());
        assert!(s
            .read_mcp_request_source(
                &sub.grant_id,
                &sub.filter.workstream_id,
                "EXECUTION",
                &d.event.observation_id
            )
            .is_err());
        s.finish_event_delivery(&sub.id, &d.event.event_id, d.attempts, Some(200))
            .unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
    }
    #[test]
    fn native_request_resolution_tombstones_stop_late_capture_and_late_delivery_ack() {
        let (_d, s, mut sub, _, _) = fixture();
        sub.name = DECISION_REQUIRED.into();
        s.save_event_subscription(&sub).unwrap();
        s.resolve_mcp_native_request("fixture-control", Some("already-terminal"), None)
            .unwrap();
        s.observe_mcp_native_request(
            "fixture-control",
            "already-terminal",
            "1",
            &serde_json::json!({"id":"late"}),
        )
        .unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
        s.resolve_mcp_native_request("fixture-control", None, Some("2"))
            .unwrap();
        s.observe_mcp_native_request(
            "fixture-control",
            "other-turn",
            "2",
            &serde_json::json!({"id":"already-resolved"}),
        )
        .unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
        s.observe_mcp_native_request(
            "fixture-control",
            "live-turn",
            "3",
            &serde_json::json!({"id":"live"}),
        )
        .unwrap();
        let d = s.claim_event_delivery().unwrap().unwrap();
        assert!(s
            .event_message_still_allowed(&sub.id, &d.event.event_id)
            .unwrap());
        s.resolve_mcp_native_request("fixture-control", None, Some("3"))
            .unwrap();
        assert!(!s
            .event_message_still_allowed(&sub.id, &d.event.event_id)
            .unwrap());
        s.finish_event_delivery(&sub.id, &d.event.event_id, d.attempts, Some(200))
            .unwrap();
        assert_eq!(
            s.event_status_for_grant(&sub.grant_id).unwrap()["webhookReceipts"],
            0
        );
        assert!(s.claim_event_delivery().unwrap().is_none());
    }
    #[test]
    fn exact_native_round_trip_yields_a_real_owner_decision_not_an_unbounded_relay() {
        let (_d, s, base, decision, execution) = fixture();
        let g = s
            .create_assistant_instance_grant(assistant::InstanceGrantInput {
                label: "fixture instance".into(),
                rules: vec![BriefRule {
                    id: "continue".into(),
                    text: "Continue".into(),
                }],
                expires_at: now() + 86400000,
            })
            .unwrap();
        let mut sub = base.clone();
        sub.grant_id = g.id.clone();
        s.save_event_subscription(&sub).unwrap();
        let mut other = sub.clone();
        other.id = "fixture-exec".into();
        other.filter.source_role = "EXECUTION".into();
        s.save_event_subscription(&other).unwrap();
        let mut stop = sub.clone();
        stop.id = "fixture-stop".into();
        stop.name = DECISION_REQUIRED.into();
        s.save_event_subscription(&stop).unwrap();
        let first = complete(&s, &sub, &decision, "codex:root-turn:root-item");
        let event = s.claim_event_delivery().unwrap().unwrap();
        s.finish_event_delivery(
            &event.subscription.id,
            &event.event.event_id,
            event.attempts,
            Some(200),
        )
        .unwrap();
        let h = s
            .prepare_role_handoff(
                &sub.filter.workstream_id,
                "DECISION",
                &first.id,
                "fixture instruction",
            )
            .unwrap();
        s.register_assistant_prepare_event(
            &g.id,
            &h.id,
            &first.id,
            "one",
            &"a".repeat(64),
            &"a".repeat(64),
            Some(&event.event.event_id),
        )
        .unwrap();
        s.note_mcp_native_cause(&h.id, "fixture-execution", "next-turn")
            .unwrap();
        let second = complete(&s, &sub, &execution, "codex:next-turn:next-item");
        let event2 = s.claim_event_delivery().unwrap().unwrap();
        assert_eq!(event2.event.root_event_id, event.event.event_id);
        assert_eq!(event2.event.hop_count, 1);
        s.finish_event_delivery(
            &event2.subscription.id,
            &event2.event.event_id,
            event2.attempts,
            Some(200),
        )
        .unwrap();
        let back = s
            .prepare_role_handoff(
                &sub.filter.workstream_id,
                "EXECUTION",
                &second.id,
                "fixture return",
            )
            .unwrap();
        s.register_assistant_prepare_event(
            &g.id,
            &back.id,
            &second.id,
            "two",
            &"b".repeat(64),
            &"b".repeat(64),
            Some(&event2.event.event_id),
        )
        .unwrap();
        s.note_mcp_native_cause(&back.id, "fixture-control", "return-turn")
            .unwrap();
        complete(&s, &sub, &decision, "codex:return-turn:return-item");
        let boundary = s.claim_event_delivery().unwrap().unwrap();
        assert_eq!(boundary.event.name, DECISION_REQUIRED);
        assert_eq!(boundary.event.reason.as_deref(), Some("LOOP_BOUNDARY"));
        assert_eq!(boundary.event.root_event_id, event.event.event_id);
        assert_eq!(boundary.event.hop_count, 2);
        let ready = s
            .prepare_role_handoff(
                &sub.filter.workstream_id,
                "DECISION",
                &boundary.event.observation_id,
                "requires actual owner",
            )
            .unwrap();
        s.register_assistant_prepare_event(
            &g.id,
            &ready.id,
            &boundary.event.observation_id,
            "three",
            &"c".repeat(64),
            &"c".repeat(64),
            Some(&boundary.event.event_id),
        )
        .unwrap();
        assert_eq!(
            s.assistant_decisions(&g.id)
                .unwrap()
                .iter()
                .filter(|d| d.answer.is_none())
                .count(),
            1
        );
    }
    #[test]
    fn subscription_identity_limit_and_historical_completion_are_fenced() {
        let (_d, s, base, decision, _) = fixture();
        s.save_event_subscription(&base).unwrap();
        let mut changed = base.clone();
        changed.callback_url = "https://elsewhere.example/callback".into();
        assert!(s.save_event_subscription(&changed).is_err());
        let obs = s
            .record_reply_observation(
                &base.filter.workstream_id,
                &decision,
                Some("old-terminal"),
                "Old historical body",
                None,
            )
            .unwrap()
            .unwrap();
        s.note_reply_completion(
            &base.filter.workstream_id,
            &decision,
            "old-terminal",
            Some(1),
        )
        .unwrap();
        assert!(s.claim_event_delivery().unwrap().is_none());
        for i in 1..32 {
            let mut sub = base.clone();
            sub.id = format!("limit-{i}");
            s.save_event_subscription(&sub).unwrap();
        }
        let mut overflow = base.clone();
        overflow.id = "limit-33".into();
        assert_eq!(
            s.save_event_subscription(&overflow).unwrap_err(),
            "MCP_EVENT_SUBSCRIPTION_LIMIT"
        );
        s.unsubscribe_event(&base.grant_id, &base.id).unwrap();
        s.save_event_subscription(&overflow).unwrap();
        assert_eq!(
            s.save_event_subscription(&base).unwrap_err(),
            "MCP_EVENT_SUBSCRIPTION_LIMIT"
        );
        assert_eq!(
            s.reply_observations_for_workstream(&base.filter.workstream_id)
                .unwrap()[0]
                .id,
            obs.id
        );
    }
    #[test]
    fn exact_provider_run_lineage_and_passive_chatgpt_baseline_are_preserved() {
        let (_d, s, sub, decision, execution) = fixture();
        s.save_event_subscription(&sub).unwrap();
        let obs = complete(&s, &sub, &decision, "codex:lineage-root:item");
        let first = s.claim_event_delivery().unwrap().unwrap();
        let h = s
            .prepare_role_handoff(
                &sub.filter.workstream_id,
                "DECISION",
                &obs.id,
                "Exact outgoing instruction",
            )
            .unwrap();
        s.register_assistant_prepare_event(
            &sub.grant_id,
            &h.id,
            &obs.id,
            "lineage",
            &"a".repeat(64),
            &"a".repeat(64),
            Some(&first.event.event_id),
        )
        .unwrap();
        s.with_connection(|c| {
            c.execute(
                "UPDATE endpoints SET provider='CHATGPT' WHERE id=?1",
                [&execution],
            )
            .map_err(db_error)?;
            Ok(())
        })
        .unwrap();
        let run = s
            .create_provider_run(
                &sub.filter.workstream_id,
                &execution,
                "CHATGPT",
                Some(&h.id),
                Some("exact-provider-run"),
                "RUNNING",
            )
            .unwrap();
        s.accept_completed_provider_result(
            &run.id,
            "exact-provider-run",
            "chatgpt-message",
            "Exact provider result".into(),
        )
        .unwrap();
        let result = s
            .record_reply_observation(
                &sub.filter.workstream_id,
                &execution,
                Some("chatgpt-message"),
                "Exact provider result",
                None,
            )
            .unwrap()
            .unwrap();
        s.note_reply_completion(
            &sub.filter.workstream_id,
            &execution,
            "chatgpt-message",
            None,
        )
        .unwrap();
        s.with_connection(|c| {
            let (root, hops): (String, i64) = c
                .query_row(
                    "SELECT root_event_id,hop_count FROM mcp_events WHERE observation_id=?1",
                    [&result.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(db_error)?;
            assert_eq!(root, first.event.event_id);
            assert_eq!(hops, 1);
            Ok(())
        })
        .unwrap();
        let (_baseline_dir,s,sub,decision,_)=fixture();
        s.with_connection(|c| {
            c.execute(
                "UPDATE endpoints SET provider='CHATGPT' WHERE id=?1",
                [&decision],
            )
            .map_err(db_error)?;
            Ok(())
        })
        .unwrap();
        let baseline = complete(&s, &sub, &decision, "first-passive-chatgpt");
        s.with_connection(|c| {
            let present: bool = c
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM mcp_events WHERE observation_id=?1)",
                    [&baseline.id],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            assert!(!present);
            Ok(())
        })
        .unwrap();
        let next = complete(&s, &sub, &decision, "new-passive-chatgpt");
        s.with_connection(|c| {
            let present: bool = c
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM mcp_events WHERE observation_id=?1)",
                    [&next.id],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            assert!(present);
            Ok(())
        })
        .unwrap();
    }
}
