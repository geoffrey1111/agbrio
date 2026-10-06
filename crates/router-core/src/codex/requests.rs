//! Bounded live native requests, separate from task approval. No raw RPC ID in UI.
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize)]
pub struct Question {
    pub id: String,
    pub label: String,
    pub options: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct RequestView {
    pub action_id: String,
    pub revision: i64,
    pub kind: String,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub summary: String,
    pub choices: Vec<String>,
    pub questions: Vec<Question>,
    pub expires_at: i64,
}
/// Internal binding, never deserialized from tool/HTTP input or serialized to UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestBinding {
    pub run_id: String,
    pub epoch: String,
    pub action_id: String,
    pub revision: i64,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub request_hash: String,
}
#[derive(Clone)]
pub struct PendingView {
    pub view: RequestView,
    pub binding: RequestBinding,
}
struct Pending {
    public: PendingView,
    raw_id: Value,
    method: String,
    params: Value,
    consumed: bool,
}
pub struct PendingRequests {
    run: String,
    epoch: String,
    thread: String,
    turn: String,
    entries: HashMap<String, Pending>,
}
pub struct NativeAnswer {
    pub question_id: String,
    pub value: String,
}
/// Constructed only by the live gate after a trusted browser action. Not Deserialize.
pub struct NativeReply {
    pub(crate) id: Value,
    pub(crate) result: Value,
    pub(crate) epoch: String,
}
fn text(v: &Value, key: &str) -> Result<String, String> {
    let s = v
        .get(key)
        .and_then(Value::as_str)
        .ok_or("CAPABILITY_UNVERIFIED")?;
    if s.is_empty() || s.len() > 2048 {
        return Err("CAPABILITY_UNVERIFIED".into());
    }
    Ok(s.into())
}
fn hash(v: &Value) -> String {
    format!("{:x}", Sha256::digest(v.to_string().as_bytes()))
}
impl PendingRequests {
    pub fn new(run: String, epoch: String, thread: String, turn: String) -> Self {
        Self {
            run,
            epoch,
            thread,
            turn,
            entries: HashMap::new(),
        }
    }
    pub fn observe(&mut self, event: &Value, now: i64) -> Result<(), String> {
        let id = event.get("id").ok_or("CAPABILITY_UNVERIFIED")?;
        if !(id.is_i64()
            || id.is_u64()
            || id.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 256))
        {
            return Err("CAPABILITY_UNVERIFIED".into());
        }
        let method = text(event, "method")?;
        let params = event.get("params").ok_or("CAPABILITY_UNVERIFIED")?;
        let thread = text(params, "threadId")?;
        let turn = text(params, "turnId")?;
        let item = text(params, "itemId")?;
        if thread != self.thread || turn != self.turn {
            return Err("FORBIDDEN".into());
        }
        let digest = hash(event);
        if let Some(old) = self.entries.values().find(|p| p.raw_id == *id) {
            return if old.public.binding.request_hash == digest {
                Ok(())
            } else {
                Err("EVIDENCE_CONFLICT".into())
            };
        }
        if self.entries.len() >= 8 {
            return Err("RATE_LIMITED".into());
        }
        // Full bounded native details are shown as plain text. Never approve a truncation.
        let summary = serde_json::to_string_pretty(params).map_err(|_| "INTERNAL")?;
        if summary.len() > 8192 {
            return Err("CAPABILITY_UNVERIFIED".into());
        }
        let (kind, mut choices) = match method.as_str() {
            "item/commandExecution/requestApproval" => {
                ("COMMAND_APPROVAL", vec!["accept", "decline", "cancel"])
            }
            "item/fileChange/requestApproval" => (
                "FILE_CHANGE_APPROVAL",
                if params.get("grantRoot").is_some_and(|v| !v.is_null()) {
                    vec!["decline", "cancel"]
                } else {
                    vec!["accept", "decline", "cancel"]
                },
            ),
            "item/permissions/requestApproval" => {
                if !params.get("permissions").is_some_and(Value::is_object) {
                    return Err("CAPABILITY_UNVERIFIED".into());
                }
                ("PERMISSIONS_APPROVAL", vec!["accept", "decline"])
            }
            "item/tool/requestUserInput" => ("USER_INPUT", vec![]),
            _ => ("UNSUPPORTED_NEEDS_ATTENTION", vec![]),
        };
        // Some native envelopes further restrict decisions; never add a choice.
        if let Some(offered) = params.get("availableDecisions") {
            let offered = offered.as_array().ok_or("CAPABILITY_UNVERIFIED")?;
            choices.retain(|c| offered.iter().any(|v| v.as_str() == Some(c)));
        }
        let mut questions = Vec::new();
        if kind == "USER_INPUT" {
            let qs = params
                .get("questions")
                .and_then(Value::as_array)
                .ok_or("CAPABILITY_UNVERIFIED")?;
            if qs.is_empty() || qs.len() > 8 {
                return Err("CAPABILITY_UNVERIFIED".into());
            }
            for q in qs {
                // No credential collection through this text slice.
                if q.get("isSecret").and_then(Value::as_bool) != Some(false) {
                    return Err("CAPABILITY_UNVERIFIED".into());
                }
                let id = text(q, "id")?;
                if questions.iter().any(|x: &Question| x.id == id) {
                    return Err("CAPABILITY_UNVERIFIED".into());
                }
                let options = match q.get("options") {
                    Some(Value::Array(opts)) if opts.len() <= 16 => opts
                        .iter()
                        .map(|o| text(o, "label"))
                        .collect::<Result<Vec<_>, _>>()?,
                    Some(Value::Null) => vec![],
                    _ => return Err("CAPABILITY_UNVERIFIED".into()),
                };
                questions.push(Question {
                    id,
                    label: text(q, "question")?,
                    options,
                });
            }
        }
        let action = uuid::Uuid::new_v4().to_string();
        let binding = RequestBinding {
            run_id: self.run.clone(),
            epoch: self.epoch.clone(),
            action_id: action.clone(),
            revision: 1,
            thread_id: thread.clone(),
            turn_id: turn.clone(),
            item_id: item.clone(),
            request_hash: digest,
        };
        let view = RequestView {
            action_id: action.clone(),
            revision: 1,
            kind: kind.into(),
            thread_id: thread,
            turn_id: turn,
            item_id: item,
            summary,
            choices: choices.into_iter().map(String::from).collect(),
            questions,
            expires_at: now + 600_000,
        };
        self.entries.insert(
            action,
            Pending {
                public: PendingView { view, binding },
                raw_id: id.clone(),
                method,
                params: params.clone(),
                consumed: false,
            },
        );
        Ok(())
    }
    pub fn views(&self, now: i64) -> Vec<PendingView> {
        self.entries
            .values()
            .filter(|p| !p.consumed && p.public.view.expires_at > now)
            .map(|p| p.public.clone())
            .collect()
    }
    pub fn resolved(&mut self, event: &Value) -> Result<(), String> {
        let p = event.get("params").ok_or("CAPABILITY_UNVERIFIED")?;
        if p.get("threadId").and_then(Value::as_str) != Some(&self.thread) {
            return Err("FORBIDDEN".into());
        }
        let id = p.get("requestId").ok_or("CAPABILITY_UNVERIFIED")?;
        self.entries.retain(|_, p| p.raw_id != *id);
        Ok(())
    }
    /// Consume before transport write. A lost write/ACK is never retried.
    pub fn consume(
        &mut self,
        binding: &RequestBinding,
        choice: Option<&str>,
        answers: &[NativeAnswer],
        now: i64,
    ) -> Result<NativeReply, String> {
        let p = self
            .entries
            .get_mut(&binding.action_id)
            .ok_or("NOT_FOUND")?;
        if &p.public.binding != binding || p.consumed || p.public.view.expires_at <= now {
            return Err("FORBIDDEN".into());
        }
        let result = if p.method == "item/tool/requestUserInput" {
            if choice.is_some() || answers.len() != p.public.view.questions.len() {
                return Err("FORBIDDEN".into());
            }
            let mut map = serde_json::Map::new();
            for a in answers {
                if a.value.len() > 2048
                    || a.value.trim().is_empty()
                    || map.contains_key(&a.question_id)
                    || !p
                        .public
                        .view
                        .questions
                        .iter()
                        .any(|q| q.id == a.question_id)
                {
                    return Err("FORBIDDEN".into());
                }
                map.insert(a.question_id.clone(), json!({"answers":[a.value]}));
            }
            json!({"answers":map})
        } else {
            let c = choice.ok_or("FORBIDDEN")?;
            if !answers.is_empty() || !p.public.view.choices.iter().any(|v| v == c) {
                return Err("FORBIDDEN".into());
            }
            if p.method == "item/permissions/requestApproval" {
                json!({"permissions":if c == "accept" { p.params["permissions"].clone() } else {json!({})},"scope":"turn"})
            } else {
                json!({"decision":c})
            }
        };
        p.consumed = true;
        Ok(NativeReply {
            id: p.raw_id.clone(),
            result,
            epoch: self.epoch.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(id: Value) -> Value {
        json!({"id":id,"method":"item/commandExecution/requestApproval","params":{"threadId":"t","turnId":"u","itemId":"i","command":"echo fixture"}})
    }
    fn gate() -> PendingRequests {
        PendingRequests::new("r".into(), "e".into(), "t".into(), "u".into())
    }
    #[test]
    fn native_response_is_typed_exact_one_shot_and_never_session_approval() {
        let mut g = gate();
        g.observe(&request(json!(1)), 100).unwrap();
        g.observe(&request(json!("1")), 100).unwrap();
        assert_eq!(g.views(101).len(), 2);
        let b = g.views(101)[0].binding.clone();
        for field in ["epoch", "run", "turn", "item", "hash"] {
            let mut bad = b.clone();
            match field {
                "epoch" => bad.epoch = "bad".into(),
                "run" => bad.run_id = "bad".into(),
                "turn" => bad.turn_id = "bad".into(),
                "item" => bad.item_id = "bad".into(),
                _ => bad.request_hash = "bad".into(),
            };
            assert!(g.consume(&bad, Some("accept"), &[], 101).is_err());
        }
        assert!(g.consume(&b, Some("acceptForSession"), &[], 101).is_err());
        let reply = g.consume(&b, Some("accept"), &[], 101).unwrap();
        assert_eq!(reply.result, json!({"decision":"accept"}));
        assert!(g.consume(&b, Some("accept"), &[], 101).is_err());
        g.resolved(&json!({"params":{"threadId":"t","requestId":reply.id}}))
            .unwrap();
        assert_eq!(g.entries.len(), 1);
    }
    #[test]
    fn overflow_expiry_foreign_turn_and_changed_request_fail_closed() {
        let mut g = gate();
        for n in 0..8 {
            g.observe(&request(json!(n)), 0).unwrap();
        }
        assert!(g.observe(&request(json!(9)), 0).is_err());
        assert!(g.views(600_000).is_empty());
        let mut changed = request(json!(0));
        changed["params"]["command"] = json!("other");
        assert!(g.observe(&changed, 0).is_err());
        changed["params"]["turnId"] = json!("foreign");
        assert!(gate().observe(&changed, 0).is_err());
        let b = g.entries.values().next().unwrap().public.binding.clone();
        assert!(g.consume(&b, Some("accept"), &[], 600_000).is_err());
    }
    #[test]
    fn grant_root_unknown_and_oversized_details_cannot_be_accepted() {
        for method in ["item/fileChange/requestApproval", "future/approval"] {
            let mut g = gate();
            let mut r = request(json!(1));
            r["method"] = json!(method);
            r["params"]["grantRoot"] = json!("C:/");
            g.observe(&r, 0).unwrap();
            let b = g.views(0)[0].binding.clone();
            assert!(g.consume(&b, Some("accept"), &[], 1).is_err());
        }
        let mut r = request(json!(1));
        r["params"]["command"] = json!("x".repeat(8192));
        assert!(gate().observe(&r, 0).is_err());
    }
    #[test]
    fn permissions_response_is_exact_requested_profile_and_turn_scoped() {
        for choice in ["accept", "decline"] {
            let mut g = gate();
            let mut r = request(json!(17));
            r["method"] = json!("item/permissions/requestApproval");
            r["params"]["permissions"] = json!({"network":{"enabled":true},"fileSystem":{"read":["fixture-root"],"write":[]}});
            g.observe(&r, 0).unwrap();
            let b = g.views(0)[0].binding.clone();
            assert!(g.consume(&b, Some("acceptForSession"), &[], 1).is_err());
            let reply = g.consume(&b, Some(choice), &[], 1).unwrap();
            assert_eq!(reply.id, json!(17));
            assert_eq!(reply.result["scope"], "turn");
            assert_eq!(
                reply.result["permissions"],
                if choice == "accept" {
                    r["params"]["permissions"].clone()
                } else {
                    json!({})
                }
            );
            assert!(g.consume(&b, Some(choice), &[], 1).is_err());
        }
    }
    #[test]
    fn user_input_has_exact_question_ids_and_cannot_grant_approval() {
        let mut g = gate();
        let mut r = request(json!("input"));
        r["method"] = json!("item/tool/requestUserInput");
        r["params"]["questions"] = json!([{"id":"q1","question":"Choose","isSecret":false,"options":[{"label":"A"},{"label":"B"}]},{"id":"q2","question":"Text","isSecret":false,"options":null}]);
        g.observe(&r, 0).unwrap();
        let b = g.views(0)[0].binding.clone();
        let answers = vec![
            NativeAnswer {
                question_id: "q1".into(),
                value: "A".into(),
            },
            NativeAnswer {
                question_id: "q2".into(),
                value: "fixture answer".into(),
            },
        ];
        assert!(g.consume(&b, Some("accept"), &answers, 1).is_err());
        assert!(g.consume(&b, None, &answers[..1], 1).is_err());
        assert!(g
            .consume(
                &b,
                None,
                &[
                    NativeAnswer {
                        question_id: "q1".into(),
                        value: "A".into()
                    },
                    NativeAnswer {
                        question_id: "q1".into(),
                        value: "B".into()
                    }
                ],
                1
            )
            .is_err());
        let reply = g.consume(&b, None, &answers, 1).unwrap();
        assert_eq!(reply.id, json!("input"));
        assert_eq!(
            reply.result,
            json!({"answers":{"q1":{"answers":["A"]},"q2":{"answers":["fixture answer"]}}})
        );
        assert!(g.consume(&b, None, &answers, 1).is_err());
        r["params"]["questions"][0]["isSecret"] = json!(true);
        assert!(gate().observe(&r, 0).is_err());
    }
}
