# Agbrio observation plugin for Claude Code Desktop

Source candidate. Not automatically installed by Agbrio. Requires a Mods-capable
Desktop engine (documented minimum2.1.286); installed-engine validation remains
open. No test messages or model requests are needed to inspect/install an
observation-only plugin, but this development task did not alter the owner's
Claude installation or settings.

1. In the candidate Agbrio binding picker choose Claude Code and explicitly
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

The deployed module only calls native identity/version metadata, clock and local
HTTP. It observes turn lifecycle and native permission requests without answering
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
