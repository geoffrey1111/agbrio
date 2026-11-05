# Agbrio local control

Run `control.ps1` with PowerShell7 on the same Windows user as Agbrio. Commands: Status, Capabilities, Diagnostics, Start, Stop, Restart. Installed default executable is its parent directory; source use needs `-Executable <existing ai-work-router.exe>`. Stop/Restart additionally require `-ExpectedInstance <instanceId from Status>`. JSON results, nonzero exit on uncertainty; never blindly repeat a write. No credentials or UI automation.

The executable must implement application control API1. Older installations remain unreachable until updated. MCP has separate scoped read/review tools; this local client does not grant remote maintenance or provider control. See the repository's Docs/APP_CONTROL.md for the protocol and acceptance boundaries.

On macOS use the bundled Node with `control.mjs` (no PowerShell install needed):

```text
/Applications/Agbrio.app/Contents/MacOS/browser-executor-node /Applications/Agbrio.app/Contents/Resources/app-control/control.mjs Status
```

Commands: Status, Capabilities, Diagnostics, Start, Stop, Restart, PublishBoard, PublishReviewBoard, RegisterCodexNotices. Use `--app /absolute/path/Agbrio.app` for another installation; Stop/Restart require `--expected-instance <instanceId from Status>`. Board publication uses `--folder`; review boards may add `--review-port` and `--title`. Notice registration uses `--notice-binding-file` with the existing reviewed binding schema. Never repeat an uncertain write automatically.

The macOS listener is `/private/tmp/agbrio-control-v1-<effective UID>/control.sock`. Its directory is mode0700, socket mode0600; the Host verifies the kernel peer UID. A lifetime lock fences duplicate listeners and permits replacing only an owned refused stale socket. No TCP listener or remote lifecycle API is added. Installed lifecycle, account and permission behavior still require separate acceptance.
