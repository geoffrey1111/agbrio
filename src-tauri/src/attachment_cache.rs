//! Owner-only managed upload cache; never arbitrary project or conversation deletion.
use crate::host_application::RouterCore;
use serde::{Deserialize,Serialize};use std::{path::{Path,PathBuf},collections::{HashMap,HashSet},sync::{Mutex,OnceLock,MutexGuard,atomic::{AtomicU64,Ordering}},time::{SystemTime,UNIX_EPOCH},io::Read};
const RETAIN:u64=7*86400;static LOCK:OnceLock<Mutex<()>>=OnceLock::new();static LAST:AtomicU64=AtomicU64::new(0);
pub(crate) fn guard()->Result<MutexGuard<'static,()>,String>{LOCK.get_or_init(Mutex::default).lock().map_err(|_|"CACHE_BUSY".into())}
#[derive(Default,Serialize)]#[serde(rename_all="camelCase")]
pub(crate) struct Report{pub upload_count:u64,pub bytes:u64,pub eligible_count:u64,pub eligible_bytes:u64,pub protected_count:u64,pub protected_bytes:u64,pub removed_count:u64,pub removed_logical_bytes:u64,pub skipped:u64,pub retention_days:u64}
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]struct Manifest{id:String,thread_id:String,cwd:String,name:String,size:u64,sha256:String}
fn linked(path:&Path)->bool{let Ok(meta)=std::fs::symlink_metadata(path)else{return true;};if meta.file_type().is_symlink(){return true;}
 #[cfg(windows)]{use std::os::windows::fs::MetadataExt;if meta.file_attributes()&0x400!=0{return true;}}false}
fn hash(path:&Path)->Result<String,String>{use sha2::{Digest,Sha256};let mut f=std::fs::File::open(path).map_err(|_|"CACHE_FILE_BUSY")?;let mut sha=Sha256::new();let mut buf=[0;65536];loop{let n=f.read(&mut buf).map_err(|_|"CACHE_FILE_BUSY")?;if n==0{break;}sha.update(&buf[..n]);}Ok(format!("{:x}",sha.finalize()))}
pub(crate) fn inspect(core:&RouterCore,remove:bool)->Result<Report,String>{let _lock=LOCK.get_or_init(Mutex::default).try_lock().map_err(|_|"CACHE_BUSY")?;scan(core,remove,SystemTime::now())}
fn scan(core:&RouterCore,remove:bool,now:SystemTime)->Result<Report,String>{
 let mut report=Report{retention_days:7,..Default::default()};let mut roots:HashMap<PathBuf,HashSet<String>>=HashMap::new();
 for(thread,cwd)in core.store.attachment_cache_contexts()?{if let Ok(root)=std::fs::canonicalize(cwd){roots.entry(root).or_default().insert(thread);}}
 for(root,threads)in roots{let aiwr=root.join(".aiwr");let uploads=aiwr.join("watch-replies");if !uploads.exists(){continue;}if linked(&aiwr)||linked(&uploads){report.skipped+=1;continue;}
 let Ok(entries)=std::fs::read_dir(&uploads)else{report.skipped+=1;continue;};
 for entry in entries.flatten(){let dir=entry.path();let name=entry.file_name().to_string_lossy().to_string();if !uuid::Uuid::parse_str(&name).is_ok_and(|id|id.to_string()==name)||linked(&dir)||!dir.is_dir(){report.skipped+=1;continue;}
 let manifest=dir.join("manifest.json");if linked(&manifest)||std::fs::metadata(&manifest).map(|m|m.len()>4096).unwrap_or(true){report.skipped+=1;continue;}
 let Ok(raw)=std::fs::read(&manifest)else{report.skipped+=1;continue;};let Ok(m)=serde_json::from_slice::<Manifest>(&raw)else{report.skipped+=1;continue;};
 if m.id!=name||!threads.contains(&m.thread_id)||m.name.is_empty()||m.name.len()>150||std::fs::canonicalize(&m.cwd).ok().as_ref()!=Some(&root)||m.sha256.len()!=64{report.skipped+=1;continue;}
 let Ok(files)=std::fs::read_dir(&dir).map(|v|v.flatten().map(|e|e.path()).collect::<Vec<_>>())else{report.skipped+=1;continue;};
 if files.iter().any(|p|linked(p)||!p.is_file()||!matches!(p.file_name().and_then(|n|n.to_str()),Some("manifest.json"|"content.bin"|"attachment.png"|"attachment.jpg"|"attachment.gif"|"attachment.webp"))){report.skipped+=1;continue;}
 let content=dir.join("content.bin");if content.exists()&&std::fs::metadata(&content).map(|v|v.len()!=m.size).unwrap_or(true){report.skipped+=1;continue;}
 let bytes=files.iter().filter_map(|p|std::fs::metadata(p).ok()).map(|m|m.len()).sum::<u64>();report.upload_count+=1;report.bytes+=bytes;
 if core.store.attachment_cache_bound(&m.id)?{report.protected_count+=1;report.protected_bytes+=bytes;continue;}
 let old=std::fs::metadata(&manifest).ok().and_then(|m|m.modified().ok()).and_then(|t|now.duration_since(t).ok()).is_some_and(|d|d.as_secs()>=RETAIN);if !old{continue;}
 report.eligible_count+=1;report.eligible_bytes+=bytes;if !remove{continue;}
 if files.iter().filter(|p|p.file_name().is_some_and(|n|n!="manifest.json")).any(|p|hash(p).ok().as_deref()!=Some(&m.sha256)){report.skipped+=1;continue;}
 // Recheck binding while holding the same application lock as prepare/send.
 if core.store.attachment_cache_bound(&m.id)?{continue;}
 let Some(parent)=dir.canonicalize().ok().and_then(|p|p.parent().map(Path::to_path_buf))else{continue;};if parent!=uploads{report.skipped+=1;continue;}
 let mut ordered=files.clone();ordered.sort_by_key(|p|p.file_name().is_some_and(|n|n=="manifest.json"));let mut failed=false;for p in &ordered{if std::fs::remove_file(p).is_err(){failed=true;break;}}if failed||std::fs::remove_dir(&dir).is_err(){report.skipped+=1;}else{report.removed_count+=1;report.removed_logical_bytes+=bytes;}
 }}Ok(report)
}
pub(crate) fn maintain(core:&RouterCore){let now=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();if now.saturating_sub(LAST.load(Ordering::Relaxed))<3600{return;}let Ok(_lock)=LOCK.get_or_init(Mutex::default).try_lock()else{return;};LAST.store(now,Ordering::Relaxed);let _=scan(core,true,SystemTime::now());}

#[cfg(test)]mod tests{
 use super::*;use base64ct::{Base64,Encoding};use serde_json::json;
 fn fixture()->(tempfile::TempDir,RouterCore){let scratch=Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/notice-attachments-20261025");std::fs::create_dir_all(&scratch).unwrap();let d=tempfile::tempdir_in(scratch).unwrap();let core=RouterCore{store:std::sync::Arc::new(crate::RouterStore::open_at(d.path().join("router.db")).unwrap()),chatgpt:Default::default(),session:Default::default(),completed_chatgpt_responses:Default::default()};core.store.enable_codex_watch("exact","fixture",&d.path().to_string_lossy(),&router_core::store::codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()}).unwrap();(d,core)}
 fn upload(c:&RouterCore)->PathBuf{let r=crate::watch_chat::upload(c,"exact","fixture.txt",&Base64::encode_string(b"fixture bytes")).unwrap();Path::new(&c.store.codex_chat_context("exact").unwrap().cwd).join(".aiwr/watch-replies").join(r["id"].as_str().unwrap())}
 fn old(p:&Path){std::fs::OpenOptions::new().write(true).open(p.join("manifest.json")).unwrap().set_modified(SystemTime::now()-std::time::Duration::from_secs(RETAIN+60)).unwrap();}
 #[test]fn only_old_unbound_managed_uploads_are_deleted(){let(_d,c)=fixture();let fresh=upload(&c);let stale=upload(&c);old(&stale);let s=scan(&c,false,SystemTime::now()).unwrap();assert_eq!((s.upload_count,s.eligible_count,s.removed_count),(2,1,0));assert!(stale.exists());let s=scan(&c,true,SystemTime::now()).unwrap();assert_eq!(s.removed_count,1);assert!(!stale.exists());assert!(fresh.join("content.bin").exists());assert_eq!(scan(&c,true,SystemTime::now()).unwrap().removed_count,0);}
 #[test]fn all_reply_states_protect_attachments_even_when_older_than_seven_days(){for state in ["QUEUED","SENDING","UNKNOWN","SENT","FAILED","CANCELLED"]{let(_d,c)=fixture();let p=upload(&c);old(&p);let w=c.store.codex_chat_context("exact").unwrap();let reply:router_core::store::watch_reply::WatchReply=serde_json::from_value(json!({"id":uuid::Uuid::new_v4().to_string(),"threadId":"exact","cwd":w.cwd,"generation":w.generation,"sourceSequence":null,"expectedTurnId":null,"mode":"SEND","text":"fixture","options":{"attachments":[p.file_name().unwrap().to_str().unwrap()]},"payloadHash":"","status":"QUEUED","turnId":null,"errorCode":null,"createdAt":0,"updatedAt":0})).unwrap();c.store.prepare_watch_reply(&reply).unwrap();match state{"SENDING"=>{c.store.claim_watch_reply(&reply.id).unwrap();},"SENT"|"UNKNOWN"|"FAILED"=>{c.store.finish_watch_reply(&reply.id,state,None,None).unwrap();},"CANCELLED"=>{c.store.cancel_watch_reply("exact",&reply.id).unwrap();},_=>{}}let s=scan(&c,true,SystemTime::now()).unwrap();assert_eq!((s.protected_count,s.removed_count),(1,0),"{state}");assert!(p.join("content.bin").exists());}}
 #[test]fn modified_files_extra_user_files_and_symlinks_are_preserved(){let(d,c)=fixture();let modified=upload(&c);old(&modified);std::fs::write(modified.join("content.bin"),b"changed bytes").unwrap();let extra=upload(&c);old(&extra);std::fs::write(extra.join("user-notes.txt"),b"unique").unwrap();let outside=d.path().join("outside");std::fs::create_dir_all(&outside).unwrap();std::fs::write(outside.join("keep.txt"),b"unique").unwrap();let fake=modified.parent().unwrap().join(uuid::Uuid::new_v4().to_string());
 #[cfg(windows)]let made=std::os::windows::fs::symlink_dir(&outside,&fake).is_ok();#[cfg(not(windows))]let made=std::os::unix::fs::symlink(&outside,&fake).is_ok();
 let s=scan(&c,true,SystemTime::now()).unwrap();assert_eq!(s.removed_count,0);assert!(modified.exists()&&extra.exists()&&outside.join("keep.txt").exists());if made{assert!(linked(&fake));std::fs::remove_dir(&fake).unwrap();}}
 #[test]fn partial_own_deletion_can_finish_without_removing_foreign_data(){let(_d,c)=fixture();let p=upload(&c);old(&p);std::fs::remove_file(p.join("content.bin")).unwrap();assert_eq!(scan(&c,true,SystemTime::now()).unwrap().removed_count,1);assert!(!p.exists());}
}
