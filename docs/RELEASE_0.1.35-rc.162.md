# Windows rc.162 — Claude conversation display titles

Claude conversations without desktop App title metadata now use an exact native
session UUID and matching project root to read a bounded display-title fallback:
a native custom title, then the first public user prompt's first line. The phone
listener picker, conversation catalog and observed chat projection use it. Desktop
titles retain priority. Tool results, sidechains, internal/meta messages and local
slash commands do not become fallback titles. This never starts a model or writes
native transcripts, stored sessions or delivery receipts.

Desktop metadata accepts title-before-identity field ordering while still stopping
at transcript, MCP and permission payload boundaries. Native transcript fallback
reads at most the first and last 128 KiB, skips oversized lines and validates the
native session and root; unreadable/missing identity still stays unnamed. Cached
fallbacks invalidate when the source file changes. Conversation identity, archives,
publisher leases, authorization and UNKNOWN delivery fences are unchanged.

Includes rc.161 read-only review-board status support. Source/fixture validation is
not verification on the reporting user's Windows PC or physical phone. Prepared
packages require an owner-run install; this does not publish or restart the Host.
