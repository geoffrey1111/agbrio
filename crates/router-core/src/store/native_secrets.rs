//! Non-secret, versioned references; native vault access belongs to the Host.
use super::*;
const PURPOSE: &str = "EMAIL_PASSWORD";
fn valid_identity(identity: &str) -> bool {
    identity.len() == 64
        && identity
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
impl RouterStore {
    pub fn email_secret_reference(
        &self,
        identity: &str,
        create: bool,
    ) -> Result<Option<String>, String> {
        if !valid_identity(identity) {
            return Err("SECRET_REFERENCE_INVALID".into());
        }
        self.with_connection(|c| {
            if create {
                c.execute("INSERT OR IGNORE INTO native_secret_references(purpose,identity_hash,item_ref) VALUES(?1,?2,?3)",params![PURPOSE,identity,format!("v1:{}",Uuid::new_v4())]).map_err(db_error)?;
            }
            c.query_row("SELECT item_ref FROM native_secret_references WHERE purpose=?1 AND identity_hash=?2",params![PURPOSE,identity],|r|r.get(0)).optional().map_err(db_error)
        })
    }
    pub fn forget_email_secret_reference(
        &self,
        identity: &str,
        expected: &str,
    ) -> Result<(), String> {
        if !valid_identity(identity) {
            return Err("SECRET_REFERENCE_INVALID".into());
        }
        self.with_connection(|c| {
            c.execute("DELETE FROM native_secret_references WHERE purpose=?1 AND identity_hash=?2 AND item_ref=?3",params![PURPOSE,identity,expected]).map_err(db_error)?;
            Ok(())
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_survives_reopen_and_exact_delete_preserves_other_accounts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.db");
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let store = RouterStore::open_at(&path).unwrap();
        assert!(store.email_secret_reference(&a, false).unwrap().is_none());
        let first = store.email_secret_reference(&a, true).unwrap().unwrap();
        assert!(Uuid::parse_str(first.strip_prefix("v1:").unwrap()).is_ok());
        let other = store.email_secret_reference(&b, true).unwrap().unwrap();
        assert_ne!(first, other);
        drop(store);
        let store = RouterStore::open_at(&path).unwrap();
        assert_eq!(
            store.email_secret_reference(&a, true).unwrap().unwrap(),
            first
        );
        store.forget_email_secret_reference(&a, &other).unwrap();
        assert!(store.email_secret_reference(&a, false).unwrap().is_some());
        store.forget_email_secret_reference(&a, &first).unwrap();
        assert!(store.email_secret_reference(&a, false).unwrap().is_none());
        assert_eq!(
            store.email_secret_reference(&b, false).unwrap().unwrap(),
            other
        );
        assert!(store
            .email_secret_reference("plaintext-account", true)
            .is_err());
    }
}
