# Build from source on Windows

Prerequisites: Windows x64, Node 24.16.0, Rust 1.98 stable MSVC toolchain, Python
3.12+, PowerShell 7, Microsoft C++ build tools/Windows SDK, and WebView2.
The first native build and pinned Chromium download are substantial. Do not
copy another user's app-data, credentials or browser profile.

```powershell
npm ci
npm ci --prefix tools/isolated-browser-executor
npm test
cargo test --manifest-path crates/Cargo.toml --locked
npm run build:browser-executor
cargo test --manifest-path src-tauri/Cargo.toml --locked
python scripts/license-inventory.py
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
