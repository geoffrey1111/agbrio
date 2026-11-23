//! Metadata-only reminders on the existing, guarded management notice route.
use super::{
    codex_notices::{self, Subscription},
    management_zones::validate_subscription,
    *,
};

const MINUTE: i64 = 60_000;
pub(super) const PROOF_LIFETIME: i64 = 3 * MINUTE;
const KIND: &str = "AWAITING_MANAGER";

pub(super) fn decorate_backlog(c: &Connection, rows: &mut [Value]) -> Result<(), String> {
    for row in rows.iter_mut().filter(|row| row["kind"] == KIND) {
        let id = row["noticeId"].as_str().unwrap_or_default();
        let thread = row["threadId"].as_str().unwrap_or_default();
        let basis:Option<(i64,String)>=c.query_row("SELECT e.source_at,w.label FROM management_reminder_evidence e JOIN codex_watches w ON w.thread_id=?2 WHERE e.notice_id=?1",params![id,thread],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
        if let Some((at, label)) = basis {
            row["threadLabel"] = Value::String(
                label
                    .chars()
                    .filter(|ch| !ch.is_control())
                    .take(120)
                    .collect(),
            );
            row["waitingMinutes"] = serde_json::json!(now().saturating_sub(at) / MINUTE);
        }
    }
    Ok(())
}

pub struct PendingQuestion<'a> {
    pub turn: &'a str,
    pub request: &'a str,
    pub observed_at: i64,
}

fn handled(c: &Connection, s: &Subscription, since: i64) -> Result<bool, String> {
    // An INSTANCE grant does not attest a native manager. Count only an actual
    // successful authorized follow-up on the exact thread; a draft or QUEUED / UNKNOWN send cannot clear a reminder.
    c.query_row("SELECT EXISTS(SELECT 1 FROM assistant_actions a WHERE json_extract(a.input_json,'$.threadId')=?1
        AND (CASE WHEN a.operation='SEND_CHAT' THEN EXISTS(SELECT 1 FROM watch_replies w WHERE w.id=a.id AND w.thread_id=?1 AND w.generation=?2 AND w.status='SENT' AND w.updated_at>?3)
                  WHEN a.operation IN ('RESPOND_CHAT','STOP_CHAT') THEN a.status='APPLIED' AND a.updated_at>?3 AND (json_extract(a.result_json,'$.ok')=1 OR (a.operation='RESPOND_CHAT' AND json_extract(a.result_json,'$.status')='SENT'))
                  ELSE 0 END))", params![s.binding.thread_id,s.binding.generation,since], |r| r.get(0)).map_err(db_error)
}

fn identity(s: &Subscription, turn: &str, request: Option<&str>, tier: &str) -> String {
    length_prefixed_hash(&[
        b"management-reminder-v1",
        s.management_zone_id.as_deref().unwrap_or("").as_bytes(),
        s.binding.thread_id.as_bytes(),
        &s.binding.generation.to_be_bytes(),
        turn.as_bytes(),
        request.unwrap_or("").as_bytes(),
        tier.as_bytes(),
    ])
}

fn tier(age: i64, question: bool) -> Option<&'static str> {
    if question {
        (age >= 10 * MINUTE).then_some("QUESTION_10M")
    } else if age >= 120 * MINUTE {
        Some("COMPLETED_120M")
    } else if age >= 30 * MINUTE {
        Some("COMPLETED_30M")
    } else {
        None
    }
}

pub(super) fn validate_native_kind(kind: &str) -> Result<(), String> {
    if kind == KIND {
        return Err("MANAGEMENT_REMINDER_READ_ONLY".into());
    }
    Ok(())
}

fn enqueue(
    c: &Connection,
    s: &Subscription,
    turn: &str,
    request: Option<&str>,
    state: &str,
    since: i64,
    at: i64,
    label: &str,
) -> Result<String, String> {
    let id = identity(s, turn, request, state);
    let label = serde_json::to_string(
        &label
            .chars()
            .filter(|ch| !ch.is_control())
            .take(120)
            .collect::<String>(),
    )
    .map_err(|_| "MANAGEMENT_REMINDER_LABEL_INVALID")?;
    let body = format!(
        "{}\nthreadLabel (display only): {label}\nwaitingMinutes: {}",
        codex_notices::text(s, &id, turn, KIND, state, request),
        at.saturating_sub(since) / MINUTE
    );
    // A never-admitted notice can be re-evaluated, including after reassignment.
    // Delivered / unknown notices retain their identity and native recipient.
    c.execute("INSERT INTO claude_codex_notices(id,subscription_id,turn_id,kind,source_state,request_id,text,created_at,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'BACKLOG')
      ON CONFLICT(id) DO UPDATE SET subscription_id=excluded.subscription_id,text=excluded.text,status=excluded.status
      WHERE claude_codex_notices.status IN ('READY','BACKLOG','OBSOLETE') AND claimed_at IS NULL AND offered_at IS NULL AND NOT EXISTS(SELECT 1 FROM management_codex_attempts WHERE notice_id=excluded.id)",params![id,s.id,turn,KIND,state,request,body,at]).map_err(db_error)?;
    c.execute("INSERT INTO management_reminder_evidence VALUES(?1,?2,?3) ON CONFLICT(notice_id) DO UPDATE SET source_at=excluded.source_at, verified_at=excluded.verified_at",params![id,since,at]).map_err(db_error)?;
    Ok(id)
}

impl RouterStore {
    /// Call after an exact native observation. `wait_at` is a validated external
    /// marker; its contents never enter the notification or the executor.
    pub fn observe_management_reminders(
        &self,
        zone: &str,
        thread: &str,
        generation: i64,
        turn: &str,
        native_state: &str,
        questions: &[PendingQuestion<'_>],
        wait_at: Option<i64>,
        at: i64,
    ) -> Result<(), String> {
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let route=tx.query_row("SELECT * FROM claude_codex_subscriptions WHERE management_zone_id=?1 AND thread_id=?2 AND generation=?3 AND status='ACTIVE'",params![zone,thread,generation],codex_notices::subscription).optional().map_err(db_error)?;
            let Some(s)=route else { return Ok(()); };
            validate_subscription(&tx,&s)?;
            let w=super::codex_watch::chat_context(&tx,thread)?;
            let mut keep=Vec::new();
            if native_state=="completed" && w.snapshot.state=="RESULT_READY" && w.snapshot.turn_id.as_deref()==Some(turn) {
                let finished:Option<i64>=tx.query_row("SELECT MIN(n.created_at) FROM claude_codex_notices n JOIN claude_codex_subscriptions source ON source.id=n.subscription_id WHERE source.management_zone_id=?1 AND source.thread_id=?2 AND source.generation=?3 AND source.source_cwd=?4 AND n.turn_id=?5 AND n.kind='TURN_RESULT' AND n.source_state='completed'",params![zone,thread,generation,s.binding.source_cwd,turn],|r| r.get(0)).map_err(db_error)?;
                if let Some(since)=finished.filter(|v|*v>0) {
                    if let Some(state)=tier(at.saturating_sub(since),false) {
                        if !wait_at.is_some_and(|stamp|stamp>=since) && !handled(&tx,&s,since)? {
                            keep.push(enqueue(&tx,&s,turn,None,state,since,at,&w.label)?);
                        }
                    }
                }
            }
            for q in questions.iter().take(32) {
                if q.turn.is_empty() || q.turn.len()>256 || q.request.is_empty() || q.request.len()>256 || q.observed_at<=0 { continue; }
                let Some(state)=tier(at.saturating_sub(q.observed_at),true) else { continue; };
                keep.push(enqueue(&tx,&s,q.turn,Some(q.request),state,q.observed_at,at,&w.label)?);
            }
            // Cancel pending offers on settled work; never reset an ambiguous append or a receipt.
            tx.execute("UPDATE claude_codex_notices SET status='OBSOLETE' WHERE subscription_id=?1 AND kind='AWAITING_MANAGER' AND status IN ('READY','BACKLOG') AND NOT EXISTS(SELECT 1 FROM json_each(?2) WHERE value=claude_codex_notices.id)",params![s.id,serde_json::to_string(&keep).map_err(|_|"MANAGEMENT_REMINDER_EVIDENCE_INVALID")?]).map_err(db_error)?;
            tx.commit().map_err(db_error)
        })
    }
}
