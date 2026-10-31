//! Derived Pi presentation in the incumbent reader. Native IDs remain Pi IDs.
use super::application;
use crate::host_application::RouterCore;
use router_core::store::{
    codex_watch::{CodexWatch, WatchSnapshot},
    pi::Task,
};
use serde_json::{json, Value};

pub(crate) fn message(entry: &Value, epoch: &str, order: usize) -> Value {
    let id = entry["id"].as_str().unwrap_or_default();
    let mut text = Vec::new();
    let mut image = 0;
    for block in entry["content"].as_array().into_iter().flatten() {
        match block["type"].as_str() {
            Some("text") => {
                if let Some(t) = block["text"].as_str() {
                    text.push(t.to_owned());
                }
            }
            Some("image") => {
                let ext = if block["mimeType"] == "image/png" {
                    "png"
                } else {
                    "jpg"
                };
                text.push(format!("![Pi 图片](pi-media/{id}/{image}.{ext})"));
                image += 1;
            }
            Some("toolCall") => text.push(format!(
                "{} · {}",
                "工具",
                block["name"].as_str().unwrap_or("tool")
            )),
            _ => {}
        }
    }
    let tool = entry["role"] == "toolResult";
    let timestamp = entry["timestamp"]
        .as_str()
        .and_then(|s| {
            time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
        })
        .map(|t| (t.unix_timestamp_nanos() / 1_000_000) as i64);
    json!({"id":id,"turnId":epoch,"role":if tool{"tool"}else{entry["role"].as_str().unwrap_or("assistant")},"kind":if tool{"TOOL"}else{"PUBLIC"},"toolName":entry["toolName"].as_str(),"isError":entry["isError"].as_bool(),"text":text.join("\n\n"),"timestamp":timestamp,"order":entry["order"].as_u64().unwrap_or(order as u64),"clientId":entry["clientId"].as_str()})
}
pub(crate) fn watch(task: &Task) -> Result<CodexWatch, String> {
    let native = task
        .native_session_id
        .as_ref()
        .ok_or("PI_SESSION_ID_UNCONFIRMED")?;
    let epoch = format!("pi-session:{native}");
    let last = task.result.as_ref().and_then(|r| r.get("lastAssistant"));
    let text = last
        .map(|e| {
            message(e, &epoch, 0)["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .unwrap_or_default();
    let state = match task.state.as_str() {
        "CREATING" | "EMPTY_LOST" => "UNKNOWN",
        "WAITING_DECISION" => "ACTION_REQUIRED",
        "PARTIAL" => "INCOMPLETE",
        "CANCELLED" => "INTERRUPTED",
        s => s,
    };
    Ok(CodexWatch {
        runtime:Some(json!({"model":task.selected.as_ref().map(|v|v.model.as_str()),"modelLabel":null,"effort":task.selected.as_ref().map(|v|v.thinking.as_str()),"source":if task.selected.is_some(){"SELECTED"}else{"DEFAULT"},"observedAt":task.updated_at})),
        thread_id: format!("pi:{native}"),
        label: task.label.clone(),
        cwd: task.directory.clone(),
        enabled: true,
        generation: task.generation,
        snapshot: WatchSnapshot {
            state: state.into(),
            turn_id: Some(epoch.clone()),
            item_id: last.and_then(|v| v["id"].as_str()).map(str::to_owned),
            text,
        },
        checked_at: task.updated_at,
        error_code: task.error_code.clone(),
        retry_after: 0,
        send_count: 0,
        shown_at: task.created_at,
        changed_at: task.updated_at,
    })
}
pub(crate) fn children(core: &RouterCore, id: &str, cwd: &str) -> Result<Vec<Value>, String> {
    let (provider, native) = if let Some(id) = id.strip_prefix("claude_code:") {
        ("CLAUDE_CODE", id)
    } else {
        ("CODEX", id)
    };
    let tasks = core.store.pi_tasks()?;
    Ok(tasks.iter().filter(|t|t.parent.provider==provider&&t.parent.external_id==native&&router_core::claude_code::same_root(&t.parent.cwd,cwd)).filter_map(|task|watch(task).ok().map(|watch|json!({"watch":watch,"settings":task.selected,"relation":"AGBRIO_CROSS_PROVIDER_DELEGATION"}))).collect())
}
pub(crate) fn history(core: &RouterCore, id: &str, cursor: Option<&str>) -> Result<Value, String> {
    let task = core.store.pi_task(id)?;
    let _ = application::read(core, id, None)?;
    let page = core.store.pi_public_history_page(&task.id, cursor, 50)?;
    let next = if page.len() == 50 {
        page.first().and_then(|v| v["id"].as_str())
    } else {
        None
    };
    let messages = page
        .iter()
        .enumerate()
        .map(|(index, item)| {
            message(
                item,
                &format!(
                    "pi-session:{}",
                    task.native_session_id.as_deref().unwrap_or_default()
                ),
                index,
            )
        })
        .collect::<Vec<_>>();
    Ok(json!({"messages":messages,"nextCursor":next,"coverage":"VERIFIED_PI_NATIVE_ENTRIES"}))
}
pub(crate) fn command(
    core: &RouterCore,
    command: crate::watch_chat::ChatCommand,
) -> Result<Value, String> {
    use crate::watch_chat::ChatCommand;
    let id = command.thread_id().to_owned();
    let task = core.store.pi_task(&id)?;
    match command {
        ChatCommand::Send {
            thread_id,
            id,
            generation,
            source_sequence,
            expected_turn_id,
            mode,
            queue_requested,
            text,
            options,
        } => {
            if let Some(sequence) = source_sequence {
                if core.store.codex_watch_event(sequence)?.thread_id != thread_id {
                    return Err("REPLY_TARGET_CHANGED_REFRESH".into());
                }
            }
            let group = format!(
                "pi-session:{}",
                task.native_session_id.as_deref().unwrap_or_default()
            );
            if expected_turn_id
                .as_deref()
                .is_some_and(|s| s != group && Some(s) != task.runtime_epoch.as_deref())
            {
                return Err("PI_RUNTIME_CHANGED_REFRESH".into());
            }
            application::send(
                core,
                &thread_id,
                &id,
                generation,
                &mode,
                queue_requested,
                text,
                &options,
            )
        }
        ChatCommand::History { thread_id, cursor } => history(core, &thread_id, cursor.as_deref()),
        ChatCommand::Upload {
            thread_id,
            name,
            data,
        } => {
            let _cache = crate::attachment_cache::guard()?;
            crate::watch_chat::upload(core, &thread_id, &name, &data)
        }
        ChatCommand::Options { .. } => Ok(
            json!({"models":[],"permissions":"PI_DIRECTORY_SCOPE","settings":task.selected,"selectionSource":task.selection_source}),
        ),
        ChatCommand::Receipt { id, .. } => Ok(core
            .store
            .pi_reply_views(&task.id)?
            .into_iter()
            .find(|r| r["id"] == id)
            .unwrap_or(Value::Null)),
        ChatCommand::Abandon { id, .. } => {
            Ok(json!({"cancelled":core.store.pi_abandon_unattempted(&task.id,&id)?}))
        }
        ChatCommand::AcknowledgeUnknown { id, confirmed, .. } => {
            core.store
                .pi_acknowledge_unknown(&task.id, &id, confirmed)?;
            Ok(json!({"ok":true,"delivery":"STILL_UNKNOWN","automaticRetry":false}))
        }
        ChatCommand::Stop { thread_id, turn_id } => application::stop(
            core,
            &thread_id,
            &uuid::Uuid::new_v4().to_string(),
            &turn_id,
        ),
        ChatCommand::Restore {
            thread_id,
            generation,
        } => application::restore(core, &thread_id, generation),
        ChatCommand::Cancel { .. } => {
            Err("PI_CANCEL_SINGLE_QUEUE_UNSUPPORTED_STOP_TASK_EXPLICITLY".into())
        }
        _ => Err("PI_OPERATION_NOT_SUPPORTED".into()),
    }
}
