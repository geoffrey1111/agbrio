# Codex result reminders in Claude Code

- Ask Claude to notify you when a selected Codex turn finishes. Its result or request for attention returns to the same open Claude conversation.
- Check or cancel a reminder without stopping Codex. Reminders expire and avoid duplicate notifications.
- Update the Agbrio Claude plugin to **0.1.9** as well as the desktop app. Reload the plugin when your current task is idle, then ask Claude to wait for the selected result. Your existing MCP authorization is reused for reading the reply.

Keep Agbrio and the requesting Claude session open. A reminder contains status only; it does not authorize further messages or decisions. Closed sessions cannot be woken.

[Claude plugin setup](https://github.com/geoffrey1111/agbrio/tree/main/tools/claude-code-plugin)
