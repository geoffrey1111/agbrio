<img src="docs/assets/agbrio-icon.png" width="72" alt="Agbrio">

# Agbrio

**Agent Bridge — handoffs between the conversation that manages and the conversation that executes.**

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

Already use one agent conversation to manage a project and another to do the work? Agbrio is built around that workflow.

The managing conversation keeps the plan, reviews results and decides the next task. The executing conversation works in the project, implements changes and returns evidence. **Bridge carries the selected instructions, results and supporting files between those two existing conversations, with your confirmation at each handoff.**

The phone is a convenient place to review and confirm that exchange. The central product is the bridge between conversations; remote access is the way you reach it.

## Release status

**[v0.1.0-alpha.1](https://github.com/geoffrey1111/agbrio/releases/tag/v0.1.0-alpha.1) — MIT-licensed Windows alpha.** Download the installer and checksums, or build the included source. Clean Windows installation and native first-setup controls have passed automated validation, separately from the real remote Codex/file handoff. App labels are Chinese; Mac and other agents remain future work. [Install and pair](docs/SETUP.md).

## For an existing two-conversation workflow

This is a workflow tool for people who deliberately separate management and execution contexts. You choose both conversations and their roles. Agbrio does not assign agents automatically or make the managing agent approve its own result.

**Instructions → execution. Results + evidence → management. You → final confirmation.**

## One conversation manages. One executes. Bridge connects the handoff.

![Managing and executing conversations side by side in one Bridge](docs/assets/desktop-bridge.png)

<p><img src="docs/assets/mobile-handoff.png" width="320" alt="Management-to-execution instruction selection"> <img src="docs/assets/mobile-return.png" width="320" alt="Execution-to-management result selection"></p>

The desktop view shows both sides of the same Bridge. The phone views show management-to-execution instruction selection and execution-to-management result selection. Real app components, fictional demo data, browser-rendered previews; not private conversations or device acceptance. App labels are currently Chinese. README translations do not imply a translated app.

## What the personal implementation does

- **Bridge** — Connect two existing conversations as control and execution sides. Read either side and hand work back in the opposite direction.
- **Selective handoffs** — Review the full source reply, select whole content blocks, edit the outgoing text and see the recipient before one final Confirm and send. Block recommendations are suggestions; you decide what leaves.
- **Supporting materials** — Selected Codex handoff files are copied into the destination project’s `.aiwr/incoming/<handoff-id>/` directory. The outgoing message includes the relative path and SHA-256. The recipient reads the local copy; this is not an upload of the file into a ChatGPT attachment slot.
- **Notifications and original replies** — Open a specific observed result and reply to that exact conversation. Public user and assistant messages share an oldest-to-newest timeline; older pages load above the current conversation.
- **Personal desktop Host + PWA** — The desktop Host owns bindings, observations and delivery records. The phone uses a paired web session. The computer must remain powered on, awake and connected for live reads or sends.

## A typical handoff

1. Create a Bridge and bind the exact control and execution conversations.
2. Read the control result, then select the instruction blocks and any supporting files.
3. Check the visible destination and press Confirm and send once.
4. After execution returns, review its result and send the selected material back to control.

## How the phone reaches your computer

![Agent Bridge topology](docs/assets/connection-flow.svg)

[Install and pair](docs/SETUP.md) · [Build from source](docs/BUILD.md) · [MIT license](LICENSE)

Choose Cloudflare Tunnel, Tailscale Funnel, private Tailscale Serve or your existing HTTPS entry in desktop Settings → Devices. The app verifies TLS and this exact Host before saving, and can copy a deployment prompt for your agent. The domain-free options are Funnel/Serve; you supply your own provider account. Tailscale account provisioning still needs independent real-account validation. No Agbrio-hosted relay is offered.

## Control, data and delivery

- Destinations are bound by exact conversation/thread identifiers, not conversation titles or screen positions.
- Every handoff needs an explicit user confirmation. Agbrio does not create an autonomous manager/executor loop.
- If delivery is uncertain, the record remains uncertain and is checked before another attempt; the tool does not blindly resend.
- Selected file bytes and hashes matter. A displayed source-computer path alone does not make a file available to a remote recipient.
- Self-hosting does not remove trust in your chosen tunnel/network provider or the agent provider. Phone caches and locally retained messages should be treated as private data.

## Current boundaries

Windows x64 + Codex + phone PWA is the first alpha scope. App labels are currently Chinese. Other agents, macOS and ChatGPT browser workflows are not supported-release claims. Physical iPhone fixes reported by the owner are distinct from new-machine installer validation.

## Build and contribute

Start with [SETUP](docs/SETUP.md) or [BUILD](docs/BUILD.md). Keep exact routing, explicit confirmation and uncertain-send recovery intact. The project is [MIT licensed](LICENSE); [third-party notices](THIRD_PARTY_NOTICES.md) retain dependency licenses.

## Contact and feedback

[Author](mailto:geoffreyzjx@qq.com) · [GitHub](https://github.com/geoffrey1111) · [Report a problem](https://github.com/geoffrey1111/agbrio/issues/new?template=bug_report.yml) · [Suggest a workflow improvement](https://github.com/geoffrey1111/agbrio/issues/new?template=feature_request.yml)

Questions are welcome in Issues. This is a public channel; use demo data and remove private material before posting.
