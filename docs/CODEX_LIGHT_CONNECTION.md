# Codex light connection (Windows)

The only Codex connection on Windows, always on. Codex is used as normally opened: no quit-and-reopen, no special entry and no resident backend. Phone and MCP keep reading, sending, steering, stopping, approving and answering. Other platforms keep their independent app-server and have no light connection, which is unverified since this removal: they cannot be compiled on the Windows machine this was built on.

Not available: Goal writes and automatic model-failure Goal recovery, administrator launch, tasks continuing after Codex Desktop quits, and token-by-token streaming. It does not reduce the memory Codex itself uses for open conversations.

## Upgrading from the shared connection

The resident shared connection was removed, but an upgraded machine can still have its backend running from the pinned copy in `%LOCALAPPDATA%\AIWorkRouter\codex-shared\runtime`, which carries its own `node.exe`. `src-tauri/src/codex_light/retirement.rs` retires it once, using only that copy's own scripts, while the Host stays off Codex:

- A backend with no active task and no connected Desktop retires by itself through the copy's guarded disable.
- One in use waits for the owner's confirmation in a desktop dialog, which says what will be interrupted. The dialog stops the processes the copy verifies as its own, then runs the same disable. The phone and MCP have no such path. Interrupted turns are not replayed. Desktop itself is never stopped, and a window that was using the backend must be reopened.
- An administrator backend asks for Windows consent. A declined consent or any other refusal stops the attempts until the next launch, and Settings shows the code.
- Unreadable settings or a pinned copy without the files the disable needs are reported the same way, and nothing is run.
- Nothing is killed by name.

Once nothing is pending, `legacy_cleanup.rs` clears reproducible leftovers on a fixed list: the `codex-shared` folder of a backend recorded as `DISABLED`, downloaded browser files and extension copies, and the cache folders inside old browser profiles. The rest of each profile stays, including cookies, saved passwords and local storage, and links are not followed. Folders it does not list are left alone.

## Threads running on the on-demand backend

The writer lock keeps Desktop from loading a thread while the backend holds it. Desktop already shares a thread between its own windows, with one window as owner and the others following its state over the pipe, so `owner.rs` answers as the owner of exactly the threads loaded on this process's backend:

- Each such thread has its own pipe connection, which claims only that thread. Closing it is how Desktop learns the owner has gone.
- `owner_state.rs` keeps Desktop's private conversation state up to date from the backend's notifications and sends full snapshots. Requests Desktop sends as a follower go to the backend. Nothing is retried.
- A `thread-unarchived` broadcast makes Desktop read a new thread and list it, so it appears without a restart.
- When the turn ends, the backend lets the thread go, and the connection closes only once the lock is free. Desktop then loads the thread itself.
- Nothing is claimed when the package's method table differs from the validated one, because a malformed state raises an error dialog in Desktop's main process. The thread then shows as open in another app until its turn ends.
- While a thread is shown this way only its newest turns are available. Desktop reads the complete history when it takes over. A queued follow-up, editing the last turn and changing settings are refused rather than dropped silently.

## How it works

The Host still talks app-server JSON-RPC over a child's stdio, so its receipts and uncertain-delivery handling are unchanged. `src-tauri/src/codex_light/` replaces only that child, as the same executable with `--codex-light-transport`:

- Reads go to an on-demand stdio app-server, cached against the rollout file, so an idle machine runs no extra Codex process.
- Writes to a thread Desktop holds go through Desktop's private pipe.
- Opening a thread the backend holds shows it through this process, as below.
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

`cargo test --lib codex_light` covers routing against a fake pipe and a scripted backend, and the retirement decisions. `scripts/codex-ipc-probe.mjs` is a read-only probe. `real_host_send_paths_over_the_light_transport` is an explicit ignored live test that needs real Desktop and a disposable thread; never point it at a real conversation. File and permission approvals, async questions, large-conversation snapshots and reconnecting after Desktop restarts were not exercised at this revision.
