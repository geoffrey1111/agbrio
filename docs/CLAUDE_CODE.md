# Claude Code Desktop — quota-free engineering phase

Owner decision, 2026-10-12: implement Claude Code Desktop integration, but do not
send messages using the owner's Claude Code or consume the limited quota.
This document records a **source candidate**, not a released/native acceptance.
Installed Agbrio remains0.1.30 unless a separate verified release is recorded.

## 2026-10-12 continuation: live observation authorized

The owner is already using Desktop Code and asked to see naturally arriving replies
in Agbrio. Observation setup, packaging and safe activation may continue; the
original no-send/no-extra-model-request restriction remains. Do not restart the
active Code engine, force plugin reload, submit a prompt or resume a duplicate CLI
session. A pending cache-invalidating reload must be left pending. Native observation
acceptance requires an actual naturally occurring answer at the bound Bridge.

The running Desktop engine was resolved through its actual Windows process image
(not its virtualized path) and reports2.1.295. Its official strict plugin validator
first rejected an SDK helper nested inside register. Lifting SDK helpers to module
scope repaired that compatibility error; strict validation and7offline fixtures pass.
Permission observation has a native error handler that passes through unchanged.
These results prove packaging/API compatibility, not an observed real reply.

## Implemented boundary

`CLAUDE_CODE` is an exact-role Bridge provider. `ClaudeCodeAdapter` owns native
session UUID + canonical local cwd verification, supported engine version,
45-second publisher lease, publisher-instance identity and normalized events.
Display labels and Desktop panes never route. Only explicit owner binding creates
a business endpoint; catalog registration is not automatic takeover.

The official Mods plugin in `tools/claude-code-plugin` registers observation
hooks only. Desktop Mods require Claude Code2.1.286 or later per the current
official documentation. An executable's Desktop app version is not the engine
version and does not prove compatibility. Runtime acceptance is still OPEN.

Native `turn.start` pins a runtime turn UUID. `turn.complete` with reason `answer`,
not aborted, carries the final visible answer and the same turn UUID. The adapter
keeps the complete text/code bytes; it never reads reasoning, credentials, other
chat files, screenshots or terminal input. Empty, oversized, interrupted,
refused/error or unconfirmed-turn outcomes never become a complete source reply.
Missing content is not silently truncated. Reply identity is the native session
and turn, not an invented native message ID.

Metadata heartbeat is5s, below the existing15s UI freshness budget. It does not
read provider content or start a model turn. Event delivery is serialized,
bounded and retries the same event ID/body; SQLite digest/sequence fences survive
restart. A new publisher instance cannot replace a still-live owner. Revocation
or expiry stops observation; explicit re-pairing may reclaim the same native
identity only after the prior observation credential is inactive.

## Setup and privacy

In the binding picker choose Claude Code. Desktop's explicit **Generate
observation credentials** action creates a30-day observation-only credential.
Its hash is stored in the existing private Router database; the secret is only
returned for the owner's setup. Native connection metadata supports later
revocation. Generating/copying config does not install, start or configure Claude.
Mobile may inspect the directory/select a live session but cannot mint credentials.

The plugin's `hostUrl` accepts only a literal loopback HTTP origin and port from
Agbrio. Its token option is marked `sensitive` for Claude's secure config storage.
Do not paste this private setup into dot or publish it. The event endpoint rejects
browser Origin and public proxy/Cloudflare headers; mobile/OAuth credentials do
not grant event publishing. Catalog responses contain metadata only. Registered
sessions are not automatically exposed as new delegated Bridges.

Install the plugin through the owner's supported Claude Desktop plugin workflow
only after native compatibility/configuration is ready. **This task did not
install it, change Claude settings, open/start/resume a session or submit a prompt.**
No `claude -p`, model API, first-party token extraction, remote UI automation,
CLI duplicate session or permission bypass is involved.

## What can and cannot be claimed

The source candidate can bind registered sessions and read normalized completed
answers through desktop/PWA Bridge. Hook completion feeds the existing exact
ReplyObservation and reply-ready Events machinery only for active, non-trashed
bound Bridges. Duplicate callbacks do not create duplicate observations. The
existing8 real subscriptions are not edited or broadened by this implementation.
Waiting native approval is shown as ACTION_REQUIRED; resolving Claude approval,
Claude-specific decision-required event integration and Goal controls are not
implemented here.

All production Claude writes are closed by `WRITES_ENABLED=false`, the Host send
guard, the Core claim guard and disabled confirmation UI. Approving a draft is
not a physical send. No Claude writer/run, queued prompt or UNKNOWN attempt is
created by a rejected send. Other providers retain their normal approval rules.

`tools/claude-code-adapter/control.mjs` is a separate offline prototype using the
documented `$.prompt.submit` / exact-turn `$.turn.abort` interfaces. It is not
bundled in the observation plugin, registered, polled or loaded by Host. Tests
use fake API objects only. Write-ahead UNKNOWN records, request fingerprints,
concurrent dedupe and pending-approval fences are checked. ACCEPTED there is
native-protocol acceptance, **not Domain SENT**, task completion or real provider
proof. Do not enable writes just by flipping a constant.

Quota-free observation can be validated with natural replies. Real write acceptance
separately requires owner authorization to spend quota: exact disposable Desktop session/root; one reviewed prompt;
independent same-session receipt/full terminal answer; interruption and restart
without duplicate submission. Preserve current tasks and model settings. Only
after transport/receipt integration is proven may production control be enabled.
This limitation follows the owner's no-send instruction, not an SDK claim that
Desktop control is impossible.

## Persistence and verification

One feature migration backs up SQLite, extends only endpoints/handoff provider
constraints, preserves exact rows/indexes/cross-table triggers and validates FKs.
Adapter credentials/session projection/bounded event journal stay in that same
database. Provider runs/project links are not advertised as Claude-capable.
Failure rolls back and restores foreign-key enforcement; historical SENDING is
retained and never replayed. Migration was exercised on disposable fixtures only.

Offline test evidence is recorded in AI/TASKS.md and AI/DEVLOG.md. Automated
fixtures, TypeScript/build and fictional browser layout are engineering evidence,
not USER_UAT_PASS or native Claude acceptance. No app installation/release is
implied by source edits.

Official sources checked2026-10-12:

- [Desktop / shared configuration](https://code.claude.com/docs/en/desktop)
- [Mods overview / Desktop availability](https://code.claude.com/docs/en/plugins/mods/overview)
- [Mods reference / native event and API contracts](https://code.claude.com/docs/en/plugins/mods/reference)
- [Mods API](https://code.claude.com/docs/en/plugins/mods/api)
- [Plugin manifest / sensitive config](https://code.claude.com/docs/en/plugins/manifest-reference)
- [Official published type declarations](https://github.com/anthropics/claude-code/blob/main/mods/types/claude-code.d.ts)

## Native no-session compatibility gate

Engine2.1.295 native test kit caught an asynchronous clock mismatch that synchronous Node fixtures missed. Await $.clock.now before serializing completedAt; native VM test verifies numeric time and full final bytes. The earlier signed candidate is retired and must not be distributed. Signature acceptance does not prove callback semantics.

Run `node tools/claude-code-adapter/native-compat.mjs <Claude executable> <existing task scratch directory>` to create an isolated fake-config fixture and run only `claude plugin test`. No session, sign-in, real network, model call or real turn is created. Remove the printed task-owned fixture after preserving the minimal result. This native test is still a fixture, not live Bridge delivery acceptance.

## Setup correction32

Native31 installation preserved all existing data/processes, but unmanaged local observation setup failed before creating any credential: it passed an empty static directory into the deployment parser. Fixed32 setup reads the actual existing loopback listener address and packaged plugin path; it never reparses or writes deployment settings. A real fixture listener with ephemeral port and no index.html proves setup plus authenticated callback, missing resource/listener creates no credential, and no provider starts. Live Claude activation/full reply acceptance remains OPEN.

## Delivered32 / safe existing-session activation

Corrected32 passed source Windows38205960554 and exact public installer38207804623.
Normal native updating and genuine unmanaged-deployment observation setup are
verified: the existing listener's address is used, a limited credential is generated,
and the native plugin installs/enables with both secure options read back. Existing
history, OAuth, pairing and subscription rows remain exact before any explicit
endpoint change. This is not natural-reply acceptance; current-session activation
and a real reply at the bound Bridge remain OPEN until observed.

1. In desktop Bridge binding choose Claude Code, open connection settings and
   generate one observation configuration. Keep it local and private.
2. Add the shown packaged plugin directory as a local marketplace:
   `claude plugin marketplace add "<plugin-directory>"`.
3. Install `claude plugin install agbrio-bridge@agbrio-observation --scope user`.
   This administrative command does not start a model session.
4. Fill `hostUrl` and the sensitive `token` in the native plugin configuration
   dialog. A trusted local tool may use `plugin configure ... --values-stdin`;
   do not put the secret in command arguments, files, AI prompts or public logs.
5. In the already-existing Desktop Code conversation, run `/reload-plugins` when
   safe. Do not use `--force` or spend an uncached request just for validation.
   If warned about cost/cache or restricted policy, leave activation pending.
   Never start/resume a duplicate CLI task to work around it.
6. Select the published native session UUID + canonical root in Agbrio, explicitly
   save the chosen role, then verify the next naturally occurring complete answer.
   No test message is needed. Binding changes revision; old scoped event filters
   require an explicit client update and cannot be silently treated as current.

Revoke the limited connection or disable the native plugin to stop observation.
The observation plugin does not automatically bind sessions, submit prompts or
forward replies. Existing assistant delegations and subscriptions retain their
separate authorization rules; this setup does not broaden them.
