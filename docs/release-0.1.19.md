# Agbrio0.1.19: resident progress and conversation push titles

The Host prepares exact-ID Bridge status on its existing resident observer,
independent of whether the desktop window is visible. Directory and role-state
presentation reads use those local snapshots without waiting for native SDK RPC.
Desktop read caches continue on their clock in the background; restoring the
window does not force every successful cached resource to refresh at once.
Status freshness is bounded; rebinding, reconnects and retired Bridges invalidate
old status. Cold requested complete replies are hydrated by the observer, while
send and explicit reread retain their live authority checks. Mobile suspension
still follows the OS; its foreground reads use the same resident Host snapshot.

PWA notifications now put the source conversation name in the primary title and
the existing privacy-safe progress state in the body. Application identity is
separate. Existing notification IDs, click targets and receipts are preserved.
No reply bodies, generated summaries or credentials are added to push payloads.
MCP Events is not part of this release and remains under separate implementation.

Validation:528 frontend tests,202 native tests (29 explicit real-path tests ignored), TypeScript/production build. Resident tests exercise no-renderer progress, a busy RPC lock, expiry/rebind/retirement, and one new-reply push with no historical/repeated push. Full-component desktop hidden/resume counters and real Service Worker Notification API format pass on isolated localhost fixtures. Installed resume, immutable package identity and exact Windows installer CI are recorded after packaging.
