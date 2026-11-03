# Agbrio

**Your agents keep working. Stay in control from your phone.**

[![Download](https://img.shields.io/github/v/release/geoffrey1111/agbrio?label=download)](https://github.com/geoffrey1111/agbrio/releases/latest)
[![Build](https://github.com/geoffrey1111/agbrio/actions/workflows/windows-alpha.yml/badge.svg?branch=main)](https://github.com/geoffrey1111/agbrio/actions/workflows/windows-alpha.yml)
[![MIT](https://img.shields.io/github/license/geoffrey1111/agbrio)](LICENSE)
[![Windows](https://img.shields.io/badge/Windows-x64-0078D4)](https://github.com/geoffrey1111/agbrio/releases/latest)

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

Follow Codex and Claude Code conversations, reply with the next instruction, and get notified when something needs you. Pair planning with execution, or let one managing conversation coordinate several executors.

![Desktop conversation overview](docs/assets/en/conversations-current.png)

## Check progress. Reply from wherever you are.

See running work, pending decisions and child-conversation activity at a glance. Open the original conversation to read results, follow up or answer supported questions—without a remote desktop.

<p align="center"><img src="docs/assets/en/mobile-conversations-current.png" width="300" alt="Phone conversation list and activity"> &nbsp; <img src="docs/assets/en/mobile-reply-current.png" width="300" alt="Reply to the original conversation from your phone"></p>

## Choose conversations. Watch or connect them.

Pick existing conversations on your computer. Watch one to stay up to date, or bind a manager and executor as a Bridge. Select the content, check the destination and approve each handoff.

<p align="center"><img src="docs/assets/en/select-conversation-current.png" width="300" alt="Choose a conversation to watch"> &nbsp; <img src="docs/assets/en/review-current.png" width="300" alt="Review content and recipient before sending"></p>

![Choose a planning conversation and executor](docs/assets/en/binding-current.png)

## Let a managing conversation handle the follow-up.

Delegate progress checks, task messages and result summaries across several Codex conversations. Bring decisions back to you when needed. Your managing conversation can be in Claude Code or Codex.

```mermaid
flowchart TD
  U["You · direction and key decisions"] <--> M["Manager · Claude Code / Codex"]
  M <--> A["Codex · interface"]
  M <--> B["Codex · API work"]
  M <--> C["Codex · verification"]
```

Delegated sends target Codex. You can reply to Claude Code yourself from your phone. You keep control of decisions and delegated scope.

## Get started

1. Download the Windows app and connect the agent you already use.
2. Choose a conversation to watch, or connect a manager and executor.
3. Pair your phone in Settings → Devices, then open your Agbrio address.

Keep your computer and Agbrio online. Enable notifications on the receiving device.

[Download](https://github.com/geoffrey1111/agbrio/releases/latest) · [Setup guide](docs/START_HERE.md)

<details>
<summary>A few useful extras</summary>

Read linked documents, images and project boards in place. API / MCP is available when you need a custom integration.

</details>

<sub>Screenshots show the current interface with fictional example data.</sub>


## macOS preview candidates (Apple Silicon)

macOS support is in progress, without Windows feature parity. The manual macOS candidate DMG Actions workflow creates an ad-hoc-signed .dmg artifact for macOS 13.5+ / Apple Silicon; it does not publish a Release or store updater signing secrets. macOS source checks/tests the code. Until actual runs succeed, builds remain unverified. Integrations may show Not supported on this platform yet.

Verify the checksum, then copy Agbrio.app to Applications. For a trusted unsigned/ad-hoc candidate blocked by macOS, use System Settings → Privacy & Security → Open Anyway, or run the command below yourself. No automatic quarantine removal. No Apple Developer account is required; candidates have no Developer ID signature or notarization.

```text
xattr -dr com.apple.quarantine /Applications/Agbrio.app
```

Only implemented features request notifications, Files & Folders/removable volumes for selected files, or Full Disk Access for protected locations. A future read-only AX integration needs Accessibility; the legacy browser integration is currently retired. Pi elevation, all-local-volume authorization, privileged helpers and signed in-app updates remain unimplemented. Ad-hoc identity changes each build; updates may require permission approval again. CI/build/startup smoke is not feature parity or user UAT.
