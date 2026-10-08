# Agbrio0.1.1 — Windows preview

Bridge connects the existing conversation that manages a project with the one that executes it. Review selected instructions/results and material versions, confirm the exact receiving conversation, then hand off. Phone access supports that workflow.

- One current active side in the Bridge list. Executing turns animate; red dots identify stopped work requiring attention.
- Retain earlier full results when later wait/blocked replies arrive. Explicit source selection keeps each result's draft and materials separate.
- Native Codex question choices, free answers, collapse/reopen, skip/send, and current-turn follow-up/steer/stop.
- Chronological original-chat timelines, notification seen/read synchronization and reversible removal.
- Bound Bridge replies remain in the Bridge rather than creating independent watches.
- Desktop Settings → Check for updates: explicit check, download, publisher-signature verification, backup and install/restart.
- Optional whole-instance assistant MCP connections now use conversation review without a mandatory saved Brief. Actual Dot owner OAuth/installation remains unverified; no grant is created automatically.
- Existing hosted redemption, free self-hosting, English/简体中文/繁體中文 UI, and MIT license remain.

## Package and source identity

Tag/application version0.1.1, interface2026.10.08-21. The source base remains0.1.0; the signed build overlay and VITE_AGBRIO_APP_VERSION produce0.1.1 package metadata. Run scripts/build-signed-update.ps1 with Version0.1.1 to build signed artifacts using your own publisher key. Signing secrets are not included. Default local developer builds retain base metadata until given an explicit version overlay.

The package has a Tauri/minisign updater signature. This is separate from Windows Authenticode signing; Windows may still show an unsigned-publisher prompt. Verify the SHA256SUMS and source. No recording demo, private conversation/profile/database, code batch or operator credential is distributed.

## Validation boundaries

Actual0.1.1 local package build, native/frontend/manifest identity and publisher-signature/tamper checks passed. Isolated native/HTTP/browser evidence for questions and handoffs has its recorded scope. Fresh Windows CI, actual public-file updater installation and physical-device acceptance are separate checks; see the release's updates for their result. Physical iPhone keyboard/gesture, Mac, fresh recipient hosted redemption and real Dot owner connection are not accepted by this package's unit checks. This remains a Windows preview.
