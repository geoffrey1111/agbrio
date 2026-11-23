# 0.1.35-rc.158

- Fix conversation chronology when a long Codex turn contains several owner follow-ups and spans native item pages. Unpositioned old snapshot-cache messages no longer append after the latest native page. Earlier messages return through native pagination; streaming text still updates its exact native item.
- Add reviewed INSTANCE assistant text messages to existing Claude sessions through the authenticated native input plugin. Reuse existing management areas and grants. AUTO/SEND requires idle and a fresh input lease; QUEUE requires an explicit choice. Native approvals, exact target identity, independent delivery receipts, revocation and UNKNOWN fences remain enforced.
- Read Claude history through its own adapter in `agbrio_read_chat`. Action receipts expose the current send status and independent native message ID. APPLIED with QUEUED does not mean delivered.

Prepared locally using the deferred owner-run updater workflow. No live business messages or client configuration are used for acceptance. No physical iPhone acceptance or public publication is implied.
