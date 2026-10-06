-- A same-origin PWA Service Worker may acknowledge that it completed the
-- privacy-safe notification rendering path. This is not a handset banner or
-- user-attention receipt, and it stores no PushSubscription capability data.
ALTER TABLE reply_observations ADD COLUMN push_rendered_at INTEGER NULL;
