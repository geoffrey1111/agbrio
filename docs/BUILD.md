# Build from source on Windows

Prerequisites: Windows x64, Node 24.16.0, Rust 1.98 stable MSVC toolchain, Python
3.12+, PowerShell 7, Microsoft C++ build tools/Windows SDK, and WebView2.
The first native build and pinned Chromium download are substantial. Do not
copy another user's app-data, credentials or browser profile.

```powershell
npm ci
npm ci --prefix tools/isolated-browser-executor
npm test
npm run build
cargo test --manifest-path crates/Cargo.toml --locked
npm run build:browser-executor
cargo test --manifest-path src-tauri/Cargo.toml --locked
python scripts/license-inventory.py
# Optional: build with your own verified hosted service. Omit for self-host-only.
# $env:AGBRIO_HOSTED_CONTROL_ORIGIN = 'https://connect.your-domain.example'
npm run build:windows
```

`build:windows` runs the web build and verified runtime packaging automatically.
The NSIS installer is under `src-tauri/target/release/bundle/nsis/`. Packaging
uses a hash-pinned official non-branded Chromium and your Node 24 executable,
plus retained dependency notices. Codex Desktop, its account/session data and
your tunnel credentials are never bundled.

`npm run dev` is a browser UI surface; `npm run tauri dev` runs native IPC.
Ignored native provider tests require explicit fixture/account authorization;
ordinary test suites do not write messages into your existing conversations.

## Validation labels

Unit checks, engine browser tests, clean Windows installer smoke tests and
native provider handoffs have different evidence scopes. They do not by
themselves certify an iPhone physical layout, every Windows configuration, Mac,
or every provider version. Public release notes list the actually completed
gates and known limitations.

## Preview 16 package identity

The published tag is `v0.1.0-preview.16`; the installed application metadata is
`0.1.0`, and the interface revision is `2026.10.08-16`. The tag identifies this
preview snapshot without relabelling the already-tested installer. Official
preview builds set `AGBRIO_HOSTED_CONTROL_ORIGIN` to
`https://agbrio-connect.geoffreygroup.cc`. This is a public service address, not
a credential. Self-host builds may omit it or compile their own control origin.
The optional Worker needs separately provisioned custom domains, secrets and
capacity; it is not automatically deployed by building the desktop.

Run `node --test services/hosted-relay/worker.test.mjs` for isolated service tests.
The owner issuance utility is source-only. It does not include code strings,
operator configuration or Cloudflare account credentials.

## Signed0.1.1 builds

See [release identity and validation](RELEASE_0.1.1.md). Set your own TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD securely, then run `pwsh -NoProfile -File scripts/build-signed-update.ps1 -Version 0.1.1`. Nothing is published automatically. Never distribute private keys, passwords or operator/code files. Official builds use the already documented public hosted control origin.
