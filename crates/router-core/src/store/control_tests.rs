use super::control::*;
use super::*;
use crate::{
    identity::ScopeIdentity,
    runtime::PreviewProfile,
    store::control::binding::{
        BindingGrant, BindingOperation, BindingProposal, VerifiedBindingTarget,
    },
};

fn setup() -> (tempfile::TempDir, RouterStore, ScopeIdentity) {
    let dir = tempfile::tempdir().unwrap();
    let profile = PreviewProfile::acquire(dir.path().join("mcp-preview")).unwrap();
    RouterStore::initialize_preview(&profile).unwrap();
    let store = RouterStore::open_preview(profile).unwrap();
    let identity = ScopeIdentity {
        principal_key: "a".repeat(64),
        scope_key: "b".repeat(64),
        key_version: 1,
    };
    let ctx = store.ensure_unbound_context(&identity).unwrap();
    let policy = PolicySnapshot {
        version: 1,
        backend_id: Uuid::new_v4().to_string(),
        native_version: "0.154.0".into(),
        model: "fixture".into(),
        permission_mode: PermissionMode::ReadOnly,
        approval_mode: ApprovalMode::OnRequest,
        network_access: false,
        root_identity_hash: "c".repeat(64),
        config_digest: "d".repeat(64),
        native_launch_digest: None,
    };
    store.with_connection(|c| {
        c.execute_batch("INSERT INTO projects(id,name,created_at,updated_at) VALUES('p','fixture',1,1); INSERT INTO workstreams(id,project_id,name,status,created_at,updated_at) VALUES('w','p','fixture','ACTIVE',1,1); INSERT INTO endpoints(id,workstream_id,provider,external_id,label,status,created_at) VALUES('e','w','CODEX','exact-thread','fixture','ACTIVE',1);").map_err(db_error)?;
        c.execute("INSERT INTO project_execution_roots(project_id,canonical_path,path_identity_hash,revision,policy_json,policy_hash,updated_at) VALUES('p','fixture-root',?1,1,?2,?3,1)",params![policy.root_identity_hash,serde_json::to_string(&policy).unwrap(),policy.hash().unwrap()]).map_err(db_error)?;
        c.execute("UPDATE control_contexts SET state='BOUND',workstream_id='w',revision=1 WHERE id=?1",params![ctx.id]).map_err(db_error)?;
        Ok(())
    }).unwrap();
    (dir, store, identity)
}
fn draft(store: &RouterStore, identity: &ScopeIdentity) -> DraftRecord {
    store
        .prepare_instruction(
            identity,
            &Uuid::new_v4().to_string(),
            1,
            0,
            "  exact\r\ntext  ",
        )
        .unwrap()
}
fn grant(store: &RouterStore, identity: &ScopeIdentity, draft: &DraftRecord) -> ApprovalGrant {
    ApprovalGrant::from_verified_browser(
        identity.principal_key.clone(),
        store
            .review_snapshot(&identity.principal_key, &draft.handoff_id)
            .unwrap(),
        "e".repeat(64),
        now() + 600_000,
    )
    .unwrap()
}
fn count(store: &RouterStore, table: &str) -> i64 {
    store
        .with_connection(|c| {
            c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .map_err(db_error)
        })
        .unwrap()
}

#[test]
fn second_owner_scope_must_reconfirm_existing_workstream_before_selecting_same_thread() {
    let (_dir, store, identity) = setup();
    let second = ScopeIdentity {
        principal_key: identity.principal_key.clone(),
        scope_key: "e".repeat(64),
        key_version: 1,
    };
    let review = store
        .prepare_binding(
            &second,
            &BindingProposal {
                client_request_id: id(),
                operation: BindingOperation::Select,
                expected_context_revision: 0,
                project_id: Some("p".into()),
                candidate_thread_id: Some("exact-thread".into()),
                workstream_id: None,
            },
        )
        .unwrap();
    assert!(store
        .binding_requires_owned_workstream_reconfirmation(&identity.principal_key, &review)
        .unwrap());
    let target = VerifiedBindingTarget {
        thread_id: "exact-thread".into(),
        policy_hash: store.registered_root("p").unwrap().policy.hash().unwrap(),
        root_revision: 1,
        durable: true,
    };
    assert_eq!(
        store
            .apply_binding(
                &BindingGrant {
                    principal_key: identity.principal_key.clone(),
                    review,
                    nonce_hash: "f".repeat(64),
                },
                Some(&target),
            )
            .unwrap_err(),
        "OWNED_WORKSTREAM_RECONFIRMATION_REQUIRED"
    );
    assert_eq!(count(&store, "workstreams"), 1);
    assert_eq!(count(&store, "endpoints"), 1);
    assert_eq!(
        store.control_context(&second).unwrap().unwrap().state,
        "UNBOUND"
    );
}

#[test]
fn native_baseline_root_migration_is_cas_idempotent_and_invalidates_old_binding() {
    let (_dir, store, identity) = setup();
    let (context_id, old_policy): (String, PolicySnapshot) = store
        .with_connection(|connection| {
            let context_id = connection
                .query_row("SELECT id FROM control_contexts", [], |row| row.get(0))
                .map_err(db_error)?;
            let raw: String = connection
                .query_row(
                    "SELECT policy_json FROM project_execution_roots WHERE project_id='p'",
                    [],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            Ok((
                context_id,
                serde_json::from_str(&raw).map_err(|_| "POLICY_CHANGED")?,
            ))
        })
        .unwrap();
    store
        .with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO binding_requests(id,context_id,client_request_id,request_hash,operation,project_id,workstream_id,expected_context_revision,expected_binding_revision,root_revision,candidate_thread_id,candidate_policy_hash,state,approved_principal_key,review_nonce_hash,created_at,expires_at) VALUES(?1,?2,?3,?4,'SELECT','p','w',1,0,1,'exact-thread',?5,'APPLIED',?6,?7,1,2)",
                    params![id(), context_id, id(), "a".repeat(64), old_policy.hash()?, identity.principal_key, "b".repeat(64)],
                )
                .map_err(db_error)?;
            Ok(())
        })
        .unwrap();
    let migration = store
        .migrate_native_baseline_roots(&"d".repeat(64), &"e".repeat(64))
        .unwrap();
    assert!(migration.changed);
    assert_eq!(migration.revisions, vec![2]);
    assert_eq!(
        store
            .prepare_instruction(&identity, &id(), 1, 0, "must not draft")
            .unwrap_err(),
        "BINDING_REVALIDATION_REQUIRED"
    );
    assert_eq!(count(&store, "handoffs"), 0);
    assert_eq!(count(&store, "provider_runs"), 0);
    let replay = store
        .migrate_native_baseline_roots(&"d".repeat(64), &"e".repeat(64))
        .unwrap();
    assert!(!replay.changed);
    assert_eq!(replay.revisions, vec![2]);
    let rollback = store
        .migrate_native_baseline_roots(&"e".repeat(64), &"d".repeat(64))
        .unwrap();
    assert!(rollback.changed);
    assert_eq!(rollback.revisions, vec![3]);
    assert_eq!(
        store
            .prepare_instruction(&identity, &id(), 1, 0, "still not a draft")
            .unwrap_err(),
        "BINDING_REVALIDATION_REQUIRED"
    );
    assert!(store
        .migrate_native_baseline_roots(&"e".repeat(64), &"f".repeat(64))
        .is_err());
}

#[test]
fn native_launch_policy_amendment_is_cas_idempotent_and_invalidates_old_binding() {
    let (_dir, store, identity) = setup();
    let (context_id, old_policy): (String, PolicySnapshot) = store
        .with_connection(|connection| {
            let context_id = connection
                .query_row("SELECT id FROM control_contexts", [], |row| row.get(0))
                .map_err(db_error)?;
            let raw: String = connection
                .query_row(
                    "SELECT policy_json FROM project_execution_roots WHERE project_id='p'",
                    [],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            Ok((
                context_id,
                serde_json::from_str(&raw).map_err(|_| "POLICY_CHANGED")?,
            ))
        })
        .unwrap();
    store
        .with_connection(|connection| {
            connection.execute(
                "INSERT INTO binding_requests(id,context_id,client_request_id,request_hash,operation,project_id,workstream_id,expected_context_revision,expected_binding_revision,root_revision,candidate_thread_id,candidate_policy_hash,state,approved_principal_key,review_nonce_hash,created_at,expires_at) VALUES(?1,?2,?3,?4,'SELECT','p','w',1,0,1,'exact-thread',?5,'APPLIED',?6,?7,1,2)",
                params![id(), context_id, id(), "a".repeat(64), old_policy.hash()?, identity.principal_key, "b".repeat(64)],
            ).map_err(db_error)?;
            Ok(())
        })
        .unwrap();
    let next = "e".repeat(64);
    let changed = store
        .amend_native_launch_policy_roots(&"d".repeat(64), None, Some(&next))
        .unwrap();
    assert!(changed.changed);
    assert_eq!(changed.revisions, vec![2]);
    let migrated_policy: PolicySnapshot = store
        .with_connection(|connection| {
            let raw: String = connection
                .query_row(
                    "SELECT policy_json FROM project_execution_roots WHERE project_id='p'",
                    [],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            serde_json::from_str(&raw).map_err(|_| "POLICY_CHANGED".into())
        })
        .unwrap();
    assert_eq!(
        migrated_policy.permission_mode,
        PermissionMode::WorkspaceWrite
    );
    assert_eq!(migrated_policy.approval_mode, ApprovalMode::Never);
    assert_eq!(
        store
            .prepare_instruction(&identity, &id(), 1, 0, "must not draft")
            .unwrap_err(),
        "BINDING_REVALIDATION_REQUIRED"
    );
    assert_eq!(count(&store, "handoffs"), 0);
    let replay = store
        .amend_native_launch_policy_roots(&"d".repeat(64), None, Some(&next))
        .unwrap();
    assert!(!replay.changed);
    assert_eq!(replay.revisions, vec![2]);
    assert!(store
        .amend_native_launch_policy_roots(&"d".repeat(64), None, Some(&"f".repeat(64)))
        .is_err());
}
#[test]
fn native_source_input_migration_is_atomic_idempotent_and_invalidates_old_binding() {
    let (_dir, store, identity) = setup();
    let old = "d".repeat(64);
    let next = "e".repeat(64);
    let launch = "f".repeat(64);
    let (context_id, old_policy): (String, PolicySnapshot) = store
        .with_connection(|connection| {
            let context_id = connection
                .query_row("SELECT id FROM control_contexts", [], |row| row.get(0))
                .map_err(db_error)?;
            let raw: String = connection
                .query_row(
                    "SELECT policy_json FROM project_execution_roots WHERE project_id='p'",
                    [],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            Ok((
                context_id,
                serde_json::from_str(&raw).map_err(|_| "POLICY_CHANGED")?,
            ))
        })
        .unwrap();
    store
        .with_connection(|connection| {
            connection.execute(
                "INSERT INTO binding_requests(id,context_id,client_request_id,request_hash,operation,project_id,workstream_id,expected_context_revision,expected_binding_revision,root_revision,candidate_thread_id,candidate_policy_hash,state,approved_principal_key,review_nonce_hash,created_at,expires_at) VALUES(?1,?2,?3,?4,'SELECT','p','w',1,0,1,'exact-thread',?5,'APPLIED',?6,?7,1,2)",
                params![id(), context_id, id(), "a".repeat(64), old_policy.hash()?, identity.principal_key, "b".repeat(64)],
            ).map_err(db_error)?;
            Ok(())
        })
        .unwrap();
    let changed = store
        .migrate_native_source_input_roots(&old, None, &next, Some(&launch))
        .unwrap();
    assert!(changed.changed);
    assert_eq!(changed.revisions, vec![2]);
    assert_eq!(
        store
            .prepare_instruction(&identity, &id(), 1, 0, "must not draft")
            .unwrap_err(),
        "BINDING_REVALIDATION_REQUIRED"
    );
    assert_eq!(count(&store, "handoffs"), 0);
    let replay = store
        .migrate_native_source_input_roots(&old, None, &next, Some(&launch))
        .unwrap();
    assert!(!replay.changed);
    assert_eq!(replay.revisions, vec![2]);
    assert!(store
        .migrate_native_source_input_roots(&old, None, &next, Some(&"a".repeat(64)))
        .is_err());
}
#[test]
fn draft_is_idempotent_content_exact_and_never_claims() {
    let (_dir, s, i) = setup();
    let request = Uuid::new_v4().to_string();
    let a = s
        .prepare_instruction(&i, &request, 1, 0, "  exact\r\ntext  ")
        .unwrap();
    let b = s
        .prepare_instruction(&i, &request, 1, 0, "  exact\ntext  ")
        .unwrap();
    assert_eq!(a.handoff_id, b.handoff_id);
    assert_eq!(
        s.review_snapshot(&i.principal_key, &a.handoff_id)
            .unwrap()
            .text,
        "  exact\ntext  "
    );
    assert_eq!(
        s.prepare_instruction(&i, &request, 1, 0, "changed")
            .unwrap_err(),
        "IDEMPOTENCY_CONFLICT"
    );
    assert_eq!(count(&s, "provider_runs"), 0);
    assert_eq!(count(&s, "handoff_dispatches"), 0);
}
#[test]
fn cross_scope_cross_principal_and_stale_versions_do_not_mutate() {
    let (_dir, s, i) = setup();
    let d = draft(&s, &i);
    let other = ScopeIdentity {
        scope_key: "f".repeat(64),
        ..i.clone()
    };
    s.ensure_unbound_context(&other).unwrap();
    assert_eq!(
        s.scoped_draft(&other, &d.handoff_id).unwrap_err(),
        "NOT_FOUND"
    );
    assert_eq!(
        s.edit_instruction(&other, &d.handoff_id, 1, "bad")
            .unwrap_err(),
        "NOT_FOUND"
    );
    assert_eq!(
        s.review_snapshot(&"f".repeat(64), &d.handoff_id)
            .unwrap_err(),
        "NOT_FOUND"
    );
    assert_eq!(
        s.prepare_instruction(&i, &Uuid::new_v4().to_string(), 0, 0, "bad")
            .unwrap_err(),
        "STALE_CONTEXT"
    );
    assert_eq!(
        s.prepare_instruction(&i, &Uuid::new_v4().to_string(), 1, 99, "bad")
            .unwrap_err(),
        "STALE_BINDING"
    );
    assert_eq!(
        s.edit_instruction(&i, &d.handoff_id, 99, "bad")
            .unwrap_err(),
        "STALE_DRAFT"
    );
    assert_eq!(count(&s, "handoffs"), 1);
    assert_eq!(count(&s, "provider_runs"), 0);
}
#[test]
fn edit_invalidates_old_approval_and_claim_is_atomic_one_shot() {
    let (_dir, s, i) = setup();
    let d = draft(&s, &i);
    let old = grant(&s, &i, &d);
    let epoch = id();
    let d = s
        .edit_instruction(&i, &d.handoff_id, 1, "edited exact")
        .unwrap();
    assert_eq!(
        s.approve_and_claim(&old, &epoch).unwrap_err(),
        "STALE_DRAFT"
    );
    let g = grant(&s, &i, &d);
    let claimed = s.approve_and_claim(&g, &epoch).unwrap();
    assert!(claimed.newly_claimed);
    let repeated = s.approve_and_claim(&g, &epoch).unwrap();
    assert!(!repeated.newly_claimed);
    assert_eq!(claimed.run_id, repeated.run_id);
    assert_eq!(count(&s, "provider_runs"), 1);
    assert_eq!(count(&s, "handoff_dispatches"), 1);
    assert_eq!(s.scoped_draft(&i, &d.handoff_id).unwrap().state, "SENDING");
    assert!(s
        .edit_instruction(&i, &d.handoff_id, 2, "cannot resend")
        .is_err());
    assert_eq!(
        s.prepare_instruction(&i, &id(), 1, 0, "second")
            .unwrap_err(),
        "RUN_IN_FLIGHT"
    );
}
#[test]
fn failed_audit_or_claim_insert_rolls_back_all_approval_facts() {
    let (_dir, s, i) = setup();
    let d = draft(&s, &i);
    let g = grant(&s, &i, &d);
    s.with_connection(|c|c.execute_batch("CREATE TRIGGER injected_failure BEFORE INSERT ON handoff_dispatches BEGIN SELECT RAISE(ABORT,'fixture'); END;").map_err(db_error)).unwrap();
    assert!(s.approve_and_claim(&g, &id()).is_err());
    assert_eq!(s.scoped_draft(&i, &d.handoff_id).unwrap().state, "READY");
    assert_eq!(count(&s, "provider_runs"), 0);
    s.with_connection(|c| {let values:(Option<i64>,Option<String>,Option<i64>)=c.query_row("SELECT h.approved_at,d.approved_principal_key,d.consumed_at FROM handoffs h JOIN control_handoff_details d ON d.handoff_id=h.id",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(db_error)?;assert_eq!(values,(None,None,None));Ok(())}).unwrap();
}
#[test]
fn expiry_root_policy_binding_and_quiesce_block_claim() {
    let (_dir, s, i) = setup();
    let d = draft(&s, &i);
    let mut g = grant(&s, &i, &d);
    g.expires_at = now() - 1;
    assert_eq!(s.approve_and_claim(&g, &id()).unwrap_err(), "EXPIRED");
    g.expires_at = now() + 600_000;
    s.with_connection(|c| {
        c.execute("UPDATE project_execution_roots SET revision=2", [])
            .map_err(db_error)?;
        Ok(())
    })
    .unwrap();
    assert_eq!(s.approve_and_claim(&g, &id()).unwrap_err(), "ROOT_CHANGED");
    s.with_connection(|c|c.execute_batch("UPDATE project_execution_roots SET revision=1; UPDATE workstreams SET binding_revision=1;").map_err(db_error)).unwrap();
    assert_eq!(s.approve_and_claim(&g, &id()).unwrap_err(), "STALE_BINDING");
    s.with_connection(|c|c.execute_batch("UPDATE workstreams SET binding_revision=0; INSERT INTO runtime_exit_state(singleton,revision,adapter_epoch,phase,unresolved_set_hash) VALUES(1,1,'epoch','QUIESCED','hash');").map_err(db_error)).unwrap();
    assert_eq!(s.approve_and_claim(&g, &id()).unwrap_err(), "EXIT_PENDING");
    assert_eq!(count(&s, "provider_runs"), 0);
}
#[test]
fn restart_preserves_claim_and_never_unlocks_it() {
    let (dir, s, i) = setup();
    let d = draft(&s, &i);
    s.approve_and_claim(&grant(&s, &i, &d), &id()).unwrap();
    drop(s);
    let s =
        RouterStore::open_preview(PreviewProfile::acquire(dir.path().join("mcp-preview")).unwrap())
            .unwrap();
    assert_eq!(count(&s, "handoff_dispatches"), 1);
    assert_eq!(
        s.prepare_instruction(&i, &id(), 1, 0, "second")
            .unwrap_err(),
        "RUN_IN_FLIGHT"
    );
}

#[test]
fn reviewed_replace_is_exact_revisioned_and_duplicate_only_reads_back() {
    use super::control::binding::*;
    let (_dir, s, i) = setup();
    let old_draft = draft(&s, &i);
    let proposal = BindingProposal {
        client_request_id: id(),
        operation: BindingOperation::Replace,
        expected_context_revision: 1,
        project_id: Some("p".into()),
        candidate_thread_id: Some("new-exact-thread".into()),
        workstream_id: None,
    };
    let review = s.prepare_binding(&i, &proposal).unwrap();
    let target = VerifiedBindingTarget {
        thread_id: "wrong".into(),
        policy_hash: review.policy_hash.clone().unwrap(),
        root_revision: review.root_revision.unwrap(),
        durable: true,
    };
    let grant = BindingGrant {
        principal_key: i.principal_key.clone(),
        review,
        nonce_hash: "f".repeat(64),
    };
    assert!(s.apply_binding(&grant, Some(&target)).is_err());
    assert_eq!(s.control_context(&i).unwrap().unwrap().revision, 1);
    let target = VerifiedBindingTarget {
        thread_id: "new-exact-thread".into(),
        ..target
    };
    let applied = s.apply_binding(&grant, Some(&target)).unwrap();
    assert_eq!(applied.revision, 2);
    assert_eq!(s.apply_binding(&grant, Some(&target)).unwrap().revision, 2);
    assert_eq!(
        s.prepare_instruction(&i, &id(), 1, 0, "stale").unwrap_err(),
        "STALE_CONTEXT"
    );
    assert!(s.scoped_draft(&i, &old_draft.handoff_id).is_err());
    s.with_connection(|c| {
        assert_eq!(
            c.query_row(
                "SELECT status FROM handoffs WHERE id=?1",
                params![old_draft.handoff_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "CANCELLED"
        );
        assert_eq!(
            c.query_row("SELECT status FROM endpoints WHERE id='e'", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "SUPERSEDED"
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn takeover_revokes_old_chat_and_unknown_prevents_unbind() {
    use super::control::binding::*;
    let (_dir, s, i) = setup();
    let next = ScopeIdentity {
        scope_key: "e".repeat(64),
        ..i.clone()
    };
    let review = s
        .prepare_binding(
            &next,
            &BindingProposal {
                client_request_id: id(),
                operation: BindingOperation::Takeover,
                expected_context_revision: 0,
                project_id: None,
                candidate_thread_id: None,
                workstream_id: Some("w".into()),
            },
        )
        .unwrap();
    let target = VerifiedBindingTarget {
        thread_id: "exact-thread".into(),
        policy_hash: review.policy_hash.clone().unwrap(),
        root_revision: 1,
        durable: true,
    };
    s.apply_binding(
        &BindingGrant {
            principal_key: next.principal_key.clone(),
            review,
            nonce_hash: "f".repeat(64),
        },
        Some(&target),
    )
    .unwrap();
    assert_eq!(s.control_context(&i).unwrap().unwrap().state, "REVOKED");
    assert!(s.prepare_instruction(&i, &id(), 2, 1, "old chat").is_err());
    let d = s
        .prepare_instruction(&next, &id(), 1, 1, "one task")
        .unwrap();
    s.approve_and_claim(&grant(&s, &next, &d), &id()).unwrap();
    assert_eq!(
        s.prepare_binding(
            &next,
            &BindingProposal {
                client_request_id: id(),
                operation: BindingOperation::Unbind,
                expected_context_revision: 1,
                project_id: None,
                candidate_thread_id: None,
                workstream_id: None
            }
        )
        .unwrap_err(),
        "RUN_IN_FLIGHT"
    );
}

#[test]
fn creation_is_durable_one_shot_and_cannot_bind_before_fresh_verification() {
    use super::control::binding::*;
    let (dir, s, i) = setup();
    let next = ScopeIdentity {
        scope_key: "e".repeat(64),
        ..i.clone()
    };
    let review = s
        .prepare_binding(
            &next,
            &BindingProposal {
                client_request_id: id(),
                operation: BindingOperation::Create,
                expected_context_revision: 0,
                project_id: Some("p".into()),
                candidate_thread_id: None,
                workstream_id: None,
            },
        )
        .unwrap();
    let target = VerifiedBindingTarget {
        thread_id: "new-candidate".into(),
        policy_hash: review.policy_hash.clone().unwrap(),
        root_revision: 1,
        durable: true,
    };
    let grant = BindingGrant {
        principal_key: next.principal_key.clone(),
        review,
        nonce_hash: "f".repeat(64),
    };
    assert!(s.apply_binding(&grant, Some(&target)).is_err());
    assert!(s.begin_creation(&grant).unwrap().1);
    assert!(!s.begin_creation(&grant).unwrap().1);
    s.record_created_candidate(&grant, &target.thread_id)
        .unwrap();
    assert!(s.apply_binding(&grant, Some(&target)).is_err());
    drop(s);
    let s =
        RouterStore::open_preview(PreviewProfile::acquire(dir.path().join("mcp-preview")).unwrap())
            .unwrap();
    assert!(!s.begin_creation(&grant).unwrap().1);
    // A real runtime restart must mark interrupted verification UNKNOWN before
    // serving; this component fixture explicitly supplies its fresh proof.
    s.finish_creation_verification(&grant, &target).unwrap();
    assert_eq!(
        s.apply_binding(&grant, Some(&target)).unwrap().state,
        "BOUND"
    );
    assert_eq!(count(&s, "provider_runs"), 0);
}
#[test]
fn ambiguous_creation_never_restarts_or_becomes_not_sent_by_expiry() {
    use super::control::binding::*;
    let (_dir, s, i) = setup();
    let next = ScopeIdentity {
        scope_key: "e".repeat(64),
        ..i.clone()
    };
    let review = s
        .prepare_binding(
            &next,
            &BindingProposal {
                client_request_id: id(),
                operation: BindingOperation::Create,
                expected_context_revision: 0,
                project_id: Some("p".into()),
                candidate_thread_id: None,
                workstream_id: None,
            },
        )
        .unwrap();
    let grant = BindingGrant {
        principal_key: next.principal_key.clone(),
        review,
        nonce_hash: "f".repeat(64),
    };
    s.begin_creation(&grant).unwrap();
    s.mark_creation_unknown(&grant).unwrap();
    assert!(!s.begin_creation(&grant).unwrap().1);
    assert_eq!(
        s.binding_review(&next.principal_key, &grant.review.id)
            .unwrap()
            .state,
        "UNKNOWN"
    );
    assert_eq!(
        s.prepare_binding(
            &next,
            &BindingProposal {
                client_request_id: id(),
                operation: BindingOperation::Create,
                expected_context_revision: 0,
                project_id: Some("p".into()),
                candidate_thread_id: None,
                workstream_id: None
            }
        )
        .unwrap_err(),
        "CREATION_UNKNOWN"
    );
}

#[test]
fn unresolved_creation_in_another_context_does_not_block_a_new_isolated_workstream() {
    use super::control::binding::*;
    let (_dir, s, i) = setup();
    let first = ScopeIdentity {
        scope_key: "e".repeat(64),
        ..i.clone()
    };
    let first_review = s
        .prepare_binding(
            &first,
            &BindingProposal {
                client_request_id: id(),
                operation: BindingOperation::Create,
                expected_context_revision: 0,
                project_id: Some("p".into()),
                candidate_thread_id: None,
                workstream_id: None,
            },
        )
        .unwrap();
    let first_grant = BindingGrant {
        principal_key: first.principal_key.clone(),
        review: first_review,
        nonce_hash: "f".repeat(64),
    };
    s.begin_creation(&first_grant).unwrap();
    s.mark_creation_unknown(&first_grant).unwrap();

    let second = ScopeIdentity {
        scope_key: "d".repeat(64),
        ..i.clone()
    };
    let second_review = s
        .prepare_binding(
            &second,
            &BindingProposal {
                client_request_id: id(),
                operation: BindingOperation::Create,
                expected_context_revision: 0,
                project_id: Some("p".into()),
                candidate_thread_id: None,
                workstream_id: None,
            },
        )
        .unwrap();
    let second_grant = BindingGrant {
        principal_key: second.principal_key.clone(),
        review: second_review,
        nonce_hash: "a".repeat(64),
    };
    assert!(s.begin_creation(&second_grant).unwrap().1);
    assert_eq!(
        s.binding_review(&first.principal_key, &first_grant.review.id)
            .unwrap()
            .state,
        "UNKNOWN"
    );
}

#[test]
fn write_intent_is_one_shot_revalidated_and_survives_restart_as_unknown() {
    let (dir, s, i) = setup();
    let d = draft(&s, &i);
    let g = grant(&s, &i, &d);
    let epoch = id();
    s.recover_preview_startup(&epoch).unwrap();
    let run = s.approve_and_claim(&g, &epoch).unwrap();
    let snap = s
        .begin_dispatch_preparation(&run.dispatch_id, &epoch)
        .unwrap();
    assert!(s
        .begin_dispatch_preparation(&run.dispatch_id, &epoch)
        .is_err());
    assert!(s
        .persist_write_intent(&run.dispatch_id, &id(), &serde_json::json!(7), &snap)
        .is_err());
    let mut altered = snap.clone();
    altered.text = "unapproved".into();
    assert!(s
        .persist_write_intent(&run.dispatch_id, &epoch, &serde_json::json!(7), &altered)
        .is_err());
    assert!(s
        .persist_write_intent(&run.dispatch_id, &epoch, &serde_json::Value::Null, &snap)
        .is_err());
    s.persist_write_intent(&run.dispatch_id, &epoch, &serde_json::json!(7), &snap)
        .unwrap();
    assert!(s
        .persist_write_intent(&run.dispatch_id, &epoch, &serde_json::json!(7), &snap)
        .is_err());
    drop(s);
    let s =
        RouterStore::open_preview(PreviewProfile::acquire(dir.path().join("mcp-preview")).unwrap())
            .unwrap();
    let next = id();
    assert_eq!(s.recover_preview_startup(&next).unwrap(), 1);
    let r = s.dispatch_record(&run.dispatch_id).unwrap();
    assert_eq!(r.phase, "UNKNOWN");
    assert_eq!(r.request_id, Some(serde_json::json!(7)));
    assert_eq!(r.adapter_epoch, epoch);
    assert!(s.recover_preview_startup(&next).is_err());
    assert!(s
        .begin_dispatch_preparation(&run.dispatch_id, &next)
        .is_err());
    assert!(s.prepare_instruction(&i, &id(), 1, 0, "retry").is_err());
    assert_eq!(count(&s, "provider_runs"), 1);
}
#[test]
fn failed_write_intent_transaction_authorizes_nothing_and_restart_does_not_unlock() {
    let (_dir, s, i) = setup();
    let d = draft(&s, &i);
    let g = grant(&s, &i, &d);
    let epoch = id();
    let run = s.approve_and_claim(&g, &epoch).unwrap();
    let snap = s
        .begin_dispatch_preparation(&run.dispatch_id, &epoch)
        .unwrap();
    s.with_connection(|c|c.execute_batch("CREATE TRIGGER reject_intent BEFORE UPDATE ON handoff_dispatches WHEN NEW.phase='WRITE_INTENT' BEGIN SELECT RAISE(ABORT,'fixture'); END;").map_err(db_error)).unwrap();
    assert!(s
        .persist_write_intent(&run.dispatch_id, &epoch, &serde_json::json!("7"), &snap)
        .is_err());
    let r = s.dispatch_record(&run.dispatch_id).unwrap();
    assert_eq!(r.phase, "PREPARING");
    assert!(r.request_id.is_none());
    s.mark_dispatch_unknown(&run.dispatch_id, &epoch, "AUDIT_FAILED")
        .unwrap();
    let rev = s.dispatch_record(&run.dispatch_id).unwrap().revision;
    s.mark_dispatch_unknown(&run.dispatch_id, &epoch, "AUDIT_FAILED")
        .unwrap();
    assert_eq!(s.dispatch_record(&run.dispatch_id).unwrap().revision, rev);
    s.recover_preview_startup(&id()).unwrap();
    assert!(s.prepare_instruction(&i, &id(), 1, 0, "retry").is_err());
}

fn claimed_intent(s: &RouterStore, i: &ScopeIdentity) -> (String, ClaimedRun, ReviewSnapshot) {
    let d = draft(s, i);
    let g = grant(s, i, &d);
    let epoch = id();
    let r = s.approve_and_claim(&g, &epoch).unwrap();
    let snap = s
        .begin_dispatch_preparation(&r.dispatch_id, &epoch)
        .unwrap();
    s.persist_write_intent(&r.dispatch_id, &epoch, &serde_json::json!(9), &snap)
        .unwrap();
    (epoch, r, snap)
}
fn native_ack(epoch: &str) -> crate::codex::adapter::NativeTurnAck {
    crate::codex::adapter::NativeTurnAck {
        adapter_epoch: epoch.into(),
        request_id: serde_json::json!(9),
        thread_id: "exact-thread".into(),
        turn_id: "exact-turn".into(),
    }
}
#[test]
fn ack_receipt_precedes_acceptance_and_typed_request_mismatch_cannot_attach() {
    let (_dir, s, i) = setup();
    let (epoch, r, _) = claimed_intent(&s, &i);
    let key = [8; 32];
    let mut ack = native_ack(&epoch);
    ack.request_id = serde_json::json!("9");
    assert!(s
        .record_native_ack(&r.dispatch_id, &epoch, &ack, &key)
        .is_err());
    assert_eq!(count(&s, "dispatch_native_evidence"), 0);
    let receipt = s
        .record_native_ack(&r.dispatch_id, &epoch, &native_ack(&epoch), &key)
        .unwrap();
    assert_eq!(
        s.dispatch_record(&r.dispatch_id).unwrap().phase,
        "WRITE_INTENT"
    );
    assert_eq!(
        s.record_native_ack(&r.dispatch_id, &epoch, &native_ack(&epoch), &key)
            .unwrap()
            .id,
        receipt.id
    );
    assert!(s
        .accept_native_ack(&r.dispatch_id, &epoch, &[7; 32])
        .is_err());
    s.accept_native_ack(&r.dispatch_id, &epoch, &key).unwrap();
    assert_eq!(s.dispatch_record(&r.dispatch_id).unwrap().phase, "ACCEPTED");
    s.accept_native_ack(&r.dispatch_id, &epoch, &key).unwrap();
    assert_eq!(count(&s, "dispatch_native_evidence"), 1);
}
#[test]
fn terminal_persistence_is_atomic_and_exact_with_no_history_reader() {
    use crate::codex::result::LiveResult;
    use serde_json::json;
    let (_dir, s, i) = setup();
    let (epoch, r, _) = claimed_intent(&s, &i);
    let key = [8; 32];
    let mut live = LiveResult::new("exact-thread".into());
    // Arrival precedes ACK; no Provider history access is involved.
    live.observe(&json!({"method":"turn/completed","params":{"threadId":"exact-thread","turn":{"id":"exact-turn","status":"completed","itemsView":"summary","items":[{"type":"agentMessage","id":"item","phase":"final_answer","text":"short result"}]}}})).unwrap();
    assert!(live.terminal().is_none());
    live.associate_ack("exact-thread", "exact-turn").unwrap();
    let term = live.terminal().unwrap();
    assert!(s
        .persist_live_terminal(&r.dispatch_id, &epoch, &term, 1, &key)
        .is_err());
    s.record_native_ack(&r.dispatch_id, &epoch, &native_ack(&epoch), &key)
        .unwrap();
    s.with_connection(|c|c.execute_batch("CREATE TRIGGER reject_result BEFORE UPDATE ON provider_result_details BEGIN SELECT RAISE(ABORT,'fixture'); END;").map_err(db_error)).unwrap();
    assert!(s
        .persist_live_terminal(&r.dispatch_id, &epoch, &term, 1, &key)
        .is_err());
    assert_eq!(count(&s, "dispatch_native_evidence"), 1);
    assert_eq!(
        s.dispatch_record(&r.dispatch_id).unwrap().phase,
        "WRITE_INTENT"
    );
    s.with_connection(|c| {
        c.execute_batch("DROP TRIGGER reject_result;")
            .map_err(db_error)
    })
    .unwrap();
    s.persist_live_terminal(&r.dispatch_id, &epoch, &term, 1, &key)
        .unwrap();
    assert_eq!(s.dispatch_record(&r.dispatch_id).unwrap().phase, "TERMINAL");
    let result=s.with_connection(|c|c.query_row("SELECT p.status,h.status,p.result_text,d.result_state FROM provider_runs p JOIN handoffs h ON h.id=p.origin_handoff_id JOIN provider_result_details d ON d.run_id=p.id WHERE p.id=?1",params![r.run_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?))).map_err(db_error)).unwrap();
    assert_eq!(
        result,
        (
            "COMPLETED".into(),
            "SENT".into(),
            "short result".into(),
            "AVAILABLE".into()
        )
    );
    s.persist_live_terminal(&r.dispatch_id, &epoch, &term, 1, &key)
        .unwrap();
    assert!(s
        .prepare_instruction(&i, &id(), 1, 0, "next requires its own approval")
        .is_ok());
}
#[test]
fn late_ack_does_not_automatically_resolve_unknown() {
    let (_dir, s, i) = setup();
    let (epoch, r, _) = claimed_intent(&s, &i);
    let key = [8; 32];
    s.mark_dispatch_unknown(&r.dispatch_id, &epoch, "ACK_LOST")
        .unwrap();
    s.record_native_ack(&r.dispatch_id, &epoch, &native_ack(&epoch), &key)
        .unwrap();
    assert_eq!(
        s.accept_native_ack(&r.dispatch_id, &epoch, &key)
            .unwrap_err(),
        "OPERATOR_RESOLUTION_REQUIRED"
    );
    assert_eq!(s.dispatch_record(&r.dispatch_id).unwrap().phase, "UNKNOWN");
    assert!(s.prepare_instruction(&i, &id(), 1, 0, "retry").is_err());
}

fn resolution_grant(
    s: &RouterStore,
    i: &ScopeIdentity,
    dispatch: &str,
    action: super::dispatch::resolution::ResolutionAction,
    key: &[u8; 32],
) -> super::dispatch::resolution::ResolutionGrant {
    super::dispatch::resolution::ResolutionGrant {
        principal_key: i.principal_key.clone(),
        preview: s
            .resolution_preview(&i.principal_key, dispatch, action, key)
            .unwrap(),
        nonce_hash: "1".repeat(64),
        expires_at: now() + 600_000,
    }
}
#[test]
fn unknown_no_evidence_only_keep_and_audit_failure_preserves_lock() {
    use super::dispatch::resolution::ResolutionAction::*;
    let (_dir, s, i) = setup();
    let (epoch, r, _) = claimed_intent(&s, &i);
    let key = [8; 32];
    s.mark_dispatch_unknown(&r.dispatch_id, &epoch, "ACK_LOST")
        .unwrap();
    assert!(s
        .resolution_preview(&i.principal_key, &r.dispatch_id, AttachRun, &key)
        .is_err());
    assert!(s
        .resolution_preview(&i.principal_key, &r.dispatch_id, ConfirmNotSent, &key)
        .is_err());
    let mut g = resolution_grant(&s, &i, &r.dispatch_id, KeepUnknown, &key);
    let rev = g.preview.dispatch_revision;
    let client = id();
    g.principal_key = "f".repeat(64);
    assert!(s.commit_resolution(&g, &client, &key).is_err());
    g.principal_key = i.principal_key.clone();
    let context = g.preview.context_id.clone();
    g.preview.context_id = Some(id());
    assert!(s.commit_resolution(&g, &client, &key).is_err());
    g.preview.context_id = context;
    s.with_connection(|c|c.execute_batch("CREATE TRIGGER reject_resolution BEFORE INSERT ON operator_resolutions BEGIN SELECT RAISE(ABORT,'fixture'); END;").map_err(db_error)).unwrap();
    assert!(s.commit_resolution(&g, &client, &key).is_err());
    assert_eq!(s.dispatch_record(&r.dispatch_id).unwrap().revision, rev);
    s.with_connection(|c| {
        c.execute_batch("DROP TRIGGER reject_resolution;")
            .map_err(db_error)
    })
    .unwrap();
    let outcome = s.commit_resolution(&g, &client, &key).unwrap();
    assert!(outcome.lock_held);
    assert_eq!(outcome.outcome, "KEPT_UNKNOWN");
    g.expires_at = 0;
    assert_eq!(
        s.commit_resolution(&g, &client, &key)
            .unwrap()
            .resolution_id,
        outcome.resolution_id
    );
    assert!(s.commit_resolution(&g, &id(), &key).is_err());
    g.preview.dispatch_revision += 1;
    assert_eq!(
        s.commit_resolution(&g, &client, &key).unwrap_err(),
        "IDEMPOTENCY_CONFLICT"
    );
    assert_eq!(count(&s, "operator_resolutions"), 1);
    assert!(s.prepare_instruction(&i, &id(), 1, 0, "retry").is_err());
}
#[test]
fn unknown_ack_attachment_retains_original_run_and_lock() {
    use super::dispatch::resolution::ResolutionAction::*;
    let (_dir, s, i) = setup();
    let (epoch, r, _) = claimed_intent(&s, &i);
    let key = [8; 32];
    s.mark_dispatch_unknown(&r.dispatch_id, &epoch, "ACK_LOST")
        .unwrap();
    let stale = resolution_grant(&s, &i, &r.dispatch_id, KeepUnknown, &key);
    s.record_native_ack(&r.dispatch_id, &epoch, &native_ack(&epoch), &key)
        .unwrap();
    assert_eq!(
        s.commit_resolution(&stale, &id(), &key).unwrap_err(),
        "STALE_RESOLUTION"
    );
    let g = resolution_grant(&s, &i, &r.dispatch_id, AttachRun, &key);
    let outcome = s.commit_resolution(&g, &id(), &key).unwrap();
    assert!(outcome.lock_held);
    assert_eq!(outcome.run_id, r.run_id);
    assert_eq!(count(&s, "provider_runs"), 1);
    let state=s.with_connection(|c|c.query_row("SELECT p.status,p.external_run_id,h.status,p.terminal_at FROM provider_runs p JOIN handoffs h ON h.id=p.origin_handoff_id WHERE p.id=?1",params![r.run_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<i64>>(3)?))).map_err(db_error)).unwrap();
    assert_eq!(
        state,
        ("UNKNOWN".into(), "exact-turn".into(), "SENT".into(), None)
    );
}
#[test]
fn unknown_proven_terminal_requires_operator_then_releases_lock_atomically() {
    use super::dispatch::resolution::ResolutionAction::*;
    use crate::codex::result::LiveResult;
    use serde_json::json;
    let (_dir, s, i) = setup();
    let (epoch, r, _) = claimed_intent(&s, &i);
    let key = [8; 32];
    s.mark_dispatch_unknown(&r.dispatch_id, &epoch, "ACK_LOST")
        .unwrap();
    s.record_native_ack(&r.dispatch_id, &epoch, &native_ack(&epoch), &key)
        .unwrap();
    let mut live = LiveResult::new("exact-thread".into());
    live.associate_ack("exact-thread", "exact-turn").unwrap();
    live.observe(&json!({"method":"turn/completed","params":{"threadId":"exact-thread","turn":{"id":"exact-turn","status":"failed","items":[]}}})).unwrap();
    s.persist_live_terminal(&r.dispatch_id, &epoch, &live.terminal().unwrap(), 2, &key)
        .unwrap();
    assert_eq!(s.dispatch_record(&r.dispatch_id).unwrap().phase, "UNKNOWN");
    let g = resolution_grant(&s, &i, &r.dispatch_id, AttachRun, &key);
    assert!(!s.commit_resolution(&g, &id(), &key).unwrap().lock_held);
    assert_eq!(s.dispatch_record(&r.dispatch_id).unwrap().phase, "TERMINAL");
    assert!(s
        .prepare_instruction(&i, &id(), 1, 0, "fresh approval needed")
        .is_ok());
}
#[test]
fn only_sealed_prewrite_proof_allows_not_sent_and_never_after_write_intent() {
    use super::dispatch::resolution::ResolutionAction::*;
    use crate::codex::adapter::SealedPrewrite;
    let (_dir, s, i) = setup();
    let d = draft(&s, &i);
    let g = grant(&s, &i, &d);
    let epoch = id();
    let key = [8; 32];
    let r = s.approve_and_claim(&g, &epoch).unwrap();
    s.begin_dispatch_preparation(&r.dispatch_id, &epoch)
        .unwrap();
    s.mark_dispatch_unknown(&r.dispatch_id, &epoch, "PREPARATION_FAILED")
        .unwrap();
    assert!(s
        .resolution_preview(&i.principal_key, &r.dispatch_id, ConfirmNotSent, &key)
        .is_err());
    s.record_prewrite_stop(
        &r.dispatch_id,
        &SealedPrewrite {
            adapter_epoch: epoch.clone(),
        },
        &key,
    )
    .unwrap();
    let g = resolution_grant(&s, &i, &r.dispatch_id, ConfirmNotSent, &key);
    let result = s.commit_resolution(&g, &id(), &key).unwrap();
    assert!(!result.lock_held);
    assert_eq!(result.outcome, "NOT_SENT");
    assert_eq!(s.dispatch_record(&r.dispatch_id).unwrap().phase, "NOT_SENT");
    let (_dir, s, i) = setup();
    let (epoch, r, _) = claimed_intent(&s, &i);
    assert!(s
        .record_prewrite_stop(
            &r.dispatch_id,
            &SealedPrewrite {
                adapter_epoch: epoch
            },
            &key
        )
        .is_err());
    assert_eq!(count(&s, "dispatch_native_evidence"), 0);
}

#[test]
fn contradictory_native_ack_is_durably_frozen_and_operator_cannot_choose_it() {
    use super::dispatch::resolution::ResolutionAction::*;
    let (_dir, s, i) = setup();
    let (epoch, r, _) = claimed_intent(&s, &i);
    let key = [8; 32];
    s.record_native_ack(&r.dispatch_id, &epoch, &native_ack(&epoch), &key)
        .unwrap();
    let mut changed = native_ack(&epoch);
    changed.turn_id = "different-turn".into();
    assert!(s
        .record_native_ack(&r.dispatch_id, &epoch, &changed, &key)
        .is_err());
    assert_eq!(s.dispatch_record(&r.dispatch_id).unwrap().phase, "UNKNOWN");
    assert_eq!(count(&s, "dispatch_native_evidence"), 2);
    assert!(s
        .resolution_preview(&i.principal_key, &r.dispatch_id, AttachRun, &key)
        .is_err());
    assert!(s
        .resolution_preview(&i.principal_key, &r.dispatch_id, KeepUnknown, &key)
        .is_ok());
}

#[test]
fn direct_paused_product_coordinator_requires_two_sessions_and_stops_after_observation_failure() {
    use super::control::binding::*;
    use crate::application::{
        native::{tests::*, *},
        roots::RootIdentity,
    };
    use serde_json::json;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };
    struct Factory {
        sessions: VecDeque<FixtureSession>,
        opens: usize,
    }
    impl NativeFactory for Factory {
        fn open(&mut self) -> Result<Box<dyn NativeSession>, String> {
            self.opens += 1;
            Ok(Box::new(self.sessions.pop_front().ok_or("NO_SESSION")?))
        }
    }
    for fail in [false, true] {
        let (dir, s, i) = setup();
        let root = dir.path().join("registered-project");
        std::fs::create_dir(&root).unwrap();
        let mut policy = s
            .with_connection(|c| {
                c.query_row(
                    "SELECT policy_json FROM project_execution_roots WHERE project_id='p'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .map_err(db_error)
            })
            .map(|raw| serde_json::from_str::<PolicySnapshot>(&raw).unwrap())
            .unwrap();
        policy.root_identity_hash = RootIdentity::inspect(&root).unwrap().hash;
        let contract = NativeContract {
            canonical_path: root.to_str().unwrap().into(),
            root_revision: 1,
            policy: policy.clone(),
            model_provider: "fixture-provider".into(),
        };
        s.with_connection(|c|{c.execute("UPDATE project_execution_roots SET canonical_path=?1,path_identity_hash=?2,policy_json=?3,policy_hash=?4 WHERE project_id='p'",params![contract.canonical_path,policy.root_identity_hash,serde_json::to_string(&policy).unwrap(),policy.hash().unwrap()]).map_err(db_error)?;Ok(())}).unwrap();
        let next = ScopeIdentity {
            scope_key: "f".repeat(64),
            ..i
        };
        let review = s
            .prepare_binding(
                &next,
                &BindingProposal {
                    client_request_id: id(),
                    operation: BindingOperation::Create,
                    expected_context_revision: 0,
                    project_id: Some("p".into()),
                    candidate_thread_id: None,
                    workstream_id: None,
                },
            )
            .unwrap();
        let grant = BindingGrant {
            principal_key: next.principal_key.clone(),
            review,
            nonce_hash: "e".repeat(64),
        };
        let thread = id();
        let response = response(&contract, &thread);
        let paused = json!({"goal":{"threadId":thread,"objective":INITIAL_OBJECTIVE,"status":"paused","tokenBudget":null}});
        let first_trace = Arc::new(Mutex::new(Observation::default()));
        let fresh_trace = Arc::new(Mutex::new(Observation::default()));
        let first = FixtureSession {
            trace: first_trace.clone(),
            fail_window: if fail { Some(1) } else { None },
            script: VecDeque::from(vec![
                ("thread/start", response.clone()),
                ("thread/goal/get", json!({"goal":null})),
                ("thread/goal/set", paused.clone()),
                ("thread/goal/get", paused.clone()),
                ("thread/goal/clear", json!({"cleared":true})),
                ("thread/goal/get", json!({"goal":null})),
            ]),
        };
        let fresh = FixtureSession {
            trace: fresh_trace.clone(),
            fail_window: None,
            script: VecDeque::from(vec![
                ("thread/read", response.clone()),
                ("thread/resume", response.clone()),
                ("thread/goal/get", json!({"goal":null})),
            ]),
        };
        let mut factory = Factory {
            sessions: VecDeque::from(vec![first, fresh]),
            opens: 0,
        };
        let result = create_direct_paused(&s, &grant, &contract, &mut factory);
        if fail {
            assert!(result.is_err());
            assert_eq!(factory.opens, 1);
            assert_eq!(first_trace.lock().unwrap().calls.len(), 3);
            assert_eq!(first_trace.lock().unwrap().cleanups, 1);
            assert_eq!(
                s.binding_review(&next.principal_key, &grant.review.id)
                    .unwrap()
                    .state,
                "UNKNOWN"
            );
            let recovery_first_trace = Arc::new(Mutex::new(Observation::default()));
            let recovery_fresh_trace = Arc::new(Mutex::new(Observation::default()));
            let recovery_first = FixtureSession {
                trace: recovery_first_trace.clone(),
                fail_window: None,
                script: VecDeque::from(vec![
                    ("thread/read", response.clone()),
                    ("thread/resume", response.clone()),
                    ("thread/goal/get", json!({"goal":null})),
                    ("thread/goal/set", paused.clone()),
                    ("thread/goal/get", paused.clone()),
                    ("thread/goal/clear", json!({"cleared":true})),
                    ("thread/goal/get", json!({"goal":null})),
                ]),
            };
            let recovery_fresh = FixtureSession {
                trace: recovery_fresh_trace.clone(),
                fail_window: None,
                script: VecDeque::from(vec![
                    ("thread/read", response.clone()),
                    ("thread/resume", response.clone()),
                    ("thread/goal/get", json!({"goal":null})),
                ]),
            };
            let mut recovery = Factory {
                sessions: VecDeque::from(vec![recovery_first, recovery_fresh]),
                opens: 0,
            };
            let recovery_grant = BindingGrant {
                nonce_hash: "f".repeat(64),
                ..grant.clone()
            };
            let target =
                recover_direct_paused(&s, &recovery_grant, &contract, &mut recovery).unwrap();
            assert_eq!(target.thread_id, thread);
            assert_eq!(recovery.opens, 2);
            assert!(recovery_first_trace
                .lock()
                .unwrap()
                .calls
                .iter()
                .all(|(method, _)| method != "thread/start"));
            assert_eq!(
                s.binding_review(&next.principal_key, &grant.review.id)
                    .unwrap()
                    .state,
                "READY"
            );
        } else {
            let target = result.unwrap();
            assert_eq!(factory.opens, 2);
            assert_eq!(target.thread_id, thread);
            assert_eq!(first_trace.lock().unwrap().windows, 2);
            assert_eq!(fresh_trace.lock().unwrap().windows, 1);
            assert_eq!(
                s.apply_binding(&grant, Some(&target)).unwrap().state,
                "BOUND"
            );
        }
        let before = factory.opens;
        assert!(create_direct_paused(&s, &grant, &contract, &mut factory).is_err());
        assert_eq!(factory.opens, before);
        let t = first_trace.lock().unwrap();
        let sent = &t.calls[2].1;
        assert_eq!(sent["status"], "paused");
        assert!(sent["tokenBudget"].is_null());
        assert!(!t.calls.iter().any(|(m, _)| m == "turn/start"));
        assert_eq!(count(&s, "provider_runs"), 0);
    }
}

#[test]
fn full_dispatch_driver_short_result_never_fetches_large_old_history_or_retries_lost_ack() {
    use crate::application::{dispatch::*, native::*, roots::RootIdentity};
    use crate::codex::adapter::{CandidateCleanup, NativeTurnAck, SealedPrewrite};
    use serde_json::{json, Value};
    use std::{collections::VecDeque, time::Duration};
    struct Native<'a> {
        store: &'a RouterStore,
        dispatch: String,
        epoch: String,
        contract: NativeContract,
        events: VecDeque<Value>,
        old_history: std::fs::File,
        history_rpc: usize,
        writes: usize,
        ack_lost: bool,
        active_goal: bool,
        closed: bool,
    }
    impl NativeSession for Native<'_> {
        fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
            if method == "thread/read" && params["includeTurns"] == true
                || matches!(method, "thread/turns/list" | "thread/items/list")
            {
                self.history_rpc += 1;
                assert_eq!(
                    self.old_history.metadata().unwrap().len(),
                    100 * 1024 * 1024
                );
                return Err("FULL_HISTORY_OVERSIZE".into());
            }
            match method {
                "thread/goal/get" => Ok(if self.active_goal {
                    json!({"goal":{"threadId":"exact-thread","status":"active"}})
                } else {
                    json!({"goal":null})
                }),
                "thread/read" => {
                    assert_eq!(params["includeTurns"], false);
                    Ok(crate::application::native::tests::response(
                        &self.contract,
                        "exact-thread",
                    ))
                }
                "thread/resume" => {
                    assert_eq!(params["excludeTurns"], true);
                    Ok(crate::application::native::tests::response(
                        &self.contract,
                        "exact-thread",
                    ))
                }
                "config/read" => Ok(
                    json!({"config":{"sandbox_workspace_write":{"exclude_tmpdir_env_var":true,"exclude_slash_tmp":true,"writable_roots":[self.contract.canonical_path]}}}),
                ),
                _ => Err("UNEXPECTED_RPC".into()),
            }
        }
        fn settle(&mut self, _: Duration) -> Result<(), String> {
            Ok(())
        }
        fn close(&mut self) -> Result<(), String> {
            self.closed = true;
            Ok(())
        }
        fn cleanup_candidate(&mut self) -> CandidateCleanup {
            CandidateCleanup::default()
        }
    }
    impl DispatchIo for Native<'_> {
        fn epoch(&self) -> &str {
            &self.epoch
        }
        fn authorize(&mut self, t: &str) -> Result<(), String> {
            assert_eq!(t, "exact-thread");
            Ok(())
        }
        fn write_turn(
            &mut self,
            p: Value,
            gate: &mut dyn FnMut(u64, &Value) -> Result<(), String>,
        ) -> Result<NativeTurnAck, String> {
            gate(9, &p)?;
            assert_eq!(
                self.store.dispatch_record(&self.dispatch).unwrap().phase,
                "WRITE_INTENT"
            );
            assert_eq!(p["input"][0]["text"], "  exact\ntext  ");
            assert_eq!(p["input"][0]["text_elements"], json!([]));
            self.writes += 1;
            // These events are queued before the ACK decoder returns.
            self.events.push_back(json!({"method":"item/completed","params":{"threadId":"exact-thread","turnId":"exact-turn","item":{"type":"agentMessage","id":"final","phase":"final_answer","text":"x".repeat(128)}}}));
            self.events.push_back(json!({"method":"turn/completed","params":{"threadId":"exact-thread","turn":{"id":"exact-turn","status":"completed","itemsView":"notLoaded","items":[]}}}));
            if self.ack_lost {
                return Err("ACK_LOST".into());
            }
            Ok(native_ack(&self.epoch))
        }
        fn next(&mut self) -> Result<Option<(i64, Value)>, String> {
            Ok(self.events.pop_front().map(|e| (1, e)))
        }
        fn seal_prewrite(&mut self, epoch: &str) -> Result<SealedPrewrite, String> {
            self.close()?;
            if self.writes > 0 {
                return Err("NOT_PREWRITE".into());
            }
            Ok(SealedPrewrite {
                adapter_epoch: epoch.into(),
            })
        }
    }
    for mode in ["normal", "lost_ack", "active_goal", "audit_failure"] {
        let (dir, s, i) = setup();
        let root = dir.path().join("project");
        std::fs::create_dir(&root).unwrap();
        let mut policy = s
            .with_connection(|c| {
                c.query_row(
                    "SELECT policy_json FROM project_execution_roots WHERE project_id='p'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .map_err(db_error)
            })
            .map(|raw| serde_json::from_str::<PolicySnapshot>(&raw).unwrap())
            .unwrap();
        policy.root_identity_hash = RootIdentity::inspect(&root).unwrap().hash;
        let contract = NativeContract {
            canonical_path: root.to_str().unwrap().into(),
            root_revision: 1,
            policy: policy.clone(),
            model_provider: "fixture-provider".into(),
        };
        s.with_connection(|c|{c.execute("UPDATE project_execution_roots SET canonical_path=?1,path_identity_hash=?2,policy_json=?3,policy_hash=?4 WHERE project_id='p'",params![contract.canonical_path,policy.root_identity_hash,serde_json::to_string(&policy).unwrap(),policy.hash().unwrap()]).map_err(db_error)?;Ok(())}).unwrap();
        let d = draft(&s, &i);
        let g = grant(&s, &i, &d);
        let epoch = id();
        let run = s.approve_and_claim(&g, &epoch).unwrap();
        let archive = std::fs::File::create(dir.path().join("synthetic-old-history")).unwrap();
        archive.set_len(100 * 1024 * 1024).unwrap();
        let mut native = Native {
            store: &s,
            dispatch: run.dispatch_id.clone(),
            epoch: epoch.clone(),
            contract,
            events: VecDeque::new(),
            old_history: archive,
            history_rpc: 0,
            writes: 0,
            ack_lost: mode == "lost_ack",
            active_goal: mode == "active_goal",
            closed: false,
        };
        if mode == "audit_failure" {
            s.with_connection(|c|c.execute_batch("CREATE TRIGGER reject_dispatch BEFORE UPDATE ON handoff_dispatches WHEN NEW.phase='WRITE_INTENT' BEGIN SELECT RAISE(ABORT,'fixture'); END;").map_err(db_error)).unwrap();
        }
        let started = start_claimed(&s, &run, &mut native, "fixture-provider", &[8; 32]);
        if mode == "normal" {
            let mut live = started.unwrap();
            assert!(matches!(
                live.poll(&s, &mut native, &[8; 32]).unwrap(),
                DispatchEvent::None
            ));
            assert!(matches!(
                live.poll(&s, &mut native, &[8; 32]).unwrap(),
                DispatchEvent::Terminal
            ));
            assert_eq!(
                s.dispatch_record(&run.dispatch_id).unwrap().phase,
                "TERMINAL"
            );
            let text = s
                .with_connection(|c| {
                    c.query_row(
                        "SELECT result_text FROM provider_runs WHERE id=?1",
                        params![run.run_id],
                        |r| r.get::<_, String>(0),
                    )
                    .map_err(db_error)
                })
                .unwrap();
            assert_eq!(text, "x".repeat(128));
        } else {
            assert!(started.is_err());
            assert_eq!(
                s.dispatch_record(&run.dispatch_id).unwrap().phase,
                "UNKNOWN"
            );
            if mode == "active_goal" {
                assert_eq!(
                    s.with_connection(|c| {
                        c.query_row(
                            "SELECT last_code FROM handoff_dispatches WHERE dispatch_id=?1",
                            params![run.dispatch_id],
                            |r| r.get::<_, Option<String>>(0),
                        )
                        .map_err(db_error)
                    })
                    .unwrap(),
                    Some("GOAL_ACTIVE".into())
                );
            }
        }
        assert_eq!(native.history_rpc, 0);
        assert_eq!(
            native.writes,
            usize::from(matches!(mode, "normal" | "lost_ack"))
        );
        let writes = native.writes;
        assert!(start_claimed(&s, &run, &mut native, "fixture-provider", &[8; 32]).is_err());
        assert_eq!(native.writes, writes);
    }
}

#[test]
fn quiesce_exit_timeout_database_failure_and_restart_never_unlock_unknown() {
    use super::dispatch::exit::ExitGrant;
    let (dir, s, i) = setup();
    let runtime = id();
    s.recover_preview_startup(&runtime).unwrap();
    let old = s.exit_preview().unwrap();
    let (epoch, run, _) = claimed_intent(&s, &i);
    s.mark_dispatch_unknown(&run.dispatch_id, &epoch, "ACK_LOST")
        .unwrap();
    let mut grant = ExitGrant {
        principal_key: i.principal_key.clone(),
        preview: old,
        nonce_hash: "e".repeat(64),
        expires_at: now() + 600_000,
    };
    assert_eq!(
        s.quiesce_preview_runtime(&grant).unwrap_err(),
        "STALE_RESOLUTION"
    );
    grant.preview = s.exit_preview().unwrap();
    assert_eq!(grant.preview.unresolved_count, 1);
    assert_eq!(s.quiesce_preview_runtime(&grant).unwrap(), "QUIESCED");
    assert_eq!(s.quiesce_preview_runtime(&grant).unwrap(), "QUIESCED");
    assert_eq!(
        s.finish_preview_exit(&runtime, false).unwrap(),
        "EXIT_PENDING"
    );
    assert_eq!(
        s.dispatch_record(&run.dispatch_id).unwrap().phase,
        "UNKNOWN"
    );
    s.with_connection(|c|c.execute_batch("CREATE TRIGGER reject_seal BEFORE UPDATE ON runtime_exit_state WHEN NEW.phase='SEALED' BEGIN SELECT RAISE(ABORT,'fixture'); END;").map_err(db_error)).unwrap();
    assert!(s.finish_preview_exit(&runtime, true).is_err());
    s.with_connection(|c| {
        c.execute_batch("DROP TRIGGER reject_seal;")
            .map_err(db_error)
    })
    .unwrap();
    assert_eq!(s.finish_preview_exit(&runtime, true).unwrap(), "SEALED");
    drop(s);
    let s =
        RouterStore::open_preview(PreviewProfile::acquire(dir.path().join("mcp-preview")).unwrap())
            .unwrap();
    s.recover_preview_startup(&id()).unwrap();
    assert_eq!(
        s.dispatch_record(&run.dispatch_id).unwrap().phase,
        "UNKNOWN"
    );
    assert!(s
        .prepare_instruction(&i, &id(), 1, 0, "retry after restart")
        .is_err());
    assert!(s.quiesce_preview_runtime(&grant).is_err());
    assert_eq!(count(&s, "provider_runs"), 1);
}

#[test]
fn scope_views_and_result_pages_preserve_utf8_exact_identity_without_marking_read() {
    use crate::codex::result::LiveResult;
    use serde_json::json;
    let (_dir, s, i) = setup();
    let (epoch, r, _) = claimed_intent(&s, &i);
    let key = [8; 32];
    s.record_native_ack(&r.dispatch_id, &epoch, &native_ack(&epoch), &key)
        .unwrap();
    let text = format!("{}中{}", "x".repeat(1023), "尾".repeat(500));
    let mut live = LiveResult::new("exact-thread".into());
    live.associate_ack("exact-thread", "exact-turn").unwrap();
    live.observe(&json!({"method":"turn/completed","params":{"threadId":"exact-thread","turn":{"id":"exact-turn","status":"completed","itemsView":"summary","items":[{"type":"agentMessage","id":"final","phase":"final_answer","text":text}]}}})).unwrap();
    s.persist_live_terminal(&r.dispatch_id, &epoch, &live.terminal().unwrap(), 1, &key)
        .unwrap();
    let view = s.context_view(&i).unwrap();
    assert_eq!(view.target.unwrap().thread_id, "exact-thread");
    assert_eq!(
        s.scoped_run(&i, &r.run_id).unwrap().turn_id.as_deref(),
        Some("exact-turn")
    );
    let other = ScopeIdentity {
        scope_key: "f".repeat(64),
        ..i.clone()
    };
    assert_eq!(s.scoped_run(&other, &r.run_id).unwrap_err(), "NOT_FOUND");
    assert!(s
        .scoped_result_chunk(&other, &r.run_id, 0, 1024, None)
        .is_err());
    let first = s.scoped_result_chunk(&i, &r.run_id, 0, 1024, None).unwrap();
    assert_eq!(first.text.len(), 1023);
    assert_eq!(first.next_offset, Some(1023));
    let mut joined = first.text;
    let mut offset = first.next_offset;
    while let Some(next) = offset {
        let page = s
            .scoped_result_chunk(&i, &r.run_id, next, 1024, Some(&first.result_hash))
            .unwrap();
        assert_eq!(page.thread_id, "exact-thread");
        assert_eq!(page.turn_id, "exact-turn");
        assert_eq!(page.result_identity, "final");
        joined.push_str(&page.text);
        offset = page.next_offset;
    }
    assert_eq!(joined, text);
    assert!(s
        .scoped_result_chunk(&i, &r.run_id, 1024, 1024, None)
        .is_err());
    assert!(s
        .scoped_result_chunk(&i, &r.run_id, 0, 1024, Some(&"f".repeat(64)))
        .is_err());
    let reviewed = s
        .with_connection(|c| {
            c.query_row(
                "SELECT reviewed_at FROM provider_runs WHERE id=?1",
                params![r.run_id],
                |r| r.get::<_, Option<i64>>(0),
            )
            .map_err(db_error)
        })
        .unwrap();
    assert!(reviewed.is_none());
}

#[test]
fn selection_preview_is_read_only_and_confirm_rolls_back_on_stale_target() {
    use super::control::binding::*;
    let (_dir, s, i) = setup();
    let original = id();
    let chosen = id();
    let r = s
        .prepare_binding(
            &i,
            &BindingProposal {
                client_request_id: id(),
                operation: BindingOperation::Replace,
                expected_context_revision: 1,
                project_id: Some("p".into()),
                candidate_thread_id: Some(original.clone()),
                workstream_id: None,
            },
        )
        .unwrap();
    let selected = s
        .binding_selection_preview(&i.principal_key, &r.id, Some(&chosen), None)
        .unwrap();
    assert_eq!(s.binding_review(&i.principal_key, &r.id).unwrap(), r);
    assert!(s
        .binding_selection_preview(&"f".repeat(64), &r.id, Some(&chosen), None)
        .is_err());
    let g = BindingGrant {
        principal_key: i.principal_key.clone(),
        review: selected.clone(),
        nonce_hash: "f".repeat(64),
    };
    let bad = VerifiedBindingTarget {
        thread_id: original,
        policy_hash: selected.policy_hash.clone().unwrap(),
        root_revision: 1,
        durable: true,
    };
    assert!(s.apply_binding(&g, Some(&bad)).is_err());
    assert_eq!(s.binding_review(&i.principal_key, &r.id).unwrap(), r);
    let good = VerifiedBindingTarget {
        thread_id: chosen.clone(),
        ..bad
    };
    s.apply_binding(&g, Some(&good)).unwrap();
    assert_eq!(
        s.binding_review(&i.principal_key, &r.id)
            .unwrap()
            .candidate_thread_id,
        Some(chosen)
    );
    assert_eq!(s.apply_binding(&g, Some(&good)).unwrap().revision, 2);
    let old = BindingGrant {
        review: r,
        nonce_hash: "e".repeat(64),
        ..g
    };
    assert!(s.apply_binding(&old, Some(&good)).is_err());
}
#[test]
fn browser_selected_takeover_checks_owner_revisions_and_preserves_unknown_lock() {
    use super::control::binding::*;
    let (_dir, s, i) = setup();
    let next = ScopeIdentity {
        scope_key: "f".repeat(64),
        ..i.clone()
    };
    let r = s
        .prepare_binding(
            &next,
            &BindingProposal {
                client_request_id: id(),
                operation: BindingOperation::Select,
                expected_context_revision: 0,
                project_id: Some("p".into()),
                candidate_thread_id: Some(id()),
                workstream_id: None,
            },
        )
        .unwrap();
    let rows = s
        .binding_owner_workstreams(&i.principal_key, &r.id, "")
        .unwrap();
    assert_eq!(rows.len(), 1);
    let selected = s
        .binding_selection_preview(&i.principal_key, &r.id, None, Some("w"))
        .unwrap();
    assert_eq!(selected.operation, BindingOperation::Takeover);
    let g = BindingGrant {
        principal_key: i.principal_key.clone(),
        review: selected.clone(),
        nonce_hash: "e".repeat(64),
    };
    let target = VerifiedBindingTarget {
        thread_id: "exact-thread".into(),
        policy_hash: selected.policy_hash.clone().unwrap(),
        root_revision: 1,
        durable: true,
    };
    s.with_connection(|c| {
        c.execute(
            "UPDATE workstreams SET binding_revision=binding_revision+1 WHERE id='w'",
            [],
        )
        .map_err(db_error)?;
        Ok(())
    })
    .unwrap();
    assert!(s.apply_binding(&g, Some(&target)).is_err());
    assert_eq!(s.binding_review(&i.principal_key, &r.id).unwrap(), r);
    let g = BindingGrant {
        review: s
            .binding_selection_preview(&i.principal_key, &r.id, None, Some("w"))
            .unwrap(),
        ..g
    };
    // A real unresolved Store claim prevents takeover even with an earlier preview.
    let d = s.prepare_instruction(&i, &id(), 1, 1, "fixture").unwrap();
    let a = grant(&s, &i, &d);
    s.approve_and_claim(&a, &id()).unwrap();
    assert!(s.apply_binding(&g, Some(&target)).is_err());
    assert_eq!(s.control_context(&i).unwrap().unwrap().state, "BOUND");
    assert_eq!(s.binding_review(&i.principal_key, &r.id).unwrap(), r);
}

#[test]
fn draft_and_binding_budgets_reject_growth_but_keep_idempotent_reads() {
    use super::control::binding::*;
    let (_dir, s, i) = setup();
    let request = id();
    let first = s
        .prepare_instruction(&i, &request, 1, 0, "fixture")
        .unwrap();
    for _ in 1..20 {
        s.prepare_instruction(&i, &id(), 1, 0, "fixture").unwrap();
    }
    assert_eq!(
        s.prepare_instruction(&i, &id(), 1, 0, "fixture")
            .unwrap_err(),
        "RATE_LIMITED"
    );
    assert_eq!(
        s.prepare_instruction(&i, &request, 1, 0, "fixture")
            .unwrap()
            .handoff_id,
        first.handoff_id
    );
    let proposal = BindingProposal {
        client_request_id: id(),
        operation: BindingOperation::Replace,
        expected_context_revision: 1,
        project_id: Some("p".into()),
        candidate_thread_id: Some(id()),
        workstream_id: None,
    };
    let first = s.prepare_binding(&i, &proposal).unwrap();
    for _ in 1..20 {
        s.prepare_binding(
            &i,
            &BindingProposal {
                client_request_id: id(),
                operation: BindingOperation::Replace,
                expected_context_revision: 1,
                project_id: Some("p".into()),
                candidate_thread_id: Some(id()),
                workstream_id: None,
            },
        )
        .unwrap();
    }
    assert_eq!(
        s.prepare_binding(
            &i,
            &BindingProposal {
                client_request_id: id(),
                operation: BindingOperation::Replace,
                expected_context_revision: 1,
                project_id: Some("p".into()),
                candidate_thread_id: Some(id()),
                workstream_id: None
            }
        )
        .unwrap_err(),
        "RATE_LIMITED"
    );
    assert_eq!(s.prepare_binding(&i, &proposal).unwrap().id, first.id);
    assert!(s.preview_database_bytes().unwrap() > 0);
    assert_eq!(count(&s, "provider_runs"), 0);
}
#[test]
fn owner_ready_budget_counts_other_contexts_without_deleting_history() {
    let (_dir, s, i) = setup();
    let mut other = i.clone();
    other.scope_key = "f".repeat(64);
    let ctx = s.ensure_unbound_context(&other).unwrap();
    s.with_connection(|c| {
        for _ in 0..100 {c.execute("INSERT INTO handoffs(id,workstream_id,source_kind,source_context_id,destination_endpoint_id,direction,original_text,approved_text,status,payload_hash,created_at) VALUES(?1,'w','CONTROL_CONTEXT',?2,'e','CONTROL_TO_CODEX','fixture','fixture','READY',?3,1)",params![id(),ctx.id,"a".repeat(64)]).map_err(db_error)?;}
        Ok(())
    }).unwrap();
    assert_eq!(
        s.prepare_instruction(&i, &id(), 1, 0, "fixture")
            .unwrap_err(),
        "RATE_LIMITED"
    );
    assert_eq!(count(&s, "handoffs"), 100);
    assert_eq!(count(&s, "provider_runs"), 0);
}

#[test]
fn authenticated_chatgpt_host_scope_survives_owner_rotation_and_isolates_sessions() {
    let (_dir, store, _) = setup();
    let owner_before = "a".repeat(64);
    let owner_after = "b".repeat(64);
    let first = store
        .resolve_mcp_scope(
            &owner_before,
            &serde_json::json!({"openai/subject":"account-one","openai/session":"management-one"}),
        )
        .unwrap();
    assert_eq!(first.key_version, 2);
    assert_ne!(first.scope_key, "management-one");
    let created = store.ensure_unbound_context(&first).unwrap();
    store
        .rotate_private_owner_epoch(&owner_before, &owner_after)
        .unwrap();
    let after_rotation = store
        .resolve_mcp_scope(
            &owner_after,
            &serde_json::json!({"openai/subject":"account-one","openai/session":"management-one"}),
        )
        .unwrap();
    assert_eq!(after_rotation.scope_key, first.scope_key);
    assert_eq!(
        store.control_context(&after_rotation).unwrap().unwrap().id,
        created.id
    );
    let other_session = store
        .resolve_mcp_scope(
            &owner_after,
            &serde_json::json!({"openai/subject":"account-one","openai/session":"management-two"}),
        )
        .unwrap();
    assert_ne!(other_session.scope_key, first.scope_key);
    assert!(store.control_context(&other_session).unwrap().is_none());
}
