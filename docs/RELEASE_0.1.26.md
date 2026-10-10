# Agbrio 0.1.26 — reuse conversations after deleting a Bridge

A recoverably deleted Bridge no longer reserves its former conversations forever.
Binding those exact conversations to a new Bridge creates distinct Endpoint rows;
old observations, handoffs, hashes and receipts keep their original IDs and owner.
A live or archived Bridge still reserves its conversations, and restoring a deleted
Bridge fails clearly if another non-deleted Bridge has claimed them. Delete the new
Bridge before restoring the old binding; no automatic
steal, provider task cancellation or replay occurs.

The SQLite upgrade uses a consistent native backup, a transactional constraint
rebuild and FK checks. A read-only production snapshot was upgraded offline:
all original columns/data in44 tables were preserved. Core220 and Host218+main1
pass. One local socket fixture initially hit Windows10055; the unchanged serial
Host rerun passes. Signed publication, Windows source/exact-installer checks and
installed verification are recorded in RELEASE_0.1.26.json after completion.

UI38 remains unchanged. [Events tutorial](MCP_EVENTS_WORKFLOW.md) /
[中文教程](MCP_EVENTS_WORKFLOW.zh-CN.md) documents the bounded owner-approved
recovery, missing automatic event context and explicit delegation limits.
[Goal recovery](GOAL_RECOVERY.md) remains a capability audit/proposal; no MCP
RESUME_GOAL, general failure event, budget increase or real-goal modification is
introduced in this release.
