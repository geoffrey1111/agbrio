//! Fixed 0.154.0 live item/turn results. No history client or history RPC exists here.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const FINAL_BYTES: usize = 1024 * 1024;
const PRE_ACK_BYTES: usize = 2 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResultState {
    Pending,
    Available,
    NotRetained,
    Unavailable,
    TooLarge,
    NotApplicable,
}
#[derive(Clone, Debug, Serialize)]
pub struct ExactTerminal {
    pub thread_id: String,
    pub turn_id: String,
    pub status: String,
    pub result_state: ResultState,
    pub text: Option<String>,
    pub item_id: Option<String>,
    pub phase: Option<String>,
    pub hash: Option<String>,
    pub bytes: Option<usize>,
    pub code: Option<String>,
    pub source: Option<String>,
}
#[derive(Clone, Debug)]
struct Candidate {
    id: String,
    phase: Option<String>,
    text: Option<String>,
    hash: String,
    bytes: usize,
}
#[derive(Default)]
struct PendingTurn {
    candidate: Option<Candidate>,
    terminal: Option<String>,
    summary: bool,
    conflict: bool,
}
pub struct LiveResult {
    thread_id: String,
    ack_turn: Option<String>,
    turns: HashMap<String, PendingTurn>,
    fault: Option<&'static str>,
    continuous: bool,
}
impl LiveResult {
    pub fn new(thread_id: String) -> Self {
        Self {
            thread_id,
            ack_turn: None,
            turns: HashMap::new(),
            fault: None,
            continuous: true,
        }
    }
    pub fn mark_discontinuity(&mut self) {
        self.continuous = false;
    }
    /// Only the matching turn/start response, never an observed started event.
    pub fn associate_ack(&mut self, thread_id: &str, turn_id: &str) -> Result<(), String> {
        if thread_id != self.thread_id
            || turn_id.is_empty()
            || turn_id.len() > 256
            || self.ack_turn.as_deref().is_some_and(|old| old != turn_id)
        {
            self.fault = Some("RESULT_EVENT_CONFLICT");
            return Err("RESULT_EVENT_CONFLICT".into());
        }
        self.ack_turn = Some(turn_id.into());
        self.turns.retain(|id, _| id == turn_id);
        Ok(())
    }
    pub fn observe(&mut self, event: &Value) -> Result<(), String> {
        let method = event.get("method").and_then(Value::as_str).unwrap_or("");
        if !matches!(method, "item/completed" | "turn/completed") {
            return Ok(());
        }
        if event.pointer("/params/threadId").and_then(Value::as_str)
            != Some(self.thread_id.as_str())
        {
            return Ok(());
        }
        let turn_id = if method == "item/completed" {
            event.pointer("/params/turnId")
        } else {
            event.pointer("/params/turn/id")
        }
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .ok_or("CAPABILITY_UNVERIFIED")?;
        if self.ack_turn.as_deref().is_some_and(|id| id != turn_id) {
            return Ok(());
        }
        if !self.turns.contains_key(turn_id) && self.turns.len() >= 128 {
            self.fault = Some("RESULT_EVENT_CONFLICT");
            return Err("RESULT_EVENT_CONFLICT".into());
        }
        let pending = self.turns.entry(turn_id.into()).or_default();
        if method == "item/completed" {
            if let Some(candidate) = candidate(
                event
                    .pointer("/params/item")
                    .ok_or("CAPABILITY_UNVERIFIED")?,
            )? {
                if pending
                    .candidate
                    .as_ref()
                    .is_some_and(|old| old.id == candidate.id && old.hash != candidate.hash)
                {
                    pending.conflict = true;
                }
                // Terminal Summary is the upstream-selected final item, independent
                // of late duplicate item notifications or arrival order.
                if !pending.summary && pending.terminal.is_none() {
                    pending.candidate = Some(candidate);
                }
            }
        } else {
            let turn = event
                .pointer("/params/turn")
                .ok_or("CAPABILITY_UNVERIFIED")?;
            let status = turn
                .get("status")
                .and_then(Value::as_str)
                .ok_or("CAPABILITY_UNVERIFIED")?;
            if !matches!(status, "completed" | "failed" | "interrupted") {
                return Err("CAPABILITY_UNVERIFIED".into());
            }
            if pending.terminal.as_deref().is_some_and(|old| old != status) {
                pending.conflict = true;
            }
            pending.terminal = Some(status.into());
            if status == "completed" {
                if let Some(items) = turn.get("items").and_then(Value::as_array) {
                    let mut selected = None;
                    for item in items {
                        if let Some(item) = candidate(item)? {
                            selected = Some(item);
                        }
                    }
                    if let Some(selected) = selected {
                        if pending
                            .candidate
                            .as_ref()
                            .is_some_and(|old| old.id == selected.id && old.hash != selected.hash)
                        {
                            pending.conflict = true;
                        }
                        pending.candidate = Some(selected);
                        pending.summary = true;
                    }
                }
            } else {
                pending.candidate = None;
            }
        }
        let retained = self
            .turns
            .values()
            .map(|t| {
                t.candidate
                    .as_ref()
                    .and_then(|c| c.text.as_ref())
                    .map_or(0, String::len)
                    + 512
            })
            .sum::<usize>();
        if self.ack_turn.is_none() && retained > PRE_ACK_BYTES {
            self.fault = Some("RESULT_TOO_LARGE");
            // Preserve exact terminal identities/status, discard only text.
            for turn in self.turns.values_mut() {
                if let Some(candidate) = turn.candidate.as_mut() {
                    candidate.text = None;
                }
            }
            return Err("RESULT_TOO_LARGE".into());
        }
        if self.fault == Some("RESULT_TOO_LARGE") {
            for turn in self.turns.values_mut() {
                if let Some(candidate) = turn.candidate.as_mut() {
                    candidate.text = None;
                }
            }
        }
        Ok(())
    }
    pub fn terminal(&self) -> Option<ExactTerminal> {
        let turn_id = self.ack_turn.as_ref()?;
        let pending = self.turns.get(turn_id)?;
        let native_status = pending.terminal.as_deref()?;
        let status = match native_status {
            "completed" => "COMPLETED",
            "failed" => "FAILED",
            _ => "CANCELLED",
        };
        let mut result = ExactTerminal {
            thread_id: self.thread_id.clone(),
            turn_id: turn_id.clone(),
            status: status.into(),
            result_state: ResultState::Unavailable,
            text: None,
            item_id: None,
            phase: None,
            hash: None,
            bytes: None,
            code: None,
            source: None,
        };
        if native_status != "completed" {
            result.result_state = ResultState::NotApplicable;
            return Some(result);
        }
        if pending.conflict || self.fault == Some("RESULT_EVENT_CONFLICT") {
            result.code = Some("RESULT_EVENT_CONFLICT".into());
            return Some(result);
        }
        if self.fault == Some("RESULT_TOO_LARGE") {
            result.result_state = ResultState::TooLarge;
            result.code = Some("RESULT_TOO_LARGE".into());
            return Some(result);
        }
        let Some(c) = &pending.candidate else {
            result.code = Some("RESULT_UNAVAILABLE".into());
            return Some(result);
        };
        if !self.continuous && !pending.summary {
            result.code = Some("RESULT_UNAVAILABLE".into());
            return Some(result);
        }
        result.item_id = Some(c.id.clone());
        result.phase = c.phase.clone();
        result.bytes = Some(c.bytes);
        result.hash = Some(c.hash.clone());
        if c.bytes > FINAL_BYTES {
            result.result_state = ResultState::TooLarge;
            result.code = Some("RESULT_TOO_LARGE".into());
            return Some(result);
        }
        result.result_state = ResultState::Available;
        result.text = c.text.clone();
        result.source = Some(
            if pending.summary {
                "LIVE_TERMINAL"
            } else {
                "LIVE_ITEM_AND_TERMINAL"
            }
            .into(),
        );
        Some(result)
    }
}
fn candidate(item: &Value) -> Result<Option<Candidate>, String> {
    if item.get("type").and_then(Value::as_str) != Some("agentMessage") {
        return Ok(None);
    }
    let phase = match item.get("phase") {
        None | Some(Value::Null) => None,
        Some(Value::String(p)) => Some(p.clone()),
        _ => return Err("CAPABILITY_UNVERIFIED".into()),
    };
    if phase.as_deref().is_some_and(|p| p != "final_answer") {
        return Ok(None);
    }
    let text = item
        .get("text")
        .and_then(Value::as_str)
        .ok_or("CAPABILITY_UNVERIFIED")?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .ok_or("CAPABILITY_UNVERIFIED")?;
    Ok(Some(Candidate {
        id: id.into(),
        phase,
        text: (text.len() <= FINAL_BYTES).then(|| text.into()),
        hash: format!("{:x}", Sha256::digest(text.as_bytes())),
        bytes: text.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn item(id: &str, text: &str, phase: Value) -> Value {
        json!({"type":"agentMessage","id":id,"text":text,"phase":phase})
    }
    fn item_done(id: &str, text: &str, phase: Value) -> Value {
        json!({"method":"item/completed","params":{"threadId":"thread","turnId":"turn","item":item(id,text,phase)}})
    }
    fn done(status: &str, items: Vec<Value>) -> Value {
        json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"turn","status":status,"itemsView":"summary","items":items}}})
    }
    #[test]
    fn terminal_before_ack_needs_exact_ack_and_never_started_as_authority() {
        let mut result = LiveResult::new("thread".into());
        result
            .observe(&item_done("a", "short", json!(null)))
            .unwrap();
        assert!(result.terminal().is_none());
        result
            .observe(&done("completed", vec![item("a", "short", json!(null))]))
            .unwrap();
        assert!(result.terminal().is_none());
        result.associate_ack("thread", "turn").unwrap();
        let terminal = result.terminal().unwrap();
        assert_eq!(terminal.text.as_deref(), Some("short"));
        assert_eq!(terminal.source.as_deref(), Some("LIVE_TERMINAL"));
    }
    #[test]
    fn last_nonempty_final_and_summary_win_over_commentary_and_late_items() {
        let mut result = LiveResult::new("thread".into());
        result.associate_ack("thread", "turn").unwrap();
        result
            .observe(&item_done("a", "first", json!(null)))
            .unwrap();
        result
            .observe(&item_done("b", "second", json!("final_answer")))
            .unwrap();
        result
            .observe(&item_done("c", "commentary", json!("commentary")))
            .unwrap();
        result
            .observe(&done(
                "completed",
                vec![
                    item("b", "second", json!("final_answer")),
                    item("blank", " ", json!(null)),
                ],
            ))
            .unwrap();
        result
            .observe(&item_done("a", "first", json!(null)))
            .unwrap();
        assert_eq!(result.terminal().unwrap().text.as_deref(), Some("second"));
    }
    #[test]
    fn failure_interruption_missing_large_and_conflict_keep_execution_separate() {
        for (status, expected) in [("failed", "FAILED"), ("interrupted", "CANCELLED")] {
            let mut r = LiveResult::new("thread".into());
            r.associate_ack("thread", "turn").unwrap();
            r.observe(&item_done("a", "not success", json!(null)))
                .unwrap();
            r.observe(&done(status, vec![])).unwrap();
            let terminal = r.terminal().unwrap();
            assert_eq!(terminal.status, expected);
            assert_eq!(terminal.result_state, ResultState::NotApplicable);
            assert!(terminal.text.is_none());
        }
        let mut r = LiveResult::new("thread".into());
        r.associate_ack("thread", "turn").unwrap();
        r.observe(&done("completed", vec![])).unwrap();
        assert_eq!(r.terminal().unwrap().status, "COMPLETED");
        assert_eq!(r.terminal().unwrap().result_state, ResultState::Unavailable);
        let mut r = LiveResult::new("thread".into());
        r.associate_ack("thread", "turn").unwrap();
        r.observe(&done(
            "completed",
            vec![item("a", &"x".repeat(FINAL_BYTES + 1), json!(null))],
        ))
        .unwrap();
        assert_eq!(r.terminal().unwrap().result_state, ResultState::TooLarge);
        let mut r = LiveResult::new("thread".into());
        r.associate_ack("thread", "turn").unwrap();
        r.observe(&item_done("a", "original", json!(null))).unwrap();
        r.observe(&done("completed", vec![item("a", "changed", json!(null))]))
            .unwrap();
        assert_eq!(
            r.terminal().unwrap().code.as_deref(),
            Some("RESULT_EVENT_CONFLICT")
        );
    }
    #[test]
    fn different_exact_run_never_reads_another_result() {
        let mut r = LiveResult::new("thread".into());
        r.observe(&done("completed", vec![item("a", "other", json!(null))]))
            .unwrap();
        r.associate_ack("thread", "other-turn").unwrap();
        assert!(r.terminal().is_none());
    }
}
