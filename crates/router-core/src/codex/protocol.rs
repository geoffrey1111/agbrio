use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

static EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedEvent {
    pub id: String,
    pub kind: FeedKind,
    pub method: String,
    pub item_id: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub text: Option<String>,
    pub detail: Option<String>,
    pub raw: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub enum FeedKind {
    UserMessage,
    AgentMessage,
    Progress,
    Warning,
    Completion,
    Unknown,
}
impl FeedEvent {
    fn new(
        kind: FeedKind,
        method: &str,
        item_id: Option<String>,
        thread_id: Option<String>,
        turn_id: Option<String>,
        text: Option<String>,
        detail: Option<String>,
        raw: Option<String>,
    ) -> Self {
        Self {
            id: format!(
                "event:{}:{}:{}",
                method,
                item_id.clone().unwrap_or_default(),
                EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ),
            kind,
            method: method.into(),
            item_id,
            thread_id,
            turn_id,
            text,
            detail,
            raw,
        }
    }
}
fn identities(params: &Value) -> (Option<String>, Option<String>) {
    (
        params
            .get("threadId")
            .and_then(Value::as_str)
            .map(str::to_string),
        params
            .get("turnId")
            .or_else(|| params.pointer("/turn/id"))
            .and_then(Value::as_str)
            .map(str::to_string),
    )
}
pub fn map_notification(message: &Value) -> FeedEvent {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let p = message.get("params").unwrap_or(&Value::Null);
    let (thread_id, turn_id) = identities(p);
    match method {
        "turn/started" => FeedEvent::new(
            FeedKind::Progress,
            method,
            turn_id.clone(),
            thread_id,
            turn_id,
            None,
            Some("Codex turn started".into()),
            None,
        ),
        "turn/completed" => FeedEvent::new(
            FeedKind::Completion,
            method,
            turn_id.clone(),
            thread_id,
            turn_id,
            None,
            Some("Codex turn completed".into()),
            None,
        ),
        "thread/status/changed" => FeedEvent::new(
            FeedKind::Progress,
            method,
            None,
            thread_id,
            turn_id,
            None,
            Some("Codex thread status changed".into()),
            None,
        ),
        "item/agentMessage/delta" => FeedEvent::new(
            FeedKind::AgentMessage,
            method,
            p.get("itemId").and_then(Value::as_str).map(str::to_string),
            thread_id,
            turn_id,
            p.get("delta").and_then(Value::as_str).map(str::to_string),
            None,
            None,
        ),
        "item/started" | "item/completed" => map_item_lifecycle(method, p),
        "warning" | "error" => FeedEvent::new(
            FeedKind::Warning,
            method,
            None,
            thread_id,
            turn_id,
            None,
            Some(p.to_string()),
            None,
        ),
        _ => FeedEvent::new(
            FeedKind::Unknown,
            method,
            None,
            thread_id,
            turn_id,
            None,
            None,
            Some(p.to_string()),
        ),
    }
}
fn map_item_lifecycle(method: &str, p: &Value) -> FeedEvent {
    let item = p.get("item").unwrap_or(&Value::Null);
    let (thread_id, turn_id) = identities(p);
    let item_id = item.get("id").and_then(Value::as_str).map(str::to_string);
    match item
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
    {
        "agentMessage" => FeedEvent::new(
            FeedKind::AgentMessage,
            method,
            item_id,
            thread_id,
            turn_id,
            item.get("text").and_then(Value::as_str).map(str::to_string),
            None,
            None,
        ),
        "userMessage" => FeedEvent::new(
            FeedKind::UserMessage,
            method,
            item_id,
            thread_id,
            turn_id,
            user_text(item),
            None,
            None,
        ),
        "reasoning" => FeedEvent::new(
            FeedKind::Progress,
            method,
            item_id,
            thread_id,
            turn_id,
            None,
            Some("Codex reasoning progress".into()),
            None,
        ),
        _ => FeedEvent::new(
            FeedKind::Unknown,
            method,
            item_id,
            thread_id,
            turn_id,
            None,
            None,
            Some(item.to_string()),
        ),
    }
}
pub fn history_events(thread: &Value) -> Vec<FeedEvent> {
    let thread_id = thread.get("id").and_then(Value::as_str).map(str::to_string);
    let mut events = Vec::new();
    for turn in thread
        .get("turns")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let turn_id = turn.get("id").and_then(Value::as_str).map(str::to_string);
        for item in turn
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let item_id = item.get("id").and_then(Value::as_str).map(str::to_string);
            let kind = match item
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
            {
                "userMessage" => FeedKind::UserMessage,
                "agentMessage" => FeedKind::AgentMessage,
                "reasoning" => FeedKind::Progress,
                _ => FeedKind::Unknown,
            };
            let text = match item.get("type").and_then(Value::as_str) {
                Some("userMessage") => user_text(item),
                Some("agentMessage") => {
                    item.get("text").and_then(Value::as_str).map(str::to_string)
                }
                _ => None,
            };
            let detail =
                matches!(kind, FeedKind::Progress).then_some("Codex reasoning item".to_string());
            let raw = matches!(kind, FeedKind::Unknown).then(|| item.to_string());
            events.push(FeedEvent::new(
                kind,
                if text.is_some() {
                    "item/completed"
                } else {
                    "thread/read"
                },
                item_id,
                thread_id.clone(),
                turn_id.clone(),
                text,
                detail,
                raw,
            ));
        }
    }
    events
}
fn user_text(item: &Value) -> Option<String> {
    let mut text = String::new();
    for part in item.get("content").and_then(Value::as_array)? {
        if let Some(value) = part.get("text").and_then(Value::as_str) {
            text.push_str(value)
        }
    }
    (!text.is_empty()).then_some(text)
}
