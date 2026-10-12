# Agbrio observation plugin for Claude Code Desktop

## Plugin 0.1.3: explicit owner replies

Receiving remains automatic and quota-free. Only an authenticated owner Send in Agbrio submits the approved exact text to the original native session via the official Mods prompt API. No helper model call, new/resumed session, permission override, goal control, interrupt or autonomous/MCP relay. Original model and permissions apply. The existing heartbeat carries a single durable command offer; submission is not SENT until an independently stored native user row is observed. Unknown attempts never automatically resend; only receipts retry.

After updating the installed plugin, safely run the built-in `/reload-plugins` once in the original conversation, without `--force`. Do not type `/agbrio-status` as a diagnostic prompt. Agbrio enables input only after observing protocol 1 from the real publisher. [Full contract](../../Docs/CLAUDE_CODE.md). Genuine owner-message acceptance is separate from SDK fixtures.


Agbrio0.1.32 packages the observation adapter. Plugin0.1.2 adds connection
failure diagnostics; static/native fixture checks are not real-session acceptance.
Installing or reloading this plugin does not submit a model prompt. Live operation
still requires a published exact native session and a naturally occurring reply.

1. In Agbrio binding picker choose Claude Code and explicitly
   generate an observation credential. Keep the local Host URL and token private.
2. Install this directory using your supported Claude Desktop plugin workflow.
   Configure `hostUrl` to the exact `http://127.0.0.1:port` origin Agbrio supplied,
   and `token` through the plugin's masked, secure `userConfig` field. Do not put
   secrets in project files, command-line strings, source control or AI prompts.
3. A plugin-enabled local session announces its exact session UUID/root/version.
   Choose that identity in Agbrio; titles are display only. Naturally occurring
   turns update state/final answers. Do not send a test message when quota is not
   authorized. Existing stored chats are not scanned/imported by this plugin.
4. Disable the plugin or revoke its observation credential in Agbrio to stop
   observation. Expired credentials require explicit re-pairing. Reusing native
   session identity does not create another task.

The deployed module calls native identity/version metadata, clock and local
HTTP, and registers one immediate connection-status command. It observes turn lifecycle and native permission requests without answering
them. It has no model, prompt submission, abort, process, file, MCP action or
automatic handoff calls. Provider/network failure never approves or sends a task.
Final answers are bounded at1MB; larger/unconfirmed answers are marked unavailable,
not partially forwarded as complete. Main sessions only; subagent completions do
not become independent Bridge replies.

Desktop/PWA/MCP production sends to Claude are currently disabled. The separate
offline control prototype is **not part of this plugin**. See
[scope and validation](https://github.com/geoffrey1111/agbrio/blob/main/docs/CLAUDE_CODE.md). Official strict validation passes on Desktop engine2.1.295. This does not prove
real session activation or delivery; verify a naturally occurring reply at Agbrio.

From the packaged plugin directory, add its local marketplace with `claude plugin marketplace add <plugin-directory>` and install `claude plugin install agbrio-bridge@agbrio-observation`. Save sensitive options through `claude plugin configure agbrio-bridge@agbrio-observation --values-stdin`, never command-line secrets. For the public marketplace: `/plugin install agbrio-bridge --marketplace geoffrey1111/agbrio`. Existing sessions need supported reload; do not force cache-invalidating reload or stop a live task.

## Connection diagnostics (plugin0.1.2)

Only when successfully registered by the running module, `/agbrio-status` is a
native immediate command. Its actual Desktop registration has NOT been verified;
do not use it as a diagnostic in the current Desktop integration. An unregistered
slash command can enter the ordinary prompt flow and reach a plan limit.
Its output contains only phase, allowlisted error, HTTP status, queue counters and
whether native identity is ready. It does not disclose tokens, URLs, session IDs,
project paths or reply text. Observation failures produce one sanitized native log
line per changed failure; repeated failures do not flood the transcript. Missing
command means the module has not registered; it does not identify why the loader
refused it. Use the actual reload/load result or existing debug log for that case.

Do not force reload or restart a live task. Keep the existing Bridge binding until
the exact session/root has a fresh lease. A successful install/configuration, an
empty catalog or a fixture PASS is not evidence of a real connection. Never put a
status command into a prompt if the client does not recognize it as a native command.
