# Agbrio 0.1.6 — OAuth Verify compatibility

Fix Codex plugin Verify OAuth authorization when the client requests both advertised scopes. Successful DCR registration in 0.1.5 did not fix this separate authorization error.

Scope sets now reach owner consent. You still choose one existing delegation; the token returns only its selected scope. Legacy Bridge grants stay at eight tools and cannot obtain whole-app permissions. PKCE, exact resource/callback, revocation and one-use authorization codes remain enforced. No refresh token is issued.

After updating, click Verify again to start a fresh OAuth flow and personally complete sign-in and Allow. Real Dot/account acceptance is separate from isolated automated tests. The public UI remains interface24; other in-progress UI work is excluded.

Regression: real HTTP tests exercise combined scopes in both orders, owner selection of either legacy Bridge or whole-app grants, narrow returned scope/tool counts, unknown scopes, wrong resource/PKCE, code replay and bearer/session isolation. No real account consent or provider turn is automated.
