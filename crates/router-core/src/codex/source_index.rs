//! Read-only identity/availability metadata from this user's existing native index.
use std::path::{Path,PathBuf};
use rusqlite::OptionalExtension;
#[derive(Debug,PartialEq,Eq)]pub enum Availability{Present,Archived,FileMissing,ProjectChanged}
pub fn current(thread:&str,cwd:&str)->Result<Option<Availability>,String>{
 if uuid::Uuid::parse_str(thread).is_err(){return Ok(None);}
 let home=std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(||std::env::var_os("USERPROFILE").or_else(||std::env::var_os("HOME")).map(|v|PathBuf::from(v).join(".codex"))).ok_or("CODEX_SOURCE_INDEX_UNAVAILABLE")?;
 read(&home,thread,cwd)
}
fn read(home:&Path,thread:&str,cwd:&str)->Result<Option<Availability>,String>{
 let path=home.join("state_5.sqlite");if !path.is_file(){return Ok(None);}
 let c=rusqlite::Connection::open_with_flags(path,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY|rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|_|"CODEX_SOURCE_INDEX_UNAVAILABLE")?;let _=c.busy_timeout(std::time::Duration::from_millis(50));
 let row:Option<(String,String,bool)>=c.query_row("SELECT cwd,rollout_path,archived FROM threads WHERE id=?1",[thread],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|_|"CODEX_SOURCE_INDEX_UNAVAILABLE")?;let Some((native_cwd,file,archived))=row else{return Ok(None)};
 if !crate::claude_code::same_root(&native_cwd,cwd){return Ok(Some(Availability::ProjectChanged));}
 if archived{return Ok(Some(Availability::Archived));}
 let root=home.canonicalize().map_err(|_|"CODEX_SOURCE_INDEX_UNAVAILABLE")?;let file=Path::new(&file);let parent=file.parent().and_then(|p|p.canonicalize().ok()).ok_or("CODEX_SOURCE_LOCATION_UNVERIFIED")?;
 if ![root.join("sessions"),root.join("archived_sessions")].iter().any(|p|parent.starts_with(p)){return Err("CODEX_SOURCE_LOCATION_UNVERIFIED".into());}
 match std::fs::metadata(file){Ok(m)if m.is_file()=>Ok(Some(Availability::Present)),Err(e)if e.kind()==std::io::ErrorKind::NotFound=>Ok(Some(Availability::FileMissing)),_=>Err("CODEX_SOURCE_INDEX_UNAVAILABLE".into())}
}
/// Display metadata only; never supplies routing or write authority. Reads the native
/// index afresh because a rename does not append anything to the rollout.
pub fn display_name_at(home:&Path,thread:&str,cwd:&str)->Option<String>{
 let c=rusqlite::Connection::open_with_flags(home.join("state_5.sqlite"),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY|rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
 c.busy_timeout(std::time::Duration::from_millis(50)).ok()?;
 let (root,label):(String,String)=c.query_row("SELECT cwd,substr(COALESCE(NULLIF(trim(name),''),title),1,120) FROM threads WHERE id=?1",[thread],|r|Ok((r.get(0)?,r.get(1)?))).ok()?;
 if !crate::claude_code::same_root(&root,cwd){return None;}
 let label=label.trim();if label.is_empty()||label.chars().any(char::is_control){return None;}Some(label.into())
}
/// Native thread update time, never Agbrio's polling or watch timestamp.
pub fn activity_at(home:&Path,thread:&str,cwd:&str)->Option<i64>{
 let db=rusqlite::Connection::open_with_flags(home.join("state_5.sqlite"),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY|rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
 db.busy_timeout(std::time::Duration::from_millis(50)).ok()?;
 let (native_root,seconds):(String,i64)=db.query_row("SELECT cwd,updated_at FROM threads WHERE id=?1",[thread],|row|Ok((row.get(0)?,row.get(1)?))).ok()?;
 if seconds<=0||!crate::claude_code::same_root(&native_root,cwd){return None;}
 seconds.checked_mul(1000)
}
pub fn current_activity_at(thread:&str,cwd:&str)->Option<i64>{
 if uuid::Uuid::parse_str(thread).is_err(){return None;}
 let home=std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(||std::env::var_os("USERPROFILE").or_else(||std::env::var_os("HOME")).map(|v|PathBuf::from(v).join(".codex")))?;
 activity_at(&home,thread,cwd)
}
pub fn current_display_name(thread:&str,cwd:&str)->Option<String>{
 if uuid::Uuid::parse_str(thread).is_err(){return None;}
 let home=std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(||std::env::var_os("USERPROFILE").or_else(||std::env::var_os("HOME")).map(|v|PathBuf::from(v).join(".codex")))?;
 display_name_at(&home,thread,cwd)
}
#[cfg(test)]mod tests{use super::*;#[test]fn archive_is_distinct_from_cold_missing_and_unknown(){let d=tempfile::tempdir().unwrap();let sessions=d.path().join("sessions");std::fs::create_dir(&sessions).unwrap();let file=sessions.join("owned.jsonl");std::fs::write(&file,"fixture").unwrap();let db=rusqlite::Connection::open(d.path().join("state_5.sqlite")).unwrap();db.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY,cwd TEXT,rollout_path TEXT,archived INTEGER)").unwrap();db.execute("INSERT INTO threads VALUES('one',?1,?2,1)",rusqlite::params![d.path().to_str().unwrap(),file.to_str().unwrap()]).unwrap();assert_eq!(read(d.path(),"one",d.path().to_str().unwrap()).unwrap(),Some(Availability::Archived));db.execute("UPDATE threads SET archived=0",[]).unwrap();assert_eq!(read(d.path(),"one",d.path().to_str().unwrap()).unwrap(),Some(Availability::Present));std::fs::remove_file(file).unwrap();assert_eq!(read(d.path(),"one",d.path().to_str().unwrap()).unwrap(),Some(Availability::FileMissing));assert_eq!(read(d.path(),"other",d.path().to_str().unwrap()).unwrap(),None);assert_eq!(read(d.path(),"one","D:/other").unwrap(),Some(Availability::ProjectChanged));}}

#[cfg(test)]mod title_tests{use super::*;
 #[test]fn names_refresh_without_rollout_changes_and_require_exact_project(){
  let d=tempfile::tempdir().unwrap();let db=rusqlite::Connection::open(d.path().join("state_5.sqlite")).unwrap();db.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY,cwd TEXT,title TEXT,name TEXT)").unwrap();let root=d.path().to_str().unwrap();db.execute("INSERT INTO threads VALUES('exact',?1,'Preview','Old')",[root]).unwrap();
  assert_eq!(display_name_at(d.path(),"exact",root).as_deref(),Some("Old"));db.execute_batch("UPDATE threads SET name='New native title'").unwrap();assert_eq!(display_name_at(d.path(),"exact",root).as_deref(),Some("New native title"));assert_eq!(display_name_at(d.path(),"exact","C:/other"),None);assert_eq!(display_name_at(d.path(),"other",root),None);
  db.execute_batch("UPDATE threads SET name='   '").unwrap();assert_eq!(display_name_at(d.path(),"exact",root).as_deref(),Some("Preview"));
 }
}
