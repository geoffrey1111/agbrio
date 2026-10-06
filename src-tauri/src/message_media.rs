use crate::host_application::RouterCore;
use base64ct::{Base64,Encoding};
use serde::{Deserialize,Serialize};
use std::path::Path;
#[derive(Deserialize)]#[serde(tag="kind",rename_all="SCREAMING_SNAKE_CASE",rename_all_fields="camelCase")]
pub(crate) enum MediaScope{Watch{thread_id:String,sequence:Option<i64>,turn_id:Option<String>,item_id:Option<String>},Role{workstream_id:String,role:String,observation_id:String}}
#[derive(Deserialize)]#[serde(rename_all="camelCase")]
pub(crate) struct MediaInput{pub scope:MediaScope,pub source:String,#[serde(default)]pub user_requested:bool}
#[derive(Serialize)]#[serde(rename_all="camelCase")]
pub(crate) struct MediaFile{pub filename:String,pub mime:String,pub data:String}
fn referenced(text:&str,source:&str)->bool{[format!("]({source})"),format!("](<{source}>)"),format!("`{source}`")].iter().any(|v|text.contains(v))||text.lines().any(|l|l.trim()==source)}
pub(crate) fn read(core:&RouterCore,input:MediaInput)->Result<MediaFile,String>{
 let (root,text)=match input.scope{
  MediaScope::Watch{thread_id,sequence,turn_id,item_id}=>{
   let w=core.store.codex_watches()?.into_iter().find(|w|w.thread_id==thread_id).ok_or("MEDIA_SOURCE_MISMATCH")?;
   if let Some(sequence)=sequence{match core.store.codex_watch_event(sequence){Ok(e)=>{if e.thread_id!=thread_id{return Err("MEDIA_SOURCE_MISMATCH".into());}(e.cwd,e.snapshot.text)},Err(e) if e=="WATCH_EVENT_NOT_FOUND"=>public_original(core,&w,&turn_id,&item_id)?,Err(e)=>return Err(e)}}
   else if turn_id.is_some()||item_id.is_some(){let exact=|s:&router_core::store::codex_watch::WatchSnapshot|s.turn_id==turn_id&&s.item_id==item_id;if exact(&w.snapshot){(w.cwd,w.snapshot.text)}else{match core.store.codex_watch_feed(0)?.events.into_iter().find(|e|e.thread_id==thread_id&&exact(&e.snapshot)){Some(e)=>(e.cwd,e.snapshot.text),None=>public_original(core,&w,&turn_id,&item_id)?}}}
   else{(w.cwd,w.snapshot.text)}
  },
  MediaScope::Role{workstream_id,role,observation_id}=>{let state=crate::role_bridge::state(core,&workstream_id)?;let side=match role.as_str(){"DECISION"=>state.bindings.decision,"EXECUTION"=>state.bindings.execution,_=>None}.ok_or("MEDIA_SOURCE_MISMATCH")?;let reply=state.replies.iter().find(|r|r.id==observation_id&&r.endpoint_id==side.endpoint.id).ok_or("MEDIA_SOURCE_MISMATCH")?;(side.cwd.ok_or("MEDIA_ROOT_UNAVAILABLE")?,reply.text.clone())}
 };
 read_file(&root,&text,&input.source,input.user_requested)
}
fn public_original(core:&RouterCore,w:&router_core::store::codex_watch::CodexWatch,turn:&Option<String>,item:&Option<String>)->Result<(String,String),String>{
 let turn=turn.as_deref().ok_or("MEDIA_SOURCE_MISMATCH")?;let item=item.as_deref().ok_or("MEDIA_SOURCE_MISMATCH")?;
 let mut session=core.session.lock().map_err(|_|"Router session unavailable")?;let adapter=session.adapter.as_mut().filter(|a|!a.is_closed()).ok_or("MEDIA_SOURCE_EXPIRED")?;
 crate::watch_chat::metadata(adapter,w)?;
 let text=adapter.read_public_chat_item(&w.thread_id,turn,item)?.ok_or("MEDIA_SOURCE_EXPIRED")?;
 Ok((w.cwd.clone(),text))
}
fn decode_reference(source:&str)->String{
 fn hex(v:u8)->Option<u8>{match v{b'0'..=b'9'=>Some(v-b'0'),b'a'..=b'f'=>Some(v-b'a'+10),b'A'..=b'F'=>Some(v-b'A'+10),_=>None}}
 let mut out=Vec::new();let bytes=source.as_bytes();let mut i=0;while i<bytes.len(){if bytes[i]==b'%'&&i+2<bytes.len(){if let(Some(a),Some(b))=(hex(bytes[i+1]),hex(bytes[i+2])){out.push(a*16+b);i+=3;continue;}}out.push(bytes[i]);i+=1;}String::from_utf8(out).unwrap_or_else(|_|source.into())
}
fn read_file(root:&str,text:&str,source:&str,user_requested:bool)->Result<MediaFile,String>{
 let decoded=decode_reference(source);
 if source.len()>4096||(!referenced(text,source)&&!referenced(text,&decoded)){return Err("MEDIA_SOURCE_MISMATCH".into());}
 let local=decoded.strip_prefix("file:///").unwrap_or(&decoded);
 if local.starts_with("\\\\")||local.starts_with("//")||root.starts_with("\\\\")||root.starts_with("//")||(decoded.starts_with("file://")&&!decoded.starts_with("file:///")){return Err("MEDIA_NONLOCAL_REFERENCE".into());}
 let root=std::fs::canonicalize(root).map_err(|_|"MEDIA_ROOT_UNAVAILABLE")?;
 let raw=decoded.strip_prefix("file:///").unwrap_or(&decoded);let raw=raw.split('#').next().unwrap_or(raw);let raw=raw.rsplit_once(':').filter(|(_,line)|!line.is_empty()&&line.chars().all(|c|c.is_ascii_digit())).map(|(path,_)|path).unwrap_or(raw);let raw=Path::new(raw);
 let path=std::fs::canonicalize(if raw.is_absolute(){raw.to_owned()}else{root.join(raw)}).map_err(|_|"MEDIA_FILE_UNAVAILABLE")?;
 let ext=path.extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase();
 // A source-declared raster is previewable even when its original cwd differs.
 if !path.is_file()||(!path.starts_with(&root)&&!user_requested&&!matches!(ext.as_str(),"png"|"jpg"|"jpeg"|"gif"|"webp")){return Err("MEDIA_OUTSIDE_PROJECT".into());}
 let meta=std::fs::metadata(&path).map_err(|_|"MEDIA_FILE_UNAVAILABLE")?;if meta.len()>8*1024*1024{return Err("MEDIA_TOO_LARGE".into());}
 let bytes=std::fs::read(&path).map_err(|_|"MEDIA_FILE_UNAVAILABLE")?;if bytes.len()>8*1024*1024{return Err("MEDIA_TOO_LARGE".into());}
 let mime=match ext.as_str(){"png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n")=>"image/png","jpg"|"jpeg" if bytes.starts_with(&[255,216,255])=>"image/jpeg","gif" if bytes.starts_with(b"GIF87a")||bytes.starts_with(b"GIF89a")=>"image/gif","webp" if bytes.starts_with(b"RIFF")&&bytes.get(8..12)==Some(b"WEBP")=>"image/webp","pdf"=>"application/pdf","md"|"txt"|"csv"|"json"|"zip"|"docx"|"xlsx"|"svg"=>"application/octet-stream",_=>return Err("MEDIA_TYPE_UNAVAILABLE".into())};
 Ok(MediaFile{filename:path.file_name().unwrap().to_string_lossy().into(),mime:mime.into(),data:Base64::encode_string(&bytes)})
}
#[cfg(test)]mod tests{use super::*;#[test]fn only_exact_referenced_project_media_is_returned(){let d=tempfile::tempdir().unwrap();let root=d.path().to_str().unwrap();std::fs::write(d.path().join("image.png"),b"\x89PNG\r\n\x1a\npublic fixture").unwrap();assert!(read_file(root,"![图](image.png)","image.png",false).is_ok());assert!(read_file(root,"![图](other.png)","image.png",false).is_err());let outside=tempfile::tempdir().unwrap();let path=outside.path().join("secret.png");std::fs::write(&path,b"\x89PNG\r\n\x1a\n").unwrap();let raw=path.to_string_lossy();assert!(read_file(root,&format!("![图]({raw})"),&raw,false).is_ok());assert!(read_file(root,&format!("![图]({raw})"),&raw,true).is_ok());assert!(read_file(root,"unrelated text",&raw,true).is_err());let doc=outside.path().join("report.txt");std::fs::write(&doc,b"public document").unwrap();let r=doc.to_string_lossy();assert!(read_file(root,&format!("[文件]({r})"),&r,false).is_err());assert!(read_file(root,&format!("[文件]({r})"),&r,true).is_ok());std::fs::write(d.path().join("bad.png"),b"not an image").unwrap();assert!(read_file(root,"![图](bad.png)","bad.png",false).is_err());}}

#[cfg(test)]mod reference_tests{use super::*;#[test]fn encoded_chinese_spaces_and_malformed_percent_are_lossless_and_non_panicking(){assert_eq!(decode_reference("%E5%9B%BE%20a.png"),"图 a.png");assert_eq!(decode_reference("100%图.png"),"100%图.png");let d=tempfile::tempdir().unwrap();std::fs::write(d.path().join("图 a.png"),b"\x89PNG\r\n\x1a\n").unwrap();assert!(read_file(d.path().to_str().unwrap(),"![图](<图 a.png>)","%E5%9B%BE%20a.png",false).is_ok());}}

#[cfg(test)]mod nonlocal_tests{use super::*;#[test]fn a_source_reference_cannot_trigger_network_share_authentication(){let root=tempfile::tempdir().unwrap();let source=r"\\remote.invalid\share\image.png";assert_eq!(read_file(root.path().to_str().unwrap(),&format!("![图]({source})"),source,false).err().as_deref(),Some("MEDIA_NONLOCAL_REFERENCE"));}}
