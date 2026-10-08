# Agbrio 0.1.5 — MCP registration compatibility

Accept Codex/rmcp dynamic registration metadata requesting authorization_code and refresh_token together. Negotiate the registration response to the authorization_code grant already supported by Agbrio. No refresh tokens are advertised or issued. Existing OAuth/PKCE, exact MCP resource, redirect, owner consent, scope, expiry/revocation and tenant isolation remain. No UI behavior changes; interface2026.10.08-24.

The historical account registration body was not captured; this repair targets a reproduced public400 with the same error. A separate generic plugin setup failure and actual Dot owner consent require separate verification.

Validation: bounded current public reproduction400 vs201;10real-localHTTP registration/OAuth tests pass including consent,PKCE,resource,scope,replay and revocation; unsupported grant/token requests remain rejected. Account-specific20:31generic validation failure lacks payload/status evidence and is not conflated with the19:46registration400.
