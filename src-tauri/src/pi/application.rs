//! Pi operations below IPC/MCP. No caller-selectable executables, env or RPC.
use super::{permissions, session::plain, settings, transport::Rpc};
use crate::host_application::RouterCore;
use router_core::store::pi::{Parent, Selection, Task};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateInput {
    pub scope_id: String,
    pub scope_generation: i64,
    #[serde(default)] pub access_roots: Option<Vec<String>>,
    #[serde(default)] pub writable: Option<bool>,
    pub parent: Parent,
    pub model_provider: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
}
pub(crate) fn normalize_directory(value: &str) -> Result<String, String> {
    Ok(plain(&permissions::canonical_directory(value)?))
}
pub(crate) fn owner_settings(core: &RouterCore) -> Result<Value, String> {
    let installation = super::installation();
    let (version, error) = match installation {
        Ok(i) => (Some(i.version), None),
        Err(e) => (None, Some(e)),
    };
    Ok(
        json!({"administratorBackend":super::cohort::status(),"installed":version.is_some(),"version":version,"installationError":error,"assistantGrants":core.store.assistant_grants()?.into_iter().filter(|g|g.revoked_at.is_none()&&g.scope=="INSTANCE"&&g.expires_at>std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()as i64).map(|g|json!({"id":g.id,"label":g.label})).collect::<Vec<_>>(),"scopes":core.store.pi_scopes()?.into_iter().map(|scope|{let mut v=serde_json::to_value(&scope).unwrap();v["policy"]=serde_json::to_value(core.store.pi_workspace_policy(&scope.id).ok().flatten()).unwrap();v}).collect::<Vec<_>>(),"tasks":overview(core)?,"automaticRouting":"NOT_INSTALLED","directoryGrant":"DESKTOP_OWNER_ONLY"}),
    )
}
pub(crate) fn owner_authorize(
    core: &RouterCore,
    root: &str,
    writable: bool,
) -> Result<Value, String> {
    let root = permissions::authorize_root(root)?;
    let pi = core
        .session
        .lock()
        .map_err(|_| "PI_SERVICE_UNAVAILABLE")?
        .pi
        .clone();
    if let Some(scope) = core.store.pi_scopes()?.into_iter().find(|s| s.root == root) {
        if core.store.pi_workspace_policy(&scope.id)?.is_some(){return Err("PI_WORKSPACE_USE_POLICY_EDITOR".into());}
        if (scope.writable != writable || !scope.enabled)
            && pi.scope_in_use(&core.store, &scope.id)?
        {
            return Err("PI_SCOPE_IN_USE_STOP_TASKS_FIRST".into());
        }
    }
    core.store.pi_authorize_directory(&root, writable, false)?;
    owner_settings(core)
}
pub(crate) fn owner_revoke(core: &RouterCore, id: &str, generation: i64) -> Result<Value, String> {
    core.store.pi_revoke_scope(id, generation)?;
    let pi = core
        .session
        .lock()
        .map_err(|_| "PI_SERVICE_UNAVAILABLE")?
        .pi
        .clone();
    pi.stop_scope(&core.store, id)?;
    owner_settings(core)
}
pub(crate) fn overview(core: &RouterCore) -> Result<Vec<Value>, String> {
    Ok(core.store.pi_tasks()?.iter().map(|t|json!({"taskId":t.id,"threadId":t.native_session_id.as_ref().map(|s|format!("pi:{s}")),"label":t.label,"directory":t.directory,"generation":t.generation,"parent":t.parent,"state":t.state,"settings":t.selected,"selectionSource":t.selection_source,"updatedAt":t.updated_at})).collect())
}
impl CreateInput {
    pub fn selection(&self) -> Result<Option<Selection>, String> {
        match (&self.model_provider, &self.model, &self.thinking) {
            (None, None, None) => Ok(None),
            (Some(provider), Some(model), Some(thinking))
                if matches!(provider.as_str(), "openai" | "anthropic")
                    && !model.is_empty()
                    && model.len() < 256
                    && !thinking.is_empty()
                    && thinking.len() < 32 =>
            {
                Ok(Some(Selection {
                    provider: provider.clone(),
                    model: model.clone(),
                    thinking: thinking.clone(),
                }))
            }
            _ => Err("PI_EXPLICIT_SELECTION_REQUIRES_PROVIDER_MODEL_THINKING".into()),
        }
    }
}
pub(crate) fn validate_create(
    core: &RouterCore,
    directory: &str,
    input: &CreateInput,
) -> Result<(), String> {
    input.selection()?;
    let scope = core.store.pi_scope(&input.scope_id)?;
    if !scope.enabled || scope.generation != input.scope_generation {
        return Err("PI_SCOPE_CHANGED".into());
    }
    let access=super::workspaces::subset(core,&scope,directory,input.access_roots.as_deref(),input.writable,None)?;
    if access.administrator{super::cohort::require_ready()?;}
    let id = match input.parent.provider.as_str() {
        "CODEX" => input.parent.external_id.clone(),
        "CLAUDE_CODE" => format!("claude_code:{}", input.parent.external_id),
        _ => return Err("PI_PARENT_INVALID".into()),
    };
    if uuid::Uuid::parse_str(&input.parent.external_id).is_err() {
        return Err("PI_PARENT_INVALID".into());
    }
    let context = core.store.codex_chat_context(&id)?;
    if permissions::canonical_directory(&context.cwd)?
        != permissions::canonical_directory(&input.parent.cwd)?
    {
        return Err("PI_PARENT_CONTEXT_CHANGED".into());
    }
    Ok(())
}
pub(crate) fn create(
    core: &RouterCore,
    action: &str,
    directory: &str,
    label: &str,
    input: CreateInput,
) -> Result<Value, String> {
    validate_create(core, directory, &input)?;
    let grant = core.store.pi_create_action_grant(action)?;
    let selected = input.selection()?;
    let scope=core.store.pi_scope(&input.scope_id)?;
    let access=super::workspaces::subset(core,&scope,directory,input.access_roots.as_deref(),input.writable,Some(&grant))?;
    let task = core.store.pi_begin_create(
        action,
        &grant,
        &input.scope_id,
        input.scope_generation,
        directory,
        label,
        input.parent,
        selected.as_ref(),
    )?;
    core.store.pi_bind_task_access(&task.id,task.scope_generation,&access)?;
    // Task identity is durable before native launch. Retrying this same action
    // returns the exact original row; it never creates another native session.
    core.store.checkpoint_assistant_created_chat(action,&json!({"provider":"PI","taskId":task.id,"firstMessageSent":false,"registration":"CREATING","automaticRetry":false}))?;
    let pi = core
        .session
        .lock()
        .map_err(|_| "PI_SERVICE_UNAVAILABLE")?
        .pi
        .clone();
    let task = match pi.start(core.store.clone(), &task.id, false) {
        Ok(t) => t,
        Err(error) => {
            let current = core.store.pi_task(&task.id)?;
            core.store.pi_set_state(
                &task.id,
                if current.native_session_id.is_some() {
                    "UNKNOWN"
                } else {
                    "FAILED"
                },
                None,
                Some(&error),
            )?;
            return Err(error);
        }
    };
    let mut result = creation_receipt(&task);
    result["execution"]=pi.audit(&task.id).unwrap_or(Value::Null);
    result["registration"] = json!("PENDING");
    core.store
        .pi_checkpoint_created_action(action, &task.id, &result)?;
    let watch = super::presentation::watch(&task)?;
    core.store
        .enable_codex_watch(&watch.thread_id, &watch.label, &watch.cwd, &watch.snapshot)?;
    let parent_id=if task.parent.provider=="CLAUDE_CODE"{format!("claude_code:{}",task.parent.external_id)}else{task.parent.external_id.clone()};
    if core.store.registered_codex_watch(&parent_id).is_err(){let parent=core.store.codex_chat_context(&parent_id)?;core.store.enable_codex_watch(&parent_id,&parent.label,&parent.cwd,&parent.snapshot)?;}
    result["registration"] = json!("REGISTERED");
    core.store
        .pi_checkpoint_created_action(action, &task.id, &result)?;
    Ok(result)
}
fn creation_receipt(task: &Task) -> Value {
    let id = task.native_session_id.as_ref().map(|id| format!("pi:{id}"));
    json!({"provider":"PI","taskId":task.id,"threadId":id,"generation":task.generation,"directory":task.directory,"parent":task.parent,"nativeSessionId":task.native_session_id,"settings":task.selected,"selectionSource":task.selection_source,"durability":if task.file_verified{"VERIFIED_NATIVE_FILE"}else{"EMPTY_SESSION_NOT_YET_DURABLE"},"firstMessageSent":false,"automaticRetry":false,"relationshipSource":"REVIEWED_AGBRIO_ASSOCIATION","nativeCallerAttested":false,"nativeRetryPolicy":"INHERITED_PI_CONFIGURATION","nextAction":"SEND_CHAT","nextActionInput":{"threadId":id,"generation":task.generation,"mode":"AUTO","sourceSequence":null,"expectedTurnId":null}})
}
pub(crate) fn read_compact(core:&RouterCore,id:&str,cursor:Option<&str>)->Result<Value,String>{
    // Refresh the native actor through the same read-only path; omit tool transcripts.
    let pi=core.session.lock().map_err(|_|"PI_SERVICE_UNAVAILABLE")?.pi.clone();
    let task=core.store.pi_task(id)?;let _=pi.history(&task.id);
    let task=core.store.pi_task(id)?;
    let failure=if task.state=="FAILED"&&task.file_verified{match(task.native_session_file.as_deref(),task.native_session_id.as_deref(),task.result.as_ref().and_then(|r|r["lastAssistant"]["id"].as_str())){(Some(file),Some(native),Some(entry))=>{super::session::verify_session_header(std::path::Path::new(file),native,std::path::Path::new(&task.directory))?;super::history::failure_metadata(std::path::Path::new(file),entry)},_=>None}}else{None};
    let messages=core.store.pi_assistant_history_page(&task.id,cursor,50)?;
    let next=messages.first().and_then(|v|v["id"].as_str());
    let result=task.result.as_ref().map(|r|json!({"nativeTerminalReceiptId":r["nativeTerminalReceiptId"],"piRunId":r["piRunId"],"completedAt":r["completedAt"],"leafId":r["leafId"],"hadToolErrors":r["hadToolErrors"],"lastAssistant":{"id":r["lastAssistant"]["id"],"stopReason":r["lastAssistant"]["stopReason"],"errorCode":r["lastAssistant"]["errorCode"]}}));
    Ok(json!({"provider":"PI","failure":failure,"task":{"id":task.id,"nativeSessionId":task.native_session_id,"generation":task.generation,"result":result,"runtimeEpoch":task.runtime_epoch,"parent":task.parent,"directory":task.directory,"state":task.state,"selected":task.selected,"errorCode":task.error_code},"threadId":id,"generation":task.generation,"state":task.state,"errorCode":task.error_code,"selected":task.selected,"nativeBranch":core.store.pi_branch_info(&task.id)?,"resultMatchesActiveBranch":task.result.is_some()&&task.error_code.as_deref()!=Some("PI_NATIVE_BRANCH_CHANGED"),"runtimeLoaded":pi.loaded(&task.id),"execution":pi.audit(&task.id).map(|a|json!({"contextApplied":a["contextApplied"],"process":a["process"],"capabilities":a["capabilities"]})),"questions":core.store.pi_owner_questions(&task.id)?,"publicMessages":messages,"history":{"messages":messages,"nextCursor":next,"direction":"OLDER","view":"ASSISTANT_TEXT"},"sendContext":{"threadId":id,"generation":task.generation,"mode":"AUTO","sourceSequence":null,"expectedTurnId":null}}))
}
pub(crate) fn read(core: &RouterCore, id: &str, cursor: Option<&str>) -> Result<Value, String> {
    let task = core.store.pi_task(id)?;
    let pi = core
        .session
        .lock()
        .map_err(|_| "PI_SERVICE_UNAVAILABLE")?
        .pi
        .clone();
    let live = pi.history(&task.id);
    let refresh = if live.is_ok() {
        "LIVE"
    } else {
        "PERSISTED_ONLY"
    };
    let native_busy = live.as_ref().ok().and_then(|v| v["nativeBusy"].as_bool());
    let task = core.store.pi_task(&task.id)?;
    let messages = core.store.pi_public_history(&task.id, cursor, 50)?;
    let next = messages.last().and_then(|v| v["id"].as_str());
    let mut execution=core.store.pi_execution_audit(&task.id)?.or_else(||pi.audit(&task.id));if let Some(v)=execution.as_mut(){v["runtimeLoaded"]=json!(pi.loaded(&task.id));}
    Ok(
        json!({"provider":"PI","task":task,"coverage":refresh,"nativeBusy":native_busy,"history":{"messages":messages,"nextCursor":next,"hasMore":messages.len()==50},"sendContext":{"threadId":id,"generation":task.generation,"mode":"AUTO","sourceSequence":null,"expectedTurnId":null},"nativeBranch":core.store.pi_branch_info(&task.id)?,"resultMatchesActiveBranch":task.result.is_some()&&task.error_code.as_deref()!=Some("PI_NATIVE_BRANCH_CHANGED"),"runtimeLoaded":pi.loaded(&task.id),"execution":execution,"terminalApproval":"OWNER_ONLY","resultConsumption":"READ_DOES_NOT_PROVE_MANAGER_CONSUMPTION"}),
    )
}
pub(crate) fn send(
    core: &RouterCore,
    id: &str,
    request: &str,
    generation: i64,
    mode: &str,
    queue_requested: bool,
    text: String,
    options: &crate::watch_chat::ReplyOptions,
) -> Result<Value, String> {
    let _cache = crate::attachment_cache::guard()?;
    if (text.trim().is_empty() && options.attachments.is_empty())
        || text.trim_start().starts_with('/')
        || text.contains('\0')
    {
        return Err("PI_INPUT_INVALID".into());
    }
    if options.model.is_some() || options.effort.is_some() {
        return Err("PI_SEND_INHERITS_CREATION_SETTINGS".into());
    }
    if !matches!(mode, "AUTO" | "SEND" | "STEER" | "QUEUE") {
        return Err("PI_SEND_MODE_INVALID".into());
    }
    let mode = if mode == "QUEUE" { "QUEUE" } else { "AUTO" };
    if mode == "QUEUE" && !queue_requested {
        return Err("PI_QUEUE_REQUIRES_EXPLICIT_CHOICE".into());
    }
    let task = core.store.pi_task(id)?;
    if task.generation != generation {
        return Err("PI_TASK_GENERATION_CHANGED".into());
    }
    let pi = core
        .session
        .lock()
        .map_err(|_| "PI_SERVICE_UNAVAILABLE")?
        .pi
        .clone();
    let images =
        crate::watch_chat::pi_attachment_images(&super::presentation::watch(&task)?, options)?;
    let _=core.store.observe_pi_manager_notices();
    let result = pi.send(
        core.store.clone(),
        &task.id,
        request.into(),
        generation,
        mode.into(),
        text,
        images,
    )?;
    let mut receipt = core
        .store
        .pi_reply_views(&task.id)?
        .into_iter()
        .find(|v| v["id"] == request)
        .ok_or("PI_COMMAND_RECEIPT_UNAVAILABLE")?;
    receipt["provider"] = json!("PI");
    receipt["completed"] = json!(false);
    receipt["deliveryReceipt"] = result["receipt"].clone();
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "real installed Pi metadata via reviewed MCP; disposable parent fixture, no model task"]
    fn reviewed_mcp_creates_once_and_keeps_parent_scope_and_effective_settings() {
        let root =
            std::path::PathBuf::from(std::env::var_os("AGBRIO_PI_QA_ROOT").expect("owned QA root"));
        let d = tempfile::tempdir_in(root).unwrap();
        let project = d.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let project = plain(&project);
        let store =
            Arc::new(router_core::store::RouterStore::open_at(d.path().join("router.db")).unwrap());
        let core = RouterCore {
            store: store.clone(),
            session: Arc::default(),
            chatgpt: Arc::default(),
            completed_chatgpt_responses: Arc::default(),
        };
        let parent = uuid::Uuid::new_v4().to_string();
        store
            .enable_codex_watch(
                &parent,
                "DISPOSABLE MCP PARENT FIXTURE",
                &project,
                &router_core::store::codex_watch::WatchSnapshot {
                    state: "IDLE".into(),
                    turn_id: None,
                    item_id: None,
                    text: String::new(),
                },
            )
            .unwrap();
        owner_authorize(&core, &project, false).unwrap();
        let scope = store.pi_scopes().unwrap().remove(0);
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let grant = store
            .connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput {
                label: "Pi metadata MCP fixture".into(),
                expires_at: at + 600000,
            })
            .unwrap();
        let call = |name: &str, args: Value| {
            crate::assistant_mcp::call(&core, &grant.id, name, args).unwrap()
        };
        assert!(call("agbrio_list_pi_models", json!({}))["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["provider"] == "openai" && m["id"] == "gpt-6-luna"));
        let action = call(
            "agbrio_prepare_action",
            json!({"requestId":"create-pi-metadata-once","operation":"CREATE_CHAT","input":{"provider":"PI","directory":project,"label":"Pi create-only QA","pi":{"scopeId":scope.id,"scopeGeneration":scope.generation,"parent":{"provider":"CODEX","externalId":parent,"cwd":project},"modelProvider":"openai","model":"gpt-6-luna","thinking":"low"}}}),
        );
        let approve = json!({"actionId":action["id"],"expectedHash":action["payloadHash"],"ruleId":null,"useOwnerAnswer":false,"assessment":"Owner-authorized isolated metadata acceptance; no first task or live parent prompt."});
        let executed = call("agbrio_execute_action", approve.clone());
        assert_eq!(executed["status"], "APPLIED");
        let result = &executed["result"];
        assert_eq!(result["provider"], "PI");
        assert_eq!(result["firstMessageSent"], false);
        assert_eq!(result["settings"]["thinking"], "low");
        assert_eq!(result["parent"]["externalId"], parent);
        assert_eq!(call("agbrio_execute_action", approve), executed);
        assert_eq!(store.pi_tasks().unwrap().len(), 1);
        let read = call("agbrio_read_chat", json!({"threadId":result["threadId"]}));
        assert_eq!(read["history"]["messages"].as_array().unwrap().len(), 0);
        assert_eq!(read["task"]["state"], "IDLE");
        owner_revoke(&core, &scope.id, scope.generation).unwrap();
        assert!(!store.pi_scope(&scope.id).unwrap().enabled);
        core.shutdown_owned_adapter();
    }
    #[test]
    #[ignore = "paid product MCP/upload/vision/media/restore acceptance; explicit isolated QA only"]
    fn real_product_upload_mcp_two_provider_results_and_media() {
        use base64ct::Encoding;
        assert_eq!(std::env::var("AGBRIO_PI_PAID_QA").as_deref(), Ok("1"));
        let root =
            std::path::PathBuf::from(std::env::var_os("AGBRIO_PI_QA_ROOT").expect("owned QA root"));
        let d = tempfile::tempdir_in(&root).unwrap();
        let store =
            Arc::new(router_core::store::RouterStore::open_at(d.path().join("router.db")).unwrap());
        let core = RouterCore {
            store: store.clone(),
            session: Arc::default(),
            chatgpt: Arc::default(),
            completed_chatgpt_responses: Arc::default(),
        };
        let manager = d.path().join("manager");
        std::fs::create_dir(&manager).unwrap();
        let manager = plain(&manager);
        let parent = uuid::Uuid::new_v4().to_string();
        store
            .enable_codex_watch(
                &parent,
                "DISPOSABLE PARENT FIXTURE, NOT GENUINE MANAGER",
                &manager,
                &router_core::store::codex_watch::WatchSnapshot {
                    state: "IDLE".into(),
                    turn_id: None,
                    item_id: None,
                    text: String::new(),
                },
            )
            .unwrap();
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let grant = store
            .connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput {
                label: "Pi product API acceptance fixture".into(),
                expires_at: at + 600000,
            })
            .unwrap();
        let call = |name: &str, args: Value| {
            crate::assistant_mcp::call(&core, &grant.id, name, args).unwrap()
        };
        let reviewed = |request: &str, operation: &str, input: Value| {
            let action = call(
                "agbrio_prepare_action",
                json!({"requestId":request,"operation":operation,"input":input}),
            );
            let result = call(
                "agbrio_execute_action",
                json!({"actionId":action["id"],"expectedHash":action["payloadHash"],"ruleId":null,"useOwnerAnswer":false,"assessment":"Owner-authorized disposable API/provider acceptance, preserving live owner chats."}),
            );
            assert_eq!(
                result["status"], "APPLIED",
                "{operation}: {}",
                result["errorCode"]
            );
            result
        };
        let mut cases = Vec::new();
        for (provider, model, ext) in [
            ("openai", "gpt-6-luna", "png"),
            ("anthropic", "claude-haiku-5-5", "jpg"),
        ] {
            let project = d.path().join(provider);
            std::fs::create_dir(&project).unwrap();
            let project = plain(&project);
            owner_authorize(&core, &project, false).unwrap();
            let scope = store
                .pi_scopes()
                .unwrap()
                .into_iter()
                .find(|s| router_core::claude_code::same_root(&s.root, &project))
                .unwrap();
            let token = format!("PRODUCT_{}", uuid::Uuid::new_v4().simple());
            std::fs::write(std::path::Path::new(&project).join("fixture.txt"), &token).unwrap();
            let code = uuid::Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase();
            let picture = d.path().join(format!("{provider}.{ext}"));
            crate::pi::live_tests::image(&code, &picture);
            let bytes = std::fs::read(&picture).unwrap();
            let created = reviewed(
                &format!("create-{provider}"),
                "CREATE_CHAT",
                json!({"provider":"PI","directory":project,"label":format!("Pi {provider} full API QA"),"pi":{"scopeId":scope.id,"scopeGeneration":scope.generation,"parent":{"provider":"CODEX","externalId":parent,"cwd":manager},"modelProvider":provider,"model":model,"thinking":"low"}}),
            );
            let id = created["result"]["threadId"].as_str().unwrap().to_owned();
            let generation = created["result"]["generation"].as_i64().unwrap();
            let upload = crate::watch_chat::command(
                &core,
                crate::watch_chat::ChatCommand::Upload {
                    thread_id: id.clone(),
                    name: format!("vision.{ext}"),
                    data: base64ct::Base64::encode_string(&bytes),
                },
            )
            .unwrap();
            let sent = reviewed(
                &format!("send-{provider}"),
                "SEND_CHAT",
                json!({"threadId":id,"generation":generation,"mode":"AUTO","text":"Use read to read fixture.txt. Read the six-character code in the attached image. Return only the exact file contents, a vertical bar, and the image code. Do not call other tools.","options":{"attachments":[upload["id"]]}}),
            );
            cases.push((
                provider,
                model,
                id,
                generation,
                token,
                code,
                bytes,
                sent["id"].as_str().unwrap().to_owned(),
            ));
        }
        let mut proofs = Vec::new();
        for (provider, model, id, generation, token, code, bytes, request) in &cases {
            let deadline = std::time::Instant::now() + Duration::from_secs(180);
            let result = loop {
                let current = call("agbrio_read_chat", json!({"threadId":id}));
                if matches!(
                    current["task"]["state"].as_str(),
                    Some("RESULT_READY" | "FAILED" | "PARTIAL" | "CANCELLED")
                ) {
                    break current;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "provider did not settle"
                );
                std::thread::sleep(Duration::from_millis(100));
            };
            assert_eq!(
                result["task"]["state"], "RESULT_READY",
                "{provider} did not produce a complete native result"
            );
            assert_eq!(result["task"]["fileVerified"], true);
            let actual = &result["task"]["result"]["lastAssistant"];
            assert_eq!(actual["provider"], *provider);
            assert_eq!(actual["model"], *model);
            assert_eq!(actual["providerThinkingLevel"], "low");
            let history = crate::pi::presentation::history(&core, id, None).unwrap();
            let messages = history["messages"].as_array().unwrap();
            for (other, _, _, _, other_token, _, _, _) in &cases {
                if other != provider {
                    assert!(
                        !history.to_string().contains(other_token),
                        "cross-session token leak"
                    );
                }
            }
            assert!(messages
                .iter()
                .any(|m| m["role"] == "tool" && m["toolName"] == "read"));
            assert!(messages.iter().any(|m| m["role"] == "assistant"
                && m["text"]
                    .as_str()
                    .is_some_and(|s| s.contains(token) && s.contains(code))));
            let image = messages
                .iter()
                .find(|m| {
                    m["role"] == "user"
                        && m["text"].as_str().is_some_and(|s| s.contains("pi-media/"))
                })
                .unwrap();
            assert_eq!(image["clientId"], *request);
            let image_id = image["id"].as_str().unwrap();
            let source = format!(
                "pi-media/{image_id}/0.{}",
                if *provider == "openai" { "png" } else { "jpg" }
            );
            let media = crate::message_media::read(
                &core,
                crate::message_media::MediaInput {
                    scope: crate::message_media::MediaScope::Watch {
                        thread_id: id.clone(),
                        sequence: None,
                        turn_id: image["turnId"].as_str().map(str::to_owned),
                        item_id: Some(image_id.into()),
                    },
                    source,
                    user_requested: false,
                    document_sources: Vec::new(),
                },
            )
            .unwrap();
            assert_eq!(base64ct::Base64::decode_vec(&media.data).unwrap(), *bytes);
            let task = store.pi_task(id).unwrap();
            assert_eq!(
                store.pi_command_receipt(&task.id, request).unwrap().status,
                "CONSUMED"
            );
            proofs.push(json!({"provider":provider,"model":model,"thinking":"low","taskId":task.id,"threadId":id,"generation":generation,"nativeImageEntry":image_id,"nativeFileVerified":true,"visionAndReadMatched":true,"mediaBytesMatched":true,"genuineManager":false,"scope":"AUTOMATED_DISPOSABLE_APPLICATION_API"}));
        }
        assert_eq!(
            crate::pi::presentation::children(&core, &parent, &manager)
                .unwrap()
                .len(),
            2
        );
        core.session.lock().unwrap().pi.shutdown();
        for (_, _, id, generation, _, _, _, _) in &cases {
            let before = store.pi_task(id).unwrap();
            let restored = restore(&core, id, *generation).unwrap();
            assert_eq!(
                restored["nativeSessionId"],
                before.native_session_id.as_deref().unwrap()
            );
            assert!(restored["generation"].as_i64().unwrap() > *generation);
            assert_eq!(
                store.pi_task(id).unwrap().native_session_file,
                before.native_session_file
            );
        }
        core.shutdown_owned_adapter();
        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../runtime/local-safety-20261028/pi-product-provider-proof.json"),
            serde_json::to_vec_pretty(&proofs).unwrap(),
        )
        .unwrap();
        eprintln!("Real product application API: two providers, upload/persist/native vision+read, public tools/media, native receipts and original restore PASS; not genuine manager/UAT proof");
    }
}
pub(crate) fn stop(
    core: &RouterCore,
    id: &str,
    request: &str,
    epoch: &str,
) -> Result<Value, String> {
    let task = core.store.pi_task(id)?;
    if task.runtime_epoch.as_deref() != Some(epoch) {
        return Err("PI_RUNTIME_CHANGED_REFRESH".into());
    }
    let pi = core
        .session
        .lock()
        .map_err(|_| "PI_SERVICE_UNAVAILABLE")?
        .pi
        .clone();
    pi.close_task(&core.store, &task.id, request, task.generation)
}
#[derive(Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub(crate) struct AccessChange{pub generation:i64,pub scope_id:String,pub scope_generation:i64,#[serde(default)]pub access_roots:Option<Vec<String>>,#[serde(default)]pub writable:Option<bool>}
pub(crate) fn validate_access_change(core:&RouterCore,id:&str,input:&AccessChange)->Result<(),String>{
 let task=core.store.pi_task(id)?;if task.generation!=input.generation{return Err("PI_TASK_GENERATION_CHANGED".into());}
 let scope=core.store.pi_scope(&input.scope_id)?;if !scope.enabled||scope.generation!=input.scope_generation{return Err("PI_SCOPE_CHANGED".into());}
 super::workspaces::subset(core,&scope,&task.directory,input.access_roots.as_deref(),input.writable,None)?;
 if core.session.lock().map_err(|_|"PI_SERVICE_UNAVAILABLE")?.pi.loaded(&task.id){return Err("PI_CLOSE_BEFORE_ACCESS_CHANGE".into());}Ok(())
}
pub(crate) fn restore_managed(core:&RouterCore,action:&str,id:&str,change:Option<AccessChange>)->Result<Value,String>{
 let grant=core.store.pi_reviewed_action_grant(action,"ENABLE_WATCH")?;let task=core.store.pi_task(id)?;
 if let Some(change)=change{
  validate_access_change(core,id,&change)?;let scope=core.store.pi_scope(&change.scope_id)?;
  let access=super::workspaces::subset(core,&scope,&task.directory,change.access_roots.as_deref(),change.writable,Some(&grant))?;
  let pi=core.session.lock().map_err(|_|"PI_SERVICE_UNAVAILABLE")?.pi.clone();
  let task=pi.reconfigure_and_restore(core.store.clone(),&task.id,change.generation,&scope.id,scope.generation,&access)?;
  return Ok(json!({"provider":"PI","threadId":id,"generation":task.generation,"restored":true,"firstMessageSent":false,"settings":task.selected,"execution":pi.audit(&task.id)}));
 }
 let scope=core.store.pi_scope(&task.scope_id)?;
 super::workspaces::subset(core,&scope,&task.directory,None,None,Some(&grant))?;
 restore(core,id,task.generation)
}
pub(crate) fn restore(core: &RouterCore, id: &str, generation: i64) -> Result<Value, String> {
    let task = core.store.pi_task(id)?;
    if task.generation != generation {
        return Err("PI_TASK_GENERATION_CHANGED".into());
    }
    let pi = core
        .session
        .lock()
        .map_err(|_| "PI_SERVICE_UNAVAILABLE")?
        .pi
        .clone();
    let task = pi.start(core.store.clone(), &task.id, true)?;
    let watch = super::presentation::watch(&task)?;
    core.store
        .enable_codex_watch(&watch.thread_id, &watch.label, &watch.cwd, &watch.snapshot)?;
    Ok(
        json!({"restored":true,"taskId":task.id,"nativeSessionId":task.native_session_id,"generation":task.generation,"firstMessageSent":false}),
    )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CatalogInput {
    pub model_provider: Option<String>,
    pub model: Option<String>,
}
pub(crate) fn catalog(core: &RouterCore, input: CatalogInput) -> Result<Value, String> {
    static DISCOVERY: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _discovery = DISCOVERY
        .try_lock()
        .map_err(|_| "PI_METADATA_DISCOVERY_BUSY")?;
    if input.model_provider.is_some() != input.model.is_some() {
        return Err("PI_DISCOVERY_REQUIRES_PROVIDER_AND_MODEL".into());
    }
    let root = core.store.pi_storage_root()?.join("catalog");
    std::fs::create_dir_all(&root).map_err(|_| "PI_CATALOG_UNAVAILABLE")?;
    let root = permissions::canonical_directory(&plain(&root))?;
    let installation = super::installation()?;
    let rpc = Rpc::start(
        &installation.node,
        &super::args(&installation, &root),
        &root,
        &plain(&root),
        false,
        Arc::new(|_| {}),
    )?;
    let models = settings::call(&rpc, json!({"type":"get_available_models"}))?;
    let models=models["models"].as_array().ok_or("PI_MODEL_CATALOG_INVALID")?.iter().filter(|m|matches!(m["provider"].as_str(),Some("openai"|"anthropic"))).map(|m|json!({"provider":m["provider"],"id":m["id"],"name":m["name"],"reasoning":m["reasoning"],"input":m["input"],"contextWindow":m["contextWindow"],"maxTokens":m["maxTokens"],"costEstimate":m["cost"],"callVerified":false})).collect::<Vec<_>>();
    let state = settings::call(&rpc, json!({"type":"get_state"}))?;
    let selected = settings::actual(&state).ok();
    if let (Some(provider), Some(model)) = (&input.model_provider, &input.model) {
        if !models
            .iter()
            .any(|v| v["provider"] == *provider && v["id"] == *model)
        {
            return Err("PI_MODEL_NOT_AVAILABLE".into());
        }
        settings::call(
            &rpc,
            json!({"type":"set_model","provider":provider,"modelId":model}),
        )?;
    }
    let levels = settings::call(&rpc, json!({"type":"get_available_thinking_levels"}))?;
    let effective = settings::call(&rpc, json!({"type":"get_state"}))?;
    let discovery = settings::actual(&effective).ok();
    rpc.close(Duration::from_secs(5))?;
    Ok(
        json!({"provider":"PI","version":installation.version,"models":models,"currentDefault":selected,"selectedForDiscovery":discovery,"thinkingLevels":levels["levels"],"automaticRouting":"NOT_INSTALLED","catalogIsCallProof":false,"instructions":"Optional modelProvider/model discovers that exact model's native thinkingLevels without a task or global setting write. Select exact modelProvider/model/thinking for CREATE_CHAT.pi. Creation rechecks thinking levels after selecting the model; partial explicit selections reject. Omit all three to use the current verified native DEFAULT."}),
    )
}
