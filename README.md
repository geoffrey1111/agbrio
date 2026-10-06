<img src="docs/assets/agbrio-icon.png" width="72" alt="Agbrio">

# Agbrio

**Agent Bridge — connect your agent conversations.**

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

Agbrio is a personal desktop-and-phone tool for developers who work across existing agent conversations. Keep your computer running, read results on your phone, choose what the next conversation should receive, and confirm the handoff once.

For example, one Codex conversation can plan and review while another implements. Agbrio keeps the two destinations visible and helps you carry the selected instructions and supporting files between them.

## Release status

**Open-source release preparation.** The personal Windows/Codex implementation is in use, and the owner has reported successful phone handoffs. This public repository currently contains the introduction, illustrations and a self-hosting plan. It does **not** yet contain the application source, an installer, a working installation command or a selected open-source license.

## A desktop workbench, a phone companion

![Agbrio desktop reader](docs/assets/desktop-reader.png)

<p><img src="docs/assets/mobile-handoff.png" width="320" alt="Agbrio handoff selection with a selected instruction block"> <img src="docs/assets/mobile-conversation.png" width="320" alt="Agbrio original conversation and reply composer"></p>

Real application components with fictional demo conversations. These are browser-rendered previews, not screenshots of private work or proof of iPhone/macOS acceptance. The application controls currently use Chinese; these README translations do not imply a translated app.

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

![Agbrio conceptual network and handoff diagram](docs/assets/connection-flow.svg)

Conceptual topology: the phone controls the desktop Host; the Host connects the two agent conversations. “HTTPS entry” denotes your selected network provider, not an Agbrio-hosted account service.

The current personal setup uses an HTTPS entry through Cloudflare Tunnel. Cloudflare supplies the network path; your desktop Host still handles the application’s state and requests. A cached PWA shell can open quickly, but cached content is not proof that the computer is online.

The proposed self-hosted setup will let people choose their own access method. The choices below are a plan, not a working selector in the current application.

- **Cloudflare Tunnel** — For an existing Cloudflare-managed domain.
- **Tailscale Funnel** — A proposed public HTTPS entry on a provider hostname, without buying a domain.
- **Tailscale Serve** — A proposed private network entry; both devices would use Tailscale.
- **Your HTTPS proxy or tunnel** — A proposed advanced option for an existing remote-access setup.

[Self-hosting design and draft agent prompt (Simplified Chinese)](docs/SELF_HOSTING_PLAN.zh-CN.md)

## Control, data and delivery

- Destinations are bound by exact conversation/thread identifiers, not conversation titles or screen positions.
- Every handoff needs an explicit user confirmation. Agbrio does not create an autonomous manager/executor loop.
- If delivery is uncertain, the record remains uncertain and is checked before another attempt; the tool does not blindly resend.
- Selected file bytes and hashes matter. A displayed source-computer path alone does not make a file available to a remote recipient.
- Self-hosting does not remove trust in your chosen tunnel/network provider or the agent provider. Phone caches and locally retained messages should be treated as private data.

## Current boundaries

Windows + Codex is the current personal-use path. The ChatGPT integration remains experimental and needs separate account/session and end-to-end verification. Claude Code, other agents, macOS and provider-independent onboarding are future work, not supported-release claims. iPhone PWA layout/rendering issues are still being investigated. No hosted multi-user service is offered.

## Before a usable open-source release

- Export a clean source tree and decide the license after a dependency/distribution audit.
- Implement access-method selection, verification and an agent-readable deployment prompt.
- Verify installation, pairing, recovery and file handoffs on a new Windows machine; validate Mac separately.
- Add other agents only through explicit adapters with independently tested read/send behavior.

Suggestions are welcome in [Issues](https://github.com/geoffrey1111/agbrio/issues). For bug reports, include the interface revision, device/OS, connection method and reproduction steps. Remove private conversation text, paths, tokens and screenshots before posting.
