# Local-control replay receipt repair (rc123)

Repeated local board publication and Codex notice registration no longer exhaust a lifetime 128-request set. The owner-local dispatcher now uses a connection-private SQLite temporary disk database with a 256 KiB page-cache target, mmap disabled, and bounded single-request input. Receipts remain valid for the entire Host epoch; the database is automatically removed on close. There is no count-based rejection or eviction of valid receipts. Disk exhaustion/IO failures fail closed.

Reserve the exact normalized request before effects. Completed retries return the original response, changed payloads conflict, and incomplete reservations return APP_CONTROL_REQUEST_UNCONFIRMED without re-execution. Error responses after reservation are also retained. Existing expected-instance, current-user pipe/ACK and shutdown fences remain. No provider messages or network/proxy configuration are changed.

Regression: 512 real fixture PUBLISH_BOARD calls followed by all 512 replays after deleting the source file; changed-command conflicts; reserved-unknown fencing and configured cache bound. Existing real Windows pipe/lost-ACK test remains required. This is engineering validation, not a 33-hour soak or owner UAT.

SQLite reference: https://www.sqlite.org/inmemorydb.html#temporary_databases and https://www.sqlite.org/pragma.html#pragma_cache_size .
