# FAQ and troubleshooting

[Complete setup](SETUP.md) · [简体中文 FAQ](FAQ.zh-CN.md) · [Home](../README.md)

## Installation

### Which version should I download? What if 0.1.34 cannot connect?

Use [0.1.35-rc.5, the latest recommended release](https://github.com/geoffrey1111/agbrio/releases/latest). 0.1.34 is historical. The installer retains its exact rc.5 version/signature and bounded validation, without claiming every scenario is accepted. Distinguish shared Codex, phone and assistant failures, retaining the exact error. An upgrade cannot be promised to fix all network/account issues. Use official assets and [setup](SETUP.md), not repeated reinstall/data deletion.

### Must I buy a code or new account? Does Agbrio reset quotas?

Self-deployment needs no code; optional hosted access is separately available. Use your own provider account/quota. No quota bypass or billed setup test prompt is required.

### Is the updater signature Windows Authenticode?

No. SHA256 identifies bytes; `.sig` is for trusted updater-key validation. Neither promises Authenticode or no SmartScreen warning. Verify the official release and decide personally; do not disable security.

## Shared Codex

### Why open ordinary Codex and then close it?

Older discovery used a running backend, but switching required exit to avoid replacing active work. **rc.5 can discover official installation with Desktop closed.** After official install/login the ordinary-first loop is unnecessary.

### Must Agbrio be open before every launch?

The independent **Codex (Agbrio shared)** Start Menu entry does not need the Agbrio window. Bridge/mobile/assistant functions still need the Host. They are different parts.

### Will my official/taskbar icon open shared mode?

No. Official entry points remain unchanged. Pin/use the shared entry separately. Exit ordinary Desktop normally after tasks finish before switching.

### Is prepared the same as connected?

No. Backend-prepared precedes **Open shared Codex** and actual Desktop-connected status. Phone pairing and assistant authorization are separate too.

### It says Desktop is still running or a task is active. What should I do?

This protects existing work. Save, wait for tasks and exit the whole app normally; closing one window may not exit the backend. Do not repeatedly click, kill active tasks or stop all Node. Retain the code/time if it persists.

### What happens after reboot or official Codex updates?

Use the shared entry as the same Windows user; normally keep Bridges/login/pairing. Idle startup checks official install/components. Updates stay official; no hot-swap or guarantee for every future version. On failure retain the error and follow guarded fallback.

### A helper is missing, verification fails or no window appears. What now?

Retain `SHARED_...` code/stage and versions. Check complete official installation and intended release. Do not download arbitrary helpers, copy profiles or loop restarts. Follow [fallback](SETUP.md#4-daily-startup-reboot-official-updates-and-fallback) and report sanitized diagnostics. Generic failure text does not identify a unique cause.

### How do I disable shared mode? Why are there several Node processes?

Save/finish work, exit shared Desktop and use **Codex (return to normal)** or **Disable shared connection**. Confirm disable before opening the official entry. Recovery verifies instance, creation and ancestry; unreachable-owner cleanup requires explicit local confirmation and may interrupt shared work. Unknown identities stay untouched. Apps/plugins/tools can have separate children: count alone proves neither leakage nor ownership. Never kill all Node as routine recovery.

### Is shared mode an MCP server? Can I keep using ordinary Codex?

MCP provides assistant tools; it does not itself attach Agbrio to a running ordinary Desktop backend. Shared mode lets Router/Desktop continue the same backend. Official ordinary startup remains available, but do not assume equal live collaborative capabilities.

## Messages and goals

### What about duplicate titles, deleted Bridges or occupied chats?

Route by stable native IDs, using title/project/identity as a human check. Deleting a Bridge removes local association, not native chats. A deleted Bridge still claiming an endpoint is a bug to report, not permission to edit the database or steal a valid binding.

### SENT appeared but the target has not finished. Send again?

No. Delivery, execution and completion differ. UNKNOWN is not proof of non-delivery. Inspect the same request receipt and exact target history; duplicate notifications must not create duplicate handoffs.

### Can I select part of a reply? Are files automatically uploaded?

Select/edit/preview actual content, destination and files. Codex files go into `.aiwr/incoming/<handoff-id>/` with paths/hashes. That is not a ChatGPT upload or proof of image-model input.

### Is an attention dot or AI-assistant-handled label a receipt?

No. They indicate reading/transition. An executing target may still show its previous complete result. Check current state, source identity and independent receipt rather than treating it as new work.

### Are paused Goal, failed turn, pending decision and completion equivalent?

No. Verify original Goal/chat, authorization, pending decisions and uncertain sends. Resume only supported native states, without recreating work, increasing budgets or resending old instructions. [Goal controls](GOAL_RECOVERY.md).

## Phone, assistant and Claude

### The phone shows old data or loads slowly. Should I delete the PWA?

First check Host, computer sleep, network/HTTPS and web-shell revision. Cache can display without proving liveness. Update in About; rc.5 also has a Settings-root control. Do not clear pairing/drafts or re-pair as the first step. A web-shell update does not update Windows Host.

### Can phone/dot continue with the computer off?

Not against an offline local Host. Cache is historical; PWA does not run Windows backend. Push depends on permission/platform and is not proof of handoff/task completion.

### Does pairing authorize my assistant? Do MCP tools enable Events/reports?

No. Pairing, OAuth, tool discovery and explicit Bridge delegation differ. Events require real platform subscriptions and wake validation. Reports require an explicit time/timezone/destination/scope request and a supported scheduler, saved and verified. Reconnects reuse/update tasks; unsupported means not enabled. Historical tests are not provisioning. [Assistant](ASSISTANT_CONNECTION_GUIDE.en.md) · [Events](MCP_EVENTS_WORKFLOW.md) · [Reports](DAILY_REPORTS.md).

### Is webhook 200 enough to prove Events?

No. Delivery, real wake, source read, dedupe and prepare/review/send/receipt need separate proof. Event-detail injection still has a known gap; delegated-scope scan fallback needs prior owner approval. No silent polling/new scope.

### Claude plugin is installed, but no session or reply control appears. Why?

Saved configuration, reload, real session registration and manual-input readiness differ. Wait for the original task to finish before safe reload, without --force or test prompts. Replies need plugin 0.1.3 actually advertising protocol 1. Observation alone is not sending; autonomous/MCP Claude writes and Claude Goal controls remain unavailable. [Claude](CLAUDE_CODE.md).

### Can automation refresh through a ChatGPT security challenge?

No. Stop automated navigation/refresh/submit; the owner completes verification. No cookie copying, identity rotation or retry loop. Browser integration is experimental.

### What belongs in a useful bug report?

Agbrio/Codex/Windows versions, connection type/entry, failing step, exact error/time and active tasks. Redact private chats, username paths, full callback URLs, codes, cookies, tokens and keys. [Issue template](https://github.com/geoffrey1111/agbrio/issues/new?template=bug_report.yml).
