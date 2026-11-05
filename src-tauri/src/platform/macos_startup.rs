//! Explicit current-user login configuration. Does not load/unload a job or
//! start/stop the live Host. The next login uses LaunchServices independently.
use std::{fs::{self,OpenOptions},io::Write,os::unix::fs::{MetadataExt,OpenOptionsExt,PermissionsExt},path::{Path,PathBuf}};
const LABEL:&str="com.geoffrey.aiworkrouter.login";
const MARKER:&str="<!-- Agbrio-owned-login-v1 -->";
extern "C"{fn geteuid()->u32;}
fn trusted(path:&Path)->Result<(),String>{
    for ancestor in path.ancestors(){
        match fs::symlink_metadata(ancestor){
            Ok(m) if m.file_type().is_symlink()=>return Err("MACOS_LOGIN_ITEM_LINK_REJECTED".into()),
            Ok(_)=>{},Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},Err(_)=>return Err("MACOS_LOGIN_ITEM_UNAVAILABLE".into())
        }
    }Ok(())
}
fn xml(value:&str)->String{value.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;").replace('\'',"&apos;")}
fn content(app:&Path)->Result<String,String>{
    if !app.is_absolute()||app.file_name().and_then(|s|s.to_str())!=Some("Agbrio.app"){return Err("MACOS_LOGIN_APP_INVALID".into());}
    let text=app.to_str().filter(|s|!s.chars().any(char::is_control)).ok_or("MACOS_LOGIN_APP_INVALID")?;
    Ok(format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{MARKER}\n<plist version=\"1.0\"><dict><key>Label</key><string>{LABEL}</string><key>ProgramArguments</key><array><string>/usr/bin/open</string><string>-g</string><string>{}</string></array><key>RunAtLoad</key><true/><key>KeepAlive</key><false/></dict></plist>\n",xml(text)))
}
fn read_at(path:&Path,expected:&str)->Result<bool,String>{
    trusted(path)?;
    let m=match fs::symlink_metadata(path){Ok(m)=>m,Err(e)if e.kind()==std::io::ErrorKind::NotFound=>return Ok(false),Err(_)=>return Err("MACOS_LOGIN_ITEM_UNAVAILABLE".into())};
    if !m.is_file()||m.uid()!=unsafe{geteuid()}||m.len()>32768||m.nlink()!=1{return Err("MACOS_LOGIN_ITEM_UNTRUSTED".into());}
    let text=fs::read_to_string(path).map_err(|_|"MACOS_LOGIN_ITEM_UNAVAILABLE")?;
    if text!=expected{return Err("MACOS_LOGIN_ITEM_FOREIGN_PRESERVED".into());}Ok(true)
}
fn set_at(path:&Path,expected:&str,enabled:bool)->Result<(),String>{
    let exists=read_at(path,expected)?;
    if enabled==exists{return Ok(());}
    if !enabled{return fs::remove_file(path).map_err(|_|"MACOS_LOGIN_ITEM_WRITE_UNCONFIRMED".into());}
    let parent=path.parent().ok_or("MACOS_LOGIN_ITEM_UNAVAILABLE")?;trusted(parent)?;fs::create_dir_all(parent).map_err(|_|"MACOS_LOGIN_ITEM_UNAVAILABLE")?;trusted(parent)?;
    let temporary=parent.join(format!(".agbrio-login-{}.plist",uuid::Uuid::new_v4()));
    let result=(||{
        let mut file=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary).map_err(|_|"MACOS_LOGIN_ITEM_UNAVAILABLE")?;
        file.write_all(expected.as_bytes()).and_then(|_|file.sync_all()).map_err(|_|"MACOS_LOGIN_ITEM_WRITE_UNCONFIRMED")?;
        // Publish without overwriting a file that appeared after inspection.
        fs::hard_link(&temporary,path).map_err(|_|"MACOS_LOGIN_ITEM_WRITE_UNCONFIRMED")?;
        fs::remove_file(&temporary).map_err(|_|"MACOS_LOGIN_ITEM_WRITE_UNCONFIRMED")?;
        if !read_at(path,expected)?{return Err("MACOS_LOGIN_ITEM_WRITE_UNCONFIRMED".into());}Ok(())
    })();
    let _=fs::remove_file(&temporary);result
}
fn location()->Result<(PathBuf,String),String>{
    let home=router_core::platform::user_home()?;
    let executable=std::env::current_exe().and_then(fs::canonicalize).map_err(|_|"MACOS_LOGIN_APP_INVALID")?;
    let app=executable.parent().and_then(Path::parent).and_then(Path::parent).ok_or("MACOS_LOGIN_APP_INVALID")?;
    Ok((home.join("Library/LaunchAgents").join(format!("{LABEL}.plist")),content(app)?))
}
pub(crate) fn enabled()->Result<bool,String>{let(path,text)=location()?;read_at(&path,&text)}
pub(crate) fn set(enabled:bool)->Result<(),String>{let(path,text)=location()?;set_at(&path,&text,enabled)}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn explicit_configuration_is_idempotent_and_does_not_adopt_foreign_files(){
        let root=tempfile::tempdir().unwrap();let file=root.path().join("LaunchAgents/login.plist");let text=content(&root.path().join("Agbrio.app")).unwrap();
        assert!(!read_at(&file,&text).unwrap());assert!(!file.parent().unwrap().exists());
        set_at(&file,&text,true).unwrap();assert!(read_at(&file,&text).unwrap());assert_eq!(fs::metadata(&file).unwrap().permissions().mode()&0o777,0o600);
        set_at(&file,&text,true).unwrap();set_at(&file,&text,false).unwrap();assert!(!file.exists());
        fs::write(&file,b"foreign job").unwrap();assert_eq!(set_at(&file,&text,true).unwrap_err(),"MACOS_LOGIN_ITEM_FOREIGN_PRESERVED");assert_eq!(fs::read(&file).unwrap(),b"foreign job");
    }
    #[test]fn login_paths_are_escaped_and_symlink_targets_preserved(){
        let root=tempfile::tempdir().unwrap();let target=root.path().join("foreign");fs::write(&target,b"preserve").unwrap();let file=root.path().join("login.plist");std::os::unix::fs::symlink(&target,&file).unwrap();
        assert_eq!(set_at(&file,&content(&root.path().join("Agbrio.app")).unwrap(),false).unwrap_err(),"MACOS_LOGIN_ITEM_LINK_REJECTED");assert_eq!(fs::read(target).unwrap(),b"preserve");
        assert!(content(&Path::new("/Applications/A & B").join("Agbrio.app")).unwrap().contains("A &amp; B"));
    }
}
