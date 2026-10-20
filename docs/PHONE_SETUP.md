# Phone entrance and pairing reference

Start with the [complete setup guide](SETUP.md) ([简体中文](SETUP.zh-CN.md)).
This is the optional deployment reference, not a prerequisite for desktop use.
Never publish credentials, pairing codes or private screenshots.

## Choose an HTTPS entrance

### Optional hosted connection

Public 0.1.34, 0.1.35-rc.5 and 0.1.35-rc.19 include the optional redemption flow. In
**Settings → Devices**, enter a code supplied separately by the author and
activate once on your desktop. The packaged service is
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

## Pair the phone

Open the saved `/mobile` URL on your iPhone. Generate a one-use five-minute
pairing code in Devices; enter it on the phone. Add to Home Screen from Safari.
You can revoke the phone from Devices, or replace an exact old login during a new pairing. Remembered sessions last up to 90 days
and renew on use; a cache does not mean the desktop is online.


Continue with [your first Bridge and handoff](SETUP.md#5-create-a-bridge-and-send-one-handoff).
