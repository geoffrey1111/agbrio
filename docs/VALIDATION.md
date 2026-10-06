# Alpha candidate validation

These scopes are distinct; none implies all-platform or physical-device UAT.

- Frontend: 406 Vitest checks across 37 files; TypeScript/Vite production build.
- Core: clean-export database/state/migration suite, 176 checks, one explicit
  provider gate ignored.
- Native: clean-export tests with provider-writing gates ignored by default.
- Runtime packaging: official pinned Chromium archive and executable hashes,
  Node notices and 757 locked dependency license entries retained.
- Real remote fixture: empty Agbrio database/config and ephemeral browser
  authentication; independent temporary public HTTPS tunnel; no personal
  Cloudflare Access/config; existing native Codex backend, low-effort gpt-6-luna.
  Pair → select only instruction block + one file → reject unapproved send →
  approve exact payload → send once → read native executor marker. Anonymous
  write returned 401; replay returned 400. Test device logged out and only the
  temporary tunnel closed. Details: `remote-handoff-proof.json`.

The remote fixture deliberately reuses an already authenticated native backend
on the existing computer. It is not a new Codex installation or Mac proof.
The first fixture exposed Codex's empty newly-created thread metadata before
its first completed turn. The test now establishes terminal persistence before
binding and reuses its disposable source/target for network retries, with no
resubmission of an uncertain handoff.

Clean Windows installation and maximized native first-setup UI are separately
verified by the Windows Actions candidate gate. Until that gate succeeds, the
repository describes a candidate rather than a usable tagged installer release.
Tailscale real-account provisioning, macOS, other agent adapters and ChatGPT
browser handoffs are unaccepted scopes. iPhone screenshots/owner reports from
the personal build do not replace a fresh-install physical-device test.
