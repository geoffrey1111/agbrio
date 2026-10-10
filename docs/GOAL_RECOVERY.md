# Goal and interrupted-turn recovery: current capability

Audited against released Agbrio 0.1.25. No real goal was stopped, resumed or faulted
for this audit. Read-only production reads confirm that read_chat returns goal
records for active execution chats. Project/session identities and objectives
are deliberately omitted here. Goal recovery through dot is not implemented.

| Surface | Actual support |
| --- | --- |
| MCP agbrio_read_chat | goal: threadId, objective, status, tokenBudget, tokensUsed, timeUsedSeconds, createdAt, updatedAt, activeTurnId; chat state and pending requests/receipts |
| MCP prepare_action | SEND_CHAT (SEND/QUEUE/STEER), STOP_CHAT, RESPOND_CHAT; no RESUME_GOAL |
| Desktop IPC | read_codex_goal, pause_codex_goal, resume_codex_goal, clear_codex_goal with confirmed human controls |
| Paired-device HTTP | GET /v1/mobile/workstreams/{workstream_id}/codex-goal; POST .../codex-goal/pause, /resume, /clear with confirmed=true and device authorization |
| CodexAdapter | thread/goal/get; thread/goal/set with exact threadId and status delta; thread/goal/clear; turn/start, turn/steer, turn/interrupt are separate |

Desktop/PWA resume updates the existing goal from paused to active, rechecks its
status/updatedAt and acquires the exact writer. It does not create a new objective
or increase tokenBudget. The current goal transition guard supports only
active->paused and paused->active; blocked, limited, complete and unknown statuses
are not generic resume candidates. Existing UI goal controls do not supply all
the additional MCP delegation/pending/UNKNOWN guards proposed below.

| State | Interpretation/action boundary |
| --- | --- |
| goal active + current turn active | Running; no resume/restart. Authorized supplemental input may STEER the exact active turn. |
| goal paused | Existing UI can explicitly resume that goal; MCP cannot currently perform the action. |
| goal blocked / pending native or owner decision | Resolve the actual decision first. Not a routine failure retry. |
| goal usageLimited / budgetLimited | Limits require owner decision; never raise budget or recreate goal. |
| goal complete/completed | Goal is finished; a new task is a separate owner decision. |
| latest turn completed | That turn ended; it does not prove an ongoing goal has finished. |
| latest turn failed | Execution turn failed; this is separate from a goal status. No universal congestion reason or recovery action is exposed. |
| persisted turn interrupted / INCOMPLETE | An idle history projection alone may be ambiguous. Confirm exact current/native terminal identity; never infer safe retry from it. |
| confirmed native turn interrupted / INTERRUPTED | Turn interruption is proven, but goal state must still be read independently. |
| UNKNOWN/SENDING send | Inspect the original receipt and pause; never resend the old handoff as recovery. |

read_chat does not currently project a structured goal stopReason/failureReason or
native turn error/retryability field. replies[].errorCode describes reply transport,
not a provider congestion diagnosis. MobileCodexGoal has no failure-reason field;
an unrecognized goal status must not be guessed to mean failed or resumable.

MCP Events currently emits complete reply_ready sources and decision_required for
owner/native questions, unresolved receipt or causal/loop boundaries. Native failed
or interrupted turn completions are recorded as FAILED/CANCELLED and return before
reply event production. No general goal-status-change or turn-failure/interruption
MCP event exists. An unrelated later event-woken scan may notice a stopped goal;
it cannot guarantee a wake on that failure itself.

## Minimal proposed additions (not implemented)

1. Project exact terminal turn status and sanitized stop/error/retryability when
   provided by the native protocol, plus a goal fingerprint (threadId, objective
   hash, updatedAt/status). Keep missing reasons explicitly unknown.
2. Add a reviewed RESUME_GOAL operation for a confirmed paused existing goal,
   guarded by Bridge/revision/role/thread/goal fingerprint, active authorization,
   explicit delegation, no active writer/turn, no pending approvals/questions and
   no uncertain sends. Use one persistent request identity and verify the updated
   same goal/receipt. Preserve objective, budget and usage; do not replay input.
3. Emit one durable decision_required notification for a newly confirmed exact
   interrupted/failed turn or goal stop, with typed status-source/read support,
   dedupe and subscription lifecycle checks. Never fabricate a complete reply.

An active goal with a failed idle turn needs native protocol investigation and an
isolated fixture before claiming a supported continuation primitive. Do not use
SEND_CHAT or goal recreation as a substitute, and do not manufacture a production
failure to test it. Blocked/limited recovery stays at an actual owner decision.
