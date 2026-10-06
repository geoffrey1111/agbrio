//! Ordered native preparation and direct-paused creation. No initialization turn.
use super::roots::RootIdentity;
use crate::{
    codex::{adapter::CandidateCleanup, preparation::verify_existing_goal},
    store::{
        control::{
            binding::{BindingGrant, VerifiedBindingTarget},
            PermissionMode, PolicySnapshot,
        },
        RouterStore,
    },
};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

pub const INITIAL_OBJECTIVE: &str =
    "Router initialization only. No engineering task; clear before binding.";
pub const OBSERVATION_WINDOW: Duration = Duration::from_secs(2);
/// Implemented by the bounded owned transport, or a deterministic fixture.
/// The production implementation observes from spawn and checks before every RPC.
pub trait NativeSession {
    fn request(&mut self, method: &str, params: Value) -> Result<Value, String>;
    /// Opens the deliberately narrow, exact historical-result read capability.
    /// Normal preparation and dispatch never call this hook.
    fn begin_exact_history_recovery(
        &mut self,
        _thread_id: &str,
        _turn_id: &str,
    ) -> Result<(), String> {
        Err("HISTORY_RECOVERY_UNSUPPORTED".into())
    }
    fn settle(&mut self, window: Duration) -> Result<(), String>;
    fn close(&mut self) -> Result<(), String>;
    fn cleanup_candidate(&mut self) -> CandidateCleanup;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactHistoryResult {
    pub item_id: String,
    pub text: String,
}

/// Recovers one already-completed result only after the caller has established
/// the persisted exact thread and turn identities. This is intentionally not a
/// general transcript API: two metadata pages and four item pages are the
/// whole read budget, every item must remain bound to the requested turn, and
/// no write/resume/Goal operation is available through this path.
pub fn recover_exact_completed_result(
    session: &mut dyn NativeSession,
    contract: &NativeContract,
    thread: &str,
    turn: &str,
) -> Result<ExactHistoryResult, String> {
    if thread.is_empty() || thread.len() > 256 || turn.is_empty() || turn.len() > 256 {
        return Err("CAPABILITY_UNVERIFIED".into());
    }
    contract.verify_root()?;
    session.begin_exact_history_recovery(thread, turn)?;
    let identity = session.request(
        "thread/read",
        json!({"threadId":thread,"includeTurns":false}),
    )?;
    contract.verify_identity(&identity, thread)?;

    let mut cursor = Value::Null;
    let mut target_found = false;
    let mut target_cursor = Value::Null;
    for _ in 0..2 {
        let page = session.request("thread/turns/list", json!({
            "threadId":thread,"cursor":cursor,"limit":20,"sortDirection":"asc","itemsView":"notLoaded"
        }))?;
        let rows = page
            .get("data")
            .and_then(Value::as_array)
            .ok_or("CAPABILITY_UNVERIFIED")?;
        if rows.len() > 20 {
            return Err("HISTORY_RECOVERY_LIMIT".into());
        }
        for row in rows {
            if row.get("itemsView").and_then(Value::as_str) != Some("notLoaded")
                || row
                    .get("items")
                    .and_then(Value::as_array)
                    .is_some_and(|v| !v.is_empty())
            {
                return Err("HISTORY_CHANGED".into());
            }
            if row.get("id").and_then(Value::as_str) == Some(turn) {
                if row.get("status").and_then(Value::as_str) != Some("completed") {
                    return Err("RESULT_UNAVAILABLE".into());
                }
                target_found = true;
                target_cursor = cursor.clone();
            }
        }
        if target_found {
            break;
        }
        cursor = page.get("nextCursor").cloned().unwrap_or(Value::Null);
        if cursor.is_null() {
            break;
        }
    }
    if !target_found {
        return Err("HISTORY_RECOVERY_LIMIT".into());
    }

    let mut item_cursor = Value::Null;
    let mut selected: Option<ExactHistoryResult> = None;
    let mut total_bytes = 0usize;
    for _ in 0..4 {
        let page = session.request("thread/items/list", json!({
            "threadId":thread,"turnId":turn,"cursor":item_cursor,"limit":20,"sortDirection":"asc"
        }))?;
        let rows = page
            .get("data")
            .and_then(Value::as_array)
            .ok_or("CAPABILITY_UNVERIFIED")?;
        if rows.len() > 20 {
            return Err("HISTORY_RECOVERY_LIMIT".into());
        }
        for row in rows {
            if row.get("turnId").and_then(Value::as_str) != Some(turn) {
                return Err("HISTORY_CHANGED".into());
            }
            let item = row.get("item").ok_or("CAPABILITY_UNVERIFIED")?;
            if item.get("type").and_then(Value::as_str) == Some("agentMessage")
                && item.get("phase").and_then(Value::as_str) == Some("final_answer")
            {
                let item_id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|v| !v.is_empty() && v.len() <= 256)
                    .ok_or("CAPABILITY_UNVERIFIED")?;
                let text = item
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|v| !v.trim().is_empty())
                    .ok_or("RESULT_UNAVAILABLE")?;
                total_bytes = total_bytes
                    .checked_add(text.len())
                    .ok_or("HISTORY_RECOVERY_LIMIT")?;
                if total_bytes > 1024 * 1024 {
                    return Err("RESULT_TOO_LARGE".into());
                }
                selected = Some(ExactHistoryResult {
                    item_id: item_id.into(),
                    text: text.into(),
                });
            }
        }
        item_cursor = page.get("nextCursor").cloned().unwrap_or(Value::Null);
        if item_cursor.is_null() {
            break;
        }
    }
    if !item_cursor.is_null() {
        return Err("HISTORY_RECOVERY_LIMIT".into());
    }
    // Re-read only the metadata page that established the target. This checks
    // for a concurrent history mutation without extending the two-page window.
    let recheck = session.request("thread/turns/list", json!({
        "threadId":thread,"cursor":target_cursor,"limit":20,"sortDirection":"asc","itemsView":"notLoaded"
    }))?;
    let stable = recheck
        .get("data")
        .and_then(Value::as_array)
        .is_some_and(|rows| {
            rows.iter().any(|row| {
                row.get("id").and_then(Value::as_str) == Some(turn)
                    && row.get("status").and_then(Value::as_str) == Some("completed")
                    && row.get("itemsView").and_then(Value::as_str) == Some("notLoaded")
                    && row
                        .get("items")
                        .and_then(Value::as_array)
                        .is_some_and(|items| items.is_empty())
            })
        });
    if !stable {
        return Err("HISTORY_CHANGED".into());
    }
    selected.ok_or("RESULT_UNAVAILABLE".into())
}
pub trait NativeFactory {
    fn open(&mut self) -> Result<Box<dyn NativeSession>, String>;
}
#[derive(Clone)]
pub struct NativeContract {
    pub canonical_path: String,
    pub root_revision: i64,
    pub policy: PolicySnapshot,
    pub model_provider: String,
}
impl NativeContract {
    pub fn verify_root(&self) -> Result<RootIdentity, String> {
        self.policy.validate()?;
        if self.root_revision < 1
            || self.model_provider.is_empty()
            || self.model_provider.len() > 100
        {
            return Err("CAPABILITY_UNVERIFIED".into());
        }
        RootIdentity::verify(
            Path::new(&self.canonical_path),
            &self.policy.root_identity_hash,
        )
    }
    fn params(&self) -> Value {
        json!({"cwd":self.canonical_path,"model":self.policy.model,"modelProvider":self.model_provider,"approvalPolicy":match self.policy.approval_mode {crate::store::control::ApprovalMode::OnRequest=>"on-request",crate::store::control::ApprovalMode::Never=>"never"},"approvalsReviewer":"user","sandbox":match self.policy.permission_mode {PermissionMode::ReadOnly=>"read-only",PermissionMode::WorkspaceWrite=>"workspace-write"}})
    }
    pub fn verify_identity(&self, response: &Value, thread: &str) -> Result<(), String> {
        let target = response.get("thread").ok_or("CAPABILITY_UNVERIFIED")?;
        if target.get("id").and_then(Value::as_str) != Some(thread)
            || target.get("ephemeral") != Some(&Value::Bool(false))
            || target.get("modelProvider").and_then(Value::as_str)
                != Some(self.model_provider.as_str())
        {
            return Err("TARGET_CHANGED".into());
        }
        if !matches!(
            target.pointer("/status/type").and_then(Value::as_str),
            Some("idle" | "notLoaded")
        ) {
            return Err("EXTERNAL_WRITER_OR_ACTIVITY".into());
        }
        let path = target
            .get("cwd")
            .and_then(Value::as_str)
            .ok_or("CAPABILITY_UNVERIFIED")?;
        let registered = self.verify_root()?;
        let observed = RootIdentity::inspect(Path::new(path))?;
        if observed.canonical != registered.canonical || observed.hash != registered.hash {
            return Err("ROOT_CHANGED".into());
        }
        if target
            .get("turns")
            .and_then(Value::as_array)
            .is_none_or(|turns| !turns.is_empty())
        {
            return Err("NORMAL_HISTORY_RPC_FORBIDDEN".into());
        }
        Ok(())
    }
    pub fn verify_effective_policy(&self, response: &Value) -> Result<(), String> {
        if response.get("model").and_then(Value::as_str) != Some(self.policy.model.as_str()) {
            return Err("EFFECTIVE_MODEL_POLICY_CHANGED".into());
        }
        if response.get("modelProvider").and_then(Value::as_str)
            != Some(self.model_provider.as_str())
        {
            return Err("EFFECTIVE_MODEL_PROVIDER_POLICY_CHANGED".into());
        }
        if response.get("approvalPolicy").and_then(Value::as_str)
            != Some(match self.policy.approval_mode {
                crate::store::control::ApprovalMode::OnRequest => "on-request",
                crate::store::control::ApprovalMode::Never => "never",
            })
        {
            return Err("EFFECTIVE_APPROVAL_POLICY_CHANGED".into());
        }
        if response.get("approvalsReviewer").and_then(Value::as_str) != Some("user") {
            return Err("EFFECTIVE_REVIEWER_POLICY_CHANGED".into());
        }
        let path = response
            .get("cwd")
            .and_then(Value::as_str)
            .ok_or("CAPABILITY_UNVERIFIED")?;
        RootIdentity::verify(Path::new(path), &self.policy.root_identity_hash)?;
        let sandbox = response.get("sandbox").ok_or("CAPABILITY_UNVERIFIED")?;
        if sandbox.get("networkAccess") != Some(&Value::Bool(false)) {
            return Err("EFFECTIVE_NETWORK_POLICY_CHANGED".into());
        }
        match self.policy.permission_mode {
            PermissionMode::ReadOnly => {
                if sandbox.get("type").and_then(Value::as_str) != Some("readOnly") {
                    return Err("EFFECTIVE_SANDBOX_POLICY_CHANGED".into());
                }
            }
            PermissionMode::WorkspaceWrite => {
                if sandbox.get("type").and_then(Value::as_str) != Some("workspaceWrite") {
                    return Err("EFFECTIVE_SANDBOX_POLICY_CHANGED".into());
                }
            }
        }
        Ok(())
    }
    /// The fixed official app-server keeps the effective workspace-write
    /// exclusions and roots in `config/read`; `thread/resume` does not echo
    /// configured roots on this runtime.  Do not treat that omission as an
    /// authorization: verify the native server's effective configuration
    /// before the no-activity window and before any turn can be authorized.
    pub fn verify_effective_workspace_write_config(&self, response: &Value) -> Result<(), String> {
        if self.policy.permission_mode != PermissionMode::WorkspaceWrite {
            return Ok(());
        }
        let sandbox = response
            .pointer("/config/sandbox_workspace_write")
            .ok_or("CAPABILITY_UNVERIFIED")?;
        if sandbox.get("exclude_tmpdir_env_var") != Some(&Value::Bool(true))
            || sandbox.get("exclude_slash_tmp") != Some(&Value::Bool(true))
        {
            return Err("EFFECTIVE_WORKSPACE_EXCLUSIONS_POLICY_CHANGED".into());
        }
        let roots = sandbox
            .get("writable_roots")
            .and_then(Value::as_array)
            .ok_or("EFFECTIVE_WORKSPACE_ROOTS_POLICY_CHANGED")?;
        if roots.len() != 1 {
            return Err("EFFECTIVE_WORKSPACE_ROOTS_POLICY_CHANGED".into());
        }
        RootIdentity::verify(
            Path::new(
                roots[0]
                    .as_str()
                    .ok_or("EFFECTIVE_WORKSPACE_ROOTS_POLICY_CHANGED")?,
            ),
            &self.policy.root_identity_hash,
        )?;
        Ok(())
    }
}
pub fn safe_goal(
    session: &mut dyn NativeSession,
    thread: &str,
    require_absent: bool,
) -> Result<(), String> {
    let goal = session.request("thread/goal/get", json!({"threadId":thread}))?;
    if goal.pointer("/goal/threadId").and_then(Value::as_str) == Some(thread)
        && goal.pointer("/goal/status").and_then(Value::as_str) == Some("active")
    {
        return Err("GOAL_ACTIVE".into());
    }
    verify_existing_goal(&goal, thread)?;
    if require_absent && goal.get("goal") != Some(&Value::Null) {
        return Err("GOAL_NOT_CLEARED".into());
    }
    Ok(())
}
/// No database transaction is held while reading/resuming. Existing Goals are
/// observed only; active or unknown states stop before read/resume.
pub fn prepare_exact(
    session: &mut dyn NativeSession,
    contract: &NativeContract,
    thread: &str,
    require_no_goal: bool,
) -> Result<(), String> {
    contract.verify_root()?;
    // A persisted Desktop thread can be listed/read as `notLoaded`: the
    // current official app-server exposes its Goal only after the exact
    // metadata-only resume has loaded that thread.  The initial read is still
    // bounded and exact; the guarded transport permits its following resume
    // only after it has verified the returned thread identity.  No turn is
    // authorized until the post-load Goal observation succeeds.
    let read = session.request(
        "thread/read",
        json!({"threadId":thread,"includeTurns":false}),
    );
    let read_not_loaded = match read {
        Ok(read) => {
            contract.verify_identity(&read, thread)?;
            false
        }
        // The pinned app-server can retain a newly-created durable thread as
        // not loaded between child processes and reject its first metadata
        // read with this exact, thread-bound response. Resume remains bounded
        // to the same sealed identity and is fully verified below; no broad
        // error fallback, history read, or task write is permitted.
        Err(error) if exact_native_error(&error, "thread not loaded", thread) => true,
        Err(error) => return Err(error),
    };
    let mut params = contract.params();
    params["threadId"] = json!(thread);
    params["excludeTurns"] = json!(true);
    // A resumed user thread may persist dynamic tools from an unrelated
    // interactive session. The Router-owned child has no such dependency;
    // explicitly provide none so resume cannot block while restoring them.
    params["dynamicTools"] = json!([]);
    let resumed = match session.request("thread/resume", params) {
        Ok(resumed) => resumed,
        // Only a read that established this exact not-loaded state followed
        // by a same-id resume rejection proves the recorded candidate was
        // never durably created.  An isolated no-rollout error remains an
        // ordinary fail-closed transport failure.
        Err(error)
            if read_not_loaded
                && exact_native_error(&error, "no rollout found for thread id", thread) =>
        {
            return Err("CANDIDATE_PROVEN_ABSENT".into())
        }
        Err(error) => return Err(error),
    };
    contract.verify_identity(&resumed, thread)?;
    contract.verify_effective_policy(&resumed)?;
    if contract.policy.permission_mode == PermissionMode::WorkspaceWrite {
        let config = session.request("config/read", json!({}))?;
        contract.verify_effective_workspace_write_config(&config)?;
    }
    session.settle(OBSERVATION_WINDOW)?;
    // Do not treat loading as Goal safety. Existing active or malformed Goals
    // still stop here, before the only `turn/start` path can be authorized.
    safe_goal(session, thread, require_no_goal)?;
    Ok(())
}

/// App-server errors are serialized JSON-RPC envelopes by the owned adapter.
/// This recognizes only the fixed official phrase and the sealed exact thread
/// id; it never promotes a generic read failure to candidate absence.
fn exact_native_error(error: &str, phrase: &str, thread: &str) -> bool {
    error.contains(phrase) && error.contains(thread)
}

fn recovery_stop_code(error: &str, thread: &str) -> &'static str {
    if exact_native_error(error, "no rollout found for thread id", thread) {
        "NO_ROLLOUT_WITHOUT_PRECONDITION"
    } else if exact_native_error(error, "thread not loaded", thread) {
        "NOT_LOADED_WITHOUT_RESUME"
    } else if error == "CAPABILITY_UNVERIFIED" {
        "CAPABILITY_UNVERIFIED"
    } else if error == "FRESH_SAFE_GOAL_OBSERVATION_REQUIRED" {
        "METADATA_SEQUENCE_REJECTED"
    } else {
        "OTHER_SAFE_STOP"
    }
}
/// Called only after a real browser creation confirmation. A durable ambiguous
/// attempt is never restarted. Returns proof; caller applies binding atomically.
pub fn create_direct_paused(
    store: &RouterStore,
    grant: &BindingGrant,
    contract: &NativeContract,
    factory: &mut dyn NativeFactory,
) -> Result<VerifiedBindingTarget, String> {
    contract.verify_root()?;
    if grant.review.policy_hash.as_deref() != Some(contract.policy.hash()?.as_str())
        || grant.review.root_revision != Some(contract.root_revision)
    {
        return Err("POLICY_CHANGED".into());
    }
    let (_, start) = store.begin_creation(grant)?;
    if !start {
        return Err("CREATION_ALREADY_ATTEMPTED".into());
    }
    let result = (|| {
        let mut first = factory.open()?;
        let attempt: Result<String, String> = (|| {
            let mut params = contract.params();
            params["ephemeral"] = json!(false);
            let created = first.request("thread/start", params)?;
            let thread = created
                .pointer("/thread/id")
                .and_then(Value::as_str)
                .filter(|v| uuid::Uuid::parse_str(v).is_ok())
                .ok_or("CAPABILITY_UNVERIFIED")?
                .to_owned();
            store.record_created_candidate(grant, &thread)?;
            contract.verify_identity(&created, &thread)?;
            contract.verify_effective_policy(&created)?;
            if contract.policy.permission_mode == PermissionMode::WorkspaceWrite {
                let config = first.request("config/read", json!({}))?;
                contract.verify_effective_workspace_write_config(&config)?;
            }
            safe_goal(first.as_mut(), &thread, true)?;
            let paused=first.request("thread/goal/set",json!({"threadId":thread,"objective":INITIAL_OBJECTIVE,"status":"paused","tokenBudget":null}))?;
            verify_paused(&paused, &thread)?;
            first.settle(OBSERVATION_WINDOW)?;
            let observed = first.request("thread/goal/get", json!({"threadId":thread}))?;
            verify_paused(&observed, &thread)?;
            let cleared = first.request("thread/goal/clear", json!({"threadId":thread}))?;
            if cleared.get("cleared") != Some(&Value::Bool(true)) {
                return Err("GOAL_NOT_CLEARED".into());
            }
            first.settle(OBSERVATION_WINDOW)?;
            safe_goal(first.as_mut(), &thread, true)?;
            first.close()?;
            Ok(thread)
        })();
        let thread = match attempt {
            Ok(t) => t,
            Err(e) => {
                let _cleanup = first.cleanup_candidate();
                let _closed = first.close();
                return Err(e);
            }
        };
        let mut fresh = factory.open()?;
        let verified = prepare_exact(fresh.as_mut(), contract, &thread, true);
        let closed = fresh.close();
        verified?;
        closed?;
        let target = VerifiedBindingTarget {
            thread_id: thread,
            policy_hash: contract.policy.hash()?,
            root_revision: contract.root_revision,
            durable: true,
        };
        store.finish_creation_verification(grant, &target)?;
        Ok(target)
    })();
    if result.is_err() {
        store.mark_creation_unknown(grant)?;
    }
    result
}
/// Verifies and finishes the one candidate left by an interrupted CREATE.
/// This never invokes `thread/start`: the durable candidate id is already
/// sealed in the Review record before recovery begins.
pub fn recover_direct_paused(
    store: &RouterStore,
    grant: &BindingGrant,
    contract: &NativeContract,
    factory: &mut dyn NativeFactory,
) -> Result<VerifiedBindingTarget, String> {
    contract.verify_root()?;
    if grant.review.policy_hash.as_deref() != Some(contract.policy.hash()?.as_str())
        || grant.review.root_revision != Some(contract.root_revision)
    {
        return Err("POLICY_CHANGED".into());
    }
    let recovered = store.begin_creation_recovery(grant)?;
    let thread = recovered.candidate_thread_id.ok_or("CREATION_UNKNOWN")?;
    let result = (|| {
        let mut first = factory.open()?;
        let attempt: Result<(), String> = (|| {
            // First prove that this exact candidate is inert and bound to the
            // sealed root/policy.  Only then perform the required paused Goal
            // set/clear bootstrap on the already-created thread.
            prepare_exact(first.as_mut(), contract, &thread, true)?;
            let paused=first.request("thread/goal/set",json!({"threadId":thread,"objective":INITIAL_OBJECTIVE,"status":"paused","tokenBudget":null}))?;
            verify_paused(&paused, &thread)?;
            first.settle(OBSERVATION_WINDOW)?;
            let observed = first.request("thread/goal/get", json!({"threadId":thread}))?;
            verify_paused(&observed, &thread)?;
            let cleared = first.request("thread/goal/clear", json!({"threadId":thread}))?;
            if cleared.get("cleared") != Some(&Value::Bool(true)) {
                return Err("GOAL_NOT_CLEARED".into());
            }
            first.settle(OBSERVATION_WINDOW)?;
            safe_goal(first.as_mut(), &thread, true)?;
            first.close()?;
            Ok(())
        })();
        if let Err(e) = attempt {
            let _cleanup = first.cleanup_candidate();
            let _closed = first.close();
            if e == "CANDIDATE_PROVEN_ABSENT" {
                store.confirm_creation_not_created(grant, &thread)?;
                return Err("CREATION_PROVEN_NOT_CREATED".into());
            }
            eprintln!(
                "router-core: CANDIDATE_RECOVERY_STOP {}",
                recovery_stop_code(&e, &thread)
            );
            return Err(e);
        }
        let mut fresh = factory.open()?;
        let verified = prepare_exact(fresh.as_mut(), contract, &thread, true);
        let closed = fresh.close();
        verified?;
        closed?;
        let target = VerifiedBindingTarget {
            thread_id: thread,
            policy_hash: contract.policy.hash()?,
            root_revision: contract.root_revision,
            durable: true,
        };
        store.finish_creation_verification(grant, &target)?;
        Ok(target)
    })();
    if result.is_err() {
        store.mark_creation_unknown(grant)?;
    }
    result
}
fn verify_paused(response: &Value, thread: &str) -> Result<(), String> {
    let goal = response.get("goal").ok_or("CAPABILITY_UNVERIFIED")?;
    if goal.get("threadId").and_then(Value::as_str) != Some(thread)
        || goal.get("status").and_then(Value::as_str) != Some("paused")
        || goal.get("objective").and_then(Value::as_str) != Some(INITIAL_OBJECTIVE)
        || goal.get("tokenBudget") != Some(&Value::Null)
    {
        return Err("CAPABILITY_UNVERIFIED".into());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };
    #[derive(Default)]
    pub struct Observation {
        pub calls: Vec<(String, Value)>,
        pub windows: usize,
        pub closes: usize,
        pub cleanups: usize,
    }
    pub struct FixtureSession {
        pub script: VecDeque<(&'static str, Value)>,
        pub trace: Arc<Mutex<Observation>>,
        pub fail_window: Option<usize>,
    }
    impl NativeSession for FixtureSession {
        fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
            self.trace
                .lock()
                .unwrap()
                .calls
                .push((method.into(), params));
            let (expected, response) = self.script.pop_front().ok_or("UNEXPECTED_RPC")?;
            if method != expected {
                return Err("UNEXPECTED_RPC_ORDER".into());
            }
            Ok(response)
        }
        fn begin_exact_history_recovery(
            &mut self,
            _thread_id: &str,
            _turn_id: &str,
        ) -> Result<(), String> {
            Ok(())
        }
        fn settle(&mut self, window: Duration) -> Result<(), String> {
            assert_eq!(window, OBSERVATION_WINDOW);
            let mut t = self.trace.lock().unwrap();
            t.windows += 1;
            if self.fail_window == Some(t.windows) {
                return Err("UNEXPECTED_NATIVE_EXECUTION".into());
            }
            Ok(())
        }
        fn close(&mut self) -> Result<(), String> {
            self.trace.lock().unwrap().closes += 1;
            Ok(())
        }
        fn cleanup_candidate(&mut self) -> CandidateCleanup {
            self.trace.lock().unwrap().cleanups += 1;
            CandidateCleanup::default()
        }
    }
    pub fn response(contract: &NativeContract, thread: &str) -> Value {
        json!({"thread":{"id":thread,"cwd":contract.canonical_path,"ephemeral":false,"modelProvider":contract.model_provider,"status":{"type":"idle"},"turns":[]},"model":contract.policy.model,"modelProvider":contract.model_provider,"cwd":contract.canonical_path,"approvalPolicy":match contract.policy.approval_mode {crate::store::control::ApprovalMode::OnRequest=>"on-request",crate::store::control::ApprovalMode::Never=>"never"},"approvalsReviewer":"user","sandbox":{"type":match contract.policy.permission_mode {PermissionMode::ReadOnly=>"readOnly",PermissionMode::WorkspaceWrite=>"workspaceWrite"},"networkAccess":false}})
    }
    fn workspace_config(contract: &NativeContract) -> Value {
        json!({"config":{"sandbox_workspace_write":{"exclude_tmpdir_env_var":true,"exclude_slash_tmp":true,"writable_roots":[contract.canonical_path]}}})
    }
    fn contract(root: &Path) -> NativeContract {
        NativeContract {
            canonical_path: root.to_str().unwrap().into(),
            root_revision: 1,
            model_provider: "fixture-provider".into(),
            policy: PolicySnapshot {
                version: 1,
                backend_id: uuid::Uuid::new_v4().to_string(),
                native_version: "0.154.0".into(),
                model: "fixture-model".into(),
                permission_mode: PermissionMode::ReadOnly,
                approval_mode: crate::store::control::ApprovalMode::OnRequest,
                network_access: false,
                root_identity_hash: RootIdentity::inspect(root).unwrap().hash,
                config_digest: "c".repeat(64),
                native_launch_digest: None,
            },
        }
    }
    #[test]
    fn exact_preparation_loads_metadata_before_goal_and_excludes_history() {
        let dir = tempfile::tempdir().unwrap();
        let c = contract(dir.path());
        let thread = uuid::Uuid::new_v4().to_string();
        let trace = Arc::new(Mutex::new(Observation::default()));
        let mut fixture = FixtureSession {
            trace: trace.clone(),
            fail_window: None,
            script: VecDeque::from(vec![
                ("thread/read", response(&c, &thread)),
                ("thread/resume", response(&c, &thread)),
                ("thread/goal/get", json!({"goal":null})),
            ]),
        };
        prepare_exact(&mut fixture, &c, &thread, true).unwrap();
        assert!(fixture.script.is_empty());
        let t = trace.lock().unwrap();
        assert_eq!(t.calls.len(), 3);
        assert_eq!(t.calls[0].1["includeTurns"], false);
        assert_eq!(t.calls[1].1["excludeTurns"], true);
        assert_eq!(t.windows, 1);
    }
    #[test]
    fn exact_history_recovery_reads_only_the_selected_completed_turn() {
        let dir = tempfile::tempdir().unwrap();
        let c = contract(dir.path());
        let thread = uuid::Uuid::new_v4().to_string();
        let turn = uuid::Uuid::new_v4().to_string();
        let trace = Arc::new(Mutex::new(Observation::default()));
        let mut fixture = FixtureSession {
            trace: trace.clone(),
            fail_window: None,
            script: VecDeque::from(vec![
                ("thread/read", response(&c, &thread)),
                (
                    "thread/turns/list",
                    json!({"data":[{"id":turn,"status":"completed","itemsView":"notLoaded","items":[]}],"nextCursor":null}),
                ),
                (
                    "thread/items/list",
                    json!({"data":[{"turnId":turn,"item":{"id":"final-item","type":"agentMessage","phase":"final_answer","text":"exact output"}}],"nextCursor":null}),
                ),
                (
                    "thread/turns/list",
                    json!({"data":[{"id":turn,"status":"completed","itemsView":"notLoaded","items":[]}],"nextCursor":null}),
                ),
            ]),
        };
        let recovered = recover_exact_completed_result(&mut fixture, &c, &thread, &turn).unwrap();
        assert_eq!(
            recovered,
            ExactHistoryResult {
                item_id: "final-item".into(),
                text: "exact output".into()
            }
        );
        let calls = &trace.lock().unwrap().calls;
        assert_eq!(
            calls
                .iter()
                .map(|(method, _)| method.as_str())
                .collect::<Vec<_>>(),
            vec![
                "thread/read",
                "thread/turns/list",
                "thread/items/list",
                "thread/turns/list"
            ]
        );
        assert!(calls
            .iter()
            .all(|(method, _)| !matches!(method.as_str(), "thread/resume" | "turn/start")));
        assert!(fixture.script.is_empty());
    }
    #[test]
    fn candidate_absence_requires_the_exact_native_error_and_thread() {
        let thread = "00000000-0000-7000-8000-71c072a2226b";
        assert!(exact_native_error(
            &format!(
                "Codex JSON-RPC error: {{\"message\":\"no rollout found for thread id {thread}\"}}"
            ),
            "no rollout found for thread id",
            thread,
        ));
        assert!(!exact_native_error(
            "Codex JSON-RPC error: {\"message\":\"no rollout found for thread id other\"}",
            "no rollout found for thread id",
            thread,
        ));
        assert!(!exact_native_error(
            &format!("Codex JSON-RPC error: {{\"message\":\"thread not loaded: {thread}\"}}"),
            "no rollout found for thread id",
            thread,
        ));
    }
    #[test]
    fn existing_active_goal_stops_after_exact_metadata_load_but_before_turn() {
        let dir = tempfile::tempdir().unwrap();
        let c = contract(dir.path());
        let thread = uuid::Uuid::new_v4().to_string();
        let trace = Arc::new(Mutex::new(Observation::default()));
        let mut f = FixtureSession {
            trace: trace.clone(),
            fail_window: None,
            script: VecDeque::from(vec![
                ("thread/read", response(&c, &thread)),
                ("thread/resume", response(&c, &thread)),
                (
                    "thread/goal/get",
                    json!({"goal":{"threadId":thread,"status":"active"}}),
                ),
            ]),
        };
        assert!(prepare_exact(&mut f, &c, &thread, false).is_err());
        assert_eq!(trace.lock().unwrap().calls.len(), 3);
        let wrong_target_trace = Arc::new(Mutex::new(Observation::default()));
        let mut wrong_target = FixtureSession {
            trace: wrong_target_trace.clone(),
            fail_window: None,
            script: VecDeque::from(vec![("thread/read", response(&c, "different"))]),
        };
        assert!(prepare_exact(&mut wrong_target, &c, &thread, false).is_err());
        assert!(!wrong_target_trace
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|(m, _)| matches!(
                m.as_str(),
                "thread/resume" | "thread/goal/set" | "thread/goal/clear" | "turn/start"
            )));
    }
    #[test]
    fn effective_policy_drift_or_automatic_activity_stops() {
        let dir = tempfile::tempdir().unwrap();
        let c = contract(dir.path());
        let thread = uuid::Uuid::new_v4().to_string();
        let good = response(&c, &thread);
        for (field, expected) in [
            ("model", "EFFECTIVE_MODEL_POLICY_CHANGED"),
            ("approvalPolicy", "EFFECTIVE_APPROVAL_POLICY_CHANGED"),
            ("approvalsReviewer", "EFFECTIVE_REVIEWER_POLICY_CHANGED"),
        ] {
            let mut bad = good.clone();
            bad[field] = json!("wrong");
            assert_eq!(c.verify_effective_policy(&bad).unwrap_err(), expected);
        }
        let mut bad = good.clone();
        bad["sandbox"]["networkAccess"] = json!(true);
        assert_eq!(
            c.verify_effective_policy(&bad).unwrap_err(),
            "EFFECTIVE_NETWORK_POLICY_CHANGED"
        );
        let trace = Arc::new(Mutex::new(Observation::default()));
        let mut f = FixtureSession {
            trace: trace.clone(),
            fail_window: Some(1),
            script: VecDeque::from(vec![("thread/read", good.clone()), ("thread/resume", good)]),
        };
        assert!(prepare_exact(&mut f, &c, &thread, true).is_err());
        assert_eq!(trace.lock().unwrap().calls.len(), 2);
    }
    #[test]
    fn workspace_write_never_policy_is_requested_and_verified() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = contract(dir.path());
        c.policy.permission_mode = PermissionMode::WorkspaceWrite;
        c.policy.approval_mode = crate::store::control::ApprovalMode::Never;
        assert_eq!(c.params()["approvalPolicy"], "never");
        assert_eq!(c.params()["sandbox"], "workspace-write");
        assert_eq!(c.params().get("dynamicTools"), None);
        let mut response = response(&c, &uuid::Uuid::new_v4().to_string());
        c.verify_effective_policy(&response).unwrap();
        c.verify_effective_workspace_write_config(&workspace_config(&c))
            .unwrap();
        let mut config = workspace_config(&c);
        config["config"]["sandbox_workspace_write"]["exclude_slash_tmp"] = json!(false);
        assert_eq!(
            c.verify_effective_workspace_write_config(&config)
                .unwrap_err(),
            "EFFECTIVE_WORKSPACE_EXCLUSIONS_POLICY_CHANGED"
        );
        let mut config = workspace_config(&c);
        config["config"]["sandbox_workspace_write"]["writable_roots"] = json!([]);
        assert_eq!(
            c.verify_effective_workspace_write_config(&config)
                .unwrap_err(),
            "EFFECTIVE_WORKSPACE_ROOTS_POLICY_CHANGED"
        );
        response["sandbox"]["networkAccess"] = json!(true);
        assert!(c.verify_effective_policy(&response).is_err());
    }
    #[test]
    fn workspace_write_requires_effective_config_before_goal_or_turn() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = contract(dir.path());
        c.policy.permission_mode = PermissionMode::WorkspaceWrite;
        c.policy.approval_mode = crate::store::control::ApprovalMode::Never;
        let thread = uuid::Uuid::new_v4().to_string();
        let trace = Arc::new(Mutex::new(Observation::default()));
        let mut fixture = FixtureSession {
            trace: trace.clone(),
            fail_window: None,
            script: VecDeque::from(vec![
                ("thread/read", response(&c, &thread)),
                ("thread/resume", response(&c, &thread)),
                ("config/read", workspace_config(&c)),
                ("thread/goal/get", json!({"goal":null})),
            ]),
        };
        prepare_exact(&mut fixture, &c, &thread, true).unwrap();
        assert!(fixture.script.is_empty());
        assert_eq!(trace.lock().unwrap().calls.len(), 4);

        let failed_trace = Arc::new(Mutex::new(Observation::default()));
        let mut failed = FixtureSession {
            trace: failed_trace.clone(),
            fail_window: None,
            script: VecDeque::from(vec![
                ("thread/read", response(&c, &thread)),
                ("thread/resume", response(&c, &thread)),
                (
                    "config/read",
                    json!({"config":{"sandbox_workspace_write":{"exclude_tmpdir_env_var":true,"exclude_slash_tmp":true,"writable_roots":[]}}}),
                ),
            ]),
        };
        assert_eq!(
            prepare_exact(&mut failed, &c, &thread, true).unwrap_err(),
            "EFFECTIVE_WORKSPACE_ROOTS_POLICY_CHANGED"
        );
        assert_eq!(failed_trace.lock().unwrap().calls.len(), 3);
    }
}
