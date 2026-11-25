//! One bounded, native stdio connection. No shell, credentials or write retry.
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};
type Pending = Arc<Mutex<HashMap<u64, mpsc::SyncSender<Result<Value, String>>>>>;
pub(super) struct Rpc {
    input: Mutex<ChildStdin>,
    pending: Pending,
    next: AtomicU64,
    closed: Arc<AtomicBool>,
    child: Mutex<Child>,
    admitted: AtomicBool,
    listener: Arc<dyn Fn(Value) + Send + Sync>,
}
impl Rpc {
    pub fn start(
        mut command: Command,
        listener: Arc<dyn Fn(Value) + Send + Sync>,
    ) -> Result<Arc<Self>, String> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|_| "ANTIGRAVITY_CLI_UNAVAILABLE")?;
        let input = child.stdin.take().ok_or("ANTIGRAVITY_CLI_UNAVAILABLE")?;
        let output = child.stdout.take().ok_or("ANTIGRAVITY_CLI_UNAVAILABLE")?;
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let p = pending.clone();
        let c = closed.clone();
        let callback = listener.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut line = Vec::new();
                match reader
                    .by_ref()
                    .take(32 * 1024 * 1024 + 1)
                    .read_until(b'\n', &mut line)
                {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line.len() > 32 * 1024 * 1024 => break,
                    _ => {}
                }
                let Ok(value) = serde_json::from_slice::<Value>(&line) else {
                    break;
                };
                if value["jsonrpc"] != "2.0" {
                    break;
                }
                if value.get("method").is_some() {
                    listener(value);
                    continue;
                }
                let Some(id) = value["id"].as_u64() else {
                    continue;
                };
                let tx = p.lock().ok().and_then(|mut p| p.remove(&id));
                if let Some(tx) = tx {
                    let result = if value.get("error").is_some() {
                        Err(format!(
                            "ANTIGRAVITY_NATIVE_RPC_ERROR:{}",
                            value["error"]["code"].as_i64().unwrap_or(-32603)
                        ))
                    } else {
                        value
                            .get("result")
                            .cloned()
                            .ok_or_else(|| "ANTIGRAVITY_NATIVE_RESPONSE_INVALID".into())
                    };
                    let _ = tx.send(result);
                }
            }
            c.store(true, Ordering::SeqCst);
            if let Ok(mut p) = p.lock() {
                for (_, tx) in p.drain() {
                    let _ = tx.send(Err("ANTIGRAVITY_NATIVE_DISCONNECTED_UNCONFIRMED".into()));
                }
            }
            listener(json!({"method":"agbrio/disconnected"}));
        });
        Ok(Arc::new(Self {
            input: Mutex::new(input),
            pending,
            next: AtomicU64::new(1),
            closed,
            child: Mutex::new(child),
            admitted: AtomicBool::new(false),
            listener: callback,
        }))
    }
    fn write(&self, v: &Value) -> Result<(), String> {
        if self.closed() {
            return Err("ANTIGRAVITY_NATIVE_DISCONNECTED_UNCONFIRMED".into());
        }
        let mut raw = serde_json::to_vec(v).map_err(|_| "ANTIGRAVITY_NATIVE_INPUT_INVALID")?;
        if raw.len() > 64 * 1024 * 1024 {
            return Err("ANTIGRAVITY_NATIVE_INPUT_TOO_LARGE".into());
        }
        raw.push(b'\n');
        let result = self
            .input
            .lock()
            .map_err(|_| "ANTIGRAVITY_NATIVE_DISCONNECTED_UNCONFIRMED")?
            .write_all(&raw);
        if result.is_err() {
            self.closed.store(true, Ordering::SeqCst);
            if let Ok(mut pending) = self.pending.lock() {
                for (_, tx) in pending.drain() {
                    let _ = tx.send(Err("ANTIGRAVITY_NATIVE_WRITE_UNCONFIRMED".into()));
                }
            }
            (self.listener)(json!({"method":"agbrio/disconnected"}));
            return Err("ANTIGRAVITY_NATIVE_WRITE_UNCONFIRMED".into());
        }
        Ok(())
    }
    pub fn begin(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(u64, mpsc::Receiver<Result<Value, String>>), String> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::sync_channel(1);
        self.pending
            .lock()
            .map_err(|_| "ANTIGRAVITY_NATIVE_UNAVAILABLE")?
            .insert(id, tx);
        if method == "session/prompt" {
            self.admitted.store(true, Ordering::SeqCst);
        }
        if let Err(code) =
            self.write(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
        {
            if let Ok(mut p) = self.pending.lock() {
                p.remove(&id);
            }
            return Err(code);
        }
        Ok((id, rx))
    }
    pub fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let (id, rx) = self.begin(method, params)?;
        let result = rx
            .recv_timeout(Duration::from_secs(20))
            .map_err(|_| "ANTIGRAVITY_NATIVE_READ_UNCONFIRMED".to_string())
            .and_then(|v| v);
        if let Ok(mut p) = self.pending.lock() {
            p.remove(&id);
        }
        result
    }
    pub fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.write(&json!({"jsonrpc":"2.0","method":method,"params":params}))
    }
    pub fn answer(&self, id: &Value, result: Value) -> Result<(), String> {
        self.write(&json!({"jsonrpc":"2.0","id":id,"result":result}))
    }
    pub fn decline(&self, id: &Value) -> Result<(), String> {
        self.write(&json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Client capability not available"}}))
    }
    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}
impl Drop for Rpc {
    fn drop(&mut self) {
        if !self.admitted.load(Ordering::SeqCst) {
            if let Ok(child) = self.child.get_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}
