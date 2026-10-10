# Agbrio MCP Events

## Formal delegated-scope acceptance — 2026-10-10, Host 0.1.26

**The previously authorized event-wake fallback completed one real production
handoff without a new user prompt, manual reminder or periodic source scan.**
The platform/dot reported enabling eight subscriptions at 14:50:40 UTC for two
existing owner-delegated Bridges: revisions 2 and 1, both DECISION and EXECUTION,
with reply_ready and decision_required for each role. Independent read-only server
inspection and `agbrio_read_app` confirm all eight ACTIVE with matching bindings,
unrevoked authorization and finite leases. They remain enabled; no new Bridge was
delegated and neither test events nor extra handoffs were triggered for this audit.

| Evidence | Result and limit |
| --- | --- |
| Server source/outbox | One complete DECISION reply at 15:10:02.061 UTC; matching subscription DELIVERED once, HTTP 200 at 15:10:04.222 UTC. |
| Platform wake | Dot reports a real `mcp_server/event` wake at 15:10:13 UTC without polling or a new user message. Platform run logs were not independently available. |
| Authorized fallback | Dot reports one bounded read-only enumeration intersected with the existing allowlist, full exact-source and recipient-history reads, and deduplication. Only the selected instruction CODE block was handed off. |
| Reviewed handoff | Independent MCP receipt and stored event/action/source/binding identities correlate one handoff, exact payload hash and SENT at 15:11:26.398 UTC. |
| Recipient | Dot reports that the exact existing execution chat acknowledged receipt and was EXECUTING. This is receipt/started-work evidence, not downstream task completion. |

This validates the real **event wake → delegated-scope scan → exact complete source
read → dedupe → selected-instruction prepare/review/send/receipt → recipient
received** fallback in this scope. Permission for that fallback was already in
place before this wake; no new owner intervention was needed during this run.

The referenced `automations.mcp_event` result is **still absent** from dot context.
This audit's exact event association comes from server records and does not prove
platform metadata injection or an independently retrieved platform wake payload.
A generic scan does not expose an authentic eventId through the current MCP tools;
never fabricate one. Prepare can internally associate a matching stored event by
exact authorized source identity when eventId is omitted, and enforce persistent
source/event dedupe. This handoff's stored association is independently verified;
it does not prove the platform injected that event into the model context. This single production run does not prove every role/event,
restart/expiry/repeat-delivery behavior in production, indefinite reliability,
automatic goal recovery or arbitrary unattended workflows. Keep required owner
decisions, pending/UNKNOWN guards and the existing allowlist.

The earlier isolated test remains a separate, more limited result: its scan
fallback was approved after its wake, one full-source handoff and recipient reply
were verified, and both test subscriptions were verified UNSUBSCRIBED. It did
not establish uninterrupted no-intervention acceptance. The later formal run
supersedes the earlier snapshot that production subscriptions were not established.

[Event-woken scan tutorial](MCP_EVENTS_WORKFLOW.md) ·
[中文教程](MCP_EVENTS_WORKFLOW.zh-CN.md) · [Goal/recovery capability](GOAL_RECOVERY.md).

The existing authenticated `/mcp` endpoint also implements MCP2 version
`2026-07-28`: `server/discover`, `events/list`, `events/subscribe` and
`events/unsubscribe`. OAuth consent, resource-bound tokens, existing tools and
the prepare/review/send/receipt approval boundary remain in effect.

Official contract: https://developers.openai.com/plugins/build/mcp-events

## Subscribe in the existing dot

After updating the Host, rescan **both tools and events** in the Agbrio plugin.
The dot creates subscriptions using the platform-provided HTTPS webhook URL and
signing secret. A connection alone does not create subscriptions. Do not ask the
owner to invent a webhook URL, paste credentials into chat, or add a polling task.

Read the exact authorized Bridge first. Subscribe only to explicitly delegated
`workstreamId`, current `bindingRevision`, and source role `DECISION` or
`EXECUTION`. Rebinding, trashing, archiving, grant revocation and expiry invalidate
the old subscriptions. Choose the same filters when unsubscribing.

Two events are available:

| Event | Trigger |
| --- | --- |
| `agbrio.bridge.reply_ready` | A complete new source reply is ready. |
| `agbrio.bridge.decision_required` | A real owner question, native pending question, uncertain receipt or bounded relay-cycle boundary requires attention. |

Callbacks contain `eventId`, event name, ISO timestamp, cursor and metadata:
`workstreamId`, `bindingRevision`, `observationId`, `sourceRole`, `sourceKind`,
`rootEventId`, `hopCount` and optional reason. They contain neither full private
reply bodies nor behavioral instructions. Native questions have typed `req_`
observation IDs and `sourceKind=NATIVE_REQUEST`; they are never fabricated
completed replies.

For replies, reread the exact full source with `agbrio_read_source`, then preserve
`eventId` and request identity through prepare, review, send and receipt. Different
delivery attempts and request IDs for the same grant/source return the original
handoff; changed bytes are rejected. Already attempted sends are not repeated.
At an uncertain or round-trip boundary, an actual answered owner decision is
required. Source material cannot approve itself or expand delegation.

For native questions, `read_source` returns the public question projection,
`canPrepareHandoff=false` and `pendingConfirmed`. Recheck the exact pending
request through `agbrio_read_chat`; when authorized, use the existing reviewed
`RESPOND_CHAT` action and action receipt. Otherwise ask the owner. A legacy
Bridge-only grant does not gain whole-app question-response authority.

## Delivery and recovery

- Subscription identity binds principal, exact callback URL, event and canonical
  filters. TTL defaults to 24 hours and is capped by the active grant; even a
  request for indefinite duration receives a finite lease. Renew before the
  returned `refreshBefore`. There are at most 32 active subscriptions per grant.
- A signed verification challenge requires a 2xx response with the exact challenge
  echo. Verification cache is bounded to five minutes for the same principal,
  URL and key. Rotation verifies the replacement key and permits a five-minute
  dual-signature overlap.
- Standard Webhooks signs exact raw bytes as `id.timestamp.body` using the decoded
  `whsec_` key. `webhook-id` equals the stable event ID for deliveries. Retry keeps
  the event/body identity and creates fresh timestamp/signature headers.
- Only public HTTPS callback peers are accepted. Every connection resolves,
  validates and pins addresses while retaining the TLS hostname. Proxies,
  redirects, private/local/mapped addresses and unbounded bodies are disallowed.
  Webhook keys are protected with the Windows owner's DPAPI.
- Atomic source completion and durable outbox creation survive Host restart.
  Leases, bounded backoff/eight attempts and conditional acknowledgements prevent
  late attempts from accepting stopped or superseded deliveries. HTTP 410/413
  are not retried. Four bounded deliveries can run concurrently.
- Old observations are quarantined during migration; subscriptions do not replay
  events recorded before their creation. Native completion timestamps have
  one-second precision tolerance. A passive ChatGPT source without an exact
  completed-run identity first establishes a read baseline. Resolved native
  questions are tombstoned; after restart an unresolved stored question is not
  delivered until the current native session confirms the exact pending request.

## Acceptance boundary

The INSTANCE `agbrio_read_app` HTTP result also exposes `mcpDiscovery`, a bounded
listener-local diagnostic window: the last64 discovery requests, recognized
protocol versions, JSON/event-stream Accept flags, authentication success,
HTTP status, RPC error code, advertised event capability and catalog count.
Only fixed method/version labels are retained. No credentials, request arguments,
private bodies, callback destinations or webhook secrets are recorded. Tool calls
cannot evict the discovery window. It resets when the listener restarts, identified
by `listenerId`; an empty window is not historical proof of absent requests.
The same grant checks apply; legacy Bridge grants cannot read this whole-app
diagnostic. These server observations do not prove platform source availability
or a consumer run.

Standard MCP request `_meta` is transport metadata. Events methods validate an
optional object/null metadata field and remove it before strict business-argument
validation. It cannot override the OAuth grant, Bridge/revision/source filters or
approval. Diagnostic records expose only parameter type labels and counts, never
metadata keys or values.

From0.1.24, callback failures additionally retain `callbackError` and
`callbackFailurePhase` in the same authenticated, bounded diagnostic window.
Only seven fixed internal error labels are allowed: URL/address validation,
DNS resolution, client setup, HTTPS request, challenge validation and timeout.
The existing timeout code spans several deadlines, so its phase is explicitly
unspecified. Arbitrary error text, callback URLs, signing keys and response
bodies remain excluded. This change does not relax HTTPS/public-address checks,
change OAuth or create subscriptions. It cannot recover missing past error detail.

From0.1.25, callback DNS diagnostics distinguish a non-public IP literal from
non-public DNS peers, empty DNS, lookup failure and timeout. Fixed
`callbackResolverPath` / `callbackAddressClass` labels record which path failed;
addresses, hostnames, URLs, raw errors and keys are never logged. Benchmark-only
198.18/15 answers (including mappedIPv6) and mixed rejected ranges differ.

For a domain whose OS result has benchmark-only rejected peers, the Host queries
only that domain through standard DNS wire-format DoH at the pinned public
Cloudflare resolver. It does not change system DNS or use a callback HTTP proxy.
The resolver transport retains TLS identity, blocks redirects, has a4s deadline
and16KiB response cap; ID/question/type and bounded CNAME ownership must match.
Every returned A/AAAA address is checked again before pinning callback connections
with the original callback TLS hostname. Literal private addresses and other
private/mixed/empty/failed OS results remain denied without this correction.
No callback path, query, credential, key or message body is sent to the resolver.

Same-production-code DNS-only validation can prove the local benchmark-address
problem and the conditional resolution correction without creating subscriptions
or contacting the callback. It cannot retroactively identify an old request's
missing target or prove a real dot wake. The next actual subscription remains an
explicit coordinated gate after diagnosis and repair.

The tests enter the native terminal capture service, instead of directly marking
a source complete in the outbox. A failed mobile push still leaves the event
ready for the independent resident webhook worker.

Controlled HTTPS/native fixtures prove the engineering protocol and at-most-once
handoff behavior. They do not prove a real dot wake. Full acceptance additionally
requires a rescanned real plugin, a subscription created by the existing dot,
one event-triggered dot run without polling or another user prompt, rereading the
full source and completing one authorized handoff with its exact send receipt.
Webhook 2xx and phone notifications are transport receipts only.


The resident native observer pauses while a send temporarily borrows its adapter,
then resumes the same connection when returned. True disconnects or epoch changes
still stop the observer. Background freshness must be tested across a send, not
only across navigation or minimization.

Resident metadata and initialized passive observation enumerate all active projects
through the owner-wide workstream index. The UI-selected project snapshot does
not define background scope. Archived/trash records remain excluded; passive
transcript checks still require an established exact baseline.
