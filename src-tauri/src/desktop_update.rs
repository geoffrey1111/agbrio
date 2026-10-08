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
#[tauri::command]
pub(crate) fn desktop_update_status(app:AppHandle,state:State<'_,UpdateState>)->Result<UpdateView,String>{let mut view=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?.view.clone();view.current_version=app.package_info().version.to_string();Ok(view)}
#[tauri::command]
pub(crate) async fn desktop_update_check(app:AppHandle,state:State<'_,UpdateState>)->Result<UpdateView,String>{
 let _busy=guard(&state)?;{let mut c=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?;*c=Cache::default();c.view.state="CHECKING".into();c.view.current_version=app.package_info().version.to_string();}
 let result:Result<(),String>=async{
  let client=reqwest::Client::builder().timeout(Duration::from_secs(15)).user_agent("Agbrio-update-check").build().map_err(|_|"UPDATE_NETWORK")?;
  let response=client.get(format!("https://api.github.com/repos/{REPO}/releases?per_page=20")).send().await.map_err(|_|"UPDATE_NETWORK")?;
  if !response.status().is_success(){return Err("UPDATE_NETWORK".into());}
  let rows:Vec<Release>=response.json().await.map_err(|_|"UPDATE_MANIFEST_INVALID")?;
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
pub(crate) fn desktop_update_install(version:String,state:State<'_,UpdateState>,core:State<'_,RouterCore>)->Result<(),String>{
 let _busy=guard(&state)?;
 {let s=core.session.lock().map_err(|_|"UPDATE_BUSY")?;if s.codex_adapter_borrowed||s.connecting||s.adapter.as_ref().is_some_and(|a|a.has_owned_active_turns()){return Err("UPDATE_TASK_RUNNING".into());}}
 let (update,data)={let mut c=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?;if c.view.version.as_deref()!=Some(&version)||c.view.state!="READY"{return Err("UPDATE_CHANGED".into());}let update=c.candidate.clone().ok_or("UPDATE_CHANGED")?;let data=c.bytes.take().ok_or("UPDATE_CHANGED")?;c.view.state="INSTALLING".into();(update,data)};
 if core.store.backup_before_application_update().is_err(){if let Ok(mut c)=state.cache.lock(){c.bytes=Some(data);c.view.state="READY".into();}return Err("UPDATE_BACKUP_FAILED".into());}
 if update.install(&data).is_err(){if let Ok(mut c)=state.cache.lock(){c.bytes=Some(data);c.view.state="READY".into();}return Err("UPDATE_INSTALL_FAILED".into());}Ok(())
}
#[tauri::command]
pub(crate) fn desktop_update_open_release(state:State<'_,UpdateState>)->Result<(),String>{
 let url=state.cache.lock().map_err(|_|"UPDATE_STATE_UNAVAILABLE")?.view.release_url.clone().unwrap_or_else(||format!("https://github.com/{REPO}/releases"));
 #[cfg(windows)]{use std::os::windows::process::CommandExt;std::process::Command::new("rundll32.exe").arg("url.dll,FileProtocolHandler").arg(url).creation_flags(0x08000000).spawn().map_err(|_|"UPDATE_OPEN_FAILED")?;Ok(())}
 #[cfg(not(windows))]{let _=url;Err("UPDATE_PLATFORM_UNSUPPORTED".into())}
}
#[cfg(test)]mod tests{
 use super::*;
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
