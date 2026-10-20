# Agbrio 0.1.35-rc.19 — bounded tunnel recovery

Windows x64 alpha. Source baseline: [c65f33b](https://github.com/geoffrey1111/agbrio/commit/c65f33bddcda1bc1f428fb5a1ab18601600aabdf).

The tunnel watchdog now requires a healthy local Host and approximately five continuous minutes with both the tunnel and public entrance unavailable before attempting recovery. It attempts at most once an hour during a Host run. Local-listener recovery is unchanged. The package also includes the current desktop/PWA interface and owner-approved Claude Code text relays; autonomous/MCP Claude writes remain disabled.

- Installer: `Agbrio_0.1.35-rc.19_x64-setup.exe`, 277,163,981 bytes.
- SHA256: `1f3bf23f2b8e7fb68134c9add0927fe1e488c7df2d1805322b569712e4893c41`.
- Trusted updater-key signature, manifest identity and modified-byte rejection: passed.
- All 19 shared text resources match the public source (CRLF/LF normalization only).
- The packaged executable matches the previously installed and checked rc.19 executable.
- Watchdog regressions: four targeted tests passed.
- [Exact public-installer Windows install/startup/UI checks](https://github.com/geoffrey1111/agbrio/actions/runs/39385168471): SUCCESS.
- [Full Windows source/build/install](https://github.com/geoffrey1111/agbrio/actions/runs/39383090604): frontend, Core and native tests passed; overall build/install job still running at publication.

The source ZIP is a Git archive of the source baseline. Private board content, artwork, conversation data, credentials and local runtime state are not included. Local Clash rules are machine configuration and are not installer changes.

Sustained real-outage behavior, physical-phone stability/UAT and the earlier unexplained Host restart remain unverified. Two healthy direct tunnel connections on the development machine do not establish four-connection or all-network acceptance. Publication does not restart a user's live services. Save work and quit Agbrio through its tray menu before installing; preserve existing data and pairing.

[Download](https://github.com/geoffrey1111/agbrio/releases/tag/v0.1.35-rc.19) · [Setup](SETUP.md) · [Historical rc.5 evidence](RELEASE_0.1.35-rc.5.md).
