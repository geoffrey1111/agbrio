//! In-process Security.framework leaf. No subprocess, profile copy or ACL migration.
use super::email_keychain::{self, Vault};
use core_foundation::{
    array::CFArray,
    base::{CFType, TCFType},
    boolean::CFBoolean,
    data::CFData,
    dictionary::CFDictionary,
    string::{CFString, CFStringRef},
};
use security_framework::os::macos::{access::SecAccess, keychain::SecKeychain};
use security_framework_sys::{
    base::SecAccessRef,
    item::*,
    keychain_item::{SecItemAdd, SecItemCopyMatching, SecItemDelete, SecItemUpdate},
};
use std::{
    ffi::CStr,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::PathBuf,
    ptr,
};
extern "C" {
    fn SecKeychainGetStatus(
        keychain: security_framework_sys::base::SecKeychainRef,
        status: *mut u32,
    ) -> i32;
    fn SecAccessCreate(
        descriptor: CFStringRef,
        trusted: *const std::ffi::c_void,
        out: *mut SecAccessRef,
    ) -> i32;
    static kSecAttrAccess: CFStringRef;
    static kSecUseAuthenticationUIFail: CFStringRef;
    static kSecUseAuthenticationUIAllow: CFStringRef;
}
const SERVICE: &str = "com.geoffrey.aiworkrouter.email.v1";
fn error(code: i32) -> String {
    match code {
        -25300 => "EMAIL_APP_PASSWORD_REQUIRED",
        -25293 | -25308 | -25315 | -128 => "KEYCHAIN_OWNER_ACTION_REQUIRED",
        -25299 => "KEYCHAIN_ITEM_COLLISION",
        _ => "KEYCHAIN_UNAVAILABLE",
    }
    .into()
}
fn check(code: i32) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(error(code))
    }
}
fn string_constant(v: CFStringRef) -> CFType {
    unsafe { CFString::wrap_under_get_rule(v).into_CFType() }
}
fn pair(k: CFStringRef, v: CFType) -> (CFString, CFType) {
    (unsafe { CFString::wrap_under_get_rule(k) }, v)
}
struct NativeVault {
    keychain: SecKeychain,
}
impl NativeVault {
    fn login() -> Result<Self, String> {
        let uid = unsafe { libc::getuid() };
        if uid == 0 || uid != unsafe { libc::geteuid() } {
            return Err("KEYCHAIN_OWNER_ACTION_REQUIRED".into());
        }
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut out = ptr::null_mut();
        let mut buffer = vec![0u8; 65536];
        if unsafe {
            libc::getpwuid_r(
                uid,
                &mut entry,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut out,
            )
        } != 0
            || out.is_null()
            || entry.pw_dir.is_null()
        {
            return Err("KEYCHAIN_UNAVAILABLE".into());
        }
        let home = unsafe { CStr::from_ptr(entry.pw_dir) }.to_bytes();
        let path = PathBuf::from(std::ffi::OsStr::from_bytes(home))
            .join("Library/Keychains/login.keychain-db");
        let meta = std::fs::symlink_metadata(&path).map_err(|_| "KEYCHAIN_UNAVAILABLE")?;
        if !meta.is_file() || meta.uid() != uid {
            return Err("KEYCHAIN_UNAVAILABLE".into());
        }
        Ok(Self {
            keychain: SecKeychain::open(path).map_err(|e| error(e.code()))?,
        })
    }
    fn query(
        &self,
        reference: &str,
        owner_action: bool,
    ) -> Result<Vec<(CFString, CFType)>, String> {
        if !email_keychain::valid_reference(reference) {
            return Err("KEYCHAIN_INVALID_REFERENCE".into());
        }
        if !owner_action {
            let mut status = 0;
            check(unsafe {
                SecKeychainGetStatus(self.keychain.as_concrete_TypeRef(), &mut status)
            })?;
            if status & 1 == 0 {
                return Err("KEYCHAIN_OWNER_ACTION_REQUIRED".into());
            }
        }
        Ok(unsafe {
            vec![
                pair(kSecClass, string_constant(kSecClassGenericPassword)),
                pair(kSecAttrService, CFString::new(SERVICE).into_CFType()),
                pair(kSecAttrAccount, CFString::new(reference).into_CFType()),
                pair(
                    kSecMatchSearchList,
                    CFArray::from_CFTypes(&[self.keychain.clone()]).into_CFType(),
                ),
                pair(
                    kSecUseAuthenticationUI,
                    string_constant(if owner_action {
                        kSecUseAuthenticationUIAllow
                    } else {
                        kSecUseAuthenticationUIFail
                    }),
                ),
            ]
        })
    }
}
impl Vault for NativeVault {
    fn read(&self, reference: &str, owner_action: bool) -> Result<Vec<u8>, String> {
        let mut pairs = self.query(reference, owner_action)?;
        pairs.push(unsafe { pair(kSecReturnData, CFBoolean::true_value().into_CFType()) });
        let q = CFDictionary::from_CFType_pairs(&pairs);
        let mut raw = ptr::null();
        check(unsafe { SecItemCopyMatching(q.as_concrete_TypeRef(), &mut raw) })?;
        if raw.is_null() {
            return Err("KEYCHAIN_INVALID_DATA".into());
        }
        let value = unsafe { CFType::wrap_under_create_rule(raw) };
        value
            .downcast::<CFData>()
            .map(|v| v.bytes().to_vec())
            .ok_or("KEYCHAIN_INVALID_DATA".into())
    }
    fn write(&self, reference: &str, secret: &[u8]) -> Result<(), String> {
        if secret.is_empty() || secret.len() > 16384 {
            return Err("KEYCHAIN_INVALID_DATA".into());
        }
        // Explicit desktop Save may request existing-item authorization. Update
        // changes only the value; it never adds trusted programs or migrates ACLs.
        match self.read(reference, true) {
            Ok(_) => {
                let q = CFDictionary::from_CFType_pairs(&self.query(reference, true)?);
                let update = CFDictionary::from_CFType_pairs(&[unsafe {
                    pair(kSecValueData, CFData::from_buffer(secret).into_CFType())
                }]);
                check(unsafe {
                    SecItemUpdate(q.as_concrete_TypeRef(), update.as_concrete_TypeRef())
                })
            }
            Err(e) if e == "EMAIL_APP_PASSWORD_REQUIRED" => {
                let label = CFString::new("Agbrio Gmail app password");
                let mut access = ptr::null_mut();
                check(unsafe {
                    SecAccessCreate(label.as_concrete_TypeRef(), ptr::null(), &mut access)
                })?;
                if access.is_null() {
                    return Err("KEYCHAIN_UNAVAILABLE".into());
                }
                let access = unsafe { SecAccess::wrap_under_create_rule(access) };
                let pairs = unsafe {
                    vec![
                        pair(kSecClass, string_constant(kSecClassGenericPassword)),
                        pair(kSecAttrService, CFString::new(SERVICE).into_CFType()),
                        pair(kSecAttrAccount, CFString::new(reference).into_CFType()),
                        pair(kSecUseKeychain, self.keychain.as_CFType()),
                        pair(kSecAttrLabel, label.into_CFType()),
                        pair(kSecAttrAccess, access.into_CFType()),
                        pair(kSecValueData, CFData::from_buffer(secret).into_CFType()),
                    ]
                };
                // A concurrent duplicate is an error; never overwrite an
                // unexpected item via a duplicate-Add fallback or broaden its ACL.
                check(unsafe {
                    SecItemAdd(
                        CFDictionary::from_CFType_pairs(&pairs).as_concrete_TypeRef(),
                        ptr::null_mut(),
                    )
                })
            }
            Err(e) => Err(e),
        }
    }
    fn delete(&self, reference: &str) -> Result<(), String> {
        let q = CFDictionary::from_CFType_pairs(&self.query(reference, false)?);
        let status = unsafe { SecItemDelete(q.as_concrete_TypeRef()) };
        if status == -25300 {
            Ok(())
        } else {
            check(status)
        }
    }
}
pub(crate) fn read(
    store: &router_core::store::RouterStore,
    account: &str,
    owner_action: bool,
) -> Result<String, String> {
    email_keychain::read(&NativeVault::login()?, store, account, owner_action)
}
pub(crate) fn save(
    store: &router_core::store::RouterStore,
    account: &str,
    secret: &str,
) -> Result<(), String> {
    email_keychain::save(&NativeVault::login()?, store, account, secret)
}
pub(crate) fn forget(store: &router_core::store::RouterStore, account: &str) -> Result<(), String> {
    email_keychain::forget(&NativeVault::login()?, store, account)
}
pub(crate) fn status(result: &Result<String, String>) -> &'static str {
    email_keychain::status(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    extern "C" {
        fn SecKeychainDelete(k: security_framework_sys::base::SecKeychainRef) -> i32;
        fn SecKeychainLock(k: security_framework_sys::base::SecKeychainRef) -> i32;
    }
    struct FixtureCleanup(Option<SecKeychain>);
    impl Drop for FixtureCleanup {
        fn drop(&mut self) {
            if let Some(k) = self.0.take() {
                let _ = unsafe { SecKeychainDelete(k.as_concrete_TypeRef()) };
            }
        }
    }
    #[test]
    fn native_error_codes_preserve_owner_action_without_secret_details() {
        for code in [-25293, -25308, -25315, -128] {
            assert_eq!(error(code), "KEYCHAIN_OWNER_ACTION_REQUIRED");
        }
        assert_eq!(error(-25300), "EMAIL_APP_PASSWORD_REQUIRED");
        assert_eq!(error(-99999), "KEYCHAIN_UNAVAILABLE");
    }
    #[test]
    #[ignore = "GitHub ephemeral Mac runner only; temporary keychain; no login/default keychain access"]
    fn isolated_native_roundtrip_and_unrelated_item_survives() {
        use security_framework::os::macos::keychain::CreateOptions;
        assert_eq!(
            std::env::var("GITHUB_ACTIONS").as_deref(),
            Ok("true"),
            "Native fixture is restricted to ephemeral GitHub runners"
        );
        let base = PathBuf::from(std::env::var("RUNNER_TEMP").unwrap());
        assert!(base.is_absolute());
        let dir = tempfile::tempdir_in(base).unwrap();
        let mut keychain = CreateOptions::new()
            .password("fixture-only-not-a-user-secret")
            .create(dir.path().join("fixture.keychain"))
            .unwrap();
        let mut cleanup = FixtureCleanup(Some(keychain.clone()));
        keychain
            .unlock(Some("fixture-only-not-a-user-secret"))
            .unwrap();
        let vault = NativeVault { keychain };
        let db = tempfile::tempdir().unwrap();
        let store = router_core::store::RouterStore::open_at(db.path().join("fixture.db")).unwrap();
        email_keychain::save(&vault, &store, "one@example.invalid", "abcdefghijklmnop").unwrap();
        email_keychain::save(&vault, &store, "two@example.invalid", "ponmlkjihgfedcba").unwrap();
        assert_eq!(
            email_keychain::read(&vault, &store, "one@example.invalid", false).unwrap(),
            "abcdefghijklmnop"
        );
        email_keychain::save(&vault, &store, "one@example.invalid", "abcdefghijklmnzz").unwrap();
        assert_eq!(
            email_keychain::read(&vault, &store, "one@example.invalid", false).unwrap(),
            "abcdefghijklmnzz"
        );
        check(unsafe { SecKeychainLock(vault.keychain.as_concrete_TypeRef()) }).unwrap();
        assert_eq!(
            email_keychain::read(&vault, &store, "one@example.invalid", false).unwrap_err(),
            "KEYCHAIN_OWNER_ACTION_REQUIRED"
        );
        let mut owned = vault.keychain.clone();
        owned
            .unlock(Some("fixture-only-not-a-user-secret"))
            .unwrap();
        email_keychain::forget(&vault, &store, "one@example.invalid").unwrap();
        assert!(email_keychain::read(&vault, &store, "one@example.invalid", false).is_err());
        assert_eq!(
            email_keychain::read(&vault, &store, "two@example.invalid", false).unwrap(),
            "ponmlkjihgfedcba"
        );
        check(unsafe { SecKeychainDelete(vault.keychain.as_concrete_TypeRef()) }).unwrap();
        cleanup.0 = None;
    }
}
