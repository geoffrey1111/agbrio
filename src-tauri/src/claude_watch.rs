//! Resident transcript watcher. For every Claude Code session that a Bridge, a
//! watch or a pending delivery depends on, it reads the session's own transcript
//! when the file changes and records the turn state and final reply. This is the
//! authority for receiving and for "is it idle"; the plugin only submits text.
use crate::host_application::RouterCore;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Default)]
pub(crate) struct Watcher { seen: HashMap<String, (u64, SystemTime)>, paths: HashMap<String, PathBuf>, cursor:Option<String> }

/// Sessions whose state matters: bound in an active Bridge, or watched.
pub(crate) fn targets(core: &RouterCore) -> Result<HashMap<String, Option<String>>, String> {
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
    for (key,_,_,cwd) in core.store.claude_child_contexts()? {if core.store.codex_chat_context(&key).is_ok(){if let Ok(id)=router_core::store::claude_manual::native_id(&key){out.entry(id.into()).or_insert(Some(cwd));}}}
    Ok(out)
}

impl Watcher {
    /// One pass. Returns how many sessions changed.
    pub(crate) fn tick(&mut self, core: &RouterCore) -> Result<usize, String> {
        if let Err(code)=refresh_titles(core,&crate::claude_code_desktop::sessions()) {crate::claude_event_log::record("watcher","TITLE",&code);}
        let Some(root) = crate::claude_transcript::projects_root() else { return Ok(0) };
        crate::claude_child_contexts::renew(core);
        crate::claude_owner_requests::tick(core);
        let targets = targets(core)?;
        let live: HashSet<&String> = targets.keys().collect();
        self.seen.retain(|k, _| live.contains(k));
        self.paths.retain(|k,_|live.contains(k));
        let selected=crate::observer_batch::next_ids(targets.keys().map(String::as_str),&mut self.cursor,crate::observer_batch::WATCH_READS_PER_PASS);
        let mut changed = 0;
        for (session, cwd) in targets.iter().filter(|(id,_)|selected.contains(*id)) {
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
        for session in &selected{let _=crate::claude_chat::observe_children(core,&format!("claude_code:{session}"));}
        Ok(changed)
    }
}

// Renaming changes desktop metadata, not necessarily the transcript stamp.
// Refresh independently of message reads, using only exact identity/root facts.
fn refresh_titles(core:&RouterCore,desktop:&[crate::claude_code_desktop::DesktopSession])->Result<(),String>{
 for watch in core.store.codex_watches()?.into_iter().filter(|w|w.enabled){
  let Some(id)=watch.thread_id.strip_prefix(router_core::store::claude_manual::PREFIX)else{continue};
  let names=desktop.iter().filter(|s|s.session_id==id&&router_core::claude_code::same_root(&s.cwd,&watch.cwd)).filter_map(|s|s.title.as_deref()).collect::<HashSet<_>>();
  if names.len()==1{let title=names.into_iter().next().unwrap();if title!=watch.label{core.store.refresh_codex_watch_label(&watch.thread_id,watch.generation,&watch.cwd,title)?;}}
 }
 Ok(())
}

#[cfg(test)]mod title_tests{
 use super::*;
 #[test]fn desktop_rename_updates_even_when_no_new_transcript_or_plugin_event_arrives(){
  let d=tempfile::tempdir().unwrap();let store=std::sync::Arc::new(router_core::store::RouterStore::open_at(d.path().join("router.db")).unwrap());let root=d.path().to_string_lossy().into_owned();let key="claude_code:display-only";
  store.enable_codex_watch(key,"old title",&root,&router_core::store::codex_watch::WatchSnapshot{state:"RUNNING".into(),turn_id:Some("same-turn".into()),item_id:None,text:"same text".into()}).unwrap();let original=store.registered_codex_watch(key).unwrap();
  let core=RouterCore{store:store.clone(),session:std::sync::Arc::new(std::sync::Mutex::default()),};
  let mut source=crate::claude_code_desktop::DesktopSession{local_id:None,session_id:"display-only".into(),cwd:root,title:Some("renamed native title".into()),archived:false,last_activity:1};
  refresh_titles(&core,&[source.clone()]).unwrap();let renamed=store.registered_codex_watch(key).unwrap();assert_eq!(renamed.label,"renamed native title");assert_eq!(renamed.generation,original.generation);assert_eq!(renamed.snapshot,original.snapshot);assert_eq!(renamed.changed_at,original.changed_at);
  source.cwd="unrelated-project".into();source.title=Some("wrong root title".into());refresh_titles(&core,&[source]).unwrap();assert_eq!(store.registered_codex_watch(key).unwrap().label,"renamed native title");
 }
}

pub(crate) fn observe(core: &RouterCore, path: &std::path::Path, session: &str, cwd: Option<&str>) -> Result<(), String> {
    let Some(status) = crate::claude_transcript::status(path, session) else { return Ok(()) };
    let root = status.cwd.as_deref().or(cwd).unwrap_or_default();
    let reply = status.reply.as_ref().map(|text| router_core::claude_code::CompleteReply { turn_id: status.turn.clone(), text: text.clone(), completed_at: status.completed_at.max(1) });
    let completed = core.store.apply_claude_transcript(session, root, status.state, Some(&status.turn), reply.as_ref())?;
    if let Some(at)=crate::conversation_message_time::claude(session,root){let _=core.store.note_conversation_message_at(&format!("claude_code:{session}"),root,at);}
    if completed { crate::claude_code::materialize_bound(core, session)?; }
    crate::claude_chat::update_watch(core, session)?;
    crate::claude_owner_requests::reconcile(core,session);
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
