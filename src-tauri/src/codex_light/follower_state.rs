//! Keep only observation metadata from Desktop. Streaming text/history must not
//! trigger another full Desktop snapshot or be retained in this mirror.
use serde::Deserialize;
use serde_json::{json, value::RawValue, Value};

#[derive(Deserialize)]
pub(super) struct Envelope<'a> {
    #[serde(rename = "type")]
    pub kind: &'a str,
    #[serde(default)]
    pub method: Option<&'a str>,
    #[serde(rename = "sourceClientId", default)]
    pub source: Option<&'a str>,
    #[serde(borrow)]
    pub params: Option<&'a RawValue>,
}
#[derive(Deserialize)]
pub(super) struct Packet<'a> {
    #[serde(rename = "conversationId")]
    pub thread: &'a str,
    #[serde(rename = "hostId", default)]
    pub host: Option<&'a str>,
    #[serde(borrow)]
    pub change: &'a RawValue,
}
#[derive(Deserialize)]
struct Change<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    revision: u64,
    #[serde(rename = "baseRevision", default)]
    base: Option<u64>,
    #[serde(rename = "conversationState", borrow, default)]
    state: Option<&'a RawValue>,
    #[serde(borrow, default)]
    patches: Vec<Patch<'a>>,
}
#[derive(Deserialize)]
struct Patch<'a> {
    op: &'a str,
    path: Vec<Value>,
    #[serde(borrow, default)]
    value: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Thin {
    #[serde(rename = "rolloutPath", default)]
    rollout: Option<String>,
    #[serde(rename = "threadRuntimeStatus")]
    status: Value,
    #[serde(default)]
    requests: Option<Vec<Value>>,
}
fn thin(raw: &RawValue) -> Result<Value, ()> {
    let v: Thin = serde_json::from_str(raw.get()).map_err(|_| ())?;
    if !v.status.is_object() || !v.status["type"].is_string() {
        return Err(());
    }
    Ok(
        json!({"rolloutPath":v.rollout,"threadRuntimeStatus":v.status,"requests":v.requests.unwrap_or_default()}),
    )
}

pub(super) enum Update {
    State(Value),
    Quiet,
    Resync,
}
#[derive(Default)]
pub(super) struct Mirror {
    revision: Option<u64>,
    state: Option<Value>,
    waiting: bool,
}
impl Mirror {
    pub fn ingest(&mut self, raw: &RawValue) -> Update {
        match self.apply(raw) {
            Ok(v) => v,
            Err(()) => {
                if self.waiting {
                    Update::Quiet
                } else {
                    self.waiting = true;
                    Update::Resync
                }
            }
        }
    }
    fn apply(&mut self, raw: &RawValue) -> Result<Update, ()> {
        let change: Change = serde_json::from_str(raw.get()).map_err(|_| ())?;
        if change.kind == "snapshot" {
            if self.revision.is_some_and(|r| change.revision < r) {
                return Ok(Update::Quiet);
            }
            let state = thin(change.state.ok_or(())?)?;
            self.state = Some(state.clone());
            self.revision = Some(change.revision);
            self.waiting = false;
            return Ok(Update::State(state));
        }
        if change.kind != "patches" {
            return Err(());
        }
        if self.waiting {
            return Ok(Update::Quiet);
        }
        if self.revision != change.base
            || self.state.is_none()
            || change.revision <= change.base.ok_or(())?
        {
            return Err(());
        }
        let mut pending: Option<Value> = None;
        for patch in &change.patches {
            if patch.path.is_empty() {
                pending = Some(thin(patch.value.ok_or(())?)?);
                continue;
            }
            let first = patch.path[0].as_str().ok_or(())?;
            if matches!(first, "requests" | "threadRuntimeStatus" | "rolloutPath") {
                let value = patch
                    .value
                    .map(|v| serde_json::from_str(v.get()).map_err(|_| ()))
                    .transpose()?;
                let state = pending.get_or_insert_with(|| self.state.as_ref().unwrap().clone());
                apply_at(state, &patch.path, patch.op, value)?;
            }
        }
        let Some(mut state) = pending else {
            self.revision = Some(change.revision);
            return Ok(Update::Quiet);
        };
        if state["requests"].is_null() {
            state["requests"] = json!([]);
        }
        if !state["threadRuntimeStatus"].is_object()
            || !state["threadRuntimeStatus"]["type"].is_string()
            || !state["requests"].is_array()
        {
            return Err(());
        }
        self.revision = Some(change.revision);
        let changed = self.state.as_ref() != Some(&state);
        self.state = Some(state.clone());
        Ok(if changed {
            Update::State(state)
        } else {
            Update::Quiet
        })
    }
}
fn apply_at(root: &mut Value, path: &[Value], op: &str, value: Option<Value>) -> Result<(), ()> {
    let (first, rest) = path.split_first().ok_or(())?;
    if !rest.is_empty() {
        let child = match root {
            Value::Object(map) => map.get_mut(first.as_str().ok_or(())?).ok_or(())?,
            Value::Array(rows) => rows.get_mut(first.as_u64().ok_or(())? as usize).ok_or(())?,
            _ => return Err(()),
        };
        return apply_at(child, rest, op, value);
    }
    match root {
        Value::Object(map) => {
            let key = first.as_str().ok_or(())?;
            match op {
                "add" => {
                    map.insert(key.into(), value.ok_or(())?);
                }
                "replace" => {
                    if !map.contains_key(key) {
                        return Err(());
                    }
                    map.insert(key.into(), value.ok_or(())?);
                }
                "remove" => {
                    map.remove(key).ok_or(())?;
                }
                _ => return Err(()),
            }
        }
        Value::Array(rows) => {
            let i = first.as_u64().ok_or(())? as usize;
            match op {
                "add" if i <= rows.len() => rows.insert(i, value.ok_or(())?),
                "replace" if i < rows.len() => rows[i] = value.ok_or(())?,
                "remove" if i < rows.len() => {
                    rows.remove(i);
                }
                _ => return Err(()),
            }
        }
        _ => return Err(()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw(value: Value) -> Box<RawValue> {
        RawValue::from_string(value.to_string()).unwrap()
    }
    fn base() -> Value {
        json!({"type":"snapshot","revision":1,"conversationState":{"rolloutPath":"fixture.jsonl","threadRuntimeStatus":{"type":"active"},"requests":[],"turnHistory":{"huge":"x".repeat(1024*1024)}}})
    }
    #[test]
    fn history_updates_advance_revision_without_resubscribing_or_retaining_history() {
        let mut m = Mirror::default();
        let Update::State(state) = m.ingest(&raw(base())) else {
            panic!()
        };
        assert!(state.to_string().len() < 200);
        assert!(state.get("turnHistory").is_none());
        for revision in 2..=101 {
            let u=m.ingest(&raw(json!({"type":"patches","baseRevision":revision-1,"revision":revision,"patches":[{"op":"replace","path":["turnHistory","history","entitiesByKey","turn:t","items",0,"text"],"value":"streaming"}]})));
            assert!(matches!(u, Update::Quiet));
        }
        assert_eq!(m.revision, Some(101));
        assert!(m.state.as_ref().unwrap().to_string().len() < 200);
    }
    #[test]
    fn decisions_status_and_exact_numeric_ids_survive_patch_projection() {
        let mut m = Mirror::default();
        m.ingest(&raw(base()));
        let u=m.ingest(&raw(json!({"type":"patches","baseRevision":1,"revision":2,"patches":[{"op":"add","path":["requests",0],"value":{"id":42,"method":"item/commandExecution/requestApproval","params":{"turnId":"turn","itemId":"item"}}},{"op":"replace","path":["threadRuntimeStatus","type"],"value":"idle"}]})));
        let Update::State(s) = u else { panic!() };
        assert_eq!(s["requests"][0]["id"], 42);
        assert_eq!(s["threadRuntimeStatus"]["type"], "idle");
        let Update::State(s)=m.ingest(&raw(json!({"type":"patches","baseRevision":2,"revision":3,"patches":[{"op":"remove","path":["requests",0]}]})))else{panic!()};
        assert_eq!(s["requests"], json!([]));
    }
    #[test]
    fn a_gap_requests_one_resync_and_never_clears_pending_decisions() {
        let mut m = Mirror::default();
        m.ingest(&raw(base()));
        let gap = raw(json!({"type":"patches","baseRevision":3,"revision":4,"patches":[]}));
        assert!(matches!(m.ingest(&gap), Update::Resync));
        assert!(matches!(m.ingest(&gap), Update::Quiet));
        assert_eq!(m.revision, Some(1));
        assert!(matches!(m.ingest(&raw(base())), Update::State(_)));
    }
    #[test]
    fn json_field_order_and_root_replacement_are_supported() {
        let mut m = Mirror::default();
        m.ingest(&raw(base()));
        let unordered=RawValue::from_string(r#"{"patches":[{"value":"idle","path":["threadRuntimeStatus","type"],"op":"replace"}],"revision":2,"type":"patches","baseRevision":1}"#.into()).unwrap();
        assert!(matches!(m.ingest(&unordered), Update::State(_)));
        assert!(matches!(m.ingest(&raw(json!({"type":"patches","baseRevision":2,"revision":3,"patches":[{"op":"replace","path":[],"value":{"threadRuntimeStatus":{"type":"active"},"requests":[]}}]}))),Update::State(_)));
    }
}

#[cfg(test)]
mod quiet_tests {
    use super::*;
    #[test]
    fn identical_metadata_is_quiet_but_requests_remain_authoritative() {
        let mut m = Mirror::default();
        let snapshot=RawValue::from_string(r#"{"type":"snapshot","revision":1,"conversationState":{"threadRuntimeStatus":{"type":"active"},"requests":[]}}"#.into()).unwrap();
        m.ingest(&snapshot);
        let patch=RawValue::from_string(r#"{"type":"patches","baseRevision":1,"revision":2,"patches":[{"op":"replace","path":["threadRuntimeStatus","type"],"value":"active"}]}"#.into()).unwrap();
        assert!(matches!(m.ingest(&patch), Update::Quiet));
        assert_eq!(m.revision, Some(2));
    }
}
