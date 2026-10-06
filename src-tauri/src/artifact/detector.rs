use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;

const STRONG_LABELS: &[&str] = &[
    "ZIP",
    "ATTACHMENT",
    "ARTIFACT",
    "ARTIFACTS",
    "REVIEW_PACKAGE",
    "OUTPUT",
    "DELIVERABLE",
];
const DANGEROUS_EXTENSIONS: &[&str] = &[
    "exe", "msi", "bat", "cmd", "ps1", "scr", "com", "dll", "lnk",
];
const GENERIC_EXTENSIONS: &[&str] = &[
    "zip", "7z", "pdf", "md", "txt", "json", "csv", "xlsx", "docx", "png", "jpg", "jpeg",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrityStatus {
    Verified,
    Mismatch,
    NotProvided,
    Error,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CandidateConfidence {
    High,
    Low,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AttachmentCandidate {
    pub id: String,
    pub source_message_id: String,
    pub raw_path: String,
    pub normalized_path: Option<String>,
    pub filename: String,
    pub extension: Option<String>,
    pub exists: bool,
    pub is_file: bool,
    pub size: Option<u64>,
    pub declared_sha256: Option<String>,
    pub actual_sha256: Option<String>,
    pub integrity_status: IntegrityStatus,
    pub detection_reason: String,
    pub confidence: CandidateConfidence,
    pub default_selected: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
struct RawCandidate {
    raw_path: String,
    strong_label: Option<String>,
    line_index: usize,
}

pub fn detect(source_message_id: &str, text: &str) -> Vec<AttachmentCandidate> {
    let lines: Vec<&str> = text.lines().collect();
    let mut raw = strong_candidates(&lines);
    for (line_index, line) in lines.iter().enumerate() {
        if let Some(path) = generic_path(line) {
            if !raw
                .iter()
                .any(|candidate| same_path(&candidate.raw_path, &path))
                && Path::new(&path).is_file()
            {
                raw.push(RawCandidate {
                    raw_path: path,
                    strong_label: None,
                    line_index,
                });
            }
        }
    }
    raw.into_iter()
        .enumerate()
        .map(|(index, candidate)| {
            let declared = nearby_sha256(&lines, candidate.line_index);
            build_candidate(source_message_id, index + 1, candidate, declared)
        })
        .collect()
}

fn strong_candidates(lines: &[&str]) -> Vec<RawCandidate> {
    let mut candidates = Vec::new();
    let mut active_list_label: Option<String> = None;
    for (index, raw_line) in lines.iter().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((label, value)) = split_label(line) {
            active_list_label = if label == "ARTIFACTS" {
                Some(label.clone())
            } else {
                None
            };
            if !value.is_empty() {
                candidates.push(RawCandidate {
                    raw_path: clean_path(value),
                    strong_label: Some(label),
                    line_index: index,
                });
            } else if label != "ARTIFACTS" {
                if let Some(next) = lines
                    .get(index + 1)
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    candidates.push(RawCandidate {
                        raw_path: clean_path(next),
                        strong_label: Some(label),
                        line_index: index + 1,
                    });
                }
            }
            continue;
        }
        if let Some(label) = &active_list_label {
            if let Some(value) = line.strip_prefix('-').or_else(|| line.strip_prefix('*')) {
                candidates.push(RawCandidate {
                    raw_path: clean_path(value),
                    strong_label: Some(label.clone()),
                    line_index: index,
                });
            } else {
                active_list_label = None;
            }
        }
    }
    candidates.retain(|candidate| is_windows_absolute(&candidate.raw_path));
    candidates
}

fn split_label(line: &str) -> Option<(String, &str)> {
    let (label, value) = line.split_once(':')?;
    let normalized = label.trim().to_ascii_uppercase().replace(' ', "_");
    STRONG_LABELS
        .iter()
        .any(|known| *known == normalized)
        .then_some((normalized, value.trim()))
}

fn nearby_sha256(lines: &[&str], from: usize) -> Option<String> {
    lines
        .iter()
        .skip(from.saturating_add(1))
        .take(3)
        .find_map(|line| {
            let (label, value) = line.trim().split_once(':')?;
            let normalized = label.trim().to_ascii_uppercase().replace(' ', "_");
            if normalized != "ZIP_SHA256" && normalized != "SHA256" {
                return None;
            }
            normalize_hash(value).or_else(|| {
                lines
                    .iter()
                    .skip(from.saturating_add(2))
                    .take(2)
                    .find_map(|next| normalize_hash(next))
            })
        })
}

fn normalize_hash(value: &str) -> Option<String> {
    let hash = value.trim().trim_matches('`').trim_matches('"');
    (hash.len() == 64 && hash.chars().all(|character| character.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_uppercase())
}

fn clean_path(value: &str) -> String {
    value
        .trim()
        .trim_start_matches(['-', '*'])
        .trim()
        .trim_matches('`')
        .trim_matches('"')
        .trim()
        .to_string()
}

fn is_windows_absolute(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

fn generic_path(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    for start in 0..bytes.len().saturating_sub(2) {
        if bytes[start].is_ascii_alphabetic()
            && bytes[start + 1] == b':'
            && (bytes[start + 2] == b'\\' || bytes[start + 2] == b'/')
        {
            let tail = &line[start..];
            let lower = tail.to_ascii_lowercase();
            for extension in GENERIC_EXTENSIONS {
                let marker = format!(".{extension}");
                if let Some(position) = lower.find(&marker) {
                    return Some(clean_path(&tail[..position + marker.len()]));
                }
            }
        }
    }
    None
}

fn same_path(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn build_candidate(
    source_message_id: &str,
    ordinal: usize,
    raw: RawCandidate,
    declared: Option<String>,
) -> AttachmentCandidate {
    let raw_path = raw.raw_path;
    let strong_label = raw.strong_label;
    let path = Path::new(&raw_path);
    let exists = path.exists();
    let is_file = path.is_file();
    let normalized_path = fs::canonicalize(path)
        .ok()
        .map(|value| value.to_string_lossy().to_string());
    let filename = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| raw_path.clone());
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase());
    let mut warnings = Vec::new();
    if !exists {
        warnings.push("Local path was not found; it cannot be relayed.".to_string());
    }
    if exists && !is_file {
        warnings.push("Local path is not a regular file.".to_string());
    }
    let dangerous = extension
        .as_deref()
        .is_some_and(|value| DANGEROUS_EXTENSIONS.contains(&value));
    if dangerous {
        warnings.push("Executable or script-like files are not selected by default.".to_string());
    }
    let (actual_sha256, integrity_status) = if is_file {
        match actual_sha256(path) {
            Ok(actual) => match &declared {
                Some(expected) if *expected == actual => (Some(actual), IntegrityStatus::Verified),
                Some(_) => {
                    warnings.push("Declared SHA256 does not match the local file.".to_string());
                    (Some(actual), IntegrityStatus::Mismatch)
                }
                None => (Some(actual), IntegrityStatus::NotProvided),
            },
            Err(error) => {
                warnings.push(format!("Could not calculate SHA256: {error}"));
                (None, IntegrityStatus::Error)
            }
        }
    } else {
        (None, IntegrityStatus::Error)
    };
    let size = if is_file {
        fs::metadata(path).ok().map(|metadata| metadata.len())
    } else {
        None
    };
    let strong = strong_label.is_some();
    let confidence = if strong {
        CandidateConfidence::High
    } else {
        CandidateConfidence::Low
    };
    let default_selected = strong
        && exists
        && is_file
        && !dangerous
        && !matches!(
            integrity_status,
            IntegrityStatus::Mismatch | IntegrityStatus::Error
        );
    let detection_reason = strong_label
        .map(|label| format!("Explicit {label} label"))
        .unwrap_or_else(|| "Existing local path in final agent text".to_string());
    AttachmentCandidate {
        id: format!("{source_message_id}:attachment:{ordinal}"),
        source_message_id: source_message_id.to_string(),
        raw_path,
        normalized_path,
        filename,
        extension,
        exists,
        is_file,
        size,
        declared_sha256: declared,
        actual_sha256,
        integrity_status,
        detection_reason,
        confidence,
        default_selected,
        warnings,
    }
}

fn actual_sha256(path: &Path) -> Result<String, std::io::Error> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8_192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:X}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn file(root: &Path, name: &str, bytes: &[u8]) -> String {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut handle = File::create(&path).unwrap();
        handle.write_all(bytes).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn explicit_zip_is_strong_and_selected() {
        let temp = tempfile::tempdir().unwrap();
        let path = file(temp.path(), "review.zip", b"review");
        let candidates = detect("message-a", &format!("ZIP:\n{path}"));
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].confidence, CandidateConfidence::High);
        assert!(candidates[0].default_selected);
    }

    #[test]
    fn unicode_and_spaces_are_preserved() {
        let temp = tempfile::tempdir().unwrap();
        let path = file(temp.path(), "临时处理/review package 01.zip", b"unicode");
        let candidates = detect("message-a", &format!("ARTIFACT:\n`{path}`"));
        assert_eq!(candidates[0].raw_path, path);
        assert!(candidates[0].exists);
    }

    #[test]
    fn source_path_is_not_selected_without_explicit_label() {
        let temp = tempfile::tempdir().unwrap();
        let path = file(temp.path(), "src/main.ts", b"source");
        let candidates = detect("message-a", &format!("Modified:\n{path}"));
        assert!(candidates.is_empty());
    }

    #[test]
    fn missing_strong_path_is_invalid_not_verified() {
        let candidates = detect("message-a", "ZIP:\nC:\\does-not-exist\\review.zip");
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].exists);
        assert!(!candidates[0].default_selected);
        assert_eq!(candidates[0].integrity_status, IntegrityStatus::Error);
    }

    #[test]
    fn valid_and_invalid_hash_change_selection() {
        let temp = tempfile::tempdir().unwrap();
        let path = file(temp.path(), "review.zip", b"hash me");
        let actual = actual_sha256(Path::new(&path)).unwrap();
        let valid = detect("message-a", &format!("ZIP:\n{path}\nZIP_SHA256:\n{actual}"));
        assert_eq!(valid[0].integrity_status, IntegrityStatus::Verified);
        assert!(valid[0].default_selected);
        let invalid = detect(
            "message-a",
            &format!("ZIP:\n{path}\nSHA256:\n{}", "0".repeat(64)),
        );
        assert_eq!(invalid[0].integrity_status, IntegrityStatus::Mismatch);
        assert!(!invalid[0].default_selected);
    }

    #[test]
    fn dangerous_file_is_never_default_selected() {
        let temp = tempfile::tempdir().unwrap();
        let path = file(temp.path(), "tool.exe", b"not executed");
        let candidates = detect("message-a", &format!("ARTIFACT:\n{path}"));
        assert!(!candidates[0].default_selected);
        assert!(!candidates[0].warnings.is_empty());
    }
}
