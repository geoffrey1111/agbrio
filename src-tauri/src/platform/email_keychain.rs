//! Email credentials persist only an opaque reference in Core, never secret bytes.
use router_core::store::RouterStore;
use sha2::{Digest, Sha256};
pub(super) trait Vault {
    fn read(&self, reference: &str, owner_action: bool) -> Result<Vec<u8>, String>;
    fn write(&self, reference: &str, secret: &[u8]) -> Result<(), String>;
    fn delete(&self, reference: &str) -> Result<(), String>;
}
pub(super) fn identity(account: &str) -> String {
    format!("{:x}", Sha256::digest(account.as_bytes()))
}
fn reference(store: &RouterStore, account: &str, create: bool) -> Result<String, String> {
    store
        .email_secret_reference(&identity(account), create)?
        .ok_or("EMAIL_APP_PASSWORD_REQUIRED".into())
}
pub(super) fn read(
    vault: &impl Vault,
    store: &RouterStore,
    account: &str,
    owner_action: bool,
) -> Result<String, String> {
    let value = vault.read(&reference(store, account, false)?, owner_action)?;
    String::from_utf8(value).map_err(|_| "KEYCHAIN_INVALID_DATA".into())
}
pub(super) fn save(
    vault: &impl Vault,
    store: &RouterStore,
    account: &str,
    secret: &str,
) -> Result<(), String> {
    // Reserve the opaque reference first. A crash before native Add leaves a
    // harmless missing item; retry uses the same exact reference and no broad query.
    vault.write(&reference(store, account, true)?, secret.as_bytes())
}
pub(super) fn forget(vault: &impl Vault, store: &RouterStore, account: &str) -> Result<(), String> {
    let Some(reference) = store.email_secret_reference(&identity(account), false)? else {
        return Ok(());
    };
    // Denial keeps the reference for a later explicit owner retry; never lose
    // the only exact deletion target while claiming successful removal.
    vault.delete(&reference)?;
    store.forget_email_secret_reference(&identity(account), &reference)
}
pub(super) fn status(result: &Result<String, String>) -> &'static str {
    match result {
        Ok(_) => "READY",
        Err(e) if e == "EMAIL_APP_PASSWORD_REQUIRED" => "MISSING",
        Err(e) if e == "KEYCHAIN_OWNER_ACTION_REQUIRED" => "NEEDS_OWNER_ACTION",
        _ => "UNAVAILABLE",
    }
}
pub(super) fn valid_reference(value: &str) -> bool {
    value.strip_prefix("v1:").is_some_and(|id| {
        uuid::Uuid::parse_str(id).is_ok_and(|v| v.get_version_num() == 4 && v.to_string() == id)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::HashMap};
    #[derive(Default)]
    struct Fake {
        items: RefCell<HashMap<String, Vec<u8>>>,
        deny: RefCell<bool>,
        reads: RefCell<Vec<bool>>,
    }
    impl Vault for Fake {
        fn read(&self, r: &str, owner: bool) -> Result<Vec<u8>, String> {
            self.reads.borrow_mut().push(owner);
            if *self.deny.borrow() {
                return Err("KEYCHAIN_OWNER_ACTION_REQUIRED".into());
            }
            self.items
                .borrow()
                .get(r)
                .cloned()
                .ok_or("EMAIL_APP_PASSWORD_REQUIRED".into())
        }
        fn write(&self, r: &str, s: &[u8]) -> Result<(), String> {
            self.items.borrow_mut().insert(r.into(), s.to_vec());
            Ok(())
        }
        fn delete(&self, r: &str) -> Result<(), String> {
            if *self.deny.borrow() {
                return Err("KEYCHAIN_OWNER_ACTION_REQUIRED".into());
            }
            self.items.borrow_mut().remove(r);
            Ok(())
        }
    }
    #[test]
    fn locked_reads_never_allow_ui_and_failed_delete_keeps_exact_reference() {
        let dir = tempfile::tempdir().unwrap();
        let store = RouterStore::open_at(dir.path().join("fixture.db")).unwrap();
        let v = Fake::default();
        save(&v, &store, "one@example.invalid", "fake password").unwrap();
        save(&v, &store, "two@example.invalid", "other fake").unwrap();
        let reference = store
            .email_secret_reference(&identity("one@example.invalid"), false)
            .unwrap()
            .unwrap();
        assert!(valid_reference(&reference));
        assert!(!reference.contains("password"));
        *v.deny.borrow_mut() = true;
        assert_eq!(
            status(&read(&v, &store, "one@example.invalid", false)),
            "NEEDS_OWNER_ACTION"
        );
        assert!(forget(&v, &store, "one@example.invalid").is_err());
        assert_eq!(v.reads.borrow().as_slice(), [false]);
        assert_eq!(
            store
                .email_secret_reference(&identity("one@example.invalid"), false)
                .unwrap()
                .unwrap(),
            reference
        );
        *v.deny.borrow_mut() = false;
        assert!(read(&v, &store, "one@example.invalid", true).is_ok());
        assert_eq!(v.reads.borrow().as_slice(), [false, true]);
        forget(&v, &store, "one@example.invalid").unwrap();
        assert_eq!(
            status(&read(&v, &store, "one@example.invalid", false)),
            "MISSING"
        );
        assert_eq!(
            read(&v, &store, "two@example.invalid", false).unwrap(),
            "other fake"
        );
    }
    #[test]
    fn malformed_or_foreign_references_are_not_native_item_authority() {
        for v in [
            "",
            "password",
            "v1:../other",
            "v2:89a21534-a51e-49e8-8a9b-41b06262f344",
            "v1:89A21534-A51E-49E8-8A9B-41B06262F344",
        ] {
            assert!(!valid_reference(v));
        }
    }
}
