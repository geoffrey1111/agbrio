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

INSTANCE global Bridge operations resolve the requested exact workspace across
projects. Desktop-selected project is a display preference, not MCP authority.
Removed/archived Bridges and stale binding versions remain rejected for writes.

MCP access exposes callable operations; it does not enable autonomous event
wakeups. Choose delegated Bridges/tasks in the assistant conversation. Required
owner decisions and platform confirmations remain in force.
