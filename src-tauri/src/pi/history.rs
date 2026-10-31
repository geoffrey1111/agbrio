//! Incremental native history. Images/replay signatures/hidden reasoning stay
//! in Pi's opaque original file, not in the application's in-memory read model.
use serde_json::{json, Value};
use std::{io::BufReader, path::Path};

#[derive(Default)]
pub(super) struct History {
    pub entries: Vec<Value>,
    pub cursor: Option<String>,
    pub initialized: bool,
    ids: std::collections::HashSet<String>,
}
impl History {
    pub fn initialize(&mut self, file: &Path) -> Result<(), String> {
        if self.initialized {
            return Ok(());
        }
        if file.is_file() {
            let mut reader = BufReader::new(
                std::fs::File::open(file).map_err(|_| "PI_HISTORY_FILE_UNAVAILABLE")?,
            );
            let header =
                super::transport::read_record(&mut reader)?.ok_or("PI_SESSION_HEADER_INVALID")?;
            if header["type"] != "session" {
                return Err("PI_SESSION_HEADER_INVALID".into());
            }
            while let Some(entry) = super::transport::read_record(&mut reader)? {
                self.append(entry)?;
            }
        }
        self.initialized = true;
        Ok(())
    }
    pub fn append(&mut self, entry: Value) -> Result<(), String> {
        let id = entry["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("PI_HISTORY_ID_INVALID")?
            .to_owned();
        if id.len() > 128
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        {
            return Err("PI_HISTORY_ID_INVALID".into());
        }
        if !self.ids.insert(id.clone()) {
            return Err("PI_HISTORY_DUPLICATE_ID".into());
        }
        self.entries.push(metadata(&entry));
        self.cursor = Some(id);
        Ok(())
    }
}
pub(super) fn failure_metadata(file:&Path,entry:&str)->Option<Value>{
 use std::io::{Read,Seek,SeekFrom};let mut f=std::fs::File::open(file).ok()?;let size=f.metadata().ok()?.len();let start=size.saturating_sub(512*1024);f.seek(SeekFrom::Start(start)).ok()?;let mut bytes=Vec::new();f.take(512*1024).read_to_end(&mut bytes).ok()?;
 for line in bytes.split(|b|*b==b'\n').rev(){if line.len()>256*1024{continue;}let Ok(v)=serde_json::from_slice::<Value>(line)else{continue};if v["id"]!=entry{continue;}let m=&v["message"];if m["role"]!="assistant"||m["stopReason"]!="error"{return None;}let code=provider_error(m["errorMessage"].as_str().unwrap_or(""));return Some(json!({"nativeEntryId":entry,"stopReason":"error","errorCode":code,"timestamp":v["timestamp"],"automaticRetry":false,"recovery":"NEW_REVIEWED_INPUT_AFTER_CONTEXT_REFRESH"}));}None
}
pub(super) fn provider_error(message:&str)->&'static str{
 let text=message.to_ascii_lowercase();if text.contains("connection error")||text.contains("econnreset")||text.contains("tls")||text.contains("timed out"){"PI_PROVIDER_CONNECTION_ERROR"}else if text.contains("context")&&(text.contains("limit")||text.contains("long")){"PI_PROVIDER_CONTEXT_LIMIT"}else if text.contains("429")||text.contains("rate limit"){"PI_PROVIDER_RATE_LIMIT"}else if text.contains("overload")||text.contains("capacity")||text.contains("503"){"PI_PROVIDER_TEMPORARY_UNAVAILABLE"}else{"PI_PROVIDER_ERROR"}
}
fn metadata(entry: &Value) -> Value {
    let mut output = json!({"id":entry["id"],"parentId":entry["parentId"],"timestamp":entry["timestamp"],"type":entry["type"]});
    if entry["type"] == "message" {
        let m = &entry["message"];
        let mut blocks = Vec::new();
        if let Some(text) = m["content"].as_str() {
            blocks.push(json!({"type":"text","text":text}));
        }
        if let Some(content) = m["content"].as_array() {
            for b in content {
                match b["type"].as_str() {
                    Some("text") => blocks.push(json!({"type":"text","text":b["text"].as_str()})),
                    Some("image") => {
                        blocks.push(json!({"type":"image","mimeType":b["mimeType"].as_str()}))
                    }
                    Some("toolCall") => blocks.push(
                        json!({"type":"toolCall","id":b["id"].as_str(),"name":b["name"].as_str()}),
                    ),
                    _ => {}
                }
            }
        }
        if !matches!(
            m["role"].as_str(),
            Some("user" | "assistant" | "toolResult")
        ) {
            blocks.clear();
        }
        output["message"] = json!({"role":m["role"],"content":blocks,"provider":m["provider"].as_str(),"model":m["model"].as_str(),"responseModel":m["responseModel"].as_str(),"providerThinkingLevel":m["providerThinkingLevel"].as_str(),"toolCallId":m["toolCallId"].as_str(),"toolName":m["toolName"].as_str(),"isError":m["isError"].as_bool(),"stopReason":m["stopReason"].as_str(),"errorMessage":m["errorMessage"].as_str().filter(|s|!s.is_empty()).map(provider_error)});
    } else if entry["type"] == "custom"
        && entry["customType"]
            .as_str()
            .is_some_and(|s| s.starts_with("agbrio."))
    {
        output["customType"] = entry["customType"].clone();
        let d = &entry["data"];
        output["data"] = json!({"runtimeId":d["runtimeId"].as_str(),"requestId":d["requestId"].as_str(),"payloadHash":d["payloadHash"].as_str(),"toolCallId":d["toolCallId"].as_str(),"accepted":d["accepted"].as_bool(),"runId":d["runId"].as_str(),"leafId":d["leafId"].as_str(),"aborted":d["aborted"].as_bool(),"requests":d["requests"].as_array().map(|v|v.iter().map(|p|json!({"requestId":p["requestId"].as_str(),"payloadHash":p["payloadHash"].as_str()})).collect::<Vec<_>>()),"remaining":d["remaining"].as_array().map(|v|v.iter().map(|p|json!({"requestId":p["requestId"].as_str(),"payloadHash":p["payloadHash"].as_str()})).collect::<Vec<_>>())});
        if entry["customType"]=="agbrio.capabilities"{output["data"]=json!({"runtimeId":d["runtimeId"].as_str(),"toolSetSource":d["toolSetSource"].as_str(),"tools":d["tools"].as_array().map(|v|v.iter().filter_map(Value::as_str).collect::<Vec<_>>()),"mcpServers":d["mcpServers"].clone(),"mcpReady":d["mcpReady"].as_bool()});}
        if entry["customType"]=="agbrio.skill_read"{output["data"]=json!({"runtimeId":d["runtimeId"].as_str(),"path":d["path"].as_str(),"sha256":d["sha256"].as_str(),"toolCallId":d["toolCallId"].as_str()});}
        if entry["customType"]=="agbrio.resources"{output["data"]=json!({"runtimeId":d["runtimeId"].as_str(),"instructions":d["instructions"].as_array().map(|v|v.iter().map(|f|json!({"path":f["path"].as_str(),"sha256":f["sha256"].as_str(),"via":f["via"].as_str(),"sourcePath":f["sourcePath"].as_str()})).collect::<Vec<_>>()),"nativeSkills":d["nativeSkills"].as_array().map(|v|v.iter().map(|s|json!({"name":s["name"].as_str(),"path":s["path"].as_str()})).collect::<Vec<_>>()),"skillRoots":d["skillRoots"].as_array().map(|v|v.iter().filter_map(Value::as_str).collect::<Vec<_>>()),"authorization":d["authorization"].as_str()});}
    }
    output
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_omits_large_media_and_all_private_replay_fields() {
        let raw = json!({"id":"one","parentId":null,"type":"message","message":{"role":"assistant","content":[{"type":"thinking","thinking":"PRIVATE_THINKING"},{"type":"text","text":"public","textSignature":"PRIVATE_SIGNATURE"},{"type":"image","mimeType":"image/png","data":"PRIVATE_BINARY"}],"errorMessage":"PRIVATE_KEY"}});
        let mut h = History::default();
        h.append(raw.clone()).unwrap();
        let serialized = h.entries[0].to_string();
        assert!(serialized.contains("public"));
        assert!(!serialized.contains("PRIVATE_"));
        assert!(h.append(raw).is_err());
        assert_eq!(h.cursor.as_deref(), Some("one"));
    }
}
