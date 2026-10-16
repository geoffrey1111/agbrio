# Agbrio 0.1.35-rc.5 — shared Codex startup and recovery

This latest recommended public release contains the shared-connection repairs prepared before the
private character-board work. It does not include that board, private artwork,
conversation data, credentials or local runtime state. The owner promoted this repair to latest on 2026-10-16. 0.1.34 is historical;
this release retains its tested rc.5 version and original signed bytes.

## Changes

- Recover from short Windows sharing locks while saving shared-backend state.
  Readiness is reported only after state is durable, and checked again immediately
  before opening Codex Desktop.
- Include all 19 shared runtime resources, with separate **Codex (Agbrio shared)**
  and **Codex (return to normal)** Start Menu entries. Official Codex shortcuts and
  default startup are unchanged.
- Retain and verify the complete native tool-host cohort when preparing the shared
  connection; detect changed or missing application resources before idle reuse.
- Stop only processes with proven instance, creation-time and ancestry identity.
  Unreachable-owner recovery requires local confirmation and preserves unrelated
  Node processes, ordinary Codex and uncertain send receipts.
- Preserve the existing Claude manual replies, scoped assistant access, Events
  workflow, daily reports, native Goal controls and paired-phone behavior from
  0.1.34. Publication does not run the installer or operate live provider tasks.

## Exact package and checks

- Installer: `Agbrio_0.1.35-rc.5_x64-setup.exe`, 276,359,040 bytes.
- SHA256: `735a6b349b111d61e80e6cd3f1b5e2af06d1c1045be6ecedf3a93f422e98b405`.
- Product-code baseline: [f53297a](https://github.com/geoffrey1111/agbrio/commit/f53297ab16db612488eb46bd830afe5b3f689673).
- [Windows source/build/install](https://github.com/geoffrey1111/agbrio/actions/runs/38514056439): SUCCESS.
- [Exact signed 0.1.34 → rc.5 upgrade/uninstall](https://github.com/geoffrey1111/agbrio/actions/runs/38514860097): SUCCESS, including installed resources, both guarded entries, and preservation of fictional data/cache and unrelated shortcuts.
- Trusted updater-key signature and modified-byte rejection, all 19 raw packaged
  resources, cold preparation, bounded recovery and native tool-host checks passed.

Later owner-reported startup and exact live connection checks confirmed that shared
Desktop and a local shell tool could use the retained backend. This is bounded
evidence; it does not certify every old conversation history, every Desktop Code
Mode path, all Codex updates, all crash conditions or physical-phone acceptance.
The exact rc.5 package identity and those remaining limits are preserved.

The release source ZIP is a Git archive of the tagged commit, never a copy of an
active workspace. Later local character-board and settings-interface changes are
outside these signed installer bytes. No live task, shared process, tunnel or
account security configuration is changed by publication.

See [setup](SETUP.md), [build instructions](BUILD.md), and the chronological
[shared-connection evidence](SHARED_CODEX_RECOVERY.md).
