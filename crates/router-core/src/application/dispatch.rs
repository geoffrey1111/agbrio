//! Application order: prepare outside TX, durable intent, one native write,
//! causal ACK receipt, exact live terminal. Never requests Provider history.
use super::{
    native::{prepare_exact, NativeContract, NativeSession},
    owned_native::OwnedNative,
};
use crate::{
    codex::{
        adapter::{NativeTurnAck, SealedPrewrite},
        result::LiveResult,
    },
    store::{control::ClaimedRun, RouterStore},
};
use serde_json::{json, Value};
use std::path::Path;

/// Narrow application seam, not an alternate protocol or event bus.
pub trait DispatchIo: NativeSession {
    fn epoch(&self) -> &str;
    fn authorize(&mut self, thread: &str) -> Result<(), String>;
    fn write_turn(
        &mut self,
        params: Value,
        gate: &mut dyn FnMut(u64, &Value) -> Result<(), String>,
    ) -> Result<NativeTurnAck, String>;
    fn next(&mut self) -> Result<Option<(i64, Value)>, String>;
    fn seal_prewrite(&mut self, epoch: &str) -> Result<SealedPrewrite, String>;
}
impl DispatchIo for OwnedNative {
    fn epoch(&self) -> &str {
        self.adapter.epoch()
    }
    fn authorize(&mut self, thread: &str) -> Result<(), String> {
        self.guard.authorize_exact_turn(thread)
    }
    fn write_turn(
        &mut self,
        params: Value,
        gate: &mut dyn FnMut(u64, &Value) -> Result<(), String>,
    ) -> Result<NativeTurnAck, String> {
        self.count_turn_attempt();
        let path = self.config_path.clone();
        let digest = self.config_sha256.clone();
        self.adapter.dispatch_turn(params, &mut |id, params| {
            super::owned_native::verify_config(&path, &digest)?;
            gate(id, params)
        })
    }
    fn next(&mut self) -> Result<Option<(i64, Value)>, String> {
        self.next_event()
    }
    fn seal_prewrite(&mut self, epoch: &str) -> Result<SealedPrewrite, String> {
        self.adapter.seal_prewrite_after_shutdown(epoch)
    }
}
pub struct LiveDispatch {
    dispatch_id: String,
    epoch: String,
    result: LiveResult,
    finished: bool,
}
pub enum DispatchEvent {
    None,
    NativeRequest(Value),
    NativeResolved(Value),
    Terminal,
}

/// Persist only fixed preparation classifications. Native transport details
/// and provider text are never durable Review evidence.
fn safe_preparation_code(error: &str) -> &'static str {
    if error.contains("already has an active writer") {
        return "EXTERNAL_WRITER_OR_ACTIVITY";
    }
    match error {
        "GOAL_ACTIVE" => "GOAL_ACTIVE",
        "GOAL_NOT_CLEARED" => "GOAL_NOT_CLEARED",
        "EXTERNAL_WRITER_OR_ACTIVITY" => "EXTERNAL_WRITER_OR_ACTIVITY",
        "ROOT_CHANGED" => "ROOT_CHANGED",
        "POLICY_CHANGED" => "POLICY_CHANGED",
        "EFFECTIVE_MODEL_POLICY_CHANGED" => "EFFECTIVE_MODEL_POLICY_CHANGED",
        "EFFECTIVE_MODEL_PROVIDER_POLICY_CHANGED" => "EFFECTIVE_MODEL_PROVIDER_POLICY_CHANGED",
        "EFFECTIVE_APPROVAL_POLICY_CHANGED" => "EFFECTIVE_APPROVAL_POLICY_CHANGED",
        "EFFECTIVE_REVIEWER_POLICY_CHANGED" => "EFFECTIVE_REVIEWER_POLICY_CHANGED",
        "EFFECTIVE_NETWORK_POLICY_CHANGED" => "EFFECTIVE_NETWORK_POLICY_CHANGED",
        "EFFECTIVE_SANDBOX_POLICY_CHANGED" => "EFFECTIVE_SANDBOX_POLICY_CHANGED",
        "EFFECTIVE_WORKSPACE_EXCLUSIONS_POLICY_CHANGED" => {
            "EFFECTIVE_WORKSPACE_EXCLUSIONS_POLICY_CHANGED"
        }
        "EFFECTIVE_WORKSPACE_ROOTS_POLICY_CHANGED" => "EFFECTIVE_WORKSPACE_ROOTS_POLICY_CHANGED",
        "TARGET_CHANGED" => "TARGET_CHANGED",
        "CAPABILITY_UNVERIFIED" => "CAPABILITY_UNVERIFIED",
        "NORMAL_HISTORY_RPC_FORBIDDEN" => "NORMAL_HISTORY_RPC_FORBIDDEN",
        "UNEXPECTED_NATIVE_EXECUTION" => "UNEXPECTED_NATIVE_EXECUTION",
        "OBSERVATION_FAILED" => "OBSERVATION_FAILED",
        _ => "PREPARATION_FAILED",
    }
}

fn stop_before_write(
    store: &RouterStore,
    dispatch_id: &str,
    epoch: &str,
    owned: &mut dyn DispatchIo,
    key: &[u8; 32],
    code: &'static str,
) -> Result<(), String> {
    let sealed = owned.seal_prewrite(epoch);
    store.mark_dispatch_unknown(dispatch_id, epoch, code)?;
    if let Ok(proof) = sealed {
        store.record_prewrite_stop(dispatch_id, &proof, key)?;
    }
    Ok(())
}

pub fn start_claimed(
    store: &RouterStore,
    claim: &ClaimedRun,
    owned: &mut dyn DispatchIo,
    model_provider: &str,
    key: &[u8; 32],
) -> Result<LiveDispatch, String> {
    if !claim.newly_claimed {
        return Err("DISPATCH_ALREADY_ATTEMPTED".into());
    }
    let record = store.dispatch_record(&claim.dispatch_id)?;
    if record.run_id != claim.run_id || record.handoff_id != claim.handoff_id {
        return Err("DISPATCH_CHANGED".into());
    }
    let epoch = record.adapter_epoch.clone();
    if owned.epoch() != epoch {
        return Err("STALE_ADAPTER_EPOCH".into());
    }
    let snapshot = store.begin_dispatch_preparation(&claim.dispatch_id, &epoch)?;
    let contract = NativeContract {
        canonical_path: snapshot.canonical_path.clone(),
        root_revision: snapshot.root_revision,
        policy: snapshot.policy.clone(),
        model_provider: model_provider.into(),
    };
    if let Err(error) = prepare_exact(owned, &contract, &snapshot.thread_id, false) {
        let code = safe_preparation_code(&error);
        stop_before_write(store, &claim.dispatch_id, &epoch, owned, key, code)?;
        return Err(code.into());
    }
    owned.authorize(&snapshot.thread_id)?;
    let params = json!({"threadId":snapshot.thread_id,"input":[{"type":"text","text":snapshot.text,"text_elements":[]}]});
    let approved_params = params.clone();
    let ack = owned.write_turn(params, &mut |id, actual| {
        if actual != &approved_params {
            return Err("STALE_APPROVAL".into());
        }
        super::roots::RootIdentity::verify(
            Path::new(&snapshot.canonical_path),
            &snapshot.policy.root_identity_hash,
        )?;
        store.persist_write_intent(&claim.dispatch_id, &epoch, &json!(id), &snapshot)
    });
    let ack = match ack {
        Ok(ack) => ack,
        Err(_) => {
            store.mark_dispatch_unknown(&claim.dispatch_id, &epoch, "NATIVE_ACCEPTANCE_UNKNOWN")?;
            return Err("NATIVE_ACCEPTANCE_UNKNOWN".into());
        }
    };
    if store
        .record_native_ack(&claim.dispatch_id, &epoch, &ack, key)
        .is_err()
    {
        store.mark_dispatch_unknown(&claim.dispatch_id, &epoch, "EVIDENCE_PERSIST_FAILED")?;
        return Err("EVIDENCE_PERSIST_FAILED".into());
    }
    let mut result = LiveResult::new(snapshot.thread_id.clone());
    result.associate_ack(&snapshot.thread_id, ack.turn_id())?;
    if store
        .accept_native_ack(&claim.dispatch_id, &epoch, key)
        .is_err()
    {
        store.mark_dispatch_unknown(&claim.dispatch_id, &epoch, "ACCEPTANCE_PERSIST_FAILED")?;
        return Err("ACCEPTANCE_PERSIST_FAILED".into());
    }
    Ok(LiveDispatch {
        dispatch_id: claim.dispatch_id.clone(),
        epoch,
        result,
        finished: false,
    })
}

/// Endpoint-source Codex handoffs share the existing owned process, exact
/// prewrite, native receipt and terminal accumulator. Core derives the
/// complete already-staged attachment wire value from the sealed snapshot
/// before the one permitted stdin write.
pub fn start_relay_claimed(
    store: &RouterStore,
    snapshot: &crate::store::relay::RelayDispatchSnapshot,
    owned: &mut dyn DispatchIo,
    model_provider: &str,
    key: &[u8; 32],
) -> Result<LiveDispatch, String> {
    let record = store.dispatch_record(&snapshot.dispatch_id)?;
    if record.run_id != snapshot.run_id
        || record.handoff_id != snapshot.handoff_id
        || owned.epoch() != record.adapter_epoch
    {
        return Err("DISPATCH_CHANGED".into());
    }
    let contract = NativeContract {
        canonical_path: snapshot.canonical_path.clone(),
        root_revision: snapshot.root_revision,
        policy: snapshot.policy.clone(),
        model_provider: model_provider.into(),
    };
    let prepared = store.begin_relay_native_preparation(
        &snapshot.owner_principal_key,
        &snapshot.handoff_id,
        &snapshot.run_id,
        &snapshot.dispatch_id,
        &record.adapter_epoch,
    )?;
    let preparation = if prepared.payload_hash != snapshot.payload_hash {
        Some("PAYLOAD_MISMATCH")
    } else if prepared.manifest_hash != snapshot.manifest_hash {
        Some("MANIFEST_MISMATCH")
    } else {
        match prepare_exact(owned, &contract, &snapshot.destination_external_id, false) {
            Ok(()) => None,
            Err(error) => {
                // This is emitted only by the Router-owned process diagnostic
                // stream. Review still receives the bounded classification
                // below, never raw native transport content.
                eprintln!("router-core: RELAY_PREPARATION_RAW_ERROR {error}");
                Some(safe_preparation_code(&error))
            }
        }
    };
    if let Some(code) = preparation {
        stop_before_write(
            store,
            &snapshot.dispatch_id,
            &record.adapter_epoch,
            owned,
            key,
            code,
        )?;
        return Err(code.into());
    }
    owned.authorize(&snapshot.destination_external_id)?;
    let input = match relay_native_input(snapshot) {
        Ok(input) => input,
        Err(_) => {
            let sealed = owned.seal_prewrite(&record.adapter_epoch);
            store.mark_dispatch_unknown(
                &snapshot.dispatch_id,
                &record.adapter_epoch,
                "RELAY_INPUT_INVALID",
            )?;
            if let Ok(proof) = sealed {
                let _ = store.record_prewrite_stop(&snapshot.dispatch_id, &proof, key);
            }
            return Err("RELAY_INPUT_INVALID".into());
        }
    };
    if input.is_empty() {
        store.mark_dispatch_unknown(
            &snapshot.dispatch_id,
            &record.adapter_epoch,
            "RELAY_INPUT_INVALID",
        )?;
        return Err("RELAY_INPUT_INVALID".into());
    }
    let params = json!({"threadId":snapshot.destination_external_id,"input":input});
    let approved = params.clone();
    let ack = owned.write_turn(params, &mut |id, actual| {
        if actual != &approved {
            return Err("STALE_APPROVAL".into());
        }
        super::roots::RootIdentity::verify(
            Path::new(&snapshot.canonical_path),
            &snapshot.policy.root_identity_hash,
        )?;
        store.persist_relay_native_write_intent(
            &snapshot.owner_principal_key,
            &snapshot.handoff_id,
            &snapshot.run_id,
            &snapshot.dispatch_id,
            &record.adapter_epoch,
            &json!(id),
        )
    });
    let ack = match ack {
        Ok(ack) => ack,
        Err(_) => {
            store.mark_dispatch_unknown(
                &snapshot.dispatch_id,
                &record.adapter_epoch,
                "NATIVE_ACCEPTANCE_UNKNOWN",
            )?;
            return Err("NATIVE_ACCEPTANCE_UNKNOWN".into());
        }
    };
    if store
        .record_native_ack(&snapshot.dispatch_id, &record.adapter_epoch, &ack, key)
        .is_err()
    {
        store.mark_dispatch_unknown(
            &snapshot.dispatch_id,
            &record.adapter_epoch,
            "EVIDENCE_PERSIST_FAILED",
        )?;
        return Err("EVIDENCE_PERSIST_FAILED".into());
    }
    let mut result = LiveResult::new(snapshot.destination_external_id.clone());
    result.associate_ack(&snapshot.destination_external_id, ack.turn_id())?;
    if store
        .accept_native_ack(&snapshot.dispatch_id, &record.adapter_epoch, key)
        .is_err()
    {
        store.mark_dispatch_unknown(
            &snapshot.dispatch_id,
            &record.adapter_epoch,
            "ACCEPTANCE_PERSIST_FAILED",
        )?;
        return Err("ACCEPTANCE_PERSIST_FAILED".into());
    }
    Ok(LiveDispatch {
        dispatch_id: snapshot.dispatch_id.clone(),
        epoch: record.adapter_epoch,
        result,
        finished: false,
    })
}

/// Derives the entire official app-server v2 input from the sealed relay
/// snapshot.  Callers cannot replace, omit, or add an attachment after the
/// review seal: documents are represented only by the fixed manifest in the
/// approved text, while each approved PNG/JPEG becomes one localImage item.
pub fn relay_native_input(
    snapshot: &crate::store::relay::RelayDispatchSnapshot,
) -> Result<Vec<Value>, String> {
    if snapshot.direction != "CHATGPT_TO_CODEX" || snapshot.destination_provider != "CODEX" {
        return Err("RELAY_DESTINATION_INVALID".into());
    }
    let root = Path::new(&snapshot.canonical_path);
    let mut input = vec![json!({"type":"text","text":snapshot.approved_text,"text_elements":[]})];
    for (artifact, target) in &snapshot.attachments {
        if artifact.kind == "DOCUMENT" {
            continue;
        }
        if artifact.kind != "IMAGE"
            || !matches!(artifact.mime.as_str(), "image/png" | "image/jpeg")
            || artifact.actual_sha256.as_deref() != Some(artifact.transfer_declared_sha256.as_str())
        {
            return Err("RELAY_INPUT_INVALID".into());
        }
        let relative = Path::new(target);
        if relative.is_absolute()
            || target.contains(':')
            || relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err("RELAY_INPUT_INVALID".into());
        }
        let path = root.join(relative);
        input.push(json!({"type":"localImage","path":path.to_string_lossy()}));
    }
    Ok(input)
}
impl LiveDispatch {
    /// Caller routes native requests to its separate browser-only request gate.
    /// This method never chooses an approval response or interprets model text.
    pub fn poll(
        &mut self,
        store: &RouterStore,
        owned: &mut dyn DispatchIo,
        key: &[u8; 32],
    ) -> Result<DispatchEvent, String> {
        if self.finished {
            return Ok(DispatchEvent::Terminal);
        }
        let next = match owned.next() {
            Ok(next) => next,
            Err(_) => {
                self.result.mark_discontinuity();
                store.mark_dispatch_unknown(
                    &self.dispatch_id,
                    &self.epoch,
                    "OBSERVATION_FAILED",
                )?;
                return Err("OBSERVATION_FAILED".into());
            }
        };
        let Some((sequence, event)) = next else {
            return Ok(DispatchEvent::None);
        };
        if event.get("id").is_some() && event.get("method").is_some() {
            return Ok(DispatchEvent::NativeRequest(event));
        }
        if event.get("method").and_then(Value::as_str) == Some("serverRequest/resolved") {
            return Ok(DispatchEvent::NativeResolved(event));
        }
        if self.result.observe(&event).is_err() {
            store.mark_dispatch_unknown(&self.dispatch_id, &self.epoch, "RESULT_EVENT_CONFLICT")?;
            return Err("RESULT_EVENT_CONFLICT".into());
        }
        if let Some(terminal) = self.result.terminal() {
            if store
                .persist_live_terminal(&self.dispatch_id, &self.epoch, &terminal, sequence, key)
                .is_err()
            {
                store.mark_dispatch_unknown(
                    &self.dispatch_id,
                    &self.epoch,
                    "RESULT_PERSIST_FAILED",
                )?;
                return Err("RESULT_PERSIST_FAILED".into());
            }
            self.finished = true;
            return Ok(DispatchEvent::Terminal);
        }
        Ok(DispatchEvent::None)
    }
}

#[cfg(test)]
mod tests {
    use super::safe_preparation_code;

    #[test]
    fn active_external_writer_is_a_bounded_prewrite_classification() {
        assert_eq!(
            safe_preparation_code(
                "Codex JSON-RPC error: {\"code\":-32600,\"message\":\"thread exact already has an active writer\"}",
            ),
            "EXTERNAL_WRITER_OR_ACTIVITY"
        );
    }
}
