# Agbrio local control

Run `control.ps1` with PowerShell7 on the same Windows user as Agbrio. Commands: Status, Capabilities, Diagnostics, Start, Stop, Restart. Installed default executable is its parent directory; source use needs `-Executable <existing ai-work-router.exe>`. Stop/Restart additionally require `-ExpectedInstance <instanceId from Status>`. JSON results, nonzero exit on uncertainty; never blindly repeat a write. No credentials or UI automation.

The executable must implement application control API1. Older installations remain unreachable until updated. MCP has separate scoped read/review tools; this local client does not grant remote maintenance or provider control. See the repository's Docs/APP_CONTROL.md for the protocol and acceptance boundaries.
