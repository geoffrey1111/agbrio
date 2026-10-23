//! Explicit owner-local publication of bounded, manifest-listed presentation files.
//! No cloud upload, arbitrary proxy, provider call or native execution.
use base64ct::{Base64,Encoding};
use serde::{Serialize,Deserialize};
use sha2::{Digest,Sha256};
use std::{collections::{BTreeMap,BTreeSet},fs,io::Read,path::{Path,PathBuf,Component},sync::Mutex};

const MAX_FILE:u64=8*1024*1024;
const MAX_TOTAL:u64=32*1024*1024;
static LOCK:Mutex<()>=Mutex::new(());
static CACHE:Mutex<Option<(PathBuf,String,Loaded)>>=Mutex::new(None);
#[cfg(test)]static TEST_STORE:Mutex<Option<PathBuf>>=Mutex::new(None);
#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Manifest{schema_version:u32,title:String,entry:String,files:Vec<String>}
#[derive(Clone,Serialize,Deserialize)]
struct Registration{id:String,root:PathBuf}
#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct BoardInfo{pub id:String,pub title:String,pub revision:String,pub available:bool}
#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Reference{pub path:String,pub id:String,pub mime:String}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Snapshot{pub board:BoardInfo,pub html:Option<String>,pub assets:Vec<Reference>,pub inline:BTreeMap<String,String>}
#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Asset{pub mime:String,pub data:String,pub filename:String,pub asset_id:String,pub content_hash:String,pub source_hash:String}
#[derive(Clone)]
struct Loaded{info:BoardInfo,html:String,assets:Vec<Reference>,inline:BTreeMap<String,String>,files:BTreeMap<String,PathBuf>}
fn hash(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
fn error(_:impl std::fmt::Debug)->String{"PUBLISHED_BOARD_UNAVAILABLE".into()}
fn store_path()->Result<PathBuf,String>{
 #[cfg(test)]{return TEST_STORE.lock().map_err(error)?.clone().ok_or_else(||"PUBLISHED_BOARD_TEST_STORE_UNSET".into());}
 #[cfg(not(test))]{std::env::var_os("LOCALAPPDATA").map(|v|PathBuf::from(v).join("AIWorkRouter/data/published-boards.json")).ok_or_else(||"PUBLISHED_BOARD_STORAGE_UNAVAILABLE".into())}
}
fn registrations(path:&Path)->Result<Vec<Registration>,String>{
 if !path.exists(){return Ok(vec![])}
 let bytes=read(path,64*1024)?;let rows:Vec<Registration>=serde_json::from_slice(&bytes).map_err(error)?;
 if rows.len()>32{return Err("PUBLISHED_BOARD_LIMIT".into())}Ok(rows)
}
fn read(path:&Path,limit:u64)->Result<Vec<u8>,String>{
 crate::character_board::no_reparse(path)?;
 let file=fs::File::open(path).map_err(error)?;let meta=file.metadata().map_err(error)?;
 if !meta.is_file()||meta.len()>limit{return Err("PUBLISHED_BOARD_FILE_LIMIT".into())}
 let mut bytes=vec![];file.take(limit+1).read_to_end(&mut bytes).map_err(error)?;
 if bytes.len() as u64>limit{return Err("PUBLISHED_BOARD_FILE_LIMIT".into())}
 crate::character_board::no_reparse(path)?;Ok(bytes)
}
fn relative(root:&Path,name:&str)->Result<PathBuf,String>{
 if name.is_empty()||name.len()>1024||name.contains(['\\',':','?','#','%','\0'])||name.split('/').any(|s|s.is_empty()||s.starts_with('.'))||Path::new(name).components().any(|c|!matches!(c,Component::Normal(_))){return Err("PUBLISHED_BOARD_PATH_REFUSED".into())}
 let path=root.join(name);crate::character_board::no_reparse(&path)?;
 let canonical=fs::canonicalize(path).map_err(error)?;
 if !canonical.starts_with(root){return Err("PUBLISHED_BOARD_PATH_REFUSED".into())}Ok(canonical)
}
fn local_root(root:&Path)->Result<(),String>{let text=root.to_string_lossy();let bytes=text.as_bytes();let extended_local=text.starts_with("\\\\?\\")&&bytes.get(4).is_some_and(u8::is_ascii_alphabetic)&&bytes.get(5)==Some(&b':');if !root.is_absolute()||text.starts_with("//")||(text.starts_with("\\\\")&&!extended_local){return Err("PUBLISHED_BOARD_PATH_REFUSED".into())}Ok(())}
fn mime(name:&str)->Option<&'static str>{Some(match name.rsplit('.').next()?.to_ascii_lowercase().as_str(){
 "html"=>"text/html","css"=>"text/css","js"=>"text/javascript","json"=>"application/json","md"|"txt"|"csv"=>"text/plain;charset=utf-8",
 "png"=>"image/png","jpg"|"jpeg"=>"image/jpeg","gif"=>"image/gif","webp"=>"image/webp",_=>return None})}
fn load(row:&Registration)->Result<Loaded,String>{
 local_root(&row.root)?;
 crate::character_board::no_reparse(&row.root)?;let root=fs::canonicalize(&row.root).map_err(error)?;
 let manifest_bytes=read(&root.join("agbrio-board.json"),64*1024)?;
 let manifest:Manifest=serde_json::from_slice(&manifest_bytes).map_err(|_|"PUBLISHED_BOARD_MANIFEST_INVALID")?;
 if manifest.schema_version!=1||manifest.title.trim().is_empty()||manifest.title.chars().count()>80||manifest.title.chars().any(char::is_control)||manifest.files.is_empty()||manifest.files.len()>256||manifest.entry!="index.html"{return Err("PUBLISHED_BOARD_MANIFEST_INVALID".into())}
 let names:BTreeSet<_>=manifest.files.iter().collect();
 if names.len()!=manifest.files.len()||!names.contains(&manifest.entry){return Err("PUBLISHED_BOARD_MANIFEST_INVALID".into())}
 let mut stamp=Sha256::new();stamp.update(&manifest_bytes);
 for name in &names{let path=relative(&root,name)?;let meta=fs::metadata(path).map_err(error)?;if !meta.is_file()||meta.len()>MAX_FILE{return Err("PUBLISHED_BOARD_FILE_LIMIT".into())}stamp.update(format!("{name}:{}:{:?}",meta.len(),meta.modified().map_err(error)?).as_bytes());}
 let source_stamp=format!("{:x}",stamp.finalize());
 if let Ok(cache)=CACHE.lock(){if let Some((cached_root,cached_stamp,loaded))=&*cache{if cached_root==&root&&cached_stamp==&source_stamp{return Ok(loaded.clone());}}}
 let mut total=0u64;let mut text_total=0usize;let mut assets=vec![];let mut files=BTreeMap::new();let mut inline=BTreeMap::new();let mut html=None;let mut revision=Sha256::new();
 revision.update(manifest.title.as_bytes());revision.update([0]);
 for name in names.iter().copied(){
  let mime=mime(name).ok_or("PUBLISHED_BOARD_TYPE_REFUSED")?;let path=relative(&root,name)?;let bytes=read(&path,MAX_FILE)?;
  total+=bytes.len() as u64;if total>MAX_TOTAL{return Err("PUBLISHED_BOARD_TOTAL_LIMIT".into())}
  if name==&manifest.entry||matches!(mime,"text/css"|"text/javascript"){text_total+=bytes.len();if text_total>1024*1024{return Err("PUBLISHED_BOARD_DOCUMENT_LIMIT".into())}}
  let content_hash=hash(&bytes);let id=hash(format!("{name}\0{content_hash}").as_bytes());
  revision.update(name.as_bytes());revision.update([0]);revision.update(id.as_bytes());
  if name==&manifest.entry{if bytes.len()>1024*1024{return Err("PUBLISHED_BOARD_FILE_LIMIT".into())}html=Some(String::from_utf8(bytes.clone()).map_err(error)?);}
  if matches!(mime,"text/css"|"text/javascript"){if bytes.len()>1024*1024{return Err("PUBLISHED_BOARD_FILE_LIMIT".into())}inline.insert(name.clone(),String::from_utf8(bytes).map_err(error)?);}
  assets.push(Reference{path:name.clone(),id:id.clone(),mime:mime.into()});files.insert(id,path);
 }
 let mut check=Sha256::new();check.update(read(&root.join("agbrio-board.json"),64*1024)?);
 for name in &names{let meta=fs::metadata(relative(&root,name)?).map_err(error)?;check.update(format!("{name}:{}:{:?}",meta.len(),meta.modified().map_err(error)?).as_bytes());}
 if format!("{:x}",check.finalize())!=source_stamp{return Err("PUBLISHED_BOARD_SOURCE_UPDATING".into())}
 if html.as_ref().is_none_or(|s|s.trim().is_empty()){return Err("PUBLISHED_BOARD_SOURCE_UPDATING".into())}
 let loaded=Loaded{info:BoardInfo{id:row.id.clone(),title:manifest.title,revision:format!("{:x}",revision.finalize()),available:true},html:html.ok_or("PUBLISHED_BOARD_MANIFEST_INVALID")?,assets,inline,files};
 if let Ok(mut cache)=CACHE.lock(){*cache=Some((root,source_stamp,loaded.clone()));}Ok(loaded)
}
fn publish_at(store:&Path,folder:&str)->Result<BoardInfo,String>{
 let _guard=LOCK.lock().map_err(error)?;let input=PathBuf::from(folder);
 if !input.is_absolute()||folder.starts_with("\\\\")||folder.starts_with("//"){return Err("PUBLISHED_BOARD_PATH_REFUSED".into())}
 crate::character_board::no_reparse(&input)?;let root=fs::canonicalize(input).map_err(error)?;
 let id=hash(root.to_string_lossy().as_bytes())[..32].to_string();let row=Registration{id:id.clone(),root};let loaded=load(&row)?;
 let mut rows=registrations(store)?;if !rows.iter().any(|r|r.id==id){if rows.len()>=32{return Err("PUBLISHED_BOARD_LIMIT".into())}rows.push(row);}
 if let Some(parent)=store.parent(){fs::create_dir_all(parent).map_err(error)?;}
 let temporary=store.with_extension("json.tmp");fs::write(&temporary,serde_json::to_vec(&rows).map_err(error)?).map_err(error)?;fs::rename(temporary,store).map_err(error)?;
 Ok(loaded.info)
}
pub(crate) fn publish(folder:&str)->Result<BoardInfo,String>{publish_at(&store_path()?,folder)}
fn row_at(store:&Path,id:&str)->Result<Registration,String>{if id.len()!=32||!id.bytes().all(|b|b.is_ascii_hexdigit()&&!b.is_ascii_uppercase()){return Err("PUBLISHED_BOARD_ID_INVALID".into())}registrations(store)?.into_iter().find(|r|r.id==id).ok_or_else(||"PUBLISHED_BOARD_NOT_FOUND".into())}
pub(crate) fn list()->Result<Vec<BoardInfo>,String>{Ok(registrations(&store_path()?)?.iter().map(|r|{let manifest=local_root(&r.root).ok().and_then(|_|read(&r.root.join("agbrio-board.json"),64*1024).ok()).and_then(|b|serde_json::from_slice::<Manifest>(&b).ok());BoardInfo{id:r.id.clone(),title:manifest.as_ref().filter(|m|!m.title.is_empty()&&m.title.chars().count()<=80).map(|m|m.title.clone()).unwrap_or_else(||"Board".into()),revision:String::new(),available:manifest.is_some()}}).collect())}
pub(crate) fn snapshot(id:&str,known:Option<&str>)->Result<Snapshot,String>{let loaded=load(&row_at(&store_path()?,id)?)?;let changed=known!=Some(loaded.info.revision.as_str());Ok(Snapshot{board:loaded.info,html:changed.then_some(loaded.html),assets:if changed{loaded.assets}else{vec![]},inline:if changed{loaded.inline}else{BTreeMap::new()}})}
fn asset_at(store:&Path,id:&str,revision:&str,asset_id:&str)->Result<Asset,String>{
 let loaded=load(&row_at(store,id)?)?;if loaded.info.revision!=revision{return Err("PUBLISHED_BOARD_REVISION_CHANGED".into())}
 let reference=loaded.assets.iter().find(|a|a.id==asset_id).ok_or("PUBLISHED_BOARD_ASSET_REFUSED")?;
 let bytes=read(loaded.files.get(asset_id).ok_or("PUBLISHED_BOARD_ASSET_REFUSED")?,MAX_FILE)?;let content_hash=hash(&bytes);
 if hash(format!("{}\0{}",reference.path,content_hash).as_bytes())!=asset_id{return Err("PUBLISHED_BOARD_SOURCE_CHANGED".into())}
 Ok(Asset{mime:reference.mime.clone(),data:Base64::encode_string(&bytes),filename:reference.path.clone(),asset_id:asset_id.into(),source_hash:content_hash.clone(),content_hash})
}
pub(crate) fn asset(id:&str,revision:&str,asset_id:&str)->Result<Asset,String>{asset_at(&store_path()?,id,revision,asset_id)}
pub(crate) const FRAME_LOADER:&str=include_str!("../../src/features/character-board/published-board-host.html");
pub(crate) const HOST_CSP:&str="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; frame-src 'self' about: blob: data:; connect-src 'none'; form-action 'none'; base-uri 'none'; sandbox allow-scripts";

#[cfg(all(test,windows,feature="native-board-acceptance"))]
mod native_acceptance;

#[cfg(test)]mod tests{
 use super::*;
 fn fixture()->(tempfile::TempDir,PathBuf,PathBuf){let temp=tempfile::tempdir().unwrap();let folder=temp.path().join("board");fs::create_dir(&folder).unwrap();let store=temp.path().join("registry.json");fs::write(folder.join("index.html"),"<h1>Result</h1><img src='plot.png'>").unwrap();fs::write(folder.join("plot.png"),b"fixture pixels").unwrap();fs::write(folder.join("private.txt"),b"NOT_PUBLISHED").unwrap();fs::write(folder.join("agbrio-board.json"),r#"{"schemaVersion":1,"title":"Results","entry":"index.html","files":["index.html","plot.png"]}"#).unwrap();(temp,folder,store)}
 #[test]fn registration_is_idempotent_and_only_manifest_files_cross_boundary(){let(_t,folder,store)=fixture();let first=publish_at(&store,folder.to_str().unwrap()).unwrap();let second=publish_at(&store,folder.to_str().unwrap()).unwrap();assert_eq!(first.id,second.id);assert_eq!(registrations(&store).unwrap().len(),1);let loaded=load(&row_at(&store,&first.id).unwrap()).unwrap();assert_eq!(loaded.assets.len(),2);assert!(!serde_json::to_string(&loaded.assets).unwrap().contains("private.txt"));let a=loaded.assets.iter().find(|a|a.path=="plot.png").unwrap();assert!(asset_at(&store,&first.id,&first.revision,&a.id).is_ok());assert!(asset_at(&store,&"f".repeat(32),&first.revision,&a.id).is_err());}
 #[test]fn updates_preserve_unchanged_image_ids_and_reject_old_revisions(){let(_t,folder,store)=fixture();let first=publish_at(&store,folder.to_str().unwrap()).unwrap();let a=load(&row_at(&store,&first.id).unwrap()).unwrap();let image=a.assets.iter().find(|a|a.path=="plot.png").unwrap().id.clone();fs::write(folder.join("index.html"),"<h1>New result</h1>").unwrap();let b=load(&row_at(&store,&first.id).unwrap()).unwrap();assert_ne!(first.revision,b.info.revision);assert!(b.assets.iter().any(|a|a.id==image));assert!(asset_at(&store,&first.id,&first.revision,&image).is_err());fs::write(folder.join("plot.png"),b"changed pixels").unwrap();let c=load(&row_at(&store,&first.id).unwrap()).unwrap();assert!(!c.assets.iter().any(|a|a.id==image));}
 #[test]fn paths_active_types_and_unlisted_entry_fail_closed(){let(_t,folder,store)=fixture();for bad in ["../private.txt","/private.txt","sub/../plot.png",".env","file:secret","a%2fsecret","plot.png?x"]{let m=Manifest{schema_version:1,title:"Test".into(),entry:"index.html".into(),files:vec!["index.html".into(),bad.into()]};fs::write(folder.join("agbrio-board.json"),serde_json::to_vec(&m).unwrap()).unwrap();assert!(publish_at(&store,folder.to_str().unwrap()).is_err(),"{bad}");}assert!(!store.exists());}
 #[test]fn declared_entry_executable_document_and_network_limits_fail_before_registration(){let(_t,folder,store)=fixture();let mut m=Manifest{schema_version:1,title:"Test".into(),entry:"index.html".into(),files:vec!["plot.png".into()]};let write=|m:&Manifest|fs::write(folder.join("agbrio-board.json"),serde_json::to_vec(m).unwrap()).unwrap();write(&m);assert!(publish_at(&store,folder.to_str().unwrap()).is_err());m.files=vec!["index.html".into(),"run.exe".into()];fs::write(folder.join("run.exe"),b"not executable").unwrap();write(&m);assert_eq!(publish_at(&store,folder.to_str().unwrap()).err().unwrap(),"PUBLISHED_BOARD_TYPE_REFUSED");m.files=vec!["index.html".into()];write(&m);fs::write(folder.join("index.html"),vec![b'x';1024*1024+1]).unwrap();assert_eq!(publish_at(&store,folder.to_str().unwrap()).err().unwrap(),"PUBLISHED_BOARD_DOCUMENT_LIMIT");assert!(publish_at(&store,r"\\unrelated.invalid\share").is_err());assert!(!store.exists());}
 #[test]fn paired_http_reads_exact_publication_and_revocation_refuses_access(){
  use std::sync::Arc;use crate::mobile_http::{MobileHttpConfig,start_with_web_auth};use crate::host_application::RouterCore;
  let(temp,folder,store)=fixture();let info=publish_at(&store,folder.to_str().unwrap()).unwrap();*TEST_STORE.lock().unwrap()=Some(store);
  struct Reset;impl Drop for Reset{fn drop(&mut self){*TEST_STORE.lock().unwrap()=None;}}let _reset=Reset;
  let core=RouterCore{store:Arc::new(crate::RouterStore::open_at(temp.path().join("router.db")).unwrap()),chatgpt:Arc::default(),session:Arc::new(Mutex::new(Default::default())),completed_chatgpt_responses:Arc::default()};
  let auth=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());let host=crate::HostRuntime::default();let runtime=tokio::runtime::Runtime::new().unwrap();
  let config=MobileHttpConfig{port:0,allowed_host:"boards.fixture.invalid".into(),allowed_origin:"https://boards.fixture.invalid".into(),access_issuer:"https://unused.invalid".into(),access_audience:"unused".into(),access_jwks_url:"https://unused.invalid/keys".into(),static_dir:temp.path().into()};
  let handle=runtime.block_on(start_with_web_auth(core,config,host.clone(),auth.clone())).unwrap();let base=format!("http://{}",handle.address);let client=reqwest::blocking::Client::new();let get=|path:&str,cookie:&str|client.get(format!("{base}/v1/mobile{path}")).header("host","boards.fixture.invalid").header("cookie",cookie).send().unwrap();
  assert_eq!(get("/boards","").status(),401);let code=auth.issue().unwrap();let paired=client.post(format!("{base}/v1/mobile/auth/pair")).header("host","boards.fixture.invalid").header("origin","https://boards.fixture.invalid").json(&serde_json::json!({"code":code.code,"deviceName":"fixture"})).send().unwrap();assert_eq!(paired.status(),200);let cookie=paired.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().to_string();
  let rows:serde_json::Value=get("/boards",&cookie).json().unwrap();assert_eq!(rows[0]["id"],info.id);assert!(!rows.to_string().contains("root"));
  let page:serde_json::Value=get(&format!("/boards/{}",info.id),&cookie).json().unwrap();assert!(page["html"].as_str().unwrap().contains("Result"));let asset=page["assets"].as_array().unwrap().iter().find(|a|a["path"]=="plot.png").unwrap()["id"].as_str().unwrap();
  let image=get(&format!("/boards/{}/assets/{asset}?revision={}",info.id,info.revision),&cookie);assert_eq!(image.status(),200);assert!(image.headers()["cache-control"].to_str().unwrap().contains("no-store"));
  assert_eq!(get(&format!("/boards/{}?knownRevision={}",info.id,info.revision),&cookie).json::<serde_json::Value>().unwrap()["html"],serde_json::Value::Null);
  let denied=client.get(format!("{base}/v1/mobile/boards/{}",info.id)).header("host","boards.fixture.invalid").header("origin","https://other.invalid").header("cookie",&cookie).send().unwrap();assert_eq!(denied.status(),403);
  assert_eq!(client.post(format!("{base}/v1/mobile/boards")).header("host","boards.fixture.invalid").header("cookie",&cookie).json(&serde_json::json!({"folder":"/private"})).send().unwrap().status(),405);
  assert!(auth.devices().unwrap().len()>0);for d in auth.devices().unwrap(){auth.revoke(Some(&d.id),None).unwrap();}assert_eq!(get("/boards",&cookie).status(),401);
 }
}
