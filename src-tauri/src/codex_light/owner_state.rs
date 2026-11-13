//! What Desktop shows for a thread running on the on-demand backend.
//!
//! Desktop renders an owner's thread from a snapshot of its private conversation state, so
//! this keeps one up to date from the backend's own notifications. The field set was read
//! from Desktop 26.1103.3517.0 and is only used behind that build's method-table fence.
use serde_json::{json, Map, Value};
use super::owner_stream::{preview_item,preview_text,trim_items};
use std::collections::HashSet;

/// Oldest turns are dropped past this, so a long thread cannot produce an enormous frame.
/// Desktop reads the complete history itself once it takes the thread over.
pub(super) const SNAPSHOT_BYTE_BUDGET: usize = 1024 * 1024;
const NOW_FALLBACK: i64 = 0;

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(NOW_FALLBACK, |since| since.as_millis() as i64)
}

fn seconds_to_ms(value: &Value) -> Option<i64> {
    value.as_i64().map(|seconds| seconds * 1000)
}

fn turn_entity(state: &OwnerState, id: &str, status: &str) -> Value {
    json!({
        "params":{
            "threadId":state.id,"input":[],"clientUserMessageId":null,"approvalPolicy":state.approval,"approvalsReviewer":state.reviewer,"sandboxPolicy":state.sandbox,
            "model":state.model,"cwd":state.cwd,"effort":state.effort,"summary":"auto","personality":null,"outputSchema":null,"collaborationMode":null,"attachments":[]
        },
        "permissionParamsSource":"inferred","turnId":id,"rootTurnId":id,"turnStartedAtMs":now_ms(),"durationMs":null,
        "finalAssistantStartedAtMs":null,"status":status,"error":null,"diff":null,"items":[],"completedItemIds":{},
        "itemsPagination":{"olderCursor":null,"isLoadingOlder":false,"hasLoadedOldest":true,"newestSnapshotItemId":null}
    })
}

/// Desktop's renderer reads this field on every text input.
fn with_text_elements(input: &Value) -> Value {
    let mut input = input.clone();
    for item in input.as_array_mut().into_iter().flatten() {
        if item["type"] == "text" && item.get("text_elements").is_none() {
            item["text_elements"] = json!([]);
        }
    }
    input
}

pub(super) struct OwnerState {
    id: String,
    session: Value,
    title: Value,
    cwd: String,
    rollout: Value,
    created_at: i64,
    updated_at: i64,
    model: String,
    effort: Value,
    provider: Value,
    approval: Value,
    reviewer: Value,
    sandbox: Value,
    source: Value,
    runtime: Value,
    token_usage: Value,
    turns: Vec<Value>,
    requests: Vec<Value>,
}

impl OwnerState {
    /// From the reply to `thread/start` or `thread/resume`.
    pub fn from_reply(result: &Value) -> Option<Self> {
        let thread = &result["thread"];
        let id = thread["id"].as_str()?.to_owned();
        let cwd = result["cwd"].as_str().or_else(|| thread["cwd"].as_str()).unwrap_or_default().to_owned();
        let created_at = seconds_to_ms(&thread["createdAt"]).unwrap_or_else(now_ms);
        Some(Self {
            session: thread.get("sessionId").cloned().unwrap_or_else(|| json!(id)),
            title: thread.get("name").cloned().unwrap_or(Value::Null),
            rollout: thread.get("path").cloned().unwrap_or(Value::Null),
            created_at,
            updated_at: seconds_to_ms(&thread["updatedAt"]).unwrap_or(created_at),
            model: result["model"].as_str().unwrap_or_default().to_owned(),
            effort: result.get("reasoningEffort").cloned().unwrap_or(Value::Null),
            provider: result.get("modelProvider").or_else(|| thread.get("modelProvider")).cloned().unwrap_or(json!("openai")),
            approval: result.get("approvalPolicy").cloned().unwrap_or(json!("on-request")),
            reviewer: result.get("approvalsReviewer").cloned().unwrap_or(json!("user")),
            sandbox: result.get("sandbox").cloned().unwrap_or(json!({"type":"readOnly"})),
            source: thread.get("source").cloned().unwrap_or(json!("vscode")),
            runtime: json!({"type":"idle"}),
            token_usage: Value::Null,
            turns: Vec::new(),
            requests: Vec::new(),
            id,
            cwd,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// Rows from `thread/turns/list`, newest first or oldest first by `newest_first`.
    pub fn load_turns(&mut self, rows: &[Value], newest_first: bool) {
        let mut loaded: Vec<Value> = rows.iter().filter_map(|row| self.entity_from_row(row)).collect();
        if newest_first {
            loaded.reverse();
        }
        self.turns = loaded;
        self.trim_to_budget();
    }

    /// Keeps only what a snapshot can show: the newest turns within the budget, and always the
    /// newest one. A thread whose tools return images would otherwise hold every older turn,
    /// hundreds of megabytes, that Desktop is never sent.
    fn trim_to_budget(&mut self) {
        let mut first = self.turns.len();
        let mut bytes = 0;
        for turn in self.turns.iter().rev() {
            bytes += turn.to_string().len();
            if bytes > SNAPSHOT_BYTE_BUDGET && first < self.turns.len() {
                break;
            }
            first -= 1;
        }
        self.turns.drain(..first);
    }

    fn entity_from_row(&self, row: &Value) -> Option<Value> {
        let id = row["id"].as_str()?;
        let mut turn = turn_entity(self, id, row["status"].as_str().unwrap_or("completed"));
        turn["error"] = row.get("error").cloned().unwrap_or(Value::Null);
        turn["items"] = json!(row["items"].as_array().into_iter().flatten().cloned().map(preview_item).collect::<Vec<_>>());
        if let Some(message) = turn["items"].as_array().and_then(|items| items.iter().find(|item| item["type"] == "userMessage")).cloned() {
            turn["params"]["input"] = with_text_elements(&message["content"]);
        }
        if let Some(started) = seconds_to_ms(&row["startedAt"]) {
            turn["turnStartedAtMs"] = json!(started);
        }
        turn["durationMs"] = row.get("durationMs").cloned().unwrap_or(Value::Null);
        let finished: Map<String, Value> = turn["items"].as_array().into_iter().flatten().filter_map(|item| item["id"].as_str().map(|id| (id.to_owned(), json!(true)))).collect();
        turn["completedItemIds"] = Value::Object(finished);
        trim_items(&mut turn,&HashSet::new());
        Some(turn)
    }

    fn turn_mut(&mut self, id: &str) -> &mut Value {
        if let Some(index) = self.turns.iter().position(|turn| turn["turnId"] == id) {
            return &mut self.turns[index];
        }
        let turn = turn_entity(self, id, "inProgress");
        self.turns.push(turn);
        self.trim_to_budget();
        self.turns.last_mut().unwrap()
    }

    fn item_mut<'a>(turn: &'a mut Value, item: &str) -> Option<&'a mut Value> {
        turn["items"].as_array_mut()?.iter_mut().find(|row| row["id"] == item)
    }

    fn append(item: &mut Value, field: &str, text: &str) {
        let mut joined = item[field].as_str().unwrap_or_default().to_owned();
        joined.push_str(text);
        let limit=if field=="aggregatedOutput"{16*1024}else{256*1024};
        item[field] = json!(preview_text(&joined,limit));
    }

    /// The request that started a turn. Desktop shows the user's message from this: the
    /// backend announces no item for it while the turn runs.
    pub fn note_turn_request(&mut self, turn: &str, request: &Value) {
        if let Some(effort) = request.get("effort").filter(|effort| !effort.is_null()) {
            self.effort = effort.clone();
        }
        if let Some(model) = request["model"].as_str() {
            self.model = model.to_owned();
        }
        let effort = self.effort.clone();
        let model = self.model.clone();
        let entity = self.turn_mut(turn);
        entity["params"]["effort"] = effort;
        entity["params"]["model"] = json!(model);
        entity["params"]["input"] = with_text_elements(&request["input"]);
        if let Some(message) = request.get("clientUserMessageId").filter(|message| !message.is_null()) {
            entity["params"]["clientUserMessageId"] = message.clone();
        }
    }

    /// A backend notification. True when what Desktop shows changed.
    pub fn apply(&mut self, method: &str, params: &Value) -> bool {
        self.updated_at = now_ms();
        match method {
            "turn/started" => {
                let Some(id) = params.pointer("/turn/id").and_then(Value::as_str) else { return false };
                let id = id.to_owned();
                self.turn_mut(&id)["status"] = json!("inProgress");
                self.runtime = json!({"type":"active","activeFlags":[]});
            }
            "turn/completed" => {
                let Some(id) = params.pointer("/turn/id").and_then(Value::as_str) else { return false };
                let id = id.to_owned();
                let turn = self.turn_mut(&id);
                let started = turn["turnStartedAtMs"].as_i64().unwrap_or_else(now_ms);
                turn["status"] = params.pointer("/turn/status").cloned().unwrap_or(json!("completed"));
                turn["error"] = params.pointer("/turn/error").cloned().unwrap_or(Value::Null);
                turn["durationMs"] = json!(now_ms() - started);
                self.runtime = json!({"type":"idle"});
                self.requests.clear();
            }
            "item/started" | "item/completed" => {
                let (Some(turn), Some(id)) = (params["turnId"].as_str(), params.pointer("/item/id").and_then(Value::as_str)) else { return false };
                let (turn, id) = (turn.to_owned(), id.to_owned());
                let done = method == "item/completed";
                let mut item = preview_item(params["item"].clone());
                let turn = self.turn_mut(&turn);
                if turn["params"]["input"].as_array().is_some_and(Vec::is_empty) && item["type"] == "userMessage" {
                    turn["params"]["input"] = item["content"].clone();
                }
                let stamp = now_ms();
                if done && item["type"] == "agentMessage" && turn["finalAssistantStartedAtMs"].is_null() {
                    turn["finalAssistantStartedAtMs"] = json!(stamp);
                }
                if let Some(existing) = Self::item_mut(turn, &id) {
                    item["startedAtMs"] = existing.get("startedAtMs").cloned().unwrap_or(json!(stamp));
                    item["completedAtMs"] = if done { json!(stamp) } else { Value::Null };
                    *existing = item;
                } else {
                    item["startedAtMs"] = json!(stamp);
                    item["completedAtMs"] = if done { json!(stamp) } else { Value::Null };
                    if let Some(items) = turn["items"].as_array_mut() {
                        items.push(item);
                    }
                }
                if done {
                    turn["completedItemIds"][&id] = json!(true);
                    turn["itemsPagination"]["newestSnapshotItemId"] = json!(id);
                }
            }
            "item/agentMessage/delta" | "item/commandExecution/outputDelta" => {
                let (Some(turn), Some(id), Some(delta)) = (params["turnId"].as_str(), params["itemId"].as_str(), params["delta"].as_str()) else { return false };
                let (turn, id, delta) = (turn.to_owned(), id.to_owned(), delta.to_owned());
                let field = if method == "item/agentMessage/delta" { "text" } else { "aggregatedOutput" };
                let Some(item) = Self::item_mut(self.turn_mut(&turn), &id) else { return false };
                Self::append(item, field, &delta);
            }
            "thread/status/changed" => self.runtime = params["status"].clone(),
            "thread/name/updated" => self.title = params.get("threadName").or_else(|| params.get("name")).cloned().unwrap_or(Value::Null),
            "thread/tokenUsage/updated" => self.token_usage = params["tokenUsage"].clone(),
            "serverRequest/resolved" => {
                let id = params["requestId"].as_str().map(|raw| raw.strip_prefix("backend:").unwrap_or(raw).trim_matches('"').to_owned());
                self.requests.retain(|row| Some(row["id"].to_string().trim_matches('"').to_owned()) != id);
            }
            _ => return false,
        }
        if let Some(id)=params["turnId"].as_str().or_else(||params.pointer("/turn/id").and_then(Value::as_str)){
            let protected:HashSet<String>=self.requests.iter().filter_map(|r|r.pointer("/params/itemId").and_then(Value::as_str).map(str::to_owned)).collect();
            if let Some(turn)=self.turns.iter_mut().find(|t|t["turnId"]==id){trim_items(turn,&protected);}
        }
        true
    }

    /// A request from the backend, under the id the backend gave it.
    pub fn add_request(&mut self, id: Value, method: &str, params: Value) {
        self.requests.push(json!({"id":id,"method":method,"params":params}));
    }

    pub fn running_turn(&self) -> Option<String> {
        self.turns.iter().rev().find(|turn| turn["status"] == "inProgress").and_then(|turn| turn["turnId"].as_str()).map(str::to_owned)
    }

    pub fn model_settings(&self) -> Value {
        json!({"model":self.model,"reasoningEffort":self.effort,"daybreakEnabled":null,"resumeState":"resumed","mode":"default","threadSource":"user","modelProvider":self.provider})
    }

    pub fn snapshot(&self) -> Value {
        let mut kept = self.turns.len();
        let mut bytes = 0;
        for turn in self.turns.iter().rev() {
            bytes += turn.to_string().len();
            if bytes > SNAPSHOT_BYTE_BUDGET && kept < self.turns.len() {
                break;
            }
            kept -= 1;
        }
        let shown = &self.turns[kept..];
        let mut entities = Map::new();
        let mut entries = Vec::new();
        for turn in shown {
            let key = format!("turn:{}", turn["turnId"].as_str().unwrap_or_default());
            entries.push(json!({"key":key,"value":key}));
            entities.insert(key, turn.clone());
        }
        let collaboration = json!({"mode":"default","settings":{"reasoning_effort":self.effort,"model":self.model,"developer_instructions":null}});
        let mut state = json!({
            "id":self.id,"sessionId":self.session,"ephemeral":false,"forkedFromId":null,"hostId":"local","requests":self.requests,
            "createdAt":self.created_at,"updatedAt":self.updated_at,"recencyAt":self.updated_at,"title":self.title,
            "originator":"Codex Desktop","source":self.source,"agentNickname":null,"threadSource":"user","historyMode":"paginated",
            "canonicalVoiceHistory":true,"parentThreadId":null,"mode":"default","threadStartKind":"default","modelProvider":self.provider,
            "daybreakEnabled":null,"daybreakProgramPropagationEnrolled":false,"latestModel":self.model,"latestReasoningEffort":self.effort,
            "previousTurnModel":null,"latestCollaborationMode":collaboration,"hasUnreadTurn":false,"threadGoal":null
        });
        let mut rest = [
            json!({
                "threadRuntimeStatus":self.runtime,"rolloutPath":self.rollout,"gitInfo":null,"resumeState":"resumed","latestTokenUsageInfo":self.token_usage,
                "workspaceKind":"project","workspaceBrowserRoot":self.cwd,"projectlessOutputDirectory":self.cwd,"cwd":self.cwd,
                "environments":[{"environmentId":"local","cwd":self.cwd,"runtimeWorkspaceRoots":[self.cwd]}],
                "environmentSelectionEvidence":{"source":"live","updatedAt":self.updated_at},
                "turnsPagination":{"olderCursor":null,"oldestLoadedTurnId":null,"isLoadingOlder":false,"hasLoadedOldest":true},
                "boundedCloudHistory":false,"shellEnvironmentPolicy":null,"paginatedHistory":null,"providerBlock":null,
                "threadGoalResumeConfirmation":null,"turns":[]
            }),
            json!({
                "currentPermissions":{"activePermissionProfile":null,"runtimeWorkspaceRoots":[self.cwd],"approvalPolicy":self.approval,"approvalsReviewer":self.reviewer,"sandboxPolicy":self.sandbox},
                "latestThreadSettings":{
                    "disabledPluginIds":[],"cwd":self.cwd,"approvalPolicy":self.approval,"approvalsReviewer":self.reviewer,"activePermissionProfile":null,
                    "sandboxPolicy":self.sandbox,"permissions":null,"model":self.model,"serviceTier":"default","effort":self.effort,
                    "multiAgentMode":"explicitRequestOnly","collaborationMode":collaboration
                },
                "turnHistory":{"kind":"canonical","history":{"entitiesByKey":entities,"generation":0,"isComplete":true,"islands":[{"id":"tail:0","entries":entries,"olderBoundary":{"status":"exhausted","boundaryId":"tail:0:older"},"newerBoundary":{"status":"exhausted","boundaryId":"tail:0:newer"}}]}}
            }),
        ];
        for part in rest.iter_mut() {
            if let (Some(all), Some(part)) = (state.as_object_mut(), part.as_object_mut()) {
                all.append(part);
            }
        }
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> OwnerState {
        OwnerState::from_reply(&json!({"thread":{"id":"t1","path":"C:/r.jsonl","createdAt":100,"updatedAt":200,"name":"标题"},"model":"m","reasoningEffort":"low","cwd":"C:/w","approvalPolicy":"never","sandbox":{"type":"readOnly"}})).unwrap()
    }

    fn entity<'a>(snapshot: &'a Value, turn: &str) -> &'a Value {
        &snapshot["turnHistory"]["history"]["entitiesByKey"][format!("turn:{turn}")]
    }

    #[test]
    fn identity_comes_from_the_reply() {
        let snapshot = state().snapshot();
        assert_eq!(snapshot["id"], "t1");
        assert_eq!(snapshot["title"], "标题");
        assert_eq!(snapshot["createdAt"], 100_000);
        assert_eq!(snapshot["rolloutPath"], "C:/r.jsonl");
        assert_eq!(snapshot["latestThreadSettings"]["approvalPolicy"], "never");
        assert_eq!(snapshot["resumeState"], "resumed");
        assert!(OwnerState::from_reply(&json!({"thread":{}})).is_none());
    }

    #[test]
    fn a_turn_builds_up_from_notifications() {
        let mut s = state();
        assert!(s.apply("turn/started", &json!({"threadId":"t1","turn":{"id":"u1"}})));
        assert_eq!(s.running_turn().as_deref(), Some("u1"));
        assert_eq!(s.snapshot()["threadRuntimeStatus"]["type"], "active");
        s.apply("item/started", &json!({"turnId":"u1","item":{"id":"m0","type":"userMessage","content":[{"type":"text","text":"你好"}]}}));
        s.apply("item/started", &json!({"turnId":"u1","item":{"id":"a1","type":"agentMessage","text":""}}));
        s.apply("item/agentMessage/delta", &json!({"turnId":"u1","itemId":"a1","delta":"好"}));
        s.apply("item/agentMessage/delta", &json!({"turnId":"u1","itemId":"a1","delta":"的"}));
        let snapshot = s.snapshot();
        assert_eq!(entity(&snapshot, "u1")["items"][1]["text"], "好的");
        assert_eq!(entity(&snapshot, "u1")["params"]["input"][0]["text"], "你好");
        assert_eq!(entity(&snapshot, "u1")["items"][1]["completedAtMs"], Value::Null);
        s.apply("item/completed", &json!({"turnId":"u1","item":{"id":"a1","type":"agentMessage","text":"好的。"}}));
        s.apply("turn/completed", &json!({"threadId":"t1","turn":{"id":"u1","status":"completed"}}));
        let snapshot = s.snapshot();
        let turn = entity(&snapshot, "u1");
        assert_eq!(turn["items"][1]["text"], "好的。");
        assert_eq!(turn["completedItemIds"]["a1"], true);
        assert_eq!(turn["status"], "completed");
        assert!(turn["items"][1]["completedAtMs"].is_number());
        assert_eq!(snapshot["threadRuntimeStatus"]["type"], "idle");
        assert_eq!(s.running_turn(), None);
    }

    #[test]
    fn the_request_supplies_the_message_the_backend_does_not_announce() {
        let mut s = state();
        s.note_turn_request("u1", &json!({"input":[{"type":"text","text":"你好"}],"effort":"high","clientUserMessageId":"c1"}));
        s.apply("turn/started", &json!({"turn":{"id":"u1"}}));
        let snapshot = s.snapshot();
        let turn = entity(&snapshot, "u1");
        assert_eq!(turn["params"]["input"][0]["text"], "你好");
        assert_eq!(turn["params"]["input"][0]["text_elements"], json!([]));
        assert_eq!(turn["params"]["effort"], "high");
        assert_eq!(turn["params"]["clientUserMessageId"], "c1");
        assert_eq!(turn["params"]["approvalPolicy"], "never");
        assert_eq!(snapshot["latestReasoningEffort"], "high");
        assert_eq!(s.turns.len(), 1, "the started event finds the same turn");
    }

    #[test]
    fn nothing_points_at_history_desktop_cannot_load() {
        let mut s = state();
        s.apply("turn/started", &json!({"turn":{"id":"u1"}}));
        let snapshot = s.snapshot();
        let island = &snapshot["turnHistory"]["history"]["islands"][0];
        // Desktop reads both boundaries' status, and a null one blanks the whole thread.
        assert_eq!(island["olderBoundary"]["status"], "exhausted");
        assert_eq!(island["newerBoundary"]["status"], "exhausted");
        assert_eq!(island["entries"][0]["key"], "turn:u1");
        assert_eq!(snapshot["paginatedHistory"], Value::Null);
        assert_eq!(snapshot["turnHistory"]["history"]["isComplete"], true);
    }

    #[test]
    fn history_rows_load_oldest_first() {
        let mut s = state();
        s.load_turns(&[json!({"id":"new","status":"inProgress","items":[]}), json!({"id":"old","status":"completed","items":[{"id":"i","type":"agentMessage"}]})], true);
        assert_eq!(s.turns[0]["turnId"], "old");
        assert_eq!(s.turns[0]["completedItemIds"]["i"], true);
        assert_eq!(s.running_turn().as_deref(), Some("new"));
    }

    #[test]
    fn requests_track_the_backend_and_end_with_the_turn() {
        let mut s = state();
        s.add_request(json!(7), "item/commandExecution/requestApproval", json!({"command":"dir"}));
        s.add_request(json!("x"), "item/fileChange/requestApproval", json!({}));
        assert_eq!(s.snapshot()["requests"].as_array().unwrap().len(), 2);
        s.apply("serverRequest/resolved", &json!({"requestId":"backend:7"}));
        let snapshot = s.snapshot();
        assert_eq!(snapshot["requests"].as_array().unwrap().len(), 1);
        assert_eq!(snapshot["requests"][0]["id"], "x");
        s.apply("turn/completed", &json!({"turn":{"id":"u1"}}));
        assert!(s.snapshot()["requests"].as_array().unwrap().is_empty());
    }

    #[test]
    fn an_enormous_history_keeps_the_newest_turns() {
        let mut s = state();
        let large = "x".repeat(SNAPSHOT_BYTE_BUDGET / 2);
        let rows: Vec<Value> = (0..5).map(|index| json!({"id":format!("u{index}"),"status":"completed","items":[{"id":"i","type":"agentMessage","text":large}]})).collect();
        s.load_turns(&rows, false);
        let snapshot = s.snapshot();
        let entities = snapshot["turnHistory"]["history"]["entitiesByKey"].as_object().unwrap();
        assert!(entities.contains_key("turn:u4"));
        assert!(!entities.contains_key("turn:u0"));
        assert!(entities.len() < 5);
    }

    #[test]
    fn turns_past_the_budget_are_not_kept_in_memory() {
        let mut s = state();
        let large = "x".repeat(SNAPSHOT_BYTE_BUDGET / 2);
        let rows: Vec<Value> = (0..6).map(|index| json!({"id":format!("u{index}"),"status":"completed","items":[{"id":"i","type":"agentMessage","text":large}]})).collect();
        s.load_turns(&rows, false);
        let kept: Vec<String> = s.turns.iter().map(|turn| turn["turnId"].as_str().unwrap().to_owned()).collect();
        assert_eq!(kept,["u3","u4","u5"],"only the bounded recent preview survives");
        let shown = s.snapshot();
        let entities = shown["turnHistory"]["history"]["entitiesByKey"].as_object().unwrap();
        assert!(entities.contains_key("turn:u5") && !entities.contains_key("turn:u0"));
        // A new turn arriving drops an older one that no longer fits beside it.
        assert!(s.apply("turn/started", &json!({"turn":{"id":"u6"}})));
        assert!(s.apply("item/completed", &json!({"turnId":"u6","item":{"id":"j","type":"agentMessage","text":large}})));
        assert!(s.apply("turn/started", &json!({"turn":{"id":"u7"}})));
        let kept: Vec<String> = s.turns.iter().map(|turn| turn["turnId"].as_str().unwrap().to_owned()).collect();
        assert_eq!(kept,["u4","u5","u6","u7"]);
        let small: Vec<Value> = (0..4).map(|index| json!({"id":format!("s{index}"),"status":"completed","items":[]})).collect();
        s.load_turns(&small, false);
        assert_eq!(s.turns.len(), 4, "a small history is kept whole");
    }

    #[test]
    fn one_large_turn_keeps_a_bounded_preview_and_preserves_the_native_payload() {
        let mut s = state();
        let native=json!({"id":"only","status":"completed","items":[{"id":"i","type":"agentMessage","text":"x".repeat(25*1024*1024)}]});
        s.load_turns(&[native.clone()], false);
        let snapshot=s.snapshot();
        assert!(snapshot["turnHistory"]["history"]["entitiesByKey"].as_object().unwrap().contains_key("turn:only"));
        assert!(super::super::owner_stream::fits(&snapshot,SNAPSHOT_BYTE_BUDGET));
        assert_eq!(native["items"][0]["text"].as_str().unwrap().len(),25*1024*1024);
        assert!(snapshot["turnHistory"]["history"]["entitiesByKey"]["turn:only"]["items"][0]["text"].as_str().unwrap().contains("Agbrio"));
    }
}
