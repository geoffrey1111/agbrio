//! Display-only native CLI metadata fallback. Never starts a provider or writes a
//! transcript. Desktop titles stay authoritative; native UUID and cwd must match.
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{Mutex, OnceLock},
    time::SystemTime,
};
const WINDOW: u64 = 128 * 1024;
const LINE: usize = 16 * 1024;
fn clean(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty()
        || text.chars().count() > 512
        || text
            .chars()
            .any(|c| c.is_control() || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'))
    {
        None
    } else {
        Some(text.into())
    }
}
fn read_at(path: &Path, session: &str, cwd: &str) -> Option<String> {
    crate::character_board::no_reparse(path).ok()?;
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut head = Vec::new();
    (&mut file).take(WINDOW).read_to_end(&mut head).ok()?;
    let mut tail = Vec::new();
    if len > WINDOW {
        let start = len.saturating_sub(WINDOW).max(WINDOW);
        file.seek(SeekFrom::Start(start - 1)).ok()?;
        let mut previous = [0u8; 1];
        file.read_exact(&mut previous).ok()?;
        file.seek(SeekFrom::Start(start)).ok()?;
        file.take(WINDOW).read_to_end(&mut tail).ok()?;
        if previous[0] != b'\n' {
            if let Some(first) = tail.iter().position(|b| *b == b'\n') {
                tail.drain(..=first);
            } else {
                tail.clear();
            }
        }
    }
    let mut identity = false;
    let mut first = None;
    let mut custom = None;
    for (in_head, bytes) in head
        .split(|b| *b == b'\n')
        .map(|line| (true, line))
        .chain(tail.split(|b| *b == b'\n').map(|line| (false, line)))
        .filter(|(_, line)| line.len() <= LINE)
    {
        let Ok(row) = serde_json::from_slice::<Value>(bytes) else {
            continue;
        };
        if row["sessionId"] != session || row["isSidechain"] == true {
            continue;
        }
        if let Some(root) = row["cwd"].as_str() {
            if !router_core::claude_code::same_root(root, cwd) {
                return None;
            }
            identity = true;
        }
        if row["type"] == "custom-title" {
            if let Some(title) = row["customTitle"].as_str().and_then(clean) {
                custom = Some(title);
            }
            continue;
        }
        if !in_head
            || first.is_some()
            || row["type"] != "user"
            || row["message"]["role"] != "user"
            || row["isMeta"] == true
            || row["isCompactSummary"] == true
        {
            continue;
        }
        let Some(text) = crate::claude_transcript::prompt_text(&row["message"]["content"]) else {
            continue;
        };
        let text = text.trim_start();
        if [
            "<",
            "Another Claude session sent a message:",
            "[Request interrupted by user",
        ]
        .iter()
        .any(|p| text.starts_with(p))
        {
            continue;
        }
        let first_line = text.lines().find(|line| !line.trim().is_empty())?.trim();
        let title: String = first_line.chars().take(80).collect();
        first = clean(&title).map(|title| {
            if first_line.chars().count() > 80 {
                format!("{title}…")
            } else {
                title
            }
        });
    }
    identity.then_some(custom.or(first)).flatten()
}
pub(crate) fn title(session: &str, cwd: &str) -> Option<String> {
    type Cache = HashMap<(std::path::PathBuf, String), ((u64, SystemTime), Option<String>)>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let root = crate::claude_transcript::projects_root()?;
    let path = crate::claude_transcript::find(&root, session)?;
    let meta = std::fs::metadata(&path).ok()?;
    let stamp = (meta.len(), meta.modified().ok()?);
    let key = (path.clone(), cwd.to_owned());
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    if let Some((old, title)) = cache.get(&key) {
        if *old == stamp {
            return title.clone();
        }
    }
    let title = read_at(&path, session, cwd);
    if cache.len() >= 128 {
        cache.clear();
    }
    cache.insert(key, (stamp, title.clone()));
    title
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "10000000-0000-4000-8000-000000000003";
    fn row(root: &Path, kind: &str, content: Value) -> Value {
        serde_json::json!({"sessionId":ID,"cwd":root,"type":kind,"message":{"role":"user","content":content}})
    }
    #[test]
    fn native_title_and_first_prompt_are_exact_identity_display_only() {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("native.jsonl");
        let rows = [
            row(
                d.path(),
                "user",
                serde_json::json!([{"type":"tool_result","content":"PRIVATE_TOOL_RESULT"}]),
            ),
            row(
                d.path(),
                "user",
                serde_json::json!("<command-name>/help</command-name>"),
            ),
            row(
                d.path(),
                "user",
                serde_json::json!("请分析清洁剂市场\n其它内容不进标题"),
            ),
        ];
        let body = rows
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&file, &body).unwrap();
        assert_eq!(
            read_at(&file, ID, d.path().to_str().unwrap()).as_deref(),
            Some("请分析清洁剂市场")
        );
        assert!(read_at(
            &file,
            "20000000-0000-4000-8000-000000000003",
            d.path().to_str().unwrap()
        )
        .is_none());
        assert!(read_at(&file, ID, d.path().join("wrong").to_str().unwrap()).is_none());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), body);
        use std::io::Write;
        let mut out = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        writeln!(
            out,
            "\n{}",
            serde_json::json!({"type":"custom-title","sessionId":ID,"customTitle":"正式重命名"})
        )
        .unwrap();
        assert_eq!(
            read_at(&file, ID, d.path().to_str().unwrap()).as_deref(),
            Some("正式重命名")
        );
    }
    #[test]
    fn late_rename_is_read_without_parsing_large_payloads_or_peer_messages() {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("native.jsonl");
        let mut peer = row(d.path(), "user", serde_json::json!("PRIVATE_PEER"));
        peer["isMeta"] = serde_json::json!(true);
        let prompt = row(d.path(), "user", serde_json::json!("原生首条消息"));
        let rename =
            serde_json::json!({"sessionId":ID,"type":"custom-title","customTitle":"尾部的新标题"});
        std::fs::write(
            &file,
            format!("{peer}\n{prompt}\n{}\n{rename}\n", "X".repeat(1024 * 1024)),
        )
        .unwrap();
        assert_eq!(
            read_at(&file, ID, d.path().to_str().unwrap()).as_deref(),
            Some("尾部的新标题")
        );
        // A complete tail line at the exact window boundary must not be discarded.
        let prefix = format!("{prompt}\n");
        let filler = "X".repeat(WINDOW as usize - prefix.len() - 1);
        std::fs::write(&file, format!("{prefix}{filler}\n{rename}\n")).unwrap();
        assert_eq!(
            read_at(&file, ID, d.path().to_str().unwrap()).as_deref(),
            Some("尾部的新标题")
        );
    }
}
