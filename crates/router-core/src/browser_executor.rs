//! Narrow resident Browser Executor process boundary. It owns no Store, binding,
//! approval or retries; callers provide already-approved immutable operations.
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{mpsc, Mutex},
    thread,
    time::{Duration, Instant},
};

struct Process {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Result<Value, String>>,
    sequence: u64,
    failed: bool,
}
pub struct ExecutorProcess(Mutex<Process>);
impl ExecutorProcess {
    pub fn start(
        node: &Path,
        script: &Path,
        resources: &Path,
        data: &Path,
    ) -> Result<Self, String> {
        if ![node, script, resources, data]
            .iter()
            .all(|path| path.is_absolute())
        {
            return Err("EXECUTOR_ABSOLUTE_PATHS_REQUIRED".into());
        }
        // Tauri/Windows canonical paths may carry a verbatim prefix. Node's
        // entry-module resolver requires the existing DOS/UNC normalization.
        let mut command = Command::new(crate::chatgpt::node_path(node)?);
        command
            .arg(crate::chatgpt::node_path(script)?)
            .arg(crate::chatgpt::node_path(resources)?)
            .arg(crate::chatgpt::node_path(data)?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|_| "EXECUTOR_START_FAILED")?;
        let input = child.stdin.take().ok_or("EXECUTOR_STDIN_REQUIRED")?;
        let stdout = child.stdout.take().ok_or("EXECUTOR_STDOUT_REQUIRED")?;
        let (sender, output) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut frame = Vec::new();
                let result = loop {
                    match reader.fill_buf() {
                        Ok([]) if frame.is_empty() => break Err("EXECUTOR_CLOSED".into()),
                        Ok([]) => break Err("EXECUTOR_FRAME_TRUNCATED".into()),
                        Ok(bytes) => {
                            let count = bytes
                                .iter()
                                .position(|byte| *byte == b'\n')
                                .map(|i| i + 1)
                                .unwrap_or(bytes.len());
                            if frame.len() + count > 1_000_000 {
                                break Err("EXECUTOR_FRAME_TOO_LARGE".into());
                            }
                            let complete = bytes[count - 1] == b'\n';
                            frame.extend_from_slice(&bytes[..count]);
                            reader.consume(count);
                            if complete {
                                break serde_json::from_slice(&frame)
                                    .map_err(|_| "EXECUTOR_FRAME_INVALID".into());
                            }
                        }
                        Err(_) => break Err("EXECUTOR_READ_FAILED".into()),
                    }
                };
                let terminal = result.is_err();
                if sender.send(result).is_err() || terminal {
                    break;
                }
            }
        });
        Ok(Self(Mutex::new(Process {
            child,
            input,
            output,
            sequence: 0,
            failed: false,
        })))
    }

    pub fn request(&self, operation: &str, fields: Value) -> Result<Value, String> {
        if !matches!(
            operation,
            "health"
                | "verify"
                | "binding"
                | "list_attachments"
                | "materialize_attachment"
                | "open"
                | "observe"
                | "send"
                | "receipt"
                | "owner_authentication_completed"
                | "quit"
        ) {
            return Err("EXECUTOR_OPERATION_INVALID".into());
        }
        let mut process = self.0.lock().map_err(|_| "EXECUTOR_SESSION_UNAVAILABLE")?;
        if process.failed {
            return Err("EXECUTOR_UNAVAILABLE_NO_AUTOMATIC_RESTART".into());
        }
        process.sequence = process
            .sequence
            .checked_add(1)
            .ok_or("EXECUTOR_SEQUENCE_EXHAUSTED")?;
        let id = process.sequence;
        let mut packet = fields
            .as_object()
            .cloned()
            .ok_or("EXECUTOR_FIELDS_INVALID")?;
        packet.insert("id".into(), json!(id));
        packet.insert("operation".into(), json!(operation));
        let mut bytes = serde_json::to_vec(&packet).map_err(|_| "EXECUTOR_FIELDS_INVALID")?;
        if bytes.len() > 999_999 {
            return Err("EXECUTOR_REQUEST_TOO_LARGE".into());
        }
        bytes.push(b'\n');
        let result = (|| {
            process
                .input
                .write_all(&bytes)
                .map_err(|_| "EXECUTOR_WRITE_FAILED")?;
            process.input.flush().map_err(|_| "EXECUTOR_WRITE_FAILED")?;
            let reply = process
                .output
                .recv_timeout(Duration::from_secs(60))
                .map_err(|_| "EXECUTOR_REPLY_UNOBSERVED")??;
            if reply.get("id").and_then(Value::as_u64) != Some(id) {
                return Err("EXECUTOR_REPLY_IDENTITY_MISMATCH".into());
            }
            reply
                .get("result")
                .cloned()
                .ok_or_else(|| "EXECUTOR_REPLY_INVALID".into())
        })();
        if result.is_err() {
            process.failed = true;
        }
        result
    }

    pub fn shutdown(&self) {
        let _ = self.request("quit", json!({}));
        if let Ok(mut process) = self.0.lock() {
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if !matches!(process.child.try_wait(), Ok(None)) {
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            }
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
    }
}

impl Drop for ExecutorProcess {
    fn drop(&mut self) {
        self.shutdown();
    }
}
