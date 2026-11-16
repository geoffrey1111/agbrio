# Assistant delivery diagnostics

`APPROVED` records a review, not delivery. A failure before the atomic writer
claim can leave an approved handoff without `sentAt`. Never replace its hash or
create another handoff to retry it blindly.

Read `agbrio_receipt` with the original `handoffId` and `expectedHash`. Historical
receipts use the live grant's exact Bridge/direction/hash scope; reading one does
not adopt its draft or grant permission to send it. Writes retain draft ownership,
current binding/source head, actual decision answers and one-attempt claim checks.

New preclaim failures persist `BRIDGE_PRECLAIM_<STAGE>` and a bounded cause in the
handoff. The receipt exposes `lastPreclaimFailure`; the MCP error also exposes a
structured code and stage. Stages distinguish selected files, assistant review,
competing chat writer, binding, adapter availability, target metadata/root,
acquisition/Goal, target refresh, exact active turn, reconciliation and claim.
These fields are diagnostic evidence, not a retry authorization.

An atomic claim clears an earlier preclaim diagnostic. A delayed diagnostic cannot
overwrite SENDING/SENT or change the approved payload. Lost acknowledgement after
claim remains uncertain under the existing protocol; do not resend, start a new
turn or infer completion from SENT. Normal active-turn delivery still uses the
exact expected turn; idle delivery keeps the existing start path.

If the assistant platform reports only `INVALID_ARGUMENT`, preserve that original
error, the call time and the original IDs. A new stage/cause from Agbrio can locate
the responsible server boundary. An old empty receipt cannot retroactively reveal
which check failed; current successful metadata reads do not prove the historical
failure is resolved. Do not disable authentication or repeat a production send
as a diagnostic test. No credentials, OAuth query URLs or conversation text are
needed in a bug report.

An async question can remain visible after its Codex turn ends. If
`RESPOND_CHAT` returns `REPLY_ASYNC_IDLE_USE_SEND_CHAT_NOT_SENT`, no answer was
submitted. If you still have permission to send the content, prepare a separate
`SEND_CHAT` with a complete message. `REPLY_ASYNC_TURN_CHANGED_NOT_SENT` means
Agbrio could not verify the exact active turn: refresh the conversation before
trying to answer. These errors do not clear any older UNKNOWN receipts.

For direct Codex replies with text only, an existing INSTANCE assistant can use
`RECOVER_UNKNOWN_CHAT{threadId,replyId,generation}` without asking again. Execute
with `ruleId=null,useOwnerAnswer=false`. The host checks that the native chat is
idle and verifies a complete, unchanged rollout window covering the attempt,
then acknowledges the original and claims one linked new SEND in the same
transaction. Text comes from the original receipt; an async answer is rendered
as a complete question/answer message. The action receipt records the policy,
the original and replacement IDs, the verified log interval and its digest.
A missing, rotated, incomplete, or changed log, observed input, attachment,
or another pending writer stops recovery. An uncertain replacement remains
uncertain and cannot be recovered recursively. This does not waive existing
approval requirements for other operations.

An UNKNOWN original-chat reply can block later Bridge sends even while the native
conversation is idle. Read `agbrio_read_chat` and its exact `state.replies` first.
To release future work without claiming old delivery, prepare the existing global
action flow with operation `ACKNOWLEDGE_UNKNOWN_CHAT` and input
`{threadId, replyId, generation}`. Ask the owner through `agbrio_ask_action_decision`,
record the actual answer/reference with `agbrio_answer_action`, and execute with
`ruleId=null, useOwnerAnswer=true`. Routine review and brief rules cannot bypass
this operation's owner-answer requirement. It applies the existing checked-but-
still-unknown marker, never sends and never makes the old request replayable.
Refresh the assistant's tool list after updating the server; the tool count and
OAuth grant stay unchanged.

Bound Bridge conversations use their exact current chat context/generation for
direct assistant replies; they do not require an extra standalone watch. Future
STEER requests include the durable reply ID as clientUserMessageId and require
an acknowledgement for the expected turn. A missing or mismatched ACK remains
UNKNOWN. This does not retrospectively prove delivery of an older request.

INSTANCE global Bridge operations resolve the requested exact workspace across
projects. Desktop-selected project is a display preference, not MCP authority.
Removed/archived Bridges and stale binding versions remain rejected for writes.

MCP access exposes callable operations; it does not enable autonomous event
wakeups. Choose delegated Bridges/tasks in the assistant conversation. Required
owner decisions and platform confirmations remain in force.
