//! Host seam for the official local Claude Code plugin. Never starts or resumes a Claude model session.
use crate::host_application::RouterCore;
use router_core::claude_code::{Event, Session};
use router_core::store::role_bridge::RoleEndpoint;
#[cfg(test)]
#[path="claude_code_tests.rs"]
mod tests;

/// Every unarchived Claude Code conversation on this computer, from all desktop
/// account partitions, plus sessions the plugin registered. A conversation the plugin
/// has not reported yet is `DESKTOP`: selectable by its exact native identity, and
/// observed once it is opened in Claude. Delivery still needs `verified_session`.
pub(crate) fn catalog(core:&RouterCore)->Result<router_core::store::claude_code::Catalog,String>{
    let mut catalog=router_core::claude_code::ClaudeCodeAdapter::new(core.store.as_ref()).sessions()?;
    let names=crate::claude_code_desktop::names(&catalog.sessions);
    for session in &mut catalog.sessions {session.title=names.get(&session.session_id).cloned();}
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
    let desktop=crate::claude_code_desktop::sessions();
    let listed=|id:&str|desktop.iter().find(|d|d.session_id==id);
    catalog.sessions.retain(|s|s.lease_until>now||!listed(&s.session_id).is_some_and(|d|d.archived));
    // A conversation that began as a desktop "spare" keeps reporting the spare folder;
    // show it under its real project and title. Binding still uses the reported root.
    for s in catalog.sessions.iter_mut().filter(|s|router_core::claude_code::spare_root(&s.cwd)) {
        if let Some(d)=listed(&s.session_id) {s.cwd=d.cwd.clone();if s.title.is_none(){s.title=d.title.clone();}}
    }
    for s in catalog.sessions.iter_mut().filter(|s|s.lease_until<=now) {
        if let Some(d)=listed(&s.session_id).filter(|d|!d.archived) {
            s.state="DESKTOP".into();s.checked_at=s.checked_at.max(d.last_activity);
            if s.title.is_none(){s.title=d.title.clone();}
        }
    }
    for d in desktop.iter().filter(|d|!d.archived) {
        if catalog.sessions.iter().any(|s|s.session_id==d.session_id){continue;}
        catalog.sessions.push(Session{session_id:d.session_id.clone(),instance_id:String::new(),cwd:d.cwd.clone(),version:String::new(),state:"DESKTOP".into(),turn_id:None,checked_at:d.last_activity,lease_until:0,title:d.title.clone(),latest_reply:None});
    }
    Ok(catalog)
}
/// Exact native identity for binding or watching: a live registered session, or an
/// unarchived conversation the Claude desktop app lists.
pub(crate) fn selectable_cwd(core:&RouterCore,external:&str)->Result<String,String>{
    match observed_session(core,external,None) {
        Ok(session)=>Ok(session.cwd),
        Err(error)=>crate::claude_code_desktop::sessions().into_iter().find(|d|d.session_id==external&&!d.archived).map(|d|d.cwd).ok_or(error),
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct ConnectionResult {pub phase:String,pub writes_enabled:bool}
fn connection_result(phase:&str)->ConnectionResult{ConnectionResult{phase:phase.into(),writes_enabled:false}}
fn live_publisher(core:&RouterCore)->Result<bool,String>{
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
    Ok(core.store.claude_catalog()?.sessions.iter().any(|s|s.lease_until>now&&s.state!="SESSION_END"))
}

/// An explicit native-app action. Page entry/copy/mobile/MCP never invokes it.
pub(crate) fn connect(core:&RouterCore,host:&crate::HostRuntime,resources:&std::path::Path)->Result<ConnectionResult,String>{
    // Reuse proven registration before discovering or configuring anything.
    let required:serde_json::Value=std::fs::read(resources.join("claude-code-plugin/.claude-plugin/plugin.json")).ok().and_then(|b|serde_json::from_slice(&b).ok()).ok_or("CLAUDE_OBSERVATION_PLUGIN_NOT_PACKAGED")?;
    let required=required["version"].as_str().and_then(|v|semver::Version::parse(v).ok()).ok_or("CLAUDE_PLUGIN_VERSION_UNCONFIRMED")?;
    let versions=plugin_versions();let reported=versions["selfReportedLiveVersions"].as_object();
    let compatible=reported.is_some_and(|v|!v.is_empty()&&v.keys().all(|v|semver::Version::parse(v).is_ok_and(|v|v>=required)));
    if live_publisher(core)?&&compatible{return Ok(connection_result("CONNECTED"));}
    static CONFIGURE:std::sync::Mutex<()>=std::sync::Mutex::new(());
    let _guard=CONFIGURE.try_lock().map_err(|_|"CLAUDE_CONNECTION_BUSY")?;
    let executable=crate::claude_code_desktop::engine_at(&crate::claude_code_desktop::engine_roots())?;
    let profile=std::env::var_os("USERPROFILE").ok_or("CLAUDE_PLUGIN_CACHE_UNAVAILABLE")?;
    connect_admin(core,host,resources,&crate::claude_code_desktop::OfficialCli{executable},&std::path::PathBuf::from(profile).join(".claude/plugins/cache"))
}
fn connect_admin(core:&RouterCore,host:&crate::HostRuntime,resources:&std::path::Path,admin:&dyn crate::claude_code_desktop::PluginAdmin,cache_root:&std::path::Path)->Result<ConnectionResult,String>{
    if live_publisher(core)? {return Ok(connection_result("CONNECTED"));}
    let package=resources.join("claude-code-plugin");
    let manifest:serde_json::Value=std::fs::read(package.join(".claude-plugin/plugin.json")).ok().and_then(|b|serde_json::from_slice(&b).ok()).ok_or("CLAUDE_OBSERVATION_PLUGIN_NOT_PACKAGED")?;
    if manifest["name"]!="agbrio-bridge"||manifest["userConfig"]["token"]["sensitive"]!=true {return Err("CLAUDE_OBSERVATION_PLUGIN_NOT_PACKAGED".into());}
    let address=host.mobile_address().filter(|a|a.ip().is_loopback()&&a.port()>0).ok_or("CLAUDE_OBSERVATION_HOST_UNAVAILABLE")?;
    let origin=format!("http://{address}");
    let args=|values:&[&str]|values.iter().map(|s|s.to_string()).collect::<Vec<_>>();
    let id="agbrio-bridge@agbrio-observation";
    let mut list=admin.run(&args(&["plugin","list","--json"]),None)?;
    if !list.is_array(){return Err("CLAUDE_PLUGIN_ADMIN_INVALID_RESULT".into());}
    if !list.as_array().unwrap().iter().any(|p|p["id"]==id&&p["scope"]=="user"){
        let path=package.to_string_lossy();
        admin.run(&args(&["plugin","marketplace","add",&path,"--json"]),None)?;
        admin.run(&args(&["plugin","install",id,"--scope","user","--json"]),None)?;
        list=admin.run(&args(&["plugin","list","--json"]),None)?;
    }
    let required=manifest["version"].as_str().and_then(|v|semver::Version::parse(v).ok()).ok_or("CLAUDE_PLUGIN_VERSION_UNCONFIRMED")?;
    let cache=std::fs::canonicalize(cache_root).map_err(|_|"CLAUDE_PLUGIN_CACHE_UNAVAILABLE")?;
    let mut checked=None;
    for attempt in 0..2 {
        let installed=list.as_array().and_then(|a|a.iter().find(|p|p["id"]==id&&p["scope"]=="user")).ok_or("CLAUDE_PLUGIN_USER_INSTALL_REQUIRED")?;
        let path=installed["installPath"].as_str().and_then(|p|std::fs::canonicalize(p).ok()).filter(|p|p.starts_with(&cache)&&p.join(".claude-plugin/plugin.json").is_file()).ok_or("CLAUDE_PLUGIN_CACHE_UNAVAILABLE")?;
        let current:serde_json::Value=std::fs::read(path.join(".claude-plugin/plugin.json")).ok().and_then(|b|serde_json::from_slice(&b).ok()).ok_or("CLAUDE_OBSERVATION_PLUGIN_NOT_PACKAGED")?;
        if current["name"]!="agbrio-bridge"||current["userConfig"]["token"]["sensitive"]!=true{return Err("CLAUDE_OBSERVATION_PLUGIN_NOT_PACKAGED".into());}
        let version=current["version"].as_str().and_then(|v|semver::Version::parse(v).ok()).ok_or("CLAUDE_PLUGIN_VERSION_UNCONFIRMED")?;
        if version<required {
            if attempt==1{return Err("CLAUDE_PLUGIN_UPDATE_REQUIRED".into());}
            admin.run(&args(&["plugin","update",id,"--scope","user","--json"]),None)?;
            list=admin.run(&args(&["plugin","list","--json"]),None)?;
            continue;
        }
        checked=Some((path,installed["enabled"]==true));break;
    }
    let(path,enabled)=checked.ok_or("CLAUDE_PLUGIN_UPDATE_REQUIRED")?;
    if !enabled {
        admin.run(&args(&["plugin","enable",id,"--scope","user","--json"]),None)?;
        let confirmed=admin.run(&args(&["plugin","list","--json"]),None)?;
        if !confirmed.as_array().is_some_and(|rows|rows.iter().any(|p|p["id"]==id&&p["scope"]=="user"&&p["enabled"]==true)){return Err("CLAUDE_PLUGIN_ENABLE_UNCONFIRMED".into());}
    }
    let path=path.to_string_lossy();
    let identities=[id,"agbrio-bridge@inline"];
    // Claude 2.1.299+: --plugin-dir shadows the installed identity, so only the inline one uses it.
    let scoped=|identity:&str,rest:&[&str]|{let mut v=if identity==id{vec![]}else{vec!["--plugin-dir".to_string(),path.to_string()]};v.extend(["plugin","configure",identity].iter().chain(rest).map(|s|s.to_string()));v};
    let mut missing_token=false;let mut missing_hosts=Vec::new();
    for identity in identities {
        let config=admin.run(&scoped(identity,&["--json"]),None)?;
        if config["pluginId"]!=identity||!config["configured"].is_array()||!config["unconfigured"].is_array(){return Err("CLAUDE_PLUGIN_ADMIN_INVALID_RESULT".into());}
        if let Some(existing)=config["inputs"]["hostUrl"].as_str().filter(|s|!s.is_empty()) {if existing!=origin {return Err("CLAUDE_PLUGIN_DIFFERENT_HOST".into());}}
        missing_token|=!config["configured"].as_array().unwrap().iter().any(|v|v=="token");
        if !config["configured"].as_array().unwrap().iter().any(|v|v=="hostUrl")||config["inputs"]["hostUrl"].as_str().is_none_or(str::is_empty){missing_hosts.push(identity);}
    }
    // Existing saved credentials remain in their native secure store. Never
    // export them, rotate them on repeat connection, or claim a live publisher.
    if !missing_token {
        // Repair only the missing non-secret address; never export/copy/rotate
        // a saved native credential merely because hostUrl was absent.
        for identity in missing_hosts {
            let input=serde_json::to_vec(&serde_json::json!({"hostUrl":origin})).map_err(|_|"CLAUDE_CONNECTION_CONFIGURATION_FAILED")?;
            let result=admin.run(&scoped(identity,&["--values-stdin","--json"]),Some(&input))?;
            if result["refused"].is_object()||!result["saved"].as_array().is_some_and(|a|a.iter().any(|v|v=="hostUrl")){return Err("CLAUDE_CONNECTION_CONFIGURATION_FAILED".into());}
            let checked=admin.run(&scoped(identity,&["--json"]),None)?;
            if checked["inputs"]["hostUrl"]!=origin||!checked["configured"].as_array().is_some_and(|a|["hostUrl","token"].iter().all(|k|a.iter().any(|v|v==k))){return Err("CLAUDE_CONNECTION_CONFIGURATION_FAILED".into());}
        }
        return Ok(connection_result("WAITING_SESSION"));
    }
    let pairing=core.store.create_claude_connection()?;
    let mut input=serde_json::to_vec(&serde_json::json!({"hostUrl":origin,"token":pairing.token})).map_err(|_|"CLAUDE_CONNECTION_CONFIGURATION_FAILED")?;
    let configured=(||{
        for identity in identities {
            let result=admin.run(&scoped(identity,&["--values-stdin","--json"]),Some(&input))?;
            if result["refused"].is_object()||!result["saved"].as_array().is_some_and(|a|["hostUrl","token"].iter().all(|k|a.iter().any(|v|v==k))){return Err("CLAUDE_CONNECTION_CONFIGURATION_FAILED".to_string());}
            let checked=admin.run(&scoped(identity,&["--json"]),None)?;
            if checked["pluginId"]!=identity||!checked["unconfigured"].as_array().is_some_and(Vec::is_empty)||!checked["configured"].as_array().is_some_and(|a|["hostUrl","token"].iter().all(|k|a.iter().any(|v|v==k))){return Err("CLAUDE_CONNECTION_CONFIGURATION_FAILED".to_string());}
        }
        Ok(connection_result("WAITING_SESSION"))
    })();
    input.fill(0);
    if configured.is_err(){let _=core.store.revoke_claude_connection(&pairing.connection_id);}
    configured
}

pub(crate) fn verified_session(core:&RouterCore,external:&str,cwd:Option<&str>)->Result<Session,String> {
    router_core::claude_code::ClaudeCodeAdapter::new(core.store.as_ref()).verify(external,cwd)
}
/// Exact identity for reading and for queuing a delivery; no live plugin lease needed.
pub(crate) fn observed_session(core:&RouterCore,external:&str,cwd:Option<&str>)->Result<Session,String> {
    let mut session=router_core::claude_code::ClaudeCodeAdapter::new(core.store.as_ref()).observe(external,cwd)?;
    // A reader may open a bound conversation outside the resident watch set.
    // Always project its exact native turn, rather than putting an old stored
    // notification below a freshly read transcript. No provider/store write.
    if let Some(status)=crate::claude_transcript::projects_root().and_then(|root|crate::claude_transcript::find(&root,external)).and_then(|path|crate::claude_transcript::status(&path,external)).filter(|s|s.cwd.as_ref().is_some_and(|root|router_core::claude_code::same_root(root,&session.cwd))) {
        let approval=session.state=="WAITING_DECISION"&&session.turn_id.as_deref()==Some(&status.turn);
        session.state=if approval{"WAITING_DECISION".into()}else{status.state.into()};
        session.turn_id=Some(status.turn.clone());
        session.checked_at=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        session.latest_reply=status.reply.map(|text|router_core::claude_code::CompleteReply{turn_id:status.turn,text,completed_at:status.completed_at.max(1)});
    }
    if core.store.claude_owner_request_pending(external,&session.cwd).unwrap_or(false){session.state="WAITING_DECISION".into();}
    Ok(session)
}
/// The session's own transcript owns state, turn and reply once it exists.
pub(crate) fn transcript_owned(session:&str)->bool {
    crate::claude_transcript::projects_root().and_then(|root|crate::claude_transcript::find(&root,session)).is_some()
}
/// Records a new completed reply for every active Bridge bound to this session.
pub(crate) fn materialize_bound(core:&RouterCore,session:&str)->Result<(),String> {
    for workstream in core.store.active_workstreams()?.into_iter().filter(|w|w.status=="ACTIVE"&&w.trashed_at.is_none()) {
        let bindings=core.store.role_bridge(&workstream.id)?;
        for side in [&bindings.decision,&bindings.execution].into_iter().flatten() {
            if side.endpoint.provider==router_core::claude_code::PROVIDER && side.endpoint.external_id==session {
                if let Err(code)=materialize(core,&workstream.id,side){crate::claude_event_log::record(session,"MATERIALIZE",&code);}
            }
        }
    }
    Ok(())
}

pub(crate) fn materialize(core:&RouterCore,workstream:&str,side:&RoleEndpoint)->Result<(),String> {
    let session=observed_session(core,&side.endpoint.external_id,side.cwd.as_deref())?;
    if session.state!="COMPLETE" {return Err(format!("CLAUDE_REPLY_{}",session.state));}
    let reply=session.latest_reply.ok_or("CLAUDE_COMPLETE_REPLY_UNPROVEN")?;
    let identity=format!("claude_code:{}",reply.turn_id);
    core.store.record_reply_observation(workstream,&side.endpoint.id,Some(&identity),&reply.text,None)?;
    core.store.note_reply_completion(workstream,&side.endpoint.id,&identity,Some(reply.completed_at))?;
    Ok(())
}
pub(crate) fn activity(core:&RouterCore,side:&RoleEndpoint)->crate::role_bridge::RoleActivity {
    let session=observed_session(core,&side.endpoint.external_id,side.cwd.as_deref()).ok();
    let reply=session.as_ref().filter(|s|s.state=="COMPLETE").and_then(|s|s.latest_reply.as_ref());
    let result_observation_id=reply.and_then(|r|core.store.reply_observations_for_workstream(&side.endpoint.workstream_id).ok().and_then(|rows|rows.into_iter().find(|o|o.endpoint_id==side.endpoint.id&&o.assistant_identity.as_deref()==Some(&format!("claude_code:{}",r.turn_id))).map(|o|o.id)));
    let state=session.as_ref().map(|s|match s.state.as_str(){"SESSION_START"=>"UNCONFIRMED","WAITING_DECISION"=>"ACTION_REQUIRED",s=>s}).unwrap_or("UNCONFIRMED");
    let tree=crate::claude_task_tree::read(core,&side.endpoint.external_id,side.cwd.as_deref().unwrap_or_default());
    let state=tree.phase(state);
    crate::role_bridge::RoleActivity {role:side.role.clone(),endpoint_id:side.endpoint.id.clone(),state:state.into(),checked_at:if tree.nodes.is_empty(){session.as_ref().map(|s|s.checked_at as u64).unwrap_or(0)}else{std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64},turn_id:session.as_ref().and_then(|s|s.turn_id.clone()),result_observation_id,goal_status:None,turn_active:session.as_ref().is_some_and(|s|matches!(s.state.as_str(),"RUNNING"|"WAITING_DECISION")),latest_turn:None,goal:None,goal_controls:None,task_tree:Some(tree)}
}
pub(crate) fn ingest(core:&RouterCore,token:&str,event:&Event)->Result<bool,String> {
    let adapter=router_core::claude_code::ClaudeCodeAdapter::new(core.store.as_ref());
    // With a transcript, plugin events only prove liveness; the transcript watcher
    // records turns and replies even when the plugin is quiet or an event is lost.
    if transcript_owned(&event.session_id) {return adapter.receive_liveness(token,event);}
    let inserted=adapter.receive(token,event)?;
    if event.kind=="COMPLETE" {
        // An old duplicate acknowledged after a newer turn must not re-read a
        // different current result or block the publisher's serialized queue.
        let current=core.store.claude_session(&event.session_id)?;
        if current.state!="COMPLETE" || current.turn_id!=event.turn_id {return Ok(inserted);}
        materialize_bound(core,&event.session_id)?;
    }
    Ok(inserted)
}

/// Read the already-running listener. Never reparse or rewrite deployment config.
pub(crate) fn setup(core:&RouterCore,host:&crate::HostRuntime,resources:&std::path::Path)->Result<serde_json::Value,String>{
    let plugin=resources.join("claude-code-plugin");
    if !plugin.join(".claude-plugin/plugin.json").is_file(){return Err("CLAUDE_OBSERVATION_PLUGIN_NOT_PACKAGED".into());}
    let address=host.mobile_address().filter(|a|a.ip().is_loopback()&&a.port()>0).ok_or("CLAUDE_OBSERVATION_HOST_UNAVAILABLE")?;
    let pairing=core.store.create_claude_connection()?;
    Ok(serde_json::json!({"connectionId":pairing.connection_id,"hostUrl":format!("http://{}",address),"token":pairing.token,"pluginDirectory":plugin.to_string_lossy(),"expiresAt":pairing.expires_at,"writesEnabled":false}))
}

// Self-reported version on an authenticated adapter heartbeat; diagnostic only.
static PLUGIN_VERSIONS:std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String,(String,std::time::Instant)>>> =std::sync::OnceLock::new();
pub(crate) fn record_plugin_version(session:&str,version:&str){if version.len()>24||version.split('.').count()!=3||version.split('.').any(|n|n.is_empty()||!n.bytes().all(|b|b.is_ascii_digit())){return;}let Ok(mut rows)=PLUGIN_VERSIONS.get_or_init(Default::default).lock()else{return;};rows.retain(|_,(_,at)|at.elapsed().as_secs()<90);if rows.len()<512||rows.contains_key(session){rows.insert(session.into(),(version.into(),std::time::Instant::now()));}}
pub(crate) fn plugin_versions()->serde_json::Value{let mut versions=std::collections::BTreeMap::<String,usize>::new();if let Ok(rows)=PLUGIN_VERSIONS.get_or_init(Default::default).lock(){for(version,at)in rows.values().filter(|(_,at)|at.elapsed().as_secs()<90){let _=at;*versions.entry(version.clone()).or_default()+=1;}}serde_json::json!({"selfReportedLiveVersions":versions,"requiredForExpandedWaits":"0.1.11","requiredForDurableTaskNotices":"0.1.19","source":"AUTHENTICATED_HEARTBEAT"})}
