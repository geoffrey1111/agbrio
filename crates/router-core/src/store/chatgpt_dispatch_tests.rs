use super::*;
use std::sync::{Arc, Barrier};

fn route(store: &RouterStore) -> (String, String) {
    let project = store
        .create_project("ChatGPT writer fixture".into(), None)
        .unwrap();
    let workstream = store
        .create_workstream(&project.id, "exact route".into())
        .unwrap();
    let endpoint = store
        .bind_endpoint(
            &workstream.id,
            Provider::Chatgpt,
            "00000000-0000-4000-8000-000000000001".into(),
            "fixture".into(),
            false,
        )
        .unwrap();
    (workstream.id, endpoint.id)
}

#[test]
fn concurrent_http_and_ipc_claims_reserve_only_one_chatgpt_writer() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(RouterStore::open_at(directory.path().join("router.db")).unwrap());
    let (workstream, endpoint) = route(&store);
    let barrier = Arc::new(Barrier::new(3));
    let workers = (0..2)
        .map(|_| {
            let (store, barrier, workstream, endpoint) = (
                Arc::clone(&store),
                Arc::clone(&barrier),
                workstream.clone(),
                endpoint.clone(),
            );
            std::thread::spawn(move || {
                barrier.wait();
                store.create_chatgpt_dispatch_run(&workstream, &endpoint, None)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let results = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results.into_iter().find_map(Result::err).unwrap(),
        "CHATGPT_EXISTING_DELIVERY_PENDING_OR_UNKNOWN"
    );
    assert_eq!(
        store
            .provider_runs_for_workstream(&workstream)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn unknown_chatgpt_delivery_excludes_another_write_after_store_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("router.db");
    let store = RouterStore::open_at(&path).unwrap();
    let (workstream, endpoint) = route(&store);
    let original = store
        .create_chatgpt_dispatch_run(&workstream, &endpoint, None)
        .unwrap();
    store
        .reconcile_provider_run(&original.id, "UNKNOWN", "CHATGPT_ACCEPTANCE_UNPROVEN")
        .unwrap();
    drop(store);
    let reopened = RouterStore::open_at(path).unwrap();
    assert_eq!(
        reopened
            .create_chatgpt_dispatch_run(&workstream, &endpoint, None)
            .unwrap_err(),
        "CHATGPT_EXISTING_DELIVERY_PENDING_OR_UNKNOWN"
    );
    let runs = reopened.provider_runs_for_workstream(&workstream).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, original.id);
    assert_eq!(runs[0].status, "UNKNOWN");
}
