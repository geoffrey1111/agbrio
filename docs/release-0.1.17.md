# Agbrio0.1.17: cached Bridge list navigation

Switching from Bridge to Notifications/Settings and back uses the existing
activity snapshot. Previously the list effect directly fetched on every return,
and warm cache reads also started refreshes when due. Exact-ID batch caching
and one shared five-second foreground clock now own activity synchronization,
including while a different page is open. Failed refreshes retain the real
observation time; expired status does not appear newly running. Cold reads,
explicit refresh and foreground/network recovery remain valid reads.

No visual redesign. Existing drafts, manual history reading, OAuth/MCP tools,
owner approval and live backend send checks are retained. In-progress MCP Events
is separate work and is not included in this release.

Validation:521 public frontend checks and production build;61 final scoped
checks. Actual localhost production components at360/440/1280:20 navigation
switches add no activity requests, clock and away-page clock update once,
return adds none; offline reentry preserves cache without claiming fresh status.
Transport fixtures are fictional and do not prove physical phone acceptance.

Source frozen at f60e017962da464bfd7b04b5998b2c85413f1dfc.
Windows installer SHA256 d50b4520ada400c5f893387e0e15a9aff84cbbd65d2b8b9ea453d8116b9554e9.
