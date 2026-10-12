//! Host seam for the official local Claude Code plugin. Never launches Claude.
use crate::host_application::RouterCore;
use router_core::claude_code::{Event, Session};
use router_core::store::role_bridge::RoleEndpoint;
#[cfg(test)]
#[path="claude_code_tests.rs"]
mod tests;

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
        for workstream in core.store.snapshot()?.workstreams.into_iter().filter(|w|w.status=="ACTIVE"&&w.trashed_at.is_none()) {
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
