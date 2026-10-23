---
description: Send an owner-authorized message to an exact Codex conversation and read its reply through Agbrio; no Bridge binding required.
argument-hint: <Codex thread ID or link> <message, or read the latest reply>
---

# Codex conversations through Agbrio

Handle the owner's request: $ARGUMENTS

This command uses the existing Agbrio INSTANCE MCP tools. The observation adapter credential does not authorize MCP. No Bridge is required for Codex original-chat reads and SEND_CHAT. This does not enable autonomous writes into Claude sessions.

## First use

1. Check actual available tools, including namespaced plugin variants. Reuse an existing Agbrio MCP connection. Required: agbrio_read_app, agbrio_list_conversations, agbrio_read_chat, agbrio_prepare_action, agbrio_execute_action and agbrio_action_receipt.
2. If tools are missing, say that the observation plugin is present but the application MCP connection is missing. Do not say all cross-platform messages are disabled. Use Agbrio Settings > AI assistant to obtain its exact HTTPS /mcp address and an owner-consented whole-application connection. Add it with the supported Claude Code HTTP MCP workflow (for example, claude mcp add --transport http agbrio '<exact-HTTPS-MCP-URL>'). Inspect existing configuration before adding; preserve other servers and grants. The owner completes /mcp login and consent. Never use the observation token, phone pairing code or activation code as an MCP token. Do not copy credentials or create a grant on the owner's behalf. Tool discovery alone is not successful authorization.
3. Read agbrio_read_app; require a live INSTANCE grant and inspect approvalMode. Access does not delegate unrelated work. Respect the owner's exact destination, text, task and stop conditions. If only reading is requested, never send anything.

## Exact target and reading

Use the supplied native Codex thread_id or extract it from an exact Codex link. If missing, list conversations and let the owner choose by readable metadata; retain the returned native identity. Never route by title, sidebar position or a similar conversation. Reject a Claude session target for this command. Read agbrio_read_chat({threadId,cursor:null}); use its state.watch.threadId, generation, cwd and native history to confirm the exact target. If the native identity cannot be verified or the backend is unavailable, stop with the actual reason. A Bridge binding is not required. If the exact Codex thread has no registered context, first verify its native identity in agbrio_list_conversations, then prepare/review/execute one ENABLE_WATCH{threadId} action covered by the owner's request to communicate with that thread. This registers only that selected conversation; do not create a Bridge or enable unrelated watches. Read the resulting exact context again. If the target is absent or ambiguous, ask the owner rather than guessing.

## Sending

The owner's request to send the specified message is the task authorization. Do not fabricate approval or self-approve your own execution result. Preserve the requested text. Read current state before preparing. If idle use SEND; use QUEUE only when the owner requested waiting for the current turn. Never STEER, STOP, change model/effort/permissions, resume a goal or answer a decision merely to make a message go through. If busy and queueing is not authorized, report that condition.

Prepare one action with a fresh requestId and retain it:

operation: SEND_CHAT
input: {threadId: <exact ID>, generation: <state.watch.generation>, sourceSequence: null, expectedTurnId: null, mode: SEND, text: <owner-authorized text>, options: {model: null, effort: null, attachments: []}}

Use the actual tool schema and current returned fields. Review the prepared action's exact input, destination and payload hash. Execute with actionId=<returned id>, expectedHash=<returned payloadHash>, ruleId=null and useOwnerAnswer=false for a covered CONVERSATION_REVIEW action, plus a meaningful assessment only if allowed by the grant and owner instructions. If a decision is required, use the existing question/actual-owner-answer path. Never invent an owner answer or change the payload to bypass it.

Inspect agbrio_action_receipt with the same action ID and hash, then agbrio_read_chat. APPLIED alone is not delivery: inspect the nested original reply receipt and independently observed native user message. QUEUED means waiting, not sent. UNKNOWN/EXECUTING, a lost response or an ambiguous native row blocks another attempt. Preserve the original request/action/reply IDs; never replace them or automatically resend. Pending native questions stay pending; ask the owner if they require action.

## Receiving the reply

Retain the pre-send native turn/message anchor and original send receipt. Read only the same thread and follow returned pagination cursors; select the new assistant reply belonging after the independently confirmed send. Do not call an older result the reply, substitute a busy-state preview for history, or treat a turn started by another writer as proof of this send's result. Distinguish sent, running, needs-owner-decision and completed.

If the owner asked to be notified when the result arrives, prefer the native plugin tool `mcp__agbrio-bridge__codex_wait` (use its actual discovered name). After independently confirming delivery and reading the exact current Codex turn through authorized MCP, call it with `{action:"wait",threadId,generation,turnId}` from that verified watch. Do not guess a turn, subscribe to a different writer's turn, or register waits for every conversation. Confirm `status:"WAITING"` before promising a notification. The default expiry is one hour; specify a longer owner-requested duration up to 24 hours when necessary. The same tool supports `status` and exact-target `cancel`. Cancellation affects only an unattempted notification, never the Codex task. A pending native decision also ends this one-shot wait; after handling it, read the conversation again rather than automatically rearming or sending follow-ups.

The plugin checks only the Host's existing local watch projection on its observation heartbeat, and submits one clearly plugin-authored metadata notice into this same open Claude session. No additional provider polling, model helper, background agent, recurring task or Bridge is created. On that notice, use authorized `agbrio_read_chat` to read the full exact result and actual pending decisions. A notice is not a result, user instruction, approval or authority to send more messages. Continue only the owner's existing delegation. A closed/offline plugin cannot wake; expiry, generation changes or an unavailable/pruned source can prevent notification. UNKNOWN is never automatically resubmitted, including after reload; inspect the original wait status and read the source manually instead.

If the native tool or compatible Host is missing, say background notification is not active and an update is needed. Only when the owner explicitly requests an active bounded wait may you use at most 10 on-demand reads, spaced at least 15 seconds, stopping after 3 minutes or a terminal result/decision/error. Never replace an unavailable notification with a resident polling process or scheduled job. Preserve all send/wait IDs and writer ownership.

Report the verified destination, delivery state and actual new reply (or the precise unresolved state). Never send a test prompt to a real owner conversation for validation.
