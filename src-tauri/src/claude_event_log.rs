//! Small local diagnostic log of Claude Code plugin events Agbrio could not accept
//! and of transcript-watcher failures: time, session prefix, event kind and reason
//! code only. Never message text, paths or credentials. Kept under 1 MB.
use std::io::Write;
use std::path::PathBuf;

const MAX_BYTES: u64 = 1024 * 1024;

pub(crate) fn path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("AIWorkRouter").join("logs").join("claude-events.log"))
}

pub(crate) fn record(session: &str, kind: &str, code: &str) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() { let _ = std::fs::create_dir_all(dir); }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) { let _ = std::fs::rename(&path, path.with_extension("log.1")); }
    let clean = |v: &str, n: usize| v.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ':')).take(n).collect::<String>();
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{at} {} {} {}", clean(session, 8), clean(kind, 24), clean(code, 80));
    }
}
