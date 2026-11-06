//! Read-only views of Codex's own files. Never takes a lock and never loads a whole rollout.
use serde_json::Value;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

const TAIL_CAP: u64 = 64 * 1024 * 1024;
const TAIL_CHUNK: u64 = 1024 * 1024;
const ERROR_LOCK_VIOLATION: i32 = 33;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BoundaryKind {
    Started,
    Completed,
    Aborted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Boundary {
    pub kind: BoundaryKind,
    pub turn_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Signature {
    pub length: u64,
    pub modified: SystemTime,
}

pub(super) fn signature(path: &Path) -> Option<Signature> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(Signature { length: metadata.len(), modified: metadata.modified().ok()? })
}

pub(super) fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".codex")))
}

/// Reading one byte reports a whole-file `LockFileEx` without ever acquiring it, so a
/// check can never block the writer it is looking at.
pub(super) fn writer_lock_held(home: &Path, thread: &str) -> bool {
    let mut file = match File::open(home.join("thread-writer-locks").join(format!("{thread}.lock"))) {
        Ok(file) => file,
        Err(error) => return error.kind() != std::io::ErrorKind::NotFound,
    };
    let mut byte = [0_u8; 1];
    match file.read(&mut byte) {
        Ok(_) => false,
        Err(error) => error.raw_os_error() == Some(ERROR_LOCK_VIOLATION) || error.kind() != std::io::ErrorKind::UnexpectedEof,
    }
}

/// The newest turn boundary, scanning backward with a hard byte cap.
pub(super) fn latest_boundary(path: &Path) -> Option<Boundary> {
    let mut file = File::open(path).ok()?;
    let mut position = file.metadata().ok()?.len();
    let mut consumed = 0_u64;
    let mut carry: Vec<u8> = Vec::new();
    while position > 0 && consumed < TAIL_CAP {
        let length = TAIL_CHUNK.min(position);
        position -= length;
        file.seek(SeekFrom::Start(position)).ok()?;
        let mut chunk = vec![0_u8; length as usize];
        file.read_exact(&mut chunk).ok()?;
        consumed += length;
        chunk.extend_from_slice(&carry);
        let mut end = chunk.len();
        while let Some(newline) = chunk[..end].iter().rposition(|byte| *byte == b'\n') {
            if let Some(found) = parse_boundary(&chunk[newline + 1..end]) {
                return Some(found);
            }
            end = newline;
        }
        chunk.truncate(end);
        carry = chunk;
    }
    if position == 0 { parse_boundary(&carry) } else { None }
}

fn parse_boundary(line: &[u8]) -> Option<Boundary> {
    // Cheap filter: most lines are large items that can never be a boundary.
    if line.len() > 64 * 1024 || !contains(line, b"\"event_msg\"") {
        return None;
    }
    let value: Value = serde_json::from_slice(line).ok()?;
    if value["type"] != "event_msg" {
        return None;
    }
    let kind = match value.pointer("/payload/type")?.as_str()? {
        "task_started" => BoundaryKind::Started,
        "task_complete" => BoundaryKind::Completed,
        "turn_aborted" => BoundaryKind::Aborted,
        _ => return None,
    };
    Some(Boundary { kind, turn_id: value.pointer("/payload/turn_id").and_then(Value::as_str).map(str::to_owned) })
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

/// The Codex build Desktop itself uses, so reads match its on-disk format. The owner's
/// registered package is never copied or modified.
pub(super) fn desktop_codex_executable() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("OpenAI/Codex/bin");
    std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let executable = entry.path().join("codex.exe");
            let modified = std::fs::metadata(&executable).ok()?.modified().ok()?;
            Some((modified, executable))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, executable)| executable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        for line in lines {
            std::io::Write::write_all(&mut file, format!("{line}\n").as_bytes()).unwrap();
        }
        file
    }

    #[test]
    fn newest_boundary_wins_and_text_cannot_forge_one() {
        let file = write(&[
            r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"old"}}"#,
            r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"new"}}"#,
            r#"{"type":"response_item","payload":{"type":"message","text":"\"event_msg\" task_complete"}}"#,
            r#"{"type":"event_msg","payload":{"type":"token_count"}}"#,
        ]);
        assert_eq!(latest_boundary(file.path()), Some(Boundary { kind: BoundaryKind::Started, turn_id: Some("new".into()) }));
    }

    #[test]
    fn missing_or_boundaryless_files_report_nothing() {
        assert_eq!(latest_boundary(Path::new("definitely-missing.jsonl")), None);
        assert_eq!(latest_boundary(write(&[r#"{"type":"session_meta"}"#]).path()), None);
    }

    #[test]
    fn a_boundary_across_chunks_is_found() {
        let padding = format!(r#"{{"type":"response_item","payload":"{}"}}"#, "x".repeat(3 * 1024 * 1024));
        let file = write(&[r#"{"type":"event_msg","payload":{"type":"turn_aborted","turn_id":"t"}}"#, &padding]);
        assert_eq!(latest_boundary(file.path()).unwrap().kind, BoundaryKind::Aborted);
    }

    #[test]
    fn an_absent_lock_file_is_free() {
        let home = tempfile::tempdir().unwrap();
        assert!(!writer_lock_held(home.path(), "missing"));
    }
}
