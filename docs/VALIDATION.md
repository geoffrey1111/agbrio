# Windows alpha validation

These scopes are distinct; none implies all-platform or physical-device UAT.

- Frontend: 415 Vitest checks across 38 files; TypeScript/Vite production build.
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
verified by the Windows Actions candidate gate. The localized release gate
succeeded on source e490066; its actual installer hash is recorded below.
Tailscale real-account provisioning, macOS, other agent adapters and ChatGPT
browser handoffs are unaccepted scopes. iPhone screenshots/owner reports from
the personal build do not replace a fresh-install physical-device test.

## Clean Windows result

[Successful installation workflow](https://github.com/geoffrey1111/agbrio/actions/runs/37455874754). The actual unsigned NSIS installer runs on a separate clean Windows runner, followed by native Windows accessibility invocation of Settings/Devices, English system language, language-selector presence, invalid HTTPS recovery, no premature pairing controls and actual maximized capture. Production WebView2 does not expose the requested CDP endpoint in this build; native UI Automation validates the shipped binary without enabling devtools. The source mock preview separately checks 1024 and 1920 layouts; it is not native acceptance.

## First-release app localization

Nine additional frontend cases prove preference persistence and state/content
boundaries. Global Playwright exercised the production PWA with fixture APIs at
390×844, 440×956 and 1920×1080: all three languages, reload persistence, visible
selectors and notification preferences, no horizontal overflow or page errors.
This is automated browser validation, not physical iPhone acceptance. The localized
Windows installer and fresh native first-setup gate passed in workflow 37455874754.
Native UI Automation proves English first-setup and selector presence; the
three-language switch/layout/state checks are browser/component evidence. The
earlier Chinese-only gate 37445032617 remains historical evidence.

Locale recovery uses raw ownership error identity, never a translated-message substring.
A focused test changes English to Traditional Chinese after THREAD_OWNED and verifies
the connection recovery action remains available without dispatching.

Validated app/build source: `e4900662a6c45695cb8104c46053b8956f6911ac`.
Installer SHA-256: `6fd9e750e6361c7fb2fbf350e8761972b303e7caa54b7ae7229ec7793c1f5aa4`.
This build is unsigned and remains an alpha, not an all-platform stable release.

## 2026-10-08 · preview.16 hosted activation

Separate from alpha.1: the final local NSIS package was installed and checked in
a maximized native Windows window, with interface 2026.10.08-16 and served asset /
native connector parity. Existing self-host entry and paired sessions remained;
Codex, its Gateway and the existing Cloudflare tunnel were not restarted. The
release asset is byte-identical to this installer, not a relabelled alpha.1 build.

Before export: 445 frontend cases plus 8 focused cases after expiry-copy refinement;
TypeScript/Vite and NSIS build; 192 Core and 171 Host cases (24 ignored). Pairing
replacement covers the 20-session cap. Seven isolated Worker tests pass. Two empty
disposable Rust Hosts exercised real public HTTPS/Worker/WSS: private content,
foreign cookie/Origin/material rejection, independent inbox, redemption/renewal,
old-code replay, wrong installation key, duplicate connector, expiry/revocation
and Host restart retaining phone login, without model/provider writes. Browser
checks used 440×956 and 1920×1080; these are not physical-iPhone acceptance.

Private runtime proofs, credentials, codes and personal conversations are not
included in source. Public export checks are listed separately in release notes.
No fresh-recipient Windows, physical-iPhone, Mac, live Dot account or large-scale
capacity acceptance is claimed. Earlier alpha.1 clean-Windows results remain
historical and must not be attributed to this preview.

Public sanitized export checks: 446 frontend tests in 44 files, production
TypeScript/Vite build, 192 Core tests (1 ignored), 7 Worker tests and 15 shared
lifecycle tests. Privacy scan of 1,481 selected/tracked files matched none of the
20 actual private code/admin values and no private runtime files. 758 locked
third-party dependencies have notices/source links; GSAP is separately licensed.
`npm audit --omit=dev` reports zero findings. The complete npm audit reports two
high-severity dev-tool packages (jsdom's undici 8.10.0 and source-map-js 1.2.1);
those npm modules are not shipped in the app. This release preserves the tested
lockfiles; dependency upgrades are a separate build/test change, not a claim that
all dependencies or bundled runtimes have been security-audited.

Public sanitized native source also passes 171 Host tests (24 ignored), plus
one launcher test. Native resource inputs reuse verified vendor files only;
no production processes are launched or changed by these unit checks.


## 0.1.7 tutorial and localized introduction
487frontend tests/52files, TypeScript/build, native browser whitelist and existingOAuth tests.54browsercases/3languages/3widths/6steps have no documentoverflow; Englishguide CJK0. See tutorial-browser-geometry-final.json and fictionalhandoff proof. These are isolatedrealcomponents, notliveDotwrite or physicaliPhone acceptance. Exact signedartifact/installer evidence is tracked separately.


## 0.1.8 INSTANCE directory
Regression first reproduces selected-project-only/removed-row enumeration, then checks cross-project activeIDs, lifecycle partitions, selectionindependence, exactproject/bindingRevision, unchangedgrant/snapshot, legacy8tool refusal and revoked/expired guards. NativeMCPRPC read_app/read_bridge/read_source pass with a read-only offlineadapter, zero provider/modelturns. Core199pass/1ignored; OAuthHTTP11pass/2ignored. RealDotpostupdate acceptance remains owner-only; independent read_chat/receipt reports are not claimed fixed.
