use crate::host_application::RouterCore;
use base64ct::{Base64,Encoding};
use serde::{Deserialize,Serialize};
use std::path::Path;
#[derive(Deserialize)]#[serde(tag="kind",rename_all="SCREAMING_SNAKE_CASE",rename_all_fields="camelCase")]
pub(crate) enum MediaScope{Watch{thread_id:String,sequence:Option<i64>,turn_id:Option<String>,item_id:Option<String>},Role{workstream_id:String,role:String,observation_id:String}}
#[derive(Deserialize)]#[serde(rename_all="camelCase")]
pub(crate) struct MediaInput{pub scope:MediaScope,pub source:String,#[serde(default)]pub user_requested:bool,#[serde(default)]pub document_sources:Vec<String>}
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
 read_documents(root,text,&input.document_sources,&input.source,input.user_requested)
}
fn read_documents(mut root:String,mut text:String,documents:&[String],source:&str,user_requested:bool)->Result<MediaFile,String>{
 if documents.len()>8{return Err("MEDIA_SOURCE_MISMATCH".into());}
 for document in documents{
 let file=read_file(&root,&text,document,true)?;
 if !file.filename.to_ascii_lowercase().ends_with(".md") {return Err("MEDIA_TYPE_UNAVAILABLE".into());}
 let bytes=Base64::decode_vec(&file.data).map_err(|_|"MEDIA_FILE_UNAVAILABLE")?;
 let content=String::from_utf8(bytes).map_err(|_|"MEDIA_TYPE_UNAVAILABLE")?;
 let decoded=decode_reference(document);let raw=local_reference(&decoded);
 let path=Path::new(raw);let path=std::fs::canonicalize(if path.is_absolute(){path.to_owned()}else{Path::new(&root).join(path)}).map_err(|_|"MEDIA_FILE_UNAVAILABLE")?;
 root=path.parent().ok_or("MEDIA_ROOT_UNAVAILABLE")?.to_string_lossy().into_owned();
 // canonicalize returns a verbatim drive path on Windows; retain local-drive semantics.
 if let Some(local)=root.strip_prefix(r"\\?\"){if !local.starts_with("UNC"){root=local.to_owned();}}
 text=content;
 }
 read_file(&root,&text,&source,user_requested)
}
fn public_original(core:&RouterCore,w:&router_core::store::codex_watch::CodexWatch,turn:&Option<String>,item:&Option<String>)->Result<(String,String),String>{
 let turn=turn.as_deref().ok_or("MEDIA_SOURCE_MISMATCH")?;let item=item.as_deref().ok_or("MEDIA_SOURCE_MISMATCH")?;
 if let Ok(sid)=router_core::store::claude_manual::native_id(&w.thread_id){
 let path=crate::claude_transcript::projects_root().and_then(|r|crate::claude_transcript::find(&r,sid)).ok_or("MEDIA_SOURCE_EXPIRED")?;
 let text=transcript_item(&path,sid,&w.cwd,turn,item)?;
 return Ok((w.cwd.clone(),text));
 }
 let mut session=core.session.lock().map_err(|_|"Router session unavailable")?;let adapter=session.adapter.as_mut().filter(|a|!a.is_closed()).ok_or("MEDIA_SOURCE_EXPIRED")?;
 crate::watch_chat::metadata(adapter,w)?;
 let text=adapter.read_public_chat_item(&w.thread_id,turn,item)?.ok_or("MEDIA_SOURCE_EXPIRED")?;
 Ok((w.cwd.clone(),text))
}
fn transcript_item(path:&Path,sid:&str,cwd:&str,turn:&str,item:&str)->Result<String,String>{
 let mut before=None;
 for _ in 0..32{
 let page=crate::claude_transcript::page(path,sid,before).map_err(|_|"MEDIA_SOURCE_EXPIRED")?;
 if !page.cwd.as_ref().is_some_and(|v|router_core::claude_code::same_root(v,cwd)){return Err("MEDIA_SOURCE_MISMATCH".into());}
 if let Some(message)=page.turns.iter().find(|t|t.id==turn).and_then(|t|t.messages.iter().find(|m|m.id==item)){return Ok(message.text.clone());}
 match page.next{Some(next) if Some(next)!=before=>before=Some(next),_=>break}
 }
 Err("MEDIA_SOURCE_EXPIRED".into())
}
fn decode_reference(source:&str)->String{
 fn hex(v:u8)->Option<u8>{match v{b'0'..=b'9'=>Some(v-b'0'),b'a'..=b'f'=>Some(v-b'a'+10),b'A'..=b'F'=>Some(v-b'A'+10),_=>None}}
 let mut out=Vec::new();let bytes=source.as_bytes();let mut i=0;while i<bytes.len(){if bytes[i]==b'%'&&i+2<bytes.len(){if let(Some(a),Some(b))=(hex(bytes[i+1]),hex(bytes[i+2])){out.push(a*16+b);i+=3;continue;}}out.push(bytes[i]);i+=1;}String::from_utf8(out).unwrap_or_else(|_|source.into())
}
fn local_reference(decoded:&str)->&str{
 let raw=decoded.strip_prefix("file:///").unwrap_or(decoded).split('#').next().unwrap_or(decoded);
 fn strip(v:&str)->Option<&str>{v.rsplit_once(':').filter(|(_,n)|!n.is_empty()&&n.chars().all(|c|c.is_ascii_digit())).map(|(p,_)|p)}
 // Strip at most two numeric suffixes, never the drive separator.
 let first=strip(raw).unwrap_or(raw);strip(first).unwrap_or(first)
}
fn read_file(root:&str,text:&str,source:&str,user_requested:bool)->Result<MediaFile,String>{
 let decoded=decode_reference(source);
 if source.len()>4096||(!referenced(text,source)&&!referenced(text,&decoded)){return Err("MEDIA_SOURCE_MISMATCH".into());}
 let local=decoded.strip_prefix("file:///").unwrap_or(&decoded);
 if local.starts_with("\\\\")||local.starts_with("//")||root.starts_with("\\\\")||root.starts_with("//")||(decoded.starts_with("file://")&&!decoded.starts_with("file:///")){return Err("MEDIA_NONLOCAL_REFERENCE".into());}
 let root=std::fs::canonicalize(root).map_err(|_|"MEDIA_ROOT_UNAVAILABLE")?;
 let raw=Path::new(local_reference(&decoded));
 let path=std::fs::canonicalize(if raw.is_absolute(){raw.to_owned()}else{root.join(raw)}).map_err(|_|"MEDIA_FILE_UNAVAILABLE")?;
 let ext=path.extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase();
 // A source-declared raster is previewable even when its original cwd differs.
 if !path.is_file()||(!path.starts_with(&root)&&!user_requested&&!matches!(ext.as_str(),"png"|"jpg"|"jpeg"|"gif"|"webp")){return Err("MEDIA_OUTSIDE_PROJECT".into());}
 let meta=std::fs::metadata(&path).map_err(|_|"MEDIA_FILE_UNAVAILABLE")?;if meta.len()>8*1024*1024{return Err("MEDIA_TOO_LARGE".into());}
 let bytes=std::fs::read(&path).map_err(|_|"MEDIA_FILE_UNAVAILABLE")?;if bytes.len()>8*1024*1024{return Err("MEDIA_TOO_LARGE".into());}
 let mime=match ext.as_str(){"png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n")=>"image/png","jpg"|"jpeg" if bytes.starts_with(&[255,216,255])=>"image/jpeg","gif" if bytes.starts_with(b"GIF87a")||bytes.starts_with(b"GIF89a")=>"image/gif","webp" if bytes.starts_with(b"RIFF")&&bytes.get(8..12)==Some(b"WEBP")=>"image/webp","pdf"=>"application/pdf","md"|"txt"|"csv"|"json"|"rs"|"ts"|"tsx"|"js"|"jsx"|"py"|"cs"|"css"|"html"|"yaml"|"yml"|"toml"|"log"|"xml"|"sql"|"zip"|"docx"|"xlsx"|"svg"=>"application/octet-stream",_=>return Err("MEDIA_TYPE_UNAVAILABLE".into())};
 Ok(MediaFile{filename:path.file_name().unwrap().to_string_lossy().into(),mime:mime.into(),data:Base64::encode_string(&bytes)})
}
#[cfg(test)]mod tests{use super::*;#[test]fn only_exact_referenced_project_media_is_returned(){let d=tempfile::tempdir().unwrap();let root=d.path().to_str().unwrap();std::fs::write(d.path().join("image.png"),b"\x89PNG\r\n\x1a\npublic fixture").unwrap();assert!(read_file(root,"![图](image.png)","image.png",false).is_ok());assert!(read_file(root,"![图](other.png)","image.png",false).is_err());let outside=tempfile::tempdir().unwrap();let path=outside.path().join("secret.png");std::fs::write(&path,b"\x89PNG\r\n\x1a\n").unwrap();let raw=path.to_string_lossy();assert!(read_file(root,&format!("![图]({raw})"),&raw,false).is_ok());assert!(read_file(root,&format!("![图]({raw})"),&raw,true).is_ok());assert!(read_file(root,"unrelated text",&raw,true).is_err());let doc=outside.path().join("report.txt");std::fs::write(&doc,b"public document").unwrap();let r=doc.to_string_lossy();assert!(read_file(root,&format!("[文件]({r})"),&r,false).is_err());assert!(read_file(root,&format!("[文件]({r})"),&r,true).is_ok());std::fs::write(d.path().join("bad.png"),b"not an image").unwrap();assert!(read_file(root,"![图](bad.png)","bad.png",false).is_err());}}

#[cfg(test)]mod reference_tests{use super::*;#[test]fn encoded_chinese_spaces_and_malformed_percent_are_lossless_and_non_panicking(){assert_eq!(decode_reference("%E5%9B%BE%20a.png"),"图 a.png");assert_eq!(decode_reference("100%图.png"),"100%图.png");let d=tempfile::tempdir().unwrap();std::fs::write(d.path().join("图 a.png"),b"\x89PNG\r\n\x1a\n").unwrap();assert!(read_file(d.path().to_str().unwrap(),"![图](<图 a.png>)","%E5%9B%BE%20a.png",false).is_ok());}}

#[cfg(test)]mod nonlocal_tests{use super::*;#[test]fn a_source_reference_cannot_trigger_network_share_authentication(){let root=tempfile::tempdir().unwrap();let source=r"\\remote.invalid\share\image.png";assert_eq!(read_file(root.path().to_str().unwrap(),&format!("![图]({source})"),source,false).err().as_deref(),Some("MEDIA_NONLOCAL_REFERENCE"));}}

#[cfg(test)]mod document_tests{
 use super::*;
 #[test]fn nested_markdown_is_read_only_exact_and_relative_to_its_document(){
 let d=tempfile::tempdir().unwrap();std::fs::create_dir(d.path().join("docs")).unwrap();std::fs::write(d.path().join("docs/guide.md"),"[next](next.md)\n[picture](pic.png)").unwrap();std::fs::write(d.path().join("docs/next.md"),"[data](data.csv)").unwrap();std::fs::write(d.path().join("docs/data.csv"),"name,value\na,1").unwrap();
 let root=d.path().to_string_lossy().into_owned();let original="[guide](docs/guide.md)".to_string();
 let docs=vec!["docs/guide.md".into(),"next.md".into()];
 assert_eq!(read_documents(root.clone(),original.clone(),&docs,"data.csv",true).unwrap().filename,"data.csv");
 assert!(read_documents(root.clone(),original.clone(),&docs,"secret.csv",true).is_err());
 assert!(read_documents(root,"unrelated".into(),&docs,"data.csv",true).is_err());
 }
 #[test]fn line_and_column_suffixes_do_not_change_source_authority(){let d=tempfile::tempdir().unwrap();std::fs::write(d.path().join("code.ts"),"const value = 1;").unwrap();let root=d.path().to_str().unwrap();assert!(read_file(root,"[code](code.ts:12:3)","code.ts:12:3",true).is_ok());assert_eq!(local_reference("C:/work/code.ts:12:3"),"C:/work/code.ts");assert!(read_file(root,"no link","code.ts:12:3",true).is_err());}
}

#[cfg(test)]mod transcript_tests{
 use super::*;use serde_json::json;
 #[test]fn older_native_claude_item_requires_exact_session_turn_item_and_root(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("transcript.jsonl");let sid="10000000-0000-4000-8000-000000000001";
 let mut rows=vec![];for n in 0..8{rows.push(json!({"type":"user","sessionId":sid,"promptId":format!("p{n}"),"uuid":format!("u{n}"),"cwd":d.path().to_str().unwrap(),"message":{"role":"user","content":"question"}}).to_string());rows.push(json!({"type":"assistant","sessionId":sid,"cwd":d.path().to_str().unwrap(),"message":{"id":format!("m{n}"),"content":[{"type":"text","text":format!("[document](report{n}.md)")}]}}).to_string());}
 std::fs::write(&path,rows.join("\n")+"\n").unwrap();
 assert_eq!(transcript_item(&path,sid,d.path().to_str().unwrap(),"claude_turn:p0","claude_msg:m0").unwrap(),"[document](report0.md)");
 assert!(transcript_item(&path,sid,"X:/foreign","claude_turn:p0","claude_msg:m0").is_err());assert!(transcript_item(&path,sid,d.path().to_str().unwrap(),"claude_turn:p1","claude_msg:m0").is_err());
 }
}
