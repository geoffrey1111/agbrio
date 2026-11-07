//! Host-epoch receipts backed by a SQLite temporary file, not a lifetime RAM set.
//! SQLite removes the private temporary database when this connection closes.
use super::{Request, Value};
use rusqlite::{params, Connection, OptionalExtension};

pub(super) struct ReplayStore(Connection);

impl ReplayStore {
    #[cfg(test)]
    pub(super) fn cache_kib(&self) -> i64 {
        -self.0.query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0)).unwrap()
    }
    pub(super) fn new() -> Result<Self, String> {
        // An empty filename creates a private, automatically deleted disk DB.
        // No request-count cap, mmap or cache proportional to Host uptime.
        let db = Connection::open("").map_err(storage_error)?;
        db.execute_batch(
            "PRAGMA cache_size=-256; PRAGMA mmap_size=0;
             PRAGMA temp_store=FILE;
             CREATE TABLE receipts(id TEXT PRIMARY KEY, request TEXT NOT NULL, reply TEXT);",
        ).map_err(storage_error)?;
        Ok(Self(db))
    }

    pub(super) fn lookup(&self, request: &Request) -> Result<Option<Value>, String> {
        let row: Option<(String, Option<String>)> = self.0.query_row(
            "SELECT request, reply FROM receipts WHERE id=?1",
            [&request.request_id], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(storage_error)?;
        let Some((prior, reply)) = row else { return Ok(None) };
        let prior: Request = serde_json::from_str(&prior).map_err(storage_error)?;
        if &prior != request { return Err("APP_CONTROL_REPLAY_CONFLICT".into()) }
        // A reserved request whose effect/result is uncertain must never run again.
        let reply = reply.ok_or("APP_CONTROL_REQUEST_UNCONFIRMED")?;
        serde_json::from_str(&reply).map(Some).map_err(storage_error)
    }

    pub(super) fn reserve(&self, request: &Request) -> Result<(), String> {
        let bytes = serde_json::to_string(request).map_err(storage_error)?;
        self.0.execute("INSERT INTO receipts(id,request) VALUES(?1,?2)",
            params![request.request_id, bytes]).map_err(storage_error)?;
        Ok(())
    }

    pub(super) fn complete(&self, id: &str, reply: &Value) -> Result<(), String> {
        self.0.execute("UPDATE receipts SET reply=?2 WHERE id=?1",
            params![id, serde_json::to_string(reply).map_err(storage_error)?])
            .map_err(storage_error)?;
        Ok(())
    }
}

fn storage_error(_: impl std::fmt::Debug) -> String {
    "APP_CONTROL_REPLAY_STORAGE_UNAVAILABLE".into()
}
