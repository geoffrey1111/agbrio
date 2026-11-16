# Agbrio local control

Use this CLI first for operations on the owner's computer. Use MCP capability discovery first for a delegated assistant; MCP management reads never grant local owner writes.

In the desktop sidebar, open **MCP & CLI** below **Settings**. Copy the setup prompt for an assistant or the PowerShell command for the CLI installed on this computer.

Run `control.ps1` with PowerShell7 on the same Windows user as Agbrio. Commands: Status, Capabilities, Diagnostics, Start, Stop, Restart. Installed default executable is its parent directory; source use needs `-Executable <existing ai-work-router.exe>`. Stop/Restart additionally require `-ExpectedInstance <instanceId from Status>`. JSON results, nonzero exit on uncertainty; never blindly repeat a write. No credentials or UI automation.

The executable must implement application control API1. Older installations remain unreachable until updated. MCP has separate scoped read/review tools; this local client does not grant remote maintenance or provider control. See the repository's Docs/APP_CONTROL.md for the protocol and acceptance boundaries.

Replace one management area's manager from PowerShell on the computer running Agbrio:

    ./control.ps1 -Command ReplaceZoneManager -ZoneId <uuid> -Provider CLAUDE_CODE -SessionId <native-uuid-or-local_id> -ExpectedRevision <revision> -RequestId <uuid>

Use ReadManagementZone -ZoneId <uuid> for the current revision and each member's active subscriptions. Keep the request ID and original arguments if you need the original receipt. A stale revision returns MANAGEMENT_CHANGED_REFRESH. The change uses the same transaction as the desktop action, and does not register or impersonate a native publisher. This command is only available on the current user's local control pipe.

Add one or more existing watched conversations to a management area, together with their notification routes:

    ./control.ps1 -Command AddZoneExecutors -ZoneId <uuid> -ExpectedRevision <revision> -RequestId <uuid> -ExecutorsJson '[{"threadId":"<native-id>","generation":1}]'

Use the exact watch IDs and generations from Agbrio. The entire list is checked before anything commits; conversations in another area require an explicit transfer. The original request returns its original receipt on retry. A subscription does not attest that the manager's notification plugin is online.

### Management areas

- Read all areas, exact member IDs, revisions, state and last observed activity: `-Command ListManagementZones`.
- Read one area: `-Command ReadManagementZone -ZoneId <uuid>`.
- Use `-Command ChangeManagementZone -RequestId <uuid> -ChangeJson '<json>'` for the same `CREATE`, `ADOPT`, `APPOINT`, `ADD`, `MOVE`, `REMOVE`, `RENAME` and `DELETE` actions as the owner UI. Supply the exact area revision and member generation where required. The script binds the request to the installed instance and uses the same durable action receipt. Keep the original request ID and content when checking an uncertain result.
- Example JSON for removing a single assignment: `{"action":"REMOVE","zoneId":"<uuid>","revision":12,"threadId":"<native-id>"}`. This does not delete or stop the native conversation.
- An instance MCP grant can inspect assignments via `agbrio_read_app` with `view: "MANAGEMENT"`. Manage assignments locally using the CLI.
