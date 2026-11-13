//! Native Immer-compatible state patches. One baseline per thread, never per-client copies.
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

pub(super) const STATE_LIMIT: usize = 8 * 1024 * 1024;
const CHANGE_LIMIT: usize = STATE_LIMIT + 4096;
const PATCH_LIMIT: usize = 2048;

struct Size {
    used: usize,
    limit: usize,
}
impl Write for Size {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if b.len() > self.limit.saturating_sub(self.used) {
            return Err(io::Error::other("preview budget"));
        }
        self.used += b.len();
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn fits(value: &Value, limit: usize) -> bool {
    serde_json::to_writer(&mut Size { used: 0, limit }, value).is_ok()
}

/// Read-only display preview. Native history and the original backend event are untouched.
pub(super) fn preview_item(mut item: Value) -> Value {
    if item["type"] == "userMessage" {
        return item;
    }
    for key in ["text", "aggregatedOutput", "output"] {
        if let Some(text) = item.get(key).and_then(Value::as_str) {
            let limit = if item["type"] == "agentMessage" {
                256 * 1024
            } else {
                16 * 1024
            };
            if text.len() > limit {
                item[key] = json!(preview_text(text, limit));
            }
        }
    }
    // App-server tool content uses a text/image union. Replacing an oversized tool
    // image by an explicitly labelled text preview keeps the native wire shape valid.
    for part in item
        .get_mut("content")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if part["type"] == "text" {
            if let Some(text) = part["text"].as_str().filter(|s| s.len() > 16 * 1024) {
                part["text"] = json!(preview_text(text, 16 * 1024));
            }
        } else if part["type"] == "image" && !fits(part, 256 * 1024) {
            *part = json!({"type":"text","text":"[Agbrio 实时预览：图片内容较大；完整图片保留在原生对话历史，任务结束交还桌面后可读取。]"});
        }
    }
    item
}
pub(super) fn preview_text(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.into();
    }
    let marker = "[Agbrio 实时预览已缩短；完整内容保留在原生对话历史]\n";
    let mut start = text
        .len()
        .saturating_sub(limit.saturating_sub(marker.len()));
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("{marker}{}", &text[start..])
}

/// Keep pending/unfinished items and exact user input, plus a bounded recent
/// window of completed display items. This affects only the temporary replica.
pub(super) fn trim_items(turn: &mut Value, protected: &HashSet<String>) {
    let Some(items) = turn["items"].as_array() else {
        return;
    };
    let completed = &turn["completedItemIds"];
    let mut keep = HashSet::new();
    let mut bytes = 0;
    let mut recent = 0;
    for item in items.iter().rev() {
        let Some(id) = item["id"].as_str() else {
            continue;
        };
        let required =
            item["type"] == "userMessage" || completed[id] != true || protected.contains(id);
        let size = Size {
            used: 0,
            limit: 768 * 1024,
        };
        let mut size = size;
        let fits = serde_json::to_writer(&mut size, item).is_ok();
        if required || (recent < 64 && fits && bytes + size.used <= 768 * 1024) {
            keep.insert(id.to_owned());
            if !required {
                bytes += size.used;
                recent += 1;
            }
        }
    }
    if let Some(items) = turn["items"].as_array_mut() {
        items.retain(|v| v["id"].as_str().is_none_or(|id| keep.contains(id)));
    }
    if let Some(done) = turn["completedItemIds"].as_object_mut() {
        done.retain(|id, _| keep.contains(id));
    }
}

fn item_ids(values: &[Value]) -> Option<Vec<&str>> {
    let ids: Vec<_> = values
        .iter()
        .map(|v| v.get("id")?.as_str())
        .collect::<Option<_>>()?;
    let unique: HashSet<_> = ids.iter().copied().collect();
    (unique.len() == ids.len()).then_some(ids)
}
/// History windows slide forward; remove omitted IDs and append the new items
/// instead of replacing all remaining array indexes (and their large bodies).
fn diff_items(a: &[Value], b: &[Value], path: &mut Vec<Value>, out: &mut Vec<Value>) -> bool {
    let (Some(old), Some(new)) = (item_ids(a), item_ids(b)) else {
        return false;
    };
    let old_set: HashSet<_> = old.iter().copied().collect();
    let new_set: HashSet<_> = new.iter().copied().collect();
    let retained: Vec<_> = old
        .iter()
        .copied()
        .filter(|id| new_set.contains(id))
        .collect();
    if retained
        != new
            .iter()
            .copied()
            .filter(|id| old_set.contains(id))
            .collect::<Vec<_>>()
    {
        return false;
    }
    for i in (0..a.len()).rev().filter(|i| !new_set.contains(old[*i])) {
        path.push(json!(i));
        out.push(json!({"op":"remove","path":path}));
        path.pop();
    }
    for (i, item) in b.iter().enumerate() {
        path.push(json!(i));
        if let Some(before) = old.iter().position(|id| *id == new[i]) {
            diff(&a[before], item, path, out);
        } else {
            out.push(json!({"op":"add","path":path,"value":item}));
        }
        path.pop();
    }
    true
}

fn diff(old: &Value, new: &Value, path: &mut Vec<Value>, out: &mut Vec<Value>) {
    if old == new || out.len() > PATCH_LIMIT {
        return;
    }
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => {
            // Immer rejects prototype-mutating paths. Treat untrusted objects with
            // these keys as atomic values, never as individual property assignments.
            if a.keys()
                .chain(b.keys())
                .any(|k| matches!(k.as_str(), "__proto__" | "constructor" | "prototype"))
            {
                out.push(json!({"op":"replace","path":path,"value":new}));
                return;
            }
            for key in a.keys().filter(|k| !b.contains_key(*k)) {
                path.push(json!(key));
                out.push(json!({"op":"remove","path":path}));
                path.pop();
            }
            for (key, value) in b {
                path.push(json!(key));
                if let Some(before) = a.get(key) {
                    diff(before, value, path, out);
                } else {
                    out.push(json!({"op":"add","path":path,"value":value}));
                }
                path.pop();
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if diff_items(a, b, path, out) {
                return;
            }
            for i in (b.len()..a.len()).rev() {
                path.push(json!(i));
                out.push(json!({"op":"remove","path":path}));
                path.pop();
            }
            for (i, value) in b.iter().enumerate() {
                path.push(json!(i));
                if let Some(before) = a.get(i) {
                    diff(before, value, path, out);
                } else {
                    out.push(json!({"op":"add","path":path,"value":value}));
                }
                path.pop();
            }
        }
        _ => out.push(json!({"op":"replace","path":path,"value":new})),
    }
}

#[derive(Default)]
pub(super) struct Stream {
    baseline: Option<Value>,
    revision: u64,
    clients: HashMap<String, u64>,
}
impl Stream {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn forget(&mut self, client: &str) {
        self.clients.remove(client);
    }
    pub fn reset_clients(&mut self) {
        self.clients.clear();
    }
    /// Per-connection advertised revisions, not delivery acknowledgements.
    /// New/rejoining clients receive a full baseline; reconnect clears the client map.
    pub fn changes(
        &mut self,
        state: Value,
        targets: &[String],
        resync: &HashSet<String>,
    ) -> Vec<(Vec<String>, Value)> {
        if targets.is_empty() || !fits(&state, STATE_LIMIT) {
            return vec![];
        }
        let base = self.revision;
        let changed = self.baseline.as_ref() != Some(&state);
        let mut patches = vec![];
        if changed {
            if let Some(before) = &self.baseline {
                diff(before, &state, &mut vec![], &mut patches);
            }
            self.revision += 1;
        }
        let patch = json!({"type":"patches","baseRevision":base,"revision":self.revision,"patches":patches});
        let use_patch = self.baseline.is_some()
            && patches_len(&patch) <= PATCH_LIMIT
            && fits(&patch, CHANGE_LIMIT);
        let mut incremental = vec![];
        let mut snapshots = vec![];
        for client in targets {
            if self.clients.get(client) == Some(&base) && use_patch && !resync.contains(client) {
                if changed {
                    incremental.push(client.clone());
                }
            } else if resync.contains(client)
                || changed
                || self.clients.get(client) != Some(&self.revision)
            {
                snapshots.push(client.clone());
            }
            self.clients.insert(client.clone(), self.revision);
        }
        let mut out = vec![];
        if !incremental.is_empty() {
            out.push((incremental, patch));
        }
        if !snapshots.is_empty() {
            out.push((
                snapshots,
                json!({"type":"snapshot","revision":self.revision,"conversationState":state}),
            ));
        }
        self.baseline = Some(state);
        out
    }
}
fn patches_len(value: &Value) -> usize {
    value["patches"].as_array().map_or(0, Vec::len)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn clients(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn initial_snapshot_then_only_changed_paths_and_no_duplicate_updates() {
        let mut stream = Stream::default();
        let target = clients(&["a"]);
        let empty = HashSet::new();
        let state = json!({"largeUnchanged":"x".repeat(512*1024),"progress":{"text":"a"}});
        let first = stream.changes(state.clone(), &target, &empty);
        assert_eq!(first[0].1["type"], "snapshot");
        let mut next = state.clone();
        next["progress"]["text"] = json!("ab");
        let second = stream.changes(next.clone(), &target, &empty);
        assert_eq!(second[0].1["type"], "patches");
        assert_eq!(second[0].1["baseRevision"], 1);
        assert_eq!(second[0].1["revision"], 2);
        assert_eq!(
            second[0].1["patches"],
            json!([{"op":"replace","path":["progress","text"],"value":"ab"}])
        );
        assert!(second[0].1.to_string().len() < 256);
        assert!(stream.changes(next, &target, &empty).is_empty());
    }
    #[test]
    fn joining_and_resync_do_not_break_existing_followers() {
        let mut stream = Stream::default();
        let empty = HashSet::new();
        stream.changes(json!({"x":1}), &clients(&["a"]), &empty);
        let joined = stream.changes(json!({"x":1}), &clients(&["a", "b"]), &empty);
        assert_eq!(joined.len(), 1);
        assert_eq!(joined[0].0, clients(&["b"]));
        assert_eq!(stream.revision(), 1);
        let forced = stream.changes(
            json!({"x":2}),
            &clients(&["a", "b"]),
            &HashSet::from(["b".to_string()]),
        );
        assert_eq!(forced.len(), 2);
        assert_eq!(forced[0].0, clients(&["a"]));
        assert_eq!(forced[0].1["type"], "patches");
        assert_eq!(forced[1].0, clients(&["b"]));
        let next = stream.changes(json!({"x":3}), &clients(&["a", "b"]), &empty);
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].0, clients(&["a", "b"]));
        assert_eq!(next[0].1["baseRevision"], 2);
        stream.forget("a");
        assert_eq!(
            stream.changes(json!({"x":3}), &clients(&["a"]), &empty)[0].1["type"],
            "snapshot"
        );
        stream.reset_clients();
        assert_eq!(
            stream.changes(json!({"x":3}), &clients(&["b"]), &empty)[0].1["type"],
            "snapshot"
        );
    }
    #[test]
    fn arrays_use_numeric_paths_and_removals_run_backwards() {
        let mut patches = vec![];
        diff(
            &json!({"arr":[1,2,3],"gone":true}),
            &json!({"arr":[4],"new":5}),
            &mut vec![],
            &mut patches,
        );
        assert!(patches.contains(&json!({"op":"remove","path":["arr",2]})));
        assert!(patches.contains(&json!({"op":"replace","path":["arr",0],"value":4})));
        let removals: Vec<_> = patches
            .iter()
            .filter(|p| p["op"] == "remove" && p["path"][0] == "arr")
            .map(|p| p["path"][1].as_u64().unwrap())
            .collect();
        assert_eq!(removals, [2, 1]);
    }
    #[test]
    fn oversize_state_is_not_queued_or_advertised_and_a_later_small_state_recovers() {
        let mut stream = Stream::default();
        let target = clients(&["a"]);
        let empty = HashSet::new();
        stream.changes(json!({"x":1}), &target, &empty);
        assert!(stream
            .changes(json!({"x":"x".repeat(STATE_LIMIT+1)}), &target, &empty)
            .is_empty());
        assert_eq!(stream.revision(), 1);
        let next = stream.changes(json!({"x":2}), &target, &empty);
        assert_eq!(next[0].1["baseRevision"], 1);
        assert_eq!(next[0].1["revision"], 2);
    }
    #[test]
    fn live_output_previews_are_bounded_utf8_and_do_not_modify_the_native_event() {
        let original =
            json!({"id":"t","type":"commandExecution","aggregatedOutput":"中文".repeat(10000)});
        let preview = preview_item(original.clone());
        let text = preview["aggregatedOutput"].as_str().unwrap();
        assert!(text.len() <= 16 * 1024);
        assert!(text.starts_with("[Agbrio"));
        assert!(original["aggregatedOutput"].as_str().unwrap().len() > 16 * 1024);
        let user =
            json!({"id":"u","type":"userMessage","content":[{"type":"text","text":"user text"}]});
        assert_eq!(preview_item(user.clone()), user);
        let image = json!({"type":"dynamicToolCall","content":[{"type":"image","imageUrl":"x".repeat(300*1024)}]});
        let p = preview_item(image);
        assert_eq!(p["content"][0]["type"], "text");
    }
    #[test]
    fn many_path_changes_fall_back_to_a_single_bounded_snapshot() {
        let mut stream = Stream::default();
        let empty = HashSet::new();
        let targets = clients(&["a"]);
        stream.changes(json!({"a":vec![0;3000]}), &targets, &empty);
        let changes = stream.changes(json!({"a":vec![1;3000]}), &targets, &empty);
        assert_eq!(changes[0].1["type"], "snapshot");
        assert!(fits(&changes[0].1, CHANGE_LIMIT));
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;
    #[test]
    fn sliding_item_window_keeps_existing_large_items_structurally_shared() {
        let mut stream = Stream::default();
        let targets = vec!["a".into()];
        let empty = HashSet::new();
        let large = "x".repeat(128 * 1024);
        let a = json!({"items":[{"id":"user","text":"prompt"},{"id":"old","text":large},{"id":"keep","text":large}]});
        let b = json!({"items":[{"id":"user","text":"prompt"},{"id":"keep","text":large},{"id":"new","text":"new output"}]});
        stream.changes(a, &targets, &empty);
        let change = stream.changes(b, &targets, &empty);
        let p = &change[0].1["patches"];
        assert!(p
            .as_array()
            .unwrap()
            .contains(&json!({"op":"remove","path":["items",1]})));
        assert!(change[0].1.to_string().len() < 512);
    }
    #[test]
    fn prototype_keys_are_atomic_and_never_appear_in_patch_paths() {
        let mut patches = vec![];
        diff(
            &json!({"data":{"__proto__":1}}),
            &json!({"data":{"__proto__":2}}),
            &mut vec![],
            &mut patches,
        );
        assert_eq!(
            patches,
            json!([{"op":"replace","path":["data"],"value":{"__proto__":2}}])
                .as_array()
                .unwrap()
                .clone()
        );
    }
    #[test]
    fn completed_preview_window_is_bounded_but_current_work_and_input_survive() {
        let items:Vec<Value>=(0..120).map(|i|json!({"id":format!("i{i}"),"type":"commandExecution","aggregatedOutput":"x".repeat(16384)})).collect();
        let done: serde_json::Map<_, _> = items
            .iter()
            .map(|i| (i["id"].as_str().unwrap().into(), json!(true)))
            .collect();
        let mut turn = json!({"items":items,"completedItemIds":done});
        turn["items"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"running","type":"commandExecution"}));
        trim_items(&mut turn, &HashSet::from(["i0".into()]));
        let items = turn["items"].as_array().unwrap();
        assert!(items.len() <= 66);
        assert!(items.iter().any(|i| i["id"] == "i0"));
        assert!(items.iter().any(|i| i["id"] == "running"));
        assert!(fits(&turn, 1024 * 1024));
    }
}
