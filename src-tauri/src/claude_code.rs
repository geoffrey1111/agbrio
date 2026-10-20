//! Host seam for the official local Claude Code plugin. Never starts or resumes a Claude model session.
use crate::host_application::RouterCore;
use router_core::claude_code::{Event, Session};
use router_core::store::role_bridge::RoleEndpoint;
#[cfg(test)]
#[path="claude_code_tests.rs"]
mod tests;

pub(crate) fn catalog(core:&RouterCore)->Result<router_core::store::claude_code::Catalog,String>{
    let mut catalog=router_core::claude_code::ClaudeCodeAdapter::new(core.store.as_ref()).sessions()?;
    let names=crate::claude_code_desktop::names(&catalog.sessions);
    for session in &mut catalog.sessions {session.title=names.get(&session.session_id).cloned();}
    Ok(catalog)
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
    if live_publisher(core)? {return Ok(connection_result("CONNECTED"));}
    static CONFIGURE:std::sync::Mutex<()>=std::sync::Mutex::new(());
    let _guard=CONFIGURE.try_lock().map_err(|_|"CLAUDE_CONNECTION_BUSY")?;
    let executable=crate::claude_code_desktop::engine_at(&crate::claude_code_desktop::data_roots())?;
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
    if !list.as_array().unwrap().iter().any(|p|p["id"]==id){
        let path=package.to_string_lossy();
        admin.run(&args(&["plugin","marketplace","add",&path,"--json"]),None)?;
        admin.run(&args(&["plugin","install",id,"--scope","user","--json"]),None)?;
        list=admin.run(&args(&["plugin","list","--json"]),None)?;
    }
    let installed=list.as_array().and_then(|a|a.iter().find(|p|p["id"]==id&&p["scope"]=="user")).ok_or("CLAUDE_PLUGIN_USER_INSTALL_REQUIRED")?;
    let cache=std::fs::canonicalize(cache_root).map_err(|_|"CLAUDE_PLUGIN_CACHE_UNAVAILABLE")?;
    let path=installed["installPath"].as_str().and_then(|p|std::fs::canonicalize(p).ok()).filter(|p|p.starts_with(&cache)&&p.join(".claude-plugin/plugin.json").is_file()).ok_or("CLAUDE_PLUGIN_CACHE_UNAVAILABLE")?;
    let installed_manifest:serde_json::Value=std::fs::read(path.join(".claude-plugin/plugin.json")).ok().and_then(|b|serde_json::from_slice(&b).ok()).ok_or("CLAUDE_OBSERVATION_PLUGIN_NOT_PACKAGED")?;
    if installed_manifest["name"]!="agbrio-bridge"||installed_manifest["userConfig"]["token"]["sensitive"]!=true {return Err("CLAUDE_OBSERVATION_PLUGIN_NOT_PACKAGED".into());}
    if installed["enabled"]!=true {admin.run(&args(&["plugin","enable",id,"--scope","user","--json"]),None)?;}
    let path=path.to_string_lossy();
    let identities=[id,"agbrio-bridge@inline"];
    // Claude 2.1.299+: --plugin-dir shadows the installed identity, so only the inline one uses it.
    let scoped=|identity:&str,rest:&[&str]|{let mut v=if identity==id{vec![]}else{vec!["--plugin-dir".to_string(),path.to_string()]};v.extend(["plugin","configure",identity].iter().chain(rest).map(|s|s.to_string()));v};
    let mut missing=false;
    for identity in identities {
        let config=admin.run(&scoped(identity,&["--json"]),None)?;
        if config["pluginId"]!=identity||!config["configured"].is_array()||!config["unconfigured"].is_array(){return Err("CLAUDE_PLUGIN_ADMIN_INVALID_RESULT".into());}
        if let Some(existing)=config["inputs"]["hostUrl"].as_str().filter(|s|!s.is_empty()) {if existing!=origin {return Err("CLAUDE_PLUGIN_DIFFERENT_HOST".into());}}
        missing|=!["hostUrl","token"].iter().all(|k|config["configured"].as_array().unwrap().iter().any(|v|v==k));
    }
    // Existing saved credentials remain in their native secure store. Never
    // export them, rotate them on repeat connection, or claim a live publisher.
    if !missing {return Ok(connection_result("WAITING_SESSION"));}
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

pub(crate) fn materialize(core:&RouterCore,workstream:&str,side:&RoleEndpoint)->Result<(),String> {
    let session=verified_session(core,&side.endpoint.external_id,side.cwd.as_deref())?;
    if session.state!="COMPLETE" {return Err(format!("CLAUDE_REPLY_{}",session.state));}
    let reply=session.latest_reply.ok_or("CLAUDE_COMPLETE_REPLY_UNPROVEN")?;
    let identity=format!("claude_code:{}",reply.turn_id);
    core.store.record_reply_observation(workstream,&side.endpoint.id,Some(&identity),&reply.text,None)?;
    core.store.note_reply_completion(workstream,&side.endpoint.id,&identity,Some(reply.completed_at))?;
    Ok(())
}
pub(crate) fn activity(core:&RouterCore,side:&RoleEndpoint)->crate::role_bridge::RoleActivity {
    let session=verified_session(core,&side.endpoint.external_id,side.cwd.as_deref()).ok();
    let reply=session.as_ref().filter(|s|s.state=="COMPLETE").and_then(|s|s.latest_reply.as_ref());
    let result_observation_id=reply.and_then(|r|core.store.reply_observations_for_workstream(&side.endpoint.workstream_id).ok().and_then(|rows|rows.into_iter().find(|o|o.endpoint_id==side.endpoint.id&&o.assistant_identity.as_deref()==Some(&format!("claude_code:{}",r.turn_id))).map(|o|o.id)));
    let state=session.as_ref().map(|s|match s.state.as_str(){"SESSION_START"=>"UNCONFIRMED","WAITING_DECISION"=>"ACTION_REQUIRED",s=>s}).unwrap_or("UNCONFIRMED");
    crate::role_bridge::RoleActivity {role:side.role.clone(),endpoint_id:side.endpoint.id.clone(),state:state.into(),checked_at:session.as_ref().map(|s|s.checked_at as u64).unwrap_or(0),turn_id:session.as_ref().and_then(|s|s.turn_id.clone()),result_observation_id,goal_status:None,turn_active:session.as_ref().is_some_and(|s|matches!(s.state.as_str(),"RUNNING"|"WAITING_DECISION")),latest_turn:None,goal:None,goal_controls:None}
}
pub(crate) fn ingest(core:&RouterCore,token:&str,event:&Event)->Result<bool,String> {
    let inserted=router_core::claude_code::ClaudeCodeAdapter::new(core.store.as_ref()).receive(token,event)?;
    if event.kind=="COMPLETE" {
        // An old duplicate acknowledged after a newer turn must not re-read a
        // different current result or block the publisher's serialized queue.
        let current=core.store.claude_session(&event.session_id)?;
        if current.state!="COMPLETE" || current.turn_id!=event.turn_id {return Ok(inserted);}
        // Lifecycle/exact role identity is rechecked before creating observations.
        // No binding means no business notification or takeover of a new Bridge.
        for workstream in core.store.active_workstreams()?.into_iter().filter(|w|w.status=="ACTIVE"&&w.trashed_at.is_none()) {
            let bindings=core.store.role_bridge(&workstream.id)?;
            for side in [&bindings.decision,&bindings.execution].into_iter().flatten() {
                if side.endpoint.provider==router_core::claude_code::PROVIDER && side.endpoint.external_id==event.session_id {
                    materialize(core,&workstream.id,side)?;
                }
            }
        }
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
