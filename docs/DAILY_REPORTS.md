# Optional assistant daily reports

Agbrio supplies read-only receipt accounting and a copyable configuration request.
The connected client owns scheduling and delivery. OAuth, connection, tutorial
navigation and copying text do not create a scheduled task or consent to one.

## Configure, change or disable

1. Connect the intended cloud/local assistant and complete **actual read-only**
   acceptance. Agree the exact Bridge IDs/revisions and both permitted directions.
2. In **Settings → AI assistant → Daily report (optional)**, choose the delegated
   Bridges, destination, IANA timezone and daily local time (suggested **22:00**).
   The acceptance checkbox records your statement; it is not independent proof.
   Skip this step if you do not want reports.
3. Copy and explicitly submit the request to that assistant. It must inspect its
   real scheduling, list, update and saved-task readback tools, as well as whether
   a scheduled run can access this MCP connection and deliver to your destination.
   Exact-time/delivery/tool limitations mean **NOT ENABLED**, not a prompt promise.
4. Inspect existing reports for the same instance, assistant and destination;
   update the original ID. Unavailable listing or ambiguous matches must stop
   creation. Recopying/reconnecting does not authorize a duplicate.
5. Verify actual saved task ID, enabled state, local time, timezone, next run,
   destination and allowlist. Change with the same entry; **Disable existing
   report** pauses that report only. Existing Events remain untouched.

The client is account/version dependent. [Official task documentation](https://help.openai.com/en/articles/10291617-scheduled-tasks-in-chatgpt)
and [dot tasks and memory](https://learn.chatgpt.com/docs/dots/tasks-and-memory)
describe saved schedules and management; they do not prove this user's tools or
MCP availability during an actual run. A local client may support separate
automations or none. Unsupported clients can make an explicitly requested manual
read-only report. Agbrio never starts a background business-polling agent.

## Continuous windows and honest accounting

Use **[previous scheduled cutoff, current scheduled cutoff)** in UTC milliseconds.
The first report must state its agreed exact start. At 22:00, do not count only
00:00–22:00: tomorrow's window starts at today's 22:00, so 22:00–24:00 is covered.
Delayed/missed runs keep the prior cutoff, not actual wake time. Compute calendar
cutoffs using the selected timezone (including DST). A changed schedule retains
its original accounting cursor. Advance after report delivery; explicitly retain
any unverified interval for later reconciliation rather than dropping it.

`agbrio_read_report` is INSTANCE-only and takes an explicit owner-delegated
`workstreamIds` allowlist, `windowStart`, `windowEnd`, optional `limit` (1–200) and
`cursor`. It reads stored local receipts, **never observes providers, sends,
changes grants, creates subscriptions or schedules**. The same authorization
expiry/revocation checks apply on every page. A cursor binds the grant, normalized
scope, window and receipt digest. On `REPORT_LEDGER_CHANGED_RESTART_READ`, restart
the same read-only window; do not emit a final complete report from mixed pages.
Follow all `nextCursor` values until exhausted. `pageComplete` only means there
is no following page; it cannot prove the client read previous pages.

Group by project/Bridge and DECISION→EXECUTION / EXECUTION→DECISION. Success
requires `confirmedSentInWindow=true`, `status=SENT`, and `sentAt` in the window.
Deduplicate `handoffId`, then `(actorGrantId,requestId)`; count SEND_CHAT separately
by action/request identity and its SENT receipt. Preparing earlier and sending
in this window counts here. Failed/UNKNOWN/EXECUTING/APPROVED-only/read-only and
duplicate events do not count. **SENT is delivery, not completed business work.**

The authenticated physical-send attribution added in 0.1.27 proves a **grant**,
not a unique dot. Match this assistant's retained exact handoff/request evidence
before calling a record its own. `THIS_GRANT`, `OTHER_ASSISTANT_GRANT` and
`UNATTRIBUTED` remain distinct; approval alone cannot prove sender identity.
Separate confirmed manual and other-agent evidence. Never label all unattributed
history "manual", fabricate old attribution, or treat grant totals as dot totals.
The API exposes `attributionComplete=false` and explicit warnings. SEND_CHAT
records cover current exact Bridge generations; earlier rebindings/manual native
writes may be unavailable. Coverage is **AGBRIO_STORED_RECEIPTS_ONLY**, not every
message sent outside Agbrio. Such gaps require **PARTIALLY VERIFIED**, with known
counts and unknown items clearly separated. Scope never includes other users.

Read authorized Bridge/source/native chat history for actual results, follow-ups,
blocks and pending decisions. History retention/pagination gaps must be disclosed.
Daily runs cannot prepare/send/execute business actions or alter Events.

## Sanitized example

Time `22:00`, timezone `Asia/Shanghai`, destination `this assistant conversation`,
scope `example-bridge` revision2, both directions. First start is explicitly
agreed. "Confirmed own handoffs: 2; SEND_CHAT coordination: 1; 3 unattributed
receipts pending verification" is appropriate **only with matching real evidence**.
These numbers are fictional, not a product acceptance result.

## Verification boundary

Receipt/window/cursor/scope and configuration/copy behavior are testable locally.
Saved-task creation and a timed report delivered through each supported client
need real client evidence. This implementation does not claim universal client
scheduling acceptance, change the owner's existing 22:00 task, or fix the open
platform event-payload injection gap.
