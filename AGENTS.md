# Agbrio source development

Read README.md and docs/SETUP.md before setup. Windows + Codex + PWA is the current alpha scope; Mac and other agent adapters are not accepted runtime targets yet.

- Preserve user's live desktop/agent processes, data and paired devices. Never force-quit agents or install/start a second Codex Desktop.
- Route by exact native thread/conversation IDs. Titles and screenshots are presentation, not routing authority.
- Keep provider details inside adapters. Core remains independent of desktop/DOM/HTTP transport payloads.
- A handoff sends only after the user confirms visible content and destination. Never create an autonomous agent relay loop or approve an agent's own outcome.
- Failed/ambiguous sends are UNKNOWN; do not blindly retry or mint another request identity.
- Mobile listener is loopback-only. HTTPS entrances preserve exact Host/Origin and paired-device authentication. Optional Cloudflare Access does not replace device authorization for generic providers.
- Never commit credentials, private keys, personal conversations, runtime/profile data or attachments. Test artifacts remain ignored.
- Validate affected tests and real paths where feasible. Unit/browser/screenshot checks are not physical-device acceptance. Report unavailable platforms and account/security steps honestly.
