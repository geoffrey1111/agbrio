# Agbrio local control

Use this CLI first for operations on the owner's computer. Use MCP capability discovery first for a delegated assistant; MCP management reads never grant local owner writes.

In the desktop sidebar, open **MCP & CLI** below **Settings**. Copy the setup prompt for an assistant or the PowerShell command for the CLI installed on this computer.

Run `control.ps1` with PowerShell7 on the same Windows user as Agbrio. Commands: Status, Capabilities, Diagnostics, Start, Stop, Restart. Installed default executable is its parent directory; source use needs `-Executable <existing ai-work-router.exe>`. Stop/Restart additionally require `-ExpectedInstance <instanceId from Status>`. JSON results, nonzero exit on uncertainty; never blindly repeat a write. No credentials or UI automation.

The executable must implement application control API1. Older installations remain unreachable until updated. MCP has separate scoped read/review tools; this local client does not grant remote maintenance or provider control. See the repository's Docs/APP_CONTROL.md for the protocol and acceptance boundaries.

### Install a prepared local update when ready

The maintainer prepares a signed installer and a version-pinned PowerShell 7 wrapper; run that command when a Host restart is suitable. The wrapper invokes `update-local.ps1` with the exact installer path, installer SHA256, target version, bundled native SHA256 and installed executable path. It verifies before exiting the exact current instance through this CLI, waits for transport shutdown, backs up the installation and local profile, installs into the same folder, verifies the installed binary and starts the new Host. A checksum mismatch, newer installed version or uncertain shutdown stops the operation. It never force-kills agents or replays tasks. The backup and receipt stay under `%LOCALAPPDATA%/AIWorkRouter/updates/backups/`. `-ValidateOnly` validates paths and the package without stopping or installing.

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

### Pi workspace connection repair

Owner policy (2026-11-22): all active INSTANCE connections may create/manage Pi
conversations within existing workspace ceilings. Source resolves current active
connections on policy reads and native writer admission; expiry/revocation and
action review still apply. This is not anonymous access or an extension of roots.

For a legacy installed Host, the owner-only compatibility command is
`python -B tools/agbrio-control/sync-pi-grants.py --database <router.db> --scope-id <uuid>`.
It previews active connections missing from that exact enabled workspace. Add
`--apply --receipt <new-retained-json-path>` to save the original policy and append
only missing active INSTANCE grant IDs in a transaction. No scope/task generation,
directory/terminal/admin capability, grant, task, message or Host process changes.
It uses the profile store because this Host has no Pi policy operation on its
current-user control pipe; never expose this local owner repair through remote MCP.
Verify with `agbrio_read_app({view:"FULL"}).piDelegation.workspacePolicies` afterward.
New-connection resolution in source requires a later owner-timed app update;
until then rerun this command after authorizing a new assistant connection.

### Connector discovery recovery

A connector may retain an older tools/list schema after Host updates. If MANAGEMENT is rejected by the client enum, use `agbrio_read_app({view:"FULL"}).management`; the current-user CLI `ReadManagementZone` remains a read-only alternative. Refresh client tool discovery without revoking or recreating the grant.

A listed tool returning `not available to the model` is not proof that Host received the request. Host Diagnostics reports local Host/adapter observations, not the client tool registry. Rediscover client tools and retry a read once; persistent failures need client-side call/time/error evidence. For writes, inspect the original action and native delivery receipts. Never resend, mint another identity or infer NOT_SENT from a client availability error.

Claude explicit QUEUE accepts up to32 independent unattempted queue entries. Physical offers are FIFO with only one outstanding delivery; SENDING/UNKNOWN blocks every later offer. This does not reopen terminal FAILED actions or authorize replay.
