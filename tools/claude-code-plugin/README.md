# Agbrio for Claude Code

Keep Claude Code conversations within reach in Agbrio. With an authorized Agbrio MCP connection, ask Claude to message a selected Codex conversation and read its reply — no Bridge binding needed.

## Talk to Codex

Use `/agbrio-bridge:codex` with a Codex conversation link or native thread ID and the message you want to send. Use the same command to read a later reply.

For example:

> /agbrio-bridge:codex <Codex thread ID> Send: Please summarize the current progress, then show me the reply.

The command reuses the existing Agbrio MCP tools. If they are missing, connect Agbrio under **Settings → AI assistant**, add its exact HTTPS MCP address in Claude Code and complete the owner OAuth login/consent in `/mcp`. The observation connection alone does not supply these tools. Keep existing MCP servers and permissions.

Read first, send once, check the original receipt. Queued means waiting; sent means delivered, not finished. Unknown delivery is never resent automatically.

With a compatible Agbrio Host and plugin 0.1.11, ask “notify this conversation when that Codex turn finishes.” Claude registers a one-shot wait for the exact selected turn. A finished result, decision request or failure returns a short notice to the same open Claude conversation; Claude then reads the full result through your existing MCP authorization. Ask to check or cancel that wait at any time. Cancelling a wait does not stop Codex. Waits expire after one hour by default, at most 24 hours, and require the plugin/session to remain open. An unavailable source or uncertain notification is reported honestly and never automatically resent. Updating this source alone does not update a running installation.

## Follow Claude conversations in Agbrio

Use Agbrio's **Connect Claude Code** setup. Its local observation connection announces exact native session identity and project root; select that conversation in Agbrio. Original model settings, native tasks and permissions remain in place. Owner-authored replies and native decision handling retain Agbrio's existing authorization controls.

Observation credentials and MCP authorization are separate. Never paste tokens, pairing codes or activation codes into a conversation or source file. The narrow local wait probe returns only exact Codex turn identity and state, never reply bodies. The plugin only wakes the same session for an explicitly requested wait; it does not turn on arbitrary Claude writes or a cross-agent loop.

## Install or update

Use the Agbrio marketplace in your supported Claude Code plugin interface:

`/plugin install agbrio-bridge --marketplace geoffrey1111/agbrio`

Already installed? Update the plugin in that interface. Existing sessions use the supported reload flow when idle; do not stop a task or force a cache-invalidating reload. Installing the command is separate from authenticating MCP and verifying a real send/reply.

[Agbrio setup](https://github.com/geoffrey1111/agbrio/blob/main/docs/START_HERE.md) · [Claude integration boundaries](https://github.com/geoffrey1111/agbrio/blob/main/docs/CLAUDE_CODE.md)

Up to 128 waits can run at once. Settled history no longer blocks new waits; exact-turn receipts are preserved to prevent duplicate notices. Update the installed plugin and reload only while the session is idle.
