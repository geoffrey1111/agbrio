//! Authenticated owner creation, shared by desktop and paired mobile.
use crate::host_application::RouterCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Input {
    pub request_id: String,
    pub provider: String,
    pub directory: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub text: String,
}
#[derive(Deserialize)]
#[serde(
    tag = "action",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub(crate) enum Command {
    Options,
    Create {
        input: Input,
    },
    Receipt {
        #[serde(rename = "requestId")]
        request_id: String,
    },
}
pub(crate) fn command_with_host(
    core: &RouterCore,
    host: &crate::HostRuntime,
    input: Command,
) -> Result<Value, String> {
    command(core, input)
}
pub(crate) fn command(core: &RouterCore, input: Command) -> Result<Value, String> {
    match input {
        Command::Options => {
            let models = core.codex_creation_models();
            let mut projects = std::collections::BTreeSet::new();
            for w in core.store.codex_watches()? {
                if !w.cwd.is_empty() && std::path::Path::new(&w.cwd).is_dir() {
                    projects.insert(w.cwd);
                }
            }
            let claude:Result<Value,String> = Err("CLAUDE_DESKTOP_NATIVE_CREATION_PENDING".into());
            let antigravity = crate::antigravity::models();
            Ok(
                json!({"providers":[{"id":"CODEX","label":"Codex","available":models.is_ok(),"reason":models.as_ref().err(),"models":models.as_ref().ok().map(|v|&v["models"])},{"id":"CLAUDE_CODE","label":"Claude Code","available":claude.is_ok(),"reason":claude.as_ref().err(),"models":claude.as_ref().ok().map(|v|&v["models"])},{"id":"ANTIGRAVITY","label":"Antigravity","available":antigravity.is_ok(),"reason":antigravity.as_ref().err(),"models":antigravity.as_ref().ok().map(|v|&v["models"])}],"projects":projects,"models":models.as_ref().ok().map(|v|&v["models"]).unwrap_or(&Value::Null)}),
            )
        }
        Command::Receipt { request_id } => {
            serde_json::to_value(core.store.owner_conversation_creation(&request_id)?)
                .map_err(|_| "CONVERSATION_CREATION_RESPONSE_INVALID".into())
        }
        Command::Create { input } => {
            // Repeated requests are receipt reads even if provider state has changed.
            // Retain the original payload: a removed directory, changed symlink target
            // or disconnected provider cannot turn an admitted request into NOT_STARTED.
            let payload =
                serde_json::to_value(&input).map_err(|_| "CONVERSATION_CREATION_INPUT_INVALID")?;
            if let Some(receipt) = core
                .store
                .matching_owner_conversation_creation(&input.request_id, &payload)?
            {
                return serde_json::to_value(receipt)
                    .map_err(|_| "CONVERSATION_CREATION_RESPONSE_INVALID".into());
            }
            if !matches!(
                input.provider.as_str(),
                "CODEX" | "CLAUDE_CODE" | "ANTIGRAVITY"
            ) {
                return Err("CONVERSATION_CREATION_PROVIDER_UNAVAILABLE_NOT_STARTED".into());
            }
            if input.provider=="CLAUDE_CODE"{return Err("CONVERSATION_CREATION_CLAUDE_DESKTOP_PENDING_NOT_STARTED".into());}
            if input.text.trim().is_empty() || input.text.len() > 100000 {
                return Err("CONVERSATION_CREATION_INSTRUCTION_INVALID_NOT_STARTED".into());
            }
            let directory = crate::assistant_operations::create_directory(&input.directory)
                .map_err(|_| "CONVERSATION_CREATION_PROJECT_UNAVAILABLE_NOT_STARTED")?;
            let settings = crate::codex::creation::Settings {
                model: input.model.clone(),
                effort: input.effort.clone(),
                service_tier: None,
            };
            match input.provider.as_str() {
                "CODEX" => core.validate_codex_creation_settings(&settings),
                "CLAUDE_CODE" => crate::claude_creation::preflight(
                    &directory,
                    input.model.as_deref(),
                    input.effort.as_deref(),
                ),
                _ => crate::antigravity::preflight(input.model.as_deref(), input.effort.as_deref()),
            }
            .map_err(|_| "CONVERSATION_CREATION_SETTINGS_UNAVAILABLE_NOT_STARTED")?;
            let (receipt, claimed) = core
                .store
                .claim_owner_conversation_creation(&input.request_id, &payload)?;
            if !claimed {
                return serde_json::to_value(receipt)
                    .map_err(|_| "CONVERSATION_CREATION_RESPONSE_INVALID".into());
            }
            if input.provider == "CLAUDE_CODE" {
                return crate::claude_creation::create(core, input, directory);
            }
            let result = (|| -> Result<Value, String> {
                if input.provider == "ANTIGRAVITY" {
                    let mut created = crate::antigravity::create_empty(
                        core,
                        &input.request_id,
                        &directory,
                        input.model.as_deref(),
                        input.effort.as_deref(),
                    )?;
                    let key = created["threadId"]
                        .as_str()
                        .ok_or("ANTIGRAVITY_NATIVE_IDENTITY_UNCONFIRMED")?;
                    let generation = created["watch"]["generation"]
                        .as_i64()
                        .ok_or("ANTIGRAVITY_NATIVE_IDENTITY_UNCONFIRMED")?;
                    let delivery = crate::antigravity::send(
                        core,
                        key,
                        &input.request_id,
                        generation,
                        "SEND",
                        input.text,
                        Default::default(),
                        false,
                    )?;
                    created["firstMessageSent"] = json!(delivery["status"] == "SENT");
                    created["delivery"] = delivery;
                    return Ok(created);
                }
                let created = core.start_configured_codex_thread(&directory, &settings)?;
                let id = created
                    .pointer("/thread/id")
                    .and_then(Value::as_str)
                    .filter(|s| uuid::Uuid::parse_str(s).is_ok())
                    .ok_or("CONVERSATION_CREATION_NATIVE_ID_UNCONFIRMED")?;
                core.store
                    .checkpoint_owner_conversation_created(&input.request_id, id)?;
                crate::codex::creation::verify(&created, &settings)?;
                core.store.enable_codex_watch(
                    id,
                    "新 Codex 对话",
                    &directory,
                    &router_core::store::codex_watch::WatchSnapshot {
                        state: "IDLE".into(),
                        turn_id: None,
                        item_id: None,
                        text: String::new(),
                    },
                )?;
                let watch = core.store.codex_chat_context(id)?;
                let delivery = crate::watch_chat::command(
                    core,
                    crate::watch_chat::ChatCommand::Send {
                        thread_id: id.into(),
                        id: input.request_id.clone(),
                        generation: watch.generation,
                        source_sequence: None,
                        expected_turn_id: None,
                        mode: "SEND".into(),
                        queue_requested: false,
                        text: input.text,
                        options: crate::watch_chat::ReplyOptions::default(),
                    },
                )?;
                Ok(
                    json!({"threadId":id,"watch":watch,"delivery":delivery,"firstMessageSent":delivery["status"]=="SENT","automaticRetry":false}),
                )
            })();
            let receipt=match result{
   Ok(value)=>core.store.finish_owner_conversation_creation(&input.request_id,&value,false)?,
   Err(_)=>core.store.finish_owner_conversation_creation(&input.request_id,&json!({"error":"CONVERSATION_CREATION_UNCONFIRMED_CHECK_ORIGINAL_RECEIPT","automaticRetry":false}),true)?,
  };
            serde_json::to_value(receipt)
                .map_err(|_| "CONVERSATION_CREATION_RESPONSE_INVALID".into())
        }
    }
}

#[cfg(test)]
#[path = "conversation_creation_live.rs"]
mod live;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    fn fixture() -> (tempfile::TempDir, RouterCore) {
        let d = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::RouterStore::open_at(d.path().join("router.sqlite")).unwrap());
        let core = RouterCore {
            store,
            session: Arc::new(Mutex::new(Default::default())),
        };
        (d, core)
    }
    #[test]
    fn unavailable_provider_is_not_silently_replaced_and_creates_no_intent() {
        let (d, core) = fixture();
        let id = uuid::Uuid::new_v4().to_string();
        for provider in ["UNSUPPORTED", "INVALID"] {
            let result = command(
                &core,
                Command::Create {
                    input: Input {
                        request_id: id.clone(),
                        provider: provider.into(),
                        directory: d.path().to_string_lossy().into_owned(),
                        model: None,
                        effort: None,
                        text: "fixture: never sent".into(),
                    },
                },
            );
            assert!(result.unwrap_err().ends_with("NOT_STARTED"));
            assert!(core.store.owner_conversation_creation(&id).is_err());
        }
    }
    #[test]
    fn invalid_project_or_unavailable_model_cannot_start_a_native_creation() {
        let (d, core) = fixture();
        let id = uuid::Uuid::new_v4().to_string();
        let input = |directory: String| Input {
            request_id: id.clone(),
            provider: "CODEX".into(),
            directory,
            model: Some("fixture-model".into()),
            effort: None,
            text: "fixture: never sent".into(),
        };
        assert!(command(
            &core,
            Command::Create {
                input: input(d.path().join("absent").to_string_lossy().into_owned())
            }
        )
        .unwrap_err()
        .ends_with("NOT_STARTED"));
        assert!(command(
            &core,
            Command::Create {
                input: input(d.path().to_string_lossy().into_owned())
            }
        )
        .unwrap_err()
        .ends_with("NOT_STARTED"));
        assert!(core.store.owner_conversation_creation(&id).is_err());
    }
    #[test]
    fn admitted_creation_returns_original_receipt_even_when_project_and_provider_are_unavailable() {
        let (d, core) = fixture();
        let id = uuid::Uuid::new_v4().to_string();
        let input = || Input {
            request_id: id.clone(),
            provider: "CODEX".into(),
            directory: d
                .path()
                .join("no-longer-present")
                .to_string_lossy()
                .into_owned(),
            model: Some("fixture-unavailable".into()),
            effort: None,
            text: "fixture: never sent".into(),
        };
        let payload = serde_json::to_value(input()).unwrap();
        core.store
            .claim_owner_conversation_creation(&id, &payload)
            .unwrap();
        core.store
            .checkpoint_owner_conversation_created(&id, "original-native")
            .unwrap();
        core.store
            .finish_owner_conversation_creation(&id, &json!({"automaticRetry":false}), true)
            .unwrap();
        let r = command(&core, Command::Create { input: input() }).unwrap();
        assert_eq!(r["status"], "UNKNOWN");
        assert_eq!(r["nativeId"], "original-native");
        let mut changed = input();
        changed.text.push_str(" changed");
        assert_eq!(
            command(&core, Command::Create { input: changed }).unwrap_err(),
            "CONVERSATION_CREATION_REQUEST_CHANGED"
        );
    }
}
