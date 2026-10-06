//! Exact, owner-approved ChatGPT writer composition. The normal Chrome
//! extension proves one accepted user turn; later terminal content returns by
//! the independent extension observation path and is never fabricated here.
use super::*;
const CHATGPT_ACCEPTED_PENDING_TERMINAL: &str = "CHATGPT_ACCEPTED_PENDING_TERMINAL";
pub fn send_text(
    state: &RouterCore,
    workstream_id: &str,
    endpoint: &Endpoint,
    run_id: &str,
    handoff_id: Option<&str>,
    text: &str,
) -> Result<CompletedChatGptResponse, String> {
    let result = send_text_inner(state, workstream_id, endpoint, run_id, handoff_id, text);
    if let Err(error) = &result {
        let _ = record_unclassified_failure(&state.store, workstream_id, run_id, handoff_id, error);
    }
    result
}

pub fn send_handoff(
    state: &RouterCore,
    workstream_id: &str,
    endpoint: &Endpoint,
    run_id: &str,
    handoff_id: &str,
    text: &str,
    attachments: &[chatgpt::model::HandoffAttachment],
) -> Result<CompletedChatGptResponse, String> {
    let result = send_handoff_inner(
        state,
        workstream_id,
        endpoint,
        run_id,
        handoff_id,
        text,
        attachments,
    );
    if let Err(error) = &result {
        let _ = record_unclassified_failure(
            &state.store,
            workstream_id,
            run_id,
            Some(handoff_id),
            error,
        );
    }
    result
}

fn send_text_inner(
    state: &RouterCore,
    workstream_id: &str,
    endpoint: &Endpoint,
    run_id: &str,
    handoff_id: Option<&str>,
    text: &str,
) -> Result<CompletedChatGptResponse, String> {
    let current = state
        .store
        .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
        .ok_or("No ACTIVE ChatGPT Endpoint")?;
    if current.id != endpoint.id || current.external_id != endpoint.external_id {
        return Err("ChatGPT ACTIVE Endpoint changed before dispatch".into());
    }
    let accepted = submit_exact(
        state,
        &endpoint.external_id,
        run_id,
        text,
        serde_json::json!([]),
    )?;
    let _ = record_acceptance(&state.store, run_id, handoff_id, &accepted)?;
    // The exact provider acceptance is durable. Returning a synthetic terminal
    // response here would make a sent turn look complete, so callers receive
    // an explicit pending state and the extension observer supplies the later
    // terminal reply / mobile notification.
    Err(CHATGPT_ACCEPTED_PENDING_TERMINAL.into())
}

fn send_handoff_inner(
    state: &RouterCore,
    workstream_id: &str,
    endpoint: &Endpoint,
    run_id: &str,
    handoff_id: &str,
    text: &str,
    attachments: &[chatgpt::model::HandoffAttachment],
) -> Result<CompletedChatGptResponse, String> {
    let current = state
        .store
        .active_endpoint_for_workstream(workstream_id, Provider::Chatgpt)?
        .ok_or("No ACTIVE ChatGPT Endpoint")?;
    if current.id != endpoint.id || current.external_id != endpoint.external_id {
        return Err("ChatGPT ACTIVE Endpoint changed before dispatch".into());
    }
    let evidence = chatgpt::direct::validate_attachments(attachments).map_err(|error| {
        let _ = state.store.transition_handoff(
            handoff_id,
            "FAILED",
            Some((
                "CHATGPT_ATTACHMENT_PRE_DISPATCH_ERROR".into(),
                error.clone(),
            )),
        );
        let _ = state
            .store
            .fail_provider_run_by_id(run_id, "CHATGPT_ATTACHMENT_PRE_DISPATCH_ERROR");
        error
    })?;
    state.store.record_attachment_send_evidence(
        handoff_id,
        &evidence
            .iter()
            .map(|item| persistence::AttachmentSendEvidence {
                attachment_id: item.attachment_id.clone(),
                review_sha256: item.review_sha256.clone(),
                integrity_status: item.integrity_status.clone(),
                send_sha256: item.send_sha256.clone(),
            })
            .collect::<Vec<_>>(),
    )?;
    let files = attachments.iter().zip(&evidence).map(|(attachment, verified)|
        serde_json::json!({"path": attachment.path, "sha256": verified.send_sha256})).collect::<Vec<_>>();
    let accepted = submit_exact(
        state,
        &endpoint.external_id,
        run_id,
        text,
        serde_json::json!(files),
    )?;
    let _ = record_acceptance(&state.store, run_id, Some(handoff_id), &accepted)?;
    Err(CHATGPT_ACCEPTED_PENDING_TERMINAL.into())
}

fn submit_exact(
    state: &RouterCore,
    conversation_id: &str,
    dispatch_id: &str,
    text: &str,
    attachments: serde_json::Value,
) -> Result<String, String> {
    match state.chatgpt.submit_approved_with_attachments(
        conversation_id,
        dispatch_id,
        text,
        attachments,
    )? {
        router_core::chatgpt::Receipt::Sent {
            accepted_message_id,
            ..
        } => Ok(accepted_message_id),
        router_core::chatgpt::Receipt::Unknown { .. } => Err("CHATGPT_ACCEPTANCE_UNPROVEN".into()),
        router_core::chatgpt::Receipt::NotFound => Err("CHATGPT_ACCEPTANCE_UNPROVEN".into()),
    }
}

// These persistence steps are shared by direct discussion, revision feedback
// (no Handoff), and approved attachment-free Handoffs. None dispatches a provider
// request or falls back after ambiguous native evidence.
fn record_acceptance(
    store: &RouterStore,
    run_id: &str,
    handoff_id: Option<&str>,
    accepted_user_turn_identity: &str,
) -> Result<String, String> {
    if accepted_user_turn_identity.trim().is_empty() {
        return Err("ChatGPT accepted user-turn identity is missing".into());
    }
    let observation_id = accepted_user_turn_identity.to_owned();
    store.attach_provider_run_external_identity(run_id, "CHATGPT", &observation_id)?;
    if let Some(id) = handoff_id {
        store.transition_handoff(id, "SENT", None)?;
    }
    Ok(observation_id)
}

fn record_acceptance_unproven(
    store: &RouterStore,
    run_id: &str,
    handoff_id: Option<&str>,
) -> Result<(), String> {
    if let Some(id) = handoff_id {
        store.mark_chatgpt_handoff_acceptance_unproven_if_unattached(id, run_id)?;
    } else {
        store.mark_chatgpt_acceptance_unproven_if_unattached(run_id)?;
    }
    Ok(())
}

fn record_unclassified_failure(
    store: &RouterStore,
    workstream_id: &str,
    run_id: &str,
    handoff_id: Option<&str>,
    error: &str,
) -> Result<(), String> {
    if error == CHATGPT_ACCEPTED_PENDING_TERMINAL {
        return Ok(());
    }
    let run = store
        .provider_runs_for_workstream(workstream_id)?
        .into_iter()
        .find(|run| run.id == run_id)
        .ok_or("Native ProviderRun missing")?;
    // Preserve specific acceptance/terminal uncertainty and durable completion.
    // Previously the outer catch replaced UNKNOWN's useful terminal code.
    if !matches!(run.status.as_str(), "STARTING" | "RUNNING") {
        return Ok(());
    }
    let code = classified_chatgpt_carrier_failure(error);
    // These sidecar failures occur before `fillComposer` or
    // `submitComposer`: no provider text has been entered and there can be no
    // accepted user-turn identity. Keep that materially different from an
    // ambiguous post-submit failure, so the owner may safely choose a later
    // manual dispatch instead of being trapped behind a misleading SENDING
    // state.
    if is_proven_prewrite_failure(&code) {
        store.reconcile_provider_run(run_id, "FAILED", &code)?;
        if let Some(id) = handoff_id {
            store.transition_handoff(
                id,
                "FAILED",
                Some((code, "ChatGPT rejected the carrier before text entry; no provider submission occurred".into())),
            )?;
        }
        return Ok(());
    }
    store.reconcile_provider_run(run_id, "UNKNOWN", &code)?;
    if let Some(id) = handoff_id {
        let _ = store.record_sending_handoff_uncertainty(
            id,
            "CHATGPT_ACCEPTANCE_UNPROVEN",
            "Exact acceptance unavailable; no automatic retry",
        );
    }
    Ok(())
}

/// A carrier code in this set is emitted before the sidecar reaches
/// `fillComposer`.  It is consequently safe to recover the review as
/// explicitly not submitted; it must never be displayed as an uncertain
/// delivery.
pub(crate) fn is_proven_prewrite_failure(code: &str) -> bool {
    matches!(
        code,
        "CHATGPT_AUTH_OR_COMPOSER_REQUIRED"
            | "CHATGPT_ACCOUNT_SECURITY_REQUIRED"
            | "CHATGPT_CONVERSATION_ID_INVALID"
            | "CHATGPT_CONVERSATION_NOT_OPEN"
            | "CHATGPT_EXACT_IDENTITY_MISMATCH"
            | "CHATGPT_GENERATING"
            | "CHATGPT_HUMAN_DRAFT_PRESENT"
            | "CHATGPT_ROUTER_PROFILE_IN_USE"
            | "CHATGPT_ROUTER_PROFILE_REQUIRED"
            | "CHATGPT_EXTENSION_NOT_CONNECTED"
            | "CHATGPT_EXTENSION_EXACT_TAB_NOT_OPEN"
            | "CHATGPT_EXTENSION_EXACT_IDENTITY_MISMATCH"
            | "CHATGPT_EXTENSION_STREAMING"
            | "CHATGPT_EXTENSION_HUMAN_DRAFT_PRESENT"
            | "CHATGPT_EXTENSION_SEND_CONTROL_UNAVAILABLE"
            | "CHATGPT_EXTENSION_ATTACHMENTS_NOT_AVAILABLE"
            | "AUTH_REQUIRED"
            | "HUMAN_DRAFT"
            | "HASH_MISMATCH"
            | "INVALID_REQUEST"
            | "INVALID_CONVERSATION_ID"
            | "IMMUTABLE_OUTER_WHITESPACE_UNSUPPORTED"
            | "INVALID_ATTACHMENTS"
            | "ATTACHMENT_CHANGED_BEFORE_UPLOAD"
            | "AMBIGUOUS_ATTACHMENT_NAMES"
    )
}

fn record_completion(
    state: &RouterCore,
    endpoint: &Endpoint,
    run_id: &str,
    observation_id: &str,
    identity: &str,
    final_text: String,
    cleanup: impl FnOnce() -> Result<(), String>,
) -> Result<CompletedChatGptResponse, String> {
    state.store.accept_completed_provider_result(
        run_id,
        observation_id,
        identity,
        final_text.clone(),
    )?;
    let completed = CompletedChatGptResponse {
        conversation_id: endpoint.external_id.clone(),
        response_identity: Some(identity.into()),
        relay_candidates: chatgpt::model::relay_candidates(&final_text),
        final_text,
        artifacts: None,
    };
    if let Ok(mut responses) = state.completed_chatgpt_responses.lock() {
        responses.insert(identity.into(), completed.clone());
    }
    // Failure to clear the page guard does not revoke a persisted exact result.
    // Leaving that guard in place remains fail-closed against a subsequent send.
    let _ = cleanup();
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::NewHandoff;

    struct Fixture {
        _directory: tempfile::TempDir,
        core: RouterCore,
        ws: String,
        endpoint: Endpoint,
        handoff: Option<String>,
        run: String,
    }
    fn fixture(mode: &str) -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
        let project = store
            .create_project("Native composition".into(), None)
            .unwrap();
        let ws = store
            .create_workstream(&project.id, "Fixture".into())
            .unwrap()
            .id;
        let endpoint = store
            .bind_endpoint(
                &ws,
                Provider::Chatgpt,
                "exact-conversation".into(),
                "ChatGPT".into(),
                false,
            )
            .unwrap();
        let handoff = if mode == "handoff" {
            let source = store
                .bind_endpoint(
                    &ws,
                    Provider::Codex,
                    "exact-thread".into(),
                    "Codex".into(),
                    false,
                )
                .unwrap();
            let h = store
                .create_ready_handoff(NewHandoff {
                    workstream_id: ws.clone(),
                    source_endpoint_id: source.id,
                    destination_endpoint_id: endpoint.id.clone(),
                    direction: "CODEX_TO_CHATGPT".into(),
                    source_response_identity: None,
                    original_text: "reviewed".into(),
                    approved_text: "reviewed".into(),
                    attachments: vec![],
                })
                .unwrap();
            store.transition_handoff(&h.id, "APPROVED", None).unwrap();
            store.transition_handoff(&h.id, "SENDING", None).unwrap();
            Some(h.id)
        } else {
            None
        };
        let retained = if mode == "feedback" {
            let prior = store
                .create_provider_run(&ws, &endpoint.id, "CHATGPT", None, None, "STARTING")
                .unwrap();
            store
                .attach_provider_run_external_identity(&prior.id, "CHATGPT", "historical:prior")
                .unwrap();
            store
                .accept_completed_provider_result(
                    &prior.id,
                    "historical:prior",
                    "old-reply",
                    "retained result".into(),
                )
                .unwrap();
            Some(prior.id)
        } else {
            None
        };
        let run = store
            .create_provider_run(
                &ws,
                &endpoint.id,
                "CHATGPT",
                handoff.as_deref(),
                None,
                "STARTING",
            )
            .unwrap()
            .id;
        if let Some(prior) = retained {
            store.record_chatgpt_feedback_source(&run, &prior).unwrap();
        }
        Fixture {
            _directory: directory,
            core: RouterCore {
                chatgpt: Arc::default(),
                session: Arc::new(Mutex::new(Session::default())),
                completed_chatgpt_responses: Arc::new(Mutex::new(HashMap::new())),
                store,
            },
            ws,
            endpoint,
            handoff,
            run,
        }
    }
    fn run(f: &Fixture) -> persistence::ProviderRun {
        f.core
            .store
            .provider_runs_for_workstream(&f.ws)
            .unwrap()
            .into_iter()
            .find(|r| r.id == f.run)
            .unwrap()
    }

    #[test]
    fn native_compositions_acceptance_is_not_completion_and_handoff_sent_requires_acceptance() {
        for mode in ["direct", "feedback", "handoff"] {
            let f = fixture(mode);
            assert_eq!(run(&f).status, "STARTING");
            if let Some(id) = &f.handoff {
                assert_eq!(f.core.store.handoff_by_id(id).unwrap().status, "SENDING");
            }
            let correlation = record_acceptance(
                &f.core.store,
                &f.run,
                f.handoff.as_deref(),
                "accepted-user-turn",
            )
            .unwrap();
            assert_eq!(correlation, "accepted-user-turn");
            let accepted = run(&f);
            assert_eq!(accepted.status, "RUNNING");
            assert_eq!(
                accepted.external_run_id.as_deref(),
                Some(correlation.as_str())
            );
            assert!(accepted.result_text.is_none());
            if let Some(id) = &f.handoff {
                assert_eq!(f.core.store.handoff_by_id(id).unwrap().status, "SENT");
            }
            let completed = record_completion(
                &f.core,
                &f.endpoint,
                &f.run,
                &correlation,
                "same-turn-assistant",
                "exact result".into(),
                || Ok(()),
            )
            .unwrap();
            assert_eq!(
                completed.response_identity.as_deref(),
                Some("same-turn-assistant")
            );
            assert_eq!(run(&f).status, "COMPLETED");
            assert_eq!(run(&f).result_text.as_deref(), Some("exact result"));
        }
    }

    #[test]
    fn native_unaccepted_compositions_preserve_uncertainty_without_external_identity_or_sent_handoff(
    ) {
        for mode in ["direct", "feedback", "handoff"] {
            let f = fixture(mode);
            record_acceptance_unproven(&f.core.store, &f.run, f.handoff.as_deref()).unwrap();
            record_unclassified_failure(
                &f.core.store,
                &f.ws,
                &f.run,
                f.handoff.as_deref(),
                "CHATGPT_SUBMIT_ACCEPTANCE_UNKNOWN",
            )
            .unwrap();
            let unproved = run(&f);
            assert_eq!(unproved.status, "UNKNOWN");
            assert_eq!(
                unproved.terminal_code.as_deref(),
                Some("CHATGPT_ACCEPTANCE_UNPROVEN")
            );
            assert!(unproved.external_run_id.is_none());
            assert!(unproved.result_text.is_none());
            if let Some(id) = &f.handoff {
                assert_eq!(f.core.store.handoff_by_id(id).unwrap().status, "SENDING");
            }
            if mode == "feedback" {
                assert_eq!(
                    f.core
                        .store
                        .latest_reviewable_provider_result(&f.ws, "CHATGPT")
                        .unwrap()
                        .unwrap()
                        .result_text
                        .as_deref(),
                    Some("retained result")
                );
            }
        }
    }

    #[test]
    fn missing_composer_is_proven_prewrite_and_releases_the_handoff_for_manual_recovery() {
        let f = fixture("handoff");
        record_unclassified_failure(
            &f.core.store,
            &f.ws,
            &f.run,
            f.handoff.as_deref(),
            "CHATGPT_AUTH_OR_COMPOSER_REQUIRED",
        )
        .unwrap();

        let failed = run(&f);
        assert_eq!(failed.status, "FAILED");
        assert_eq!(
            failed.terminal_code.as_deref(),
            Some("CHATGPT_AUTH_OR_COMPOSER_REQUIRED")
        );
        assert!(failed.external_run_id.is_none());
        let handoff = f
            .core
            .store
            .handoff_by_id(f.handoff.as_deref().unwrap())
            .unwrap();
        assert_eq!(handoff.status, "FAILED");
        assert_eq!(
            handoff.error_code.as_deref(),
            Some("CHATGPT_AUTH_OR_COMPOSER_REQUIRED")
        );
    }

    #[test]
    fn executor_auth_before_write_is_failed_not_unknown_and_does_not_claim_acceptance() {
        let f = fixture("handoff");
        record_unclassified_failure(&f.core.store, &f.ws, &f.run, f.handoff.as_deref(), "AUTH_REQUIRED").unwrap();
        assert_eq!(run(&f).status, "FAILED");
        assert_eq!(run(&f).terminal_code.as_deref(), Some("AUTH_REQUIRED"));
        assert!(run(&f).external_run_id.is_none());
        assert_eq!(f.core.store.handoff_by_id(f.handoff.as_deref().unwrap()).unwrap().status, "FAILED");
    }

    #[test]
    fn accepted_but_unobserved_terminal_preserves_specific_code_and_sent_handoff() {
        let f = fixture("handoff");
        record_acceptance(
            &f.core.store,
            &f.run,
            f.handoff.as_deref(),
            "accepted-user-turn",
        )
        .unwrap();
        f.core
            .store
            .reconcile_provider_run(&f.run, "UNKNOWN", "CHATGPT_TERMINAL_NOT_OBSERVED")
            .unwrap();
        record_unclassified_failure(
            &f.core.store,
            &f.ws,
            &f.run,
            f.handoff.as_deref(),
            "CHATGPT_TERMINAL_NOT_OBSERVED",
        )
        .unwrap();
        assert_eq!(
            run(&f).terminal_code.as_deref(),
            Some("CHATGPT_TERMINAL_NOT_OBSERVED")
        );
        assert_eq!(
            f.core
                .store
                .handoff_by_id(f.handoff.as_deref().unwrap())
                .unwrap()
                .status,
            "SENT"
        );
    }

    #[test]
    fn page_cleanup_failure_does_not_revoke_persisted_exact_completion() {
        let f = fixture("direct");
        let correlation =
            record_acceptance(&f.core.store, &f.run, None, "accepted-user-turn").unwrap();
        let mut cleanup_calls = 0;
        let completed = record_completion(
            &f.core,
            &f.endpoint,
            &f.run,
            &correlation,
            "assistant",
            "done".into(),
            || {
                cleanup_calls += 1;
                Err("native page closed after completion".into())
            },
        )
        .unwrap();
        assert_eq!(cleanup_calls, 1);
        assert_eq!(completed.final_text, "done");
        record_unclassified_failure(
            &f.core.store,
            &f.ws,
            &f.run,
            None,
            "CHATGPT_SUBMIT_ACCEPTANCE_UNKNOWN",
        )
        .unwrap();
        assert_eq!(run(&f).status, "COMPLETED");
        assert_eq!(run(&f).terminal_code.as_deref(), Some("COMPLETED"));
    }

    #[test]
    fn wrong_completion_correlation_cannot_persist_result_or_release_page_guard() {
        let f = fixture("direct");
        record_acceptance(&f.core.store, &f.run, None, "accepted-user-turn").unwrap();
        let mut cleanup_calls = 0;
        assert!(record_completion(
            &f.core,
            &f.endpoint,
            &f.run,
            "historical:wrong",
            "assistant",
            "wrong".into(),
            || {
                cleanup_calls += 1;
                Ok(())
            }
        )
        .is_err());
        assert_eq!(cleanup_calls, 0);
        assert_eq!(run(&f).status, "RUNNING");
        assert!(run(&f).result_text.is_none());
    }
}
