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
 if let MediaScope::Watch{thread_id,turn_id,item_id,..}=&input.scope{if thread_id.starts_with("pi:"){if !input.document_sources.is_empty(){return Err("PI_MEDIA_REQUIRES_EXACT_NATIVE_IMAGE".into());}return crate::pi::media::read(core,thread_id,turn_id.as_deref(),item_id.as_deref(),&input.source);}}
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
 let decoded=decode_reference(document.split('#').next().unwrap_or(document));let raw=local_reference(&decoded);
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
 return native_transcript(core,w,&path,turn,item);
 }
 let mut session=core.session.lock().map_err(|_|"Router session unavailable")?;let adapter=session.adapter.as_mut().filter(|a|!a.is_closed()).ok_or("MEDIA_SOURCE_EXPIRED")?;
 crate::watch_chat::metadata(adapter,w)?;
 let text=adapter.read_public_chat_item(&w.thread_id,turn,item)?.ok_or("MEDIA_SOURCE_EXPIRED")?;
 Ok((w.cwd.clone(),text))
}
fn native_transcript(core:&RouterCore,w:&router_core::store::codex_watch::CodexWatch,path:&Path,turn:&str,item:&str)->Result<(String,String),String>{
 let sid=router_core::store::claude_manual::native_id(&w.thread_id)?;
 // The registered adapter session/root is identity. A native shell cd or an
 // assistant's working directory is not a session rebinding.
 crate::claude_code::observed_session(core,sid,Some(&w.cwd)).map_err(|_|"MEDIA_SOURCE_MISMATCH")?;
 Ok((w.cwd.clone(),transcript_item(path,sid,turn,item)?))
}
fn transcript_item(path:&Path,sid:&str,turn:&str,item:&str)->Result<String,String>{
 let mut before=None;
 for _ in 0..32{
 let page=crate::claude_transcript::page(path,sid,before).map_err(|_|"MEDIA_SOURCE_EXPIRED")?;
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
 let raw=decoded.strip_prefix("file:///").unwrap_or(decoded);
 fn strip(v:&str)->Option<&str>{v.rsplit_once(':').filter(|(_,n)|!n.is_empty()&&n.chars().all(|c|c.is_ascii_digit())).map(|(p,_)|p)}
 // Strip at most two numeric suffixes, never the drive separator.
 let first=strip(raw).unwrap_or(raw);strip(first).unwrap_or(first)
}
fn read_file(root:&str,text:&str,source:&str,user_requested:bool)->Result<MediaFile,String>{
 let decoded=decode_reference(source);
 if source.len()>4096||(!referenced(text,source)&&!referenced(text,&decoded)){return Err("MEDIA_SOURCE_MISMATCH".into());}
 // A literal URL fragment is not part of the file, but percent-encoded # is.
 let decoded_path=decode_reference(source.split('#').next().unwrap_or(source));
 let local=decoded_path.strip_prefix("file:///").unwrap_or(&decoded_path);
 if local.starts_with("\\\\")||local.starts_with("//")||root.starts_with("\\\\")||root.starts_with("//")||(decoded.starts_with("file://")&&!decoded.starts_with("file:///")){return Err("MEDIA_NONLOCAL_REFERENCE".into());}
 let root=std::fs::canonicalize(root).map_err(|_|"MEDIA_ROOT_UNAVAILABLE")?;
 let raw=Path::new(local_reference(&decoded_path));
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
 assert_eq!(transcript_item(&path,sid,"claude_turn:p0","claude_msg:m0").unwrap(),"[document](report0.md)");
 assert!(transcript_item(&path,"foreign","claude_turn:p0","claude_msg:m0").is_err());assert!(transcript_item(&path,sid,"claude_turn:p1","claude_msg:m0").is_err());
 }
}

#[cfg(test)]
mod gif_tests {
    use super::*;
    #[test]
    fn presented_gif_bytes_are_preserved_for_animation() {
        let root = tempfile::tempdir().unwrap();
        let bytes = b"GIF89a exact animated bytes";
        std::fs::write(root.path().join("animation.gif"), bytes).unwrap();
        let media = read_file(root.path().to_str().unwrap(), "![animation](<animation.gif>)", "animation.gif", false).unwrap();
        assert_eq!(media.mime, "image/gif");
        assert_eq!(Base64::decode_vec(&media.data).unwrap(), bytes);
        assert!(read_file(root.path().to_str().unwrap(), "other message", "animation.gif", false).is_err());
    }
}

#[cfg(test)]
mod owner_presentation_acceptance {
    use super::*;
    #[test]
    #[ignore = "read-only exact owner transcript/file acceptance; explicit environment required"]
    fn exact_native_file_card_resolves_existing_rasters() {
        let path = std::path::PathBuf::from(std::env::var_os("AGBRIO_MEDIA_TRANSCRIPT").expect("exact transcript"));
        let session = std::env::var("AGBRIO_MEDIA_SESSION").unwrap();
        let item = std::env::var("AGBRIO_MEDIA_ITEM").unwrap();
        let mut before = None;
        for _ in 0..32 {
            let page = crate::claude_transcript::page(&path, &session, before).unwrap();
            if let Some((turn, message)) = page.turns.iter().find_map(|turn| turn.messages.iter().find(|m| m.id == item).map(|m| (turn, m))) {
                let root = page.cwd.as_ref().unwrap();
                let exact = transcript_item(&path, &session, &turn.id, &message.id).unwrap();
                let sources: Vec<_> = exact.split("](<").skip(1).filter_map(|v| v.split_once(">)").map(|(source,_)| source)).collect();
                assert!(!sources.is_empty(), "native file card absent");
                for source in sources {
                    let media = read_file(root, &exact, source, false).unwrap();
                    assert!(media.mime.starts_with("image/"));
                    let bytes = Base64::decode_vec(&media.data).unwrap();
                    assert!(!bytes.is_empty());
                    eprintln!("native presentation accepted: {} bytes, {}", bytes.len(), media.mime);
                }
                return;
            }
            match page.next { Some(next) if Some(next) != before => before = Some(next), _ => break }
        }
        panic!("exact native item not found");
    }
}

#[cfg(test)]
mod native_root_tests {
 use super::*;
 use std::sync::{Arc,Mutex};
 #[test]
 fn shell_directory_changes_do_not_rebind_native_media_identity(){
  let d=tempfile::tempdir().unwrap();let root=d.path().to_string_lossy().into_owned();let sid="10000000-0000-4000-8000-000000000001";
  let store=Arc::new(crate::RouterStore::open_at(d.path().join("router.db")).unwrap());
  let core=RouterCore{chatgpt:Arc::default(),session:Arc::new(Mutex::new(crate::host_application::Session::default())),completed_chatgpt_responses:Arc::default(),store:store.clone()};
  let pair=store.create_claude_connection().unwrap();
  let event=router_core::claude_code::Event{event_id:uuid::Uuid::new_v4().to_string(),session_id:sid.into(),instance_id:"20000000-0000-4000-8000-000000000001".into(),sequence:1,cwd:root.clone(),version:"2.1.309".into(),kind:"SESSION_START".into(),turn_id:None,text:None,completed_at:None};
  store.ingest_claude_event(&pair.token,&event).unwrap();
  let thread=format!("claude_code:{sid}");store.enable_codex_watch(&thread,"fixture",&root,&router_core::store::codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()}).unwrap();
  let w=store.codex_watches().unwrap().into_iter().find(|w|w.thread_id==thread).unwrap();
  let path=d.path().join("transcript.jsonl");let prompt=serde_json::json!({"type":"user","sessionId":sid,"promptId":"p","uuid":"u","cwd":"X:/different-shell-directory","message":{"content":"question"}});let reply=serde_json::json!({"type":"assistant","sessionId":sid,"message":{"id":"m","content":[{"type":"tool_use","name":"SendUserFile","input":{"files":["animation.gif"]}}]}});
  std::fs::write(&path,format!("{prompt}\n{reply}\n")).unwrap();std::fs::write(d.path().join("animation.gif"),b"GIF89a original animation").unwrap();
  let (actual_root,text)=native_transcript(&core,&w,&path,"claude_turn:p","claude_msg:m").unwrap();
  assert_eq!(read_file(&actual_root,&text,"animation.gif",false).unwrap().mime,"image/gif");
  let mut wrong=w.clone();wrong.cwd="X:/foreign".into();assert_eq!(native_transcript(&core,&wrong,&path,"claude_turn:p","claude_msg:m").unwrap_err(),"MEDIA_SOURCE_MISMATCH");
  assert!(native_transcript(&core,&w,&path,"claude_turn:other","claude_msg:m").is_err());
 }
}

#[cfg(test)]
mod native_media_http_tests {
 use super::*;use std::sync::{Arc,Mutex};use serde_json::{json,Value};
 #[test]
 #[ignore="isolated authenticated native HISTORY -> media HTTP; run alone, no provider"]
 fn changed_shell_directory_survives_authenticated_native_history_to_media(){
  let d=tempfile::tempdir().unwrap();let root=d.path().to_string_lossy().into_owned();let sid="10000000-0000-4000-8000-000000000001";
  let store=Arc::new(crate::RouterStore::open_at(d.path().join("router.db")).unwrap());let core=RouterCore{chatgpt:Arc::default(),session:Arc::new(Mutex::new(crate::host_application::Session::default())),completed_chatgpt_responses:Arc::default(),store:store.clone()};
  let pair=store.create_claude_connection().unwrap();let e=router_core::claude_code::Event{event_id:uuid::Uuid::new_v4().to_string(),session_id:sid.into(),instance_id:"20000000-0000-4000-8000-000000000001".into(),sequence:1,cwd:root.clone(),version:"2.1.309".into(),kind:"SESSION_START".into(),turn_id:None,text:None,completed_at:None};store.ingest_claude_event(&pair.token,&e).unwrap();
  let thread=format!("claude_code:{sid}");store.enable_codex_watch(&thread,"fixture",&root,&router_core::store::codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()}).unwrap();
  let projects=d.path().join("projects");let project=projects.join("registered-project");std::fs::create_dir_all(&project).unwrap();let file=d.path().join("图 #1.gif");let bytes=b"GIF89a original animation";std::fs::write(&file,bytes).unwrap();
  let prompt=json!({"type":"user","sessionId":sid,"promptId":"p","uuid":"u","cwd":"X:/different-shell-directory","message":{"content":"question"}});let reply=json!({"type":"assistant","sessionId":sid,"message":{"id":"m","content":[{"type":"tool_use","name":"SendUserFile","input":{"files":[file.to_string_lossy()]}}]}});std::fs::write(project.join(format!("{sid}.jsonl")),format!("{prompt}\n{reply}\n")).unwrap();
  struct Reset;impl Drop for Reset{fn drop(&mut self){*crate::claude_transcript::TEST_PROJECTS.lock().unwrap()=None;}}
  *crate::claude_transcript::TEST_PROJECTS.lock().unwrap()=Some(projects);let _reset=Reset;
  let host=crate::HostRuntime::default();let auth=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());let code=auth.issue().unwrap();let token=auth.exchange(&code.code,"fixture phone",false).unwrap();*host.test_web_auth.lock().unwrap()=Some(auth);
  host.ensure_mobile(core.clone(),crate::mobile_http::MobileHttpConfig{port:0,allowed_host:"media.fixture.invalid".into(),allowed_origin:"https://media.fixture.invalid".into(),access_issuer:String::new(),access_audience:String::new(),access_jwks_url:String::new(),static_dir:d.path().into()});let base=format!("http://{}/v1/mobile",host.mobile_address().unwrap());
  let client=reqwest::blocking::Client::new();let post=|path:&str,body:Value|client.post(format!("{base}{path}")).header("host","media.fixture.invalid").header("origin","https://media.fixture.invalid").header("cookie",format!("{}={token}",crate::web_auth::COOKIE)).json(&body).send().unwrap();
  let response=post("/claude-code/chat",json!({"action":"HISTORY","threadId":thread}));assert!(response.status().is_success());let history:Value=response.json().unwrap();let message=history["messages"].as_array().unwrap().iter().find(|m|m["id"]=="claude_msg:m").unwrap();let text=message["text"].as_str().unwrap();let source=text.split("](<").nth(1).unwrap().split(">)").next().unwrap();let mut body=json!({"scope":{"kind":"WATCH","threadId":thread,"turnId":message["turnId"],"itemId":message["id"]},"source":source,"userRequested":false});
  assert_eq!(client.post(format!("{base}/media")).header("host","media.fixture.invalid").header("origin","https://media.fixture.invalid").json(&body).send().unwrap().status(),reqwest::StatusCode::UNAUTHORIZED);
  let response=post("/media",body.clone());assert!(response.status().is_success(),"{}",response.text().unwrap());let media:Value=response.json().unwrap();assert_eq!(media["mime"],"image/gif");assert_eq!(Base64::decode_vec(media["data"].as_str().unwrap()).unwrap(),bytes);
  body["scope"]["itemId"]=json!("claude_msg:foreign");assert!(!post("/media",body.clone()).status().is_success());body["scope"]["itemId"]=message["id"].clone();body["source"]=json!("unreferenced.gif");assert!(!post("/media",body).status().is_success());assert!(core.session.lock().unwrap().adapter.is_none());host.shutdown(&core);
 }
}

#[cfg(test)]mod fragment_tests{use super::*;#[test]fn encoded_filename_hash_and_literal_line_anchor_remain_distinct(){
 let d=tempfile::tempdir().unwrap();std::fs::write(d.path().join("图 #1.gif"),b"GIF89a exact bytes").unwrap();let source="%E5%9B%BE%20%231.gif";
 assert_eq!(read_file(d.path().to_str().unwrap(),&format!("![图](<{source}>)"),source,false).unwrap().filename,"图 #1.gif");
 std::fs::write(d.path().join("code.ts"),"const value=1;").unwrap();assert!(read_file(d.path().to_str().unwrap(),"[code](code.ts#L12)","code.ts#L12",true).is_ok());
}}
