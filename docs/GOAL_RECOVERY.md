# Native Goal status and controls

Agbrio0.1.27 synchronizes the existing Codex Goal. It displays native objective,
status, elapsed accounting, usage and current turn diagnostics, and controls the
same Goal through the native protocol. It does not implement another Goal engine.

## Desktop and PWA

Each Bridge role reader and original Codex chat shows its exact Goal above the
reply/composer. Expand the objective to read it in full. Pause or resume using the
adjacent control; ordinary success updates the Goal without a dismiss dialog.
The elapsed display follows native timeUsedSeconds/updatedAt interpolation while
fresh and active; pause or stale Host data freezes it. Derived display seconds
are never written into native accounting. Pending questions and uncertain sends
must be checked first. An uncertain Goal control keeps its original request and
receipt across reopening; it is not automatically repeated.

| Native status | Control boundary |
| --- | --- |
| active | Pause. An active turn and an active Goal are separate states. |
| paused / blocked | Resume the same Goal when the actual pending decisions and delivery records are clear and the thread is idle. A paused Goal is not permission to disregard an owner's stop. |
| usageLimited | Native resume is available to an explicit user; assistant execution additionally requires a saved actual owner answer. No budget increase. |
| budgetLimited / complete | No resume control. Do not recreate the Goal or raise its budget. |
| unknown / missing identity | Retain the status as unconfirmed; no guessed transition. |

Direct supplemental text remains genuine chat input. A verified loaded shared
idle chat can receive new instructions while its Goal remains active; active-turn
adjustments use native steer. Neither path is a replacement for RESUME_GOAL or
permission to replay an old handoff.

## Assistant workflow

1. Read `agbrio_read_chat` for the exact delegated chat. `state.goal` includes
   the native fields plus fingerprint; `state.goalControls.target` carries exact
   threadId/generation and the current Bridge/revision/role/endpoint when bound.
   Check native pending questions, owner decisions, original UNKNOWN/SENDING
   receipts, latest turn diagnostics and the owner's current instructions.
2. Prepare `PAUSE_GOAL` or `RESUME_GOAL` through `agbrio_prepare_action`, using
   input `{target: state.goalControls.target, expectedGoalFingerprint: state.goal.fingerprint}`.
   Keep one requestId and review its exact payloadHash. Do not write an objective,
   tokenBudget, accounting, new chat input or a replayed handoff in this operation.
3. When covered by delegation, use the existing reviewed execute/action_receipt
   flow. Missing business decisions still require the actual owner. For a native
   usage limit, first ask_action_decision and record the actual answer/reference;
   a routine assessment alone cannot authorize that resume.
4. Verify the action receipt and its nested Goal receipt/result. APPLIED confirms
   the native Goal transition; active does not mean the user's task completed.
   EXECUTING/UNKNOWN is not permission to mint another action or blindly retry.

The fingerprint pins thread, native creation identity, exact objective, status
and budget. Accounting counters and their advancing updatedAt are kept outside
that identity so normal progress does not make pause/resume unusable. Controls
recheck the current instance/binding, serialize local writers, and verify the
native returned state/objective/budget/accounting. Native status-set has no atomic
version-CAS parameter; it is not a promise against every concurrent external edit.
Missing required native identity disables controls.

`state.latestTurn` and each role activity project only exact turn/status,
whitelisted native errorCode, willRetry and confirmedTerminal. Raw native error
messages/URLs are excluded. A failed turn is distinct from a Goal status; native
retry-in-progress blocks an additional resume. The current Goal schema has no
universal stopReason/retryability field. Unsupported active-Goal/failed-turn
recovery is not synthesized by sending old text or creating another Goal.

## Scope and verification

The new desktop/PWA Goal command uses exact chat/role identity, including dual
Codex Bridges. Older workstream-only Goal HTTP/IPC endpoints remain legacy and
still fail closed on ambiguous dual-Codex bindings.

Seven isolated Host/store/adapter fixtures cover pause, paused/blocked resume,
unchanged objective/budget/accounting, fingerprint/binding/decision/active-turn
checks, persistent uncertainty, actual-owner usage-limit approval and active-Goal
supplemental input. A separate real shared-native disposable Goal validates one
resume/APPLIED/original-request replay with the same objective and budget, then
clears that test Goal. Existing user Goals and shared services were not changed.
Real native pause and all production failure scenarios are not independently
claimed by that one resume gate. Engineering validation is not owner UAT.

Events still publishes reply_ready and decision_required under the existing
contract. This change adds readable Goal/turn information and reviewed controls;
it does not add a general Goal-stop/turn-failure event or repair missing platform
`automations.mcp_event` injection. Keep existing delegated subscription scope.

Native lifecycle reference: [official Codex Goals guide](https://developers.openai.com/cookbook/examples/codex/using_goals_in_codex).
