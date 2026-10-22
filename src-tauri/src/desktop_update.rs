//! Desktop-only, owner-clicked signed updates. No PWA/MCP update authority.
use crate::{host::HostRuntime,host_application::RouterCore};
use serde::{Deserialize,Serialize};
use std::{sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
use tauri::{AppHandle,Emitter,Manager,State};
use tauri_plugin_updater::{Update,UpdaterExt};
const REPO:&str="geoffrey1111/agbrio";
const MAX_PACKAGE:u64=600*1024*1024;
#[derive(Clone,Serialize)]#[serde(rename_all="camelCase")]
pub(crate) struct UpdateView{pub state:String,pub current_version:String,pub version:Option<String>,pub notes:Option<String>,pub release_url:Option<String>,pub downloaded:u64,pub total:Option<u64>,pub error:Option<String>}
impl Default for UpdateView{fn default()->Self{Self{state:"IDLE".into(),current_version:env!("CARGO_PKG_VERSION").into(),version:None,notes:None,release_url:None,downloaded:0,total:None,error:None}}}
#[derive(Default)]struct Cache{view:UpdateView,candidate:Option<Update>,bytes:Option<Vec<u8>>}
#[derive(Default)]pub(crate) struct UpdateState{busy:AtomicBool,cache:Arc<Mutex<Cache>>}
struct Busy<'a>(&'a AtomicBool);impl Drop for Busy<'_>{fn drop(&mut self){self.0.store(false,Ordering::Release);}}
fn guard(s:&UpdateState)->Result<Busy<'_>,String>{s.busy.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).map_err(|_|"UPDATE_BUSY")?;Ok(Busy(&s.busy))}
fn fault(s:&UpdateState,code:&str)->String{if let Ok(mut c)=s.cache.lock(){c.view.state="FAILED".into();c.view.error=Some(code.into());}code.into()}
#[derive(Deserialize)]struct Asset{name:String,browser_download_url:String,size:u64}
#[derive(Deserialize)]struct Release{tag_name:String,draft:bool,body:Option<String>,assets:Vec<Asset>}
fn release_version(r:&Release)->Option<semver::Version>{if r.draft{return None;}semver::Version::parse(r.tag_name.strip_prefix('v').unwrap_or(&r.tag_name)).ok()}
fn release_asset_url(tag:&str,url:&url::Url)->bool{
 url.scheme()=="https"&&url.host_str()==Some("github.com")&&url.username().is_empty()&&url.password().is_none()&&url.port().is_none()&&url.query().is_none()&&url.fragment().is_none()&&url.path().starts_with(&format!("/{REPO}/releases/download/{tag}/"))
}
fn choose_release(rows:&[Release])->Option<&Release>{rows.iter().filter(|r|release_version(r).is_some()).max_by_key(|r|release_version(r).unwrap())}
// GitHub's anonymous API quota is shared by an egress IP. Its exhaustion
// must not disable the publisher's independent, fixed latest-manifest endpoint.
const LATEST_MANIFEST:&str="https://github.com/geoffrey1111/agbrio/releases/latest/download/latest.json";
const MAX_MANIFEST:usize=1024*1024;
struct PublishedManifest{version:semver::Version,url:url::Url,notes:Option<String>}
enum Discovery{Index(Vec<Release>),Manifest(PublishedManifest)}
fn manifest_metadata(bytes:&[u8])->Result<PublishedManifest,String>{
 if bytes.len()>MAX_MANIFEST{return Err("UPDATE_MANIFEST_INVALID".into())}
 let v:serde_json::Value=serde_json::from_slice(bytes).map_err(|_|"UPDATE_MANIFEST_INVALID")?;
 let version=semver::Version::parse(v["version"].as_str().ok_or("UPDATE_MANIFEST_INVALID")?).map_err(|_|"UPDATE_MANIFEST_INVALID")?;
 let platform=&v["platforms"]["windows-x86_64"];
 let signature=platform["signature"].as_str().ok_or("UPDATE_MANIFEST_INVALID")?;
 if signature.trim().is_empty()||signature.len()>8192{return Err("UPDATE_MANIFEST_INVALID".into())}
 let url=url::Url::parse(platform["url"].as_str().ok_or("UPDATE_PACKAGE_INVALID")?).map_err(|_|"UPDATE_PACKAGE_INVALID")?;
 let expected=format!("https://github.com/{REPO}/releases/download/v{version}/Agbrio_{version}_x64-setup.exe");
 if url.as_str()!=expected||!release_asset_url(&format!("v{version}"),&url){return Err("UPDATE_PACKAGE_INVALID".into())}
 Ok(PublishedManifest{version,url,notes:v["notes"].as_str().map(str::to_owned)})
}
async fn discover(client:&reqwest::Client,index:&str,manifest:&str)->Result<Discovery,String>{
 if let Ok(response)=client.get(index).send().await{
  if response.status().is_success(){return response.json().await.map(Discovery::Index).map_err(|_|"UPDATE_MANIFEST_INVALID".into())}
 }
 let mut response=client.get(manifest).send().await.map_err(|_|"UPDATE_NETWORK")?;
 if !response.status().is_success(){return Err("UPDATE_NETWORK".into())}
 if response.content_length().is_some_and(|n|n>MAX_MANIFEST as u64){return Err("UPDATE_MANIFEST_INVALID".into())}
 let mut bytes=Vec::new();
 while let Some(chunk)=response.chunk().await.map_err(|_|"UPDATE_NETWORK")?{
  if bytes.len()+chunk.len()>MAX_MANIFEST{return Err("UPDATE_MANIFEST_INVALID".into())}bytes.extend_from_slice(&chunk);
 }
 manifest_metadata(&bytes).map(Discovery::Manifest)
}
fn package_size(length:Option<u64>)->Result<u64,String>{length.filter(|n|*n>0&&*n<=MAX_PACKAGE).ok_or_else(||"UPDATE_PACKAGE_INVALID".into())}
async fn check_manifest(app:&AppHandle,state:&UpdateState,client:&reqwest::Client,published:PublishedManifest)->Result<(),String>{
 let tag=format!("v{}",published.version);
 {let mut c=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?;c.view.version=Some(published.version.to_string());c.view.release_url=Some(format!("https://github.com/{REPO}/releases/tag/{tag}"));c.view.notes=published.notes;
  if published.version<=app.package_info().version{c.view.state="UP_TO_DATE".into();return Ok(())}}
 // Keep the original pre-download size fence without requiring the API.
 let head=client.head(published.url.clone()).send().await.map_err(|_|"UPDATE_NETWORK")?;
 if !head.status().is_success(){return Err("UPDATE_PACKAGE_INVALID".into())}let size=package_size(head.headers().get(reqwest::header::CONTENT_LENGTH).and_then(|h|h.to_str().ok()).and_then(|s|s.parse().ok()))?;
 let handle=app.clone();let builder=app.updater_builder().timeout(Duration::from_secs(15)).endpoints(vec![url::Url::parse(LATEST_MANIFEST).unwrap()]).map_err(|_|"UPDATE_MANIFEST_INVALID")?;
 #[cfg(windows)]let builder=builder.on_before_exit(move||{let host=handle.state::<HostRuntime>();let core=handle.state::<RouterCore>();host.shutdown(&core);});
 let mut update=builder.build().map_err(|_|"UPDATE_CONFIGURATION")?.check().await.map_err(|_|"UPDATE_MANIFEST_INVALID")?.ok_or("UPDATE_CHANGED")?;
 if update.version!=published.version.to_string()||update.download_url!=published.url{return Err("UPDATE_CHANGED".into())}
 update.timeout=Some(Duration::from_secs(600));let mut c=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?;c.view.state="AVAILABLE".into();c.view.total=Some(size);c.candidate=Some(update);Ok(())
}
#[tauri::command]
pub(crate) fn desktop_update_status(app:AppHandle,state:State<'_,UpdateState>)->Result<UpdateView,String>{let mut view=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?.view.clone();view.current_version=app.package_info().version.to_string();Ok(view)}
#[tauri::command]
pub(crate) async fn desktop_update_check(app:AppHandle,state:State<'_,UpdateState>)->Result<UpdateView,String>{
 let _busy=guard(&state)?;{let mut c=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?;*c=Cache::default();c.view.state="CHECKING".into();c.view.current_version=app.package_info().version.to_string();}
 let result:Result<(),String>=async{
  let client=reqwest::Client::builder().timeout(Duration::from_secs(15)).user_agent("Agbrio-update-check").build().map_err(|_|"UPDATE_NETWORK")?;
  let rows=match discover(&client,&format!("https://api.github.com/repos/{REPO}/releases?per_page=20"),LATEST_MANIFEST).await?{
   Discovery::Index(rows)=>rows,
   Discovery::Manifest(published)=>return check_manifest(&app,&state,&client,published).await,
  };
  let selected=choose_release(&rows);let current=app.package_info().version.clone();
  let Some(r)=selected else{state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?.view.state="NO_SIGNED_RELEASE".into();return Ok(());};
  let remote=release_version(r).unwrap();let signed=r.assets.iter().find(|a|a.name=="latest.json");
  {let mut c=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?;c.view.release_url=Some(format!("https://github.com/{REPO}/releases/tag/{}",r.tag_name));c.view.notes=r.body.clone();c.view.version=Some(remote.to_string());
   if remote<=current{c.view.state=if signed.is_some(){"UP_TO_DATE"}else{"NO_SIGNED_RELEASE"}.into();return Ok(());}
   if signed.is_none(){c.view.state="MANUAL_AVAILABLE".into();return Ok(());}}
  let url=url::Url::parse(&signed.unwrap().browser_download_url).map_err(|_|"UPDATE_MANIFEST_INVALID")?;
  if !release_asset_url(&r.tag_name,&url)||signed.unwrap().size>1024*1024{return Err("UPDATE_MANIFEST_INVALID".into());}
  let handle=app.clone();let builder=app.updater_builder().timeout(Duration::from_secs(15)).endpoints(vec![url]).map_err(|_|"UPDATE_MANIFEST_INVALID")?;
  #[cfg(windows)]let builder=builder.on_before_exit(move||{let host=handle.state::<HostRuntime>();let core=handle.state::<RouterCore>();host.shutdown(&core);});
  let mut update=builder.build().map_err(|_|"UPDATE_CONFIGURATION")?.check().await.map_err(|_|"UPDATE_MANIFEST_INVALID")?.ok_or("UPDATE_MANIFEST_INVALID")?;
  let asset=r.assets.iter().find(|a|a.browser_download_url==update.download_url.as_str()).ok_or("UPDATE_PACKAGE_INVALID")?;
  if update.version!=remote.to_string()||!release_asset_url(&r.tag_name,&update.download_url)||!asset.name.ends_with(".exe")||asset.size==0||asset.size>MAX_PACKAGE{return Err("UPDATE_PACKAGE_INVALID".into());}
  update.timeout=Some(Duration::from_secs(600));let mut c=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?;c.view.state="AVAILABLE".into();c.view.total=Some(asset.size);c.candidate=Some(update);Ok(())
 }.await;
 if let Err(e)=result{return Err(fault(&state,&e));}Ok(state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?.view.clone())
}
#[tauri::command]
pub(crate) async fn desktop_update_download(app:AppHandle,version:String,state:State<'_,UpdateState>)->Result<UpdateView,String>{
 let _busy=guard(&state)?;let update={let mut c=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?;if c.view.version.as_deref()!=Some(&version)||c.view.state!="AVAILABLE"{return Err("UPDATE_CHANGED".into());}c.view.state="DOWNLOADING".into();c.candidate.clone().ok_or("UPDATE_CHANGED")?};
 let cache=state.cache.clone();let mut emitted=Instant::now();let data=update.download(move|length,total|{if let Ok(mut c)=cache.lock(){c.view.downloaded+=length as u64;if total.is_some(){c.view.total=total;}if emitted.elapsed()>Duration::from_millis(150){let _=app.emit("agbrio-update-progress",c.view.clone());emitted=Instant::now();}}},||{}).await.map_err(|_|fault(&state,"UPDATE_DOWNLOAD_OR_SIGNATURE_FAILED"))?;
 if data.len() as u64>MAX_PACKAGE{return Err(fault(&state,"UPDATE_PACKAGE_INVALID"));}let mut c=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?;c.view.state="READY".into();c.view.downloaded=data.len() as u64;c.bytes=Some(data);Ok(c.view.clone())
}
#[tauri::command]
pub(crate) fn desktop_update_install(version:String,confirm_restart:Option<bool>,state:State<'_,UpdateState>,core:State<'_,RouterCore>)->Result<(),String>{
 let _busy=guard(&state)?;
 let active={let s=core.session.lock().map_err(|_|"UPDATE_BUSY")?;s.codex_adapter_borrowed||s.connecting||s.adapter.as_ref().is_some_and(|a|a.has_owned_active_turns())};
 let (update,data)={let mut c=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?;install_gate(&version,c.view.version.as_deref(),&c.view.state,c.candidate.is_some()&&c.bytes.is_some(),active,confirm_restart.unwrap_or(false))?;let update=c.candidate.clone().ok_or("UPDATE_CHANGED")?;let data=c.bytes.take().ok_or("UPDATE_CHANGED")?;c.view.state="INSTALLING".into();(update,data)};
 if core.store.backup_before_application_update().is_err(){if let Ok(mut c)=state.cache.lock(){c.bytes=Some(data);c.view.state="READY".into();}return Err("UPDATE_BACKUP_FAILED".into());}
 if update.install(&data).is_err(){if let Ok(mut c)=state.cache.lock(){c.bytes=Some(data);c.view.state="READY".into();}return Err("UPDATE_INSTALL_FAILED".into());}Ok(())
}
// Desktop owner confirmation overrides only the active-task gate, never the
// verified package/version gate or the mandatory backup before installation.
fn install_gate(version:&str,cached_version:Option<&str>,state:&str,verified:bool,active:bool,confirmed:bool)->Result<(),String>{
 if cached_version!=Some(version)||state!="READY"||!verified{return Err("UPDATE_CHANGED".into());}
 if active&&!confirmed{return Err("UPDATE_TASK_RUNNING".into());}Ok(())
}
#[tauri::command]
pub(crate) fn desktop_update_open_release(state:State<'_,UpdateState>)->Result<(),String>{
 let url=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?.view.release_url.clone().unwrap_or_else(||format!("https://github.com/{REPO}/releases"));
 #[cfg(windows)]{use std::os::windows::process::CommandExt;std::process::Command::new("rundll32.exe").arg("url.dll,FileProtocolHandler").arg(url).creation_flags(0x08000000).spawn().map_err(|_|"UPDATE_OPEN_FAILED")?;Ok(())}
 #[cfg(not(windows))]{let _=url;Err("UPDATE_PLATFORM_UNSUPPORTED".into())}
}
#[cfg(test)]mod tests{
 use super::*;
 fn manifest_fixture()->serde_json::Value{serde_json::json!({"version":"0.1.35-rc.54","notes":"update","platforms":{"windows-x86_64":{"signature":"fixture-signature","url":"https://github.com/geoffrey1111/agbrio/releases/download/v0.1.35-rc.54/Agbrio_0.1.35-rc.54_x64-setup.exe"}}})}
 #[test]fn independent_manifest_retains_exact_repository_version_filename_and_size_fences(){
  let v=manifest_fixture();assert_eq!(manifest_metadata(&serde_json::to_vec(&v).unwrap()).unwrap().version.to_string(),"0.1.35-rc.54");
  for bad in ["https://example.test/a.exe","https://github.com/other/repo/releases/download/v0.1.35-rc.54/a.exe","https://github.com/geoffrey1111/agbrio/releases/download/v0.1.35-rc.53/Agbrio_0.1.35-rc.54_x64-setup.exe","https://github.com/geoffrey1111/agbrio/releases/download/v0.1.35-rc.54/other.exe"]{let mut wrong=v.clone();wrong["platforms"]["windows-x86_64"]["url"]=bad.into();assert!(manifest_metadata(&serde_json::to_vec(&wrong).unwrap()).is_err())}
  let mut no_signature=v;no_signature["platforms"]["windows-x86_64"]["signature"]=" ".into();assert!(manifest_metadata(&serde_json::to_vec(&no_signature).unwrap()).is_err());assert!(manifest_metadata(&vec![b' ';MAX_MANIFEST+1]).is_err());
  for n in [None,Some(0),Some(MAX_PACKAGE+1)]{assert!(package_size(n).is_err())}assert_eq!(package_size(Some(MAX_PACKAGE)).unwrap(),MAX_PACKAGE);
 }
 #[tokio::test]async fn anonymous_api_failure_uses_only_the_fixed_manifest_transport(){
  use axum::{Router,routing::get,http::StatusCode,Json};use std::sync::atomic::{AtomicUsize,Ordering};
  let hits=Arc::new(AtomicUsize::new(0));let count=hits.clone();
  let server=Router::new().route("/rate",get(||async{(StatusCode::FORBIDDEN,[("x-ratelimit-remaining","0")],"rate limit")})).route("/down",get(||async{StatusCode::SERVICE_UNAVAILABLE})).route("/invalid",get(||async{"not json"})).route("/index",get(||async{Json(serde_json::json!([]))})).route("/latest",get(move||{let count=count.clone();async move{count.fetch_add(1,Ordering::SeqCst);Json(manifest_fixture())}})).route("/oversize",get(||async{vec![b' ';MAX_MANIFEST+1]}));
  let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let origin=format!("http://{}",listener.local_addr().unwrap());let task=tokio::spawn(async move{axum::serve(listener,server).await.unwrap()});
  let client=reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap();
  for path in ["/rate","/down"]{let found=discover(&client,&format!("{origin}{path}"),&format!("{origin}/latest")).await.unwrap();assert!(matches!(found,Discovery::Manifest(p) if p.version.to_string()=="0.1.35-rc.54"));}
  assert!(matches!(discover(&client,&format!("{origin}/index"),&format!("{origin}/latest")).await.unwrap(),Discovery::Index(_)));
  assert!(discover(&client,&format!("{origin}/invalid"),&format!("{origin}/latest")).await.is_err());assert_eq!(hits.load(Ordering::SeqCst),2);
  assert!(discover(&client,&format!("{origin}/rate"),&format!("{origin}/oversize")).await.is_err());task.abort();
 }
 #[tokio::test]#[ignore="explicit read-only public GitHub quota fallback; no app/provider/installation"]
 async fn real_public_manifest_discovery_without_api_quota(){
  let client=reqwest::Client::builder().timeout(Duration::from_secs(15)).user_agent("Agbrio-update-check").build().unwrap();
  let found=discover(&client,&format!("https://api.github.com/repos/{REPO}/releases?per_page=20"),LATEST_MANIFEST).await.unwrap();
  let Discovery::Manifest(published)=found else{panic!("This explicit counterexample requires an exhausted anonymous API quota")};
  let response=client.head(published.url).send().await.unwrap();assert!(response.status().is_success());let size=package_size(response.headers().get(reqwest::header::CONTENT_LENGTH).and_then(|h|h.to_str().ok()).and_then(|s|s.parse().ok())).unwrap();
  println!("apiFallback=true version={} packageBytes={} providerCalls=0 installs=0",published.version,size);
 }
 #[test]fn active_restart_requires_confirmation_for_the_exact_verified_candidate(){
  assert_eq!(install_gate("0.1.3",Some("0.1.3"),"READY",true,true,false),Err("UPDATE_TASK_RUNNING".into()));
  assert!(install_gate("0.1.3",Some("0.1.3"),"READY",true,true,true).is_ok());
  assert!(install_gate("0.1.3",Some("0.1.3"),"READY",true,false,false).is_ok());
  for (version,state,verified) in [(Some("0.1.4"),"READY",true),(Some("0.1.3"),"DOWNLOADING",true),(Some("0.1.3"),"READY",false),(None,"READY",true)]{assert_eq!(install_gate("0.1.3",version,state,verified,true,true),Err("UPDATE_CHANGED".into()));}
 }
 fn release(tag:&str,draft:bool)->Release{Release{tag_name:tag.into(),draft,body:None,assets:vec![]}}
 #[test]fn version_selection_ignores_drafts_and_preserves_prerelease_order(){let rows=vec![release("v0.1.0-preview.16",false),release("v0.1.1",false),release("v9.0.0",true),release("invalid",false)];assert_eq!(choose_release(&rows).unwrap().tag_name,"v0.1.1");}
 #[test]fn only_the_pinned_release_can_supply_an_installer(){for u in ["http://github.com/geoffrey1111/agbrio/releases/download/v0.1.1/a.exe","https://example.test/a.exe","https://github.com/other/repo/releases/download/v0.1.1/a.exe","https://github.com/geoffrey1111/agbrio/releases/download/v0.1.0/a.exe","https://github.com/geoffrey1111/agbrio/releases/download/v0.1.1/a.exe?x=1"]{assert!(!release_asset_url("v0.1.1",&url::Url::parse(u).unwrap()));}assert!(release_asset_url("v0.1.1",&url::Url::parse("https://github.com/geoffrey1111/agbrio/releases/download/v0.1.1/Agbrio.exe").unwrap()));}
 #[test]fn publisher_signature_is_verified_and_changed_bytes_are_rejected(){
  use base64ct::{Base64,Encoding};
  let config:serde_json::Value=serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
  let public=String::from_utf8(Base64::decode_vec(config["plugins"]["updater"]["pubkey"].as_str().unwrap()).unwrap()).unwrap();
  let raw=String::from_utf8(Base64::decode_vec(include_str!("../test-fixtures/updater-signature-sample.txt.sig").trim()).unwrap()).unwrap();
  let key=minisign_verify::PublicKey::decode(&public).unwrap();let signature=minisign_verify::Signature::decode(&raw).unwrap();
  assert!(key.verify(include_bytes!("../test-fixtures/updater-signature-sample.txt"),&signature,true).is_ok());assert!(key.verify(b"changed fixture bytes",&signature,true).is_err());
 }
 #[test]fn mutation_guard_cannot_reenter_and_is_released_on_failure(){let state=UpdateState::default();{let _guard=guard(&state).unwrap();assert!(guard(&state).is_err());}assert!(guard(&state).is_ok());}
 #[test]#[ignore="Explicit local installer signing gate; never publishes or installs"]
 fn actual_installer_signature_accepts_original_and_rejects_tampering(){
  use base64ct::{Base64,Encoding};
  let path=std::path::PathBuf::from(std::env::var("AGBRIO_SIGNED_CANDIDATE").expect("local candidate path"));
  let allowed=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/demo-expanded-20261008/signing-proof").canonicalize().unwrap();
  let path=path.canonicalize().unwrap();assert_eq!(path.parent(),Some(allowed.as_path()));
  let config:serde_json::Value=serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
  let public=String::from_utf8(Base64::decode_vec(config["plugins"]["updater"]["pubkey"].as_str().unwrap()).unwrap()).unwrap();
  let raw=String::from_utf8(Base64::decode_vec(std::fs::read_to_string(path.with_extension("exe.sig")).unwrap().trim()).unwrap()).unwrap();
  let key=minisign_verify::PublicKey::decode(&public).unwrap();let signature=minisign_verify::Signature::decode(&raw).unwrap();let mut bytes=std::fs::read(path).unwrap();
  assert!(bytes.len()>1024*1024);assert!(key.verify(&bytes,&signature,true).is_ok());let end=bytes.len()-1;bytes[end]^=1;assert!(key.verify(&bytes,&signature,true).is_err());
 }
 #[test]#[ignore="Explicit local signed-release gate; never publishes, launches or installs"]
 fn actual_signed_release_manifest_matches_verified_installer(){
  use base64ct::{Base64,Encoding};
  let runtime=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime").canonicalize().unwrap();
  let directory=std::path::PathBuf::from(std::env::var("AGBRIO_SIGNED_RELEASE_DIR").expect("local signed release directory")).canonicalize().unwrap();
  assert_eq!(directory.parent(),Some(runtime.as_path()));
  let name=directory.file_name().unwrap().to_str().unwrap();
  let version=semver::Version::parse(name.strip_prefix("signed-release-").expect("signed release directory")).unwrap();
  assert!(version>=semver::Version::parse("0.1.1").unwrap());
  let manifest:serde_json::Value=serde_json::from_slice(&std::fs::read(directory.join("latest.json")).unwrap()).unwrap();
  assert_eq!(manifest["version"].as_str(),Some(version.to_string().as_str()));
  let filename=format!("Agbrio_{version}_x64-setup.exe");
  let installer=directory.join(&filename).canonicalize().unwrap();assert_eq!(installer.parent(),Some(directory.as_path()));
  let platform=&manifest["platforms"]["windows-x86_64"];
  assert_eq!(platform["url"].as_str(),Some(format!("https://github.com/{REPO}/releases/download/v{version}/{filename}").as_str()));
  let encoded=std::fs::read_to_string(directory.join(format!("{filename}.sig"))).unwrap();
  assert_eq!(platform["signature"].as_str(),Some(encoded.trim()));
  let config:serde_json::Value=serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
  let public=String::from_utf8(Base64::decode_vec(config["plugins"]["updater"]["pubkey"].as_str().unwrap()).unwrap()).unwrap();
  let raw=String::from_utf8(Base64::decode_vec(encoded.trim()).unwrap()).unwrap();
  let key=minisign_verify::PublicKey::decode(&public).unwrap();let signature=minisign_verify::Signature::decode(&raw).unwrap();
  let mut bytes=std::fs::read(installer).unwrap();assert!(bytes.len()>1024*1024&&bytes.len() as u64<=MAX_PACKAGE);
  assert!(key.verify(&bytes,&signature,true).is_ok());bytes[0]^=1;assert!(key.verify(&bytes,&signature,true).is_err());
 }
}
