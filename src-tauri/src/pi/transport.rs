//! Private Pi RPC transport. No shell, auth-file access, prompt retry or PID routing.
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

const MAX_FRAME: usize = 100 * 1024 * 1024;
type Reply = mpsc::SyncSender<Result<Value, String>>;
pub(super) enum Event {
    Record(Value),
    Exited,
}
struct Inner {
    process: Mutex<ManagedProcess>,
    outbound: mpsc::SyncSender<Vec<u8>>,
    pending: Mutex<HashMap<String, Reply>>,
    closed: AtomicBool,
}
enum ManagedProcess {
 Direct {child:Child,_job:super::windows_job::OwnedJob,identity:super::elevation::Identity},
 Elevated(super::elevation::Elevated),
}
impl ManagedProcess {
 fn exited(&mut self)->Result<bool,String>{match self{Self::Direct{child,_job,..}=>{if child.try_wait().map_err(|_|"PI_PROCESS_UNAVAILABLE")?.is_some(){_job.terminate_and_wait(Duration::from_secs(3))?;Ok(true)}else{Ok(false)}},Self::Elevated(p)=>Ok(p.closed())}}
 fn stop(&mut self){match self{Self::Direct{child,_job,..}=>{let _=child.kill();let _=child.wait();let _=_job.terminate_and_wait(Duration::from_secs(3));},Self::Elevated(p)=>p.stop()}}
 fn identity(&self)->super::elevation::Identity{match self{Self::Direct{identity,..}=>identity.clone(),Self::Elevated(p)=>p.identity.clone()}}
}
pub(super) struct Rpc {
    inner: Arc<Inner>,
}
impl Rpc {
    pub fn start(
        node: &Path,
        argv: &[String],
        cwd: &Path,
        scope: &str,
        write: bool,
        sink: Arc<dyn Fn(Event) + Send + Sync>,
    ) -> Result<Self, String> {
        Self::start_guarded(node, argv, cwd, scope, write, None, sink)
    }
    pub fn start_guarded(
        node: &Path,
        argv: &[String],
        cwd: &Path,
        scope: &str,
        write: bool,
        guard: Option<(&str, bool)>,
        sink: Arc<dyn Fn(Event) + Send + Sync>,
    ) -> Result<Self, String> {
        Self::start_configured(node,argv,cwd,scope,write,guard,None,false,sink)
    }
    pub fn start_configured(
        node:&Path,argv:&[String],cwd:&Path,scope:&str,write:bool,
        guard:Option<(&str,bool)>,access:Option<(&str,&str)>,administrator:bool,
        sink:Arc<dyn Fn(Event)+Send+Sync>,
    )->Result<Self,String>{
        if !node.is_file() || !cwd.is_absolute() || !cwd.is_dir() {
            return Err("PI_START_TARGET_INVALID".into());
        }
        let mut cmd = Command::new(node);
        // Build a platform environment rather than inheriting the management
        // agent's credential/plugin/session variables. Pi reads its own auth.
        cmd.env_clear();
        for key in [
            "SystemRoot",
            "WINDIR",
            "USERPROFILE",
            "HOME",
            "HOMEDRIVE",
            "HOMEPATH",
            "APPDATA",
            "LOCALAPPDATA",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramData",
            "PATH",
            "PATHEXT",
            "ComSpec",
            "TEMP",
            "TMP",
            "LANG",
            "LC_ALL",
            "TZ",
            "SYSTEMDRIVE",
            "NUMBER_OF_PROCESSORS",
        ] {
            if let Some(value) = std::env::var_os(key) {
                cmd.env(key, value);
            }
        }
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "no_proxy",
        ] {
            if let Ok(value) = std::env::var(key) {
                if value.trim().is_empty() {
                    continue;
                }
                if !key.to_ascii_lowercase().contains("no_proxy") {
                    let proxy =
                        url::Url::parse(&value).map_err(|_| "PI_PROXY_CONFIGURATION_INVALID")?;
                    if !proxy.username().is_empty()
                        || proxy.password().is_some()
                        || proxy.query().is_some()
                    {
                        return Err("PI_PROXY_CREDENTIAL_INHERITANCE_DISABLED".into());
                    }
                }
                cmd.env(key, value);
            }
        }
        if std::env::var("NODE_USE_ENV_PROXY").is_ok_and(|v| v == "1") {
            cmd.env("NODE_USE_ENV_PROXY", "1");
        }
        cmd.args(argv)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Pi alone reads its own credentials. Never borrow another agent's API key,
        // endpoint override, module loader or extension environment.
        for key in [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "OPENAI_BASE_URL",
            "ANTHROPIC_BASE_URL",
            "NODE_OPTIONS",
            "NODE_PATH",
        ] {
            cmd.env_remove(key);
        }
        cmd.env("AGBRIO_PI_SCOPE", scope)
            .env("AGBRIO_PI_WRITE", if write { "1" } else { "0" });
        cmd.env_remove("AGBRIO_PI_CAP")
            .env_remove("AGBRIO_PI_TERMINAL");
        if let Some((cap, terminal)) = guard {
            cmd.env("AGBRIO_PI_CAP", cap)
                .env("AGBRIO_PI_TERMINAL", if terminal { "1" } else { "0" });
        }
        if let Some((file,digest))=access{cmd.env("AGBRIO_PI_ACCESS_FILE",file).env("AGBRIO_PI_ACCESS_DIGEST",digest);}
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let (process,input,output,error):(ManagedProcess,Box<dyn Write+Send>,Box<dyn Read+Send>,Box<dyn Read+Send>)=if administrator&&!super::elevation::identity(None)?.administrator{
            let (process,input,output)=super::elevation::start(&cmd,argv,cwd)?;
            (ManagedProcess::Elevated(process),input,output,Box::new(std::io::empty()))
        }else{
        let mut child = cmd.spawn().map_err(|_| "PI_PROCESS_START_FAILED")?;
        let job = match super::windows_job::OwnedJob::attach(&child) {
            Ok(j) => j,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let input = child.stdin.take().ok_or("PI_STDIN_UNAVAILABLE")?;
        let output = child.stdout.take().ok_or("PI_STDOUT_UNAVAILABLE")?;
        let error = child.stderr.take().ok_or("PI_STDERR_UNAVAILABLE")?;
        let identity=super::elevation::identity(Some(&child))?;
        if administrator&&!identity.administrator{let _=child.kill();let _=child.wait();return Err("PI_ADMINISTRATOR_TOKEN_MISMATCH".into());}
        (ManagedProcess::Direct{child,_job:job,identity},Box::new(input),Box::new(output),Box::new(error))
        };
        let (outbound, incoming) = mpsc::sync_channel::<Vec<u8>>(8);
        let inner = Arc::new(Inner {
            process: Mutex::new(process),
            outbound,
            pending: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
        });
        let writer = Arc::downgrade(&inner);
        std::thread::spawn(move || {
            let mut input = input;
            while let Ok(bytes) = incoming.recv() {
                if bytes.is_empty() {
                    break;
                }
                if input.write_all(&bytes).and_then(|_| input.flush()).is_err() {
                    if let Some(state) = writer.upgrade() {
                        state.closed.store(true, Ordering::SeqCst);
                        if let Ok(mut pending) = state.pending.lock() {
                            for (_, reply) in pending.drain() {
                                let _ = reply.send(Err("PI_COMMAND_WRITE_UNCONFIRMED".into()));
                            }
                        }
                    }
                    break;
                }
            }
        });
        // Drain stderr continuously, without persisting provider errors/credential echoes.
        std::thread::spawn(move || {
            let mut reader = error;
            let mut buffer = [0u8; 8192];
            while reader.read(&mut buffer).is_ok_and(|n| n > 0) {}
        });
        let weak = Arc::downgrade(&inner);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let record = read_record(&mut reader);
                let Some(state) = weak.upgrade() else { break };
                match record {
                    Ok(Some(v)) => {
                        if v["type"] == "response" {
                            if let Some(id) = v["id"].as_str() {
                                if let Ok(mut pending) = state.pending.lock() {
                                    if let Some(reply) = pending.remove(id) {
                                        let _ = reply.send(Ok(v));
                                    }
                                }
                            }
                        } else {
                            sink(Event::Record(v));
                        }
                    }
                    _ => {
                        state.closed.store(true, Ordering::SeqCst);
                        if let Ok(mut pending) = state.pending.lock() {
                            for (_, reply) in pending.drain() {
                                let _ = reply.send(Err("PI_PROCESS_RESPONSE_UNCONFIRMED".into()));
                            }
                        }
                        sink(Event::Exited);
                        break;
                    }
                }
            }
        });
        Ok(Self { inner })
    }
    pub fn request(
        &self,
        id: &str,
        mut command: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err("PI_PROCESS_CLOSED".into());
        }
        let object = command.as_object_mut().ok_or("PI_COMMAND_INVALID")?;
        object.insert("id".into(), id.into());
        let (tx, rx) = mpsc::sync_channel(1);
        {
            let mut pending = self
                .inner
                .pending
                .lock()
                .map_err(|_| "PI_TRANSPORT_UNAVAILABLE")?;
            if pending.contains_key(id) {
                return Err("PI_COMMAND_ID_IN_FLIGHT".into());
            }
            pending.insert(id.into(), tx);
        }
        let mut bytes = serde_json::to_vec(&command).map_err(|_| "PI_COMMAND_INVALID")?;
        if bytes.len() > MAX_FRAME {
            self.remove(id);
            return Err("PI_COMMAND_TOO_LARGE".into());
        }
        bytes.push(b'\n');
        // Listeners/pending identity exist before the first physical byte. No transport
        // retry. Callers journal UNKNOWN on any uncertain write/response outcome.
        if self.inner.outbound.try_send(bytes).is_err() {
            self.remove(id);
            return Err("PI_COMMAND_NOT_QUEUED".into());
        }
        let result = rx
            .recv_timeout(timeout)
            .map_err(|_| "PI_COMMAND_RESPONSE_UNCONFIRMED".to_string());
        self.remove(id);
        result?
    }
    fn remove(&self, id: &str) {
        if let Ok(mut pending) = self.inner.pending.lock() {
            pending.remove(id);
        }
    }
    /// The native UI subprotocol has no normal response/ACK. Callers must
    /// reconcile the guard's independent appendEntry receipt, never retry.
    pub fn owner_ui_response(&self, id: &str, confirmed: bool) -> Result<(), String> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err("PI_PROCESS_CLOSED".into());
        }
        let mut bytes = serde_json::to_vec(
            &serde_json::json!({"type":"extension_ui_response","id":id,"confirmed":confirmed}),
        )
        .map_err(|_| "PI_UI_RESPONSE_INVALID")?;
        bytes.push(b'\n');
        self.inner
            .outbound
            .try_send(bytes)
            .map_err(|_| "PI_UI_RESPONSE_NOT_QUEUED".into())
    }
    pub fn close(&self, timeout: Duration) -> Result<(), String> {
        self.inner.closed.store(true, Ordering::SeqCst);
        if self.inner.outbound.try_send(Vec::new()).is_err() {
            return Err("PI_CLOSE_UNCONFIRMED".into());
        }
        let until = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < until {
            if self.inner.process.lock().map_err(|_|"PI_PROCESS_UNAVAILABLE")?.exited()?
            {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err("PI_CLOSE_UNCONFIRMED".into())
    }
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::SeqCst)
    }
    pub fn process_identity(&self)->Result<super::elevation::Identity,String>{Ok(self.inner.process.lock().map_err(|_|"PI_PROCESS_UNAVAILABLE")?.identity())}
}
impl Drop for Rpc {
    fn drop(&mut self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        if let Ok(mut process) = self.inner.process.lock() {
            process.stop();
        }
    }
}
pub(super) fn read_record(input: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut bytes = Vec::new();
    let n = input
        .take((MAX_FRAME + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "PI_FRAME_READ_FAILED")?;
    if n == 0 {
        return Ok(None);
    }
    if bytes.len() > MAX_FRAME || bytes.last() != Some(&b'\n') {
        return Err("PI_FRAME_TOO_LARGE_OR_TRUNCATED".into());
    }
    bytes.pop();
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    if bytes.is_empty() {
        return Err("PI_FRAME_INVALID".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "PI_FRAME_INVALID".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "explicit owned Node environment probe; dummy parent secrets only, no Pi auth/model"]
    fn native_child_does_not_inherit_management_credentials() {
        assert_eq!(
            std::env::var("AGBRIO_ENV_CANARY").as_deref(),
            Ok("DUMMY_STAGE2_CANARY")
        );
        let root =
            std::path::PathBuf::from(std::env::var_os("AGBRIO_PI_QA_ROOT").expect("owned QA root"));
        assert!(root.is_dir());
        let script="require('node:readline').createInterface({input:process.stdin}).on('line',line=>{const p=JSON.parse(line);process.stdout.write(JSON.stringify({type:'response',id:p.id,command:p.type,success:true,data:{openai:Object.hasOwn(process.env,'OPENAI_API_KEY'),anthropic:Object.hasOwn(process.env,'ANTHROPIC_API_KEY'),unrelated:Object.hasOwn(process.env,'AGBRIO_ENV_CANARY'),profile:Object.hasOwn(process.env,'USERPROFILE')}})+'\\n')})";
        let rpc = Rpc::start(
            &super::super::installation().unwrap().node,
            &["-e".into(), script.into()],
            &root,
            root.to_str().unwrap(),
            false,
            Arc::new(|_| {}),
        )
        .unwrap();
        let response = rpc
            .request(
                "owned-env-probe",
                serde_json::json!({"type":"probe_env"}),
                Duration::from_secs(10),
            )
            .unwrap();
        assert_eq!(
            response["data"],
            serde_json::json!({"openai":false,"anthropic":false,"unrelated":false,"profile":true})
        );
        rpc.close(Duration::from_secs(5)).unwrap();
    }
    #[test]
    fn frames_only_lf_not_unicode_separators() {
        let mut r =
            std::io::Cursor::new("{\"text\":\"a\u{2028}b\u{2029}c\"}\r\n{\"text\":\"next\"}\n");
        assert_eq!(
            read_record(&mut r).unwrap().unwrap()["text"],
            "a\u{2028}b\u{2029}c"
        );
        assert_eq!(read_record(&mut r).unwrap().unwrap()["text"], "next");
        assert!(read_record(&mut r).unwrap().is_none());
    }
    #[test]
    fn rejects_partial_or_invalid_frames() {
        assert!(read_record(&mut std::io::Cursor::new("{}")).is_err());
        assert!(read_record(&mut std::io::Cursor::new(vec![255, b'\n'])).is_err());
    }
}
