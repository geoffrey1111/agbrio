//! Read-only projection of the owner's existing Card board. No provider or domain writes.
use base64ct::{Base64, Encoding};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap,VecDeque}, fs, io::{Cursor,Read}, path::{Path, PathBuf}, sync::Mutex, time::UNIX_EPOCH};

const MAX_SOURCE: u64 = 4 * 1024 * 1024;
const MAX_ASSET: u64 = 8 * 1024 * 1024;
#[derive(Clone, Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Snapshot { pub revision: String, pub html: Option<String>, pub assets: Vec<AssetReference> }
#[derive(Clone, Serialize)]
pub(crate) struct AssetReference { pub source: String, pub id: String }
#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Asset { pub mime: String, pub data: String, pub filename: String, pub asset_id:String, pub content_hash:String, pub source_hash:String }
#[derive(Clone)]
struct FileReference { path: PathBuf, stamp: String }
pub(crate) struct Board { root: PathBuf, allowed: Vec<PathBuf>, revision: String, document: String, files: BTreeMap<String, FileReference>, previews:VecDeque<(String,Asset)>,source_stamps:Vec<String>,references:Vec<AssetReference> }
static BOARD: Mutex<Option<Board>> = Mutex::new(None);
const MAX_EXTRA_ROOTS:usize=16;
/// Owner-chosen board source. Only the desktop can change it; the phone reads it.
#[derive(Clone,Debug,PartialEq,serde::Serialize,serde::Deserialize)]
#[serde(tag="kind")]
pub(crate) enum Source{ #[serde(rename="LOCAL")] Local{root:String,#[serde(default,rename="extraRoots")] extra_roots:Vec<String>}, #[serde(rename="URL")] Url{url:String,#[serde(default,rename="desktopUrl",skip_serializing_if="Option::is_none")] desktop_url:Option<String>} }
fn source_path()->Option<PathBuf>{std::env::var_os("LOCALAPPDATA").map(|l|PathBuf::from(l).join("AIWorkRouter").join("data").join("board-source.json"))}
/// None until the owner chooses a board in desktop settings; there is no built-in default.
pub(crate) fn source()->Option<Source>{
 source_path().and_then(|p|fs::read(p).ok()).filter(|b|b.len()<=16*1024).and_then(|b|serde_json::from_slice::<Source>(&b).ok()).and_then(|s|validate(s).ok())
}
fn validate(source:Source)->Result<Source,String>{
 match source{
  Source::Url{url,desktop_url}=>{
   // Phones load the public https page; the desktop may load the same board from this computer.
   let parsed=url::Url::parse(url.trim()).map_err(|_|"BOARD_URL_INVALID")?;
   if parsed.scheme()!="https"||parsed.host_str().is_none()||!parsed.username().is_empty()||parsed.password().is_some()||parsed.as_str().len()>2048{return Err("BOARD_URL_INVALID".into())}
   let desktop=match desktop_url.map(|u|u.trim().to_string()).filter(|u|!u.is_empty()){
    None=>None,
    Some(u)=>{let d=url::Url::parse(&u).map_err(|_|"BOARD_DESKTOP_URL_INVALID")?;
     let loopback=matches!(d.host_str(),Some("127.0.0.1")|Some("localhost")|Some("[::1]"));
     if !(d.scheme()=="https"||(d.scheme()=="http"&&loopback))||!d.username().is_empty()||d.password().is_some()||d.as_str().len()>2048{return Err("BOARD_DESKTOP_URL_INVALID".into())}
     Some(d.to_string())}
   };
   Ok(Source::Url{url:parsed.to_string(),desktop_url:desktop})}
  Source::Local{root,extra_roots}=>{let path=PathBuf::from(root.trim());
   if !path.is_absolute()||!path.join("index.html").is_file(){return Err("BOARD_FOLDER_INVALID".into())}
   no_reparse(&path)?;
   // Extra image folders the board page may reference outside its own folder.
   let mut extras=vec![];for extra in extra_roots.iter().map(|r|r.trim()).filter(|r|!r.is_empty()){let p=PathBuf::from(extra);if !p.is_absolute()||!p.is_dir(){return Err("BOARD_EXTRA_FOLDER_INVALID".into())}no_reparse(&p)?;extras.push(p.to_string_lossy().into_owned());}
   if extras.len()>MAX_EXTRA_ROOTS{return Err("BOARD_EXTRA_FOLDER_INVALID".into())}
   Ok(Source::Local{root:path.to_string_lossy().into_owned(),extra_roots:extras})}
 }
}
/// Saves a validated source and drops the cached projection so the next read uses it.
pub(crate) fn set_source(source:Source)->Result<Source,String>{
 let source=validate(source)?;let path=source_path().ok_or("LOCALAPPDATA is unavailable")?;
 if let Some(dir)=path.parent(){fs::create_dir_all(dir).map_err(error)?;}
 let temp=path.with_extension("json.tmp");fs::write(&temp,serde_json::to_vec(&source).map_err(error)?).map_err(error)?;fs::rename(&temp,&path).map_err(error)?;
 *BOARD.lock().map_err(|_|"BOARD_BUSY")?=None;Ok(source)
}
fn allowed(root:&Path,extras:&[String])->Vec<PathBuf>{
 // The board folder plus the extra image folders the owner listed; nothing else.
 std::iter::once(root.to_path_buf()).chain(extras.iter().map(PathBuf::from)).collect()
}
fn with_board<T>(work:impl FnOnce(&mut Board)->Result<T,String>)->Result<T,String>{
 let Some(source)=source() else {return Err("BOARD_SOURCE_UNSET".into())};
 let Source::Local{root,extra_roots}=source else {return Err("BOARD_SOURCE_IS_URL".into())};
 let root=PathBuf::from(root);let roots=allowed(&root,&extra_roots);let mut guard=BOARD.lock().map_err(|_|"BOARD_BUSY")?;
 if guard.as_ref().is_none_or(|b|b.root!=root||b.allowed!=roots){*guard=Some(Board::new(root.clone(),roots));}
 work(guard.as_mut().expect("board initialized"))
}
pub(crate) fn snapshot(known: Option<&str>) -> Result<Snapshot,String> { with_board(|b|b.snapshot(known)) }
pub(crate) fn asset(revision: &str, id: &str) -> Result<Asset,String> { with_board(|b|b.asset(revision,id)) }
pub(crate) fn display_asset(revision:&str,id:&str,variant:&str)->Result<Asset,String>{with_board(|b|b.display_asset(revision,id,variant))}
pub(crate) const FRAME_CSP:&str="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; connect-src 'none'; frame-src 'none'; form-action 'none'; base-uri 'none'; sandbox allow-scripts";
pub(crate) fn frame(revision:&str,channel:&str)->Result<String,String>{
    if channel.len()!=36||!channel.chars().all(|c|c.is_ascii_hexdigit()||c=='-'){return Err("BOARD_CHANNEL_INVALID".into())}
    let guard=BOARD.lock().map_err(|_|"BOARD_BUSY")?;let board=guard.as_ref().ok_or("BOARD_REVISION_CHANGED")?;
    if board.revision!=revision||board.document.is_empty(){return Err("BOARD_REVISION_CHANGED".into())}
    let boot=include_str!("../../src/features/character-board/board-frame.js");
    let insertion=format!("<head><script data-channel=\"{channel}\">{boot}</script>");
    Ok(board.document.replacen("<head>",&insertion,1))
}

fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn error(_: impl std::fmt::Debug) -> String { "BOARD_FILE_UNAVAILABLE".into() }
pub(crate) fn no_reparse(path: &Path) -> Result<(),String> {
    for part in path.ancestors() {
        let m=fs::symlink_metadata(part).map_err(error)?;
        if m.file_type().is_symlink() { return Err("BOARD_LINK_REFUSED".into()); }
        #[cfg(windows)] { use std::os::windows::fs::MetadataExt; if m.file_attributes() & 0x400 != 0 { return Err("BOARD_LINK_REFUSED".into()); } }
    }
    Ok(())
}
fn bounded(path:&Path,max:u64)->Result<Vec<u8>,String>{
    no_reparse(path)?;
    let f=fs::File::open(path).map_err(error)?;
    if !f.metadata().map_err(error)?.is_file()||f.metadata().map_err(error)?.len()>max { return Err("BOARD_FILE_TOO_LARGE".into()); }
    let mut bytes=Vec::new(); f.take(max+1).read_to_end(&mut bytes).map_err(error)?;
    if bytes.len() as u64>max { return Err("BOARD_FILE_TOO_LARGE".into()); }
    no_reparse(path)?;
    Ok(bytes)
}
fn stamp(path:&Path)->Result<String,String>{
    let m=fs::metadata(path).map_err(error)?;
    Ok(format!("{}:{}",m.len(),m.modified().map_err(error)?.duration_since(UNIX_EPOCH).map_err(error)?.as_nanos()))
}
fn local_path(source:&str)->Option<PathBuf>{
    if source.len()>4096||source.contains('\0') { return None; }
    if source.starts_with("file:///") { let url=url::Url::parse(source).ok()?; if url.host_str().is_some_and(|h|!h.is_empty()){return None} return url.to_file_path().ok(); }
    if source.starts_with("file:")||source.starts_with("\\\\")||source.starts_with("//") { return None; }
    let p=PathBuf::from(source); p.is_absolute().then_some(p)
}
fn references(value:&Value,out:&mut Vec<String>){
    match value { Value::String(s) if local_path(s).is_some()=>out.push(s.clone()),Value::Array(a)=>a.iter().for_each(|v|references(v,out)),Value::Object(o)=>o.values().for_each(|v|references(v,out)),_=>() }
}
fn supported(path:&Path)->bool { matches!(path.extension().and_then(|v|v.to_str()).unwrap_or("").to_ascii_lowercase().as_str(),"png"|"jpg"|"jpeg"|"webp"|"gif"|"json"|"txt"|"md") }
impl Board {
    fn new(root:PathBuf,allowed:Vec<PathBuf>)->Self { Self{root,allowed,revision:String::new(),document:String::new(),files:BTreeMap::new(),previews:VecDeque::new(),source_stamps:Vec::new(),references:Vec::new()} }
    fn approved(&self,source:&str)->Option<PathBuf>{
        let path=local_path(source)?;
        if !supported(&path)||no_reparse(&path).is_err() {return None}
        let canonical=fs::canonicalize(&path).ok()?;
        self.allowed.iter().filter_map(|r|fs::canonicalize(r).ok()).any(|r|canonical.starts_with(r)).then_some(canonical)
    }
    fn snapshot(&mut self,known:Option<&str>)->Result<Snapshot,String>{
        let approval_present=self.root.join("approval-ui.js").is_file();
        let mut source_names=vec!["index.html","style.css","app.js","data/board-data.js"];if approval_present{source_names.push("approval-ui.js");}
        let source_stamps=source_names.iter().map(|name|{let path=self.root.join(name);no_reparse(&path)?;stamp(&path)}).collect::<Result<Vec<_>,String>>()?;
        // Most checks see unchanged files. Avoid rereading megabytes of source,
        // reparsing JSON and repeatedly canonicalizing every historical path.
        // Actual media reads still revalidate roots, reparse points and stamps.
        if !self.document.is_empty()&&source_stamps==self.source_stamps&&self.files.values().all(|f|stamp(&f.path).is_ok_and(|s|s==f.stamp)){
            let changed=known!=Some(self.revision.as_str());
            return Ok(Snapshot{revision:self.revision.clone(),html:changed.then(||self.document.clone()),assets:if changed{self.references.clone()}else{Vec::new()}});
        }
        let load=|name:&str|String::from_utf8(bounded(&self.root.join(name),MAX_SOURCE)?).map_err(error);
        let html=load("index.html")?; let css=load("style.css")?; let js=load("app.js")?; let data=load("data/board-data.js")?;
        if !html.contains("src=\"app.js\"")||!html.contains("src=\"data/board-data.js\"")||!html.contains("href=\"style.css\""){return Err("BOARD_LAYOUT_CHANGED".into())}
        let approval=if approval_present{load("approval-ui.js")?}else{String::new()};
        let json=data.split_once('=').ok_or("BOARD_DATA_INCOMPLETE")?.1.trim().trim_end_matches(';');
        let parsed:Value=serde_json::from_str(json).map_err(|_|"BOARD_DATA_INCOMPLETE")?;
        if !parsed.get("characters").is_some_and(Value::is_array){return Err("BOARD_DATA_INCOMPLETE".into())}
        let mut sources=Vec::new();references(&parsed,&mut sources);
        // Historical links are part of the same board, not a general filesystem grant.
        for part in html.split("file:///").skip(1) {
            let tail=part.split(['\"','\'','<','>','\n','\r',' ']).next().unwrap_or("");
            sources.push(format!("file:///{tail}"));
        }
        sources.push("维护说明.md".into());
        sources.sort();sources.dedup();if sources.len()>4096{return Err("BOARD_REFERENCE_LIMIT".into())}
        let mut files=BTreeMap::new();let mut assets=Vec::new();
        for source in sources { let lookup=if source=="维护说明.md"{self.root.join(&source).to_string_lossy().into_owned()}else{source.clone()};if let Some(path)=self.approved(&lookup) {if let Ok(stamp)=stamp(&path) {
            let id=digest(format!("{}:{stamp}",path.to_string_lossy()).as_bytes());
            files.insert(id.clone(),FileReference{path,stamp});assets.push(AssetReference{source,id});
        }} }
        let mut hash=Sha256::new();for bytes in [&html,&css,&js,&data,&approval]{hash.update(bytes.as_bytes());hash.update([0]);}
        hash.update(include_bytes!("../../src/features/character-board/board-frame.js"));
        for a in &assets {hash.update(a.source.as_bytes());hash.update(a.id.as_bytes());}
        let revision=format!("{:x}",hash.finalize());
        // A producer can rebuild these files while the viewer is reading them.
        // Never replace the last good projection with a partially written batch.
        if source_names.iter().map(|n|stamp(&self.root.join(n))).collect::<Result<Vec<_>,_>>()?!=source_stamps{return Err("BOARD_SOURCE_UPDATING".into())}
        let changed=known!=Some(revision.as_str());
        self.source_stamps=source_stamps;self.references=assets.clone();
        self.revision=revision.clone();self.files=files;
        self.previews.retain(|(_,a)|self.files.contains_key(&a.asset_id));
        let html=html.replace("<link rel=\"stylesheet\" href=\"style.css\">",&format!("<style>{}</style>",css.replace("</style", "<\\/style")))
            .replace("<script src=\"data/board-data.js\"></script>",&format!("<script>{}</script>",data.replace("</script","<\\/script")))
            .replace("<script src=\"app.js\"></script>",&format!("<script>{}</script>",js.replace("</script","<\\/script")));
        let html=if !approval.is_empty(){html.replace("<script src=\"approval-ui.js\"></script>",&format!("<script>{}</script>",approval.replace("</script","<\\/script")))}else{html};
        self.document=html.clone();
        Ok(Snapshot{revision,html:changed.then_some(html),assets:if changed{assets}else{Vec::new()}})
    }
    fn asset(&self,revision:&str,id:&str)->Result<Asset,String>{
        if revision!=self.revision{return Err("BOARD_REVISION_CHANGED".into())}
        let reference=self.files.get(id).ok_or("BOARD_ASSET_NOT_REFERENCED")?;
        let path=&reference.path;
        no_reparse(path)?;
        let canonical=fs::canonicalize(path).map_err(error)?;
        if canonical!=*path||!self.allowed.iter().filter_map(|r|fs::canonicalize(r).ok()).any(|r|canonical.starts_with(r)){return Err("BOARD_ASSET_NOT_REFERENCED".into())}
        if stamp(path)?!=reference.stamp {return Err("BOARD_REVISION_CHANGED".into())}
        let bytes=bounded(path,MAX_ASSET)?;
        if stamp(path)?!=reference.stamp {return Err("BOARD_REVISION_CHANGED".into())}
        let ext=path.extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase();
        let mime=match ext.as_str(){
            "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n")=>"image/png",
            "jpg"|"jpeg" if bytes.starts_with(&[255,216,255])=>"image/jpeg",
            "gif" if bytes.starts_with(b"GIF87a")||bytes.starts_with(b"GIF89a")=>"image/gif",
            "webp" if bytes.starts_with(b"RIFF")&&bytes.get(8..12)==Some(b"WEBP")=>"image/webp",
            "json"|"md"|"txt" if std::str::from_utf8(&bytes).is_ok()=>"text/plain;charset=utf-8",
            _=>return Err("BOARD_ASSET_TYPE_REFUSED".into())
        };
        let hash=digest(&bytes);
        Ok(Asset{mime:mime.into(),data:Base64::encode_string(&bytes),filename:path.file_name().unwrap_or_default().to_string_lossy().into(),asset_id:id.into(),content_hash:hash.clone(),source_hash:hash})
    }
    fn display_asset(&mut self,revision:&str,id:&str,variant:&str)->Result<Asset,String>{
        if !matches!(variant,"original"|"preview"|"thumbnail"){return Err("BOARD_ASSET_VARIANT_REFUSED".into())}
        // Revalidate the current catalog and file even when a derivative is cached.
        if revision!=self.revision{return Err("BOARD_REVISION_CHANGED".into())}
        let reference=self.files.get(id).ok_or("BOARD_ASSET_NOT_REFERENCED")?;
        no_reparse(&reference.path)?;
        if stamp(&reference.path)?!=reference.stamp{return Err("BOARD_REVISION_CHANGED".into())}
        let key=format!("{id}:{variant}");
        if variant!="original" {
            if let Some(i)=self.previews.iter().position(|(k,_)|k==&key){let item=self.previews.remove(i).unwrap();let value=item.1.clone();self.previews.push_back(item);return Ok(value)}
        }
        let mut result=self.asset(revision,id)?;
        if variant=="original"||!result.mime.starts_with("image/"){return Ok(result)}
        let bytes=Base64::decode_vec(&result.data).map_err(error)?;
        let original_len=bytes.len();
        let dimensions=image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format().map_err(error)?.into_dimensions().map_err(error)?;
        if u64::from(dimensions.0)*u64::from(dimensions.1)>16_777_216{return Err("BOARD_IMAGE_TOO_LARGE".into())}
        let mut reader=image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(error)?;
        let mut limits=image::Limits::default();limits.max_image_width=Some(8192);limits.max_image_height=Some(8192);limits.max_alloc=Some(128*1024*1024);reader.limits(limits);
        let decoded=reader.decode().map_err(|_|"BOARD_IMAGE_DECODE_FAILED")?;
        let size=if variant=="thumbnail"{192}else{1600};
        let reduced=decoded.thumbnail(size,size).to_rgba8();
        let mut background=image::RgbaImage::from_pixel(reduced.width(),reduced.height(),image::Rgba([255,255,255,255]));
        image::imageops::overlay(&mut background,&reduced,0,0);
        let rgb=image::DynamicImage::ImageRgba8(background).to_rgb8();let mut jpeg=Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg,85).encode_image(&rgb).map_err(error)?;
        if jpeg.len()<original_len||variant=="thumbnail" {
            result.mime="image/jpeg".into();result.content_hash=digest(&jpeg);result.data=Base64::encode_string(&jpeg);
        }
        while self.previews.len()>=96||self.previews.iter().map(|(_,a)|a.data.len()).sum::<usize>()+result.data.len()>24*1024*1024 {if self.previews.pop_front().is_none(){break}}
        self.previews.push_back((key,result.clone()));Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture()->(tempfile::TempDir,Board){
        let temp=tempfile::tempdir().unwrap();let root=temp.path();fs::create_dir(root.join("data")).unwrap();
        fs::write(root.join("index.html"),"<html><head><link rel=\"stylesheet\" href=\"style.css\"></head><body><script src=\"data/board-data.js\"></script><script src=\"app.js\"></script></body></html>").unwrap();
        fs::write(root.join("style.css"),"body{color:black}").unwrap();fs::write(root.join("app.js"),"window.rendered=true;").unwrap();
        fs::write(root.join("image.png"),b"\x89PNG\r\n\x1a\nfixture").unwrap();
        fs::write(root.join("data/board-data.js"),format!("window.CHARACTER_BOARD={};",serde_json::json!({"characters":[{"path":root.join("image.png").to_str().unwrap()}]}))).unwrap();
        let board=Board::new(root.to_owned(),vec![root.to_owned()]);(temp,board)
    }
    #[test] fn live_board_is_read_only_versioned_and_assets_are_exact(){
        let(temp,mut board)=fixture();let a=board.snapshot(None).unwrap();assert!(a.html.as_ref().unwrap().contains("window.rendered=true"));assert_eq!(a.assets.len(),1);
        assert_eq!(board.asset(&a.revision,&a.assets[0].id).unwrap().mime,"image/png");assert!(board.asset(&a.revision,"../image.png").is_err());
        assert!(board.snapshot(Some(&a.revision)).unwrap().html.is_none());
        fs::write(temp.path().join("app.js"),"window.rendered=2;").unwrap();let b=board.snapshot(Some(&a.revision)).unwrap();assert_ne!(a.revision,b.revision);assert!(b.html.unwrap().contains("rendered=2"));assert!(board.asset(&a.revision,&a.assets[0].id).is_err());
    }
    #[test] fn approval_script_is_inlined_versioned_and_not_served_as_an_asset(){
        let(temp,mut board)=fixture();let original=board.snapshot(None).unwrap();
        let p=temp.path().join("index.html");let html=fs::read_to_string(&p).unwrap().replace("</body>","<script src=\"approval-ui.js\"></script></body>");fs::write(&p,html).unwrap();fs::write(temp.path().join("approval-ui.js"),"window.boardApprovalVersion=1;").unwrap();
        let a=board.snapshot(Some(&original.revision)).unwrap();assert!(a.html.unwrap().contains("window.boardApprovalVersion=1"));assert!(board.approved(temp.path().join("approval-ui.js").to_str().unwrap()).is_none());
        fs::write(temp.path().join("approval-ui.js"),"window.boardApprovalVersion=2;").unwrap();let b=board.snapshot(Some(&a.revision)).unwrap();assert_ne!(a.revision,b.revision);assert!(b.html.unwrap().contains("window.boardApprovalVersion=2"));
    }
    #[test] fn paths_types_partial_writes_and_asset_changes_fail_closed(){
        let(temp,mut board)=fixture();assert!(board.approved("\\\\example.invalid\\share\\secret.png").is_none());assert!(board.approved(temp.path().join("data/board-data.js").to_str().unwrap()).is_none());
        let a=board.snapshot(None).unwrap();fs::write(temp.path().join("image.png"),"not a png").unwrap();assert!(board.asset(&a.revision,&a.assets[0].id).is_err());
        let b=board.snapshot(None).unwrap();assert!(board.asset(&b.revision,&b.assets[0].id).is_err());
        fs::write(temp.path().join("data/board-data.js"),"window.CHARACTER_BOARD={").unwrap();assert!(board.snapshot(None).is_err());
    }
    #[test] fn referenced_files_outside_the_explicit_roots_are_not_exposed(){
        let(temp,mut board)=fixture();let other=tempfile::tempdir().unwrap();let file=other.path().join("private.png");fs::write(&file,b"\x89PNG\r\n\x1a\nprivate").unwrap();
        fs::write(temp.path().join("data/board-data.js"),format!("window.CHARACTER_BOARD={};",serde_json::json!({"characters":[{"path":file}]}))).unwrap();assert!(board.snapshot(None).unwrap().assets.is_empty());
    }
    #[test] fn preview_identity_is_pinned_to_the_file_not_character_order_and_original_stays_exact(){
        let(temp,mut board)=fixture();
        let make=|path:&Path,color:[u8;3]|{
            let pixels=image::RgbImage::from_fn(900,1800,|x,y|image::Rgb([color[0].saturating_add((x%20)as u8),color[1].saturating_add((y%20)as u8),color[2]]));
            pixels.save(path).unwrap();
        };
        let own=temp.path().join("image.png");let grey=temp.path().join("grey.png");make(&own,[20,70,150]);make(&grey,[150,10,10]);
        fs::write(temp.path().join("data/board-data.js"),format!("window.CHARACTER_BOARD={};",serde_json::json!({"characters":[{"id":"char-102","path":own,"styleReference":grey}]}))).unwrap();
        let a=board.snapshot(None).unwrap();let id=&a.assets.iter().find(|a|a.source.ends_with("image.png")).unwrap().id;let grey_id=&a.assets.iter().find(|a|a.source.ends_with("grey.png")).unwrap().id;
        let original=board.asset(&a.revision,id).unwrap();assert_eq!(Base64::decode_vec(&original.data).unwrap(),fs::read(&own).unwrap());
        let thumbnail=board.display_asset(&a.revision,id,"thumbnail").unwrap();let preview=board.display_asset(&a.revision,id,"preview").unwrap();let other=board.display_asset(&a.revision,grey_id,"preview").unwrap();
        assert_eq!(thumbnail.asset_id,*id);assert_eq!(preview.source_hash,original.content_hash);assert_ne!(preview.source_hash,other.source_hash);
        let bytes=Base64::decode_vec(&thumbnail.data).unwrap();assert_eq!(thumbnail.content_hash,digest(&bytes));let decoded=image::load_from_memory(&bytes).unwrap();assert!(decoded.width()<=192&&decoded.height()<=192);assert!(thumbnail.data.len()<original.data.len());
        assert!(board.display_asset(&a.revision,id,"../../original").is_err());
        // Warm derivatives retain the same exact file lineage, never a gallery index.
        assert_eq!(board.display_asset(&a.revision,id,"preview").unwrap().content_hash,preview.content_hash);
        fs::write(&own,b"changed source").unwrap();assert!(board.display_asset(&a.revision,id,"preview").is_err());
    }
    #[test]
    #[ignore = "explicit read-only acceptance against the owner's existing board"]
    fn actual_owner_board_is_readable_without_changing_sources(){
        let started=std::time::Instant::now();let a=snapshot(None).unwrap();let cold=started.elapsed();assert!(a.html.as_ref().unwrap().contains("window.CHARACTER_BOARD"));assert!(!a.assets.is_empty());
        let id=a.assets.iter().find(|a|a.source.ends_with(".png")||a.source.ends_with(".jpeg")).unwrap().id.clone();
        let image=asset(&a.revision,&id).unwrap();assert!(image.mime.starts_with("image/"));
        let body=frame(&a.revision,"11111111-1111-4111-8111-111111111111").unwrap();assert!(body.contains("data-channel"));
        let started=std::time::Instant::now();assert!(snapshot(Some(&a.revision)).unwrap().html.is_none());let warm=started.elapsed();
        println!("BOARD_SNAPSHOT_TIMING cold_ms={} warm_ms={}",cold.as_millis(),warm.as_millis());
        println!("READ_ONLY_REAL_BOARD_PASS assets={} imageBytes={}",a.assets.len(),image.data.len());
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;
    #[test]
    fn only_https_urls_and_real_board_folders_are_accepted() {
        assert!(validate(Source::Url{url:"https://board.example.com/".into(),desktop_url:None}).is_ok());
        assert!(validate(Source::Url{url:"https://r.example.com/board/".into(),desktop_url:Some("http://127.0.0.1:47816/board/".into())}).is_ok());
        assert!(validate(Source::Url{url:"https://r.example.com/board/".into(),desktop_url:Some("http://evil.example.com/".into())}).is_err());
        for bad in ["http://board.example.com","https://user:pw@board.example.com","file:///C:/x","javascript:alert(1)","not a url"] {
            assert!(validate(Source::Url{url:bad.into(),desktop_url:None}).is_err(),"{bad}");
        }
        let dir=tempfile::tempdir().unwrap();
        let local=|root:String,extra_roots:Vec<String>|Source::Local{root,extra_roots};
        assert!(validate(local(dir.path().to_string_lossy().into(),vec![])).is_err());
        fs::write(dir.path().join("index.html"),"<html></html>").unwrap();
        assert!(validate(local(dir.path().to_string_lossy().into(),vec![])).is_ok());
        assert!(validate(local("relative/board".into(),vec![])).is_err());
        let art=tempfile::tempdir().unwrap();
        assert!(validate(local(dir.path().to_string_lossy().into(),vec![art.path().to_string_lossy().into()])).is_ok());
        assert!(validate(local(dir.path().to_string_lossy().into(),vec!["relative/art".into()])).is_err());
    }
    #[test]
    fn a_board_is_confined_to_its_folder_and_listed_extras() {
        let custom=PathBuf::from("D:/somewhere/board");
        assert_eq!(allowed(&custom,&[]),vec![custom.clone()]);
        assert_eq!(allowed(&custom,&["D:/art".into()]).len(),2);
    }
}
