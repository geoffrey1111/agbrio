//! Desktop leaf of ClaudeCodeAdapter: native display metadata and administrative
//! plugin commands only. Never launches, resumes, prompts or interrupts a session.
use router_core::claude_code::Session;
use serde::{de::{self, MapAccess, Visitor}, Deserializer};
use std::{collections::HashMap, fs, io::{BufReader, Read, Write}, path::{Path, PathBuf}, process::{Command, Stdio}, sync::{Mutex, OnceLock}, time::{Duration, Instant}};

pub(crate) fn data_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(roaming) = std::env::var_os("APPDATA") { roots.push(PathBuf::from(roaming).join("Claude")); }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude"));
    }
    roots.into_iter().filter_map(|p| fs::canonicalize(p).ok()).fold(Vec::new(), |mut result, p| {
        if !result.contains(&p) { result.push(p); } result
    })
}

// Stop parsing before any transcript, MCP configuration or approval data. The
// sentinel returns the captured prefix without requiring the rest of the JSON.
struct Metadata<'a> { wanted: &'a HashMap<String, String>, found: Option<(String, String)> }
impl<'de> Visitor<'de> for &mut Metadata<'_> {
    type Value = ();
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("Desktop session metadata prefix") }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut id = None;
        let mut cwd = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "cliSessionId" => {
                    let native: Option<String> = map.next_value()?;
                    if native.as_ref().is_none_or(|s| !self.wanted.contains_key(s)) { return Err(de::Error::custom("METADATA_OUTSIDE_SCOPE")); }
                    id = native;
                }
                "cwd" => { cwd = map.next_value::<Option<String>>()?; }
                "title" => {
                    let (Some(id), Some(cwd)) = (id.as_ref(), cwd.as_ref()) else { return Err(de::Error::custom("METADATA_IDENTITY_FIRST")); };
                    let exact = fs::canonicalize(cwd).ok().zip(fs::canonicalize(&self.wanted[id]).ok()).is_some_and(|(a,b)| a == b);
                    if !exact { return Err(de::Error::custom("METADATA_ROOT_MISMATCH")); }
                    let title: Option<String> = map.next_value()?;
                    if let Some(title) = title.filter(|v| !v.trim().is_empty() && v.chars().count() <= 512 && !v.chars().any(|c| c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))) {
                        self.found = Some((id.clone(), title));
                    }
                    return Err(de::Error::custom("METADATA_COMPLETE"));
                }
                "sessionId" | "originCwd" | "gitAnchorsFolderRealpath" | "lastFocusedAt" | "createdAt" | "lastActivityAt" | "model" | "effort" | "isArchived" => { map.next_value::<de::IgnoredAny>()?; }
                _ => return Err(de::Error::custom("METADATA_PREFIX_ENDED")),
            }
        }
        Ok(())
    }
}
fn read_name(path: &Path, wanted: &HashMap<String,String>) -> Option<(String,String)> {
    let file = fs::File::open(path).ok()?;
    let mut reader = serde_json::Deserializer::from_reader(BufReader::with_capacity(1024, file.take(64 * 1024)));
    let mut metadata = Metadata { wanted, found: None };
    let _ = reader.deserialize_map(&mut metadata);
    metadata.found
}
fn children(path: &Path, budget: &mut usize) -> Vec<fs::DirEntry> {
    let Ok(entries) = fs::read_dir(path) else { return Vec::new(); };
    let mut result = Vec::new();
    for entry in entries.flatten() {
        if *budget == 0 { break; }
        *budget -= 1;
        if entry.file_type().is_ok_and(|t| !t.is_symlink()) { result.push(entry); }
    }
    result
}
pub(crate) fn names_at(roots: &[PathBuf], sessions: &[Session]) -> HashMap<String,String> {
    let wanted: HashMap<_,_> = sessions.iter().map(|s| (s.session_id.clone(), s.cwd.clone())).collect();
    if wanted.is_empty() { return HashMap::new(); }
    let mut names = HashMap::<String, Option<String>>::new();
    let mut budget = 4096;
    for root in roots {
        let Ok(boundary) = fs::canonicalize(root.join("claude-code-sessions")) else { continue; };
        if !fs::canonicalize(root).is_ok_and(|root| boundary.starts_with(root)) { continue; }
        for account in children(&boundary, &mut budget).into_iter().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
            for project in children(&account.path(), &mut budget).into_iter().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
                for entry in children(&project.path(), &mut budget).into_iter().filter(|e| e.file_type().is_ok_and(|t| t.is_file())) {
                    let file_name = entry.file_name(); let Some(file_name) = file_name.to_str() else { continue; };
                    if !file_name.starts_with("local_") || !file_name.ends_with(".json") { continue; }
                    let Ok(path) = fs::canonicalize(entry.path()) else { continue; };
                    if !path.starts_with(&boundary) { continue; }
                    if let Some((id,title)) = read_name(&path, &wanted) {
                        names.entry(id).and_modify(|old| { if old.as_ref() != Some(&title) { *old = None; } }).or_insert(Some(title));
                    }
                }
            }
        }
    }
    names.into_iter().filter_map(|(id,title)| title.map(|title| (id,title))).collect()
}
struct NameCache { key: Vec<(String,String)>, roots: Vec<PathBuf>, at: Instant, names: HashMap<String,String> }
pub(crate) fn names(sessions: &[Session]) -> HashMap<String,String> {
    static CACHE: OnceLock<Mutex<Option<NameCache>>> = OnceLock::new();
    let roots = data_roots();
    let mut key: Vec<_> = sessions.iter().map(|s| (s.session_id.clone(),s.cwd.clone())).collect(); key.sort();
    let Ok(mut cache) = CACHE.get_or_init(|| Mutex::new(None)).lock() else { return HashMap::new(); };
    if let Some(old) = cache.as_ref().filter(|c| c.key == key && c.roots == roots && c.at.elapsed() < Duration::from_secs(60)) { return old.names.clone(); }
    let names = names_at(&roots, sessions);
    *cache = Some(NameCache { key, roots, at: Instant::now(), names: names.clone() }); names
}

pub(crate) fn engine_at(roots: &[PathBuf]) -> Result<PathBuf,String> {
    let mut engines = Vec::new(); let mut budget = 128;
    for root in roots {
        let Ok(boundary) = fs::canonicalize(root.join("claude-code")) else { continue; };
        if !fs::canonicalize(root).is_ok_and(|root| boundary.starts_with(root)) { continue; }
        for version in children(&boundary, &mut budget) {
            let Some(name) = version.file_name().to_str().map(str::to_owned) else { continue; };
            let Ok(version_number) = semver::Version::parse(&name) else { continue; };
            if version_number < semver::Version::new(2,1,286) { continue; }
            for build in children(&version.path(), &mut budget) {
                let Ok(executable) = fs::canonicalize(build.path().join("claude.exe")) else { continue; };
                if executable.is_file() && executable.starts_with(&boundary) { engines.push((version_number.clone(),executable)); }
            }
        }
    }
    engines.sort_by(|a,b| a.0.cmp(&b.0));
    engines.pop().map(|(_,path)| path).ok_or_else(|| "CLAUDE_DESKTOP_ENGINE_NOT_FOUND".into())
}

/// Fixed administrative argv only. Secret values, stdout and stderr are never
/// logged or returned. Only this child may be stopped on a bounded timeout.
pub(crate) trait PluginAdmin { fn run(&self, args: &[String], input: Option<&[u8]>) -> Result<serde_json::Value,String>; }
pub(crate) struct OfficialCli { pub executable: PathBuf }
impl PluginAdmin for OfficialCli {
    fn run(&self, args: &[String], input: Option<&[u8]>) -> Result<serde_json::Value,String> {
        let mut command = Command::new(&self.executable);
        command.args(args).stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
        let mut child = command.spawn().map_err(|_| "CLAUDE_PLUGIN_ADMIN_UNAVAILABLE")?;
        let output = child.stdout.take().ok_or("CLAUDE_PLUGIN_ADMIN_UNAVAILABLE")?;
        let reader = std::thread::spawn(move || { let mut bytes = Vec::new(); let _ = output.take(64 * 1024).read_to_end(&mut bytes); bytes });
        if let Some(input) = input {
            let write = child.stdin.take().ok_or("CLAUDE_PLUGIN_ADMIN_UNAVAILABLE").and_then(|mut stdin| stdin.write_all(input).map_err(|_| "CLAUDE_PLUGIN_ADMIN_UNAVAILABLE"));
            if write.is_err() { let _ = child.kill(); let _ = child.wait(); let _ = reader.join(); return Err("CLAUDE_PLUGIN_ADMIN_UNAVAILABLE".into()); }
        }
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Err(_) => { let _ = child.kill(); let _ = child.wait(); let _ = reader.join(); return Err("CLAUDE_PLUGIN_ADMIN_UNAVAILABLE".into()); }
                Ok(None) if start.elapsed() < Duration::from_secs(20) => std::thread::sleep(Duration::from_millis(25)),
                Ok(None) => { let _ = child.kill(); let _ = child.wait(); let _ = reader.join(); return Err("CLAUDE_PLUGIN_ADMIN_TIMEOUT".into()); }
            }
        };
        let bytes = reader.join().map_err(|_| "CLAUDE_PLUGIN_ADMIN_UNAVAILABLE")?;
        if !status.success() { return Err("CLAUDE_PLUGIN_ADMIN_REFUSED".into()); }
        serde_json::from_slice(&bytes).or_else(|_| {
            // Some official install commands announce progress before their
            // documented last-line JSON result. Never return that raw output.
            let last=bytes.split(|b|*b==b'\n').rev().find(|line|!line.iter().all(u8::is_ascii_whitespace)).unwrap_or(&[]);
            serde_json::from_slice(last)
        }).map_err(|_| "CLAUDE_PLUGIN_ADMIN_INVALID_RESULT".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session(root: &Path, id: &str) -> Session { Session { session_id:id.into(), instance_id:"fixture".into(), cwd:root.to_string_lossy().into(), version:"2.1.295".into(), state:"SESSION_START".into(), turn_id:None,checked_at:0,lease_until:1,title:None,latest_reply:None } }
    #[test] fn only_exact_registered_identity_and_root_supply_titles_without_reading_transcript() {
        let temp = tempfile::tempdir().unwrap(); let project = temp.path().join("project"); fs::create_dir(&project).unwrap();
        let id = "10000000-0000-4000-8000-000000000001"; let current = session(&project,id);
        let folder = temp.path().join("claude-code-sessions/account/project"); fs::create_dir_all(&folder).unwrap();
        let prefix = format!("{{\"cliSessionId\":\"{id}\",\"cwd\":{},\"title\":\"真实标题 🌱\",\"completedTurns\":NOT_JSON_DO_NOT_READ",serde_json::to_string(&current.cwd).unwrap());
        fs::write(folder.join("local_fixture.json"),prefix).unwrap();
        let names = names_at(&[temp.path().into()],&[current.clone()]); assert_eq!(names[id],"真实标题 🌱");
        let mut wrong = current.clone(); wrong.session_id = "other".into(); assert!(names_at(&[temp.path().into()],&[wrong]).is_empty());
        let other = temp.path().join("other"); fs::create_dir(&other).unwrap(); let wrong = session(&other,id); assert!(names_at(&[temp.path().into()],&[wrong]).is_empty());
        fs::write(folder.join("local_duplicate.json"),serde_json::json!({"cliSessionId":id,"cwd":current.cwd,"title":"Another"}).to_string()).unwrap();
        // serde_json's sorted map writes cliSessionId/cwd before title; no title
        // arbitrarily wins when multiple native files disagree.
        assert!(names_at(&[temp.path().into()],&[current]).is_empty());
    }
    #[test] fn payload_keys_before_title_fail_closed_and_unrelated_files_are_ignored() {
        let temp = tempfile::tempdir().unwrap(); let current = session(temp.path(),"native"); let wanted=HashMap::from([("native".into(),current.cwd.clone())]);
        let file=temp.path().join("local_fixture.json");
        fs::write(&file,format!("{{\"cliSessionId\":\"native\",\"cwd\":{},\"remoteMcpServersConfig\":INVALID,\"title\":\"not read\"}}",serde_json::to_string(&current.cwd).unwrap())).unwrap();
        assert!(read_name(&file,&wanted).is_none());
        fs::write(&file,format!("{{\"cliSessionId\":\"unregistered\",\"cwd\":{},\"title\":INVALID}}",serde_json::to_string(&current.cwd).unwrap())).unwrap(); assert!(read_name(&file,&wanted).is_none());
    }
    #[test] fn engine_discovery_is_bounded_to_supported_desktop_versions() {
        let temp=tempfile::tempdir().unwrap();
        for version in ["2.1.285","2.1.295","2.1.300"] {let p=temp.path().join(format!("claude-code/{version}/build"));fs::create_dir_all(&p).unwrap();fs::write(p.join("claude.exe"),b"fixture").unwrap();}
        assert!(engine_at(&[temp.path().into()]).unwrap().to_string_lossy().contains("2.1.300"));
    }
    #[test]
    #[ignore = "owner's existing desktop metadata only; no provider call or write"]
    fn live_registered_desktop_title_metadata() {
        let db=PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join("AIWorkRouter/data/router.db");
        let connection=rusqlite::Connection::open_with_flags(db,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let mut statement=connection.prepare("SELECT session_id,instance_id,cwd,version,state,turn_id,checked_at,lease_until FROM claude_sessions").unwrap();
        let sessions=statement.query_map([],|r|Ok(Session{session_id:r.get(0)?,instance_id:r.get(1)?,cwd:r.get(2)?,version:r.get(3)?,state:r.get(4)?,turn_id:r.get(5)?,checked_at:r.get(6)?,lease_until:r.get(7)?,title:None,latest_reply:None})).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
        assert!(!sessions.is_empty());
        let names=names_at(&data_roots(),&sessions);
        assert!(!names.is_empty(),"Registered native metadata did not yield a title");
        for id in names.keys(){assert!(sessions.iter().any(|s|&s.session_id==id));}
        assert!(engine_at(&data_roots()).unwrap().is_file());
    }
}
