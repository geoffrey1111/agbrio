# Security and private data

## Claude desktop setup and title metadata

Native Connect is an explicit desktop-only user action, separate from mobile/MCP authorization. Fixed official plugin-admin argv only; limited credentials pass stdin in memory, never arguments/files/clipboard or returned IPC. No prompt, session resume, restart, force reload or provider-account credential read. Live publishers/saved configuration are reused; other Host configuration is refused; only a failed attempt's own newly minted credential is revoked.

Title projection checks an already-registered native UUID and canonical root before reading the current-user Desktop metadata title. It stops parsing before transcript/MCP-config/approval fields, skips links/outside paths/ambiguous records and fails closed on unsupported layouts. Files never establish registration, replies, status or authorization. Real first-time application configuration remains a distinct native acceptance gate; fixtures do not bypass a previously refused agent credential operation.


Please report exploitable security problems privately to geoffreyzjx@qq.com.
Do not include credentials, live cookies, private files or complete chat logs.

Agbrio exposes a loopback-only HTTP listener behind your chosen HTTPS entrance.
The Host validates exact Host/Origin, TLS entry identity and revocable paired
device sessions. Cloudflare Access is optional for generic entries. A setup
probe never grants chat/data access. The same local Windows account controls
configuration and pairing. Public transport providers and agent providers
remain part of your trust boundary; no end-to-end-encryption claim is made.

Bridge uses stable provider thread/conversation identifiers. It never routes by
title, OCR or sidebar position. You approve visible content and target before a
send. Ambiguous delivery stays uncertain; automatic blind retry is prohibited.

Saved data is under the current user's `LOCALAPPDATA/AIWorkRouter/`; the internal
name is retained for existing-install compatibility. Private agent credentials
remain in their own provider installation. Phone cache/push subscriptions and
selected attachments are private data. No hosted multi-user service exists.

Do not expose the loopback port directly on a public network, disable TLS checks,
copy profiles/keys between users, or treat a selected attachment as system
instructions. Keep user-controlled setup requests in the intended local data
directory. Revoke lost phones and provider access separately.
