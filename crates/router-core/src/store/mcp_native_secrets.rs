//! External-vault reference lifecycle. No OS calls, key material or transports.
use super::*;
const PREFIX: &str = "agbrio-native-key:v1:";
pub fn native_key_account(reference: &[u8]) -> Result<String, String> {
    let text = std::str::from_utf8(reference).map_err(|_| "MCP_EVENT_SECRET_REFERENCE_INVALID")?;
    let id = text
        .strip_prefix(PREFIX)
        .ok_or("MCP_EVENT_SECRET_REFERENCE_INVALID")?;
    let uuid = Uuid::parse_str(id).map_err(|_| "MCP_EVENT_SECRET_REFERENCE_INVALID")?;
    if uuid.get_version_num() != 4 || uuid.to_string() != id {
        return Err("MCP_EVENT_SECRET_REFERENCE_INVALID".into());
    }
    Ok(format!("v1:{id}"))
}
pub(super) fn attach(c: &Connection, gid: &str, sid: &str, reference: &[u8]) -> Result<(), String> {
    if !reference.starts_with(PREFIX.as_bytes()) {
        return Ok(());
    } // Existing protected Windows blobs.
    native_key_account(reference)?;
    let changed=c.execute("UPDATE mcp_native_secret_refs SET state='IN_USE' WHERE reference=?1 AND subscription_id=?2 AND grant_id=?3 AND state IN ('RESERVED','IN_USE') AND creation_settled=1 AND created=1",params![reference,sid,gid]).map_err(db_error)?;
    if changed != 1 {
        return Err("MCP_EVENT_SECRET_REFERENCE_UNCONFIRMED".into());
    }
    Ok(())
}
#[derive(Clone)]
pub struct NativeKeyDeletion {
    pub reference: Vec<u8>,
    pub lease: String,
    pub revision: i64,
    pub creation_settled: bool,
    pub created: bool,
}
impl RouterStore {
    pub fn reserve_mcp_native_key(&self, gid: &str, sid: &str) -> Result<Vec<u8>, String> {
        if sid.is_empty() || sid.len() > 200 || sid.chars().any(char::is_control) {
            return Err("MCP_EVENT_SECRET_REFERENCE_INVALID".into());
        }
        let reference = format!("{PREFIX}{}", Uuid::new_v4()).into_bytes();
        self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;assistant::active(&tx,gid)?;
            let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM mcp_native_secret_refs WHERE subscription_id=?1 AND state='RESERVED')",[sid],|r|r.get(0)).map_err(db_error)?;
            if pending{return Err("MCP_EVENT_SECRET_REFERENCE_PENDING".into())}
            let targets:Vec<String>={let mut q=tx.prepare("SELECT id FROM mcp_event_subscriptions WHERE grant_id=?1 AND status='ACTIVE' AND expires_at>?2 UNION SELECT subscription_id FROM mcp_native_secret_refs WHERE grant_id=?1 AND state='RESERVED'").map_err(db_error)?;let rows=q.query_map(params![gid,now()],|r|r.get(0)).map_err(db_error)?.collect::<rusqlite::Result<Vec<String>>>().map_err(db_error)?;rows};
            if targets.len()>=32&&!targets.iter().any(|id|id==sid){return Err("MCP_EVENT_SUBSCRIPTION_LIMIT".into())}
            tx.execute("INSERT INTO mcp_native_secret_refs(reference,subscription_id,grant_id,state,reserved_at) VALUES(?1,?2,?3,'RESERVED',?4)",params![reference,sid,gid,now()]).map_err(db_error)?;tx.commit().map_err(db_error)?;Ok(reference)
        })
    }
    /// Called in the same blocking operation that performs native Add, even if
    /// the awaiting HTTP task is cancelled. A late settlement invalidates GC ACKs.
    pub fn settle_mcp_native_key(&self, reference: &[u8], created: bool) -> Result<(), String> {
        native_key_account(reference)?;
        self.with_connection(|c|{c.execute("UPDATE mcp_native_secret_refs SET creation_settled=1,created=?2,revision=revision+1,delete_after=0,delete_lease=NULL,delete_lease_until=NULL,deleted_at=NULL WHERE reference=?1 AND creation_settled=0",params![reference,created]).map_err(db_error)?;Ok(())})
    }
    pub fn require_mcp_native_key_reservation(
        &self,
        gid: &str,
        sid: &str,
        reference: &[u8],
    ) -> Result<(), String> {
        native_key_account(reference)?;
        self.with_connection(|c|{assistant::active(c,gid)?;let ok:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM mcp_native_secret_refs WHERE reference=?1 AND grant_id=?2 AND subscription_id=?3 AND state='RESERVED' AND creation_settled=0)",params![reference,gid,sid],|r|r.get(0)).map_err(db_error)?;if ok{Ok(())}else{Err("MCP_EVENT_SECRET_REFERENCE_UNCONFIRMED".into())}})
    }
    pub fn retire_reserved_mcp_native_key(&self, reference: &[u8]) -> Result<(), String> {
        native_key_account(reference)?;
        self.with_connection(|c|{c.execute("UPDATE mcp_native_secret_refs SET state='RETIRED',revision=revision+1,delete_after=0,delete_lease=NULL,delete_lease_until=NULL,deleted_at=NULL WHERE reference=?1 AND state='RESERVED'",[reference]).map_err(db_error)?;Ok(())})
    }
    pub fn require_mcp_native_key(&self, sid: &str, reference: &[u8]) -> Result<(), String> {
        native_key_account(reference)?;
        self.with_connection(|c|{let ok:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM mcp_native_secret_refs WHERE reference=?1 AND subscription_id=?2 AND state IN ('RESERVED','IN_USE') AND creation_settled=1 AND created=1)",params![reference,sid],|r|r.get(0)).map_err(db_error)?;if ok{Ok(())}else{Err("MCP_EVENT_SECRET_REFERENCE_UNCONFIRMED".into())}})
    }
    pub fn claim_mcp_native_key_deletions(&self) -> Result<Vec<NativeKeyDeletion>, String> {
        self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let at=now();
            tx.execute("UPDATE mcp_native_secret_refs SET state='RETIRED',revision=revision+1,delete_after=0,delete_lease=NULL,delete_lease_until=NULL WHERE state='RESERVED' AND reserved_at<=?1",[at-300000]).map_err(db_error)?;
            let rows={let mut q=tx.prepare("SELECT reference,revision,creation_settled,created FROM mcp_native_secret_refs WHERE state='RETIRED' AND deleted_at IS NULL AND delete_after<=?1 AND (delete_lease_until IS NULL OR delete_lease_until<=?1) ORDER BY reserved_at LIMIT 16").map_err(db_error)?;let rows=q.query_map([at],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,i64>(1)?,r.get::<_,bool>(2)?,r.get::<_,bool>(3)?))).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;rows};
            let mut result=Vec::new();
            for (reference,revision,creation_settled,created) in rows{
                native_key_account(&reference)?;let lease=Uuid::new_v4().to_string();
                tx.execute("UPDATE mcp_native_secret_refs SET delete_lease=?2,delete_lease_until=?3 WHERE reference=?1",params![reference,lease,at+30000]).map_err(db_error)?;
                result.push(NativeKeyDeletion{reference,lease,revision,creation_settled,created});
            }
            tx.commit().map_err(db_error)?;Ok(result)
        })
    }
    pub fn finish_mcp_native_key_deletion(
        &self,
        claim: &NativeKeyDeletion,
        result: Result<(), &str>,
    ) -> Result<(), String> {
        let error = match result {
            Ok(()) => None,
            Err("KEYCHAIN_OWNER_ACTION_REQUIRED") => Some("KEYCHAIN_OWNER_ACTION_REQUIRED"),
            Err(_) => Some("KEYCHAIN_UNAVAILABLE"),
        };
        self.with_connection(|c|{c.execute("UPDATE mcp_native_secret_refs SET deleted_at=CASE WHEN ?4 IS NULL AND creation_settled=1 THEN ?5 ELSE NULL END,delete_after=?5+60000,last_error=?4,delete_lease=NULL,delete_lease_until=NULL WHERE reference=?1 AND state='RETIRED' AND revision=?2 AND delete_lease=?3",params![claim.reference,claim.revision,claim.lease,error,now()]).map_err(db_error)?;Ok(())})
    }
    /// Exact references for a desktop-only owner authorization. No secret data.
    pub fn mcp_native_keys_for_owner(&self, after: Option<&[u8]>) -> Result<Vec<Vec<u8>>, String> {
        if let Some(after) = after {
            native_key_account(after)?;
        }
        self.with_connection(|c|{let mut q=c.prepare("SELECT reference FROM mcp_native_secret_refs WHERE (state='IN_USE' OR (state='RETIRED' AND deleted_at IS NULL)) AND created=1 AND reference>?1 ORDER BY reference LIMIT 65").map_err(db_error)?;let rows=q.query_map([after.unwrap_or_default()],|r|r.get(0)).map_err(db_error)?.collect::<rusqlite::Result<Vec<Vec<u8>>>>().map_err(db_error)?;Ok(rows)})
    }
    pub fn mcp_native_key_cleanup_pending(&self) -> Result<i64, String> {
        self.with_connection(|c|c.query_row("SELECT COUNT(*) FROM mcp_native_secret_refs WHERE state='RETIRED' AND deleted_at IS NULL",[],|r|r.get(0)).map_err(db_error))
    }
    pub fn expedite_mcp_native_key_cleanup(&self) -> Result<(), String> {
        self.with_connection(|c|{c.execute("UPDATE mcp_native_secret_refs SET delete_after=0 WHERE state='RETIRED' AND deleted_at IS NULL",[]).map_err(db_error)?;Ok(())})
    }
    pub fn note_mcp_native_key_access(
        &self,
        reference: &[u8],
        result: Result<(), &str>,
    ) -> Result<(), String> {
        native_key_account(reference)?;
        let error = match result {
            Ok(()) => None,
            Err("KEYCHAIN_OWNER_ACTION_REQUIRED") => Some("KEYCHAIN_OWNER_ACTION_REQUIRED"),
            Err(_) => Some("KEYCHAIN_UNAVAILABLE"),
        };
        self.with_connection(|c| {
            c.execute(
                "UPDATE mcp_native_secret_refs SET last_error=?2 WHERE reference=?1",
                params![reference, error],
            )
            .map_err(db_error)?;
            Ok(())
        })
    }
    pub fn mcp_native_key_owner_action_required(&self) -> Result<bool, String> {
        self.with_connection(|c|c.query_row("SELECT EXISTS(SELECT 1 FROM mcp_native_secret_refs WHERE (state='IN_USE' OR (state='RETIRED' AND deleted_at IS NULL)) AND last_error='KEYCHAIN_OWNER_ACTION_REQUIRED')",[],|r|r.get(0)).map_err(db_error))
    }
    pub fn mcp_native_key_owner_count(&self) -> Result<i64, String> {
        self.with_connection(|c|c.query_row("SELECT COUNT(*) FROM mcp_native_secret_refs WHERE state='IN_USE' OR (state='RETIRED' AND deleted_at IS NULL)",[],|r|r.get(0)).map_err(db_error))
    }
}

#[cfg(test)]
mod tests {
    use super::super::mcp_events::{EventFilter, EventSubscription, REPLY_READY};
    use super::*;
    fn fixture() -> (tempfile::TempDir, RouterStore, EventSubscription) {
        let d = tempfile::tempdir().unwrap();
        let s = RouterStore::open_at(d.path().join("fixture.db")).unwrap();
        let p = s.create_project("Native key fixture".into(), None).unwrap();
        let w = s.create_workstream(&p.id, "Key lifecycle".into()).unwrap();
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
                side("key-control"),
                side("key-execution"),
            )
            .unwrap();
        let g = s
            .create_assistant_grant(assistant::GrantInput {
                workstream_id: w.id.clone(),
                source_role: "DECISION".into(),
                binding_revision: b.binding_revision,
                label: "key fixture".into(),
                rules: vec![assistant::BriefRule {
                    id: "continue".into(),
                    text: "fixture".into(),
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
            callback_url: "https://example.invalid/callback".into(),
            secret: vec![1],
            previous_secret: None,
            rotation_until: None,
            verified_at: now(),
            expires_at: now() + 600000,
            status: "ACTIVE".into(),
        };
        (d, s, sub)
    }
    fn key(s: &RouterStore, gid: &str, sid: &str) -> Vec<u8> {
        let r = s.reserve_mcp_native_key(gid, sid).unwrap();
        s.settle_mcp_native_key(&r, true).unwrap();
        r
    }
    #[test]
    fn only_settled_exact_subscription_references_can_activate() {
        let (_d, s, mut sub) = fixture();
        let r = s.reserve_mcp_native_key(&sub.grant_id, &sub.id).unwrap();
        sub.secret = r.clone();
        assert!(s.save_event_subscription(&sub).is_err());
        s.settle_mcp_native_key(&r, true).unwrap();
        sub.id = "another-sub".into();
        assert!(s.save_event_subscription(&sub).is_err());
        sub.id = "fixture-sub".into();
        s.save_event_subscription(&sub).unwrap();
        assert!(s.require_mcp_native_key(&sub.id, &r).is_ok());
        assert!(s.require_mcp_native_key("another-sub", &r).is_err());
        s.retire_reserved_mcp_native_key(&r).unwrap();
        assert!(
            s.require_mcp_native_key(&sub.id, &r).is_ok(),
            "post-commit Drop cannot retire an active key"
        );
        assert!(native_key_account(b"agbrio-native-key:v1:../../foreign").is_err());
    }
    #[test]
    fn rotation_keeps_overlap_and_retires_previous_only_when_removed() {
        let (_d, s, mut sub) = fixture();
        let first = key(&s, &sub.grant_id, &sub.id);
        sub.secret = first.clone();
        s.save_event_subscription(&sub).unwrap();
        let second = key(&s, &sub.grant_id, &sub.id);
        sub.secret = second.clone();
        sub.previous_secret = Some(first.clone());
        sub.rotation_until = Some(now() + 300000);
        s.save_event_subscription(&sub).unwrap();
        assert!(s.claim_mcp_native_key_deletions().unwrap().is_empty());
        assert!(s.require_mcp_native_key(&sub.id, &first).is_ok());
        s.save_event_subscription(&sub).unwrap();
        assert!(
            s.claim_mcp_native_key_deletions().unwrap().is_empty(),
            "same-reference renewal cannot retire either key"
        );
        s.with_connection(|c| {
            c.execute(
                "UPDATE mcp_event_subscriptions SET rotation_until=0 WHERE id=?1",
                [&sub.id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
        s.claim_event_delivery().unwrap();
        let retired = s.claim_mcp_native_key_deletions().unwrap();
        assert_eq!(retired.len(), 1);
        assert_eq!(retired[0].reference, first);
        assert!(s.require_mcp_native_key(&sub.id, &second).is_ok());
        s.finish_mcp_native_key_deletion(&retired[0], Ok(()))
            .unwrap();
        assert_eq!(s.mcp_native_key_cleanup_pending().unwrap(), 0);
    }
    #[test]
    fn unsubscribe_expiry_and_revocation_queue_exact_cleanup() {
        for mode in ["unsubscribe", "expiry", "revoke", "cascade"] {
            let (_d, s, mut sub) = fixture();
            let r = key(&s, &sub.grant_id, &sub.id);
            sub.secret = r.clone();
            s.save_event_subscription(&sub).unwrap();
            match mode {
                "unsubscribe" => s.unsubscribe_event(&sub.grant_id, &sub.id).unwrap(),
                "revoke" => {
                    s.revoke_assistant_grant(&sub.grant_id).unwrap();
                    s.claim_event_delivery().unwrap();
                }
                "expiry" => {
                    s.with_connection(|c| {
                        c.execute(
                            "UPDATE mcp_event_subscriptions SET expires_at=0 WHERE id=?1",
                            [&sub.id],
                        )
                        .unwrap();
                        Ok(())
                    })
                    .unwrap();
                    s.claim_event_delivery().unwrap();
                }
                _ => s
                    .with_connection(|c| {
                        c.execute("DELETE FROM assistant_grants WHERE id=?1", [&sub.grant_id])
                            .unwrap();
                        Ok(())
                    })
                    .unwrap(),
            }
            assert!(s.require_mcp_native_key(&sub.id, &r).is_err(), "{mode}");
            let claims = s.claim_mcp_native_key_deletions().unwrap();
            assert_eq!(claims.len(), 1, "{mode}");
            assert_eq!(claims[0].reference, r);
            s.finish_mcp_native_key_deletion(&claims[0], Err("KEYCHAIN_OWNER_ACTION_REQUIRED"))
                .unwrap();
            assert_eq!(s.mcp_native_key_cleanup_pending().unwrap(), 1);
            assert!(s.mcp_native_key_owner_action_required().unwrap());
            assert!(s.claim_mcp_native_key_deletions().unwrap().is_empty());
            s.expedite_mcp_native_key_cleanup().unwrap();
            let claims = s.claim_mcp_native_key_deletions().unwrap();
            s.finish_mcp_native_key_deletion(&claims[0], Ok(()))
                .unwrap();
            assert_eq!(s.mcp_native_key_cleanup_pending().unwrap(), 0);
        }
    }
    #[test]
    fn late_native_creation_invalidates_an_earlier_delete_ack_and_cannot_reactivate() {
        let (_d, s, mut sub) = fixture();
        let r = s.reserve_mcp_native_key(&sub.grant_id, &sub.id).unwrap();
        s.retire_reserved_mcp_native_key(&r).unwrap();
        let early = s.claim_mcp_native_key_deletions().unwrap().remove(0);
        s.settle_mcp_native_key(&r, true).unwrap();
        s.finish_mcp_native_key_deletion(&early, Ok(())).unwrap();
        assert_eq!(s.mcp_native_key_cleanup_pending().unwrap(), 1);
        sub.secret = r.clone();
        assert!(s.save_event_subscription(&sub).is_err());
        let late = s.claim_mcp_native_key_deletions().unwrap().remove(0);
        s.finish_mcp_native_key_deletion(&late, Ok(())).unwrap();
        assert_eq!(s.mcp_native_key_cleanup_pending().unwrap(), 0);
        s.with_connection(|c|{assert_eq!(c.query_row("SELECT COUNT(*) FROM mcp_native_secret_refs WHERE reference=?1 AND state='RETIRED'",[r],|r|r.get::<_,i64>(0)).unwrap(),1,"minimum tombstone remains");Ok(())}).unwrap();
    }
    #[test]
    fn abandoned_reservation_is_retired_but_unconfirmed_native_creation_remains_reconcilable() {
        let (_d, s, sub) = fixture();
        let r = s
            .reserve_mcp_native_key(&sub.grant_id, "abandoned")
            .unwrap();
        s.with_connection(|c| {
            c.execute(
                "UPDATE mcp_native_secret_refs SET reserved_at=0 WHERE reference=?1",
                [&r],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
        let claim = s.claim_mcp_native_key_deletions().unwrap().remove(0);
        s.finish_mcp_native_key_deletion(&claim, Ok(())).unwrap();
        assert_eq!(
            s.mcp_native_key_cleanup_pending().unwrap(),
            1,
            "absence during unknown Add is not final erasure"
        );
        s.settle_mcp_native_key(&r, true).unwrap();
        let claim = s.claim_mcp_native_key_deletions().unwrap().remove(0);
        s.finish_mcp_native_key_deletion(&claim, Ok(())).unwrap();
        assert_eq!(s.mcp_native_key_cleanup_pending().unwrap(), 0);
    }
    #[test]
    fn pending_key_admission_obeys_the_existing_grant_limit_and_exact_target_fence() {
        let (_d, s, mut sub) = fixture();
        sub.secret = key(&s, &sub.grant_id, &sub.id);
        s.save_event_subscription(&sub).unwrap();
        for i in 0..31 {
            s.reserve_mcp_native_key(&sub.grant_id, &format!("pending-{i}"))
                .unwrap();
        }
        assert!(s
            .reserve_mcp_native_key(&sub.grant_id, "over-limit")
            .unwrap_err()
            .contains("LIMIT"));
        let rotation = s.reserve_mcp_native_key(&sub.grant_id, &sub.id).unwrap();
        assert!(s
            .reserve_mcp_native_key(&sub.grant_id, &sub.id)
            .unwrap_err()
            .contains("PENDING"));
        s.retire_reserved_mcp_native_key(&rotation).unwrap();
        assert!(s.require_mcp_native_key(&sub.id, &sub.secret).is_ok());
        assert!(s
            .reserve_mcp_native_key("unknown-grant", "unknown-target")
            .is_err());
    }
}
