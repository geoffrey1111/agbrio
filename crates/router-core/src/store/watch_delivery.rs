//! Durable, provider-independent delivery records. No credentials or transports.
use super::codex_watch::WatchEvent;
use super::*;
use serde::Deserialize;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryChannel {
    pub channel: String,
    pub enabled: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliverySettings {
    pub host_id: String,
    pub account: String,
    pub recipient: String,
    pub channels: Vec<DeliveryChannel>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryRecord {
    pub event_sequence: i64,
    pub channel: String,
    pub status: String,
    pub attempts: i64,
    pub error_code: Option<String>,
    pub updated_at: i64,
}
fn channel_valid(channel: &str) -> Result<(), String> {
    if matches!(channel, "WEB" | "WINDOWS" | "EMAIL") {
        Ok(())
    } else {
        Err("DELIVERY_CHANNEL_INVALID".into())
    }
}
impl RouterStore {
    pub fn watch_delivery_settings(&self) -> Result<DeliverySettings, String> {
        self.with_connection(|c| {
            c.execute(
                "INSERT OR IGNORE INTO watch_delivery_settings(id,host_id) VALUES(1,?1)",
                [Uuid::new_v4().to_string()],
            )
            .map_err(db_error)?;
            let (host_id, account, recipient) = c
                .query_row(
                    "SELECT host_id,account,recipient FROM watch_delivery_settings WHERE id=1",
                    [],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    },
                )
                .map_err(db_error)?;
            let mut s = c
                .prepare("SELECT channel,enabled FROM watch_delivery_channels ORDER BY channel")
                .map_err(db_error)?;
            let channels = s
                .query_map([], |r| {
                    Ok(DeliveryChannel {
                        channel: r.get(0)?,
                        enabled: r.get(1)?,
                    })
                })
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?;
            Ok(DeliverySettings {
                host_id,
                account,
                recipient,
                channels,
            })
        })
    }
    pub fn set_watch_delivery_channel(&self, channel: &str, enabled: bool) -> Result<(), String> {
        channel_valid(channel)?;
        self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
  tx.execute("UPDATE watch_delivery_channels SET enabled=?2,cursor=CASE WHEN enabled=0 AND ?2=1 THEN (SELECT COALESCE(MAX(sequence),0) FROM codex_watch_events) ELSE cursor END WHERE channel=?1",params![channel,enabled]).map_err(db_error)?;
  if !enabled {tx.execute("UPDATE watch_deliveries SET status='SKIPPED',error_code='CHANNEL_DISABLED',updated_at=?2 WHERE channel=?1 AND status IN ('PENDING','FAILED')",params![channel,now()]).map_err(db_error)?;}
  tx.commit().map_err(db_error)
 })
    }
    pub fn set_watch_email_addresses(&self, account: &str, recipient: &str) -> Result<(), String> {
        // Detailed mailbox validation lives in the SMTP adapter.
        if [account, recipient]
            .iter()
            .any(|v| v.is_empty() || v.len() > 254 || v.contains(['\r', '\n', '\0']))
        {
            return Err("EMAIL_ADDRESS_INVALID".into());
        }
        self.watch_delivery_settings()?;
        self.with_connection(|c| {
            c.execute(
                "UPDATE watch_delivery_settings SET account=?1,recipient=?2 WHERE id=1",
                params![account, recipient],
            )
            .map_err(db_error)?;
            Ok(())
        })
    }
    pub fn enqueue_watch_deliveries(&self) -> Result<(), String> {
        self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
  let channels={let mut statement=tx.prepare("SELECT channel,cursor FROM watch_delivery_channels WHERE enabled=1").map_err(db_error)?;
  let rows=statement.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;rows};
  for (channel,cursor) in channels {
   let mut s=tx.prepare("SELECT sequence,thread_id,snapshot_json FROM codex_watch_events WHERE sequence>?1 ORDER BY sequence LIMIT 100").map_err(db_error)?;
   let rows=s.query_map([cursor],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
   for (seq,thread,raw) in rows {
    let snapshot:codex_watch::WatchSnapshot=serde_json::from_str(&raw).map_err(|_|"WATCH_SNAPSHOT_INVALID")?;
    if matches!(snapshot.state.as_str(),"RESULT_READY"|"FAILED"|"ACTION_REQUIRED"){
     let key=if snapshot.state=="ACTION_REQUIRED"{length_prefixed_hash(&[thread.as_bytes(),snapshot.turn_id.as_deref().unwrap_or("").as_bytes(),snapshot.state.as_bytes(),snapshot.item_id.as_deref().unwrap_or("").as_bytes()])}else{length_prefixed_hash(&[thread.as_bytes(),snapshot.turn_id.as_deref().unwrap_or("").as_bytes(),snapshot.state.as_bytes()])};
     tx.execute("INSERT OR IGNORE INTO watch_deliveries(event_sequence,channel,notification_key,status,updated_at) VALUES(?1,?2,?3,'PENDING',?4)",params![seq,channel,key,now()]).map_err(db_error)?;
    }
    tx.execute("UPDATE watch_delivery_channels SET cursor=?2 WHERE channel=?1",params![channel,seq]).map_err(db_error)?;
   }
  }
  tx.commit().map_err(db_error)
 })
    }
    /// An interrupted network send is uncertain, so restart never blindly replays it.
    pub fn recover_watch_deliveries(&self) -> Result<(), String> {
        self.with_connection(|c|{c.execute("UPDATE watch_deliveries SET status='UNKNOWN',error_code='HOST_RESTARTED_DURING_SEND',updated_at=?1 WHERE status='SENDING'",[now()]).map_err(db_error)?;Ok(())})
    }
    pub fn claim_watch_delivery(&self) -> Result<Option<(String, WatchEvent)>, String> {
        self.with_connection(|c|{
   let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   let row=tx.query_row("SELECT d.event_sequence,d.channel FROM watch_deliveries d JOIN codex_watch_events e ON e.sequence=d.event_sequence JOIN watch_delivery_channels ch ON ch.channel=d.channel WHERE ch.enabled=1 AND d.status IN ('PENDING','FAILED') AND d.attempts<3 AND d.retry_at<=?1 ORDER BY d.event_sequence,d.channel LIMIT 1",[now()],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).optional().map_err(db_error)?;
   let result=if let Some((seq,channel))=row {
    let event=tx.query_row("SELECT sequence,thread_id,label,cwd,snapshot_json,observed_at FROM codex_watch_events WHERE sequence=?1",[seq],codex_watch::event_row).map_err(db_error)?;
    tx.execute("UPDATE watch_deliveries SET status='SENDING',attempts=attempts+1,updated_at=?3 WHERE event_sequence=?1 AND channel=?2",params![seq,channel,now()]).map_err(db_error)?;
    Some((channel,event))
   }else{None};
   tx.commit().map_err(db_error)?;Ok(result)
  })
    }
    pub fn finish_watch_delivery(
        &self,
        seq: i64,
        channel: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), String> {
        channel_valid(channel)?;
        if !matches!(status, "SENT" | "FAILED" | "UNKNOWN" | "SKIPPED")
            || error.is_some_and(|e| {
                e.len() > 80
                    || !e
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            })
        {
            return Err("DELIVERY_OUTCOME_INVALID".into());
        }
        self.with_connection(|c|{c.execute("UPDATE watch_deliveries SET status=?3,error_code=?4,retry_at=?5+30000*attempts,updated_at=?5 WHERE event_sequence=?1 AND channel=?2 AND status='SENDING'",params![seq,channel,status,error,now()]).map_err(db_error)?;Ok(())})
    }
    pub fn watch_delivery_history(&self) -> Result<Vec<DeliveryRecord>, String> {
        self.with_connection(|c|{
  let mut s=c.prepare("SELECT d.event_sequence,d.channel,d.status,d.attempts,d.error_code,d.updated_at FROM watch_deliveries d JOIN codex_watch_events e ON e.sequence=d.event_sequence ORDER BY d.event_sequence DESC,d.channel LIMIT 100").map_err(db_error)?;
  let rows=s.query_map([],|r|Ok(DeliveryRecord{event_sequence:r.get(0)?,channel:r.get(1)?,status:r.get(2)?,attempts:r.get(3)?,error_code:r.get(4)?,updated_at:r.get(5)?})).map_err(db_error)?;
  rows.collect::<Result<Vec<_>,_>>().map_err(db_error)
 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_upgrade_cleans_originals_without_losing_delivery_audit() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("legacy.db");
        let s = RouterStore::open_at(&path).unwrap();
        let baseline = codex_watch::WatchSnapshot {
            state: "IDLE".into(),
            turn_id: None,
            item_id: None,
            text: String::new(),
        };
        s.enable_codex_watch("a", "A", d.path().to_str().unwrap(), &baseline)
            .unwrap();
        let project = s.create_project("preserved".into(), None).unwrap();
        s.with_connection(|c|{
            c.execute_batch("DROP TABLE watch_deliveries; DELETE FROM router_feature_migrations WHERE key='watch-retention-v1';").map_err(db_error)?;
            let old_schema=include_str!("../../migrations/normal/008_watch_delivery.sql");
            let old_table=format!("CREATE TABLE watch_deliveries{}",old_schema.split("CREATE TABLE watch_deliveries").nth(1).unwrap());
            c.execute_batch(&old_table).map_err(db_error)?;
            for n in 1..=25 {
                let raw=serde_json::to_string(&codex_watch::WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some(format!("turn-{n}")),item_id:None,text:format!("original-{n}")}).unwrap();
                c.execute("INSERT INTO codex_watch_events(thread_id,label,cwd,snapshot_json,observed_at) VALUES('a','A',?1,?2,?3)",params![d.path().to_str().unwrap(),raw,now()]).map_err(db_error)?;
            }
            c.execute("INSERT INTO watch_deliveries(event_sequence,channel,notification_key,status,attempts,updated_at) VALUES(1,'WINDOWS','stable-old-intent','UNKNOWN',1,1)",[]).map_err(db_error)?;
            Ok(())
        }).unwrap();
        drop(s);
        let s = RouterStore::open_at(&path).unwrap();
        assert_eq!(
            s.codex_watch_feed(0)
                .unwrap()
                .events
                .first()
                .unwrap()
                .sequence,
            6
        );
        assert_eq!(s.codex_watch_feed(0).unwrap().events.len(), 20);
        assert!(s.codex_watch_event(1).is_err());
        s.with_connection(|c|{
            assert_eq!(c.query_row("SELECT status,attempts,notification_key FROM watch_deliveries WHERE event_sequence=1",[],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?))).map_err(db_error)?,("UNKNOWN".into(),1,"stable-old-intent".into()));
            assert_eq!(c.query_row("SELECT name FROM projects WHERE id=?1",[project.id.as_str()],|r|r.get::<_,String>(0)).map_err(db_error)?,"preserved");
            assert_eq!(c.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check",[],|r|r.get::<_,i64>(0)).map_err(db_error)?,0);Ok(())
        }).unwrap();
    }
    #[test]
    fn pruning_preserves_claimed_send_and_dedupe_but_expires_unclaimed_work() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("retention.db");
        let s = RouterStore::open_at(&path).unwrap();
        let snap = |turn: &str| codex_watch::WatchSnapshot {
            state: "RESULT_READY".into(),
            turn_id: Some(turn.into()),
            item_id: None,
            text: turn.into(),
        };
        s.enable_codex_watch("a", "A", d.path().to_str().unwrap(), &snap("baseline"))
            .unwrap();
        s.set_watch_delivery_channel("EMAIL", true).unwrap();
        s.record_codex_watch("a", 1, &snap("claimed")).unwrap();
        s.enqueue_watch_deliveries().unwrap();
        let (ch, claimed) = s.claim_watch_delivery().unwrap().unwrap();
        s.record_codex_watch("a", 1, &snap("pending")).unwrap();
        s.enqueue_watch_deliveries().unwrap();
        for n in 0..20 {
            s.record_codex_watch("a", 1, &snap(&format!("later-{n}")))
                .unwrap();
        }
        assert!(s.codex_watch_event(claimed.sequence).is_err());
        assert_eq!(claimed.snapshot.text, "claimed");
        s.finish_watch_delivery(claimed.sequence, &ch, "SENT", None)
            .unwrap();
        s.with_connection(|c| {
            assert_eq!(
                c.query_row(
                    "SELECT status FROM watch_deliveries WHERE event_sequence=1",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .map_err(db_error)?,
                "SENT"
            );
            assert_eq!(
                c.query_row(
                    "SELECT status FROM watch_deliveries WHERE event_sequence=2",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .map_err(db_error)?,
                "SKIPPED"
            );
            assert_eq!(
                c.query_row("SELECT COUNT(*) FROM codex_watch_events", [], |r| r
                    .get::<_, i64>(0))
                    .map_err(db_error)?,
                20
            );
            Ok(())
        })
        .unwrap();
        s.record_codex_watch("a", 1, &snap("claimed")).unwrap();
        s.enqueue_watch_deliveries().unwrap();
        let mut seen = Vec::new();
        while let Some((ch, e)) = s.claim_watch_delivery().unwrap() {
            seen.push(e.snapshot.turn_id.clone());
            s.finish_watch_delivery(e.sequence, &ch, "SENT", None)
                .unwrap();
        }
        assert!(!seen.contains(&Some("claimed".into())));
        drop(s);
        let s = RouterStore::open_at(&path).unwrap();
        assert_eq!(s.codex_watch_feed(0).unwrap().events.len(), 20);
        assert_eq!(s.codex_watch_feed(0).unwrap().next_cursor, 23);
        assert!(s.claim_watch_delivery().unwrap().is_none());
        assert!(
            s.with_connection(|c| c
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .map_err(db_error))
                .unwrap()
                == 0
        );
    }
    #[test]
    fn baseline_dedupe_claim_restart_and_channel_pause() {
        let d = tempfile::tempdir().unwrap();
        let s = RouterStore::open_at(d.path().join("d.db")).unwrap();
        let snap = |state: &str, turn: &str, text: &str| codex_watch::WatchSnapshot {
            state: state.into(),
            turn_id: Some(turn.into()),
            item_id: None,
            text: text.into(),
        };
        s.enable_codex_watch(
            "a",
            "A",
            d.path().to_str().unwrap(),
            &snap("IDLE", "old", ""),
        )
        .unwrap();
        s.record_codex_watch("a", 1, &snap("RESULT_READY", "old", "old"))
            .unwrap();
        s.set_watch_delivery_channel("EMAIL", true).unwrap();
        s.enqueue_watch_deliveries().unwrap();
        assert!(s.claim_watch_delivery().unwrap().is_none());
        s.record_codex_watch("a", 1, &snap("RUNNING", "new", "progress"))
            .unwrap();
        s.record_codex_watch("a", 1, &snap("RESULT_READY", "new", "result"))
            .unwrap();
        s.enqueue_watch_deliveries().unwrap();
        let (ch, e) = s.claim_watch_delivery().unwrap().unwrap();
        assert_eq!(e.snapshot.text, "result");
        assert!(s.claim_watch_delivery().unwrap().is_none());
        s.recover_watch_deliveries().unwrap();
        assert!(s.claim_watch_delivery().unwrap().is_none());
        assert_eq!(s.watch_delivery_history().unwrap()[0].status, "UNKNOWN");
        s.record_codex_watch("a", 1, &snap("RESULT_READY", "new", "updated result"))
            .unwrap();
        s.enqueue_watch_deliveries().unwrap();
        assert!(s.claim_watch_delivery().unwrap().is_none());
        s.record_codex_watch("a", 1, &snap("FAILED", "next", "failure"))
            .unwrap();
        s.enqueue_watch_deliveries().unwrap();
        let (_, e2) = s.claim_watch_delivery().unwrap().unwrap();
        s.finish_watch_delivery(e2.sequence, &ch, "FAILED", Some("SMTP_REJECTED"))
            .unwrap();
        assert!(s.claim_watch_delivery().unwrap().is_none());
        s.set_watch_delivery_channel("EMAIL", false).unwrap();
        assert_eq!(s.watch_delivery_history().unwrap()[0].status, "SKIPPED");
        assert_eq!(
            s.watch_delivery_settings().unwrap().recipient,
            ""
        );
        assert!(s
            .finish_watch_delivery(e.sequence, &ch, "SENT", Some("password secret"))
            .is_err());
    }
    #[test]
    fn bounded_retry_and_single_claim_across_store_reopen() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("retry.db");
        let s = RouterStore::open_at(&path).unwrap();
        let snap = |state: &str| codex_watch::WatchSnapshot {
            state: state.into(),
            turn_id: Some("turn".into()),
            item_id: None,
            text: "public".into(),
        };
        s.enable_codex_watch(
            "thread",
            "Thread",
            d.path().to_str().unwrap(),
            &snap("IDLE"),
        )
        .unwrap();
        s.set_watch_delivery_channel("EMAIL", true).unwrap();
        s.record_codex_watch("thread", 1, &snap("RESULT_READY"))
            .unwrap();
        s.enqueue_watch_deliveries().unwrap();
        let other = RouterStore::open_at(&path).unwrap();
        for attempt in 1..=3 {
            let (_, e) = s.claim_watch_delivery().unwrap().unwrap();
            assert!(other.claim_watch_delivery().unwrap().is_none());
            s.finish_watch_delivery(
                e.sequence,
                "EMAIL",
                "FAILED",
                Some("SMTP_TEMPORARILY_REJECTED"),
            )
            .unwrap();
            assert_eq!(s.watch_delivery_history().unwrap()[0].attempts, attempt);
            assert!(other.claim_watch_delivery().unwrap().is_none());
            s.with_connection(|c| {
                c.execute("UPDATE watch_deliveries SET retry_at=0", [])
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        }
        assert!(other.claim_watch_delivery().unwrap().is_none());
        assert_eq!(s.watch_delivery_history().unwrap()[0].attempts, 3);
    }
}
