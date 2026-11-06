# FAQ and troubleshooting

[Complete setup](SETUP.md) · [简体中文 FAQ](FAQ.zh-CN.md) · [Home](../README.md)

## Installation

### Which version should I download? What if 0.1.34 cannot connect?

Use [the latest recommended release](https://github.com/geoffrey1111/agbrio/releases/latest). 0.1.34 is historical. A recommended release has bounded validation, without claiming every scenario is accepted. Distinguish Codex connection, phone and assistant failures, retaining the exact error. An upgrade cannot be promised to fix all network/account issues. Use official assets and [setup](SETUP.md), not repeated reinstall/data deletion.

### Must I buy a code or new account? Does Agbrio reset quotas?

Self-deployment needs no code; optional hosted access is separately available. Use your own provider account/quota. No quota bypass or billed setup test prompt is required.

### Is the updater signature Windows Authenticode?

No. SHA256 identifies bytes; `.sig` is for trusted updater-key validation. Neither promises Authenticode or no SmartScreen warning. Verify the official release and decide personally; do not disable security.

## Codex connection

### Do I have to open Codex a special way?

No. Open it your usual way. Agbrio connects by itself; there is no shared entry, quit-and-reopen or resident backend any more. [How it works](CODEX_LIGHT_CONNECTION.md).

### What can't this connection do?

Change a Goal from Agbrio, launch Codex as administrator, keep tasks running after Codex Desktop quits, or stream token by token. It does not reduce the memory Codex itself uses.

A task sent to a conversation that is not open in Codex runs on a temporary backend owned by Agbrio. The conversation still appears in Codex, and you can open it, watch it and send messages from there. Quitting Agbrio interrupts it. A few seconds after it finishes, Codex takes the conversation over by itself.

### Settings says the old shared backend is still there. What should I do?

Confirm the dialog, or use the same action in Settings → Codex. Running tasks are interrupted and not resent, and a Codex window using it needs to be reopened. If you would rather not interrupt anything, finish your tasks and quit Codex, and Agbrio stops it by itself. If an error code is shown instead, nothing was ended; reopen Agbrio to try again, and report the code if it persists. Do not kill all Node. [Upgrading](SETUP.md#3-upgrading-from-a-version-with-the-shared-connection).

### Codex says a chat is open in another app. What now?

A task started from your phone or an assistant is running on that chat in a temporary backend. Desktop can open it a few seconds after the task ends. To watch a task live, open the chat in Desktop first.

### What happened to ChatGPT Bridges and the Router browser?

They were removed. A Bridge with a ChatGPT end keeps its saved replies and must be rebound to Codex or Claude Code to continue. ChatGPT connected as an assistant through MCP still works.

### Why are there several Node processes?

Apps, plugins and tools can have separate children: count alone proves neither leakage nor ownership. Never kill all Node as routine recovery.

## Messages and goals

### What about duplicate titles, deleted Bridges or occupied chats?

Route by stable native IDs, using title/project/identity as a human check. Deleting a Bridge removes local association, not native chats. A deleted Bridge still claiming an endpoint is a bug to report, not permission to edit the database or steal a valid binding.

### SENT appeared but the target has not finished. Send again?

No. Delivery, execution and completion differ. UNKNOWN is not proof of non-delivery. Inspect the same request receipt and exact target history; duplicate notifications must not create duplicate handoffs.

### Can I select part of a reply? Are files automatically uploaded?

Select/edit/preview actual content, destination and files. Codex files go into `.aiwr/incoming/<handoff-id>/` with paths/hashes. That is not proof of image-model input.

### Is an attention dot or AI-assistant-handled label a receipt?

No. They indicate reading/transition. An executing target may still show its previous complete result. Check current state, source identity and independent receipt rather than treating it as new work.

### Are paused Goal, failed turn, pending decision and completion equivalent?

No. Verify original Goal/chat, authorization, pending decisions and uncertain sends. Change Goals in Codex itself on Windows; the light connection does not write them. Never recreate work, increase budgets or resend old instructions. [Goal controls](GOAL_RECOVERY.md).

## Phone, assistant and Claude

### The phone shows old data or loads slowly. Should I delete the PWA?

First check Host, computer sleep, network/HTTPS and web-shell revision. Cache can display without proving liveness. Update in About. Do not clear pairing/drafts or re-pair as the first step. A web-shell update does not update Windows Host.

### Can phone/dot continue with the computer off?

Not against an offline local Host. Cache is historical; PWA does not run Windows backend. Push depends on permission/platform and is not proof of handoff/task completion.

### Does pairing authorize my assistant? Do MCP tools enable Events/reports?

No. Pairing, OAuth, tool discovery and explicit Bridge delegation differ. Events require real platform subscriptions and wake validation. Reports require an explicit time/timezone/destination/scope request and a supported scheduler, saved and verified. Reconnects reuse/update tasks; unsupported means not enabled. Historical tests are not provisioning. [Assistant](ASSISTANT_CONNECTION_GUIDE.en.md) · [Events](MCP_EVENTS_WORKFLOW.md) · [Reports](DAILY_REPORTS.md).

### Is webhook 200 enough to prove Events?

No. Delivery, real wake, source read, dedupe and prepare/review/send/receipt need separate proof. Event-detail injection still has a known gap; delegated-scope scan fallback needs prior owner approval. No silent polling/new scope.

### Claude plugin is installed, but no session or reply control appears. Why?

Saved configuration, reload, real session registration and manual-input readiness differ. Wait for the original task to finish before safe reload, without --force or test prompts. Replies need plugin 0.1.3 actually advertising protocol 1. Observation alone is not sending; autonomous/MCP Claude writes and Claude Goal controls remain unavailable. [Claude](CLAUDE_CODE.md).

### What belongs in a useful bug report?

Agbrio/Codex/Windows versions, connection type/entry, failing step, exact error/time and active tasks. Redact private chats, username paths, full callback URLs, codes, cookies, tokens and keys. [Issue template](https://github.com/geoffrey1111/agbrio/issues/new?template=bug_report.yml).
