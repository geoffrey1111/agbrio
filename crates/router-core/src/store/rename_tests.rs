use super::*;

#[test]
fn rename_preserves_the_exact_bridge_binding_and_other_records() {
    let directory=tempfile::tempdir().unwrap();
    let store=RouterStore::open_at(directory.path().join("router.db")).unwrap();
    let project=store.create_project("test".into(),None).unwrap();
    let bridge=store.create_workstream(&project.id,"same title".into()).unwrap();
    let other=store.create_workstream(&project.id,"same title".into()).unwrap();
    store.bind_endpoint(&bridge.id,Provider::Codex,"native-exact".into(),"Codex".into(),false).unwrap();
    let before=store.snapshot_for_workstream(&bridge.id).unwrap();
    let counts=store.smoke_record_counts().unwrap();
    let renamed=store.rename_workstream(&bridge.id,"  新名称  ").unwrap();
    let after=store.snapshot_for_workstream(&bridge.id).unwrap();
    assert_eq!(renamed.id,bridge.id);assert_eq!(renamed.name,"新名称");
    assert_eq!(renamed.binding_revision,before.workstreams.iter().find(|item|item.id==bridge.id).unwrap().binding_revision);
    assert_eq!(before.active_codex_endpoint.unwrap().external_id,after.active_codex_endpoint.unwrap().external_id);
    assert_eq!(store.workstream_name(&other.id).unwrap(),"same title");
    let after_counts=store.smoke_record_counts().unwrap();
    assert_eq!(counts.handoffs,after_counts.handoffs);assert_eq!(counts.provider_runs,after_counts.provider_runs);assert_eq!(counts.endpoints,after_counts.endpoints);
    for invalid in [" ".to_string(),"a\nb".to_string(),"x".repeat(101)] {assert!(store.rename_workstream(&bridge.id,&invalid).is_err());}
    assert_eq!(store.workstream_name(&bridge.id).unwrap(),"新名称");
    assert!(store.rename_workstream("missing", "new name").is_err());
}
