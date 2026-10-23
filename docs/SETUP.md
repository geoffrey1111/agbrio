# Complete setup and everyday use

Public Windows releases, checked 2026-10-22. Complete desktop setup first; phone, Claude Code and external assistants are optional, separate connections.

[简体中文完整教程](SETUP.zh-CN.md) · [FAQ and troubleshooting](FAQ.md) · [Home](../README.md)

## 1. Choose a version and install

Use [0.1.35-rc.55, the latest recommended release](https://github.com/geoffrey1111/agbrio/releases/latest). 0.1.34 remains historical; existing users upgrade instead of choosing between two recommended channels.

The exact **0.1.35-rc.55** installer version, signature and verified bytes are retained. Latest promotion does not certify every future Codex version, history or crash case. [Package identity and validation limits](RELEASE_0.1.35-rc.55.md). The installer includes a generic board viewer, without private board content, artwork or runtime data.

1. Use Windows x64 with your own official Windows Store Codex Desktop. Complete official installation/login first if needed. Agbrio does not bundle Desktop or provide an account/quota.
2. Download the installer from the selected release. Compare `Get-FileHash -Algorithm SHA256 -LiteralPath 'full installer path'` with its `SHA256SUMS.txt`; stop on mismatch.
3. Save work. Quit Agbrio through its tray menu before updating; do not run two instances. If switching Codex modes, finish tasks and exit Desktop normally at that point. Never force-stop a working task.
4. Install while retaining data, login, Bridges, drafts and pairing. Personally review Windows confirmations. The updater `.sig` is not a Windows Authenticode signature and does not promise no SmartScreen warning.
5. Open Agbrio and check Settings/About. rc.55 installs **Codex (Agbrio shared)** and **Codex (return to normal)** in the Start Menu.

Closing the Agbrio window normally leaves the Host in the tray; quitting there stops it. Remote access needs the computer awake and online. Settings → Language offers English, 简体中文, 繁體中文 or system default. Desktop and phone retain their own choice; messages are not translated. Japanese/Korean currently have README introductions only.

## 2. Why ordinary Codex used to open first

Official Codex Desktop is your existing application. Agbrio Host provides local Bridge/API/mobile service. Shared Codex starts the official Desktop against the checked shared backend so Desktop and Router can continue the same work.

Older discovery used a running native backend. Preparation and switching were separate: an ordinary Desktop already owns its backend, which cannot safely be replaced underneath active work. It had to exit before shared startup.

**rc.55 discovers and verifies the registered official installation while Desktop is closed. After official installation/login, an ordinary-open → prepare → close loop is no longer required on each use.** This is an alpha compatibility integration, not an official OpenAI remote-control feature or an ordinary MCP connection. Interface changes may make it fail closed.

Prepared backend, connected Desktop, paired phone and authorized assistant are four different checks.

## 3. First shared startup (rc.55)

### Independent Start Menu route

1. Ensure official installation/login is ready.
2. If ordinary Desktop is running, save work, let tasks finish and exit the whole app normally. Closing one window may leave a background instance. Do not kill all Node processes.
3. Open **Codex (Agbrio shared)**. It checks the install, native tools and existing owner before starting the official app. The Agbrio window need not be open first.
4. Run Agbrio Host for Bridge/mobile/assistant access. In Settings → Codex, verify **Desktop connected to shared connection** (`Desktop 已接入共享连接`).

### Agbrio settings route

With ordinary Desktop exited, choose Settings → Codex → **Prepare shared connection** (`准备共享连接`), then **Open shared Codex** (`打开共享 Codex`). **Shared connection prepared** (`共享连接已准备好`) only means backend-ready; wait for actual Desktop-connected status.

Active-task, still-running Desktop, unknown identity or native-tool errors are stop/diagnose conditions. Keep the exact code and stage; do not click repeatedly or terminate unrelated work. See [FAQ](FAQ.md).

## 4. Daily startup, reboot, official updates and fallback

| Situation | Action |
| --- | --- |
| Use shared mode again | Open **Codex (Agbrio shared)** directly. No ordinary-first loop or Agbrio-window prerequisite. Host must still run for Bridge/mobile. |
| Reboot | Sign into the same Windows user and use the shared entry. Do not recreate all Bridges or pairing. Automatic startup is not guaranteed. |
| Open official/taskbar icon | Official startup remains unchanged; it is not rebound to shared mode. Pin/use the shared entry separately. |
| Ordinary Desktop already open | Finish work and exit normally before switching. No competing writer around active work. |
| Official Codex update | Update through official channels. Idle shared startup rediscovers/checks components. No active-backend hot-swap or guarantee for every future version. |
| Shared runtime update pending | Finish work, exit shared Desktop, disable, then prepare/open again. Retain failures rather than looping. |
| Return to ordinary Codex | Save work, exit shared Desktop, use **Codex (return to normal)** or **Disable shared connection**. After confirmed disable, use your original official entry. |

The independent return entry does not need the Agbrio window. Unreachable-owner recovery requires explicit local confirmation, default No, and may interrupt unfinished shared work. Only proven instance/process creation/ancestry identities are eligible; uncertainty is retained and reported. Never kill all Node, erase the profile, replay uncertain sends or fill a missing helper with an arbitrary downloaded executable.

## 5. Create a Bridge and send one handoff

1. Prepare the two actual conversations, such as project management and implementation. No billed test prompt is required for connection setup.
2. Create a Bridge and select control/execution conversations. Check title, project and identity details. Routing uses stable native IDs, not names; equal titles are not equal chats.
3. Save and inspect both sides. Cache is not proof of live connectivity. Rebinding requires another identity/direction check; do not steal another valid Bridge's endpoint.
4. Wait for a complete source reply. Choose instruction/result blocks and necessary files; not all analysis needs forwarding.
5. Edit and preview the complete outgoing text, exact destination, direction and files. Confirm send once and check its original record/receipt; no success-dismiss acknowledgement is required.
6. `SENT` confirms that handoff's delivery, not completion of the target task. Follow execution and the next complete reply.
7. Timeout or `UNKNOWN` is not proof of non-delivery. Read the same request receipt and exact target history before another action. Do not create another request to hide uncertainty or resend on duplicate notifications.

Selected Codex files are copied to the target project's `.aiwr/incoming/<handoff-id>/`, with paths/SHA-256 in the message. This is not a ChatGPT upload or proof that every model received a visual image input. Check supported content in the preview.

Deleting a Bridge removes local association, not native chats. A deleted Bridge still claiming a chat is a bug to report, not a reason to edit the database.

## 6. Monitor, receive notifications and answer questions

Monitoring follows a single native chat without a two-ended Bridge. Pause does not delete it; paused entries sort below active entries, with order stable during a visit. Notifications link to exact chats; historic notifications and dots are not delivery receipts.

For supported Codex question cards, choose or supplement an answer after checking the role/request. Unsupported approvals/actions stay in the native app. Goal status synchronizes the native objective and accounting; Goal, turn, pending decision and send states differ. Pause/resume only supported states of the same Goal, never recreate or enlarge its budget. [Goal controls](GOAL_RECOVERY.md).

An **AI assistant handled** transition label does not replace the independent receipt. An executing target can still show its previous complete reply; do not treat it as new work.

## 7. Add a phone (optional)

1. Keep Windows, Agbrio Host and required shared/provider connections awake and online. PWA installation does not move the backend to the phone.
2. Settings → Devices: select your own HTTPS/Cloudflare/Tailscale entrance or an optional separately supplied hosted code. Self-deployment requires no code.
3. Use **Copy deployment instruction** for the provider/actual port. Verify HTTPS reaches this exact Host before saving; never expose unauthenticated chat access or share credentials.
4. Open the displayed mobile URL, generate a desktop pairing code and enter it there. Do not use someone else's installation/code.
5. On iPhone, add to Home Screen from Safari and allow notifications when supported. Other devices follow their own installation/notification controls. Pairing does not grant push permission automatically.
6. Changing HTTPS origins requires pairing and push registration at the new origin.

[Phone deployment and pairing](PHONE_SETUP.md) covers providers, TLS/Host checks, limits, renewal/revoke/replace and commands. Real Tailscale-account end-to-end acceptance is not claimed. Cached content offline is not a live status.

## 8. Connect an assistant (optional)

1. Settings → AI assistant: choose **ChatGPT / dot cloud** or **Codex local**, copy this instance's instructions and use the intended actual client.
2. Complete personal consent without publishing credentials. Rediscover tools and perform a real read-only check in that client. Desktop-ready is not remote-connected.
3. Explicitly delegate Bridges, directions, task, must-ask cases and stop conditions. Application-wide access does not delegate all projects.
4. Routine delegated handoffs keep prepare → review → send → receipt and deduplication. Conflicts, scope changes and decisions still require the owner.
5. **Events** need platform support, real subscriptions and wake acceptance. Ordinary MCP tools do not create subscriptions. Event-detail injection still has a known gap; only an explicitly preauthorized delegated-scope scan fallback may process a wake without details. Never add polling/new Bridge scope silently. [Events workflow](MCP_EVENTS_WORKFLOW.md).
6. **Daily reports** follow read-only acceptance: choose time (suggested local 22:00), timezone, destination and scope; explicitly submit. Verify real scheduling tools, update/reuse existing reports and verify the saved task. Unsupported means not enabled. OAuth consent alone does not authorize schedules. [Accounting and continuous windows](DAILY_REPORTS.md).

[Assistant walkthrough](ASSISTANT_CONNECTION_GUIDE.en.md) · [简体中文](ASSISTANT_CONNECTION_GUIDE.zh-CN.md) · [繁體中文](ASSISTANT_CONNECTION_GUIDE.zh-TW.md). Historical Events acceptance does not provision subscriptions for new users.

## 9. Claude Desktop Code (optional)

Select Claude Code in a Bridge role or watched-chat picker. First configuration uses desktop **Connect Claude Code**; mobile selects already-connected sessions. Reuse existing configuration, wait for real publisher registration and check title/root/native session ID. Configuration-saved is not session-connected.

If reload is needed, wait for the original task to finish and use built-in `/reload-plugins` in that session, without `--force` or diagnostic/test prompts. Stop on extra-cost/forced-load warnings. Only observed status/replies are shown; full pre-connection history is not promised.

Authenticated owner text replies require plugin 0.1.3 actually advertising manual-input protocol 1. Use **Reply to original chat** and verify independent stored-native-row `SENT`, not submit/HTTP success. Unknown sends do not automatically repeat. Autonomous/MCP/cross-provider Claude writes, attachments, Goal control and abort/steer remain unavailable. [Claude contract](CLAUDE_CODE.md). Installed/reloaded does not equal real-provider send acceptance.

## 10. Updates, recovery and support

- **Windows:** check app updates or use the latest release page. Keep data; 0.1.34 is historical.
- **Phone interface:** use About's interface-update control. Web-shell updates do not update Windows Host. Do not clear pairing/drafts to update.
- **Shared recovery:** use section 4's guarded return/disable flow; no all-Node kills, profile copy, certificate change or uncertain-send replay.
- **Provider security challenge:** stop automated navigation/refresh/submit. The owner clears it; no cookie copying, identity rotation or retry loop.
- **Feedback:** include Agbrio/Codex/Windows versions, entry, connection type, failing step, exact error/time and active-work status. Redact private text, username paths, full callback URLs, tokens, codes and keys. [Report a bug](https://github.com/geoffrey1111/agbrio/issues/new?template=bug_report.yml).

Bounded Windows/package/native/live-connection checks do not certify every history, Desktop Code Mode, future update, crash case or physical-phone experience. macOS is not an accepted release target. [FAQ](FAQ.md) · [Release evidence](RELEASE_0.1.35-rc.55.md) · [Build](BUILD.md).
