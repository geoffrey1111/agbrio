# Codex light connection (beta, off by default, Windows only)

An opt-in alternative to the shared Codex connection, in Settings. Codex is used as normally opened: no quit-and-reopen, no special entry and no resident shared backend. Phone and MCP keep reading, sending, steering, stopping, approving and answering.

Not available in this mode: Goal writes and automatic model-failure Goal recovery, administrator launch, tasks continuing after Codex Desktop quits, and token-by-token streaming. It does not reduce the memory Codex itself uses for open conversations.

## How it works

The Host still talks app-server JSON-RPC over a child's stdio, so its receipts and uncertain-delivery handling are unchanged. `src-tauri/src/codex_light/` replaces only that child, as the same executable with `--codex-light-transport`:

- Reads go to an on-demand stdio app-server, cached against the rollout file, so an idle machine runs no extra Codex process.
- Writes to a thread Desktop holds go through Desktop's private pipe.
- Writes to a free thread go to the on-demand app-server. Codex's own writer lock keeps it from sharing a thread with Desktop.
- A held thread with no reachable owner is refused.
- A write whose outcome is uncertain gets no reply, so the Host records it as unknown. Nothing retries a write.

## The private pipe

`\\.\pipe\codex-ipc` is private to OpenAI and has no compatibility promise. Before any frame, the client requires the pipe server to be the `ChatGPT.exe` of the installed OpenAI Codex package; anything unverifiable, including an elevated Desktop, closes the pipe. Writes close when the package's method table differs from the versions validated here. Validated on Desktop 26.1103.3517.0 only.

Observed behaviour, not a contract:

- A wrong method version is answered as `no-client-found`, the same as no owner, so the table is compared first and the writer lock is checked.
- `thread-follower-steer-turn` needs `restoreMessage` with `cwd` and `context.workspaceRoots`, and ignores the expected turn, so the caller compares it.
- `thread-follower-interrupt-turn` returns `ok:true` for a stale turn; only `interruptedTurnId` proves the stop.
- A held writer lock fails a one-byte read with Windows error 33, so holding can be observed without acquiring it.
- An empty thread does not outlive the app-server that created it, so that backend stays until the first turn or a 15-minute idle limit.

A thread the on-demand backend holds cannot be opened in Desktop, which shows "open in another app". The backend lets go a few seconds after the turn ends, and Retry then works. Until the turn ends Desktop cannot open it, so open a conversation in Desktop first to watch a task live: threads Desktop holds are sent through Desktop.

## Validation

`cargo test --lib codex_light` covers routing against a fake pipe and a scripted backend. `scripts/codex-ipc-probe.mjs` is a read-only probe. `real_host_send_paths_over_the_light_transport` is an explicit ignored live test that needs real Desktop and a disposable thread; never point it at a real conversation. File and permission approvals, async questions, large-conversation snapshots and reconnecting after Desktop restarts were not exercised at this revision.
