//! Discovery uses this instance's validated listener/configuration, never a
//! product-wide hostname, caller-supplied Host header or credential-bearing URL.
pub(crate) fn endpoint(live_origin:Option<&str>,saved_origin:Option<&str>)->Option<String>{
 let origin=live_origin.or(saved_origin)?;
 let parsed=url::Url::parse(origin).ok()?;
 if parsed.scheme()!="https"||parsed.host_str().is_none()||!parsed.username().is_empty()||parsed.password().is_some()||parsed.path()!="/"||parsed.query().is_some()||parsed.fragment().is_some(){return None;}
 Some(format!("{}/mcp",parsed.origin().ascii_serialization()))
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn actual_listener_wins_over_missing_or_stale_configuration(){
  assert_eq!(endpoint(Some("https://ag-owner.fixture.invalid"),None),Some("https://ag-owner.fixture.invalid/mcp".into()));
  assert_eq!(endpoint(Some("https://ag-user-b.fixture.invalid"),Some("https://ag-user-a.fixture.invalid")),Some("https://ag-user-b.fixture.invalid/mcp".into()));
  assert_eq!(endpoint(None,Some("https://configured.fixture.invalid/")),Some("https://configured.fixture.invalid/mcp".into()));
 }
 #[test]fn absent_or_unsafe_origin_never_turns_into_a_copyable_placeholder(){
  assert_eq!(endpoint(None,None),None);
  for bad in ["http://127.0.0.1:47114","https://name:secret@fixture.invalid","https://fixture.invalid/mobile","https://fixture.invalid?token=secret","https://fixture.invalid#private","<MCP_URL>"]{assert_eq!(endpoint(Some(bad),Some("https://stale.fixture.invalid")),None);}
 }
 #[test]fn paired_settings_uses_this_listeners_real_address(){
  use std::sync::{Arc,Mutex};use crate::mobile_http::{MobileHttpConfig,start_with_web_auth};
  let temp=tempfile::tempdir().unwrap();std::fs::write(temp.path().join("index.html"),"fixture").unwrap();let store=Arc::new(crate::RouterStore::open_at(temp.path().join("router.db")).unwrap());
  let core=crate::host_application::RouterCore{store,session:Arc::new(Mutex::new(Default::default())),chatgpt:Arc::default(),completed_chatgpt_responses:Arc::default()};let auth=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());let rt=tokio::runtime::Runtime::new().unwrap();
  let config=MobileHttpConfig{port:0,allowed_host:"user-b.fixture.invalid".into(),allowed_origin:"https://user-b.fixture.invalid".into(),access_issuer:"https://unused.invalid".into(),access_audience:"unused".into(),access_jwks_url:"https://unused.invalid/keys".into(),static_dir:temp.path().into()};let server=rt.block_on(start_with_web_auth(core,config,crate::HostRuntime::default(),auth.clone())).unwrap();let base=format!("http://{}",server.address);let client=reqwest::blocking::Client::builder().no_proxy().build().unwrap();
  assert_eq!(client.get(format!("{base}/v1/mobile/assistant/settings")).header("host","user-b.fixture.invalid").send().unwrap().status(),401);
  let code=auth.issue().unwrap();let paired=client.post(format!("{base}/v1/mobile/auth/pair")).header("host","user-b.fixture.invalid").header("origin","https://user-b.fixture.invalid").json(&serde_json::json!({"code":code.code,"deviceName":"MCP onboarding fixture"})).send().unwrap();assert_eq!(paired.status(),200);let cookie=paired.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().to_string();
  let response=client.get(format!("{base}/v1/mobile/assistant/settings")).header("host","user-b.fixture.invalid").header("cookie",cookie).send().unwrap();assert_eq!(response.status(),200);let v:serde_json::Value=response.json().unwrap();assert_eq!(v["mcpUrl"],"https://user-b.fixture.invalid/mcp");assert_eq!(v["grants"].as_array().unwrap().len(),0);
 }

}
