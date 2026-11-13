# Agbrio local control

Run `control.ps1` with PowerShell7 on the same Windows user as Agbrio. Commands: Status, Capabilities, Diagnostics, Start, Stop, Restart. Installed default executable is its parent directory; source use needs `-Executable <existing ai-work-router.exe>`. Stop/Restart additionally require `-ExpectedInstance <instanceId from Status>`. JSON results, nonzero exit on uncertainty; never blindly repeat a write. No credentials or UI automation.

The executable must implement application control API1. Older installations remain unreachable until updated. MCP has separate scoped read/review tools; this local client does not grant remote maintenance or provider control. See the repository's Docs/APP_CONTROL.md for the protocol and acceptance boundaries.

Replace one management area's manager from PowerShell on the computer running Agbrio:

    ./control.ps1 -Command ReplaceZoneManager -ZoneId <uuid> -Provider CLAUDE_CODE -SessionId <native-uuid-or-local_id> -ExpectedRevision <revision> -RequestId <uuid>

Use ReadManagementZone -ZoneId <uuid> for the current revision and each member's active subscriptions. Keep the request ID and original arguments if you need the original receipt. A stale revision returns MANAGEMENT_CHANGED_REFRESH. The change uses the same transaction as the desktop action, and does not register or impersonate a native publisher. This command is only available on the current user's local control pipe.

Add one or more existing watched conversations to a management area, together with their notification routes:

    ./control.ps1 -Command AddZoneExecutors -ZoneId <uuid> -ExpectedRevision <revision> -RequestId <uuid> -ExecutorsJson '[{"threadId":"<native-id>","generation":1}]'

Use the exact watch IDs and generations from Agbrio. The entire list is checked before anything commits; conversations in another area require an explicit transfer. The original request returns its original receipt on retry. A subscription does not attest that the manager's notification plugin is online.
