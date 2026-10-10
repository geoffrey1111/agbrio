# Agbrio MCP Events

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

The tests enter the native terminal capture service, instead of directly marking
a source complete in the outbox. A failed mobile push still leaves the event
ready for the independent resident webhook worker.

Controlled HTTPS/native fixtures prove the engineering protocol and at-most-once
handoff behavior. They do not prove a real dot wake. Full acceptance additionally
requires a rescanned real plugin, a subscription created by the existing dot,
one event-triggered dot run without polling or another user prompt, rereading the
full source and completing one authorized handoff with its exact send receipt.
Webhook 2xx and phone notifications are transport receipts only.
