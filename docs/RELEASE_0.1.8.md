# Agbrio0.1.8 — INSTANCE Bridge directory across all projects

Fix agbrio_read_app using the selected-project snapshot. INSTANCE now uses an
independent all-projects index. bridges contains only ACTIVE entries;
archivedBridges and trashedBridges are separate history, each row explicitly
marks lifecycle and retains exact ID, projectId and bindingRevision.

Desktop selected-workspace snapshot semantics remain unchanged. Choosing a
project does not alter the full MCP directory. No schema migration, grant
upgrade, pairing reset, provider write or message resend. Legacy BRIDGE grants
still have eight tools and cannot enumerate the app. Authorization and source
version/destination guards remain in force.

Preserve0.1.7 manual assistant tutorial, localized marketing and owner-restored
Bridge interactions. agbrio_read_chat -32603 and an old APPROVED receipt reported
as INVALID_ARGUMENT are independent unresolved issues, not claimed fixed here.
After updating, use the existing dot connection to call read_app, read_bridge,
read_source read-only. Do not send messages for acceptance.
