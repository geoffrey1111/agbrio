//! Owner-issued, one-use pairing codes and revocable browser sessions.
//! This authenticates access to the existing Host; it never approves a task.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, sync::{Arc, Mutex, OnceLock}, time::{SystemTime, UNIX_EPOCH}};
use uuid::Uuid;

pub(crate) const COOKIE: &str = "__Host-aiwr_session";
const CODE_SECONDS: u64 = 300;
const SESSION_SECONDS: u64 = 90 * 24 * 60 * 60;
const TEMPORARY_SECONDS: u64 = 12 * 60 * 60;
const RENEW_INTERVAL: u64 = 24 * 60 * 60;
fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }
fn hash(value: &str) -> String { format!("{:x}", Sha256::digest(value.as_bytes())) }
fn same(a: &str, b: &str) -> bool { a.len()==b.len() && a.bytes().zip(b.bytes()).fold(0u8, |diff,(x,y)|diff|(x^y))==0 }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Device { pub id:String, pub label:String, pub created_at:u64, pub expires_at:u64 }
#[derive(Clone, Serialize, Deserialize)]
struct SavedSession { device:Device, token_hash:String, #[serde(default)] remembered:Option<bool> }
#[derive(Default, Serialize, Deserialize)]
struct Saved { sessions:Vec<SavedSession> }
struct Code { hash:String, expires_at:u64, tries:u8, replace_device_id:Option<String> }
#[derive(Default)]
struct Inner { saved:Saved, code:Option<Code> }
pub(crate) struct WebAuth { inner:Mutex<Inner>, path:Option<PathBuf> }
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct PairingCode { pub code:String, pub expires_at:u64 }

impl WebAuth {
 pub(crate) fn open(path:Option<PathBuf>)->Result<Self,String> {
  let saved=match path.as_ref(){Some(p) if p.exists()=>serde_json::from_slice::<Saved>(&fs::read(p).map_err(|_|"WEB_AUTH_STORE_UNAVAILABLE")?).map_err(|_|"WEB_AUTH_STORE_INVALID")?,_=>Saved::default()};
  if saved.sessions.len()>20 {return Err("WEB_AUTH_STORE_INVALID".into());}
  Ok(Self{inner:Mutex::new(Inner{saved,code:None}),path})
 }
 fn save(&self,sessions:&[SavedSession])->Result<(),String>{
  if let Some(path)=&self.path {
   fs::create_dir_all(path.parent().ok_or("WEB_AUTH_STORE_UNAVAILABLE")?).map_err(|_|"WEB_AUTH_STORE_UNAVAILABLE")?;
   let temporary=path.with_extension("json.pending");
   let bytes=serde_json::to_vec(&Saved{sessions:sessions.to_vec()}).map_err(|_|"WEB_AUTH_STORE_UNAVAILABLE")?;
   fs::write(&temporary,bytes).map_err(|_|"WEB_AUTH_STORE_UNAVAILABLE")?;
   fs::rename(temporary,path).map_err(|_|"WEB_AUTH_STORE_UNAVAILABLE")?;
  } Ok(())
 }
 pub(crate) fn issue(&self)->Result<PairingCode,String>{
  self.issue_replacing(None)
 }
 pub(crate) fn issue_replacing(&self,replace_device_id:Option<String>)->Result<PairingCode,String>{
  let mut inner=self.inner.lock().map_err(|_|"WEB_AUTH_UNAVAILABLE")?;
  if replace_device_id.as_ref().is_some_and(|id|!inner.saved.sessions.iter().any(|s|&s.device.id==id&&s.device.expires_at>now())){return Err("WEB_REPLACEMENT_TARGET_CHANGED".into());}
  let random=Uuid::new_v4();let b=random.as_bytes();let number=u32::from_le_bytes([b[0],b[1],b[2],b[3]])%1_000_000;
  let code=format!("{number:06}");let expires_at=now()+CODE_SECONDS;
  inner.code=Some(Code{hash:hash(&code),expires_at,tries:0,replace_device_id});
  Ok(PairingCode{code,expires_at})
 }
 pub(crate) fn exchange(&self,code:&str,label:&str,remember:bool)->Result<String,String>{self.exchange_at(code,label,remember,now())}
 fn exchange_at(&self,code:&str,label:&str,remember:bool,time:u64)->Result<String,String>{
  let mut inner=self.inner.lock().map_err(|_|"WEB_AUTH_UNAVAILABLE")?;
  let pending=inner.code.as_mut().ok_or("WEB_PAIRING_INVALID_OR_EXPIRED")?;
  if pending.expires_at<=time || pending.tries>=5 {inner.code=None;return Err("WEB_PAIRING_INVALID_OR_EXPIRED".into());}
  pending.tries+=1;
  if code.len()!=6 || !code.bytes().all(|b|b.is_ascii_digit()) || !same(&hash(code),&pending.hash){return Err("WEB_PAIRING_INVALID_OR_EXPIRED".into());}
  let replace=pending.replace_device_id.clone();
  if replace.as_ref().is_some_and(|id|!inner.saved.sessions.iter().any(|s|&s.device.id==id&&s.device.expires_at>time)){return Err("WEB_REPLACEMENT_TARGET_CHANGED".into());}
  let token=format!("{}{}",Uuid::new_v4().simple(),Uuid::new_v4().simple());
  let label:String=label.trim().chars().filter(|c|!c.is_control()).take(40).collect();
  let device=Device{id:Uuid::new_v4().to_string(),label:if label.is_empty(){"浏览器设备".into()}else{label},created_at:time,expires_at:time+if remember{SESSION_SECONDS}else{TEMPORARY_SECONDS}};
  let mut next:Vec<_>=inner.saved.sessions.iter().filter(|s|s.device.expires_at>time&&replace.as_ref()!=Some(&s.device.id)).cloned().collect();
  if next.len()>=20{return Err("WEB_DEVICE_LIMIT".into());}
  next.push(SavedSession{device,token_hash:hash(&token),remembered:Some(remember)});
  self.save(&next)?;inner.saved.sessions=next;inner.code=None;Ok(token)
 }
 pub(crate) fn valid(&self,token:&str)->bool {self.valid_at(token,now())}
 fn valid_at(&self,token:&str,time:u64)->bool {
  if token.len()!=64 || !token.bytes().all(|b|b.is_ascii_hexdigit()){return false;}
  let digest=hash(token);self.inner.lock().map(|i|i.saved.sessions.iter().any(|s|s.device.expires_at>time&&same(&s.token_hash,&digest))).unwrap_or(false)
 }
 // Only a valid, remembered device can renew. Old 30-day records migrate
 // on use without changing their identity/hash; expired/revoked devices stay out.
 pub(crate) fn renew(&self,token:&str)->Result<Option<bool>,String>{self.renew_at(token,now())}
 fn renew_at(&self,token:&str,time:u64)->Result<Option<bool>,String>{
  if token.len()!=64 || !token.bytes().all(|b|b.is_ascii_hexdigit()){return Ok(None);}
  let digest=hash(token);let mut inner=self.inner.lock().map_err(|_|"WEB_AUTH_UNAVAILABLE")?;
  let Some(index)=inner.saved.sessions.iter().position(|s|s.device.expires_at>time&&same(&s.token_hash,&digest)) else{return Ok(None);};
  let session=&inner.saved.sessions[index];
  let remembered=session.remembered.unwrap_or(session.device.expires_at.saturating_sub(session.device.created_at)>TEMPORARY_SECONDS);
  if remembered && session.device.expires_at<time+SESSION_SECONDS-RENEW_INTERVAL {
   let mut next=inner.saved.sessions.clone();next[index].device.expires_at=time+SESSION_SECONDS;next[index].remembered=Some(true);
   self.save(&next)?;inner.saved.sessions=next;
  }
  Ok(Some(remembered))
 }
 pub(crate) fn devices(&self)->Result<Vec<Device>,String>{Ok(self.inner.lock().map_err(|_|"WEB_AUTH_UNAVAILABLE")?.saved.sessions.iter().filter(|s|s.device.expires_at>now()).map(|s|s.device.clone()).collect())}
 pub(crate) fn revoke(&self,id:Option<&str>,token:Option<&str>)->Result<(),String>{
  let mut inner=self.inner.lock().map_err(|_|"WEB_AUTH_UNAVAILABLE")?;let digest=token.map(hash);
  let next:Vec<_>=inner.saved.sessions.iter().filter(|s|!(id==Some(s.device.id.as_str())||digest.as_ref().is_some_and(|h|same(h,&s.token_hash)))).cloned().collect();
  self.save(&next)?;inner.saved.sessions=next;Ok(())
 }
}

pub(crate) fn cookie_token(headers:&axum::http::HeaderMap)->Option<String>{
 let mut found=None;
 for value in headers.get_all(axum::http::header::COOKIE){for part in value.to_str().ok()?.split(';'){if let Some((name,value))=part.trim().split_once('='){if name==COOKIE{if found.is_some(){return None;}found=Some(value.to_string());}}}}
 found
}
pub(crate) fn global()->Result<Arc<WebAuth>,String>{
 static AUTH:OnceLock<Result<Arc<WebAuth>,String>>=OnceLock::new();
 AUTH.get_or_init(||{let local=std::env::var_os("LOCALAPPDATA").ok_or("WEB_AUTH_STORE_UNAVAILABLE")?;WebAuth::open(Some(PathBuf::from(local).join("AIWorkRouter/data/web-auth.json"))).map(Arc::new)}).clone()
}
pub(crate) fn session_cookie(token:&str,remember:bool)->String {format!("{COOKIE}={token}; Path=/; Secure; HttpOnly; SameSite=Strict{}",if remember{format!("; Max-Age={SESSION_SECONDS}")}else{String::new()})}
pub(crate) fn clear_cookie()->String {format!("{COOKIE}=; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=0")}

#[cfg(test)]
mod tests {
 use super::*;
 #[test]fn concurrent_redemption_has_one_winner_and_failed_persistence_issues_nothing(){
  let auth=Arc::new(WebAuth::open(None).unwrap());let code=auth.issue().unwrap().code;let mut workers=Vec::new();
  for _ in 0..10{let a=auth.clone();let c=code.clone();workers.push(std::thread::spawn(move||a.exchange(&c,"phone",true).is_ok()));}
  assert_eq!(workers.into_iter().map(|w|w.join().unwrap() as u8).sum::<u8>(),1);assert_eq!(auth.devices().unwrap().len(),1);
  let d=tempfile::tempdir().unwrap();let blocked=d.path().join("not-a-directory");fs::write(&blocked,"file").unwrap();let a=WebAuth::open(Some(blocked.join("auth.json"))).unwrap();let c=a.issue().unwrap();assert!(a.exchange(&c.code,"cannot-save",true).is_err());assert!(a.devices().unwrap().is_empty());
 }
 #[test]fn once_expiry_rate_limit_rotation_and_revocation(){
  let a=WebAuth::open(None).unwrap();let c=a.issue().unwrap();let t=a.exchange(&c.code,"phone",true).unwrap();assert!(a.valid(&t));assert!(a.exchange(&c.code,"replay",true).is_err());
  let id=a.devices().unwrap()[0].id.clone();a.revoke(Some(&id),None).unwrap();assert!(!a.valid(&t));
  let c=a.issue().unwrap();assert!(a.exchange_at(&c.code,"expired",true,c.expires_at).is_err());
  let c=a.issue().unwrap();for _ in 0..5{assert!(a.exchange("bad","guess",true).is_err());}assert!(a.exchange(&c.code,"locked",true).is_err());
  let old=a.issue().unwrap();let new=a.issue().unwrap();if old.code!=new.code{assert!(a.exchange(&old.code,"old",true).is_err());}let token=a.exchange(&new.code,"new",false).unwrap();assert!(!a.valid_at(&token,now()+12*60*60));
 }
 #[test]fn restart_preserves_hashed_sessions_but_not_codes(){
  let d=tempfile::tempdir().unwrap();let p=d.path().join("auth.json");let a=WebAuth::open(Some(p.clone())).unwrap();let c=a.issue().unwrap();let token=a.exchange(&c.code,"phone",true).unwrap();let unused=a.issue().unwrap();drop(a);
  let raw=fs::read_to_string(&p).unwrap();assert!(!raw.contains(&token));assert!(!raw.contains("\"code\""));let b=WebAuth::open(Some(p)).unwrap();assert!(b.valid(&token));assert!(b.exchange(&unused.code,"old",true).is_err());b.revoke(None,Some(&token)).unwrap();assert!(!b.valid(&token));
 }
 #[test]fn rolling_renewal_migrates_old_devices_but_never_revives_expired_or_temporary_devices(){
  let d=tempfile::tempdir().unwrap();let p=d.path().join("auth.json");let token="a".repeat(64);let start=1000;
  fs::write(&p,serde_json::to_vec(&serde_json::json!({"sessions":[{"device":{"id":"old","label":"phone","createdAt":start,"expiresAt":start+30*86400},"token_hash":hash(&token)}]})).unwrap()).unwrap();
  let a=WebAuth::open(Some(p.clone())).unwrap();assert_eq!(a.renew_at(&token,start+86400).unwrap(),Some(true));
  let expiry=a.inner.lock().unwrap().saved.sessions[0].device.expires_at;assert_eq!(expiry,start+86400+SESSION_SECONDS);
  assert_eq!(a.renew_at(&token,start+86401).unwrap(),Some(true));assert_eq!(a.inner.lock().unwrap().saved.sessions[0].device.expires_at,expiry);
  drop(a);let a=WebAuth::open(Some(p)).unwrap();assert!(a.valid_at(&token,expiry-1));assert_eq!(a.renew_at(&token,expiry).unwrap(),None);
  a.revoke(Some("old"),None).unwrap();assert_eq!(a.renew_at(&token,start+2*86400).unwrap(),None);
  let code=a.issue().unwrap();let time=now();let temporary=a.exchange_at(&code.code,"temporary",false,time).unwrap();assert_eq!(a.renew_at(&temporary,time+3600).unwrap(),Some(false));assert!(!a.valid_at(&temporary,time+TEMPORARY_SECONDS));
 }
 #[test]fn corrupt_store_and_duplicate_cookie_fail_closed(){let d=tempfile::tempdir().unwrap();let p=d.path().join("auth.json");fs::write(&p,"broken").unwrap();assert!(WebAuth::open(Some(p)).is_err());let mut h=axum::http::HeaderMap::new();h.insert("cookie",format!("{COOKIE}=a; {COOKIE}=b").parse().unwrap());assert!(cookie_token(&h).is_none());}
 #[test]fn replacing_a_reinstalled_phone_at_twenty_retains_other_devices_and_persists(){let d=tempfile::tempdir().unwrap();let p=d.path().join("auth.json");let auth=WebAuth::open(Some(p.clone())).unwrap();let tokens:Vec<_>=(0..20).map(|_|{let code=auth.issue().unwrap();auth.exchange(&code.code,"Same phone label",true).unwrap()}).collect();let ordinary=auth.issue().unwrap();assert!(auth.exchange(&ordinary.code,"extra",true).is_err());let target=auth.devices().unwrap()[0].id.clone();let code=auth.issue_replacing(Some(target)).unwrap();assert!(tokens.iter().all(|t|auth.valid(t)));assert!(auth.exchange("wrong","new",true).is_err());assert!(auth.valid(&tokens[0]));let replacement=auth.exchange(&code.code,"Reinstalled phone",true).unwrap();assert_eq!(auth.devices().unwrap().len(),20);assert!(!auth.valid(&tokens[0]));assert!(tokens[1..].iter().all(|t|auth.valid(t)));drop(auth);let auth=WebAuth::open(Some(p)).unwrap();assert!(auth.valid(&replacement));assert!(tokens[1..].iter().all(|t|auth.valid(t)));assert!(!auth.valid(&tokens[0]));}
 #[test]fn replacement_never_revokes_on_expired_code_or_failed_persistence(){let d=tempfile::tempdir().unwrap();let p=d.path().join("auth.json");let auth=WebAuth::open(Some(p.clone())).unwrap();let code=auth.issue().unwrap();let token=auth.exchange(&code.code,"Old",true).unwrap();let id=auth.devices().unwrap()[0].id.clone();assert!(auth.issue_replacing(Some("wrong-exact-id".into())).is_err());let expired=auth.issue_replacing(Some(id.clone())).unwrap();assert!(auth.exchange_at(&expired.code,"new",true,expired.expires_at).is_err());assert!(auth.valid(&token));let replacement=auth.issue_replacing(Some(id)).unwrap();fs::create_dir(p.with_extension("json.pending")).unwrap();assert!(auth.exchange(&replacement.code,"new",true).is_err());assert!(auth.valid(&token));assert_eq!(auth.devices().unwrap().len(),1);}
}
