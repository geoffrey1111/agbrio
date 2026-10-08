# Agbrio assistant MCP — whole application

0.1.7 adds the manual tutorial to the existing conversation-reviewed whole-app connection. One connection covers current/future Bridges,
watches, notification inbox and original-chat follow-ups. Exact IDs/revisions
still own operations; old BRIDGE/BRIEF_RULES grants are never silently upgraded.

## Human-operated setup

Settings → AI assistant has the six-step manual guide. Choose cloud ChatGPT/dot
or local Codex first. Reuse a valid current grant and instance URL, complete
owner consent, and verify actual tools and three reads in the target assistant.
Desktop ready, a ZIP upload or DCR201 is not cloud acceptance.

Manual guides: [English](ASSISTANT_CONNECTION_GUIDE.en.md),
[简体中文](ASSISTANT_CONNECTION_GUIDE.zh-CN.md),
[繁體中文](ASSISTANT_CONNECTION_GUIDE.zh-TW.md).

After connecting, ask which Bridges are delegated and agree exact tasks,
directions, ask/pause conditions. Whole-instance access does not delegate all
work. Preserve the existing prepare/confirm/receipt and decision-answer flow.
Owner reports dot16tools and3reads; live dot writes are not yet accepted.
Events discovery/subscription/wakeup remains unimplemented.

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

## 0.1.6 OAuth Verify repair

Codex Verify requests `agbrio:handoff agbrio:instance` together. The server now validates a scope set rather than one string enum. Owner consent selects one existing delegation and the token grants only its actual scope. Re-run Verify after updating and complete owner sign-in/Allow. Unknown scopes and wrong callback/resource/PKCE remain rejected. Successful registration alone is not full OAuth/account acceptance.
