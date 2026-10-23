# Agbrio for Claude Code

Keep Claude Code conversations within reach in Agbrio. With an authorized Agbrio MCP connection, ask Claude to message a selected Codex conversation and read its reply — no Bridge binding needed.

## Talk to Codex

Use `/agbrio-bridge:codex` with a Codex conversation link or native thread ID and the message you want to send. Use the same command to read a later reply.

For example:

> /agbrio-bridge:codex <Codex thread ID> Send: Please summarize the current progress, then show me the reply.

The command reuses the existing Agbrio MCP tools. If they are missing, connect Agbrio under **Settings → AI assistant**, add its exact HTTPS MCP address in Claude Code and complete the owner OAuth login/consent in `/mcp`. The observation connection alone does not supply these tools. Keep existing MCP servers and permissions.

Read first, send once, check the original receipt. Queued means waiting; sent means delivered, not finished. Unknown delivery is never resent automatically. Waiting is bounded while the assistant is active; it is not a background subscription and does not keep a closed conversation running.

## Follow Claude conversations in Agbrio

Use Agbrio's **Connect Claude Code** setup. Its local observation connection announces exact native session identity and project root; select that conversation in Agbrio. Original model settings, native tasks and permissions remain in place. Owner-authored replies and native decision handling retain Agbrio's existing authorization controls.

Observation credentials and MCP authorization are separate. Never paste tokens, pairing codes or activation codes into a conversation or source file. The plugin does not turn on autonomous Claude writes or a cross-agent loop.

## Install or update

Use the Agbrio marketplace in your supported Claude Code plugin interface:

`/plugin install agbrio-bridge --marketplace geoffrey1111/agbrio`

Already installed? Update the plugin in that interface. Existing sessions use the supported reload flow when idle; do not stop a task or force a cache-invalidating reload. Installing the command is separate from authenticating MCP and verifying a real send/reply.

[Agbrio setup](https://github.com/geoffrey1111/agbrio/blob/main/docs/START_HERE.md) · [Claude integration boundaries](https://github.com/geoffrey1111/agbrio/blob/main/docs/CLAUDE_CODE.md)
