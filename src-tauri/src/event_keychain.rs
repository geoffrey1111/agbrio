//! Mac Events vault coordination. Owner IPC returns metadata only, never keys.
use crate::host_application::{RouterCore, RouterState};
use router_core::store::RouterStore;
use serde::Serialize;
use std::sync::Arc;

pub(crate) struct PendingKey {
    store: Arc<RouterStore>,
    reference: Vec<u8>,
    armed: bool,
}
impl PendingKey {
    pub(crate) fn reference(&self) -> Vec<u8> {
        self.reference.clone()
    }
    pub(crate) fn disarm(mut self) {
        self.armed = false;
    }
}
impl Drop for PendingKey {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.store.retire_reserved_mcp_native_key(&self.reference);
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) async fn prepare(
    core: &RouterCore,
    gid: &str,
    sid: &str,
    secret: &str,
) -> Result<PendingKey, String> {
    prepare_with(
        core.store.clone(),
        gid,
        sid,
        secret,
        crate::platform::macos_keychain::create_event,
    )
    .await
}
async fn prepare_with(
    store: Arc<RouterStore>,
    gid: &str,
    sid: &str,
    secret: &str,
    create: impl FnOnce(&[u8], &[u8]) -> Result<(), String> + Send + 'static,
) -> Result<PendingKey, String> {
    let reference = store.reserve_mcp_native_key(gid, sid)?;
    let pending = PendingKey {
        store: store.clone(),
        reference: reference.clone(),
        armed: true,
    };
    let secret = secret.as_bytes().to_vec();
    let gid = gid.to_string();
    let sid = sid.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let result = store
            .require_mcp_native_key_reservation(&gid, &sid, &reference)
            .and_then(|_| create(&reference, &secret));
        // This settlement stays inside the native operation, even when its HTTP
        // waiter disappears. Late completion invalidates an older deletion ACK.
        let settled = store.settle_mcp_native_key(&reference, result.is_ok());
        result?;
        settled
    })
    .await
    .map_err(|_| "MCP_EVENT_SECRET_REFERENCE_UNCONFIRMED")??;
    Ok(pending)
}

#[cfg(target_os = "macos")]
pub(crate) async fn read(core: &RouterCore, sid: &str, reference: &[u8]) -> Result<String, String> {
    let store = core.store.clone();
    let sid = sid.to_string();
    let reference = reference.to_vec();
    tauri::async_runtime::spawn_blocking(move || {
        store.require_mcp_native_key(&sid, &reference)?;
        let value = crate::platform::macos_keychain::read_event(&reference, false);
        let _ = store.note_mcp_native_key_access(
            &reference,
            value.as_ref().map(|_| ()).map_err(|e| e.as_str()),
        );
        let value = value?;
        store.require_mcp_native_key(&sid, &reference)?; // Retirement while native read was pending.
        String::from_utf8(value).map_err(|_| "MCP_EVENT_SECRET_STORE".into())
    })
    .await
    .map_err(|_| "MCP_EVENT_SECRET_STORE")?
}
#[cfg(target_os = "macos")]
pub(crate) async fn cleanup(core: &RouterCore) {
    let store = core.store.clone();
    let Ok(claims) = store.claim_mcp_native_key_deletions() else {
        return;
    };
    if claims.is_empty() {
        return;
    }
    let _ = tauri::async_runtime::spawn_blocking(move || {
        cleanup_claims(
            &store,
            claims,
            crate::platform::macos_keychain::delete_event,
        )
    })
    .await;
}
fn cleanup_claims(
    store: &RouterStore,
    claims: Vec<router_core::store::mcp_native_secrets::NativeKeyDeletion>,
    mut delete: impl FnMut(&[u8]) -> Result<(), String>,
) {
    for claim in claims {
        // A reserved UUID is not proof that Add created the native item. Never
        // delete an existing collision or an unconfirmed creation as our own.
        let result = if !claim.creation_settled {
            Err("MCP_EVENT_SECRET_REFERENCE_UNCONFIRMED".into())
        } else if !claim.created {
            Ok(())
        } else {
            delete(&claim.reference)
        };
        let _ = store.finish_mcp_native_key_deletion(
            &claim,
            result.as_ref().map(|_| ()).map_err(|e| e.as_str()),
        );
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    supported: bool,
    stored_references: i64,
    pending_cleanup: i64,
    needs_owner_action: bool,
}
fn status(store: &RouterStore) -> Result<Status, String> {
    if !cfg!(target_os = "macos") {
        return Ok(Status {
            supported: false,
            stored_references: 0,
            pending_cleanup: 0,
            needs_owner_action: false,
        });
    }
    Ok(Status {
        supported: true,
        stored_references: store.mcp_native_key_owner_count()?,
        pending_cleanup: store.mcp_native_key_cleanup_pending()?,
        needs_owner_action: store.mcp_native_key_owner_action_required()?,
    })
}
#[tauri::command]
pub(crate) async fn assistant_event_keychain_status(
    state: tauri::State<'_, RouterState>,
) -> Result<Status, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || status(&store))
        .await
        .map_err(|_| "KEYCHAIN_UNAVAILABLE")?
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Authorized {
    checked: usize,
    next_cursor: Option<String>,
    notification_sent: bool,
}
#[tauri::command]
pub(crate) async fn assistant_event_keychain_authorize(
    after: Option<String>,
    state: tauri::State<'_, RouterState>,
) -> Result<Authorized, String> {
    #[cfg(target_os = "macos")]
    {
        let store = state.store.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut rows = store.mcp_native_keys_for_owner(after.as_deref().map(str::as_bytes))?;
            crate::platform::macos_keychain::authorize_event_store()?;
            let has_more = rows.len() > 64;
            rows.truncate(64);
            for reference in &rows {
                let result = crate::platform::macos_keychain::read_event(reference, true);
                // A removed item needs no access. Its retirement record remains
                // until the worker confirms deletion and native creation settled.
                let result = match result {
                    Ok(_) => Ok(()),
                    Err(e) if e == "MCP_EVENT_SECRET_REQUIRED" => Ok(()),
                    Err(e) => Err(e),
                };
                store.note_mcp_native_key_access(
                    reference,
                    result.as_ref().map(|_| ()).map_err(|e| e.as_str()),
                )?;
                result?;
            }
            store.expedite_mcp_native_key_cleanup()?;
            let next_cursor = if has_more {
                rows.last().and_then(|r| String::from_utf8(r.clone()).ok())
            } else {
                None
            };
            Ok(Authorized {
                checked: rows.len(),
                next_cursor,
                notification_sent: false,
            })
        })
        .await
        .map_err(|_| "KEYCHAIN_UNAVAILABLE")?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (after, state);
        Err("PLATFORM_NOT_YET_SUPPORTED".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_only_exposes_metadata_without_selecting_any_secret() {
        let d = tempfile::tempdir().unwrap();
        let store = RouterStore::open_at(d.path().join("fixture.db")).unwrap();
        let value = serde_json::to_value(status(&store).unwrap()).unwrap();
        assert_eq!(value["supported"], cfg!(target_os = "macos"));
        assert!(value.get("secret").is_none());
        assert!(value.get("references").is_none());
        assert_eq!(value["pendingCleanup"], 0);
    }
    #[tokio::test]
    async fn cancelled_http_waiter_does_not_lose_or_replay_a_late_native_creation() {
        use router_core::store::{
            assistant::{BriefRule, GrantInput},
            role_bridge::RoleBindingInput,
        };
        use std::{
            sync::{
                atomic::{AtomicUsize, Ordering},
                mpsc,
            },
            time::Duration,
        };
        let d = tempfile::tempdir().unwrap();
        let store = Arc::new(RouterStore::open_at(d.path().join("fixture.db")).unwrap());
        let p = store.create_project("vault fixture".into(), None).unwrap();
        let w = store
            .create_workstream(&p.id, "vault boundary".into())
            .unwrap();
        let side = |id: &str| RoleBindingInput {
            provider: "CODEX".into(),
            external_id: id.into(),
            label: id.into(),
            cwd: Some(d.path().to_string_lossy().into()),
        };
        let b = store
            .bind_role_bridge(
                &w.id,
                w.binding_revision,
                side("vault-control"),
                side("vault-execution"),
            )
            .unwrap();
        let grant = store
            .create_assistant_grant(GrantInput {
                workstream_id: w.id,
                source_role: "DECISION".into(),
                binding_revision: b.binding_revision,
                label: "vault".into(),
                rules: vec![BriefRule {
                    id: "continue".into(),
                    text: "fixture".into(),
                }],
                expires_at: time::OffsetDateTime::now_utc().unix_timestamp() * 1000 + 600000,
            })
            .unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let worker_store = store.clone();
        let waiter = tokio::spawn(async move {
            prepare_with(
                worker_store,
                &grant.id,
                "fixture-native",
                "fake-only",
                move |reference, secret| {
                    assert_eq!(secret, b"fake-only");
                    counter.fetch_add(1, Ordering::SeqCst);
                    entered_tx.send(reference.to_vec()).unwrap();
                    release_rx
                        .recv_timeout(Duration::from_secs(20))
                        .map_err(|_| "fixture deadline".to_string())?;
                    Ok(())
                },
            )
            .await
        });
        let reference = tokio::task::spawn_blocking(move || {
            entered_rx.recv_timeout(Duration::from_secs(20)).unwrap()
        })
        .await
        .unwrap();
        waiter.abort();
        let _ = waiter.await;
        let early = store.claim_mcp_native_key_deletions().unwrap().remove(0);
        store
            .finish_mcp_native_key_deletion(&early, Ok(()))
            .unwrap();
        assert_eq!(store.mcp_native_key_cleanup_pending().unwrap(), 1);
        release_tx.send(()).unwrap();
        let late = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Some(claim) = store
                    .claim_mcp_native_key_deletions()
                    .unwrap()
                    .into_iter()
                    .next()
                {
                    break claim;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(late.reference, reference);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        store.finish_mcp_native_key_deletion(&late, Ok(())).unwrap();
        assert_eq!(store.mcp_native_key_cleanup_pending().unwrap(), 0);
        assert!(store
            .require_mcp_native_key("fixture-native", &reference)
            .is_err());
    }
    #[tokio::test]
    async fn cleanup_never_deletes_an_unconfirmed_or_rejected_native_creation() {
        use router_core::store::{
            assistant::{BriefRule, GrantInput},
            role_bridge::RoleBindingInput,
        };
        let d = tempfile::tempdir().unwrap();
        let store = Arc::new(RouterStore::open_at(d.path().join("fixture.db")).unwrap());
        let p = store
            .create_project("ownership fixture".into(), None)
            .unwrap();
        let w = store.create_workstream(&p.id, "ownership".into()).unwrap();
        let side = |id: &str| RoleBindingInput {
            provider: "CODEX".into(),
            external_id: id.into(),
            label: id.into(),
            cwd: Some(d.path().to_string_lossy().into()),
        };
        let b = store
            .bind_role_bridge(
                &w.id,
                w.binding_revision,
                side("ownership-control"),
                side("ownership-execution"),
            )
            .unwrap();
        let grant = store
            .create_assistant_grant(GrantInput {
                workstream_id: w.id,
                source_role: "DECISION".into(),
                binding_revision: b.binding_revision,
                label: "ownership".into(),
                rules: vec![BriefRule {
                    id: "continue".into(),
                    text: "fixture".into(),
                }],
                expires_at: time::OffsetDateTime::now_utc().unix_timestamp() * 1000 + 600000,
            })
            .unwrap();
        assert!(
            prepare_with(store.clone(), &grant.id, "collision", "fake", |_, _| Err(
                "KEYCHAIN_ITEM_COLLISION".into()
            ))
            .await
            .is_err()
        );
        let owned = prepare_with(store.clone(), &grant.id, "owned", "fake", |_, _| Ok(()))
            .await
            .unwrap();
        let reference = owned.reference();
        drop(owned);
        let unknown = store.reserve_mcp_native_key(&grant.id, "unknown").unwrap();
        store.retire_reserved_mcp_native_key(&unknown).unwrap();
        let claims = store.claim_mcp_native_key_deletions().unwrap();
        let mut removed = Vec::new();
        cleanup_claims(&store, claims, |r| {
            removed.push(r.to_vec());
            Ok(())
        });
        assert_eq!(
            removed,
            vec![reference],
            "an intent or rejected Add is not native ownership"
        );
        assert_eq!(store.mcp_native_key_cleanup_pending().unwrap(), 1);
        assert!(store.mcp_native_keys_for_owner(None).unwrap().is_empty());
    }
}
