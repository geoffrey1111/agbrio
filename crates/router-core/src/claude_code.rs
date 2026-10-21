//! Claude Code adapter contract. Native IDs, not Desktop panes or titles, route.
//! No process launcher, provider credentials, model API or transcript-file access.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROVIDER: &str = "CLAUDE_CODE";
/// Autonomous/MCP/cross-provider writes remain closed. A separate authenticated
/// owner UI may submit a journaled manual text reply through ClaudeCodeAdapter.
pub const WRITES_ENABLED: bool = false;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Event {
    pub event_id: String,
    pub session_id: String,
    pub instance_id: String,
    pub sequence: i64,
    pub cwd: String,
    pub version: String,
    pub kind: String,
    pub turn_id: Option<String>,
    pub text: Option<String>,
    pub completed_at: Option<i64>,
}

fn native_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
impl Event {
    pub fn validate(&self, now: i64) -> Result<(), String> {
        if !native_id(&self.event_id) || !native_id(&self.session_id)
            || !native_id(&self.instance_id) || self.sequence < 1
            || self.cwd.len() > 8192 || !std::path::Path::new(&self.cwd).is_absolute()
            || self.cwd.starts_with("\\\\") || self.cwd.starts_with("//")
            || !supported_version(&self.version)
        { return Err("CLAUDE_EVENT_IDENTITY_INVALID".into()); }
        if !matches!(self.kind.as_str(), "SESSION_START" | "RUNNING" | "COMPLETE" | "WAITING_DECISION" | "FAILED" | "INTERRUPTED" | "SESSION_END" | "HEARTBEAT" | "EMPTY" | "LIMITED" | "RESULT_PENDING") {
            return Err("CLAUDE_EVENT_KIND_UNSUPPORTED".into());
        }
        if self.turn_id.as_ref().is_some_and(|v| !native_id(v)) {
            return Err("CLAUDE_EVENT_IDENTITY_INVALID".into());
        }
        if self.kind=="RUNNING" && self.turn_id.is_none() {return Err("CLAUDE_RUNNING_TURN_ID_REQUIRED".into());}
        if self.kind == "COMPLETE" {
            if self.turn_id.is_none()
                || self.text.as_ref().is_none_or(|v| v.trim().is_empty() || v.len() > 1_000_000)
                || self.completed_at.is_none_or(|v| v <= 0 || v > now + 30_000)
            { return Err("CLAUDE_COMPLETE_REPLY_UNPROVEN".into()); }
        } else if self.text.is_some() || self.completed_at.is_some() {
            return Err("CLAUDE_NONTERMINAL_CONTENT_REJECTED".into());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(self).map_err(|_| "CLAUDE_EVENT_INVALID")?)))
    }
}
pub fn supported_version(value: &str) -> bool {
    let Some(parts) = value.split('.').map(str::parse::<u32>).collect::<Result<Vec<_>, _>>().ok() else { return false; };
    parts.len() == 3 && (parts[0], parts[1], parts[2]) >= (2, 1, 286)
}
/// The Claude desktop app pre-starts "spare" sessions in `~/.claude/spares/spare-*`
/// and deletes that folder once a spare becomes a real conversation. The session
/// keeps reporting the spare path, so it is matched by its exact text.
pub fn spare_root(cwd: &str) -> bool {
    let lower = cwd.replace('/', "\\").to_lowercase();
    lower.contains("\\.claude\\spares\\spare-") && !lower.contains("..")
}
/// Comparable identity of a session root: the canonical folder, or for a spare
/// folder that may already be gone, its exact path text.
pub fn root_key(cwd: &str) -> Option<std::path::PathBuf> {
    std::fs::canonicalize(cwd).ok().or_else(|| spare_root(cwd).then(|| std::path::PathBuf::from(cwd.replace('/', "\\").to_lowercase())))
}
pub fn same_root(a: &str, b: &str) -> bool {
    match (root_key(a), root_key(b)) { (Some(a), Some(b)) => a == b || (spare_root(&a.to_string_lossy()) && a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()), _ => false }
}
pub fn require_write_capability() -> Result<(), String> {
    if !WRITES_ENABLED { return Err("CLAUDE_WRITES_DISABLED_BY_OWNER".into()); }
    Ok(())
}

/// Adapter-owned read surface. Application code receives normalized identities
/// and final replies; no Desktop handles, vendor payloads or transcript paths.
pub struct ClaudeCodeAdapter<'a> { store:&'a crate::store::RouterStore }
impl<'a> ClaudeCodeAdapter<'a> {
    pub fn new(store:&'a crate::store::RouterStore)->Self {Self{store}}
    pub fn sessions(&self)->Result<crate::store::claude_code::Catalog,String>{self.store.claude_catalog()}
    pub fn receive(&self,token:&str,event:&Event)->Result<bool,String>{self.store.ingest_claude_event(token,event)}
    pub fn verify(&self,id:&str,cwd:Option<&str>)->Result<Session,String>{
        let session=self.store.claude_session(id)?;
        let at=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        if session.lease_until<=at || session.state=="SESSION_END" {return Err("CLAUDE_SESSION_UNCONFIRMED".into());}
        if let Some(cwd)=cwd {
            if !same_root(cwd,&session.cwd) {return Err("CLAUDE_ROOT_CHANGED".into());}
        }
        Ok(session)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub session_id: String,
    pub instance_id: String,
    pub cwd: String,
    pub version: String,
    pub state: String,
    pub turn_id: Option<String>,
    pub checked_at: i64,
    pub lease_until: i64,
    /// Native display metadata only; never a routing or authorization key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub latest_reply: Option<CompleteReply>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteReply {
    pub turn_id: String,
    pub text: String,
    pub completed_at: i64,
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn quota_gate_and_version_are_fail_closed() {
        assert_eq!(require_write_capability().unwrap_err(), "CLAUDE_WRITES_DISABLED_BY_OWNER");
        for value in ["2.1.285", "2.1", "2.1.286-dev", "unknown"] { assert!(!supported_version(value)); }
        assert!(supported_version("2.1.286"));
        assert!(supported_version("2.2.0"));
    }
}
