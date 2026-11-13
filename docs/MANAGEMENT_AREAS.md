# Management areas

Open **Conversations → Management areas** to organize separate lines of work.

1. Create an area and select an existing Claude Code or Codex manager.
2. Add its executors, or adopt existing notification subscriptions together. Adoption can merge them into an existing area with the same manager; the dialog shows the target area.
3. Use **Change manager** to hand over an area. Its previous notification subscriptions are retired automatically. Use **Transfer** to move an executor to another area.
4. **Delete area** removes the area and all its assignments in one confirmation, cancelling its notification subscriptions. It preserves original conversations, watch settings and running tasks; uncertain delivery receipts are retained.

Each executor belongs to one area. Tasks keep running and their native IDs, histories and permissions stay in place. An area assignment does not grant extra MCP access.

Claude managers need the application MCP connection and the notification plugin. Configure MCP once for your computer's user account; after adding it, an already-open Code conversation may need its existing tools reconnected or the same conversation reopened.

While a Claude manager is working, fixed task notices can arrive with its next original text tool result. If it is idle, a separate plugin notice is used. A long tool must still finish before the next model step. Native delivery is different from the manager processing the task.

Managers can read the current assignments through `agbrio_read_app` in its `management` field. New tool lists also support `view: "MANAGEMENT"`. Appointment and transfer remain owner actions in Agbrio.

Conversation display names are refreshed from native metadata independently of history/status availability. A rename does not require reopening or watching the conversation again, and never changes its native ID or routing.
