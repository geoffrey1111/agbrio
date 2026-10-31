//! One native process per exact Pi session. No provider or raw-RPC API escapes.
use super::{
    args, settings,
    transport::{Event, Rpc},
    Installation,
};
use router_core::store::pi::Selection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

fn tool_loadout(writable: bool, terminal: bool, servers: &[String]) -> Result<String, String> {
    let mut names = vec!["read".to_string(), "grep".into(), "find".into(), "ls".into()];
    if writable { names.extend(["edit".into(), "write".into()]); }
    if terminal { names.push("powershell".into()); }
    if writable && terminal {
        for server in servers {
            if server.is_empty() || !server.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return Err("PI_MCP_APPROVAL_INVALID".into());
            }
            names.push(format!("mcp__{server}__*"));
        }
    }
    Ok(names.join(","))
}

pub(super) struct Session {
    pub rpc: Rpc,
    pub events: mpsc::Receiver<Event>,
    pub overflow: Arc<AtomicBool>,
    pub native_id: String,
    pub native_file: PathBuf,
    pub selected: Selection,
    pub catalog: Value,
    pub skill_catalog:Vec<Value>,
    pub capabilities:Value,
    _guard_lock:std::fs::File,
    _context_lock:Option<std::fs::File>,
    _access_lock:Option<std::fs::File>,
    resources_snapshot:std::sync::Mutex<Option<super::resources::Resources>>,
    capability: String,
    directory: PathBuf,
    history: std::sync::Mutex<super::history::History>,
}
pub(super) fn plain(path: &Path) -> String {
    let s = path.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_owned()
}
fn runtime_event(event: Event) -> Option<Event> {
    match event {
        Event::Exited => Some(Event::Exited),
        Event::Record(value) => {
            let kind = value["type"].as_str()?;
            match kind {
                "agbrio_guard_error"=>Some(Event::Record(json!({"type":kind,"errorCode":value["errorCode"].as_str().filter(|s|s.starts_with("PI_")&&s.len()<80)}))),
                "agent_start" | "message_start" | "message_end" | "entry_appended" => {
                    Some(Event::Record(json!({"type":kind})))
                }
                "agent_settled" => Some(Event::Record(
                    json!({"type":kind,"aborted":value["aborted"]}),
                )),
                "tool_execution_start" | "tool_execution_end" => Some(Event::Record(
                    json!({"type":kind,"toolName":value["toolName"].as_str(),"toolCallId":value["toolCallId"].as_str(),"isError":value["isError"].as_bool()}),
                )),
                "extension_ui_request"
                    if value["message"].as_str().map(str::len).unwrap_or(0) <= 32768 =>
                {
                    Some(Event::Record(
                        json!({"type":kind,"id":value["id"],"method":value["method"],"title":value["title"],"message":value["message"],"timeout":value["timeout"]}),
                    ))
                }
                _ => None,
            }
        }
    }
}
impl Session {
    pub fn resource_epoch(&self)->String{format!("{:x}",Sha256::digest(self.capability.as_bytes()))}
    pub fn open(
        installation: &Installation,
        directory: &Path,
        owned: &Path,
        writable: bool,
        terminal: bool,
        requested: Option<&Selection>,
        resume: Option<(&str, &Path)>,
    ) -> Result<Self, String> {
        Self::open_checkpointed(
            installation,
            directory,
            owned,
            writable,
            terminal,
            requested,
            resume,
            None,
        )
    }
    pub fn open_checkpointed(
        installation: &Installation,
        directory: &Path,
        owned: &Path,
        writable: bool,
        terminal: bool,
        requested: Option<&Selection>,
        resume: Option<(&str, &Path)>,
        checkpoint: Option<
            &mut dyn FnMut(&str, &Path, Option<&Selection>) -> Result<(), String>,
        >,
    ) -> Result<Self, String> {
        Self::open_scoped(installation,directory,owned,writable,terminal,requested,resume,checkpoint,None)
    }
    pub fn open_scoped(
        installation: &Installation,
        directory: &Path,
        owned: &Path,
        writable: bool,
        terminal: bool,
        requested: Option<&Selection>,
        resume: Option<(&str, &Path)>,
        mut checkpoint: Option<
            &mut dyn FnMut(&str, &Path, Option<&Selection>) -> Result<(), String>,
        >,
        access:Option<(&router_core::store::pi_policy::PiTaskAccess,&super::resources::Resources)>,
    ) -> Result<Self, String> {
        if !directory.is_absolute()
            || !directory.is_dir()
            || !owned.is_absolute()
            || !owned.is_dir()
        {
            return Err("PI_START_TARGET_INVALID".into());
        }
        let owned = std::fs::canonicalize(owned).map_err(|_| "PI_SESSION_DIRECTORY_INVALID")?;
        let guard = owned.join("agbrio-guard.mjs");
        let bytes = include_bytes!("../../../tools/pi-adapter/guard.mjs");
        if let Ok(meta) = std::fs::symlink_metadata(&guard) {
            if meta.file_type().is_symlink() {
                return Err("PI_GUARD_IDENTITY_INVALID".into());
            }
        }
        std::fs::write(&guard, bytes).map_err(|_| "PI_GUARD_UNAVAILABLE")?;
        if std::fs::read(&guard).map_err(|_| "PI_GUARD_UNAVAILABLE")? != bytes {
            return Err("PI_GUARD_IDENTITY_INVALID".into());
        }
        let mut guard_open=std::fs::OpenOptions::new();guard_open.read(true);
        #[cfg(windows)] {use std::os::windows::fs::OpenOptionsExt;guard_open.share_mode(1);}
        let guard_lock=guard_open.open(&guard).map_err(|_|"PI_GUARD_UNAVAILABLE")?;
        let native_root = owned.join("sessions");
        std::fs::create_dir_all(&native_root).map_err(|_| "PI_SESSION_DIRECTORY_INVALID")?;
        let mut argv = args(installation, Path::new(&plain(&native_root)));
        argv.retain(|s| s != "--no-tools");
        argv.extend([
            "--extension".into(),
            plain(&guard),
            "--tools".into(),
            tool_loadout(writable, terminal, access.map(|(_, r)| r.mcp_servers.as_slice()).unwrap_or(&[]))?,
        ]);
        if let Some((policy,resources))=access{
            if let Some(file)=&resources.append_file{argv.extend(["--append-system-prompt".into(),file.clone()]);}
            if policy.inherit_context{argv.retain(|s|s!="--no-context-files");for file in &resources.skill_files{argv.extend(["--skill".into(),file.path.clone()]);}}
        }
        let mut integration_paths=Vec::new();
        if let Some((_,resources))=access{
            let mut selected=Vec::new();
            for extension in &resources.extensions{selected.extend(["--extension".to_string(),extension.path.clone()]);integration_paths.push(extension.path.clone());}
            if !resources.mcp_servers.is_empty(){let shim=owned.join("agbrio-mcp.mjs");let bytes=include_bytes!("../../../tools/pi-adapter/mcp.mjs");if std::fs::symlink_metadata(&shim).is_ok_and(|m|m.file_type().is_symlink()){return Err("PI_MCP_SHIM_INVALID".into());}std::fs::write(&shim,bytes).map_err(|_|"PI_MCP_SHIM_INVALID")?;selected.extend(["--extension".into(),plain(&shim)]);integration_paths.push(plain(&shim));}
            let index=argv.iter().position(|s|s=="--extension").ok_or("PI_GUARD_LOAD_UNVERIFIED")?;argv.splice(index..index,selected);
        }
        let context_lock=if let Some((_,resources))=access{if let Some(file)=&resources.append_file{let mut options=std::fs::OpenOptions::new();options.read(true);#[cfg(windows)]{use std::os::windows::fs::OpenOptionsExt;options.share_mode(1);}Some(options.open(file).map_err(|_|"PI_CONTEXT_UNAVAILABLE")?)}else{None}}else{None};
        if let Some((_, file)) = resume {
            if !file.is_file() {
                return Err("PI_ORIGINAL_SESSION_MISSING".into());
            }
            let file = std::fs::canonicalize(file).map_err(|_| "PI_ORIGINAL_SESSION_MISSING")?;
            if !file.starts_with(&native_root) {
                return Err("PI_SESSION_FILE_OUT_OF_SCOPE".into());
            }
            verify_session_header(&file, resume.unwrap().0, directory)?;
            argv.extend(["--session".into(), plain(&file)]);
        }
        let capability = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let (tx, events) = mpsc::sync_channel(256);
        let overflow = Arc::new(AtomicBool::new(false));
        let failed = overflow.clone();
        let access_json=access.map(|(policy,resources)|serde_json::json!({"roots":policy.roots.iter().map(|p|serde_json::json!({"path":p,"writable":policy.writable})).collect::<Vec<_>>(),"resources":resources})).map(|v|serde_json::to_string(&v).map_err(|_|"PI_ACCESS_INVALID")).transpose()?;
        if access_json.as_ref().is_some_and(|v|v.len()>1024*1024)||argv.iter().map(|v|v.encode_utf16().count()+3).sum::<usize>()>30000{return Err("PI_CONTEXT_TOO_LARGE".into());}
        let access_file=owned.join("agbrio-access.json");
        let access_lock=if let Some(json)=&access_json{if std::fs::symlink_metadata(&access_file).is_ok_and(|m|m.file_type().is_symlink()){return Err("PI_ACCESS_INVALID".into());}std::fs::write(&access_file,json).map_err(|_|"PI_ACCESS_INVALID")?;let mut options=std::fs::OpenOptions::new();options.read(true);#[cfg(windows)]{use std::os::windows::fs::OpenOptionsExt;options.share_mode(1);}Some(options.open(&access_file).map_err(|_|"PI_ACCESS_INVALID")?)}else{None};
        let access_spec=access_json.as_ref().map(|json|(plain(&access_file),format!("{:x}",Sha256::digest(json.as_bytes()))));
        let rpc = Rpc::start_configured(
            &installation.node,
            &argv,
            directory,
            &plain(directory),
            writable,
            Some((&capability, terminal)),
            access_spec.as_ref().map(|(p,h)|(p.as_str(),h.as_str())),
            access.is_some_and(|(p,_)|p.administrator),
            Arc::new(move |event| {
                let Some(event) = runtime_event(event) else {
                    return;
                };
                if tx.try_send(event).is_err() {
                    failed.store(true, Ordering::SeqCst);
                }
            }),
        )?;
        let commands = settings::call(&rpc, json!({"type":"get_commands"}))?;
        let commands = commands["commands"]
            .as_array()
            .ok_or("PI_GUARD_LOAD_UNVERIFIED")?;
        let names = [
            "agbrio-control",
            "agbrio-clear-control",
            "agbrio-forget-control",
            "agbrio-guard-ready",
            "agbrio-capabilities",
        ];
        if commands.iter().filter(|c|c["source"]=="extension"&&c["sourceInfo"]["path"].as_str().is_some_and(|p|plain(Path::new(p))==plain(&guard))).count()!=names.len()
            || commands.iter().any(|c|c["source"]=="extension"&&!c["sourceInfo"]["path"].as_str().is_some_and(|p|plain(Path::new(p))==plain(&guard)||integration_paths.iter().any(|v|plain(Path::new(p))==*v)))
            || commands.iter().any(|c|c["source"]!="extension"&&!(c["source"]=="skill"&&access.is_some_and(|(_,r)|c["sourceInfo"]["path"].as_str().is_some_and(|p|std::fs::canonicalize(p).ok().is_some_and(|p|r.skill_files.iter().any(|file|plain(&p)==file.path))))))
            || names.iter().any(|name| {
                !commands.iter().any(|c| {
                    c["name"] == *name
                        && c["source"] == "extension"
                        && c["sourceInfo"]["path"]
                            .as_str()
                            .is_some_and(|p| plain(Path::new(p)) == plain(&guard))
                })
            })
        {
            return Err("PI_GUARD_LOAD_UNVERIFIED".into());
        }
        let state = settings::call(&rpc, json!({"type":"get_state"}))?;
        let native_id = state["sessionId"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("PI_SESSION_ID_UNCONFIRMED")?
            .to_owned();
        let native_file = PathBuf::from(
            state["sessionFile"]
                .as_str()
                .ok_or("PI_SESSION_FILE_UNCONFIRMED")?,
        );
        if uuid::Uuid::parse_str(&native_id).is_err() {
            return Err("PI_SESSION_ID_INVALID".into());
        }
        let expected_root = PathBuf::from(plain(&native_root));
        if !native_file.is_absolute()
            || !native_file.starts_with(&expected_root)
            || native_file.extension().and_then(|s| s.to_str()) != Some("jsonl")
        {
            return Err("PI_SESSION_FILE_OUT_OF_SCOPE".into());
        }
        if let Some((id, file)) = resume {
            if id != native_id || plain(file) != plain(&native_file) {
                return Err("PI_SESSION_IDENTITY_CHANGED".into());
            }
        }
        if let Some(callback) = checkpoint.as_mut() {
            callback(
                &native_id,
                &native_file,
                settings::actual(&state).ok().as_ref(),
            )?;
        }
        let (verified, selected, catalog) = match settings::handshake(&rpc, requested) {
            Ok(v) => v,
            Err(error) => {
                if let Ok(observed) = settings::call(&rpc, json!({"type":"get_state"})) {
                    if observed["sessionId"] == native_id
                        && observed["sessionFile"].as_str() == Some(&plain(&native_file))
                    {
                        if let Some(callback) = checkpoint.as_mut() {
                            callback(
                                &native_id,
                                &native_file,
                                settings::actual(&observed).ok().as_ref(),
                            )?;
                        }
                    }
                }
                return Err(error);
            }
        };
        if verified["sessionId"] != native_id
            || verified["sessionFile"].as_str() != Some(&plain(&native_file))
        {
            return Err("PI_SESSION_IDENTITY_CHANGED".into());
        }
        if !matches!(selected.provider.as_str(), "openai" | "anthropic") {
            return Err("PI_PROVIDER_NOT_APPROVED".into());
        }
        let response=settings::call(&rpc,json!({"type":"prompt","message":format!("/agbrio-capabilities {}",capability)}))?;
        if response["disposition"]!="handled"{return Err("PI_CAPABILITY_RECEIPT_UNCONFIRMED".into());}
        let entries=settings::call(&rpc,json!({"type":"get_entries"}))?;
        let capabilities=entries["entries"].as_array().into_iter().flatten().rev().find(|v|v["customType"]=="agbrio.capabilities").map(|v|v["data"].clone()).ok_or("PI_CAPABILITY_RECEIPT_UNCONFIRMED")?;
        if capabilities["toolSetSource"] != "ACTIVE_MODEL" || capabilities["mcpReady"] != true {
            return Err("PI_ACTIVE_TOOLS_UNCONFIRMED".into());
        }
        Ok(Self {
            capabilities,
            rpc,
            events,
            overflow,
            native_id,
            native_file,
            selected,
            catalog,
            skill_catalog:commands.iter().filter(|c|c["source"]=="skill").map(|c|json!({"name":c["name"],"path":c["sourceInfo"]["path"]})).collect(),
            _guard_lock:guard_lock,
            _context_lock:context_lock,
            _access_lock:access_lock,
            resources_snapshot:std::sync::Mutex::new(access.map(|(_,r)|r.clone())),
            capability,
            directory: super::permissions::canonical_directory(&plain(directory))?,
            history: std::sync::Mutex::default(),
        })
    }
    pub fn history(&self) -> Result<Value, String> {
        if self.overflow.load(Ordering::SeqCst) {
            return Err("PI_EVENT_STREAM_INCOMPLETE".into());
        }
        let state = settings::call(&self.rpc, json!({"type":"get_state"}))?;
        if state["sessionId"] != self.native_id
            || state["sessionFile"].as_str() != Some(&plain(&self.native_file))
        {
            return Err("PI_SESSION_IDENTITY_CHANGED".into());
        }
        let mut history = self.history.lock().map_err(|_| "PI_HISTORY_UNAVAILABLE")?;
        history.initialize(&self.native_file)?;
        let mut command = json!({"type":"get_entries"});
        if let Some(cursor) = &history.cursor {
            command["since"] = json!(cursor);
        }
        let delta = settings::call(&self.rpc, command)?;
        for entry in delta["entries"].as_array().ok_or("PI_HISTORY_INVALID")? {
            history.append(entry.clone())?;
        }
        if let Some(receipt)=history.entries.iter().rev().find(|e|e["customType"]=="agbrio.resources"&&e["data"]["runtimeId"]==self.resource_epoch()){
            if let Some(snapshot)=self.resources_snapshot.lock().map_err(|_|"PI_CONTEXT_UNAVAILABLE")?.as_mut(){if let Some(files)=receipt["data"]["instructions"].as_array(){snapshot.instructions.retain(|rule|files.iter().any(|f|f["path"]==rule.path));}}
        }
        Ok(
            json!({"entries":history.entries,"leafId":delta["leafId"],"isStreaming":state["isStreaming"],"isCompacting":state["isCompacting"],"pendingMessageCount":state["pendingMessageCount"]}),
        )
    }
    pub fn directory_intact(&self) -> Result<(), String> {
        if super::permissions::canonical_directory(&plain(&self.directory))? != self.directory {
            return Err("PI_DIRECTORY_CHANGED".into());
        }
        Ok(())
    }
    /// Must be called under the task's single writer lock after a durable claim.
    /// Hashes bind the invisible native intent to the exact public user input.
    pub fn submit(
        &self,
        request: &str,
        payload_hash: &str,
        text: &str,
        images: &[Value],
        mode: &str,
    ) -> Result<Value, String> {
        self.directory_intact()?;
        if self.resources_snapshot.lock().map_err(|_|"PI_CONTEXT_UNAVAILABLE")?.as_ref().is_some_and(|r|!r.unchanged()){return Err("PI_CONTEXT_CHANGED_BEFORE_SEND".into());}
        if text.trim_start().starts_with('/')
            || text.contains('\0')
            || request.is_empty()
            || request.len() > 128
            || !request
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
            || payload_hash.len() != 64
            || !payload_hash
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err("PI_INPUT_INVALID".into());
        }
        let state = settings::call(&self.rpc, json!({"type":"get_state"}))?;
        let actual = settings::actual(&state)?;
        if state["sessionId"] != self.native_id
            || actual.provider != self.selected.provider
            || actual.model != self.selected.model
            || actual.thinking != self.selected.thinking
        {
            return Err("PI_SESSION_SETTINGS_CHANGED".into());
        }
        let streaming = state["isStreaming"]
            .as_bool()
            .ok_or("PI_STATE_UNCONFIRMED")?;
        if mode == "SEND" && streaming {
            return Err("PI_RUNNING_USE_STEER".into());
        }
        if !matches!(mode, "AUTO" | "SEND" | "STEER" | "QUEUE") {
            return Err("PI_SEND_MODE_INVALID".into());
        }
        let text_hash = format!("{:x}", Sha256::digest(text.as_bytes()));
        let mut image_hash = Sha256::new();
        image_hash.update((images.len() as u32).to_be_bytes());
        let mut total = 0;
        if images.len() > 4 {
            return Err("PI_ATTACHMENT_LIMIT".into());
        }
        for image in images {
            use base64ct::Encoding;
            let mime = image["mimeType"].as_str().ok_or("PI_IMAGE_INVALID")?;
            if image["type"] != "image" || !matches!(mime, "image/png" | "image/jpeg") {
                return Err("PI_IMAGE_INVALID".into());
            }
            let raw =
                base64ct::Base64::decode_vec(image["data"].as_str().ok_or("PI_IMAGE_INVALID")?)
                    .map_err(|_| "PI_IMAGE_INVALID")?;
            total += raw.len();
            if raw.is_empty() || raw.len() > 10 * 1024 * 1024 || total > 40 * 1024 * 1024 {
                return Err("PI_ATTACHMENT_LIMIT".into());
            }
            let format = image::guess_format(&raw).map_err(|_| "PI_IMAGE_INVALID")?;
            if !matches!(
                (mime, format),
                ("image/png", image::ImageFormat::Png) | ("image/jpeg", image::ImageFormat::Jpeg)
            ) {
                return Err("PI_IMAGE_INVALID".into());
            }
            image_hash.update((mime.len() as u32).to_be_bytes());
            image_hash.update(mime);
            image_hash.update((raw.len() as u64).to_be_bytes());
            image_hash.update(&raw);
        }
        let marker = format!(
            "/agbrio-control {} {} {} {} {:x}",
            self.capability,
            request,
            payload_hash,
            text_hash,
            image_hash.finalize()
        );
        let handled = settings::call(&self.rpc, json!({"type":"prompt","message":marker}))?;
        if handled["disposition"] != "handled" {
            return Err("PI_INTENT_RECEIPT_UNCONFIRMED".into());
        }
        // Guard-load failure must never degrade to unguarded native execution.
        let history = self.history()?;
        if !history["entries"]
            .as_array()
            .ok_or("PI_HISTORY_INVALID")?
            .iter()
            .any(|e| {
                e["type"] == "custom"
                    && e["customType"] == "agbrio.intent"
                    && e["data"]["requestId"] == request
                    && e["data"]["payloadHash"] == payload_hash
            })
        {
            return Err("PI_INTENT_RECEIPT_UNCONFIRMED".into());
        }
        let native_images = images
            .iter()
            .map(|image| json!({"type":"image","mimeType":image["mimeType"],"data":image["data"]}))
            .collect::<Vec<_>>();
        let mut command = json!({"type":"prompt","message":text,"images":native_images});
        if mode != "SEND" {
            command["streamingBehavior"] =
                json!(if mode == "QUEUE" { "followUp" } else { "steer" });
        }
        let accepted = match settings::call(&self.rpc, command) {
            Ok(v) => v,
            Err(error) => {
                if error == "PI_PROMPT_REJECTED" {
                    let _ = settings::call(
                        &self.rpc,
                        json!({"type":"prompt","message":format!("/agbrio-forget-control {} {request} {payload_hash}",self.capability)}),
                    );
                }
                return Err(error);
            }
        };
        if !matches!(accepted["disposition"].as_str(), Some("started" | "queued")) {
            let _ = settings::call(
                &self.rpc,
                json!({"type":"prompt","message":format!("/agbrio-forget-control {} {request} {payload_hash}",self.capability)}),
            );
            return Err("PI_INPUT_NOT_ACCEPTED".into());
        }
        let history = self.history()?;
        let receipt = history["entries"]
            .as_array()
            .ok_or("PI_HISTORY_INVALID")?
            .iter()
            .find(|e| {
                e["type"] == "custom"
                    && e["customType"] == "agbrio.input"
                    && e["data"]["requestId"] == request
                    && e["data"]["payloadHash"] == payload_hash
            })
            .and_then(|e| e["id"].as_str())
            .ok_or("PI_INPUT_RECEIPT_UNCONFIRMED")?;
        Ok(
            json!({"disposition":accepted["disposition"],"nativeEntryId":receipt,"effectiveMode":if accepted["disposition"]=="started"{"SEND"}else if mode=="QUEUE"{"QUEUE"}else{"STEER"}}),
        )
    }
    pub fn cancel(&self) -> Result<(), String> {
        settings::call(&self.rpc, json!({"type":"clear_queue"}))?;
        let clear = settings::call(
            &self.rpc,
            json!({"type":"prompt","message":format!("/agbrio-clear-control {}",self.capability)}),
        )?;
        if clear["disposition"] != "handled" {
            return Err("PI_QUEUE_CLEAR_UNCONFIRMED".into());
        }
        settings::call(&self.rpc, json!({"type":"abort"}))?;
        let state = settings::call(&self.rpc, json!({"type":"get_state"}))?;
        if state["isStreaming"] != false || state["isCompacting"] != false {
            return Err("PI_ABORT_UNCONFIRMED".into());
        }
        Ok(())
    }
    pub fn close(&self) -> Result<(), String> {
        self.rpc.close(Duration::from_secs(5))
    }
    pub fn verify_durable(&self, directory: &Path) -> Result<bool, String> {
        if !self.native_file.exists() {
            return Ok(false);
        }
        verify_session_header(&self.native_file, &self.native_id, directory)?;
        Ok(true)
    }
}

pub(super) fn verify_session_header(file: &Path, id: &str, directory: &Path) -> Result<(), String> {
    use std::io::{BufRead, Read};
    let metadata = std::fs::symlink_metadata(file).map_err(|_| "PI_ORIGINAL_SESSION_MISSING")?;
    if metadata.file_type().is_symlink() {
        return Err("PI_SESSION_FILE_IDENTITY_CHANGED".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("PI_SESSION_FILE_IDENTITY_CHANGED".into());
        }
    }
    let mut line = String::new();
    std::io::BufReader::new(std::fs::File::open(file).map_err(|_| "PI_ORIGINAL_SESSION_MISSING")?)
        .take(65537)
        .read_line(&mut line)
        .map_err(|_| "PI_SESSION_HEADER_INVALID")?;
    if line.len() > 65536 || !line.ends_with('\n') {
        return Err("PI_SESSION_HEADER_INVALID".into());
    }
    let header: Value = serde_json::from_str(&line).map_err(|_| "PI_SESSION_HEADER_INVALID")?;
    let expected = plain(directory);
    let actual = header["cwd"].as_str().ok_or("PI_SESSION_HEADER_INVALID")?;
    if header["type"] != "session"
        || header["id"] != id
        || !router_core::claude_code::same_root(actual, &expected)
    {
        return Err("PI_SESSION_IDENTITY_CHANGED".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tools_include_only_owner_selected_mcp_namespaces_with_full_workspace_grant() {
        let servers = vec!["unityMCP".into()];
        assert_eq!(tool_loadout(true, true, &servers).unwrap(), "read,grep,find,ls,edit,write,powershell,mcp__unityMCP__*");
        assert_eq!(tool_loadout(false, true, &servers).unwrap(), "read,grep,find,ls,powershell");
        assert_eq!(tool_loadout(true, false, &servers).unwrap(), "read,grep,find,ls,edit,write");
        assert_eq!(tool_loadout(false, false, &servers).unwrap(), "read,grep,find,ls");
        for bad in ["", "*", "unityMCP__*,mcp__other", "unity-MCP"] {
            assert!(tool_loadout(true, true, &[bad.into()]).is_err());
        }
    }
    #[test]
    #[ignore = "installed Pi, isolated owned directories; metadata only, no model prompt"]
    fn guarded_session_has_native_identity_and_internal_intent_receipt() {
        let root = PathBuf::from(std::env::var_os("AGBRIO_PI_QA_ROOT").expect("owned QA root"));
        assert!(root.is_absolute() && root.is_dir());
        let directory = root.join("guarded-project");
        let owned = root.join("guarded-runtime");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::create_dir_all(&owned).unwrap();
        let selected = Selection {
            provider: "openai".into(),
            model: "gpt-6-luna".into(),
            thinking: "low".into(),
        };
        let session = Session::open(
            &super::super::installation().unwrap(),
            &directory,
            &owned,
            false,
            false,
            Some(&selected),
            None,
        )
        .unwrap();
        let marker = format!(
            "/agbrio-control {} metadata {} {} {}",
            session.capability,
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64)
        );
        let reply =
            settings::call(&session.rpc, json!({"type":"prompt","message":marker})).unwrap();
        assert_eq!(reply["disposition"], "handled");
        let history = session.history().unwrap();
        assert!(history["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["customType"] == "agbrio.intent" && e["data"]["requestId"] == "metadata"));
        assert_eq!(session.selected.thinking, "low");
        session.close().unwrap();
    }
}
