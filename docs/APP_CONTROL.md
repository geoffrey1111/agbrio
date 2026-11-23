# Agent application operations

Start with MCP capability discovery for delegated work and the installed current-user CLI for owner maintenance. Desktop and PWA are human interfaces using the same application actions; missing script coverage is a product gap. See [the CLI commands](../tools/agbrio-control/README.md).

Settings → AI assistant exposes the current instance MCP address, connection instructions and capability discovery. Existing authenticated instance connections can discover supported operations and read app status/diagnostics. Exact conversation/Bridge identities, scopes, owner consent and existing send fences still apply. Never route by title or UI position.

Authenticated desktop/phone owner HTTP routes share application operations with the MCP boundary. An MCP bearer is not phone-owner authentication. Autonomous/MCP/cross-provider writes into Claude sessions remain disabled; existing owner-authored Claude replies and native decisions keep their supported controls. This does not prohibit an authorized Claude assistant from messaging a selected Codex thread.

Windows maintenance is available through packaged `app-control/control.ps1`: Status, Capabilities, Diagnostics, Start, Stop and Restart. Stop/Restart require the exact current instance identity. A current-user protected named pipe rejects remote clients; this is not a remote shell or provider-control endpoint. Lost acknowledgement never authorizes retrying a write. Diagnostics omit message content, credentials and raw logs.

Project boards: capability discovery includes `boards.cliPath`, publication instructions and the manifest schema. A desktop agent can use the installed CLI `-Command PublishBoard -Folder <absolute-folder>`. The current-user pipe pins publication to the current Host epoch; registration is idempotent by canonical folder. Only the manifest-listed presentation files become readable through paired phone APIs. Publication is not a remote MCP operation, shell, provider action or cloud upload. See [Boards](BOARDS.md).

Codex conversations do not require a Bridge. An INSTANCE assistant can read the exact native thread with `agbrio_read_chat` and use the existing reviewed `SEND_CHAT` action plus its durable receipt. Register only the selected conversation with `ENABLE_WATCH` if it has no context yet. The Claude plugin offers `/agbrio-bridge:codex` for this workflow; it needs the separately owner-consented MCP connection, not the observation token. See the [plugin guide](../tools/claude-code-plugin/README.md).


## Management area reminders (local opt-in)

Set `%LOCALAPPDATA%/AIWorkRouter/data/management-reminders.json` to opt one management area into reminders. Find the exact area ID with `ListManagementZones`. For example:

```json
{
  "zoneId": "00000000-0000-4000-8000-000000000000",
  "planbanRoadmap": "C:/path/to/local/planban/roadmap.json"
}
```

For the selected area's Codex members, Agbrio adds a separate `AWAITING_MANAGER` notice at 30 minutes and 2 hours after it first observes the current round completed, and at 10 minutes for a still open async question. Each threshold has one notice identity. After downtime only the latest due threshold is added. Existing `TURN_RESULT` behavior is unchanged.

A Planban wait exemption requires a unique thread binding on a current card, `metadata.waitOn` set to `user` or `prior`, and a nonempty `metadata.waitWhat`. Agbrio uses `metadata.waitUpdatedAt` if present and otherwise uses the card's `updatedAt`; set `waitUpdatedAt` when available so unrelated card edits do not refresh the wait evidence. A local owner script can instead maintain a `waits` array in the same Agbrio configuration file; each entry has `threadId`, `turnId`, `waitOn`, `waitWhat` and `recordedAt` (RFC 3339). Omit `planbanRoadmap` to use Agbrio evidence alone. An unreadable configured roadmap pauses reminder evaluation.

Successful, authorized Agbrio action receipts for `SEND_CHAT`, `RESPOND_CHAT` and `STOP_CHAT` on the exact execution thread clear its completed-round reminder. A pending, failed, or uncertain operation does not clear it. An async question stays eligible until the provider confirms the answer. Reminders stay in the management backlog and never start a model turn. A Claude manager can read them through `codex_wait status` or `ListManagementZones` (MCP: `agbrio_read_app`, `MANAGEMENT`).

## 2026-11-06 — Host-epoch replay receipts

Repeated local board publication and Codex notice registration no longer exhaust a lifetime 128-request set. The owner-local dispatcher now uses a connection-private SQLite temporary disk database with a 256 KiB page-cache target, mmap disabled, and bounded single-request input. Receipts remain valid for the entire Host epoch; the database is automatically removed on close. There is no count-based rejection or eviction of valid receipts. Disk exhaustion/IO failures fail closed.

Reserve the exact normalized request before effects. Completed retries return the original response, changed payloads conflict, and incomplete reservations return APP_CONTROL_REQUEST_UNCONFIRMED without re-execution. Error responses after reservation are also retained. Existing expected-instance, current-user pipe/ACK and shutdown fences remain. No provider messages or network/proxy configuration are changed.

Regression: 512 real fixture PUBLISH_BOARD calls followed by all 512 replays after deleting the source file; changed-command conflicts; reserved-unknown fencing and configured cache bound. Existing real Windows pipe/lost-ACK test remains required. This is engineering validation, not a 33-hour soak or owner UAT.

SQLite reference: https://www.sqlite.org/inmemorydb.html#temporary_databases and https://www.sqlite.org/pragma.html#pragma_cache_size .
