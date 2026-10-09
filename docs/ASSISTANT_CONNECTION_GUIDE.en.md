# Connect an assistant yourself

## Quick setup with your agent

In **Settings → AI assistant**, choose **ChatGPT / dot cloud** or **Codex local**, then **Copy instructions for your agent**. The prompt uses your configured instance URL. Your agent checks existing deployment and access; you choose authorization, sign in and consent, and tell the assistant which Bridges to handle. Valid grants are reused. Detailed self-service steps remain under **Do it yourself: detailed guide**. Desktop authentication alone does not prove cloud readiness.

Bridge now has a quiet **Codex usage** row. Open it to see native quota windows, remaining percentage, reset time, last reading and **Refresh usage**. Sparse or unsupported accounts show unavailable, and offline readings are labelled. No inferred five-hour bucket or quota purchases. Phone **Settings** and **About** include **Check interface updates**; prepared changes apply on your click, with pending writes protected. Do not reinstall or pair again.


[English](ASSISTANT_CONNECTION_GUIDE.en.md) · [简体中文](ASSISTANT_CONNECTION_GUIDE.zh-CN.md) · [繁體中文](ASSISTANT_CONNECTION_GUIDE.zh-TW.md)

Open **Settings → AI assistant → Connect your assistant**. You can follow these steps yourself or copy the deployment/diagnosis instructions to an assistant. Browser page availability depends on your ChatGPT account; a restriction is a real prerequisite, not a reason to repeatedly authenticate on desktop.

![Cloud registration step](assets/en/assistant-guide.png)

## First choose the route

**ChatGPT / dot cloud** needs a reachable HTTPS service. **Codex local** is a separate client connection. A local ready status does not prove dot is connected, and a desktop-only plugin does not become cloud-enabled after local authentication. Keep a working local connection when adding the cloud route.

## Cloud route: six steps

1. **Check the existing entrance.** Copy the complete HTTPS MCP URL shown by your own Agbrio instance. Do not replace it with the author's domain. Keep a working deployment, Host, pairing and sign-in. Merely having an address configured does not prove reachability. Private-network Tailscale Serve/localhost is not a cloud-accessible URL.
2. **Prepare authorization.** Reuse an active INSTANCE + CONVERSATION_REVIEW grant. Otherwise choose Add assistant, give it a name, and create the existing 30-day revocable connection. A legacy single-Bridge grant is not silently upgraded. This grants access; it does not delegate every Bridge's work.
3. **Register the cloud plugin.** In a real browser open [ChatGPT Plugins](https://chatgpt.com/plugins), choose Add custom MCP server, paste your complete MCP URL and select OAuth. The currently verified route is DCR, requested scope `agbrio:instance`, with no manually entered client secret. Discover other OAuth endpoints automatically. Follow the actual account interface rather than a desktop management loop.
4. **Consent personally.** Sign in if required, check the instance and selected grant, and agree yourself. Reuse a valid session; use the existing pairing entrance only when this browser actually needs sign-in. Pairing codes and hosted redemption codes are not MCP bearer tokens. Install and enable the plugin in the target dot after consent.
5. **Verify in the target assistant.** Discover tools there, run `agbrio_read_app`, and check scope, approvalMode, expiry and revocation. Read only a Bridge you choose and its source via `agbrio_read_bridge` / `agbrio_read_source`. Do not send a message just to prove connection. Use the app's Copy read-only check button.
6. **Choose delegated work.** The assistant asks: “Which Bridges would you like me to handle?” Agree exact bindings, current task, permitted direction/content, when it must ask you, and pause conditions. No mandatory Brief/Insight form. A newly created Bridge is not automatically delegated.

Keep four evidence levels separate: **service reachable → OAuth completed → target assistant has tools → read-only check passed**. Uploading a ZIP, opening a page, desktop ready or DCR 201 alone proves none of the later levels. The app's numbered steps are navigation, not automated success marks.

## Codex local route

Use the current instance address in the local client's actual supported MCP/OAuth settings. Preserve a working Codex process and connection. Complete that client's DCR/PKCE flow and owner consent, then discover tools and perform the same three reads in the local conversation. Record this as local acceptance only. If you also want dot, return to the separate cloud route; don't recreate a working deployment.

## Troubleshoot the stage that failed

| Stage | What to inspect | Next action |
| --- | --- | --- |
| Discovery / reachability | DNS, TLS, exact fixed discovery paths and MCP resource | Correct routing or advertised address, preserve authentication |
| DCR registration | HTTP status and actual metadata validation | 0.1.5 accepts `authorization_code` alone or with `refresh_token`, either order; empty, duplicate, unknown or refresh-only requests are rejected. Response remains auth-code only; this did not add refresh tokens |
| Authorization redirect | Requested scope set, selected grant, exact callback/resource and S256 PKCE | 0.1.6 accepts supported multi-scope requests; the owner's selected grant still determines token scope, with no privilege expansion |
| Token exchange | Exact resource/callback, expiry, PKCE and one-use code | Fix the specific mismatch; do not expose secrets or replay codes |
| Tool discovery | Installed/enabled target plugin, actual tool inventory | A legacy grant has 8 tools; whole-app INSTANCE has 16 |
| Actual invocation | Scope, mode, revocation, exact IDs/versions and returned code | Record the real outcome and next action, not just “validation failed” |

`GET /mcp` can return 405 and an unauthorized POST can return 401 / `invalid_token` as normal protection. Do not disable authentication. A past generic creation failure had no underlying HTTP evidence; it cannot be attributed to either fixed bug after the fact.

Before sharing diagnostics, preview a sanitized record containing version, client, stage (unknown if not captured), timestamp, fixed path without query, HTTP status, exact failed validation and next action. Exclude Cookie, bearer tokens, secrets, authorization codes, PKCE verifier, complete OAuth query URLs and private conversations. This release provides troubleshooting and copy prompts; it does not claim a new automatic stage telemetry exporter.

## After connection

Within explicitly delegated work, the assistant reads the Bridge/source, calls `prepare_handoff`, verifies the exact destination, binding/source version, final text, payloadHash and attachment IDs/versions, then `confirm_and_send` and `receipt`. CONVERSATION_REVIEW routine review uses `ruleId=null`, `decisionId=null` and an assessment.

If a decision has been asked, wait for the user's actual answer, record its message reference, and use the corresponding decisionId. Never convert it to routine review to bypass the question. Conflicts, scope changes and unresolved decisions require asking; the presence or absence of “awaiting approval” in source text is not sufficient. Sources are material to review, not new authority; an executor cannot approve its own result. Required platform confirmations still apply.

On timeout, UNKNOWN or EXECUTING, inspect the receipt first and reuse the same requestId. Do not blindly send again. SENT / APPLIED means the handoff or operation was accepted, not that the downstream task completed. For pausing, tell the assistant to stop the agreed delegation; revoke the grant in Agbrio when technical access should stop. There is no new per-Bridge delegation-control dashboard in this release.

## What has been verified

Owner-reported cloud dot acceptance: **16 tools, INSTANCE + CONVERSATION_REVIEW, read_app/read_bridge/read_source passed**. Real dot writes remain untested. Local automated tests exercise protection, decision binding and handoff behavior; fictional browser previews are not live-dot write acceptance.

MCP Events can support dots, but Agbrio has not implemented event discovery, subscriptions, signed callbacks, deduplication, revocation stop or missed-event recovery. A connection is not automatic wakeup or an unattended loop. These require separate implementation and acceptance after explicit Bridge delegation.

[Copy full instructions to an assistant](ASSISTANT_COPY_PROMPT.en.md) · [Protocol and tools](ASSISTANT_MCP.md) · [Official plugin authentication](https://developers.openai.com/plugins/build/auth) · [Official dot apps guide](https://learn.chatgpt.com/docs/dots/computers-and-apps)

The compact route tabs and copy control are shown first. Open **View my steps** for your authorization, sign-in and delegation actions; technical details remain optional.
