//! Explicit process-isolated transport test only; absent from shipped builds.
use super::*;
pub(crate) fn active()->bool{std::env::var_os("AGBRIO_HOSTED_FIXTURE_ROOT").is_some()}
pub(crate) fn root()->Result<PathBuf,String>{
 let p=PathBuf::from(std::env::var_os("AGBRIO_HOSTED_FIXTURE_ROOT").ok_or("FIXTURE_NOT_SELECTED")?).canonicalize().map_err(|_|"FIXTURE_ROOT_INVALID")?;
 let data=router_core::platform::local_data_base()?.canonicalize().map_err(|_|"FIXTURE_PROFILE_NOT_ISOLATED")?;
 if !data.starts_with(&p)||!p.join("fixture-only").is_file(){return Err("FIXTURE_PROFILE_NOT_ISOLATED".into());}Ok(p)
}
pub(crate) fn client(builder:reqwest::blocking::ClientBuilder)->Result<reqwest::blocking::ClientBuilder,String>{
 if !active(){return Ok(builder);}let p=root()?;
 let cert=reqwest::Certificate::from_pem(&fs::read(p.join("cert.pem")).map_err(|_|"FIXTURE_CERT_MISSING")?).map_err(|_|"FIXTURE_CERT_INVALID")?;
 let mut b=builder.no_proxy().add_root_certificate(cert);
 for name in ["control.agbrio.localhost","ag-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.agbrio.localhost","ag-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.agbrio.localhost"]{b=b.resolve(name,"127.0.0.1:443".parse().unwrap());}Ok(b)
}
#[test]
#[ignore="explicit one-time local hosted service; isolated HOME and synthetic native provider only"]
fn native_hosted_transport(){
 use std::sync::{Arc,Mutex};
 let root=root().unwrap();let input:serde_json::Value=serde_json::from_slice(&fs::read(root.join("input.json")).unwrap()).unwrap();
 // Prove fixture TLS/DNS independently before any one-use request is sent.
 let health=client(reqwest::blocking::Client::builder().timeout(Duration::from_secs(10))).unwrap().build().unwrap().get("https://control.agbrio.localhost/health").send().expect("isolated fixture TLS/DNS preflight");
 assert!(health.status().is_success(),"isolated fixture health");
 let node=PathBuf::from(std::env::var_os("AGBRIO_FIXTURE_NODE").expect("explicit staged Node"));assert!(node.is_absolute()&&node.is_file());
 let source=PathBuf::from(env!("CARGO_MANIFEST_DIR"));
 let mut command=Command::new(&node);command.arg(source.parent().unwrap().join("tools/hosted-relay-fixture/native-provider.mjs")).arg(&root);
 let mut adapter=crate::codex::adapter::CodexAdapter::start_shared_validation(command,Arc::new(|_|{})).unwrap();adapter.initialize().unwrap();
 let store=Arc::new(crate::RouterStore::open_at(root.join("router.db")).unwrap());
 store.enable_codex_watch("fixture-chat","Hosted transport fixture",root.to_str().unwrap(),&router_core::store::codex_watch::WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some("fixture-seed".into()),item_id:Some("fixture-answer".into()),text:"Fixture ready".into()}).unwrap();
 let core=RouterCore{chatgpt:Arc::default(),store,session:Arc::new(Mutex::new(crate::Session{adapter:Some(adapter),..Default::default()})),completed_chatgpt_responses:Arc::default()};
 let host=HostRuntime::default();let auth=Arc::new(crate::web_auth::WebAuth::open(Some(root.join("web-auth.json"))).unwrap());*host.test_web_auth.lock().unwrap()=Some(auth.clone());
 host.ensure_mobile(core.clone(),crate::mobile_http::MobileHttpConfig{port:0,allowed_host:"127.0.0.1:0".into(),allowed_origin:"http://127.0.0.1:0".into(),access_issuer:String::new(),access_audience:String::new(),access_jwks_url:String::new(),static_dir:source.parent().unwrap().join("dist")});
 configure_assets(source.join("resources/hosted-relay"),node);
 let outcome=if input["trial"]==true{
  #[cfg(target_os="macos")] let id=trial_device_id().unwrap();
  #[cfg(not(target_os="macos"))] let id=hash_platform_uuid("12345678-1234-5678-9abc-123456789abc").unwrap();
  start_trial_for_device(&core,&host,id)
 }else{redeem(&core,&host,RedeemInput{code:input["code"].as_str().unwrap().into()})};
 if let Err(error)=outcome{host.shutdown(&core);panic!("fixture activation failed: {error}");}
 let pair=auth.issue().unwrap();let s=status(&host);
 fs::write(root.join("ready.json"),serde_json::to_vec(&serde_json::json!({"origin":s.origin,"expiresAt":s.expires_at,"kind":s.kind,"pairCode":pair.code})).unwrap()).unwrap();
 let began=std::time::Instant::now();let mut renewed=false;
 while !root.join("stop").is_file()&&began.elapsed()<Duration::from_secs(300){
  if !renewed&&root.join("renew.json").is_file(){
   let v:serde_json::Value=serde_json::from_slice(&fs::read(root.join("renew.json")).unwrap()).unwrap();
   let s=redeem(&core,&host,RedeemInput{code:v["code"].as_str().unwrap().into()}).unwrap();
   fs::write(root.join("renewed.json"),serde_json::to_vec(&serde_json::json!({"origin":s.origin,"expiresAt":s.expires_at,"kind":s.kind})).unwrap()).unwrap();renewed=true;
  }
  ensure(&host);std::thread::sleep(Duration::from_millis(200));
 }
 host.shutdown(&core);assert!(root.join("stop").is_file(),"fixture deadline");
 println!("\nHOSTED_TRANSPORT_NATIVE_PASS");
}
