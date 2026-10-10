//! Shared role-compatible services. No UI handle or provider-title routing.
use crate::host_application::*;
use crate::RouterStore;
use std::{collections::{HashMap,HashSet},sync::{Arc,Mutex}};
use crate::persistence::{HandoffHistoryItem, ReplyObservation};
use router_core::store::role_bridge::{RoleBindingInput, RoleBridge, RoleEndpoint};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BridgeState {
    pub(crate) bindings: RoleBridge,
    pub(crate) replies: Vec<ReplyObservation>,
    pub(crate) handoffs: Vec<HandoffHistoryItem>,
    pub(crate) handoff_sources:Vec<HandoffSource>,
    pub(crate) assistant_processed:Option<router_core::store::assistant_processed::AssistantProcessed>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub(crate) read_outcome:Option<BridgeReadOutcome>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub(crate) activities:Option<Vec<RoleActivity>>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub(crate) snapshot_at:Option<u64>,
}
#[derive(Clone,Serialize)]#[serde(rename_all="camelCase")]
pub(crate) struct RoleActivity{pub role:String,pub endpoint_id:String,pub state:String,pub checked_at:u64,pub turn_id:Option<String>,pub result_observation_id:Option<String>,pub goal_status:Option<String>,pub turn_active:bool,pub latest_turn:Option<router_core::codex::adapter::NativeTurnDiagnostic>,pub goal:Option<MobileCodexGoal>,pub goal_controls:Option<crate::goal_control::GoalControls>}
#[derive(Serialize)]#[serde(rename_all="camelCase")]
pub(crate) struct BridgeReadOutcome{role:String,endpoint_id:String,state:String,retained_reply:bool}
#[derive(Serialize)]#[serde(rename_all="camelCase")]
pub(crate) struct HandoffSource{pub id:String,pub binding_revision:i64,pub role:String}
pub(crate) fn catalog(core: &RouterCore) -> Result<ExistingCodexThreadCatalog, String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let s = core
            .session
            .lock()
            .map_err(|_| "Router session unavailable")?;
        if s.adapter.is_some() {
            break;
        }
        if !s.connecting || std::time::Instant::now() >= deadline {
            return Err("Codex backend disconnected. Use Reconnect first.".into());
        }
        drop(s);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    service_list_existing_codex_threads(core)
}
pub(crate) fn state(core: &RouterCore, workstream: &str) -> Result<BridgeState, String> {
    let bindings = core.store.role_bridge(workstream)?;
    let handoffs = core
        .store
        .snapshot_for_workstream(workstream)?
        .handoffs
        .into_iter()
        .filter(|h| core.store.role_handoff_source(&h.id).is_ok())
        .collect::<Vec<_>>();
    let handoff_sources=handoffs.iter().filter_map(|h|core.store.role_handoff_source(&h.id).ok().map(|(binding_revision,role)|HandoffSource{id:h.id.clone(),binding_revision,role})).collect();
    Ok(BridgeState {
        bindings,
        replies: core.store.reply_observations_for_workstream(workstream)?,
        handoffs,
        handoff_sources,
        assistant_processed:core.store.assistant_processed(workstream,activity_now() as i64)?,
        read_outcome:None,
        activities:None,snapshot_at:None,
    })
}
pub(crate) fn create(core:&RouterCore,name:&str)->Result<String,String>{
    let name=name.trim();if name.is_empty()||name.chars().count()>100{return Err("BRIDGE_NAME_INVALID".into());}
    let snapshot=core.store.snapshot()?;
    let project=if let Some(project)=snapshot.projects.first(){project.id.clone()}else{core.store.create_project("Bridge".into(),None)?.id};
    Ok(core.store.create_workstream(&project,name.into())?.id)
}
fn activity_now()->u64{std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64}
pub(crate) fn sync(core:&RouterCore,workstream:&str)->Result<BridgeState,String>{
    if let Ok(mut session)=core.session.try_lock(){
        if let Some(adapter)=session.adapter.as_mut().filter(|a|!a.is_closed()){
            crate::runtime_reconciler::RuntimeReconciler::reconcile_codex_workstream(&core.store,adapter,workstream)?;
        }
    }
    let initial=state(core,workstream)?;let mut activities=Vec::new();let mut fetch=Vec::new();
    {
        let mut session=core.session.try_lock().ok();
        for side in [&initial.bindings.decision,&initial.bindings.execution].into_iter().flatten(){
            let mut activity=RoleActivity{role:side.role.clone(),endpoint_id:side.endpoint.id.clone(),state:"UNCONFIRMED".into(),checked_at:activity_now(),turn_id:None,result_observation_id:None,goal_status:None,turn_active:false,latest_turn:None,goal:None,goal_controls:None};
            if side.endpoint.provider=="CODEX"{
                if let Some(adapter)=session.as_mut().and_then(|s|s.adapter.as_mut()).filter(|a|!a.is_closed()){
                    if let Ok(native)=adapter.read_thread_activity(&side.endpoint.external_id){
                        let root_matches=side.cwd.as_ref().is_some_and(|cwd|std::fs::canonicalize(cwd).ok().zip(std::fs::canonicalize(&native.cwd).ok()).is_some_and(|(a,b)|a==b));
                        if root_matches&&native.thread_id==side.endpoint.external_id{
                            activity.state=native.state.into();activity.turn_id=native.turn_id;activity.goal_status=native.goal_status;activity.turn_active=native.turn_active;
                            if activity.state=="COMPLETE"{
                                let prefix=activity.turn_id.as_ref().map(|t|format!("codex:{t}:"));
                                let reply=initial.replies.iter().find(|r|r.endpoint_id==side.endpoint.id&&prefix.as_ref().is_some_and(|p|r.assistant_identity.as_ref().is_some_and(|i|i.starts_with(p))));
                                activity.result_observation_id=reply.map(|r|r.id.clone());
                                if reply.is_some_and(|r|!r.completion_checked){fetch.push((side.role.clone(),side.endpoint.id.clone(),activity.turn_id.clone()));}
                                if activity.result_observation_id.is_none(){activity.state="RESULT_PENDING".into();fetch.push((side.role.clone(),side.endpoint.id.clone(),activity.turn_id.clone()));}
                            }
                        }
                    }
                }
            }
            activity.checked_at=activity_now();activities.push(activity);
        }
    }
    // Refresh only a proven current completed turn, never a persisted old result.
    for(role,endpoint,turn)in fetch{
        if core.store.codex_reply_observer_is_initialized(&endpoint)?{
            if let Some(side)=[&initial.bindings.decision,&initial.bindings.execution].into_iter().flatten().find(|s|s.endpoint.id==endpoint){
                let _=check_new_codex_endpoint_replies_with(&core.store,&core.session,workstream,side.endpoint.clone(),&mut |p|crate::push::send_payload(p));
            }
        }
        if let Ok(fresh)=read(core,workstream,&role){
            if let Some(activity)=activities.iter_mut().find(|s|s.role==role&&s.endpoint_id==endpoint){
                if let Some(outcome)=fresh.read_outcome{activity.state=match outcome.state.as_str(){"LATEST_TURN_ACTIVE"=>"RUNNING","LATEST_TURN_INTERRUPTED"=>"INTERRUPTED",_=>"UNCONFIRMED"}.into();}
                else if let Some(reply)=fresh.replies.iter().find(|r|r.endpoint_id==endpoint&&turn.as_ref().is_some_and(|t|r.assistant_identity.as_ref().is_some_and(|i|i.starts_with(&format!("codex:{t}:"))))){activity.state="COMPLETE".into();activity.result_observation_id=Some(reply.id.clone());}
                activity.checked_at=activity_now();
            }
        }
    }
    let mut result=state(core,workstream)?;
    if result.bindings.binding_revision!=initial.bindings.binding_revision{return Err("BRIDGE_BINDING_CHANGED".into());}
    result.activities=Some(activities);result.snapshot_at=Some(activity_now());Ok(result)
}
fn metadata(adapter: &mut crate::codex::adapter::CodexAdapter, id: &str) -> Result<Value, String> {
    let v = adapter.request_with_timeout(
        "thread/read",
        json!({"threadId":id,"includeTurns":false}),
        CODEX_OBSERVER_REQUEST_TIMEOUT,
    )?;
    if v.pointer("/thread/id").and_then(Value::as_str) != Some(id) {
        return Err("BRIDGE_NATIVE_IDENTITY_MISMATCH".into());
    }
    Ok(v)
}
fn native_root(v: &Value) -> Result<String, String> {
    let root = v
        .pointer("/thread/cwd")
        .and_then(Value::as_str)
        .ok_or("BRIDGE_CODEX_ROOT_UNAVAILABLE")?;
    let p = std::path::Path::new(root);
    if !p.is_absolute() || !p.is_dir() {
        return Err("BRIDGE_CODEX_ROOT_UNAVAILABLE".into());
    }
    Ok(p.to_string_lossy().into_owned())
}
fn verify_root(v: &Value, side: &RoleEndpoint) -> Result<(), String> {
    let native =
        std::fs::canonicalize(native_root(v)?).map_err(|_| "BRIDGE_CODEX_ROOT_UNAVAILABLE")?;
    if let Some(root) = &side.cwd {
        if std::fs::canonicalize(root).map_err(|_| "BRIDGE_CODEX_ROOT_UNAVAILABLE")? != native {
            return Err("BRIDGE_CODEX_ROOT_CHANGED".into());
        }
    }
    Ok(())
}
pub(crate) fn require_idle_goal(thread:&str,response:&Value)->Result<(),String>{
    let goal=codex_goal_from_response(thread,response)?;
    if goal.as_ref().is_some_and(|g|matches!(g.status.as_str(),"active"|"blocked")){return Err("BRIDGE_TARGET_GOAL_ACTIVE".into());}
    Ok(())
}

/// Passive metadata resolves one exact in-flight turn. The native expected-turn
/// precondition, not a title/cache/Goal flag, fences the subsequent physical input.
fn exact_active_turn(adapter:&mut crate::codex::adapter::CodexAdapter,thread:&str)->Result<String,String>{
    adapter.begin_latest_turn_observation(thread)?;
    let page=adapter.request_with_timeout("thread/turns/list",json!({"threadId":thread,"cursor":null,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"}),CODEX_OBSERVER_REQUEST_TIMEOUT)?;
    let rows=page["data"].as_array().filter(|rows|rows.len()==1).ok_or("BRIDGE_ACTIVE_TURN_UNCONFIRMED")?;
    let row=&rows[0];
    if row["status"].as_str()!=Some("inProgress"){return Err("BRIDGE_ACTIVE_TURN_UNCONFIRMED".into());}
    row["id"].as_str().filter(|id|!id.is_empty()&&id.len()<=256).map(str::to_owned).ok_or("BRIDGE_ACTIVE_TURN_UNCONFIRMED".into())
}
fn steer_turn(adapter:&mut crate::codex::adapter::CodexAdapter,thread:&str,turn:&str,handoff:&str,text:&str)->Result<TurnStartResult,String>{
    let result=adapter.request("turn/steer",json!({"threadId":thread,"expectedTurnId":turn,"clientUserMessageId":handoff,"input":[{"type":"text","text":text}]}))?;
    if result["turnId"].as_str()!=Some(turn){return Err("BRIDGE_STEER_ACK_UNOBSERVED".into());}
    // Do not mark_turn_started: the preexisting external execution stays external.
    Ok(TurnStartResult{turn_id:turn.into()})
}
pub(crate) fn bind(
    core: &RouterCore,
    workstream: &str,
    revision: i64,
    mut decision: RoleBindingInput,
    mut execution: RoleBindingInput,
) -> Result<RoleBridge, String> {
    if decision.provider == execution.provider && decision.external_id == execution.external_id {
        return Err("BRIDGE_SAME_NATIVE_TARGET".into());
    }
    let mut urls = Vec::new();
    for input in [&mut decision, &mut execution] {
        match input.provider.as_str() {
            "CODEX" => {
                let mut s = core
                    .session
                    .lock()
                    .map_err(|_| "Router session unavailable")?;
                let a = s
                    .adapter
                    .as_mut()
                    .ok_or("Codex backend disconnected. Use Reconnect first.")?;
                let v = metadata(a, &input.external_id)?;
                input.cwd = Some(native_root(&v)?);
            }
            "CHATGPT" => {
                let url = input.external_id.clone();
                input.external_id = crate::chatgpt::direct::parse_explicit_conversation_url(&url)?;
                core.chatgpt.verify_exact(&input.external_id)?;
                input.cwd = None;
                urls.push((input.external_id.clone(), url));
            }
            _ => return Err("BRIDGE_PROVIDER_UNSUPPORTED".into()),
        }
    }
    let b = core
        .store
        .bind_role_bridge(workstream, revision, decision, execution)?;
    for (id, url) in urls {
        for side in [&b.decision, &b.execution].into_iter().flatten() {
            if side.endpoint.provider == "CHATGPT" && side.endpoint.external_id == id {
                core.store
                    .save_chatgpt_endpoint_canonical_url(&side.endpoint.id, &url)?;
            }
        }
    }
    Ok(b)
}
fn side(core: &RouterCore, workstream: &str, role: &str) -> Result<RoleEndpoint, String> {
    let b = core.store.role_bridge(workstream)?;
    match role {
        "DECISION" => b.decision,
        "EXECUTION" => b.execution,
        _ => return Err("BRIDGE_ROLE_INVALID".into()),
    }
    .ok_or("BRIDGE_SIDE_NOT_BOUND".into())
}
pub(crate) fn read(core: &RouterCore, workstream: &str, role: &str) -> Result<BridgeState, String> {
    let side = side(core, workstream, role)?;
    let (identity, text,completed) = match side.endpoint.provider.as_str() {
        "CODEX" => {
            let mut s = core
                .session
                .lock()
                .map_err(|_| "Router session unavailable")?;
            let a = s
                .adapter
                .as_mut()
                .ok_or("Codex backend disconnected. Use Reconnect first.")?;
            let checked=metadata(a,&side.endpoint.external_id)?;verify_root(&checked,&side)?;
            let externally_unconfirmed=!a.is_shared()||checked.pointer("/thread/status/type").and_then(Value::as_str)==Some("notLoaded");
            let latest = read_latest_codex_reply(a, &side.endpoint.external_id)?;
            let r = match latest.reply{Some(reply)=>reply,None=>return read_without_new_reply(core,workstream,role,&side.endpoint.id,if externally_unconfirmed&&latest.state=="LATEST_TURN_INTERRUPTED"{"EXTERNAL_STATUS_UNCONFIRMED"}else{latest.state})};
            core.store.save_codex_reply_observer_watermark(
                workstream,
                &side.endpoint.id,
                &side.endpoint.external_id,
                &r.completed_turn_id,
                &r.agent_item_id,
            )?;
            (
                format!("codex:{}:{}", r.completed_turn_id, r.agent_item_id),
                r.text,
                r.completed_at,
            )
        }
        "CHATGPT" => {
            let r = core.chatgpt.observe_exact(&side.endpoint.external_id)?;
            (r.message_id, r.text,None)
        }
        _ => return Err("BRIDGE_PROVIDER_UNSUPPORTED".into()),
    };
    core.store.record_reply_observation(
        workstream,
        &side.endpoint.id,
        Some(&identity),
        &text,
        None,
    )?;
    core.store.note_reply_completion(workstream,&side.endpoint.id,&identity,completed)?;
    state(core, workstream)
}
fn read_without_new_reply(core:&RouterCore,workstream:&str,role:&str,endpoint:&str,outcome:&str)->Result<BridgeState,String>{
    if !matches!(outcome,"LATEST_TURN_INTERRUPTED"|"LATEST_TURN_ACTIVE"|"NO_NEW_TERMINAL_REPLY"|"OBSERVATION_MATERIALIZATION_PENDING"|"EXTERNAL_STATUS_UNCONFIRMED"){return Err(outcome.into());}
    let mut value=state(core,workstream)?;let retained=value.replies.iter().any(|r|r.endpoint_id==endpoint);
    value.read_outcome=Some(BridgeReadOutcome{role:role.into(),endpoint_id:endpoint.into(),state:outcome.into(),retained_reply:retained});Ok(value)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AttachmentOption {
    pub(crate) id: String,
    pub(crate) filename: String,
    pub(crate) sha256: Option<String>,
    pub(crate) size: Option<u64>,
}
fn observation(
    core: &RouterCore,
    workstream: &str,
    role: &str,
    id: &str,
) -> Result<(RoleEndpoint, ReplyObservation), String> {
    let src = side(core, workstream, role)?;
    let obs = core
        .store
        .reply_observations_for_workstream(workstream)?
        .into_iter()
        .find(|r| r.id == id && r.endpoint_id == src.endpoint.id)
        .ok_or("BRIDGE_SOURCE_OBSERVATION_CHANGED")?;
    Ok((src, obs))
}
pub(crate) fn blocks(core: &RouterCore, workstream: &str, role: &str, id: &str) -> Result<Vec<crate::chatgpt::model::RelayTextBlock>, String> {
    let (_, obs) = observation(core, workstream, role, id)?;
    Ok(crate::chatgpt::model::relay_text_blocks(&obs.text))
}
fn codex_candidates(
    src: &RoleEndpoint,
    obs: &ReplyObservation,
) -> Result<Vec<crate::artifact::detector::AttachmentCandidate>, String> {
    let root = std::fs::canonicalize(src.cwd.as_deref().ok_or("BRIDGE_CODEX_ROOT_REQUIRED")?)
        .map_err(|_| "BRIDGE_CODEX_ROOT_UNAVAILABLE")?;
    let mut result = Vec::new();
    for mut candidate in crate::artifact::detector::detect(&obs.id, &obs.text) {
        let raw = std::path::Path::new(&candidate.raw_path);
        let path = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            root.join(raw)
        };
        let Ok(path) = std::fs::canonicalize(path) else {
            continue;
        };
        if !path.starts_with(&root) || !path.is_file() {
            continue;
        }
        let hash = sha256_path(&path)?;
        candidate.exists = true;
        candidate.is_file = true;
        candidate.normalized_path = Some(path.to_string_lossy().into_owned());
        candidate.size = std::fs::metadata(&path).ok().map(|m| m.len());
        candidate.actual_sha256 = Some(hash.clone());
        candidate.integrity_status = if candidate
            .declared_sha256
            .as_deref()
            .is_some_and(|h| !h.eq_ignore_ascii_case(&hash))
        {
            crate::artifact::detector::IntegrityStatus::Mismatch
        } else {
            crate::artifact::detector::IntegrityStatus::Verified
        };
        result.push(candidate);
    }
    Ok(result)
}
pub(crate) fn attachments(
    core: &RouterCore,
    workstream: &str,
    role: &str,
    id: &str,
) -> Result<Vec<AttachmentOption>, String> {
    let (src, obs) = observation(core, workstream, role, id)?;
    if src.endpoint.provider == "CHATGPT" {
        let native = obs
            .assistant_identity
            .ok_or("BRIDGE_SOURCE_IDENTITY_MISSING")?;
        return Ok(core
            .chatgpt
            .list_attachments(&src.endpoint.external_id, &native)?
            .into_iter()
            .map(|r| AttachmentOption {
                id: r.resource_id,
                filename: r.filename,
                sha256: None,
                size: None,
            })
            .collect());
    }
    {
        let mut session = core
            .session
            .lock()
            .map_err(|_| "Router session unavailable")?;
        let adapter = session
            .adapter
            .as_mut()
            .ok_or("Codex backend disconnected. Use Reconnect first.")?;
        verify_root(&metadata(adapter, &src.endpoint.external_id)?, &src)?;
    }
    Ok(codex_candidates(&src, &obs)?
        .into_iter()
        .map(|r| AttachmentOption {
            id: r.id,
            filename: r.filename,
            sha256: r.actual_sha256,
            size: r.size,
        })
        .collect())
}
pub(crate) fn prepare(
    core: &RouterCore,
    workstream: &str,
    role: &str,
    observation_id: &str,
    text: &str,
    selected: &[String],
) -> Result<HandoffHistoryItem, String> {
    let (src, obs) = observation(core, workstream, role, observation_id)?;
    let available = attachments(core, workstream, role, observation_id)?;
    let mut chosen = std::collections::HashSet::new();
    let mut files = Vec::new();
    let candidates = if src.endpoint.provider == "CODEX" {
        codex_candidates(&src, &obs)?
    } else {
        Vec::new()
    };
    for id in selected {
        if !chosen.insert(id) || !available.iter().any(|a| &a.id == id) {
            return Err("BRIDGE_ATTACHMENT_SELECTION_CHANGED".into());
        }
        if src.endpoint.provider == "CHATGPT" {
            let native = obs
                .assistant_identity
                .as_deref()
                .ok_or("BRIDGE_SOURCE_IDENTITY_MISSING")?;
            let captured =
                core.chatgpt
                    .materialize_attachment(&src.endpoint.external_id, native, id)?;
            files.push(crate::chatgpt::model::HandoffAttachment {
                id: id.clone(),
                filename: captured.filename,
                path: captured.path,
                actual_sha256: Some(captured.sha256),
                integrity_status: Some("VERIFIED".into()),
            });
        } else {
            let a = candidates
                .iter()
                .find(|a| &a.id == id)
                .ok_or("BRIDGE_ATTACHMENT_SELECTION_CHANGED")?;
            if matches!(
                a.integrity_status,
                crate::artifact::detector::IntegrityStatus::Mismatch
                    | crate::artifact::detector::IntegrityStatus::Error
            ) {
                return Err("BRIDGE_ATTACHMENT_HASH_MISMATCH".into());
            }
            files.push(crate::chatgpt::model::HandoffAttachment {
                id: id.clone(),
                filename: a.filename.clone(),
                path: a
                    .normalized_path
                    .clone()
                    .ok_or("BRIDGE_ATTACHMENT_PATH_UNAVAILABLE")?,
                actual_sha256: a.actual_sha256.clone(),
                integrity_status: Some("VERIFIED".into()),
            });
        }
    }
    let persisted = files.iter().map(persisted_attachment).collect();
    core.store.prepare_role_handoff_with_attachments(
        workstream,
        role,
        observation_id,
        text,
        persisted,
    )
}
fn handoff_files(
    h: &HandoffHistoryItem,
) -> Result<Vec<crate::chatgpt::model::HandoffAttachment>, String> {
    let mut files = Vec::new();
    for a in &h.attachments {
        if a.sha256.as_deref()
            != Some(sha256_path(std::path::Path::new(&a.original_path))?.as_str())
        {
            return Err("BRIDGE_ATTACHMENT_CHANGED_AFTER_REVIEW".into());
        }
        files.push(crate::chatgpt::model::HandoffAttachment {
            id: a.id.clone(),
            filename: a.filename.clone(),
            path: a.original_path.clone(),
            actual_sha256: a.sha256.clone(),
            integrity_status: a.integrity_status.clone(),
        });
    }
    Ok(files)
}
pub(crate) fn approve(
    core: &RouterCore,
    handoff: &str,
    expected_hash: &str,
) -> Result<HandoffHistoryItem, String> {
    let h = core.store.role_handoff(handoff)?;
    handoff_files(&h)?;
    core.store
        .approve_role_handoff_checked(handoff, expected_hash)
}

pub(crate) fn verify_handoff_files(core:&RouterCore,handoff:&str)->Result<(),String>{handoff_files(&core.store.role_handoff(handoff)?).map(|_|())}

pub(crate) fn preclaim<T>(core: &RouterCore, h: &HandoffHistoryItem, stage: &str, result: Result<T,String>) -> Result<T,String> {
    result.map_err(|cause| {
        // Never overwrite SENDING/SENT/UNKNOWN or alter an approval/hash.
        let _ = core.store.record_role_preclaim_failure(&h.id,&h.payload_hash,stage,&cause);
        format!("BRIDGE_PRECLAIM_{stage}: {cause}")
    })
}
pub(crate) fn send(core: &RouterCore, handoff: &str) -> Result<BridgeState, String> {
    let preview = core.store.role_handoff(handoff)?;
    if preview.status != "APPROVED" {
        return Err("BRIDGE_NOT_APPROVED_OR_ALREADY_ATTEMPTED".into());
    }
    let destination = preview.destination_endpoint.clone();
    let mut files = preclaim(core,&preview,"FILES",handoff_files(&preview))?;
    if destination.provider == "CHATGPT" {
        let h = core.store.claim_role_handoff(handoff)?;
        let run = core.store.create_chatgpt_dispatch_run(
            &h.workstream_id,
            &destination.id,
            Some(&h.id),
        )?;
        match crate::chatgpt_direct_writer::send_handoff(
            core,
            &h.workstream_id,
            &destination,
            &run.id,
            &h.id,
            &h.approved_text,
            &files,
        ) {
            Ok(_) => {}
            Err(e) if e == "CHATGPT_ACCEPTED_PENDING_TERMINAL" => {}
            Err(e) => return Err(e),
        }
    } else {
        preclaim(core,&preview,"WATCH_WRITER",core.store.require_no_watch_reply_writer(&destination.external_id))?;
        let bindings = preclaim(core,&preview,"BINDING",core.store.role_bridge(&preview.workstream_id))?;
        let target = [bindings.decision, bindings.execution]
            .into_iter()
            .flatten()
            .find(|s| s.endpoint.id == destination.id)
            .ok_or("BRIDGE_BINDING_CHANGED").map_err(String::from);
        let target=preclaim(core,&preview,"BINDING",target)?;
        let (mut adapter, needs_acquisition) = {
            let s = core
                .session
                .lock()
                .map_err(|_| "Router session unavailable".to_string());
            let mut s=preclaim(core,&preview,"ADAPTER",s)?;
            let needs = thread_needs_write_acquisition(&s.ready_threads, &destination.external_id);
            let adapter=preclaim(core,&preview,"ADAPTER",s.adapter.take().ok_or_else(||"Codex backend disconnected. Use Reconnect first.".into()))?;
            s.codex_adapter_borrowed=true;
            (adapter,needs)
        };
        let result = (|| {
            let native = preclaim(core,&preview,"TARGET_METADATA",metadata(&mut adapter, &destination.external_id))?;
            preclaim(core,&preview,"TARGET_ROOT",verify_root(&native, &target))?;
            let loaded_shared=adapter.is_shared()&&matches!(native.pointer("/thread/status/type").and_then(Value::as_str),Some("active"|"idle"));
            if !loaded_shared {
                preclaim(core,&preview,"ACQUISITION",ensure_adapter_thread_ready_for_write(&mut adapter,needs_acquisition,&destination.external_id))?;
                let goal=preclaim(core,&preview,"GOAL",adapter.get_goal(&destination.external_id))?;
                preclaim(core,&preview,"GOAL",require_idle_goal(&destination.external_id,&goal))?;
            }
            let current=preclaim(core,&preview,"TARGET_REFRESH",metadata(&mut adapter,&destination.external_id))?;
            preclaim(core,&preview,"TARGET_ROOT",verify_root(&current,&target))?;
            let steer=match current.pointer("/thread/status/type").and_then(Value::as_str){
                Some("active")=>Some(preclaim(core,&preview,"ACTIVE_TURN",exact_active_turn(&mut adapter,&destination.external_id))?),
                Some("idle")=>None,
                _=>return preclaim(core,&preview,"ACTIVE_TURN",Err("BRIDGE_ACTIVE_TURN_UNCONFIRMED".into())),
            };
            if steer.is_none(){preclaim(core,&preview,"RECONCILIATION",crate::runtime_reconciler::RuntimeReconciler::reconcile_codex_workstream(&core.store,&mut adapter,&preview.workstream_id))?;}
            let h = preclaim(core,&preview,"CLAIM",match &steer {
                Some(turn)=>core.store.claim_role_handoff_steer(handoff,&destination.external_id,turn),
                None=>core.store.claim_role_handoff(handoff),
            })?;
            let staged = (|| -> Result<(), String> {
                if !files.is_empty() {
                    let root = std::fs::canonicalize(
                        target.cwd.as_deref().ok_or("BRIDGE_CODEX_ROOT_REQUIRED")?,
                    )
                    .map_err(|_| "BRIDGE_CODEX_ROOT_UNAVAILABLE")?;
                    for ancestor in [root.join(".aiwr"), root.join(".aiwr/incoming")] {
                        if let Ok(meta) = std::fs::symlink_metadata(&ancestor) {
                            if meta.file_type().is_symlink() {
                                return Err("BRIDGE_ATTACHMENT_DIRECTORY_LINK".into());
                            }
                            #[cfg(windows)]
                            {
                                use std::os::windows::fs::MetadataExt;
                                if meta.file_attributes() & 0x400 != 0 {
                                    return Err("BRIDGE_ATTACHMENT_DIRECTORY_LINK".into());
                                }
                            }
                        }
                    }
                    stage_reverse_attachment_files(&root, &h.id, &mut files)?;
                    core.store.record_attachment_send_evidence(
                        &h.id,
                        &files
                            .iter()
                            .map(|f| crate::persistence::AttachmentSendEvidence {
                                attachment_id: f.id.clone(),
                                review_sha256: f.actual_sha256.clone(),
                                integrity_status: f.integrity_status.clone(),
                                send_sha256: f.actual_sha256.clone().unwrap(),
                            })
                            .collect::<Vec<_>>(),
                    )?;
                }
                Ok(())
            })();
            if let Err(error) = staged {
                core.store.transition_handoff(
                    &h.id,
                    "FAILED",
                    Some(("BRIDGE_ATTACHMENT_PREWRITE_FAILED".into(), error.clone())),
                )?;
                return Err(error);
            }
            let run = if steer.is_some(){None}else{Some(match core.store.create_provider_run(
                &h.workstream_id,
                &destination.id,
                "CODEX",
                Some(&h.id),
                None,
                "STARTING",
            ) {
                Ok(run) => run,
                Err(error) => {
                    core.store.transition_handoff(
                        &h.id,
                        "FAILED",
                        Some(("BRIDGE_PROVIDER_PREWRITE_FAILED".into(), error.clone())),
                    )?;
                    return Err(error);
                }
            })};
            // SENDING persists before the single physical call. Lost acknowledgement
            // remains unresolved; no retry, expiry or UI refresh can repeat it.
            let delivery=match &steer {
                Some(turn)=>steer_turn(&mut adapter,&destination.external_id,turn,&h.id,&h.approved_text),
                None=>start_turn(&mut adapter,&destination.external_id,&h.approved_text),
            };
            let ack = match delivery {
                Ok(ack) => ack,
                Err(error) => {
                    let _ = core.store.record_sending_handoff_uncertainty(
                        &h.id,
                        "CODEX_ACK_UNOBSERVED",
                        "Delivery may have occurred; inspect the original exact thread. No resend.",
                    );
                    return Err(error);
                }
            };
            core.store.note_mcp_native_cause(&h.id,&destination.external_id,&ack.turn_id)?;
            if let Some(run)=run{core.store.attach_provider_run_external_identity(&run.id,"CODEX",&ack.turn_id)?;}
            core.store.transition_handoff(&h.id, "SENT", None)?;
            Ok(())
        })();
        restore_codex_adapter(
            core,
            adapter,
            result.is_ok().then_some(destination.external_id.as_str()),
        )?;
        result?;
    }
    state(core, &preview.workstream_id)
}

#[cfg(test)]
#[path = "role_bridge_tests.rs"]
mod tests;

#[derive(Clone,Serialize)]#[serde(rename_all="camelCase")]
pub(crate) struct DirectoryActivity { pub workstream_id:String,pub binding_revision:i64,pub unread_count:usize,pub latest_role:Option<String>,pub sides:Vec<RoleActivity>,pub assistant_processed:Option<router_core::store::assistant_processed::AssistantProcessed> }
/// Bounded read-only status projection. Does not read transcript bodies, start a
/// watch, load cold conversations, or navigate an authenticated browser.
pub(crate) fn poll_directory_activity(store:&RouterStore,session_handle:&Mutex<Session>,ids:&[String])->Result<Vec<DirectoryActivity>,String>{
 if ids.len()>20||ids.iter().any(|id|id.is_empty()||id.len()>128){return Err("BRIDGE_ACTIVITY_REQUEST_INVALID".into());}
 let mut result=Vec::new();let started=std::time::Instant::now();let mut session=session_handle.try_lock().ok();
 for id in ids{
  let w=store.snapshot_for_workstream(id)?.workstreams.into_iter().find(|w|w.id==*id).ok_or("BRIDGE_UNAVAILABLE")?;
  if w.trashed_at.is_some()||w.archived_at.is_some(){continue;}
  let b=store.role_bridge(id)?;let mut sides=Vec::new();
  for side in [&b.decision,&b.execution].into_iter().flatten(){
   let pending=session.as_ref().is_some_and(|s|s.pending_codex_requests.values().any(|r|r.thread_id==side.endpoint.external_id&&!r.responded));
   let mut a=RoleActivity{role:side.role.clone(),endpoint_id:side.endpoint.id.clone(),state:"UNCONFIRMED".into(),checked_at:0,turn_id:None,result_observation_id:None,goal_status:None,turn_active:false,latest_turn:None,goal:None,goal_controls:None};
   if side.endpoint.provider=="CODEX"&&started.elapsed()<std::time::Duration::from_secs(4){
    if let Some(adapter)=session.as_mut().and_then(|s|s.adapter.as_mut()).filter(|a|!a.is_closed()){
     if let Ok(native)=adapter.read_thread_activity(&side.endpoint.external_id){
      let exact=side.cwd.as_ref().is_some_and(|cwd|std::fs::canonicalize(cwd).ok().zip(std::fs::canonicalize(&native.cwd).ok()).is_some_and(|(a,b)|a==b));
      if exact&&native.thread_id==side.endpoint.external_id&&native.authoritative{
       a.state=native.state.into();a.turn_id=native.turn_id;a.goal_status=native.goal_status;a.turn_active=native.turn_active;a.checked_at=activity_now();
       a.latest_turn=adapter.latest_turn_diagnostic(&side.endpoint.external_id).filter(|d|a.turn_id.as_deref()==Some(&d.turn_id));
       if let Ok(raw)=adapter.request_with_timeout("thread/goal/get",json!({"threadId":side.endpoint.external_id}),std::time::Duration::from_millis(750)){
        if let Ok(Some(mut goal))=codex_goal_from_response(&side.endpoint.external_id,&raw){
         goal.active_turn_id=adapter.active_turn_id(&side.endpoint.external_id);a.goal_status=Some(goal.status.clone());
         a.goal_controls=crate::goal_control::controls_for_store(store,&goal,a.turn_active,pending).ok();a.goal=Some(goal);
        }
       }
       if a.state=="RUNNING"&&pending{a.state="ACTION_REQUIRED".into();}
       else if a.state=="RUNNING"{if let Some(turn)=a.turn_id.as_ref(){
        if let Ok(items)=adapter.request_with_timeout("thread/items/list",json!({"threadId":side.endpoint.external_id,"turnId":turn,"cursor":null,"limit":1,"sortDirection":"desc"}),std::time::Duration::from_millis(500)){
         if items["data"][0]["turnId"].as_str()==Some(turn)&&items["data"][0]["item"]["type"]=="reasoning"{a.state="THINKING".into();}
        }
       }}
       if a.state=="COMPLETE"{
        let prefix=a.turn_id.as_ref().map(|t|format!("codex:{t}:"));
        a.result_observation_id=store.reply_observations_for_workstream(id)?.into_iter().find(|r|r.endpoint_id==side.endpoint.id&&prefix.as_ref().is_some_and(|p|r.assistant_identity.as_ref().is_some_and(|i|i.starts_with(p)))).map(|r|r.id);
        if a.result_observation_id.is_none(){a.state="RESULT_PENDING".into();}
       }
      }
     }
    }
   }
   sides.push(a);
  }
  // A rebind racing this read invalidates the entire row instead of displaying
  // an old role's activity under a new recipient.
  let replies=store.reply_observations_for_workstream(id)?;
  let latest_role=replies.iter().filter(|r|[&b.decision,&b.execution].into_iter().flatten().any(|side|side.endpoint.id==r.endpoint_id)).max_by_key(|r|r.completed_at.unwrap_or(r.observed_at)).and_then(|r|[&b.decision,&b.execution].into_iter().flatten().find(|side|side.endpoint.id==r.endpoint_id).map(|side|side.role.clone()));
  if store.role_bridge(id)?.binding_revision==b.binding_revision{result.push(DirectoryActivity{workstream_id:id.clone(),binding_revision:b.binding_revision,latest_role,assistant_processed:store.assistant_processed(id,activity_now() as i64)?,unread_count:replies.iter().filter(|r|r.read_at.is_none()&&r.handled_at.is_none()&&[&b.decision,&b.execution].into_iter().flatten().any(|side|side.endpoint.id==r.endpoint_id)).count(),sides});}
 }
 Ok(result)
}

// Weak store ownership keeps copied/test databases isolated and does not keep a
// closed Host store alive. This lock is independent of native RPC's session lock.
#[derive(Default)]struct ResidentActivity{epoch:u64,rows:HashMap<String,DirectoryActivity>,wanted:HashMap<String,(i64,u64)>}
type ActivityCache=Arc<Mutex<ResidentActivity>>;
static RESIDENT_ACTIVITY:std::sync::LazyLock<Mutex<Vec<(std::sync::Weak<RouterStore>,ActivityCache)>>>=std::sync::LazyLock::new(||Mutex::new(Vec::new()));
fn activity_cache(store:&Arc<RouterStore>)->ActivityCache{
 let mut entries=RESIDENT_ACTIVITY.lock().unwrap_or_else(|e|e.into_inner());entries.retain(|(w,_)|w.strong_count()>0);
 if let Some((_,cache))=entries.iter().find(|(w,_)|w.upgrade().is_some_and(|s|Arc::ptr_eq(&s,store))){return cache.clone();}
 let cache=Arc::new(Mutex::new(ResidentActivity::default()));entries.push((Arc::downgrade(store),cache.clone()));cache
}
/// Only an existing UI SYNC request authorizes cold transcript hydration. All
/// metadata polling is independent of the renderer and never starts a turn.
pub(crate) fn refresh_resident_activity(store:&Arc<RouterStore>,session:&Arc<Mutex<Session>>,epoch:u64,cursor:&mut usize){
 let cache=activity_cache(store);let Ok(workstreams)=store.active_workstreams()else{return};let mut ids=workstreams.into_iter().filter_map(|w|store.role_bridge(&w.id).ok().filter(|b|b.explicit_roles).map(|_|w.id)).collect::<Vec<_>>();ids.sort();
 if !session.try_lock().is_ok_and(|s|s.codex_observer_epoch==epoch&&s.adapter.as_ref().is_some_and(|a|!a.is_closed())){return;}
 let allowed=ids.iter().cloned().collect::<HashSet<_>>();if let Ok(mut c)=cache.lock(){if c.epoch!=epoch{c.rows.clear();c.epoch=epoch;}c.rows.retain(|id,_|allowed.contains(id));c.wanted.retain(|id,(_,at)|allowed.contains(id)&&activity_now().saturating_sub(*at)<20*60*1000);}
 if ids.is_empty(){return;}let offset=*cursor%ids.len();ids.rotate_left(offset);let batch=ids.into_iter().take(20).collect::<Vec<_>>();
 let Ok(rows)=poll_directory_activity(store,session,&batch)else{return};*cursor=offset+rows.iter().take_while(|r|r.sides.iter().any(|a|a.checked_at>0)).count().max(1);
 if !session.try_lock().is_ok_and(|s|s.codex_observer_epoch==epoch){return;}
 let wanted=cache.lock().map(|c|c.wanted.clone()).unwrap_or_default();
 for row in rows{
  for side in row.sides.iter().filter(|s|s.state=="RESULT_PENDING"){
   if wanted.get(&row.workstream_id).is_some_and(|(rev,_)|*rev==row.binding_revision){let _=materialize_requested_reply(store,session,&row.workstream_id,row.binding_revision,&side.endpoint_id);}
  }
  if row.sides.iter().any(|a|a.checked_at>0){if let Ok(mut c)=cache.lock(){if c.epoch==epoch{c.rows.insert(row.workstream_id.clone(),row);}}}
 }
}
fn materialize_requested_reply(store:&RouterStore,session:&Arc<Mutex<Session>>,id:&str,revision:i64,endpoint:&str)->Result<(),String>{
 materialize_requested_reply_with(store,session,id,revision,endpoint,&mut |p|crate::push::send_payload(p))
}
fn materialize_requested_reply_with(store:&RouterStore,session:&Arc<Mutex<Session>>,id:&str,revision:i64,endpoint:&str,push:&mut dyn FnMut(&[u8])->Result<crate::push::PushDeliveryOutcome,String>)->Result<(),String>{
 let w=store.snapshot_for_workstream(id)?.workstreams.into_iter().find(|w|w.id==id).ok_or("BRIDGE_UNAVAILABLE")?;if w.trashed_at.is_some()||w.archived_at.is_some(){return Err("BRIDGE_UNAVAILABLE".into());}
 let b=store.role_bridge(id)?;if b.binding_revision!=revision{return Err("BRIDGE_BINDING_CHANGED".into());}let side=[&b.decision,&b.execution].into_iter().flatten().find(|s|s.endpoint.id==endpoint&&s.endpoint.provider=="CODEX").ok_or("BRIDGE_SIDE_NOT_BOUND")?;
 let reply={let mut s=session.lock().map_err(|_|"Router session unavailable")?;let a=s.adapter.as_mut().filter(|a|!a.is_closed()).ok_or("Codex disconnected")?;let checked=metadata(a,&side.endpoint.external_id)?;verify_root(&checked,side)?;read_latest_codex_reply(a,&side.endpoint.external_id)?.reply};
 let Some(r)=reply else{return Ok(())};if store.role_bridge(id)?.binding_revision!=revision{return Err("BRIDGE_BINDING_CHANGED".into());}
 // Validate root/read before notification; an initialized observer owns its
 // watermark and one push. UI hydration must neither consume nor rewind it.
 if store.codex_reply_observer_is_initialized(endpoint)?{
  check_new_codex_endpoint_replies_with(store,session,id,side.endpoint.clone(),push)?;
 }else{store.save_codex_reply_observer_watermark(id,endpoint,&side.endpoint.external_id,&r.completed_turn_id,&r.agent_item_id)?;}

 let identity=format!("codex:{}:{}",r.completed_turn_id,r.agent_item_id);store.record_reply_observation(id,endpoint,Some(&identity),&r.text,None)?;store.note_reply_completion(id,endpoint,&identity,r.completed_at)?;Ok(())
}
/// Fast local-store projection; opening a page never queues native metadata reads.
pub(crate) fn directory_activity(core:&RouterCore,ids:&[String])->Result<Vec<DirectoryActivity>,String>{
 if ids.len()>20||ids.iter().any(|id|id.is_empty()||id.len()>128){return Err("BRIDGE_ACTIVITY_REQUEST_INVALID".into());}
 let mut rows=Vec::new();for id in ids{let initial=state(core,id)?;let w=core.store.snapshot_for_workstream(id)?.workstreams.into_iter().find(|w|w.id==*id).ok_or("BRIDGE_UNAVAILABLE")?;if w.trashed_at.is_some()||w.archived_at.is_some(){continue;}
  let bindings=&initial.bindings;let epoch=core.session.try_lock().ok().map(|s|s.codex_observer_epoch);let cache=activity_cache(&core.store);let cached=cache.lock().ok().and_then(|c|c.rows.get(id).filter(|row|epoch.is_none_or(|e|e==c.epoch)&&row.binding_revision==bindings.binding_revision).cloned());
  let sides=[&bindings.decision,&bindings.execution].into_iter().flatten().map(|side|cached.as_ref().and_then(|r|r.sides.iter().find(|a|a.endpoint_id==side.endpoint.id&&a.role==side.role&&a.checked_at>0&&activity_now().saturating_sub(a.checked_at)<15000)).cloned().unwrap_or(RoleActivity{role:side.role.clone(),endpoint_id:side.endpoint.id.clone(),state:"UNCONFIRMED".into(),checked_at:0,turn_id:None,result_observation_id:None,goal_status:None,turn_active:false,latest_turn:None,goal:None,goal_controls:None})).collect();
  let latest_role=initial.replies.iter().filter(|r|[&bindings.decision,&bindings.execution].into_iter().flatten().any(|side|side.endpoint.id==r.endpoint_id)).max_by_key(|r|r.completed_at.unwrap_or(r.observed_at)).and_then(|r|[&bindings.decision,&bindings.execution].into_iter().flatten().find(|side|side.endpoint.id==r.endpoint_id).map(|side|side.role.clone()));
  let unread_count=initial.replies.iter().filter(|r|r.read_at.is_none()&&r.handled_at.is_none()&&[&bindings.decision,&bindings.execution].into_iter().flatten().any(|side|side.endpoint.id==r.endpoint_id)).count();rows.push(DirectoryActivity{workstream_id:id.clone(),binding_revision:bindings.binding_revision,unread_count,latest_role,sides,assistant_processed:initial.assistant_processed});
 }Ok(rows)
}
pub(crate) fn sync_cached(core:&RouterCore,id:&str)->Result<BridgeState,String>{
 let mut state=state(core,id)?;if state.bindings.explicit_roles{if let Ok(mut c)=activity_cache(&core.store).lock(){c.wanted.insert(id.into(),(state.bindings.binding_revision,activity_now()));}}if let Some(activity)=directory_activity(core,&[id.into()])?.into_iter().next(){state.snapshot_at=Some(activity.sides.iter().map(|s|s.checked_at).max().unwrap_or(0));state.activities=Some(activity.sides);}Ok(state)
}
