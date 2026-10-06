use crate::codex::adapter::CodexAdapter;
use crate::persistence::{ReconciliationCandidate, RouterStore};
#[cfg(test)]
use serde_json::{json, Value};

/// Application-layer bounded recovery. Adapters provide exact-ID evidence only;
/// this component classifies it and persists one ProviderRun at a time.
pub struct RuntimeReconciler;
impl RuntimeReconciler {
    /// ChatGPT recovery never resurrects a retired provider transport. A
    /// persisted historical request remains a durable unknown unless the
    /// direct carrier itself has already retained its exact terminal result.
    pub fn reconcile_chatgpt(store: &RouterStore) -> Result<(), String> {
        for candidate in reconciliation_candidates_for(store, "CHATGPT")? {
            let result = Self::chatgpt(store, &candidate);
            let (status, code) = result.unwrap_or(("UNKNOWN", "EXACT_EVIDENCE_UNAVAILABLE"));
            let _ = store.reconcile_provider_run(&candidate.run.id, status, code);
        }
        Ok(())
    }

    /// Codex recovery is invoked only after an app-server is initialized. A
    /// superseded endpoint is retained for audit but must never delay the
    /// resident adapter becoming available for the current exact bindings.
    pub fn reconcile_codex(store: &RouterStore, codex: &mut CodexAdapter) -> Result<(), String> {
        Self::reconcile_codex_scope(store, codex, None)
    }
    pub fn reconcile_codex_workstream(store:&RouterStore,codex:&mut CodexAdapter,workstream:&str)->Result<(),String>{
        Self::reconcile_codex_scope(store,codex,Some(workstream))
    }
    fn reconcile_codex_scope(store:&RouterStore,codex:&mut CodexAdapter,workstream:Option<&str>)->Result<(),String>{
        for candidate in active_codex_reconciliation_candidates(store)?.into_iter().filter(|c|workstream.is_none_or(|w|c.run.workstream_id==w)) {
            let result = Self::codex(store, codex, &candidate);
            match result {
                Ok(("COMPLETED", _, Some((identity, text)))) => {
                    let external = candidate.run.external_run_id.as_deref().unwrap_or_default();
                    let _ = store.accept_completed_provider_result(
                        &candidate.run.id,
                        external,
                        &identity,
                        text,
                    );
                }
                Ok((status, code, _)) => {
                    let _ = store.reconcile_provider_run(&candidate.run.id, status, code);
                }
                Err(_) => {
                    let _ = store.reconcile_provider_run(
                        &candidate.run.id,
                        "UNKNOWN",
                        "EXACT_EVIDENCE_UNAVAILABLE",
                    );
                }
            }
        }
        Ok(())
    }

    /// Startup could not obtain a Codex adapter, so no exact turn evidence is
    /// available. Only stale candidates for that provider become UNKNOWN; this
    /// never resumes or retries a turn.
    pub fn provider_unavailable(
        store: &RouterStore,
        provider: &str,
        code: &str,
    ) -> Result<(), String> {
        for candidate in reconciliation_candidates_for(store, provider)? {
            let _ = store.reconcile_provider_run(&candidate.run.id, "UNKNOWN", code);
        }
        Ok(())
    }
    fn codex(
        _store: &RouterStore,
        codex: &mut CodexAdapter,
        candidate: &ReconciliationCandidate,
    ) -> Result<(&'static str, &'static str, Option<(String, String)>), String> {
        let thread_id = candidate.endpoint.external_id.as_str();
        if thread_id.is_empty() {
            return Ok(("UNKNOWN", "MISSING_THREAD_ID", None));
        }
        let Some(turn_id)=candidate.run.external_run_id.as_deref().filter(|id|!id.is_empty())else{return Ok(("UNKNOWN","MISSING_TURN_ID",None))};
        let status=codex.read_exact_turn_status(thread_id,turn_id)?;
        let (state,code)=classify_codex_status(status.as_deref());
        Ok((state,code,None))
    }
    fn chatgpt(
        _store: &RouterStore,
        candidate: &ReconciliationCandidate,
    ) -> Result<(&'static str, &'static str), String> {
        let Some(request_id) = candidate
            .run
            .external_run_id
            .as_deref()
            .filter(|id| !id.is_empty())
        else {
            return Ok(("UNKNOWN", "MISSING_REQUEST_ID"));
        };
        let _ = request_id;
        let code = "HISTORICAL_CHATGPT_TRANSPORT_RETIRED";
        Ok(("UNKNOWN", code))
    }
}

fn reconciliation_candidates_for(
    store: &RouterStore,
    provider: &str,
) -> Result<Vec<ReconciliationCandidate>, String> {
    Ok(store
        .reconciliation_candidates()?
        .into_iter()
        .filter(|candidate| candidate.run.provider == provider)
        .collect())
}

/// A historical ProviderRun remains attached to its historical Endpoint so it
/// can be audited.  Once that Endpoint has been superseded, however, it is no
/// longer a current external authority and cannot hold up Host startup or the
/// passive observer for a different, active binding.
fn active_codex_reconciliation_candidates(
    store: &RouterStore,
) -> Result<Vec<ReconciliationCandidate>, String> {
    Ok(reconciliation_candidates_for(store, "CODEX")?
        .into_iter()
        .filter(|candidate| candidate.endpoint.status == "ACTIVE")
        .collect())
}

fn classify_codex_status(status:Option<&str>) -> (&'static str, &'static str) {
    match status {
        Some("completed") => ("COMPLETED", "CODEX_TURN_COMPLETED"),
        Some("failed") => ("FAILED", "CODEX_TURN_FAILED"),
        Some("interrupted") => ("CANCELLED", "CODEX_TURN_INTERRUPTED"),
        Some("inProgress") => ("RUNNING", "CODEX_TURN_ACTIVE"),
        _ => ("UNKNOWN", "CODEX_TURN_NOT_PROVEN_TERMINAL"),
    }
}
#[cfg(test)]
fn classify_codex_turn(turn: &Value) -> (&'static str, &'static str) {classify_codex_status(turn.get("status").and_then(Value::as_str))}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_recovers_exact_prior_turn_without_fetching_items_or_resending() {
        use std::{process::Command,sync::Arc};
        let directory=tempfile::tempdir().unwrap();
        let store=RouterStore::open_at(directory.path().join("router.db")).unwrap();
        let project=store.create_project("recovery".into(),None).unwrap();
        let workstream=store.create_workstream(&project.id,"recovery".into()).unwrap();
        let endpoint=store.bind_endpoint(&workstream.id,crate::persistence::Provider::Codex,"thread-a".into(),"source".into(),false).unwrap();
        let old=store.create_provider_run(&workstream.id,&endpoint.id,"CODEX",None,Some("old-turn"),"RUNNING").unwrap();
        store.reconcile_provider_run(&old.id,"UNKNOWN","EXACT_EVIDENCE_UNAVAILABLE").unwrap();
        // The newest turn is unrelated and still active. Only the exact old
        // acknowledgement on the second metadata page may clear the old run.
        let script=r#"const rl=require('node:readline').createInterface({input:process.stdin});rl.on('line',l=>{const r=JSON.parse(l);let result;if(r.method==='thread/read')result={thread:{id:'thread-a'}};else if(r.method==='thread/turns/list'&&r.params.itemsView==='notLoaded'&&r.params.limit===4)result=r.params.cursor?{data:[{id:'old-turn',status:'completed'}],nextCursor:null}:{data:[{id:'new-turn',status:'inProgress'}],nextCursor:'older'};else {process.stdout.write(JSON.stringify({id:r.id,error:{code:-1,message:'Unexpected mutation or items read'}})+'\n');return;}process.stdout.write(JSON.stringify({id:r.id,result})+'\n');});"#;
        let mut command=Command::new("node");command.args(["-e",script]);
        let mut adapter=CodexAdapter::start_shared_validation(command,Arc::new(|_|{})).unwrap();
        RuntimeReconciler::reconcile_codex_workstream(&store,&mut adapter,&workstream.id).unwrap();
        let runs=store.provider_runs_for_workstream(&workstream.id).unwrap();
        assert_eq!(runs[0].status,"COMPLETED");
        assert_eq!(runs[0].external_run_id.as_deref(),Some("old-turn"));
        assert_eq!(runs[0].result_text,None);
        assert_eq!(adapter.read_exact_turn_status("thread-a","missing-turn").unwrap(),None);
        assert!(adapter.read_exact_turn_status("other-thread","old-turn").is_err());
        assert_eq!(classify_codex_status(Some("inProgress")).0,"RUNNING");
        assert_eq!(classify_codex_status(None).0,"UNKNOWN");
        assert_eq!(classify_codex_status(Some("interrupted")).0,"CANCELLED");
        adapter.shutdown();
    }

    #[test]
    fn historical_chatgpt_transport_remains_unknown_and_preserves_exact_run_identity() {
        let directory = tempfile::tempdir().unwrap();
        let store = RouterStore::open_at(directory.path().join("router.db")).unwrap();
        let project = store.create_project("Session loss".into(), None).unwrap();
        let workstream = store.create_workstream(&project.id, "WS".into()).unwrap();
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                crate::persistence::Provider::Chatgpt,
                "exact-conversation".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let run = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("historical-chatgpt-request"),
                "RUNNING",
            )
            .unwrap();
        let candidates = reconciliation_candidates_for(&store, "CHATGPT").unwrap();
        assert_eq!(candidates.len(), 1);
        // No legacy fixture, credentials, config changes or network server:
        // a historical request must fail closed before adapter construction.
        assert_eq!(
            RuntimeReconciler::chatgpt(&store, &candidates[0]).unwrap(),
            ("UNKNOWN", "HISTORICAL_CHATGPT_TRANSPORT_RETIRED")
        );
        RuntimeReconciler::reconcile_chatgpt(&store).unwrap();
        let runs = store.provider_runs_for_workstream(&workstream.id).unwrap();
        let reconciled = runs
            .iter()
            .find(|candidate| candidate.id == run.id)
            .unwrap();
        assert_eq!(reconciled.status, "UNKNOWN");
        assert_eq!(reconciled.endpoint_id, endpoint.id);
        assert_eq!(
            reconciled.external_run_id.as_deref(),
            Some("historical-chatgpt-request")
        );
        assert_eq!(reconciled.terminal_at, None);
    }

    #[test]
    fn historical_chatgpt_request_is_unknown_without_constructing_a_legacy_adapter() {
        let directory = tempfile::tempdir().unwrap();
        let store = RouterStore::open_at(directory.path().join("router.db")).unwrap();
        let project = store
            .create_project("Retired ChatGPT transport".into(), None)
            .unwrap();
        let workstream = store.create_workstream(&project.id, "WS".into()).unwrap();
        let endpoint = store
            .bind_endpoint(
                &workstream.id,
                crate::persistence::Provider::Chatgpt,
                "exact-conversation".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let run = store
            .create_provider_run(
                &workstream.id,
                &endpoint.id,
                "CHATGPT",
                None,
                Some("historical-chatgpt-request"),
                "RUNNING",
            )
            .unwrap();

        let candidate = reconciliation_candidates_for(&store, "CHATGPT")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(
            RuntimeReconciler::chatgpt(&store, &candidate).unwrap(),
            ("UNKNOWN", "HISTORICAL_CHATGPT_TRANSPORT_RETIRED")
        );
        RuntimeReconciler::reconcile_chatgpt(&store).unwrap();
        let reconciled = store
            .provider_runs_for_workstream(&workstream.id)
            .unwrap()
            .into_iter()
            .find(|item| item.id == run.id)
            .unwrap();
        assert_eq!(reconciled.status, "UNKNOWN");
        assert_eq!(
            reconciled.external_run_id.as_deref(),
            Some("historical-chatgpt-request")
        );
        assert_eq!(reconciled.terminal_at, None);
    }

    #[test]
    fn codex_exact_turn_status_preserves_active_and_proven_terminal_values() {
        assert_eq!(
            classify_codex_turn(&json!({"id":"exact","status":"completed"})).0,
            "COMPLETED"
        );
        assert_eq!(
            classify_codex_turn(&json!({"id":"exact","status":"failed"})).0,
            "FAILED"
        );
        assert_eq!(
            classify_codex_turn(&json!({"id":"exact","status":"interrupted"})).0,
            "CANCELLED"
        );
        assert_eq!(
            classify_codex_turn(&json!({"id":"exact","status":"inProgress"})).0,
            "RUNNING"
        );
    }
    #[test]
    fn reconciliation_candidates_are_provider_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let store = RouterStore::open_at(directory.path().join("router.db")).unwrap();
        let project = store.create_project("Reconcile".into(), None).unwrap();
        let workstream = store.create_workstream(&project.id, "WS".into()).unwrap();
        let codex = store
            .bind_endpoint(
                &workstream.id,
                crate::persistence::Provider::Codex,
                "thread".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        let chatgpt = store
            .bind_endpoint(
                &workstream.id,
                crate::persistence::Provider::Chatgpt,
                "conversation".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        store
            .create_provider_run(
                &workstream.id,
                &codex.id,
                "CODEX",
                None,
                Some("turn"),
                "RUNNING",
            )
            .unwrap();
        store
            .create_provider_run(
                &workstream.id,
                &chatgpt.id,
                "CHATGPT",
                None,
                Some("request"),
                "RUNNING",
            )
            .unwrap();
        assert_eq!(
            reconciliation_candidates_for(&store, "CODEX")
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            reconciliation_candidates_for(&store, "CHATGPT")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn superseded_codex_recovery_cannot_block_active_observer_startup() {
        let directory = tempfile::tempdir().unwrap();
        let store = RouterStore::open_at(directory.path().join("router.db")).unwrap();
        let project = store.create_project("Reconcile current only".into(), None).unwrap();
        let workstream = store.create_workstream(&project.id, "WS".into()).unwrap();
        let historical = store
            .bind_endpoint(
                &workstream.id,
                crate::persistence::Provider::Codex,
                "thread-historical".into(),
                "Historical Codex".into(),
                false,
            )
            .unwrap();
        let historical_run = store
            .create_provider_run(
                &workstream.id,
                &historical.id,
                "CODEX",
                None,
                Some("turn-historical"),
                "RUNNING",
            )
            .unwrap();
        let current = store
            .bind_endpoint(
                &workstream.id,
                crate::persistence::Provider::Codex,
                "thread-current".into(),
                "Current Codex".into(),
                true,
            )
            .unwrap();
        let current_run = store
            .create_provider_run(
                &workstream.id,
                &current.id,
                "CODEX",
                None,
                Some("turn-current"),
                "RUNNING",
            )
            .unwrap();

        let candidates = active_codex_reconciliation_candidates(&store).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].run.id, current_run.id);
        assert_ne!(candidates[0].run.id, historical_run.id);
        assert_eq!(candidates[0].endpoint.id, current.id);
        assert_eq!(candidates[0].endpoint.status, "ACTIVE");
        let retained = store
            .provider_runs_for_workstream(&workstream.id)
            .unwrap()
            .into_iter()
            .find(|run| run.id == historical_run.id)
            .unwrap();
        assert_eq!(retained.status, "RUNNING");
    }

    #[test]
    fn unavailable_codex_marks_only_its_stale_candidates_unknown() {
        let directory = tempfile::tempdir().unwrap();
        let store = RouterStore::open_at(directory.path().join("router.db")).unwrap();
        let project = store.create_project("Unavailable".into(), None).unwrap();
        let workstream = store.create_workstream(&project.id, "WS".into()).unwrap();
        let codex = store
            .bind_endpoint(
                &workstream.id,
                crate::persistence::Provider::Codex,
                "thread".into(),
                "Codex".into(),
                false,
            )
            .unwrap();
        let chatgpt = store
            .bind_endpoint(
                &workstream.id,
                crate::persistence::Provider::Chatgpt,
                "conversation".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let codex_run = store
            .create_provider_run(
                &workstream.id,
                &codex.id,
                "CODEX",
                None,
                Some("turn"),
                "RUNNING",
            )
            .unwrap();
        let chatgpt_run = store
            .create_provider_run(
                &workstream.id,
                &chatgpt.id,
                "CHATGPT",
                None,
                Some("request"),
                "RUNNING",
            )
            .unwrap();
        RuntimeReconciler::provider_unavailable(&store, "CODEX", "CODEX_ADAPTER_UNAVAILABLE")
            .unwrap();
        let projection = store.dashboard_projection().unwrap();
        let card = projection
            .workstreams
            .iter()
            .find(|card| card.workstream.id == workstream.id)
            .unwrap();
        assert_eq!(card.codex_run.as_ref().unwrap().id, codex_run.id);
        assert_eq!(card.codex_run.as_ref().unwrap().status, "UNKNOWN");
        assert_eq!(card.codex_run.as_ref().unwrap().terminal_at, None);
        assert_eq!(card.chatgpt_run.as_ref().unwrap().id, chatgpt_run.id);
        assert_eq!(card.chatgpt_run.as_ref().unwrap().status, "RUNNING");
        assert!(projection
            .attention_items
            .iter()
            .any(|item| item.kind == "UNKNOWN_RUN" && item.source_id == codex_run.id));
    }
}
