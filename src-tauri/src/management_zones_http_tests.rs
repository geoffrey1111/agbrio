use super::*;
use std::sync::Arc;

#[test]
fn management_http_requires_owner_and_origin_and_reconciles_same_request(){
    let scratch=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/management-zones-fixture");
    std::fs::create_dir_all(&scratch).unwrap();let d=tempfile::tempdir_in(scratch).unwrap();
    std::fs::write(d.path().join("index.html"),"isolated fixture").unwrap();
    let store=Arc::new(crate::RouterStore::open_at(d.path().join("router.db")).unwrap());
    let manager=uuid::Uuid::new_v4().to_string();
    store.enable_codex_watch(&manager,"Manager",&d.path().to_string_lossy(),&router_core::store::codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()}).unwrap();
    let core=RouterCore{store:store.clone(),session:Arc::default(),};
    let auth=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());
    let token=auth.exchange(&auth.issue().unwrap().code,"management fixture",false).unwrap();let cookie=format!("{}={token}",crate::web_auth::COOKIE);
    let runtime=tokio::runtime::Runtime::new().unwrap();
    let config=MobileHttpConfig{port:0,allowed_host:"router.fixture.invalid".into(),allowed_origin:"https://router.fixture.invalid".into(),access_issuer:"https://unused.invalid".into(),access_audience:"unused".into(),access_jwks_url:"https://unused.invalid/keys".into(),static_dir:d.path().into()};
    let handle=runtime.block_on(start_with_web_auth(core,config,crate::HostRuntime::default(),auth)).unwrap();
    let url=format!("http://{}/v1/mobile/management-zones",handle.address);let client=reqwest::blocking::Client::new();
    let get=|cookie:&str|client.get(&url).header("host","router.fixture.invalid").header("cookie",cookie).send().unwrap();
    assert_eq!(get("").status(),StatusCode::UNAUTHORIZED);
    assert_eq!(get(&cookie).json::<serde_json::Value>().unwrap()["zones"],serde_json::json!([]));
    let body=serde_json::json!({"requestId":uuid::Uuid::new_v4().to_string(),"action":"CREATE","name":"One zone","managerId":manager});
    let post=|cookie:&str,origin:&str,body:&serde_json::Value|client.post(&url).header("host","router.fixture.invalid").header("cookie",cookie).header("origin",origin).json(body).send().unwrap();
    assert_eq!(post("","https://router.fixture.invalid",&body).status(),StatusCode::UNAUTHORIZED);
    assert_eq!(post(&cookie,"https://foreign.invalid",&body).status(),StatusCode::FORBIDDEN);
    let first=post(&cookie,"https://router.fixture.invalid",&body);assert_eq!(first.status(),StatusCode::OK);let first=first.json::<serde_json::Value>().unwrap();
    assert_eq!(post(&cookie,"https://router.fixture.invalid",&body).json::<serde_json::Value>().unwrap(),first);
    assert_eq!(store.management_zones().unwrap()["zones"].as_array().unwrap().len(),1);
    let mut conflict=body.clone();conflict["name"]=serde_json::json!("Different");
    assert_eq!(post(&cookie,"https://router.fixture.invalid",&conflict).json::<serde_json::Value>().unwrap()["error"],"MANAGEMENT_REQUEST_CONFLICT");
    let mut invalid=body;invalid["grantId"]=serde_json::json!("cannot self-grant");
    assert!(!post(&cookie,"https://router.fixture.invalid",&invalid).status().is_success());
    runtime.block_on(handle.shutdown());
}
