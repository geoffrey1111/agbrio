# Antigravity native CLI integration

## Owner-selected transport

The owner selected official agy CLI with their own Google login on 2026-11-25,
superseding the earlier Desktop attachment plan. Agbrio acts as a native ACP
client over stdio. It does not extract login credentials, proxy a model API,
automate account switching, install Desktop, or require Desktop to run.
The owner completed native Agbrio MCP OAuth themselves.

The official ACP documentation supports CLI clients; the FAQ separately limits
consumer-plan headless wrappers. Native protocol compatibility is not a promise
that Google endorses every automated delegation under a personal subscription.
Agbrio retains native login, model selection, permissions and denials.
References: https://antigravity.google/docs/cli/acp/ and
https://antigravity.google/docs/faq .

## Shared lifecycle and identity

One lazy `agy acp --stdio --disable-telemetry` connection serves all watched
sessions. No process is started per page or poll. Resolve the installed native
executable through an explicit exact path or the owner's installation PATH;
never download or authenticate while discovering conversations.

Native `sessionId` is authoritative. Agbrio watch IDs are
`antigravity:<nativeSessionId>`. List and load use native ACP, never title
matching, transcript copying or lock stealing. Read-only restoration uses
`session/load`, which replays history without repeating a prompt. Successful
replay atomically replaces the observed projection; incomplete/oversized replay
cannot replace it with a truncated success. Display message IDs and control
turn tokens are labelled adapter observations, not native message/turn IDs.

## Capabilities implemented in source

| Operation | Native/shared path | Boundary |
| --- | --- | --- |
| Discover existing conversations | ACP session/list | Bounded pages, actual capability required |
| Restore and read history | session/load plus session/update | Exact native identity/project; no prompt replay |
| New conversation | Main + -> platform/project/model/effort/first instruction | Owner creation journal checkpoints native identity before first send |
| Model and reasoning selection | Native `agy models`, session/set_model | Exact native variant encodes reasoning; no invented effort or fallback |
| Title updates | session_info_update | Native title only; no extra naming model call |
| Streaming public text | user/agent_message_chunk | Bounded chronological observation projection |
| Reviewed Codex/Claude -> agy messages | prepare_action/execute_action SEND_CHAT | Existing INSTANCE grant, original hash/ID, physical claim rechecks revocation |
| agy -> Codex/Claude messages | agy's already-authorized Agbrio MCP | Same reviewed operations and recipient-native receipts |
| Busy follow-up | Explicit QUEUE/queueRequested:true | FIFO; no implicit queue or simulated STEER |
| Selected attachments | Existing hashed attachment cache and local file references | Native file/tool permissions remain authoritative |
| Stop | Exact adapter control token -> session/cancel | Request receipt is not completion confirmation |
| Native tool permission | session/request_permission -> offered once/reject option | No persistent grant, autoapproval or permission bypass |
| Disconnect | Durable UNKNOWN receipt | No retry, new ID, copied prompt or takeover |
| Management | Existing Codex/Claude manager may add a watched agy executor | No automatic membership edits; agy is not a native management-notice receiver |

Native subagent graphs, arbitrary artifacts, persistent permission changes,
Desktop coexistence and mid-turn steering are not advertised as available.
The CLI direction replaces the previous Desktop transport target. The conversation
capability inventory remains in scope; unsupported native controls stay unavailable
rather than being emulated through a shell.
The shared process remains resident once used; per-session idle memory release
and physical memory measurements have not been accepted.

## MCP usage

1. Read `agbrio_capabilities` and exact `agbrio_read_chat` before sending.
2. `agbrio_list_agent_models({provider:"ANTIGRAVITY"})` reads native variants.
3. Reviewed `CREATE_CHAT` with provider ANTIGRAVITY creates an empty native
   session, returns exact watch generation and nextActionInput; it sends no task.
4. Add the delegated text to nextActionInput and use the normal reviewed
   SEND_CHAT flow. Busy follow-up requires mode QUEUE and queueRequested true.
5. Read original action_receipt.delivery until SENT/FAILED/UNKNOWN. APPLIED
   means acceptance. SENT confirms the native prompt result, not task success.
   UNKNOWN stops later dispatch and never automatically resumes after reconnect.

The owner New conversation form sends the first instruction through the same
adapter under a separate durable owner creation receipt. Codex and Antigravity
support owner creation. Claude Desktop creation is not supported; existing
Claude conversations retain plugin-based bidirectional text messaging.

## Verification boundary

Local fictional ACP subprocess tests cover exact one-send identity, explicit FIFO
queue, native permission choices, disconnect UNKNOWN and grouped history replay.
An MCP integration fixture also checks reviewed action provenance and revocation
before a delayed native offer. No real model quota is used by these tests.
Native CLI installation/login/MCP discovery and model catalogs were observed;
real model round trips, real title changes and physical-phone acceptance remain
separate acceptance steps requiring the owner's test-quota permission.


## Real-provider acceptance boundary

A real Codex creation and reply passed. One real Antigravity attempt created a
native session, updated its native title and reached an ACP permission request.
That request was not approved, so a completed model/MCP round trip is not yet
established. Do not replay an uncertain original attempt. Independent Claude CLI
login is not required for existing Claude Desktop plugin-based messaging.


## Owner default permission mode

The current-user owner CLI exposes `AntigravityPermissions` and
`SetAntigravityPermissions -PermissionMode FULL_ACCESS` (or `ASK`).
The default for installations without an explicit owner choice is ASK.
An owner choice is persisted across Host restarts; all subsequent native ACP
processes inherit it, so it is not a per-conversation toggle. FULL_ACCESS uses
agy's native `--dangerously-skip-permissions`, not synthetic approval replies.
This is local tool approval, not Windows elevation or an Agbrio MCP grant.
Only the owner control surface changes this setting; assistant actions cannot.
Changing it never restarts a running native process. The read response reports
`defaultMode`, `runningMode` and `pendingNextLaunch`; an already-running process
keeps its original mode until its next launch. Finish active tasks before an
owner-chosen restart. Default changes do not resend any original task.

The desktop Settings > Connections > Antigravity (AGY) panel exposes the same
owner setting, its saved default, actual running mode and pending-next-launch
state. Saving never restarts AGY. On an uncertain save, refresh reads the actual
preference before another write is permitted.

## Titles, progress and client discovery

Reviewed CREATE_CHAT label is applied to the watch before the first instruction;
native session_info_update titles may subsequently replace that initial label.
read_chat.state.progress reports lastNativeEventAt, lastPublicTextAt, activeTools,
latestToolStatus, promptPending and transportConnected. No reasoning or tool
arguments are exposed. A fresh connection observation is not model progress;
silence never confirms failure and does not cause automatic retry or stop.
SENDING remains pending until the exact native session/prompt result arrives.

agbrio_capabilities.tools and MCP tools/list share one scoped registry, including
agbrio_list_agent_models for INSTANCE/OWNER (not legacy BRIDGE scope). If Claude
still lacks a tool returned by that registry, refresh its MCP discovery when safe;
this does not require an Agbrio Host restart, OAuth reset or replay of any action.
The server cannot guarantee when a third-party client refreshes cached schemas.

## Explicit reviewed management membership

A shared INSTANCE grant does not attest which native manager called CREATE_CHAT,
so Agbrio does not guess a creator zone. After creating a chat, read management
with read_app FULL/MANAGEMENT, then prepare/review/execute ENABLE_WATCH with:

```json
{"threadId":"antigravity:<exact-native-id>","management":{"zoneId":"<exact-zone-id>","revision":53,"managerId":"claude_code:<exact-manager-id>","generation":1}}
```

Use actual revision and generation, never the example values. This reuses the
existing operation and permissive input object for cached clients, not a new
tool name or operation enum. Only APPLIED CREATE_CHAT results from the same
active grant are eligible. The atomic write rechecks grant/review provenance,
manager/revision, source generation and existing membership; no transfer, manager
appointment, permission changes or native message submission is performed.
Read the original action receipt after uncertainty; never repeat UNKNOWN.

AGY management TURN_RESULT uses the existing passive notice pipeline and exact
managed:acp-agent:<original-reply-id> only after that original receipt is SENT.
The exact subscription/turn/state notice identity deduplicates repeated scans;
old UNKNOWN notice claims stay fenced. Completed joined chats can be discovered
once on a later scan; this is metadata notification, not business task replay.
AGY sources are excluded from the native Codex thread poller.
