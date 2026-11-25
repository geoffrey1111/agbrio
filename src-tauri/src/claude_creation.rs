//! Explicit owner creation through the official Claude stream interface.
//! The existing authenticated plugin owns subsequent replies and decisions.
//! Capability discovery sends no user input; an admitted creation never retries.
use crate::{conversation_creation::Input, host_application::RouterCore};
use router_core::store::{codex_watch::WatchSnapshot, watch_reply::WatchReply};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const LIMIT: usize = 2 * 1024 * 1024;
struct Native {
    child: Child,
    input: ChildStdin,
    events: mpsc::Receiver<Value>,
    admitted: bool,
}
impl Native {
    fn open(
        directory: &str,
        id: Option<&str>,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Result<Self, String> {
        let executable =
            crate::claude_code_desktop::engine_at(&crate::claude_code_desktop::engine_roots())?;
        let mut cmd = Command::new(executable);
        cmd.current_dir(directory).args([
            "--print",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--replay-user-messages",
            "--permission-prompts",
            "none",
        ]);
        if let Some(id) = id {
            cmd.args(["--session-id", id]);
        } else {
            cmd.arg("--no-session-persistence");
        }
        if let Some(model) = model {
            cmd.args(["--model", model]);
        }
        if let Some(effort) = effort {
            cmd.args(["--effort", effort]);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        Self::spawn(cmd)
    }
    fn spawn(mut cmd: Command) -> Result<Self, String> {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd
            .spawn()
            .map_err(|_| "CLAUDE_CREATION_ENGINE_UNAVAILABLE")?;
        let input = child
            .stdin
            .take()
            .ok_or("CLAUDE_CREATION_ENGINE_UNAVAILABLE")?;
        let output = child
            .stdout
            .take()
            .ok_or("CLAUDE_CREATION_ENGINE_UNAVAILABLE")?;
        let (tx, events) = mpsc::sync_channel(64);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut line = Vec::new();
                match reader
                    .by_ref()
                    .take((LIMIT + 1) as u64)
                    .read_until(b'\n', &mut line)
                {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line.len() > LIMIT => break,
                    Ok(_) => {
                        let Ok(v) = serde_json::from_slice::<Value>(&line) else {
                            break;
                        };
                        if tx.send(v).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(Self {
            child,
            input,
            events,
            admitted: false,
        })
    }
    fn write(&mut self, value: &Value) -> Result<(), String> {
        let mut bytes =
            serde_json::to_vec(value).map_err(|_| "CLAUDE_CREATION_PROTOCOL_INVALID")?;
        bytes.push(b'\n');
        self.input
            .write_all(&bytes)
            .map_err(|_| "CLAUDE_CREATION_TRANSPORT_UNCONFIRMED".into())
    }
    fn initialize(&mut self) -> Result<Value, String> {
        self.write(&json!({"type":"control_request","request_id":"agbrio-create-initialize","request":{"subtype":"initialize","hooks":{}}}))?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("CLAUDE_CREATION_INITIALIZE_UNAVAILABLE".into());
            }
            let event = self
                .events
                .recv_timeout(remaining)
                .map_err(|_| "CLAUDE_CREATION_INITIALIZE_UNAVAILABLE")?;
            if event["type"] == "control_response"
                && event["response"]["request_id"] == "agbrio-create-initialize"
            {
                if event["response"]["subtype"] != "success" {
                    return Err("CLAUDE_CREATION_INITIALIZE_REFUSED".into());
                }
                return catalog(&event["response"]["response"]);
            }
            self.decline_unhandled(&event)?;
        }
    }
    fn decline_unhandled(&mut self, event: &Value) -> Result<(), String> {
        if event["type"] == "control_request" {
            let id = event["request_id"]
                .as_str()
                .ok_or("CLAUDE_CREATION_PROTOCOL_INVALID")?;
            // A managed/native denial is never converted into an approval. The native
            // Agbrio plugin may handle eligible suspended owner questions first.
            self.write(&json!({"type":"control_response","response":{"subtype":"error","request_id":id,"error":"No host permission override; retain native permissions"}}))?;
        }
        Ok(())
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        if !self.admitted {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn catalog(response: &Value) -> Result<Value, String> {
    let rows = response["models"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 100)
        .ok_or("CLAUDE_CREATION_CATALOG_UNAVAILABLE")?;
    let mut result = Vec::new();
    for row in rows {
        let model = row["value"]
            .as_str()
            .filter(|v| !v.is_empty() && v.len() <= 200 && !v.chars().any(char::is_control))
            .ok_or("CLAUDE_CREATION_CATALOG_INVALID")?;
        let efforts = row["supportedEffortLevels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|s| matches!(*s, "low" | "medium" | "high" | "xhigh" | "max"))
            .map(|s| json!({"reasoningEffort":s}))
            .collect::<Vec<_>>();
        result.push(json!({"model":model,"displayName":row["displayName"].as_str().unwrap_or(model),"supportedReasoningEfforts":efforts}));
    }
    Ok(json!({"models":result}))
}
fn validate(catalog: &Value, model: Option<&str>, effort: Option<&str>) -> Result<(), String> {
    if model.is_none() && effort.is_some() {
        return Err("CLAUDE_CREATION_MODEL_REQUIRED".into());
    }
    if let Some(model) = model {
        let row = catalog["models"]
            .as_array()
            .and_then(|a| a.iter().find(|v| v["model"] == model))
            .ok_or("CLAUDE_CREATION_MODEL_UNAVAILABLE")?;
        if let Some(effort) = effort {
            if !row["supportedReasoningEfforts"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v["reasoningEffort"] == effort))
            {
                return Err("CLAUDE_CREATION_EFFORT_UNAVAILABLE".into());
            }
        }
    }
    Ok(())
}
fn readiness() -> Result<(), String> {
    use crate::claude_code_desktop::{OfficialCli, PluginAdmin};
    let executable =
        crate::claude_code_desktop::engine_at(&crate::claude_code_desktop::engine_roots())?;
    let admin = OfficialCli { executable };
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let id = "agbrio-bridge@agbrio-observation";
    let list = admin.run(&args(&["plugin", "list", "--json"]), None)?;
    if !list.as_array().is_some_and(|a| {
        a.iter()
            .any(|v| v["id"] == id && v["scope"] == "user" && v["enabled"] == true)
    }) {
        return Err("CLAUDE_CREATION_CONNECT_PLUGIN_FIRST".into());
    }
    let configured = admin.run(&args(&["plugin", "configure", id, "--json"]), None)?;
    if configured["pluginId"] != id
        || !configured["configured"].as_array().is_some_and(|a| {
            ["hostUrl", "token"]
                .iter()
                .all(|k| a.iter().any(|v| v == k))
        })
    {
        return Err("CLAUDE_CREATION_CONNECT_PLUGIN_FIRST".into());
    }
    Ok(())
}
pub(crate) fn verify_host(core: &RouterCore, host: &crate::HostRuntime) -> Result<(), String> {
    use crate::claude_code_desktop::{OfficialCli, PluginAdmin};
    let origin = host
        .mobile_address()
        .filter(|a| a.ip().is_loopback() && a.port() > 0)
        .map(|a| format!("http://{a}"))
        .ok_or("CLAUDE_CREATION_HOST_UNAVAILABLE")?;
    let executable =
        crate::claude_code_desktop::engine_at(&crate::claude_code_desktop::engine_roots())?;
    let config = OfficialCli { executable }.run(
        &[
            "plugin".into(),
            "configure".into(),
            "agbrio-bridge@agbrio-observation".into(),
            "--json".into(),
        ],
        None,
    )?;
    if config["inputs"]["hostUrl"] != origin
        || !core.store.claude_connections()?.iter().any(|c| {
            !c.revoked
                && c.expires_at
                    > std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64
        })
    {
        return Err("CLAUDE_CREATION_CONNECT_CURRENT_HOST_FIRST".into());
    }
    Ok(())
}
pub(crate) fn models(directory: &str) -> Result<Value, String> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<Option<(Instant, Result<Value, String>)>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "CLAUDE_CREATION_CATALOG_UNAVAILABLE")?;
    if let Some((at, result)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(if result.is_ok() { 60 } else { 5 }) {
            return result.clone();
        }
    }
    let result = readiness().and_then(|_| Native::open(directory, None, None, None)?.initialize());
    *cache = Some((Instant::now(), result.clone()));
    result
}
pub(crate) fn preflight(
    directory: &str,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<(), String> {
    readiness()?;
    validate(&models(directory)?, model, effort)
}

/// Already claimed in the owner journal. Work continues under that same request
/// even if its HTTP response disappears. No detached shell or inherited prompt.
pub(crate) fn create(core: &RouterCore, input: Input, directory: String) -> Result<Value, String> {
    let request = input.request_id.clone();
    let owned = core.clone();
    std::thread::Builder::new().name("agbrio-claude-create".into()).spawn(move||{
  let outcome=run(&owned,&input,&directory);
  match outcome{
   Ok(value)=>{let _=owned.store.finish_owner_conversation_creation(&input.request_id,&value,false);},
   Err(_)=>{let _=owned.store.finish_owner_conversation_creation(&input.request_id,&json!({"error":"CONVERSATION_CREATION_UNCONFIRMED_CHECK_ORIGINAL_RECEIPT","automaticRetry":false}),true);}
  }
 }).map_err(|_|"CLAUDE_CREATION_WORKER_UNCONFIRMED")?;
    serde_json::to_value(core.store.owner_conversation_creation(&request)?)
        .map_err(|_| "CONVERSATION_CREATION_RESPONSE_INVALID".into())
}
fn run(core: &RouterCore, input: &Input, directory: &str) -> Result<Value, String> {
    let sid = uuid::Uuid::new_v4().to_string();
    let native = Native::open(
        directory,
        Some(&sid),
        input.model.as_deref(),
        input.effort.as_deref(),
    )?;
    run_opened(core, input, directory, &sid, native)
}
fn run_opened(
    core: &RouterCore,
    input: &Input,
    directory: &str,
    sid: &str,
    mut native: Native,
) -> Result<Value, String> {
    let key = format!("claude_code:{sid}");
    validate(
        &native.initialize()?,
        input.model.as_deref(),
        input.effort.as_deref(),
    )?;
    let outcome = (|| {
        native.admitted = true;
        native.write(&json!({"type":"user","uuid":input.request_id,"session_id":sid,"parent_tool_use_id":null,"message":{"role":"user","content":input.text}}))?;
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut proven = false;
        let mut watch = None;
        let mut settled = false;
        while Instant::now() < deadline {
            match native.events.recv_timeout(Duration::from_millis(100)) {
                Ok(event) => {
                    if event["type"] == "system" && event["subtype"] == "init" {
                        if event["session_id"] != sid
                            || !event["cwd"].as_str().is_some_and(|root| {
                                router_core::claude_code::same_root(root, directory)
                            })
                        {
                            return Err("CLAUDE_CREATION_IDENTITY_UNCONFIRMED".into());
                        }
                        if !proven {
                            core.store
                                .checkpoint_owner_conversation_created(&input.request_id, &sid)?;
                            core.store.enable_codex_watch(
                                &key,
                                "未命名 Claude Code 对话",
                                directory,
                                &WatchSnapshot {
                                    state: "RUNNING".into(),
                                    turn_id: None,
                                    item_id: None,
                                    text: String::new(),
                                },
                            )?;
                            let w = core.store.codex_chat_context(&key)?;
                            core.store.prepare_watch_reply(&WatchReply {
                                id: input.request_id.clone(),
                                thread_id: key.clone(),
                                cwd: w.cwd.clone(),
                                generation: w.generation,
                                source_sequence: None,
                                expected_turn_id: None,
                                mode: "SEND".into(),
                                text: input.text.clone(),
                                options: json!({}),
                                payload_hash: String::new(),
                                status: String::new(),
                                turn_id: None,
                                error_code: None,
                                created_at: 0,
                                updated_at: 0,
                            })?;
                            watch = Some(w);
                            proven = true;
                        }
                    }
                    native.decline_unhandled(&event)?;
                    if event["type"] == "result" {
                        settled = true;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
            if proven {
                let stored = crate::claude_transcript::latest(&sid)
                    .filter(|p| {
                        p.cwd.as_ref().is_some_and(|root| {
                            router_core::claude_code::same_root(root, directory)
                        })
                    })
                    .and_then(|p| {
                        p.turns.into_iter().find_map(|t| {
                            t.messages
                                .into_iter()
                                .find(|m| {
                                    m.id == input.request_id
                                        && m.role == "user"
                                        && m.text == input.text
                                })
                                .map(|m| (t.id, m.id))
                        })
                    });
                if let Some((turn, message)) = stored {
                    core.store
                        .finish_watch_reply(&input.request_id, "SENT", Some(&turn), None)?;
                    let delivery = core.store.watch_reply(&input.request_id)?;
                    let result = json!({"threadId":key,"watch":watch,"delivery":delivery,"nativeMessageId":message,"firstMessageSent":true,"automaticRetry":false});
                    return Ok(result);
                }
            }
            if settled {
                break;
            }
        }
        // Losing the independent stored-row proof cannot authorize resending input.
        if proven {
            let _ = core.store.finish_watch_reply(
                &input.request_id,
                "UNKNOWN",
                None,
                Some("CLAUDE_CREATION_STORED_ROW_UNCONFIRMED"),
            );
        }
        Err("CLAUDE_CREATION_STORED_ROW_UNCONFIRMED".into())
    })();
    // Once a user frame may have entered the native process, even failed receipt
    // observation must not stop it. One native owner remains, without replay.
    std::thread::spawn(move || {
        while let Ok(event) = native.events.recv() {
            if native.decline_unhandled(&event).is_err() {
                break;
            }
        }
    });
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_and_selection_follow_native_model_efforts() {
        let c=catalog(&json!({"models":[{"value":"fixture","displayName":"Fixture","supportedEffortLevels":["low","high","unsafe"]}]})).unwrap();
        assert_eq!(
            c["models"][0]["supportedReasoningEfforts"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(validate(&c, Some("fixture"), Some("high")).is_ok());
        assert!(validate(&c, Some("other"), None).is_err());
        assert!(validate(&c, Some("fixture"), Some("max")).is_err());
        assert!(validate(&c, None, Some("high")).is_err());
    }
    #[test]
    fn invalid_native_catalog_cannot_advertise_a_platform() {
        for value in [
            json!({}),
            json!({"models":[]}),
            json!({"models":[{"value":"\n"}]}),
        ] {
            assert!(catalog(&value).is_err());
        }
    }
    #[test]
    fn native_creation_requires_stored_row_and_duplicate_request_cannot_prompt_twice() {
        use std::sync::{Arc, Mutex};
        let d = tempfile::tempdir().unwrap();
        let root = d.path().to_string_lossy().into_owned();
        let projects = d.path().join("projects");
        std::fs::create_dir_all(projects.join("fixture")).unwrap();
        *crate::claude_transcript::TEST_PROJECTS.lock().unwrap() = Some(projects.clone());
        let core = RouterCore {
            store: Arc::new(crate::RouterStore::open_at(d.path().join("router.sqlite")).unwrap()),
            session: Arc::new(Mutex::new(Default::default())),
        };
        let input = Input {
            request_id: uuid::Uuid::new_v4().to_string(),
            provider: "CLAUDE_CODE".into(),
            directory: root.clone(),
            model: Some("fixture".into()),
            effort: Some("high".into()),
            text: "fictional first instruction".into(),
        };
        core.store
            .claim_owner_conversation_creation(
                &input.request_id,
                &serde_json::to_value(&input).unwrap(),
            )
            .unwrap();
        let sid = uuid::Uuid::new_v4().to_string();
        let script = r#"const fs=require('node:fs'),rl=require('node:readline').createInterface({input:process.stdin});const [sid,root,projects]=process.argv.slice(1);function out(v){process.stdout.write(JSON.stringify(v)+'\n')}rl.on('line',l=>{const v=JSON.parse(l);if(v.type==='control_request')return out({type:'control_response',response:{subtype:'success',request_id:v.request_id,response:{models:[{value:'fixture',supportedEffortLevels:['high']}]}}});if(v.type==='user'){fs.appendFileSync(root+'/calls.jsonl',JSON.stringify(v)+'\n');fs.writeFileSync(projects+'/fixture/'+sid+'.jsonl',JSON.stringify({type:'user',sessionId:sid,uuid:v.uuid,promptId:'native-turn',cwd:root,message:v.message})+'\n');out({type:'system',subtype:'init',session_id:sid,cwd:root});out({type:'control_request',request_id:'native-approval',request:{subtype:'can_use_tool',tool_name:'Bash',input:{command:'MUST NOT EXECUTE'}}});}if(v.type==='control_response'){fs.writeFileSync(root+'/permission.json',JSON.stringify(v));out({type:'result'});process.exit(0)}});"#;
        let mut cmd = Command::new("node");
        cmd.args(["-e", script, &sid, &root, projects.to_str().unwrap()]);
        let result = run_opened(&core, &input, &root, &sid, Native::spawn(cmd).unwrap()).unwrap();
        assert_eq!(result["firstMessageSent"], true);
        assert_eq!(result["nativeMessageId"], input.request_id);
        assert_eq!(result["delivery"]["status"], "SENT");
        core.store
            .finish_owner_conversation_creation(&input.request_id, &result, false)
            .unwrap();
        let duplicate = crate::conversation_creation::command(
            &core,
            crate::conversation_creation::Command::Create { input },
        )
        .unwrap();
        assert_eq!(duplicate["status"], "COMPLETE");
        assert_eq!(
            std::fs::read_to_string(d.path().join("calls.jsonl"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        for _ in 0..100 {
            if d.path().join("permission.json").is_file() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let permission: Value =
            serde_json::from_slice(&std::fs::read(d.path().join("permission.json")).unwrap())
                .unwrap();
        assert_eq!(permission["response"]["subtype"], "error");
        *crate::claude_transcript::TEST_PROJECTS.lock().unwrap() = None;
    }
}
