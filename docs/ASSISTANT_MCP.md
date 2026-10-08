# Agbrio assistant MCP — whole application

Interface21 adds conversation-reviewed whole-app connections.
Public preview.16 is unchanged. One connection covers current/future Bridges,
watches, notification inbox and original-chat follow-ups. Exact IDs/revisions
still own operations; old BRIDGE/BRIEF_RULES grants are never silently upgraded.

## Owner setup

1. Desktop Settings → AI assistant → Add assistant → Connect assistant. Give it
   a name, normally Dot. No mandatory Brief/Insight or rule matching. This owner
   connection lasts30days and is revocable; no grant is auto-created.
2. In ChatGPT, follow the account's current custom MCP plugin flow: Plugins → +
   → Add custom MCP server, enter your Agbrio MCP URL, choose OAuth, create and
   install the private plugin. Exact UI availability depends on the account.
   Dot uses supported installed/enabled plugins; a URL pasted in a chat does not
   grant it access. DCR/PKCE are supported by this Agbrio server. Do not reconnect
   the retired AI Work Router M0 plugin.
3. Complete the real owner sign-in/pairing and Allow and connect screen. It
   identifies the callback client and the whole Agbrio connection. OAuth scopes
   must match; anonymous `/mcp` returning401/invalid_token is expected.
4. Tell Dot decision instructions directly in your conversation. It reads the
   actual returned result and materials, identifies routine forwarding versus
   an explicit/conflicting/unresolved owner decision, asks you when needed, then
   records the actual answer before executing. Source messages do not create new
   authority. Agbrio provides tools, not a model runner or automatic Dot wakeup.

Official references checked2026-10-08: [custom MCP](https://developers.openai.com/api/docs/guides/custom-mcp-server),
[plugin installation](https://developers.openai.com/plugins/quickstart),
[Dot apps](https://learn.chatgpt.com/docs/dots/computers-and-apps).

## Tools and execution

INSTANCE has16 tools; BRIDGE retains its existing8 and cannot call the new8.

| Tool | Use |
| --- | --- |
| agbrio_read_app | Global Bridges/revisions, watches, unread/read inbox, reversible removal metadata, approvalMode and pending actions |
| agbrio_list_conversations | Existing native conversation IDs for exact selection |
| agbrio_read_chat | Watched native public history, intermediate progress, status and requests |
| agbrio_read_bridge | Explicit workstreamId, both sides, results and decisions |
| agbrio_read_source | Explicit workstreamId/role/observationId, blocks/material IDs |
| agbrio_prepare_handoff | Reviewed text/materials, explicit workstreamId/bindingRevision/role, exact recipient/hash |
| agbrio_edit_handoff | READY edit; old hash/decision applicability invalidated |
| agbrio_request_decision / agbrio_record_answer | Handoff question and actual assistant-attested one-off owner answer/reference |
| agbrio_confirm_and_send | Approve/send once under conversation review, legacy brief rule or answered question |
| agbrio_receipt | Exact delivery/provenance, without re-sending |
| agbrio_prepare_action | Persist and return exact global operation/input/hash; stable requestId on retry |
| agbrio_ask_action_decision / agbrio_answer_action | Ask about an exact pending operation, then attest the actual answer/reference |
| agbrio_execute_action | Claim once under brief rule or answered question; return existing receipt on repeated call |
| agbrio_action_receipt | Recorded outcome/uncertainty/decision basis, without execution |

Global operations reuse existing application services: CREATE_BRIDGE,
RENAME_BRIDGE, BIND_BRIDGE, BRIDGE_LIFECYCLE(ACTIVE/TRASHED), PIN_BRIDGE,
ENABLE_WATCH, PAUSE_WATCH, REMOVE_WATCH_ITEM(reversible), MARK_NOTIFICATION_READ,
SEND_CHAT, STOP_CHAT and RESPOND_CHAT. Bind/lifecycle/name/pin mutations carry exact
workstreamId/bindingRevision; watch changes carry generation; original reply/stop/
request operations retain existing exact native-turn and request constraints.
File IDs must already be source-bound or uploaded through the existing owner UI.
No arbitrary filesystem path, shell, credential, grant/rule editing, permanent
purge or Codex/Cloudflare process restart tool is exposed.

Prepare → inspect input/hash → execute. No second PWA confirmation is required
after a new CONVERSATION_REVIEW assistant has reviewed a routine operation, or
when the owner actually answered its question. Asking a question blocks both rule-based and conversation-review bypass. APPLIED means the application
operation was accepted; underlying reply/turn status remains separate. EXECUTING
or UNKNOWN means inspect the record, never repeat with a new requestId. SOURCE,
binding, payload/material and grant checks still run at handoff physical claims.
Delayed direct QUEUE dispatch checks instance authority again inside its own writer
claim transaction, so revocation before dispatch prevents sending.

Conversation interpretation and conversational answer truth are the delegated assistant's
responsibility. Stored answers are assistant-attested, not independently verified
user input. Client/grant labels do not authenticate a Dot identity. Source messages
and files cannot create authority; an execution agent must not approve its own work.

For new CONVERSATION_REVIEW connections, routine `agbrio_confirm_and_send` uses
`ruleId:null,decisionId:null,assessment:<review reasoning>`. Global execute uses
`ruleId:null,useOwnerAnswer:false`. A previously asked question still requires its
actual answered decisionId / useOwnerAnswer:true. Audit basis is ASSISTANT_REVIEW
or ASSISTANT_ATTESTED_OWNER_ANSWER. Existing BRIEF_RULES grants keep the rule gate.
Selecting an older valuable result is allowed: the draft pins that exact result
and the source head visible at review. A subsequently arriving head invalidates
an unattempted send. Selected attachment versions must still pass integrity checks.

## Transport and validation limits

Existing stateless Streamable HTTP JSON/PKCE/exact resource audience/paired-owner
consent remain. Token fingerprints and immutable grants persist; temporary auth
intents/codes do not. Access tokens do not authenticate PWA/admin endpoints.
Migration014 rebuilds the grant dependency tables transactionally, retaining every
old record with BRIDGE scope; grants are not upgraded or synthesized. Action claims,
questions, answers and outcomes persist separately. A crash during execution stays
an attempted/uncertain record and does not replay. No refresh token is advertised.

Proofs include old-schema grant/token/answer/approval retention and FK integrity;
multiple/new Bridges and exact two-target mock handoffs; whole-app OAuth/PKCE scope
selection and legacy rejection; global watch removal/restoration, seen inbox and
original reply once; official MCP SDK1.32.1 sixteen-tool/decision/resume/replay proof.
Browser phone440/desktop1920 consent and notification visibility use isolated,
explicit disposable owner HTTP headers on a loopback fake-provider fixture, not
real browser account cookies. These are automated validation, not Dot or iPhone UAT.
Prior real shared-Codex attachment/duplicate proof is retained; new global scope
has not been exercised through a live Dot account. The approved narrow Cloudflare
rule is now applied on the existing personal entry, with discovery200 / anonymous
POST401 verified; this is not a live assistant OAuth/account acceptance result.
ASSISTANT_MCP_CLOUDFLARE.md retains the earlier login/rule preparation history.

Migration016 is additive and preserves old brief-rule authority, token IDs and decision/approval records. The new account-level Dot connection has not yet been accepted by the owner; isolated OAuth/HTTP/native/browser proofs are not live Dot UAT.
