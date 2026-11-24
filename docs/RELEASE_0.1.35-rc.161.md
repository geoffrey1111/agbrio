# Windows rc.161 — read review-board approvals through MCP

`agbrio_read_board` now reads `kind: REVIEW` registrations as read-only status
instead of returning `REVIEW_BOARD_INTERACTIVE_SERVICE_USE_READONLY_STATUS`.
Existing clients can send only `boardId` and page with `cursor`. Refreshed tool
schemas also advertise optional `group`, `since` and `until` filters.

```json
{"boardId":"<id from agbrio_list_boards>","since":"2026-11-24","until":"2026-11-25"}
```

`since` is inclusive and `until` exclusive. Date-only bounds use Asia/Singapore;
RFC3339 timestamps must include an offset. `group` follows the original status
command's group ID/text matching. Responses contain group/groupId, item, status
(采用/退回/未审), original comment including newlines, timestamp, key, fingerprint
and active flag. Follow `nextCursor` with identical filters until null. Restart
on `REVIEW_BOARD_STATUS_REVISION_CHANGED`; do not mix revisions. A group leaving
the approval queue does not remove its status history.

The adapter uses the original owner-registered review service's `board.mjs`
`status()` projection, retaining its material fingerprint and archived-row
semantics. It does not execute commands supplied by MCP or import a path supplied
by a caller. A fixed embedded reader uses Agbrio's bundled Node runtime, a cleared
environment, read permissions for the registered service root and seven fixed
optional display-name config files. Filesystem writes, subprocesses, native
addons and workers are denied; output, runtime, concurrency and page size are
bounded. Only projected status fields are returned, without material paths,
drafts, CSRF values, notifier details or source exceptions. This is an adapter for
the already trusted owner-registered local service, not a sandbox for untrusted
JavaScript. Missing/incompatible status modules fail with a bounded error.

INSTANCE grant validation, expiry/revocation and static-document reading remain
in force. This adds no approval, post, withdraw, dispatch or authorization writes.
It does not fix upstream MCP client HTTP transport failures. Prepared packages
require an owner-run update before the running Host exposes this capability.
