# Mac shared Codex lifecycle candidate

Internal stage-2 implementation; the production Rust admission remains
`PLATFORM_NOT_YET_SUPPORTED`. No environment variable, journal field, MCP call,
or fixture result may enable the shipped Mac entry. No OpenAI installer is
downloaded, bundled, or uploaded by this component or its tests.

The [official app-server protocol](https://learn.chatgpt.com/docs/app-server)
defines the JSONL transport, initialization handshake and read-only activity
queries. Agbrio's prepare/activate/close are application transactions around
that protocol, not invented app-server RPC methods:

| Step | Implemented behavior |
| --- | --- |
| prepare | Persist PREPARING; create one owned stdio child; initialize once, send initialized, verify a subsequent read; persist PREPARED. |
| activate | Bind the exact epoch; double-check loaded thread identities/status and notification revision; admit that connection, persist ACTIVE. No thread creation, resume, prompt or permission response. |
| close | Fence admission, confirm known idle, persist CLOSING, send one stdin EOF, require actual child exit and pipe drain; persist STOPPED. No shutdown RPC is fabricated, and no kill/retry is used. |

ACTIVE means this candidate's Agbrio protocol connection is admitted. It does
not attest a Codex Desktop attachment. `desktopConnected` and
`realPackageVerified` remain false. The candidate's exposed connection is
read-only lifecycle plumbing; existing reviewed business operations continue
to belong to CodexAdapter, not this module. Production consumer wiring and
Desktop interoperability cannot be asserted from fixtures; the exact installed
package and its existing native identities must be checked in stage 4 before
the product gate is opened by a separately reviewed change.

The real adapter accepts a locally selected canonical executable and its
expected SHA256, then uses fixed app-server stdio arguments. It performs no
installer discovery/download, account/token configuration, native permission
override, TLS/CA registration or Desktop UI automation. Caller configuration is
internal, not a new remote executable/argv/env API. Inherited native account
settings are not copied. There is no root helper or new listener.

The journal uses an owner-private root, exclusive operation intent and atomic
state writes. A crash, unknown transport or lost transition never causes
automatic process adoption or request replay. A saved PID is insufficient;
reconciliation requires the same in-memory child handle to have actually
exited. Orphaned locks/partial journals require explicit local inspection;
there is no automatic stale-lock deletion or reset command. Do not use this
candidate as an installed service until the stage-4 native ownership, binary
replacement, Host-restart and consumer-binding gates are closed.

Run `node --test tools/shared-codex-macos/lifecycle.test.mjs`. Tests use an
original Node protocol fixture in an isolated temporary directory, never an
OpenAI binary, real account or model turn. Twelve tests passed on Windows in
the current local batch; Mac CI for this uncommitted batch is pending owner
authorization to commit/push. Tests are added to the existing Mac source
workflow. Mock results are not real-package or user UAT evidence.
