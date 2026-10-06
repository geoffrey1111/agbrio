//! Sticky safety observation for native preparation, independent of token usage.
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct PreparationGuard(Arc<Mutex<State>>);
#[derive(Default)]
struct State {
    fault: Option<&'static str>,
    authorized_thread: Option<String>,
    observed_turn: Option<(String, String)>,
    turn_starts: u64,
    turn_request_attempted: bool,
}
impl PreparationGuard {
    pub fn check(&self) -> Result<(), String> {
        let state = self.0.lock().map_err(|_| "OBSERVATION_FAILED")?;
        state.fault.map_or(Ok(()), |reason| Err(reason.into()))
    }
    pub fn fail(&self, reason: &'static str) {
        if let Ok(mut state) = self.0.lock() {
            if state.fault.is_none() {
                state.fault = Some(reason);
            }
        }
    }
    /// Called by application only after the durable one-shot claim and all
    /// native preparation checks. It never clears an observation failure.
    pub fn authorize_exact_turn(&self, thread_id: &str) -> Result<(), String> {
        self.check()?;
        if thread_id.is_empty() {
            return Err("CAPABILITY_UNVERIFIED".into());
        }
        let mut state = self.0.lock().map_err(|_| "OBSERVATION_FAILED")?;
        if let Some(reason) = state.fault {
            return Err(reason.into());
        }
        if state.authorized_thread.is_some() {
            return Err("RUN_IN_FLIGHT".into());
        }
        state.authorized_thread = Some(thread_id.into());
        Ok(())
    }
    pub fn reserve_turn_request(&self, thread_id: &str) -> Result<(), String> {
        let mut state = self.0.lock().map_err(|_| "OBSERVATION_FAILED")?;
        if let Some(fault) = state.fault {
            return Err(fault.into());
        }
        if state.authorized_thread.as_deref() != Some(thread_id) || state.turn_request_attempted {
            return Err("TURN_NOT_AUTHORIZED_OR_ALREADY_ATTEMPTED".into());
        }
        state.turn_request_attempted = true;
        Ok(())
    }
    /// Executed synchronously by the stdout reader before queuing events/ACKs.
    pub fn observe(&self, message: &Value) {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return;
        };
        let params = message.get("params").unwrap_or(&Value::Null);
        let thread = params.get("threadId").and_then(Value::as_str);
        let turn = params
            .pointer("/turn/id")
            .or_else(|| params.get("turnId"))
            .and_then(Value::as_str);
        let mut state = match self.0.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        if method == "turn/started" {
            state.turn_starts += 1;
            if let (Some(t), Some(u)) = (thread, turn) {
                if state
                    .observed_turn
                    .as_ref()
                    .is_some_and(|(old_t, old_u)| old_t != t || old_u != u)
                {
                    if state.fault.is_none() {
                        state.fault = Some("UNEXPECTED_ADDITIONAL_TURN");
                    }
                } else {
                    state.observed_turn = Some((t.into(), u.into()));
                }
            } else if state.fault.is_none() {
                state.fault = Some("CAPABILITY_UNVERIFIED");
            }
        }
        let executing = method.starts_with("turn/")
            || method.starts_with("item/")
            || method.starts_with("codex/event/")
            || message.get("id").is_some();
        let active_goal = method == "thread/goal/updated"
            && !matches!(
                params.pointer("/goal/status").and_then(Value::as_str),
                Some("paused" | "blocked")
            );
        let active_thread = method == "thread/status/changed"
            && params.pointer("/status/type").and_then(Value::as_str) == Some("active");
        let owned = state
            .authorized_thread
            .as_deref()
            .is_some_and(|allowed| thread == Some(allowed));
        if state.fault.is_none()
            && ((executing && !owned) || active_goal || (active_thread && !owned))
        {
            state.fault = Some("UNEXPECTED_NATIVE_EXECUTION");
        }
    }
    pub(crate) fn prove_no_turn_write_after_exit(&self) -> Result<(), String> {
        let s = self.0.lock().map_err(|_| "OBSERVATION_FAILED")?;
        if s.fault.is_some() || s.turn_request_attempted || s.observed_turn.is_some() {
            return Err("PREWRITE_PROOF_UNAVAILABLE".into());
        }
        Ok(())
    }
    pub fn turn_starts(&self) -> u64 {
        self.0.lock().map(|s| s.turn_starts).unwrap_or(u64::MAX)
    }
    pub fn observed_turn(&self) -> Option<(String, String)> {
        self.0.lock().ok().and_then(|s| s.observed_turn.clone())
    }
}

/// Get is allowed before any read/resume; failure or unknown state is a stop,
/// never permission to pause/clear an existing user's Goal.
pub fn verify_existing_goal(response: &Value, expected_thread: &str) -> Result<(), String> {
    match response.get("goal") {
        Some(Value::Null) => Ok(()),
        Some(goal)
            if goal.get("threadId").and_then(Value::as_str) == Some(expected_thread)
                && matches!(
                    goal.get("status").and_then(Value::as_str),
                    Some("paused" | "blocked")
                ) =>
        {
            Ok(())
        }
        _ => Err("CAPABILITY_UNVERIFIED".into()),
    }
}
pub fn validate_initial_paused(
    params: &Value,
    expected_thread: &str,
    objective: &str,
) -> Result<(), String> {
    let object = params.as_object().ok_or("CAPABILITY_UNVERIFIED")?;
    if object.len() != 4
        || params.get("threadId").and_then(Value::as_str) != Some(expected_thread)
        || params.get("objective").and_then(Value::as_str) != Some(objective)
        || params.get("status").and_then(Value::as_str) != Some("paused")
        || params.get("tokenBudget") != Some(&Value::Null)
    {
        return Err("CAPABILITY_UNVERIFIED".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn direct_paused_only_and_existing_active_unknown_stop() {
        let good =
            json!({"threadId":"own","objective":"init","status":"paused","tokenBudget":null});
        validate_initial_paused(&good, "own", "init").unwrap();
        for bad in [
            json!(null),
            json!("active"),
            json!("complete"),
            json!("blocked"),
            json!(""),
        ] {
            let mut p = good.clone();
            p["status"] = bad;
            assert!(validate_initial_paused(&p, "own", "init").is_err());
        }
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove("status");
        assert!(validate_initial_paused(&missing, "own", "init").is_err());
        assert!(verify_existing_goal(&json!({"goal":null}), "own").is_ok());
        assert!(
            verify_existing_goal(&json!({"goal":{"threadId":"own","status":"paused"}}), "own")
                .is_ok()
        );
        for v in [
            json!({}),
            json!({"goal":{"status":"active"}}),
            json!({"goal":{}}),
        ] {
            assert!(verify_existing_goal(&v, "own").is_err());
        }
    }
    #[test]
    fn existing_blocked_goal_is_observed_not_initialized_or_mutated() {
        // Pinned ext/goal runtime restore/idle and extension on_turn_start only
        // activate Active (or account BudgetLimited), never Blocked.
        verify_existing_goal(
            &json!({"goal":{"threadId":"own","status":"blocked"}}),
            "own",
        )
        .unwrap();
        assert!(verify_existing_goal(
            &json!({"goal":{"threadId":"other","status":"blocked"}}),
            "own"
        )
        .is_err());
        let guard = PreparationGuard::default();
        guard.observe(&json!({"method":"thread/goal/updated","params":{"goal":{"threadId":"own","status":"blocked"}}}));
        guard.check().unwrap();
        assert!(validate_initial_paused(
            &json!({"threadId":"own","objective":"init","status":"blocked","tokenBudget":null}),
            "own",
            "init"
        )
        .is_err());
        guard.observe(
            &json!({"method":"thread/goal/updated","params":{"goal":{"status":"active"}}}),
        );
        assert!(guard.check().is_err());
    }
    #[test]
    fn unexpected_activity_and_observation_faults_are_sticky() {
        for msg in [
            json!({"method":"turn/started","params":{"threadId":"own","turn":{"id":"t"}}}),
            json!({"method":"item/started","params":{}}),
            json!({"id":9,"method":"item/tool/requestUserInput"}),
            json!({"method":"thread/goal/updated","params":{"goal":{"status":"active"}}}),
        ] {
            let guard = PreparationGuard::default();
            guard.observe(&msg);
            assert!(guard.check().is_err());
            assert!(guard.authorize_exact_turn("own").is_err());
            guard.observe(&json!({"method":"thread/goal/cleared"}));
            assert!(guard.check().is_err());
        }
        for reason in ["FRAME_CORRUPT", "QUEUE_OVERFLOW", "OBSERVATION_CLOSED"] {
            let guard = PreparationGuard::default();
            guard.fail(reason);
            assert_eq!(guard.check().unwrap_err(), reason);
        }
    }
    #[test]
    fn approved_execution_remains_exact_thread_and_goal_does_not_authorize_it() {
        let guard = PreparationGuard::default();
        guard.observe(
            &json!({"method":"thread/goal/updated","params":{"goal":{"status":"paused"}}}),
        );
        guard.check().unwrap();
        guard.authorize_exact_turn("own").unwrap();
        assert!(guard.reserve_turn_request("other").is_err());
        guard.reserve_turn_request("own").unwrap();
        assert!(guard.reserve_turn_request("own").is_err());
        guard.observe(
            &json!({"method":"turn/started","params":{"threadId":"own","turn":{"id":"t"}}}),
        );
        guard.check().unwrap();
        assert_eq!(guard.turn_starts(), 1);
        guard.observe(&json!({"method":"item/started","params":{"threadId":"other"}}));
        assert!(guard.check().is_err());
    }
}
