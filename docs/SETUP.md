# Install and pair

Agbrio connects the existing conversation that manages a project with the
conversation that executes it. Windows x64 + Codex + a phone PWA is the first
alpha target. You supply your agent login and remote-access provider account.
There is no Agbrio account server, subscription or bundled Codex installation.

## 1. Desktop

Use the Windows installer from [Releases](https://github.com/geoffrey1111/agbrio/releases).
Alpha installers are unsigned. Verify `SHA256SUMS.txt` against the downloaded file;
review the source before trusting a build. WebView2 must be available; the
installer uses Microsoft's bootstrapper if necessary. Do not install alongside
another running Agbrio/AI Work Router instance.

Open Agbrio. It stays in the system tray when the window closes. Quit from its
tray menu to stop the Host. Keep the computer awake for remote access.

## 2. Codex connection

Codex Desktop must already be installed and signed in. In Settings (设置) → Codex,
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

Settings (设置) → Devices (设备) offers four entry types. The HTTP Host binds only
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
paths with Cloudflare Access, make the read-only setup path
`/v1/mobile/connection/probe` reachable for verification or use provider-authorized
configuration. It exposes only an expiring nonce/Host-instance proof and gives
no data/write permission. A Quick Tunnel's random URL is for disposable trials,
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
You can revoke the phone from Devices. Remembered sessions last up to 90 days
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
