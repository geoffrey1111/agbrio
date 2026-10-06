# Windows alpha validation

These scopes are distinct; none implies all-platform or physical-device UAT.

- Frontend: 406 Vitest checks across 37 files; TypeScript/Vite production build.
- Core: clean-export database/state/migration suite, 176 checks, one explicit
  provider gate ignored.
- Native: 161 checks, with 19 account-writing gates explicitly ignored by default.
- Runtime packaging: official pinned Chromium archive and executable hashes,
  Node notices and 757 locked dependency license entries retained.
- Real remote fixture: empty Agbrio database/config and ephemeral browser
  authentication; independent temporary public HTTPS tunnel; no personal
  Cloudflare Access/config; existing native Codex 0.160.0 backend, low-effort gpt-6-luna.
  Pair → select only instruction block + one file → reject unapproved send →
  approve exact payload → send once → read native executor marker. Anonymous
  write returned 401; replay returned 400. Test device logged out and only the
  temporary tunnel closed. Details: `remote-handoff-proof.json`.

The remote fixture deliberately reuses an already authenticated native backend
on the existing computer. It is not a new Codex installation or Mac proof.
The clean Windows build initially failed before running native tests because
the workflow had not generated dist yet. The build order was corrected; an
existing local dist directory had hidden that missing setup step.

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

## Clean Windows result

[Successful installation workflow](https://github.com/geoffrey1111/agbrio/actions/runs/37445032617). The actual unsigned NSIS installer runs on a separate clean Windows runner, followed by native Windows accessibility invocation of Settings/Devices, invalid HTTPS recovery, no premature pairing controls and actual maximized capture. Production WebView2 does not expose the requested CDP endpoint in this build; native UI Automation validates the shipped binary without enabling devtools. The source mock preview separately checks 1024 and 1920 layouts; it is not native acceptance.
