Agbrio0.1.16 — faster Bridge and conversation navigation

- Last successful lists, Bridge replies and chat history appear from a bounded private cache.
- Shared foreground synchronization deduplicates reads and keeps recently used data warm.
- Paired phone cache survives reopening after live login verification; logout/revocation and device replacement isolate it.
- Preserve drafts, latest-message arrival, manual history reading and exact live send checks.
- Avoid reparsing unchanged Markdown during status refreshes. Background phone execution is not guaranteed.
