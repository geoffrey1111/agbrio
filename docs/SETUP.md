# Install and pair

Agbrio connects the existing conversation that manages a project with the
conversation that executes it. Windows x64 + Codex + a phone PWA is the first
alpha target. You supply your existing agent login. Phone access can use an optional
redemption-code hosted connection or your own HTTPS provider. Codex Desktop
is never bundled. Free self-deployment does not require a redemption code.

## 1. Desktop

Use the Windows installer from [Releases](https://github.com/geoffrey1111/agbrio/releases).
The0.1.8 package includes a publisher signature for in-app updating; it is not Windows Authenticode-signed. Verify `SHA256SUMS.txt` against the downloaded file;
review the source before trusting a build. WebView2 must be available; the
installer uses Microsoft's bootstrapper if necessary. Do not install alongside
another running Agbrio/AI Work Router instance.

Open Agbrio. It stays in the system tray when the window closes. Quit from its
tray menu to stop the Host. Keep the computer awake for remote access.

Choose **Settings → Language** for English, 简体中文, 繁體中文 or system default.
Desktop and phone keep their own choice. The pairing screen also offers this control.
Changing language keeps conversations, drafts and device login. Original messages
and outgoing instructions are not translated. Japanese/Korean currently describe
the product in the README only.

## 2. Codex connection

The Windows Store Codex Desktop must already be installed and signed in. Keep its
ordinary window open for the first preparation so Agbrio can identify its running
native backend. In Settings (设置) → Codex,
prepare the shared connection (准备共享连接). This checks the existing installation;
it does not install a second Desktop or a root certificate. Finish current agent
work, quit Desktop yourself, then choose Open shared Codex (打开共享 Codex) in
Agbrio. This starts the existing application with a session-specific local
endpoint. Its ordinary default launch remains unchanged. Once prepared, you do
not need to repeatedly open an ordinary Desktop first.

Windows Store/Desktop versions can change the external app-server interface.
Shared mode is an alpha compatibility integration, not an official OpenAI remote
access feature. If preparation/launch fails, retain the error state, disable
shared mode and use normal Desktop; do not blindly resend a handoff or kill
unrelated processes. Mac and other agent adapters are not validated targets.

## 3. Choose your phone entry

### Optional hosted connection (preview 16)

Install the **preview.16** Windows package for redemption support; alpha.1 does
not include it. In **Settings → Devices**, enter a code supplied separately by
the author and activate once on your desktop. The packaged service is
`https://agbrio-connect.geoffreygroup.cc`; you do not enter a domain, tunnel
command or connection method. The app verifies your isolated HTTPS entrance
before saving it. Keep Agbrio running and the computer awake, then pair your
phone using the displayed mobile address.

Codes have 7-, 30- or 365-day durations starting at first successful redemption.
A code binds to one desktop installation identity. A fresh code renews that
installation's existing entrance; re-entering a used code does not extend time.
Phones and tablets do not each redeem a code. The limit is **20 paired browser
sessions**, not 20 purchased licenses. For a reinstalled PWA, select **Replace**
and the exact old login when generating a pairing code. The old login is revoked
only after successful pairing. No automatic physical-device matching is claimed.
Desktop transfer/reset is not yet self-service; do not share app-data or your
installation key with another person.

Hosted codes are optional and not included in the public installer/source.
Ask the author at **geoffreyzjx@qq.com** for availability; this preview does not
implement checkout or promise unlimited hosted capacity. Expired/revoked access
stops the hosted entrance while local conversations remain. TLS protects network
connections; this relay is **not end-to-end encrypted against the operator or
Cloudflare**. See [hosted service source](../services/hosted-relay/README.md).

### Use your own entrance

Choose **Configure myself** for the four existing entry types. The HTTP Host binds only
to `127.0.0.1:47114` by default. Use the port displayed by your running instance.
Your proxy must preserve the browser's exact Host header and HTTPS origin.

| Entry | Domain required | Who can reach the entry |
| --- | --- | --- |
| Cloudflare Tunnel | Your Cloudflare domain | Public HTTPS; optional Access |
| Tailscale Funnel | No purchased domain | Public `.ts.net` HTTPS |
| Tailscale Serve | No purchased domain | Your private tailnet; phone needs Tailscale |
| Existing HTTPS | Depends on provider | Your proxy/tunnel's policy |

Configure the chosen provider using its official login and tooling. Do not paste
tokens, cookies or passwords into chats. For Tailscale, once its required account
permissions are enabled, the typical commands are:

```powershell
tailscale funnel --bg http://127.0.0.1:47114
# Or, for private access:
tailscale serve --bg http://127.0.0.1:47114
```

These are alternatives; do not run both for the same entry. Check Tailscale's
printed HTTPS address and current account/ACL permissions. Serve uses your
tailnet DNS/route, so both devices must be connected. The selector and validation
are implemented; a real Tailscale-account end-to-end test is not yet claimed.

For Cloudflare, create a named tunnel whose service is
`http://127.0.0.1:47114`, retaining the original Host header. If you protect all
paths with Cloudflare Access, make `/v1/mobile/connection/probe` and the read-only
`/v1/mobile/auth/session` reachable for verification/health checks. These give no
chat data or write permission; the probe additionally requires an expiring
owner-issued nonce. A Quick Tunnel's random URL is for disposable trials,
not a persistent PWA installation.

Paste the HTTPS root address into Devices and choose Verify and save
(验证并保存). Agbrio checks real TLS and that the entry reaches this exact Host
before storing the config. Failed verification retains the old connection.
Only Agbrio's own local HTTP listener changes; it does not restart your agent or
tunnel. A new origin requires new phone pairing and push registration there.

The Copy deployment instruction (复制部署指令) control produces an agent-readable
prompt for the selected provider and actual port. Your agent can configure the
provider, then submit the verified address with the source script:

```powershell
pwsh -NoProfile -File scripts/configure-mobile.ps1 `
  -Method TAILSCALE_FUNNEL -Origin https://your-computer.your-tailnet.ts.net
```

Run it as the same Windows user as Agbrio, with the current Host running. It
submits a single local setup request and waits for verification; it does not
weaken authentication or change the provider. No credentials are passed. Its
`-DataDirectory` option is for a deliberately selected alternate Host profile;
it does not change which directory a running Host owns.

## 4. Pair the phone

Open the saved `/mobile` URL on your iPhone. Generate a one-use five-minute
pairing code in Devices; enter it on the phone. Add to Home Screen from Safari.
You can revoke the phone from Devices, or replace an exact old login during a new pairing. Remembered sessions last up to 90 days
and renew on use; a cache does not mean the desktop is online.

Create a Bridge, select two different exact conversations, review the latest
source reply, choose instruction/result blocks and files, then Confirm and send
once. Selected Codex files are copied to the destination project's
`.aiwr/incoming/<handoff-id>/`, with relative paths and SHA-256 in the message.
This is a local project handoff, not a ChatGPT file-slot upload.

## Recovery and limits

- Wrong entry/TLS/Host: fix the provider and verify again. Old settings remain.
- App restarted: pairing and saved address persist; keep the original URL.
- Computer asleep/offline: phone may show cache; wait for a confirmed live state.
- Uncertain send: inspect the exact target record before another attempt.
- Disabling a watch/removing a Bridge is local; it does not delete provider chats.
- ChatGPT browser integration is experimental. Claude Code and macOS are future
  adapters; the README does not promise them as working.

References: [Tailscale Funnel](https://tailscale.com/docs/reference/tailscale-cli/funnel),
[Tailscale Serve](https://tailscale.com/docs/reference/tailscale-cli/serve),
[Cloudflare Quick Tunnels](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/).


## AI assistant connection

Desktop Settings → AI assistant offers a six-step human-operated guide, with cloud/local routes and copy-to-AI instructions. Use your actual instance MCP URL, reuse valid authorization and consent personally. Verify actual tools in the intended dot/client. [English](ASSISTANT_CONNECTION_GUIDE.en.md) · [简体中文](ASSISTANT_CONNECTION_GUIDE.zh-CN.md) · [繁體中文](ASSISTANT_CONNECTION_GUIDE.zh-TW.md).
