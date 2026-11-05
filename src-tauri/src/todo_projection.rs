//! Desktop/authenticated-owner projections only. No provider RPC, cold loading,
//! task mutation or new approval state. Stable native IDs remain authoritative.
use crate::host_application::{mobile_codex_request_projection, RouterCore};
use router_core::store::{codex_watch::CodexWatch, RouterStore};
use rusqlite::OptionalExtension;
use serde_json::{json, Value};

fn prompt(v: &Value) -> Option<String> {
    let text = v["questions"]
        .as_array()
        .map(|qs| {
            qs.iter()
                .filter_map(|q| {
                    q["question"]
                        .as_str()
                        .or(q["placeholder"].as_str())
                        .or(q["label"].as_str())
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .filter(|s| !s.is_empty())
        .or_else(|| {
            v["reason"]
                .as_str()
                .or(v["message"].as_str())
                .or(v["title"].as_str())
                .map(str::to_owned)
        });
    text.map(|s| s.chars().take(64000).collect())
}
fn question(
    id: &str,
    kind: &str,
    raw: &Value,
    w: &CodexWatch,
    child: Option<&str>,
    revision: u64,
    respond: bool,
    since: Option<i64>,
) -> Value {
    json!({"requestId":id,"nativeRequestId":id,"kind":kind,"prompt":prompt(raw),"childId":child,"generation":w.generation,"revision":revision,"canRespond":respond,"since":since})
}
fn native_async(core: &RouterCore, w: &CodexWatch) -> Vec<Value> {
    if uuid::Uuid::parse_str(&w.thread_id).is_err() {
        return vec![];
    }
    let Some(home) = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|p| std::path::PathBuf::from(p).join(".codex"))
        })
    else {
        return vec![];
    };
    let Ok(db) = rusqlite::Connection::open_with_flags(
        home.join("state_5.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return vec![];
    };
    let _ = db.busy_timeout(std::time::Duration::from_millis(20));
    let row: Option<(String, String, bool)> = db
        .query_row(
            "SELECT cwd,rollout_path,archived FROM threads WHERE id=?1",
            [&w.thread_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .ok()
        .flatten();
    let Some((cwd, path, false)) = row else {
        return vec![];
    };
    if !router_core::claude_code::same_root(&cwd, &w.cwd) {
        return vec![];
    }
    // Existing adapter's native reader enforces canonical sessions containment,
    // exact session header and answered IDs; this does not resume the thread.
    core.session
        .try_lock()
        .ok()
        .and_then(|s| {
            s.adapter
                .as_ref()
                .map(|a| a.read_async_owner_questions(&w.thread_id, Some(&path)))
        })
        .unwrap_or_default()
}
fn codex_pending(core: &RouterCore, w: &CodexWatch) -> (Vec<Value>, bool) {
    let mut out = vec![];
    let mut available = false;
    if let Ok(s) = core.session.try_lock() {
        available = !s.codex_adapter_borrowed && s.adapter.as_ref().is_some_and(|a| !a.is_closed());
        if available {
            for r in s
                .pending_codex_requests
                .values()
                .filter(|r| r.thread_id == w.thread_id && !r.responded)
            {
                let actionable = s.adapter.as_ref().is_some_and(|a| {
                    a.active_turn_id(&w.thread_id).as_deref() == Some(&r.turn_id)
                        || (a.is_shared_subscribed(&w.thread_id)
                            && a.observed_turn_id(&w.thread_id).as_deref() == Some(&r.turn_id))
                });
                if !actionable {
                    continue;
                }
                if let Some(view) = mobile_codex_request_projection(r) {
                    let raw = serde_json::to_value(&view).unwrap_or(Value::Null);
                    let mut item = question(
                        &r.action_id,
                        if view.kind == "USER_INPUT" {
                            "QUESTION"
                        } else if view.kind == "PERMISSIONS_APPROVAL" {
                            "PERMISSION"
                        } else {
                            "APPROVAL"
                        },
                        &raw,
                        w,
                        None,
                        r.revision,
                        true,
                        None,
                    );
                    item["nativeRequestId"] = r.raw_request_id.clone();
                    item["turnId"] = json!(r.turn_id);
                    out.push(item);
                }
            }
        }
    }
    for q in native_async(core, w)
        .into_iter()
        .filter(|q| q["responseSent"] == false)
    {
        let Some(id) = q["requestId"].as_str() else {
            continue;
        };
        // A locally offered/uncertain answer is fenced even before native echo.
        let attempted = q["answerId"]
            .as_str()
            .and_then(|id| core.store.watch_reply(id).ok().flatten())
            .is_some_and(|r| {
                matches!(
                    r.status.as_str(),
                    "QUEUED" | "SENDING" | "UNKNOWN" | "SENT" | "ACKNOWLEDGED"
                )
            });
        if !attempted {
            out.push(question(
                id,
                "QUESTION",
                &q,
                w,
                None,
                q["revision"].as_u64().unwrap_or(1),
                available,
                None,
            ));
        }
    }
    out.sort_by_key(|v| (v["childId"].to_string(), v["requestId"].to_string()));
    out.dedup_by(|a, b| a["requestId"] == b["requestId"] && a["childId"] == b["childId"]);
    (out, available)
}
pub(crate) fn watches(core: &RouterCore) -> Result<Vec<Value>, String> {
    crate::claude_chat::list(core)?
        .into_iter()
        .map(|row| {
            let w = &row.watch;
            let mut out = serde_json::to_value(&row).map_err(|_| "WATCH_RESPONSE_INVALID")?;
            out["unreadResult"] = json!(core.store.todo_unread_result(&w.thread_id)?);
            let (mut pending, mut available) = (vec![], true);
            if w.thread_id.starts_with("pi:") {
                let t = core.store.pi_task(&w.thread_id)?;
                let loaded = core.session.try_lock().ok().map(|s| s.pi.loaded(&t.id));
                available = loaded.is_some();
                let closed = core.store.todo_pi_closed_at(&t.id, t.generation)?;
                out["piLifecycle"] = lifecycle(&t.state, loaded, closed);
                // The native RPC currently cancels unsupported dialogs. Their
                // historical capture alone cannot manufacture an unanswered card.
                if closed.is_none() && t.state == "WAITING_DECISION" {
                    for q in core.store.pi_owner_questions(&t.id)? {
                        if q["disposition"] != "NATIVE_UI_UNAVAILABLE" {
                            continue;
                        }
                        let Some(id) = q["requestId"].as_str() else {
                            continue;
                        };
                        pending.push(question(
                            id,
                            "QUESTION",
                            &q,
                            w,
                            None,
                            1,
                            false,
                            q["observedAt"].as_i64(),
                        ));
                    }
                }
            } else if let Ok(sid) = router_core::store::claude_manual::native_id(&w.thread_id) {
                if w.enabled {
                    available = row
                        .task_tree
                        .as_ref()
                        .is_some_and(|tree| !tree.limited && tree.unknown == 0);
                    let mut targets = vec![(sid.to_owned(), None)];
                    if let Some(tree) = &row.task_tree {
                        targets.extend(
                            tree.nodes
                                .iter()
                                .map(|n| (n.session_id.clone(), Some(n.session_id.clone()))),
                        );
                    }
                    for (sid, child) in targets {
                        for q in
                            core.store
                                .todo_claude_requests(&w.thread_id, w.generation, &sid)?
                        {
                            let Some(id) = q["id"].as_str() else { continue };
                            let mut item = question(
                                id,
                                q["kind"].as_str().unwrap_or("QUESTION"),
                                &q["payload"],
                                w,
                                child.as_deref(),
                                1,
                                true,
                                q["since"].as_i64(),
                            );
                            item["nativeRequestId"] = q["nativeRequestId"].clone();
                            item["nativeSessionId"] = q["nativeSessionId"].clone();
                            pending.push(item);
                        }
                    }
                }
            } else if w.enabled {
                (pending, available) = codex_pending(core, w);
            }
            out["pending"] = json!(pending);
            out["pendingAvailable"] = json!(available);
            Ok(out)
        })
        .collect()
}
fn lifecycle(state: &str, loaded: Option<bool>, closed: Option<i64>) -> Value {
    let phase = if loaded == Some(true) {
        if matches!(state, "RUNNING" | "WAITING_DECISION" | "CREATING") {
            "ACTIVE"
        } else {
            "IDLE"
        }
    } else if loaded == Some(false) && closed.is_some() {
        "CLOSED"
    } else {
        "UNAVAILABLE"
    };
    json!({"state":phase,"loaded":loaded.unwrap_or(false),"runtimeKnown":loaded.is_some(),"closedAt":if phase=="CLOSED"{closed}else{None}})
}
pub(crate) fn computer_identity() -> Value {
    identity(std::env::var("COMPUTERNAME").ok().as_deref())
}
fn identity(name: Option<&str>) -> Value {
    json!({"platform":std::env::consts::OS,"features":{"pi":crate::platform::pi_offered()},"hostName":name.filter(|s|!s.trim().is_empty()).map(|s|s.chars().filter(|c|!c.is_control()).take(255).collect::<String>())})
}

pub(crate) fn bridge_activity(core: &RouterCore, ids: &[String]) -> Result<Vec<Value>, String> {
    crate::role_bridge::directory_activity(core, ids)?
        .into_iter()
        .map(|row| {
            let bindings = core.store.role_bridge(&row.workstream_id)?;
            let mut value = serde_json::to_value(&row).map_err(|_| "BRIDGE_RESPONSE_INVALID")?;
            if bindings.binding_revision != row.binding_revision {
                return Err("BRIDGE_BINDING_CHANGED_REFRESH".into());
            }
            let pending = ready_handoffs(&core.store, &bindings)?;
            if core.store.role_bridge(&row.workstream_id)?.binding_revision != row.binding_revision
            {
                return Err("BRIDGE_BINDING_CHANGED_REFRESH".into());
            }
            value["readyHandoffs"] = json!(pending);
            Ok(value)
        })
        .collect()
}
fn ready_handoffs(
    store: &RouterStore,
    b: &router_core::store::role_bridge::RoleBridge,
) -> Result<Vec<Value>, String> {
    store.todo_ready_handoffs(&b.workstream_id, b.binding_revision)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn t1_failed_and_unloaded_are_not_closed() {
        assert_eq!(lifecycle("FAILED", Some(true), None)["state"], "IDLE");
        assert_eq!(
            lifecycle("RESULT_READY", Some(false), None)["state"],
            "UNAVAILABLE"
        );
        assert_eq!(
            lifecycle("CANCELLED", Some(false), Some(12))["closedAt"],
            12
        );
        assert_eq!(
            lifecycle("RUNNING", Some(true), Some(12))["closedAt"],
            Value::Null
        );
    }
    #[test]
    fn t7_hostname_is_not_host_status() {
        assert_eq!(identity(None)["hostName"], Value::Null);
        assert_eq!(identity(Some("HOST-电脑"))["hostName"], "HOST-电脑");
        assert_eq!(identity(Some("a\nb"))["hostName"], "ab");
    }
    #[test]
    fn t2_native_prompt_not_title_guess() {
        assert_eq!(
            prompt(&json!({"questions":[{"question":"原生问题"},{"placeholder":"第二个"}]}))
                .as_deref(),
            Some("原生问题\n\n第二个")
        );
        assert_eq!(prompt(&json!({"preview":"猜测"})), None);
    }
    #[test]
    fn t4_unprepared_read_is_not_handoff_and_old_binding_is_excluded() {
        use router_core::store::role_bridge::RoleBindingInput;
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime/t-items-fixtures");
        std::fs::create_dir_all(&root).unwrap();
        let d = tempfile::tempdir_in(root).unwrap();
        let s = RouterStore::open_at(d.path().join("router.db")).unwrap();
        let p = s.create_project("fixture".into(), None).unwrap();
        let w = s.create_workstream(&p.id, "fixture".into()).unwrap();
        let side = |id: &str| RoleBindingInput {
            provider: "CODEX".into(),
            external_id: id.into(),
            label: "fixture".into(),
            cwd: Some(d.path().to_string_lossy().into()),
        };
        let b = s
            .bind_role_bridge(&w.id, 0, side("decision"), side("execution"))
            .unwrap();
        let obs = s
            .record_reply_observation(
                &w.id,
                &b.decision.as_ref().unwrap().endpoint.id,
                Some("native:one"),
                "原文",
                None,
            )
            .unwrap()
            .unwrap();
        let first = ready_handoffs(&s, &b).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0]["status"], "UNPREPARED");
        let h = s
            .prepare_role_handoff(&w.id, "DECISION", &obs.id, "reviewed")
            .unwrap();
        assert_eq!(ready_handoffs(&s, &b).unwrap()[0]["status"], "READY");
        s.approve_role_handoff_checked(&h.id, &h.payload_hash)
            .unwrap();
        assert_eq!(ready_handoffs(&s, &b).unwrap()[0]["status"], "APPROVED");
        s.transition_handoff(&h.id, "SENDING", None).unwrap();
        assert!(ready_handoffs(&s, &b).unwrap().is_empty());
        assert!(s
            .bind_role_bridge(
                &w.id,
                b.binding_revision,
                side("new-decision"),
                side("new-execution")
            )
            .is_err());
        s.transition_handoff(&h.id, "SENT", None).unwrap();
        assert!(ready_handoffs(&s, &b).unwrap().is_empty());
        let same = s
            .bind_role_bridge(
                &w.id,
                b.binding_revision,
                side("decision"),
                side("execution"),
            )
            .unwrap();
        assert!(ready_handoffs(&s, &same).unwrap().is_empty());
        let new = s
            .bind_role_bridge(
                &w.id,
                same.binding_revision,
                side("new-decision"),
                side("new-execution"),
            )
            .unwrap();
        assert!(ready_handoffs(&s, &new).unwrap().is_empty());
        for identity in ["native:second", "native:third"] {
            s.record_reply_observation(
                &w.id,
                &new.decision.as_ref().unwrap().endpoint.id,
                Some(identity),
                "new source",
                None,
            )
            .unwrap();
        }
        let sources = ready_handoffs(&s, &new).unwrap();
        assert_eq!(sources.len(), 2);
        assert!(sources.iter().all(|r| r["status"] == "UNPREPARED"));
    }
    #[test]
    fn t2_codex_projects_only_active_unanswered_native_ids_without_rpc() {
        use crate::host_application::{PendingCodexRequest, Session};
        use std::sync::{Arc, Mutex};
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime/t-items-fixtures");
        std::fs::create_dir_all(&root).unwrap();
        let d = tempfile::tempdir_in(root).unwrap();
        let store = Arc::new(RouterStore::open_at(d.path().join("router.db")).unwrap());
        let snap = router_core::store::codex_watch::WatchSnapshot {
            state: "RUNNING".into(),
            turn_id: Some("active".into()),
            item_id: None,
            text: String::new(),
        };
        store
            .enable_codex_watch("exact", "fixture", d.path().to_str().unwrap(), &snap)
            .unwrap();
        let script = r#"require('node:readline').createInterface({input:process.stdin}).on('line',s=>{let r=JSON.parse(s);if(r.id!==undefined)process.stdout.write(JSON.stringify({id:r.id,result:{}})+'\n')})"#;
        let mut command = std::process::Command::new("node");
        command.args(["-e", script]);
        let mut adapter =
            crate::codex::adapter::CodexAdapter::start_shared_validation(command, Arc::new(|_| {}))
                .unwrap();
        adapter.initialize().unwrap();
        adapter.mark_turn_started("exact", "active");
        let mut session = Session {
            adapter: Some(adapter),
            ..Default::default()
        };
        for (id, thread, turn, responded) in [
            ("live", "exact", "active", false),
            ("answered", "exact", "active", true),
            ("stale", "exact", "old", false),
            ("foreign", "other", "active", false),
        ] {
            session.pending_codex_requests.insert(
                id.into(),
                PendingCodexRequest {
                    action_id: id.into(),
                    raw_request_id: json!(id),
                    method: "item/tool/requestUserInput".into(),
                    thread_id: thread.into(),
                    turn_id: turn.into(),
                    item_id: Some("native-item".into()),
                    params: json!({"questions":[{"id":"native-question","question":"真实原文"}]}),
                    revision: 7,
                    responded,
                },
            );
        }
        let core = RouterCore {
            store,
            chatgpt: Arc::default(),
            session: Arc::new(Mutex::new(session)),
            completed_chatgpt_responses: Arc::default(),
        };
        let w = core.store.registered_codex_watch("exact").unwrap();
        let (rows, available) = codex_pending(&core, &w);
        assert!(available);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["requestId"], "live");
        assert_eq!(rows[0]["revision"], 7);
        assert_eq!(rows[0]["prompt"], "真实原文");
        assert_eq!(rows[0]["canRespond"], true);
        assert!(core
            .session
            .lock()
            .unwrap()
            .adapter
            .as_ref()
            .unwrap()
            .request_counts()
            .keys()
            .all(|m| m == "initialize"));
        core.shutdown_owned_adapter();
    }
}

#[cfg(test)]
#[path = "todo_projection_http_tests.rs"]
mod http_tests;
