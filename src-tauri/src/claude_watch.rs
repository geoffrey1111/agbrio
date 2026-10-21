//! Resident transcript watcher. For every Claude Code session that a Bridge, a
//! watch or a pending delivery depends on, it reads the session's own transcript
//! when the file changes and records the turn state and final reply. This is the
//! authority for receiving and for "is it idle"; the plugin only submits text.
use crate::host_application::RouterCore;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Default)]
pub(crate) struct Watcher { seen: HashMap<String, (u64, SystemTime)>, paths: HashMap<String, PathBuf> }

/// Sessions whose state matters: bound in an active Bridge, or watched.
fn targets(core: &RouterCore) -> Result<HashMap<String, Option<String>>, String> {
    let mut out = HashMap::new();
    for w in core.store.active_workstreams()?.into_iter().filter(|w| w.status == "ACTIVE" && w.trashed_at.is_none()) {
        let Ok(bindings) = core.store.role_bridge(&w.id) else { continue };
        for side in [bindings.decision, bindings.execution].into_iter().flatten() {
            if side.endpoint.provider == router_core::claude_code::PROVIDER { out.insert(side.endpoint.external_id, side.cwd); }
        }
    }
    for watch in core.store.codex_watches()?.into_iter().filter(|w| w.enabled) {
        if let Some(id) = watch.thread_id.strip_prefix(router_core::store::claude_manual::PREFIX) { out.entry(id.to_owned()).or_insert(Some(watch.cwd)); }
    }
    Ok(out)
}

impl Watcher {
    /// One pass. Returns how many sessions changed.
    pub(crate) fn tick(&mut self, core: &RouterCore) -> Result<usize, String> {
        let Some(root) = crate::claude_transcript::projects_root() else { return Ok(0) };
        let targets = targets(core)?;
        let live: HashSet<&String> = targets.keys().collect();
        self.seen.retain(|k, _| live.contains(k));
        let mut changed = 0;
        for (session, cwd) in &targets {
            let path = match self.paths.get(session).filter(|p| p.is_file()) {
                Some(p) => p.clone(),
                None => { let Some(p) = crate::claude_transcript::find(&root, session) else { continue }; self.paths.insert(session.clone(), p.clone()); p }
            };
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            let stamp = (meta.len(), meta.modified().unwrap_or(SystemTime::UNIX_EPOCH));
            let recovering = core.store.claude_session(session).is_ok_and(|s| matches!(s.state.as_str(), "SESSION_START"|"SESSION_END"));
            if self.seen.get(session) == Some(&stamp) && !recovering { continue; }
            if let Err(code) = observe(core, &path, session, cwd.as_deref()) { crate::claude_event_log::record(session, "TRANSCRIPT", &code); } else { self.seen.insert(session.clone(), stamp); changed += 1; }
        }
        Ok(changed)
    }
}

pub(crate) fn observe(core: &RouterCore, path: &std::path::Path, session: &str, cwd: Option<&str>) -> Result<(), String> {
    let Some(status) = crate::claude_transcript::status(path, session) else { return Ok(()) };
    let root = status.cwd.as_deref().or(cwd).unwrap_or_default();
    let reply = status.reply.as_ref().map(|text| router_core::claude_code::CompleteReply { turn_id: status.turn.clone(), text: text.clone(), completed_at: status.completed_at.max(1) });
    let completed = core.store.apply_claude_transcript(session, root, status.state, Some(&status.turn), reply.as_ref())?;
    if completed { crate::claude_code::materialize_bound(core, session)?; }
    crate::claude_chat::update_watch(core, session)?;
    crate::claude_chat::reconcile_deliveries(core, session)?;
    Ok(())
}

pub(crate) fn start(core: RouterCore) {
    std::thread::spawn(move || {
        let mut watcher = Watcher::default();
        loop {
            if let Err(code) = watcher.tick(&core) { crate::claude_event_log::record("watcher", "TICK", &code); }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}
