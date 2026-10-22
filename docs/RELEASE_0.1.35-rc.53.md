# Agbrio 0.1.35-rc.53

Source and supported development branch: **main**. The release tag identifies the exact main source used for this package; the source archive contains that commit.

## Included changes

- Configurable Boards tab: local isolated board projection or an owner-chosen HTTPS page, responsive phone viewing, verified image variants, scoped presentation caching and supported explicit manual review. Configure your own source in Settings. No private board documents, artwork or local settings are bundled.
- Full Claude conversation-tree activity and child readers; child-result push is optional and off by default. Child contexts preserve exact native membership and owner authority.
- Supported native Claude decisions/permissions and Codex questions through existing owner interactions. Unsupported/stale native requests fail closed; no autonomous Claude write capability is added.
- Message/client identity and chronological native paging, attachment bubble reconciliation, running-turn updates and warm-cache order repair. Old sends cannot overwrite later answers. Resolved questions no longer append permanent confirmation receipts.
- Quieter sending feedback, exact late receipt reconciliation, advanced recovery actions inside the affected message, entry at the latest messages, phone keyboard/zoom lifecycle and pinch zoom retained.
- Instance-scoped MCP capability/diagnostic discovery, authenticated owner application operations and protected Windows-local control CLI. Existing scope, consent and at-most-once send fences remain.
- All current main shared Codex startup/recovery and bounded mobile/tunnel recovery fixes.

## Updating

In the Windows app, open Settings → About → Check for updates, download and install the signed update. Installation preserves user data and existing agent sessions, follows the existing active-task confirmation and database-backup gates, and never resends an uncertain message. Main is the development branch; release tags retain exact source identity.

## Validation and limits

Frontend, Core, native, updater-signature/tamper and clean Windows installation checks are recorded in the release description and linked Actions runs. Real-path checks are bounded; physical-phone adoption, every external board, every native-agent version and all future agent updates are not claimed. Windows is the accepted packaged target. Private conversations, credentials, profiles and runtime data are excluded.
