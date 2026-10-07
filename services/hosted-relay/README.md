# Optional Agbrio hosted relay

This is an operator service candidate, separate from the free self-hosted desktop.
No payment or marketplace integration. Cloudflare SQLite Durable Objects enforce
one-use redemption, device binding and 7/30/365-day entitlements. Clients receive
only their own host capability, never the operator or Cloudflare account token.

Deploy `worker.mjs` with `wrangler.jsonc`, set a high-entropy `ADMIN_TOKEN` as a
secret, and replace example zone/control host values. Keep the paid/free plan
limits explicit; this repository does not promise unlimited free relay capacity.
Attach the control hostname as a Worker custom domain. Generate a private batch
using `scripts/hosted-code-batch.py`, register each exact tenant hostname as a
custom domain for this Worker, issue the hashes and run the tool's `ready` check.
Do not use a wildcard route over unrelated applications or reuse a personal tunnel.
If BIC rejects machine clients, skip only BIC on the exact new service hostnames.

Build Windows with `AGBRIO_HOSTED_CONTROL_ORIGIN` set to the verified HTTPS control
origin. The desktop Settings → Devices panel then offers code redemption. The
bundled Node connector targets only this Host's loopback listener. Phone pairing
remains required; each tenant has its own browser origin and host-only cookies.
Changing back to a self-hosted entrance stops the connector while retaining the
subscription. A fresh code renews the same device/tenant; reusing an older code
does not add time. Keep the device identity when moving/reinstalling this private
installation; do not copy it to a friend's computer. Device transfer/reset is not
a V0 self-service capability.

Operator CLI takes a private `operator.json` containing `controlOrigin`, `zoneRoot`
and `adminToken`. `generate` writes a private JSON batch without printing codes;
`issue` sends hashes only; `ready` verifies real TLS and service identity; `status`
prints counts; `revoke` withdraws a code/subscription; `expire` ends a subscription
now. These operations are authenticated and never bundled in the recipient app.
Deliver only the code strings and recipient installer. Do not distribute the
operator configuration, complete issuance manifest or any desktop runtime data.

TLS protects both network legs. V0 does not implement E2EE against the operator
or Cloudflare. Relay bodies are transient memory, not persisted transcripts.
Entitlement metadata/hash receipts persist. Requests are bounded and never replayed
after timeout/disconnect. The computer must stay online. Mac, transfer, account UI,
payment, automatic DNS provisioning and large-scale capacity are separate scopes.
